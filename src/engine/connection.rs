use std::io::{Read, Write};
use std::net::SocketAddr;
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mio::net::TcpStream;
use mio::{Interest, Registry, Token};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned};

use super::ConnectionLimit;
use crate::Method;
use crate::RunError;
use crate::protocol::response::{ParsedResponse, ResponseDecoder};
use crate::request_sequence::RequestSequence;

pub(super) struct Connection {
    stream: Transport,
    addresses: Arc<[SocketAddr]>,
    address_index: usize,
    requests: Arc<RequestSequence>,
    method: Method,
    uri: Range<usize>,
    request: Arc<[u8]>,
    not_before: Option<Instant>,
    awaiting_pace: bool,
    reconnect_at_pace: bool,
    write_offset: usize,
    decoder: ResponseDecoder,
    started: Option<Instant>,
    connected_at: Instant,
    completed: u64,
    limit: ConnectionLimit,
    done: bool,
    timer_generation: u64,
    pending_read_bytes: u64,
    tls: Option<TlsParameters>,
}

#[derive(Clone)]
pub(super) struct TlsParameters {
    pub(super) config: Arc<ClientConfig>,
    pub(super) server_name: ServerName<'static>,
}

pub(super) enum Expiration {
    Stopped,
    RequestTimeout,
    ConnectionTimeout,
}

impl Connection {
    pub(super) fn connect(
        addresses: Arc<[SocketAddr]>,
        requests: Arc<RequestSequence>,
        limit: ConnectionLimit,
        tls: Option<TlsParameters>,
    ) -> Result<Self, RunError> {
        let (stream, address_index) = connect_from(&addresses, 0)?;
        let stream = Transport::new(stream, tls.as_ref())?;
        let (method, request, uri, not_before) = requests.next();
        Ok(Self {
            stream,
            addresses,
            address_index,
            requests,
            method,
            uri,
            request,
            not_before,
            awaiting_pace: false,
            reconnect_at_pace: false,
            write_offset: 0,
            decoder: ResponseDecoder::new(method == crate::Method::Head),
            started: None,
            connected_at: Instant::now(),
            completed: 0,
            limit,
            done: false,
            timer_generation: 0,
            pending_read_bytes: 0,
            tls,
        })
    }

    pub(super) fn write_request(&mut self) -> Result<(), RunError> {
        if self.write_offset == self.request.len() {
            self.stream.flush_tls()?;
            return Ok(());
        }
        if !self.stream.finish_handshake()? {
            return Ok(());
        }
        if self
            .not_before
            .is_some_and(|not_before| Instant::now() < not_before)
        {
            if !self.awaiting_pace {
                self.awaiting_pace = true;
                self.timer_generation += 1;
            }
            return Ok(());
        }
        self.awaiting_pace = false;
        if self.started.is_none() {
            self.started = Some(Instant::now());
            self.timer_generation += 1;
        }
        loop {
            match self.stream.write(&self.request[self.write_offset..]) {
                Ok(0) => {
                    return Err(RunError::Io(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "socket accepted zero request bytes",
                    )));
                }
                Ok(written) => {
                    self.write_offset += written;
                    if self.write_offset == self.request.len() {
                        return Ok(());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
                Err(error)
                    if self.started.is_none()
                        && error.kind() == std::io::ErrorKind::InvalidData =>
                {
                    return Err(RunError::Tls(error.to_string()));
                }
                Err(error) => return Err(RunError::Io(error)),
            }
        }
    }

    pub(super) fn register(&mut self, registry: &Registry, token: Token) -> Result<(), RunError> {
        registry.register(
            self.stream.socket_mut(),
            token,
            Interest::READABLE | Interest::WRITABLE,
        )?;
        Ok(())
    }

    pub(super) fn refresh_interest(
        &mut self,
        registry: &Registry,
        token: Token,
    ) -> Result<(), RunError> {
        let request_ready = self
            .not_before
            .is_none_or(|not_before| Instant::now() >= not_before);
        let interest = self
            .stream
            .interest(self.request_is_written(), request_ready);
        registry.reregister(self.stream.socket_mut(), token, interest)?;
        Ok(())
    }

    pub(super) fn take_error(&self) -> Result<Option<std::io::Error>, RunError> {
        Ok(self.stream.socket().take_error()?)
    }

    pub(super) fn is_done(&self) -> bool {
        self.done
    }

    pub(super) fn has_started(&self) -> bool {
        self.started.is_some()
    }

    pub(super) fn generation(&self) -> u64 {
        self.timer_generation
    }

    pub(super) fn read_response(&mut self) -> Result<Option<CompletedResponse>, RunError> {
        let mut eof = false;
        let mut buffer = [0; 8192];
        loop {
            match self.stream.read(&mut buffer) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(read) => {
                    self.pending_read_bytes += read as u64;
                    if let Some(parsed) = self.decoder.feed(&buffer[..read], false)? {
                        return Ok(Some(self.completed_response(parsed, false)));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(RunError::Io(error)),
            }
        }
        let parsed = self.decoder.feed(&[], eof).map_err(|error| {
            if eof {
                RunError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    error.to_string(),
                ))
            } else {
                error
            }
        })?;
        let Some(parsed) = parsed else {
            return Ok(None);
        };
        Ok(Some(self.completed_response(parsed, eof)))
    }

