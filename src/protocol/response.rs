use crate::RunError;

pub(crate) struct ParsedResponse {
    pub(crate) status: u16,
    pub(crate) body_length: usize,
    pub(crate) connection_close: bool,
}

const MAX_HEADER_BYTES: usize = 64 * 1024;

pub(crate) struct ResponseDecoder {
    header: Vec<u8>,
    head: Option<ResponseHead>,
    body: BodyDecoder,
    no_body: bool,
}

impl ResponseDecoder {
    pub(crate) fn new(no_body: bool) -> Self {
        Self {
            header: Vec::with_capacity(1024),
            head: None,
            body: BodyDecoder::Waiting,
            no_body,
        }
    }

    pub(crate) fn feed(
        &mut self,
        data: &[u8],
        eof: bool,
    ) -> Result<Option<ParsedResponse>, RunError> {
        if self.head.is_some() {
            return self.consume_body(data, eof);
        }
        self.header.extend_from_slice(data);
        let Some(header_end) = find_bytes(&self.header, b"\r\n\r\n") else {
            if self.header.len() > MAX_HEADER_BYTES {
                return Err(RunError::InvalidResponse(
                    "response headers exceed 64 KiB".into(),
                ));
            }
            return if eof {
                Err(RunError::InvalidResponse("headers are incomplete".into()))
            } else {
                Ok(None)
            };
        };
        if header_end > MAX_HEADER_BYTES {
            return Err(RunError::InvalidResponse(
                "response headers exceed 64 KiB".into(),
            ));
        }
        let body = self.header.split_off(header_end + 4);
        self.header.truncate(header_end);
        let head = parse_response_head(&self.header)?;
        self.body = if self.no_body {
            BodyDecoder::ContentLength {
                remaining: 0,
                total: 0,
            }
        } else {
            match head.framing {
                BodyFraming::ContentLength(length) => BodyDecoder::ContentLength {
                    remaining: length,
                    total: 0,
                },
                BodyFraming::Chunked => BodyDecoder::Chunked(ChunkedDecoder::new()),
                BodyFraming::CloseDelimited => BodyDecoder::CloseDelimited { total: 0 },
            }
        };
        self.head = Some(head);
        self.consume_body(&body, eof)
    }

    fn consume_body(&mut self, data: &[u8], eof: bool) -> Result<Option<ParsedResponse>, RunError> {
        let Some(body_length) = self.body.consume(data, eof)? else {
            return Ok(None);
        };
        let head = self.head.as_ref().expect("response head is parsed");
        Ok(Some(ParsedResponse {
            status: head.status,
            body_length,
            connection_close: head.connection_close,
        }))
    }
}

struct ResponseHead {
    status: u16,
    connection_close: bool,
    framing: BodyFraming,
}

enum BodyFraming {
    ContentLength(usize),
    Chunked,
    CloseDelimited,
}

enum BodyDecoder {
    Waiting,
    ContentLength { remaining: usize, total: usize },
    Chunked(ChunkedDecoder),
    CloseDelimited { total: usize },
}

impl BodyDecoder {
    fn consume(&mut self, data: &[u8], eof: bool) -> Result<Option<usize>, RunError> {
        match self {
            Self::Waiting => unreachable!("body decoder is configured after headers"),
            Self::ContentLength { remaining, total } => {
                let consumed = data.len().min(*remaining);
                *remaining -= consumed;
                *total += consumed;
                if *remaining == 0 {
                    Ok(Some(*total))
                } else if eof {
                    Err(RunError::InvalidResponse(
                        "response body is incomplete".into(),
                    ))
                } else {
                    Ok(None)
                }
            }
            Self::Chunked(decoder) => decoder.feed(data, eof),
            Self::CloseDelimited { total } => {
                *total += data.len();
                Ok(eof.then_some(*total))
            }
        }
    }
}

