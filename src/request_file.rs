use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use chrono::DateTime;
use serde::Deserialize;
use serde_json::Value;

use crate::access_log::ReplayRequest;
use crate::{MAX_REQUEST_BODY_BYTES, Method, RunError};

const MAX_LINE_BYTES: u64 = 1024 * 1024;
const MAX_URI_BYTES: usize = 8 * 1024;
const MAX_HEADER_BYTES: usize = 64 * 1024;
const DEFAULT_TIMESTAMP_FORMAT: &str = "%d/%b/%Y:%H:%M:%S %z";
const RFC3339_TIMESTAMP_FORMAT: &str = "%+";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestSchema {
    schema_version: u64,
    fields: SchemaFields,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SchemaFields {
    method: Option<FieldMapping>,
    uri: Option<FieldMapping>,
    args: Option<FieldMapping>,
    headers: Option<FieldMapping>,
    body: Option<FieldMapping>,
    timestamp: Option<TimestampMapping>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldMapping {
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimestampMapping {
    path: String,
    #[serde(default)]
    format: Option<String>,
}

#[derive(Default)]
struct CompiledSchema {
    method: Option<Vec<String>>,
    uri: Option<Vec<String>>,
    args: Option<Vec<String>>,
    headers: Option<Vec<String>>,
    body: Option<Vec<String>>,
    timestamp: Option<CompiledTimestamp>,
}

struct CompiledTimestamp {
    path: Vec<String>,
    format: Option<String>,
}

#[derive(Deserialize)]
struct JsonRequest {
    #[serde(default)]
    method: Option<String>,
    uri: String,
    #[serde(default)]
    args: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    body: Option<String>,
}

pub(crate) fn read(
    path: &Path,
    schema_path: Option<&Path>,
    timestamps: bool,
) -> Result<Vec<ReplayRequest>, RunError> {
    let file = File::open(path)?;
    if let Some(schema_path) = schema_path {
        let schema = compile_schema(schema_path)?;
        read_records(&mut BufReader::new(file), &schema, timestamps)
    } else if !timestamps {
        read_from(BufReader::new(file))
    } else {
        read_records(
            &mut BufReader::new(file),
            &CompiledSchema::default(),
            timestamps,
        )
    }
}

fn read_from(mut reader: impl BufRead) -> Result<Vec<ReplayRequest>, RunError> {
    decode_records(&mut reader, |line, line_number| {
        let request: JsonRequest = serde_json::from_str(line)
            .map_err(|error| invalid(line_number, &format!("invalid JSON: {error}")))?;
        validate(request, line_number)
    })
}

fn read_records(
    reader: &mut impl BufRead,
    schema: &CompiledSchema,
    timestamps: bool,
) -> Result<Vec<ReplayRequest>, RunError> {
    let timestamps = timestamps || schema.timestamp.is_some();
    decode_records(reader, |line, line_number| {
        let value: Value = serde_json::from_str(line)
            .map_err(|error| invalid(line_number, &format!("invalid JSON: {error}")))?;
        validate_value(&value, schema, line_number, timestamps)
    })
}

fn decode_records(
    reader: &mut impl BufRead,
    mut decode: impl FnMut(&str, usize) -> Result<ReplayRequest, RunError>,
) -> Result<Vec<ReplayRequest>, RunError> {
    let mut requests = Vec::new();
    let mut line_number = 0;
    while let Some(line) = read_line(reader, &mut line_number)? {
        if line.trim().is_empty() {
            continue;
        }
        requests.push(decode(&line, line_number)?);
    }
    finish_requests(requests)
}

fn read_line(
    reader: &mut impl BufRead,
    line_number: &mut usize,
) -> Result<Option<String>, RunError> {
    let mut bytes = Vec::new();
    let read = reader
        .by_ref()
        .take(MAX_LINE_BYTES + 1)
        .read_until(b'\n', &mut bytes)?;
    if read == 0 {
        return Ok(None);
    }
    *line_number += 1;
    if bytes.len() as u64 > MAX_LINE_BYTES {
        return Err(invalid(*line_number, "record exceeds 1 MiB"));
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| invalid(*line_number, "record is not valid UTF-8"))
}

fn finish_requests(requests: Vec<ReplayRequest>) -> Result<Vec<ReplayRequest>, RunError> {
    if requests.is_empty() {
        return Err(RunError::InvalidRequestFile(
            "request file contains no requests".into(),
        ));
    }
    Ok(requests)
}

fn compile_schema(path: &Path) -> Result<CompiledSchema, RunError> {
    let text = std::fs::read_to_string(path)?;
    let schema: RequestSchema = serde_yaml::from_str(&text).map_err(|error| {
        RunError::InvalidRequestFile(format!("invalid request schema: {error}"))
    })?;
    if schema.schema_version != 1 {
        return Err(RunError::InvalidRequestFile(format!(
            "unsupported request schema version {}",
            schema.schema_version
        )));
    }
    Ok(CompiledSchema {
        method: compile_mapping(schema.fields.method, "method")?,
        uri: compile_mapping(schema.fields.uri, "uri")?,
        args: compile_mapping(schema.fields.args, "args")?,
        headers: compile_mapping(schema.fields.headers, "headers")?,
        body: compile_mapping(schema.fields.body, "body")?,
        timestamp: schema
            .fields
            .timestamp
            .map(|mapping| {
                Ok::<CompiledTimestamp, RunError>(CompiledTimestamp {
                    path: compile_path(&mapping.path, "timestamp")?,
                    format: mapping.format,
                })
            })
            .transpose()?,
    })
}

fn compile_mapping(
    mapping: Option<FieldMapping>,
    field: &str,
) -> Result<Option<Vec<String>>, RunError> {
    mapping
        .map(|mapping| compile_path(&mapping.path, field))
        .transpose()
}

fn compile_path(path: &str, field: &str) -> Result<Vec<String>, RunError> {
    let segments: Vec<_> = path.split('.').map(str::to_owned).collect();
    if segments.is_empty() || segments.iter().any(String::is_empty) {
        return Err(RunError::InvalidRequestFile(format!(
            "request schema field {field} has an invalid path"
        )));
    }
    Ok(segments)
}

fn validate_value(
    value: &Value,
    schema: &CompiledSchema,
    line: usize,
    timestamps: bool,
) -> Result<ReplayRequest, RunError> {
    let method = optional_string(
        value_for(value, schema.method.as_deref(), "method"),
        line,
        "method",
    )?;
    let uri = required_string(value_for(value, schema.uri.as_deref(), "uri"), line, "uri")?;
    let args = optional_string(
        value_for(value, schema.args.as_deref(), "args"),
        line,
        "args",
    )?;
    let headers_value = value_for(value, schema.headers.as_deref(), "headers");
    let headers = match headers_value {
        None | Some(Value::Null) => BTreeMap::new(),
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| invalid(line, "headers must be an object of string values"))?,
    };
    let body = optional_string(
        value_for(value, schema.body.as_deref(), "body"),
        line,
        "body",
    )?;
    let timestamp_micros = timestamps
        .then(|| extract_timestamp(value, schema, line))
        .transpose()?
        .flatten();
    let request = JsonRequest {
        method,
        uri,
        args,
        headers,
        body,
    };
    let mut replay = validate(request, line)?;
    replay.timestamp_micros = timestamp_micros;
    Ok(replay)
}

fn value_for<'a>(value: &'a Value, path: Option<&[String]>, fallback: &str) -> Option<&'a Value> {
    path.map_or_else(|| value.get(fallback), |path| value_at(value, path))
}