    pub(super) fn take_read_bytes(&mut self) -> u64 {
        std::mem::take(&mut self.pending_read_bytes)
    }

    fn completed_response(&self, parsed: ParsedResponse, eof: bool) -> CompletedResponse {
        CompletedResponse {
            status: parsed.status,
            body_length: parsed.body_length,
            connection_close: parsed.connection_close || eof,
            latency: self.started.unwrap_or(self.connected_at).elapsed(),
            method: self.method,
            uri: self.uri.clone(),
            request: Arc::clone(&self.request),
        }
    }

    pub(super) fn finish_response(
        &mut self,
        connection_close: bool,
        registry: &Registry,
        token: Token,
    ) -> Result<bool, RunError> {
        self.completed += 1;
        if !self.limit.should_continue(self.completed) {
            self.done = true;
            registry.deregister(self.stream.socket_mut())?;
            return Ok(true);
        }
        self.write_offset = 0;
        self.install_next_request();
        self.started = None;
        self.timer_generation += 1;
        if connection_close {
            registry.deregister(self.stream.socket_mut())?;
            let (stream, address_index) = connect_from(&self.addresses, 0)?;
            self.stream = Transport::new(stream, self.tls.as_ref())?;
            self.address_index = address_index;
            self.connected_at = Instant::now();
            registry.register(
                self.stream.socket_mut(),
                token,
                Interest::READABLE | Interest::WRITABLE,
            )?;
        } else {
            registry.reregister(
                self.stream.socket_mut(),
                token,
                Interest::READABLE | Interest::WRITABLE,
            )?;
            self.write_request()?;
            self.refresh_interest(registry, token)?;
        }
        Ok(false)
    }