fn parse_response_head(header: &[u8]) -> Result<ResponseHead, RunError> {
    let headers = std::str::from_utf8(header)
        .map_err(|_| RunError::InvalidResponse("headers are not valid ASCII".into()))?;
    let mut lines = headers.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| RunError::InvalidResponse("status line is invalid".into()))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| RunError::InvalidResponse("status line is invalid".into()))?;
    let header_lines: Vec<_> = lines.collect();
    let connection_close = headers_have_token(&header_lines, "connection", "close")
        || (status_line.starts_with("HTTP/1.0")
            && !headers_have_token(&header_lines, "connection", "keep-alive"));
    let framing = if headers_have_token(&header_lines, "transfer-encoding", "chunked") {
        BodyFraming::Chunked
    } else if let Some(length) =
        header_value(&header_lines, "content-length").and_then(|value| value.parse::<usize>().ok())
    {
        BodyFraming::ContentLength(length)
    } else {
        BodyFraming::CloseDelimited
    };
    Ok(ResponseHead {
        status,
        connection_close,
        framing,
    })
}

fn header_value<'a>(lines: &[&'a str], expected_name: &str) -> Option<&'a str> {
    lines.iter().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case(expected_name)
            .then(|| value.trim())
    })
}

fn headers_have_token(lines: &[&str], name: &str, expected_token: &str) -> bool {
    lines
        .iter()
        .filter_map(|line| line.split_once(':'))
        .filter(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .flat_map(|(_, value)| value.split(',').map(str::trim))
        .any(|token| token.eq_ignore_ascii_case(expected_token))
}

struct ChunkedDecoder {
    buffer: Vec<u8>,
    phase: ChunkPhase,
    total: usize,
}

enum ChunkPhase {
    Size,
    Data(usize),
    DataEnding,
    Trailers,
}

impl ChunkedDecoder {
    fn new() -> Self {
        Self {
            buffer: Vec::new(),
            phase: ChunkPhase::Size,
            total: 0,
        }
    }

    fn feed(&mut self, data: &[u8], eof: bool) -> Result<Option<usize>, RunError> {
        self.buffer.extend_from_slice(data);
        loop {
            match self.phase {
                ChunkPhase::Size => {
                    let Some(end) = find_bytes(&self.buffer, b"\r\n") else {
                        self.check_control_size()?;
                        break;
                    };
                    if end > MAX_HEADER_BYTES {
                        return Err(RunError::InvalidResponse(
                            "chunk control data exceed 64 KiB".into(),
                        ));
                    }
                    let size_line = std::str::from_utf8(&self.buffer[..end])
                        .map_err(|_| RunError::InvalidResponse("chunk size is invalid".into()))?;
                    let size = size_line
                        .split(';')
                        .next()
                        .and_then(|value| usize::from_str_radix(value, 16).ok())
                        .ok_or_else(|| RunError::InvalidResponse("chunk size is invalid".into()))?;
                    self.buffer.drain(..end + 2);
                    self.phase = if size == 0 {
                        ChunkPhase::Trailers
                    } else {
                        ChunkPhase::Data(size)
                    };
                }
                ChunkPhase::Data(remaining) => {
                    let consumed = self.buffer.len().min(remaining);
                    self.buffer.drain(..consumed);
                    self.total += consumed;
                    self.phase = if consumed == remaining {
                        ChunkPhase::DataEnding
                    } else {
                        ChunkPhase::Data(remaining - consumed)
                    };
                    if consumed == 0 {
                        break;
                    }
                }
                ChunkPhase::DataEnding => {
                    if self.buffer.len() < 2 {
                        break;
                    }
                    if &self.buffer[..2] != b"\r\n" {
                        return Err(RunError::InvalidResponse(
                            "chunk is not CRLF terminated".into(),
                        ));
                    }
                    self.buffer.drain(..2);
                    self.phase = ChunkPhase::Size;
                }
                ChunkPhase::Trailers => {
                    if self.buffer.starts_with(b"\r\n") {
                        return Ok(Some(self.total));
                    }
                    if let Some(end) = find_bytes(&self.buffer, b"\r\n\r\n") {
                        if end > MAX_HEADER_BYTES {
                            return Err(RunError::InvalidResponse(
                                "chunk control data exceed 64 KiB".into(),
                            ));
                        }
                        return Ok(Some(self.total));
                    }
                    self.check_control_size()?;
                    break;
                }
            }
        }
        if eof {
            Err(RunError::InvalidResponse(
                "chunked response body is incomplete".into(),
            ))
        } else {
            Ok(None)
        }
    }

    fn check_control_size(&self) -> Result<(), RunError> {
        if self.buffer.len() > MAX_HEADER_BYTES {
            Err(RunError::InvalidResponse(
                "chunk control data exceed 64 KiB".into(),
            ))
        } else {
            Ok(())
        }
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