fn value_at<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter()
        .try_fold(value, |current, segment| current.get(segment))
}

fn optional_string(
    value: Option<&Value>,
    line: usize,
    field: &str,
) -> Result<Option<String>, RunError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(invalid(line, &format!("{field} must be a string"))),
    }
}

fn required_string(value: Option<&Value>, line: usize, field: &str) -> Result<String, RunError> {
    optional_string(value, line, field)?
        .ok_or_else(|| invalid(line, &format!("{field} is required")))
}

fn extract_timestamp(
    value: &Value,
    schema: &CompiledSchema,
    line: usize,
) -> Result<Option<i64>, RunError> {
    if let Some(mapping) = &schema.timestamp {
        return parse_timestamp_value(
            value_at(value, &mapping.path),
            mapping.format.as_deref(),
            line,
        );
    }
    if let Some(value) = value.get("timestamp_micros") {
        return match value {
            Value::Null => Ok(None),
            Value::Number(value) => value
                .as_i64()
                .map(Some)
                .ok_or_else(|| invalid(line, "timestamp_micros must be a signed 64-bit integer")),
            _ => Err(invalid(
                line,
                "timestamp_micros must be a signed 64-bit integer",
            )),
        };
    }
    let time = value.get("time");
    let alternate = value.get("_time");
    if time.is_some() && alternate.is_some() && time != alternate {
        return Err(invalid(
            line,
            "time and _time contain conflicting timestamps",
        ));
    }
    parse_timestamp_value(time.or(alternate), None, line)
}