    pub(super) fn stop_if_expired(&mut self, registry: &Registry) -> Result<bool, RunError> {
        if self.started.is_none() && !self.limit.should_continue(self.completed) {
            self.done = true;
            if !self.reconnect_at_pace {
                registry.deregister(self.stream.socket_mut())?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn request_is_written(&self) -> bool {
        self.write_offset == self.request.len()
    }

    fn install_next_request(&mut self) {
        let (method, request, uri, not_before) = self.requests.next();
        self.method = method;
        self.uri = uri;
        self.request = request;
        self.not_before = not_before;
        self.awaiting_pace = false;
        self.reconnect_at_pace = false;
        self.decoder = ResponseDecoder::new(method == crate::Method::Head);
    }

    pub(super) fn retry_address(
        &mut self,
        _original_error: std::io::Error,
        registry: &Registry,
        token: Token,
    ) -> Result<bool, RunError> {
        registry.deregister(self.stream.socket_mut())?;
        let next = self.address_index + 1;
        let next = if next < self.addresses.len() {
            next
        } else if self.limit.deadline().is_some() {
            0
        } else {
            self.done = true;
            return Ok(false);
        };
        let (stream, address_index) = connect_from(&self.addresses, next)?;
        self.stream = Transport::new(stream, self.tls.as_ref())?;
        self.address_index = address_index;
        self.connected_at = Instant::now();
        self.awaiting_pace = false;
        self.reconnect_at_pace = false;
        self.timer_generation += 1;
        registry.register(
            self.stream.socket_mut(),
            token,
            Interest::READABLE | Interest::WRITABLE,
        )?;
        Ok(true)
    }

    pub(super) fn unfinished_requests(&self) -> u64 {
        match self.limit {
            ConnectionLimit::Requests(requests) => requests.saturating_sub(self.completed),
            ConnectionLimit::Deadline(_) => 0,
        }
    }

    pub(super) fn recover_request(
        &mut self,
        registry: &Registry,
        token: Token,
    ) -> Result<bool, RunError> {
        if self
            .limit
            .deadline()
            .is_none_or(|deadline| deadline <= Instant::now())
        {
            return Ok(false);
        }
        registry.deregister(self.stream.socket_mut())?;
        let (stream, address_index) = connect_from(&self.addresses, 0)?;
        self.stream = Transport::new(stream, self.tls.as_ref())?;
        self.address_index = address_index;
        self.write_offset = 0;
        self.decoder = ResponseDecoder::new(self.method == crate::Method::Head);
        self.started = None;
        self.connected_at = Instant::now();
        self.awaiting_pace = false;
        self.reconnect_at_pace = false;
        self.timer_generation += 1;
        registry.register(
            self.stream.socket_mut(),
            token,
            Interest::READABLE | Interest::WRITABLE,
        )?;
        Ok(true)
    }

    pub(super) fn defer_reconnect_until_pace(
        &mut self,
        registry: &Registry,
    ) -> Result<bool, RunError> {
        if self.started.is_some()
            || self
                .not_before
                .is_none_or(|not_before| Instant::now() >= not_before)
        {
            return Ok(false);
        }
        registry.deregister(self.stream.socket_mut())?;
        self.awaiting_pace = true;
        self.reconnect_at_pace = true;
        self.timer_generation += 1;
        Ok(true)
    }

    pub(super) fn resume_at_pace(
        &mut self,
        registry: &Registry,
        token: Token,
    ) -> Result<(), RunError> {
        if !self.reconnect_at_pace {
            return self.refresh_interest(registry, token);
        }
        let (stream, address_index) = connect_from(&self.addresses, 0)?;
        self.stream = Transport::new(stream, self.tls.as_ref())?;
        self.address_index = address_index;
        self.connected_at = Instant::now();
        self.awaiting_pace = false;
        self.reconnect_at_pace = false;
        self.timer_generation += 1;
        registry.register(
            self.stream.socket_mut(),
            token,
            Interest::READABLE | Interest::WRITABLE,
        )?;
        Ok(())
    }

    pub(super) fn stop_after_duration_error(
        &mut self,
        registry: &Registry,
    ) -> Result<bool, RunError> {
        if self
            .limit
            .deadline()
            .is_some_and(|deadline| deadline <= Instant::now())
        {
            self.done = true;
            registry.deregister(self.stream.socket_mut())?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn next_deadline(&self, timeout: Duration) -> Option<Instant> {
        if self.awaiting_pace {
            return None;
        }
        match self.started {
            Some(started) => Some(started + timeout),
            None => Some(
                self.limit
                    .deadline()
                    .map_or(self.connected_at + timeout, |deadline| {
                        deadline.min(self.connected_at + timeout)
                    }),
            ),
        }
    }

    pub(super) fn pacing_deadline(&self) -> Option<Instant> {
        if !self.awaiting_pace || self.done {
            return None;
        }
        self.not_before.map(|not_before| {
            self.limit
                .deadline()
                .map_or(not_before, |deadline| deadline.min(not_before))
        })
    }

    pub(super) fn expire(&mut self, registry: &Registry) -> Result<Expiration, RunError> {
        if self.started.is_some() {
            return Ok(Expiration::RequestTimeout);
        }
        if !self.limit.should_continue(self.completed) {
            self.done = true;
            registry.deregister(self.stream.socket_mut())?;
            Ok(Expiration::Stopped)
        } else {
            Ok(Expiration::ConnectionTimeout)
        }
    }
}

enum Transport {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Transport {
    fn new(stream: TcpStream, tls: Option<&TlsParameters>) -> Result<Self, RunError> {
        match tls {
            Some(tls) => {
                let connection =
                    ClientConnection::new(Arc::clone(&tls.config), tls.server_name.clone())
                        .map_err(|error| RunError::Tls(error.to_string()))?;
                Ok(Self::Tls(Box::new(StreamOwned::new(connection, stream))))
            }
            None => Ok(Self::Plain(stream)),
        }
    }

    fn socket(&self) -> &TcpStream {
        match self {
            Self::Plain(stream) => stream,
            Self::Tls(stream) => &stream.sock,
        }
    }

    fn socket_mut(&mut self) -> &mut TcpStream {
        match self {
            Self::Plain(stream) => stream,
            Self::Tls(stream) => &mut stream.sock,
        }
    }

    fn interest(&self, request_written: bool, request_ready: bool) -> Interest {
        match self {
            Self::Plain(_) if request_written => Interest::READABLE,
            Self::Plain(_) if !request_ready => Interest::READABLE,
            Self::Plain(_) => Interest::READABLE | Interest::WRITABLE,
            Self::Tls(stream)
                if stream.conn.wants_write()
                    || (!stream.conn.is_handshaking() && !request_written && request_ready) =>
            {
                Interest::READABLE | Interest::WRITABLE
            }
            Self::Tls(_) => Interest::READABLE,
        }
    }

    fn finish_handshake(&mut self) -> Result<bool, RunError> {
        let Self::Tls(stream) = self else {
            return Ok(true);
        };
        if !stream.conn.is_handshaking() {
            return Ok(true);
        }
        match stream.conn.complete_io(&mut stream.sock) {
            Ok(_) => Ok(!stream.conn.is_handshaking()),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                Ok(!stream.conn.is_handshaking())
            }
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                Err(RunError::Tls(error.to_string()))
            }
            Err(error) => Err(RunError::Io(error)),
        }
    }

    fn flush_tls(&mut self) -> Result<(), RunError> {
        let Self::Tls(stream) = self else {
            return Ok(());
        };
        if !stream.conn.wants_write() {
            return Ok(());
        }
        match stream.conn.complete_io(&mut stream.sock) {
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
            Err(error) => Err(RunError::Io(error)),
        }
    }
}

impl Read for Transport {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buffer),
            Self::Tls(stream) => stream.read(buffer),
        }
    }
}

impl Write for Transport {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buffer),
            Self::Tls(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            Self::Tls(stream) => stream.flush(),
        }
    }
}

fn connect_from(addresses: &[SocketAddr], start: usize) -> Result<(TcpStream, usize), RunError> {
    let mut last_error = None;
    for (index, address) in addresses.iter().enumerate().skip(start) {
        match TcpStream::connect(*address) {
            Ok(stream) => return Ok((stream, index)),
            Err(error) => last_error = Some(error),
        }
    }
    Err(RunError::Io(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no addresses remain")
    })))
}

pub(super) struct CompletedResponse {
    pub(super) status: u16,
    pub(super) body_length: usize,
    pub(super) connection_close: bool,
    pub(super) latency: Duration,
    pub(super) method: Method,
    pub(super) uri: Range<usize>,
    pub(super) request: Arc<[u8]>,
}
