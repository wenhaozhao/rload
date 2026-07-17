use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::{Method, RunError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReplayRequest {
    pub(crate) method: Method,
    pub(crate) path: String,
    payload: Option<Box<ReplayPayload>>,
    pub(crate) timestamp_micros: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReplayPayload {
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    body_present: bool,
}

impl ReplayRequest {
    pub(crate) fn new(
        method: Method,
        path: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        body_present: bool,
        timestamp_micros: Option<i64>,
    ) -> Self {
        let payload = (!headers.is_empty() || !body.is_empty() || body_present).then(|| {
            Box::new(ReplayPayload {
                headers,
                body,
                body_present,
            })
        });
        Self {
            method,
            path,
            payload,
            timestamp_micros,
        }
    }

    pub(crate) fn headers(&self) -> &[(String, String)] {
        self.payload
            .as_ref()
            .map_or(&[], |payload| payload.headers.as_slice())
    }

    pub(crate) fn body(&self) -> &[u8] {
        self.payload
            .as_ref()
            .map_or(&[], |payload| payload.body.as_slice())
    }

    pub(crate) fn body_present(&self) -> bool {
        self.payload
            .as_ref()
            .is_some_and(|payload| payload.body_present)
    }
}

pub(crate) struct AccessLogReplay {
    pub(crate) requests: Vec<ReplayRequest>,
    pub(crate) skipped_methods: BTreeMap<String, u64>,
}

pub(crate) fn read(path: &Path) -> Result<AccessLogReplay, RunError> {
    let file = File::open(path)?;
    let mut requests = Vec::new();
    let mut skipped_methods = BTreeMap::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match parse_line(&line, index + 1)? {
            ParsedLine::Request(request) => requests.push(request),
            ParsedLine::SkippedMethod(method) => {
                *skipped_methods.entry(method).or_default() += 1;
            }
        }
    }
    if requests.is_empty() {
        return Err(RunError::InvalidAccessLog(
            "access log contains no replayable requests".into(),
        ));
    }
    Ok(AccessLogReplay {
        requests,
        skipped_methods,
    })
}

enum ParsedLine {
    Request(ReplayRequest),
    SkippedMethod(String),
}

fn parse_line(line: &str, line_number: usize) -> Result<ParsedLine, RunError> {
    let invalid =
        |message: &str| RunError::InvalidAccessLog(format!("line {line_number}: {message}"));
    let request = line
        .split_once('"')
        .and_then(|(_, remainder)| remainder.split_once('"').map(|(request, _)| request))
        .ok_or_else(|| invalid("quoted request field is missing"))?;
    let timestamp_micros = line
        .split_once('[')
        .and_then(|(_, remainder)| remainder.split_once(']').map(|(value, _)| value))
        .and_then(parse_timestamp);
    let mut fields = request.split_whitespace();
    let method = match fields.next() {
        Some("GET") => Method::Get,
        Some("HEAD") => Method::Head,
        Some(method) => return Ok(ParsedLine::SkippedMethod(method.to_owned())),
        None => return Err(invalid("request field is empty")),
    };
    let path = fields
        .next()
        .filter(|path| path.starts_with('/') && !path.contains(['\r', '\n']))
        .ok_or_else(|| invalid("request URI must use origin form"))?;
    fields
        .next()
        .filter(|version| matches!(*version, "HTTP/1.0" | "HTTP/1.1"))
        .ok_or_else(|| invalid("HTTP version must be HTTP/1.0 or HTTP/1.1"))?;
    if fields.next().is_some() {
        return Err(invalid("request field has unexpected fields"));
    }
    Ok(ParsedLine::Request(ReplayRequest::new(
        method,
        path.to_owned(),
        Vec::new(),
        Vec::new(),
        false,
        timestamp_micros,
    )))
}

fn parse_timestamp(value: &str) -> Option<i64> {
    let (date_time, offset) = value.rsplit_once(' ')?;
    let (date, time) = date_time.split_once(':')?;
    let mut date = date.split('/');
    let day = date.next()?.parse::<i64>().ok()?;
    let month = match date.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year = date.next()?.parse::<i64>().ok()?;
    if date.next().is_some() {
        return None;
    }
    let mut time = time.split(':');
    let hour = time.next()?.parse::<i64>().ok()?;
    let minute = time.next()?.parse::<i64>().ok()?;
    let seconds = time.next()?;
    if time.next().is_some() || day == 0 || day > 31 || hour > 23 || minute > 59 {
        return None;
    }
    let (second, fraction) = seconds
        .split_once('.')
        .map_or((seconds, ""), |(second, fraction)| (second, fraction));
    let second = second.parse::<i64>().ok()?;
    if second > 60 || fraction.len() > 6 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i64>().ok()? * 10_i64.pow(6 - fraction.len() as u32)
    };
    let sign = match offset.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    if offset.len() != 5 {
        return None;
    }
    let offset_hour = offset[1..3].parse::<i64>().ok()?;
    let offset_minute = offset[3..5].parse::<i64>().ok()?;
    if offset_hour > 23 || offset_minute > 59 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let local_seconds = days * 86_400 + hour * 3_600 + minute * 60 + second;
    Some((local_seconds - sign * (offset_hour * 3_600 + offset_minute * 60)) * 1_000_000 + fraction)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_unsupported_methods() {
        let parsed = parse_line("127.0.0.1 - - [date] \"POST /items HTTP/1.1\" 200 0", 7).unwrap();

        assert!(matches!(parsed, ParsedLine::SkippedMethod(method) if method == "POST"));
    }

    #[test]
    fn parses_nginx_local_time_with_fraction_and_offset() {
        let parsed = parse_line(
            "127.0.0.1 - - [10/Oct/2000:13:55:36.250 -0700] \"GET / HTTP/1.1\" 200 0",
            1,
        )
        .unwrap();

        let ParsedLine::Request(request) = parsed else {
            panic!("expected a request");
        };
        assert_eq!(request.timestamp_micros, Some(971_211_336_250_000));
    }
}