fn parse_timestamp_value(
    value: Option<&Value>,
    format: Option<&str>,
    line: usize,
) -> Result<Option<i64>, RunError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(value)) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| invalid(line, "timestamp_micros must be a signed 64-bit integer")),
        Some(Value::String(value)) => {
            let formats = format
                .map(|format| vec![format])
                .unwrap_or_else(|| vec![DEFAULT_TIMESTAMP_FORMAT, RFC3339_TIMESTAMP_FORMAT]);
            formats
                .iter()
                .find_map(|format| DateTime::parse_from_str(value, format).ok())
                .map(|timestamp| Some(timestamp.timestamp_micros()))
                .ok_or_else(|| invalid(line, "invalid timestamp: expected Nginx or RFC3339 format"))
        }
        Some(_) => Err(invalid(line, "timestamp must be an integer or string")),
    }
}

fn validate(request: JsonRequest, line: usize) -> Result<ReplayRequest, RunError> {
    let method_name = request.method.as_deref().unwrap_or("GET");
    let method = match method_name {
        "GET" => Method::Get,
        "HEAD" => Method::Head,
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "PATCH" => Method::Patch,
        "DELETE" => Method::Delete,
        "OPTIONS" => Method::Options,
        method => return Err(invalid(line, &format!("unsupported method {method}"))),
    };
    let path = append_query(request.uri, request.args);
    let replay = ReplayRequest {
        method,
        path,
        headers: request.headers.into_iter().collect(),
        body_present: request.body.is_some(),
        body: request.body.unwrap_or_default().into_bytes(),
        timestamp_micros: None,
    };
    validate_request(&replay).map_err(|message| invalid(line, &message))?;
    Ok(replay)
}

fn append_query(mut uri: String, args: Option<String>) -> String {
    if let Some(args) = args.filter(|args| !args.is_empty()) {
        if uri.contains('?') {
            if uri.ends_with('?') && args.starts_with('&') {
                uri.push_str(&args);
                return uri;
            }
            let args = args.strip_prefix(['?', '&']).unwrap_or(&args);
            if !uri.ends_with(['?', '&']) {
                uri.push('&');
            }
            uri.push_str(args);
        } else if args.starts_with('?') {
            uri.push_str(&args);
        } else {
            uri.push('?');
            uri.push_str(&args);
        }
    }
    uri
}

pub(crate) fn validate_request(request: &ReplayRequest) -> Result<(), String> {
    if request.path.len() > MAX_URI_BYTES || !valid_origin_form(&request.path) {
        return Err("URI must use origin form".into());
    }
    if request.body.len() > MAX_REQUEST_BODY_BYTES {
        return Err("body exceeds 512 KiB".into());
    }
    if matches!(request.method, Method::Get | Method::Head) && request.body_present {
        return Err("GET and HEAD requests must not contain a body".into());
    }
    let mut header_bytes = 0;
    for (name, value) in &request.headers {
        if !valid_header_name(name) {
            return Err(format!("invalid header name {name:?}"));
        }
        if [
            "host",
            "connection",
            "content-length",
            "transfer-encoding",
            "trailer",
            "expect",
        ]
        .iter()
        .any(|reserved| name.eq_ignore_ascii_case(reserved))
        {
            return Err(format!("header {name} is managed by rload"));
        }
        if !value
            .bytes()
            .all(|byte| byte == b'\t' || (b' '..=b'~').contains(&byte))
        {
            return Err(format!("invalid value for header {name}"));
        }
        header_bytes += name.len() + value.len() + 4;
    }
    if header_bytes > MAX_HEADER_BYTES {
        return Err("headers exceed 64 KiB".into());
    }
    Ok(())
}

fn valid_origin_form(uri: &str) -> bool {
    let bytes = uri.as_bytes();
    if bytes.first() != Some(&b'/') || uri.contains('#') {
        return false;
    }
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
            continue;
        }
        if !(byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'/' | b'?'
                    | b'-'
                    | b'.'
                    | b'_'
                    | b'~'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
                    | b':'
                    | b'@'
            ))
        {
            return false;
        }
        index += 1;
    }
    true
}

fn valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

