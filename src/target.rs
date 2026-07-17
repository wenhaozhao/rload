use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;

use rustls::pki_types::ServerName;

use crate::access_log::ReplayRequest;
use crate::{Method, RunError};

pub(crate) struct Target {
    authority: String,
    host_header: String,
    path: String,
    tls_server_name: Option<ServerName<'static>>,
}

impl Target {
    pub(crate) fn parse(url: &str) -> Result<Self, RunError> {
        let (remainder, default_port, tls) = if let Some(remainder) = url.strip_prefix("http://") {
            (remainder, 80, false)
        } else if let Some(remainder) = url.strip_prefix("https://") {
            (remainder, 443, true)
        } else {
            return Err(RunError::InvalidUrl(
                "only http:// and https:// are supported".into(),
            ));
        };
        let boundary = remainder
            .char_indices()
            .find(|(_, character)| matches!(character, '/' | '?' | '#'))
            .map_or(remainder.len(), |(index, _)| index);
        let authority = &remainder[..boundary];
        let suffix = &remainder[boundary..];
        let path = request_path(suffix);
        if authority.is_empty() {
            return Err(RunError::InvalidUrl("host is missing".into()));
        }
        let host = authority_host(authority)?;
        let socket_authority = if has_explicit_port(authority) {
            authority.to_owned()
        } else {
            format!("{authority}:{default_port}")
        };
        let tls_server_name = tls
            .then(|| {
                ServerName::try_from(host.to_owned())
                    .map_err(|_| RunError::InvalidUrl("invalid TLS server name".into()))
            })
            .transpose()?;
        Ok(Self {
            authority: socket_authority,
            host_header: authority.to_owned(),
            path,
            tls_server_name,
        })
    }

    pub(crate) fn tls_server_name(&self) -> Option<ServerName<'static>> {
        self.tls_server_name.clone()
    }

    pub(crate) fn resolve(&self) -> Result<Arc<[SocketAddr]>, RunError> {
        let addresses: Arc<[SocketAddr]> =
            self.authority.to_socket_addrs()?.collect::<Vec<_>>().into();
        if addresses.is_empty() {
            return Err(RunError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "URL resolved to no addresses",
            )));
        }
        Ok(addresses)
    }

    pub(crate) fn request(&self, method: Method) -> Arc<[u8]> {
        self.request_for(method, &self.path)
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn request_for(&self, method: Method, path: &str) -> Arc<[u8]> {
        format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: keep-alive\r\n\r\n",
            method.as_str(),
            path,
            self.host_header
        )
        .into_bytes()
        .into()
    }

    pub(crate) fn replay_request(&self, request: &ReplayRequest) -> Arc<[u8]> {
        let mut encoded = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\n",
            request.method.as_str(),
            request.path,
            self.host_header
        )
        .into_bytes();
        for (name, value) in request.headers() {
            encoded.extend_from_slice(name.as_bytes());
            encoded.extend_from_slice(b": ");
            encoded.extend_from_slice(value.as_bytes());
            encoded.extend_from_slice(b"\r\n");
        }
        if !matches!(request.method, Method::Get | Method::Head) {
            encoded.extend_from_slice(
                format!("Content-Length: {}\r\n", request.body().len()).as_bytes(),
            );
        }
        encoded.extend_from_slice(b"Connection: keep-alive\r\n\r\n");
        encoded.extend_from_slice(request.body());
        encoded.into()
    }
}

fn authority_host(authority: &str) -> Result<&str, RunError> {
    if authority.contains('@') {
        return Err(RunError::InvalidUrl("userinfo is not supported".into()));
    }
    if let Some(bracketed) = authority.strip_prefix('[') {
        let (host, suffix) = bracketed
            .split_once(']')
            .filter(|(host, _)| !host.is_empty())
            .ok_or_else(|| RunError::InvalidUrl("invalid IPv6 host".into()))?;
        if !suffix.is_empty()
            && suffix
                .strip_prefix(':')
                .is_none_or(|port| port.parse::<u16>().is_err())
        {
            return Err(RunError::InvalidUrl("invalid IPv6 authority".into()));
        }
        return Ok(host);
    }
    if let Some((host, port)) = authority.rsplit_once(':') {
        if host.is_empty() || port.parse::<u16>().is_err() {
            return Err(RunError::InvalidUrl("invalid port".into()));
        }
        return Ok(host);
    }
    Ok(authority)
}

fn request_path(suffix: &str) -> String {
    let without_fragment = suffix.split_once('#').map_or(suffix, |(path, _)| path);
    match without_fragment.chars().next() {
        Some('/') => without_fragment.to_owned(),
        Some('?') => format!("/{without_fragment}"),
        _ => "/".to_owned(),
    }
}

fn has_explicit_port(authority: &str) -> bool {
    if authority.starts_with('[') {
        return authority.contains("]:");
    }
    authority
        .rsplit_once(':')
        .is_some_and(|(_, port)| port.parse::<u16>().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_default_port_and_server_name() {
        let target = Target::parse("https://example.com/health").unwrap();

        assert_eq!(target.authority, "example.com:443");
        assert_eq!(target.host_header, "example.com");
        assert_eq!(target.path, "/health");
        assert_eq!(
            target.tls_server_name,
            Some(ServerName::try_from("example.com").unwrap())
        );
    }

    #[test]
    fn parses_query_without_path_and_removes_fragment() {
        let target = Target::parse("https://example.com?key=value#ignored").unwrap();

        assert_eq!(target.authority, "example.com:443");
        assert_eq!(target.path, "/?key=value");
    }

    #[test]
    fn rejects_invalid_bracketed_ipv6_suffix() {
        assert!(Target::parse("https://[::1]junk/").is_err());
        assert!(Target::parse("https://[::1]:443junk/").is_err());
    }
}
