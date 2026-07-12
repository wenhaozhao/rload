use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use serde::Deserialize;

use crate::access_log::ReplayRequest;
use crate::{MAX_REQUEST_BODY_BYTES, Method, RunError};

const MAX_LINE_BYTES: u64 = 1024 * 1024;
const MAX_URI_BYTES: usize = 8 * 1024;
const MAX_HEADER_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonRequest {
    method: String,
    uri: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    body: Option<String>,
}

pub(crate) fn read(path: &Path) -> Result<Vec<ReplayRequest>, RunError> {
    let file = File::open(path)?;
    read_from(BufReader::new(file))
}

fn read_from(mut reader: impl BufRead) -> Result<Vec<ReplayRequest>, RunError> {
    let mut requests = Vec::new();
    let mut line_number = 0;
    loop {
        let mut bytes = Vec::new();
        let read = reader
            .by_ref()
            .take(MAX_LINE_BYTES + 1)
            .read_until(b'\n', &mut bytes)?;
        if read == 0 {
            break;
        }
        line_number += 1;
        if bytes.len() as u64 > MAX_LINE_BYTES {
            return Err(invalid(line_number, "record exceeds 1 MiB"));
        }
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
        }
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        let line = std::str::from_utf8(&bytes)
            .map_err(|_| invalid(line_number, "record is not valid UTF-8"))?;
        if line.trim().is_empty() {
            continue;
        }
        let request: JsonRequest = serde_json::from_str(line)
            .map_err(|error| invalid(line_number, &format!("invalid JSON: {error}")))?;
        requests.push(validate(request, line_number)?);
    }
    if requests.is_empty() {
        return Err(RunError::InvalidRequestFile(
            "request file contains no requests".into(),
        ));
    }
    Ok(requests)
}

fn validate(request: JsonRequest, line: usize) -> Result<ReplayRequest, RunError> {
    let method = match request.method.as_str() {
        "GET" => Method::Get,
        "HEAD" => Method::Head,
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "PATCH" => Method::Patch,
        "DELETE" => Method::Delete,
        "OPTIONS" => Method::Options,
        method => return Err(invalid(line, &format!("unsupported method {method}"))),
    };
    let replay = ReplayRequest {
        method,
        path: request.uri,
        headers: request.headers.into_iter().collect(),
        body_present: request.body.is_some(),
        body: request.body.unwrap_or_default().into_bytes(),
        timestamp_micros: None,
    };
    validate_request(&replay).map_err(|message| invalid(line, &message))?;
    Ok(replay)
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
            method: "POST".into(),
            uri: "/".into(),
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
            method: "POST".into(),
            uri: "/valid".into(),
            headers: BTreeMap::from([("Transfer-Encoding".into(), "chunked".into())]),
            body: Some("payload".into()),
        };

        let error = validate(request, 2).unwrap_err();

        assert!(error.to_string().contains("managed by rload"));
        let invalid_uri = JsonRequest {
            method: "GET".into(),
            uri: "/bad%escape".into(),
            headers: BTreeMap::new(),
            body: None,
        };
        let error = validate(invalid_uri, 3).unwrap_err();
        assert!(error.to_string().contains("URI must use origin form"));
    }

    #[test]
    fn rejects_oversized_record_before_json_parsing() {
        let input = vec![b'x'; MAX_LINE_BYTES as usize + 1];

        let error = read_from(std::io::Cursor::new(input)).unwrap_err();

        assert!(error.to_string().contains("line 1"));
        assert!(error.to_string().contains("exceeds 1 MiB"));
    }
}