fn invalid(line: usize, message: &str) -> RunError {
    RunError::InvalidRequestFile(format!("line {line}: {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_managed_headers_with_line_number() {
        let request = JsonRequest {
            method: Some("POST".into()),
            uri: "/".into(),
            args: None,
            headers: BTreeMap::from([("Content-Length".into(), "9".into())]),
            body: Some("payload".into()),
        };

        let error = validate(request, 4).unwrap_err();

        assert!(error.to_string().contains("line 4"));
        assert!(error.to_string().contains("managed by rload"));
    }

    #[test]
    fn rejects_transfer_encoding_and_invalid_percent_escape() {
        let request = JsonRequest {
            method: Some("POST".into()),
            uri: "/valid".into(),
            args: None,
            headers: BTreeMap::from([("Transfer-Encoding".into(), "chunked".into())]),
            body: Some("payload".into()),
        };

        let error = validate(request, 2).unwrap_err();

        assert!(error.to_string().contains("managed by rload"));
        let invalid_uri = JsonRequest {
            method: Some("GET".into()),
            uri: "/bad%escape".into(),
            args: None,
            headers: BTreeMap::new(),
            body: None,
        };
        let error = validate(invalid_uri, 3).unwrap_err();
        assert!(error.to_string().contains("URI must use origin form"));
    }

    #[test]
    fn defaults_method_and_appends_args_while_ignoring_unknown_fields() {
        let input = br#"{"method":null,"uri":"/items","args":"a=1&b=2","extra":true}
"#;
        let requests = read_from(std::io::Cursor::new(input)).unwrap();

        assert_eq!(requests[0].method, Method::Get);
        assert_eq!(requests[0].path, "/items?a=1&b=2");
    }

    #[test]
    fn ignores_business_time_fields_when_timestamp_pacing_is_disabled() {
        let input = br#"{"uri":"/items","time":{"business":true}}
"#;

        let requests = read_from(std::io::Cursor::new(input)).unwrap();

        assert_eq!(requests[0].timestamp_micros, None);
    }

    #[test]
    fn appends_args_with_ampersand_to_existing_query() {
        let request = JsonRequest {
            method: None,
            uri: "/items?existing=1".into(),
            args: Some("a=2".into()),
            headers: BTreeMap::new(),
            body: None,
        };

        assert_eq!(validate(request, 1).unwrap().path, "/items?existing=1&a=2");
    }

    #[test]
    fn accepts_exported_application_log_fixture() {
        let requests = read_from(std::io::Cursor::new(include_bytes!(
            "../tests/fixtures/exported-requests.jsonl"
        )))
        .unwrap();

        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].method, Method::Get);
        assert_eq!(requests[0].path, "/v1/items?uid=user-1&friendId=42");
        assert_eq!(requests[1], requests[0]);
    }

    #[test]
    fn preserves_or_normalizes_query_separators_by_uri_context() {
        assert_eq!(
            append_query("/items".into(), Some("a=1".into())),
            "/items?a=1"
        );
        assert_eq!(
            append_query("/items".into(), Some("?a=1".into())),
            "/items?a=1"
        );
        assert_eq!(
            append_query("/items".into(), Some("&a=1".into())),
            "/items?&a=1"
        );
        assert_eq!(
            append_query("/items?x=1".into(), Some("a=1".into())),
            "/items?x=1&a=1"
        );
        assert_eq!(
            append_query("/items?x=1".into(), Some("?a=1".into())),
            "/items?x=1&a=1"
        );
        assert_eq!(
            append_query("/items?x=1".into(), Some("&a=1".into())),
            "/items?x=1&a=1"
        );
        assert_eq!(
            append_query("/items?".into(), Some("?a=1".into())),
            "/items?a=1"
        );
        assert_eq!(
            append_query("/items?".into(), Some("&a=1".into())),
            "/items?&a=1"
        );
    }

    #[test]
    fn rejects_oversized_record_before_json_parsing() {
        let input = vec![b'x'; MAX_LINE_BYTES as usize + 1];

        let error = read_from(std::io::Cursor::new(input)).unwrap_err();

        assert!(error.to_string().contains("line 1"));
        assert!(error.to_string().contains("exceeds 1 MiB"));
    }

    #[test]
    fn materializes_default_and_schema_timestamp_fields_to_micros() {
        let input = br#"{"uri":"/one","timestamp_micros":1000000}
{"uri":"/two","time":"03/Jul/2026:08:41:17 +0000"}
{"uri":"/four","time":"2026-07-03T08:41:17Z"}
"#;
        let requests = read_records(
            &mut std::io::Cursor::new(input),
            &CompiledSchema::default(),
            true,
        )
        .unwrap();

        assert_eq!(requests[0].timestamp_micros, Some(1_000_000));
        let expected = requests[1].timestamp_micros;
        assert!(expected.unwrap() > 1_700_000_000_000_000);
        assert_eq!(requests[2].timestamp_micros, expected);

        let schema = CompiledSchema {
            timestamp: Some(CompiledTimestamp {
                path: vec!["event".into(), "at".into()],
                format: Some("%+".into()),
            }),
            ..CompiledSchema::default()
        };
        let requests = read_records(
            &mut std::io::Cursor::new(
                br#"{"uri":"/three","event":{"at":"2026-07-03T08:41:17Z"}}
"#,
            ),
            &schema,
            false,
        )
        .unwrap();

        assert_eq!(requests[0].timestamp_micros, expected);
    }
}
