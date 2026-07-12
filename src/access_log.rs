use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::{Method, RunError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReplayRequest {
    pub(crate) method: Method,
    pub(crate) path: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
    pub(crate) body_present: bool,
}

pub(crate) fn read(path: &Path) -> Result<Vec<ReplayRequest>, RunError> {
    let file = File::open(path)?;
    let mut requests = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        requests.push(parse_line(&line, index + 1)?);
    }
    if requests.is_empty() {
        return Err(RunError::InvalidAccessLog(
            "access log contains no requests".into(),
        ));
    }
    Ok(requests)
}

fn parse_line(line: &str, line_number: usize) -> Result<ReplayRequest, RunError> {
    let invalid =
        |message: &str| RunError::InvalidAccessLog(format!("line {line_number}: {message}"));
    let request = line
        .split_once('"')
        .and_then(|(_, remainder)| remainder.split_once('"').map(|(request, _)| request))
        .ok_or_else(|| invalid("quoted request field is missing"))?;
    let mut fields = request.split_whitespace();
    let method = match fields.next() {
        Some("GET") => Method::Get,
        Some("HEAD") => Method::Head,
        Some(method) => {
            return Err(invalid(&format!(
                "method {method} requires a request body or is unsupported"
            )));
        }
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
    Ok(ReplayRequest {
        method,
        path: path.to_owned(),
        headers: Vec::new(),
        body: Vec::new(),
        body_present: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_line_number_for_unsupported_method() {
        let error =
            parse_line("127.0.0.1 - - [date] \"POST /items HTTP/1.1\" 200 0", 7).unwrap_err();

        assert!(error.to_string().contains("line 7"));
        assert!(error.to_string().contains("method POST"));
    }
}
