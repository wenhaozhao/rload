use std::env;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};

const LISTENER: Token = Token(0);
const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\nOK";

fn main() -> std::io::Result<()> {
    let address = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:18080".into())
        .parse::<SocketAddr>()
        .expect("benchmark server address must be host:port");
    let delay_us = parse_u64_arg(2);
    let jitter_us = parse_u64_arg(3);
    let mut listener = TcpListener::bind(address)?;
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(4096);
    let mut connections: Vec<Option<Connection>> = Vec::new();
    poll.registry()
        .register(&mut listener, LISTENER, Interest::READABLE)?;
    println!(
        "READY {} delay_us={} jitter_us={}",
        listener.local_addr()?,
        delay_us,
        jitter_us
    );

    loop {
        let poll_timeout = (delay_us > 0 || jitter_us > 0).then_some(Duration::from_millis(1));
        poll.poll(&mut events, poll_timeout)?;
        for event in &events {
            if event.token() == LISTENER {
                accept_connections(&mut listener, &poll, &mut connections, delay_us, jitter_us)?;
                continue;
            }

            let index = event.token().0 - 1;
            let Some(connection) = connections.get_mut(index).and_then(Option::as_mut) else {
                continue;
            };
            let mut closed = if event.is_readable() {
                connection.read_request().unwrap_or(true)
            } else {
                false
            };
            if !closed && connection.response_due() {
                connection.enable_writes(poll.registry(), event.token())?;
            }
            if !closed && event.is_writable() {
                match connection.write_response() {
                    Ok(true) => poll.registry().reregister(
                        &mut connection.stream,
                        event.token(),
                        Interest::READABLE,
                    )?,
                    Ok(false) => {}
                    Err(_) => closed = true,
                }
            }
            if closed {
                poll.registry().deregister(&mut connection.stream)?;
                connections[index] = None;
            }
        }
        for (index, connection) in connections.iter_mut().enumerate() {
            if let Some(connection) = connection
                && connection.response_due()
            {
                connection.enable_writes(poll.registry(), Token(index + 1))?;
            }
        }
    }
}

fn accept_connections(
    listener: &mut TcpListener,
    poll: &Poll,
    connections: &mut Vec<Option<Connection>>,
    delay_us: u64,
    jitter_us: u64,
) -> std::io::Result<()> {
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nodelay(true)?;
                let index = connections
                    .iter()
                    .position(Option::is_none)
                    .unwrap_or(connections.len());
                let token = Token(index + 1);
                poll.registry()
                    .register(&mut stream, token, Interest::READABLE)?;
                let connection = Some(Connection::new(stream, delay_us, jitter_us, token.0 as u64));
                if index == connections.len() {
                    connections.push(connection);
                } else {
                    connections[index] = connection;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}

struct Connection {
    stream: TcpStream,
    request: Vec<u8>,
    response_offset: Option<usize>,
    response_ready_at: Option<Instant>,
    writes_enabled: bool,
    delay_us: u64,
    jitter_us: u64,
    random_state: u64,
}

impl Connection {
    fn new(stream: TcpStream, delay_us: u64, jitter_us: u64, seed: u64) -> Self {
        Self {
            stream,
            request: Vec::with_capacity(1024),
            response_offset: None,
            response_ready_at: None,
            writes_enabled: false,
            delay_us,
            jitter_us,
            random_state: seed.max(1),
        }
    }

    fn read_request(&mut self) -> std::io::Result<bool> {
        let mut buffer = [0; 8192];
        loop {
            match self.stream.read(&mut buffer) {
                Ok(0) => return Ok(true),
                Ok(read) => {
                    self.request.extend_from_slice(&buffer[..read]);
                    if let Some(end) = find_header_end(&self.request) {
                        self.request.drain(..end + 4);
                        self.response_offset = Some(0);
                        self.random_state = self
                            .random_state
                            .wrapping_mul(6_364_136_223_846_793_005)
                            .wrapping_add(1);
                        let jitter = if self.jitter_us == 0 {
                            0
                        } else {
                            self.random_state % (self.jitter_us + 1)
                        };
                        self.response_ready_at =
                            Some(Instant::now() + Duration::from_micros(self.delay_us + jitter));
                        return Ok(false);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
                Err(error) => return Err(error),
            }
        }
    }

    fn response_due(&self) -> bool {
        self.response_offset.is_some()
            && !self.writes_enabled
            && self
                .response_ready_at
                .is_some_and(|ready| Instant::now() >= ready)
    }

    fn enable_writes(&mut self, registry: &mio::Registry, token: Token) -> std::io::Result<()> {
        registry.reregister(&mut self.stream, token, Interest::WRITABLE)?;
        self.writes_enabled = true;
        Ok(())
    }

    fn write_response(&mut self) -> std::io::Result<bool> {
        let Some(offset) = self.response_offset else {
            return Ok(false);
        };
        match self.stream.write(&RESPONSE[offset..]) {
            Ok(0) => Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "benchmark response write returned zero",
            )),
            Ok(written) if offset + written == RESPONSE.len() => {
                self.response_offset = None;
                self.response_ready_at = None;
                self.writes_enabled = false;
                Ok(true)
            }
            Ok(written) => {
                self.response_offset = Some(offset + written);
                Ok(false)
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
            Err(error) => Err(error),
        }
    }
}

fn parse_u64_arg(index: usize) -> u64 {
    env::args()
        .nth(index)
        .map(|value| {
            value
                .parse::<u64>()
                .expect("delay and jitter must be integers")
        })
        .unwrap_or(0)
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}
