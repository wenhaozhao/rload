use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use mio::{Events, Poll, Token};
use rustls::{ClientConfig, RootCertStore};

mod connection;

use crate::access_log::{self, ReplayRequest};
use crate::replay_filter;
use crate::request_file::{self, validate_request};
use crate::request_sequence::{EncodedRequest, RequestSequence, method_uri_start};
use crate::target::Target;
use crate::{
    ReplayFilter, ReplayOptions, ReplayOrder, RequestOptions, RunConfig, RunError, RunLimit,
    RunSummary,
};
use connection::{Connection, Expiration, TlsParameters};

pub fn run(config: RunConfig) -> Result<RunSummary, RunError> {
    run_with_roots(
        config,
        RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned()),
        RequestInput::Default,
    )
}

pub fn run_access_log(config: RunConfig, path: impl AsRef<Path>) -> Result<RunSummary, RunError> {
    run_access_log_with_options(config, path, ReplayOptions::default())
}

pub fn run_access_log_with_options(
    config: RunConfig,
    path: impl AsRef<Path>,
    options: ReplayOptions,
) -> Result<RunSummary, RunError> {
    run_access_log_with_filter(config, path, options, ReplayFilter::default())
}

pub fn run_access_log_with_filter(
    config: RunConfig,
    path: impl AsRef<Path>,
    options: ReplayOptions,
    filter: ReplayFilter,
) -> Result<RunSummary, RunError> {
    validate_replay_options(&options)?;
    let replay = access_log::read(path.as_ref())?;
    let skipped_methods = replay.skipped_methods;
    let (replay, filtered) = replay_filter::apply(replay.requests, &filter)?;
    run_with_roots(
        config,
        RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned()),
        RequestInput::Replay(replay, options, filtered, skipped_methods),
    )
}

pub fn run_request_file(config: RunConfig, path: impl AsRef<Path>) -> Result<RunSummary, RunError> {
    run_request_file_with_options(config, path, ReplayOptions::default())
}

pub fn run_request_file_with_options(
    config: RunConfig,
    path: impl AsRef<Path>,
    options: ReplayOptions,
) -> Result<RunSummary, RunError> {
    run_request_file_with_filter(config, path, options, ReplayFilter::default())
}

pub fn run_request_file_with_filter(
    config: RunConfig,
    path: impl AsRef<Path>,
    options: ReplayOptions,
    filter: ReplayFilter,
) -> Result<RunSummary, RunError> {
    validate_replay_options(&options)?;
    if options.timestamps {
        return Err(RunError::InvalidConfig(
            "timestamp pacing requires an access log".into(),
        ));
    }
    let replay = request_file::read(path.as_ref())?;
    let (replay, filtered) = replay_filter::apply(replay, &filter)?;
    run_with_roots(
        config,
        RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned()),
        RequestInput::Replay(replay, options, filtered, Default::default()),
    )
}

pub fn run_with_request(
    config: RunConfig,
    options: RequestOptions,
) -> Result<RunSummary, RunError> {
    run_with_roots(
        config,
        RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned()),
        RequestInput::Single(options),
    )
}

fn validate_replay_options(options: &ReplayOptions) -> Result<(), RunError> {
    if options.order == ReplayOrder::Sequential && options.seed.is_some() {
        return Err(RunError::InvalidConfig(
            "replay seed requires shuffle or random order".into(),
        ));
    }
    if options.timestamps && options.order != ReplayOrder::Sequential {
        return Err(RunError::InvalidConfig(
            "timestamp pacing requires sequential replay order".into(),
        ));
    }
    if options.timestamps && options.rate.is_some() {
        return Err(RunError::InvalidConfig(
            "timestamp pacing cannot be combined with a fixed replay rate".into(),
        ));
    }
    if !options.stages.is_empty() && (options.rate.is_some() || options.timestamps) {
        return Err(RunError::InvalidConfig(
            "replay stages cannot be combined with fixed-rate or timestamp pacing".into(),
        ));
    }
    if options
        .stages
        .iter()
        .any(|stage| stage.duration.is_zero() || stage.rate == 0)
    {
        return Err(RunError::InvalidConfig(
            "replay stages require positive durations and rates".into(),
        ));
    }
    if options
        .stages
        .iter()
        .try_fold(Duration::ZERO, |total, stage| {
            total.checked_add(stage.duration)
        })
        .is_none()
    {
        return Err(RunError::InvalidConfig(
            "cumulative replay stage duration is too large".into(),
        ));
    }
    if !options.speed.is_finite() || options.speed <= 0.0 {
        return Err(RunError::InvalidConfig(
            "replay speed must be a finite number greater than zero".into(),
        ));
    }
    Ok(())
}

fn run_with_roots(
    config: RunConfig,
    root_store: RootCertStore,
    input: RequestInput,
) -> Result<RunSummary, RunError> {
    let target = Target::parse(&config.url)?;
    config.validate()?;
    let (requests, filtered_replay_entries, skipped_access_log_methods, replay_rate) = match input {
        RequestInput::Replay(replay, options, filtered, skipped_methods) => (
            {
                if options.timestamps {
                    validate_timestamps(&replay)?;
                }
                let sequence = RequestSequence::new(
                    replay
                        .into_iter()
                        .map(|request| {
                            let uri_start = request.method.as_str().len() + 1;
                            let uri = uri_start..uri_start + request.path.len();
                            let bytes = target.replay_request(&request);
                            EncodedRequest {
                                bytes,
                                method_uri_start: method_uri_start(request.method, uri.start),
                                uri_end: uri.end as u32,
                                timestamp_micros: request.timestamp_micros,
                            }
                        })
                        .collect(),
                    options.order,
                    options.seed.unwrap_or_else(replay_seed),
                )
                .with_rate(options.rate)
                .with_stages(&options.stages);
                if options.timestamps {
                    sequence.with_timestamps(options.speed)
                } else {
                    sequence
                }
            },
            filtered,
            skipped_methods,
            options.rate,
        ),
        RequestInput::Single(options) => {
            let request = ReplayRequest {
                method: config.method,
                path: target.path().to_owned(),
                headers: options.headers,
                body_present: options.body.is_some(),
                body: options.body.unwrap_or_default(),
                timestamp_micros: None,
            };
            validate_request(&request).map_err(RunError::InvalidConfig)?;
            let bytes = target.replay_request(&request);
            (
                RequestSequence::new(
                    vec![EncodedRequest {
                        bytes,
                        method_uri_start: method_uri_start(
                            request.method,
                            request.method.as_str().len() + 1,
                        ),
                        uri_end: (request.method.as_str().len() + 1 + request.path.len()) as u32,
                        timestamp_micros: None,
                    }],
                    ReplayOrder::Sequential,
                    0,
                ),
                0,
                Default::default(),
                None,
            )
        }
        RequestInput::Default => (
            RequestSequence::new(
                vec![EncodedRequest {
                    bytes: target.request(config.method),
                    method_uri_start: method_uri_start(
                        config.method,
                        config.method.as_str().len() + 1,
                    ),
                    uri_end: (config.method.as_str().len() + 1 + target.path().len()) as u32,
                    timestamp_micros: None,
                }],
                ReplayOrder::Sequential,
                0,
            ),
            0,
            Default::default(),
            None,
        ),
    };
    let requests = Arc::new(requests);
    let addresses = target.resolve()?;
    let tls = target.tls_server_name().map(|server_name| TlsParameters {
        config: Arc::new(
            ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth(),
        ),
        server_name,
    });
    let started = Instant::now();
    let (connection_count, limits, load_deadline) = connection_limits(&config, started);
    let thread_count = config.threads.min(connection_count);
    let mut worker_limits = vec![Vec::new(); thread_count];
    for (index, limit) in limits.into_iter().enumerate() {
        worker_limits[index % thread_count].push(limit);
    }

    let mut summary = std::thread::scope(|scope| {
        let mut workers = Vec::with_capacity(thread_count);
        for limits in worker_limits {
            let requests = Arc::clone(&requests);
            let addresses = Arc::clone(&addresses);
            let tls = tls.clone();
            workers.push(
                scope.spawn(move || run_worker(addresses, requests, limits, config.timeout, tls)),
            );
        }

        let mut summary = RunSummary::default();
        for worker in workers {
            summary.merge(worker.join().map_err(|_| RunError::WorkerPanic)??);
        }
        Ok::<RunSummary, RunError>(summary)
    })?;
    let finished = Instant::now();
    summary.filtered_replay_entries = filtered_replay_entries;
    summary.skipped_access_log_methods = skipped_access_log_methods;
    summary.configured_replay_rate = replay_rate;
    summary.runtime = finished.duration_since(started);
    match config.limit {
        RunLimit::Duration(duration) => {
            summary.load_runtime = duration.min(summary.runtime);
            summary.drain_runtime = finished
                .checked_duration_since(load_deadline.expect("duration runs have a deadline"))
                .unwrap_or_default();
        }
        RunLimit::Requests(_) => summary.load_runtime = summary.runtime,
    }
    let cycles = summary.completed / connection_count as u64;
    if cycles > 0 {
        let expected_micros = (summary.runtime.as_micros() / u128::from(cycles)).max(1);
        let expected = Duration::from_micros(u64::try_from(expected_micros).unwrap_or(u64::MAX));
        summary.latencies.correct_for_coordinated_omission(expected);
        summary.correct_method_histograms(expected);
        summary.coordinated_omission_interval = Some(expected);
    }
    Ok(summary)
}

fn validate_timestamps(requests: &[ReplayRequest]) -> Result<(), RunError> {
    let mut previous = None;
    for request in requests {
        let timestamp = request.timestamp_micros.ok_or_else(|| {
            RunError::InvalidAccessLog(
                "timestamp pacing requires a valid timestamp on every replayable line".into(),
            )
        })?;
        if previous.is_some_and(|previous| timestamp < previous) {
            return Err(RunError::InvalidAccessLog(
                "timestamp pacing requires non-decreasing log timestamps".into(),
            ));
        }
        previous = Some(timestamp);
    }
    Ok(())
}

enum RequestInput {
    Default,
    Single(RequestOptions),
    Replay(
        Vec<ReplayRequest>,
        ReplayOptions,
        u64,
        std::collections::BTreeMap<String, u64>,
    ),
}

fn replay_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    nanos as u64 ^ (nanos >> 64) as u64 ^ u64::from(std::process::id())
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use rcgen::{CertifiedKey, generate_simple_self_signed};
    use rustls::pki_types::PrivatePkcs8KeyDer;
    use rustls::{ServerConfig, ServerConnection, StreamOwned};

    use super::*;
    use crate::Method;

    #[test]
    fn sends_https_requests_over_a_reused_verified_connection() {
        let CertifiedKey { cert, signing_key } =
            generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_config = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(
                    vec![cert.der().clone()],
                    PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
                )
                .unwrap(),
        );
        let server = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            let connection = ServerConnection::new(server_config).unwrap();
            let mut stream = StreamOwned::new(connection, socket);
            for _ in 0..2 {
                read_request_head(&mut stream);
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK")
                    .unwrap();
            }
        });
        let mut roots = RootCertStore::empty();
        roots.add(cert.der().clone()).unwrap();
        let config = RunConfig {
            url: format!("https://{address}/health"),
            method: Method::Get,
            limit: RunLimit::Requests(2),
            connections: 1,
            threads: 1,
            timeout: Duration::from_secs(2),
        };

        let summary = run_with_roots(config, roots, RequestInput::Default).unwrap();

        assert_eq!(summary.completed, 2);
        assert_eq!(summary.response_body_bytes, 4);
        server.join().unwrap();
    }

    #[test]
    fn rejects_an_untrusted_https_certificate() {
        let CertifiedKey { cert, signing_key } =
            generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_config = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(
                    vec![cert.der().clone()],
                    PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
                )
                .unwrap(),
        );
        let server = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            let connection = ServerConnection::new(server_config).unwrap();
            let mut stream = StreamOwned::new(connection, socket);
            stream.read(&mut [0; 1]).is_err()
        });
        let config = RunConfig {
            url: format!("https://{address}/"),
            method: Method::Get,
            limit: RunLimit::Requests(1),
            connections: 1,
            threads: 1,
            timeout: Duration::from_secs(2),
        };

        assert!(matches!(
            run_with_roots(config, RootCertStore::empty(), RequestInput::Default),
            Err(RunError::Io(_) | RunError::Tls(_))
        ));
        assert!(server.join().unwrap());
    }

    #[test]
    fn retries_next_address_when_tls_handshake_io_fails() {
        let CertifiedKey { cert, signing_key } =
            generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let failed_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let failed_address = failed_listener.local_addr().unwrap();
        let failed_server = thread::spawn(move || drop(failed_listener.accept().unwrap()));
        let good_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let good_address = good_listener.local_addr().unwrap();
        let server_config = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(
                    vec![cert.der().clone()],
                    PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
                )
                .unwrap(),
        );
        let good_server = thread::spawn(move || {
            let (socket, _) = good_listener.accept().unwrap();
            let connection = ServerConnection::new(server_config).unwrap();
            let mut stream = StreamOwned::new(connection, socket);
            read_request_head(&mut stream);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK")
                .unwrap();
        });
        let mut roots = RootCertStore::empty();
        roots.add(cert.der().clone()).unwrap();
        let tls = TlsParameters {
            config: Arc::new(
                ClientConfig::builder()
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            ),
            server_name: "127.0.0.1".try_into().unwrap(),
        };

        let summary = run_worker(
            vec![failed_address, good_address].into(),
            Arc::new(RequestSequence::new(
                vec![EncodedRequest {
                    bytes: Arc::from(&b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"[..]),
                    method_uri_start: method_uri_start(Method::Get, 4),
                    uri_end: 11,
                    timestamp_micros: None,
                }],
                ReplayOrder::Sequential,
                0,
            )),
            vec![ConnectionLimit::Requests(1)],
            Duration::from_secs(2),
            Some(tls),
        )
        .unwrap();

        assert_eq!(summary.completed, 1);
        failed_server.join().unwrap();
        good_server.join().unwrap();
    }

    fn read_request_head(stream: &mut impl Read) {
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        assert!(request.starts_with(b"GET /health HTTP/1.1\r\n"));
    }
}

fn connection_limits(
    config: &RunConfig,
    started: Instant,
) -> (usize, Vec<ConnectionLimit>, Option<Instant>) {
    match config.limit {
        RunLimit::Requests(requests) => {
            let count = config
                .connections
                .min(usize::try_from(requests).unwrap_or(usize::MAX));
            let base = requests / count as u64;
            let extra = requests % count as u64;
            let limits = (0..count)
                .map(|index| ConnectionLimit::Requests(base + u64::from((index as u64) < extra)))
                .collect();
            (count, limits, None)
        }
        RunLimit::Duration(duration) => {
            let deadline = started + duration;
            (
                config.connections,
                vec![ConnectionLimit::Deadline(deadline); config.connections],
                Some(deadline),
            )
        }
    }
}

fn run_worker(
    addresses: Arc<[SocketAddr]>,
    requests: Arc<RequestSequence>,
    limits: Vec<ConnectionLimit>,
    timeout: Duration,
    tls: Option<TlsParameters>,
) -> Result<RunSummary, RunError> {
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(limits.len().max(16));
    let mut connections = Vec::with_capacity(limits.len());
    let mut summary = RunSummary::default();
    for (index, limit) in limits.into_iter().enumerate() {
        let mut connection = loop {
            match Connection::connect(
                Arc::clone(&addresses),
                Arc::clone(&requests),
                limit,
                tls.clone(),
            ) {
                Ok(connection) => break connection,
                Err(error) => {
                    let Some(deadline) = limit.deadline() else {
                        return Err(error);
                    };
                    summary.socket_errors.connect += 1;
                    if Instant::now() >= deadline {
                        return Ok(summary);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        };
        connection.register(poll.registry(), Token(index))?;
        connections.push(connection);
    }

    let mut active = connections.len();
    let mut deadlines = BinaryHeap::new();
    let mut pacing = BinaryHeap::new();
    for (token, connection) in connections.iter().enumerate() {
        schedule_deadline(&mut deadlines, Token(token), connection, timeout);
        schedule_pacing(&mut pacing, Token(token), connection);
    }
    while active > 0 {
        discard_stale_deadlines(&mut deadlines, &connections);
        discard_stale_pacing(&mut pacing, &connections);
        let deadline_timeout = deadlines
            .peek()
            .map(|Reverse((deadline, _, _))| deadline.saturating_duration_since(Instant::now()));
        let pacing_timeout = pacing
            .peek()
            .map(|Reverse((deadline, _, _))| deadline.saturating_duration_since(Instant::now()));
        let poll_timeout = match (deadline_timeout, pacing_timeout) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (left, right) => left.or(right),
        };
        poll.poll(&mut events, poll_timeout)?;
        for event in &events {
            let token = event.token();
            let connection = &mut connections[token.0];
            if connection.is_done() {
                continue;
            }
            if event.is_error()
                && let Some(error) = connection.take_error()?
            {
                if connection.has_started() {
                    let read_error = connection.request_is_written();
                    if connection.recover_request(poll.registry(), token)? {
                        if read_error {
                            summary.socket_errors.read += 1;
                        } else {
                            summary.socket_errors.write += 1;
                        }
                        schedule_deadline(&mut deadlines, token, connection, timeout);
                        schedule_pacing(&mut pacing, token, connection);
                        continue;
                    }
                    if connection.stop_after_duration_error(poll.registry())? {
                        if read_error {
                            summary.socket_errors.read += 1;
                        } else {
                            summary.socket_errors.write += 1;
                        }
                        active -= 1;
                        continue;
                    }
                    return Err(RunError::Io(error));
                }
                summary.socket_errors.connect += 1;
                connection.retry_address(error, poll.registry(), token)?;
                schedule_deadline(&mut deadlines, token, connection, timeout);
                schedule_pacing(&mut pacing, token, connection);
                continue;
            }
            if event.is_writable() {
                if connection.stop_if_expired(poll.registry())? {
                    active -= 1;
                    continue;
                }
                if let Some(error) = connection.take_error()? {
                    summary.socket_errors.connect += 1;
                    connection.retry_address(error, poll.registry(), token)?;
                    schedule_deadline(&mut deadlines, token, connection, timeout);
                    schedule_pacing(&mut pacing, token, connection);
                    continue;
                }
                let generation = connection.generation();
                if let Err(error) = connection.write_request() {
                    if !connection.has_started()
                        && let RunError::Io(error) = error
                    {
                        summary.socket_errors.connect += 1;
                        connection.retry_address(error, poll.registry(), token)?;
                        schedule_deadline(&mut deadlines, token, connection, timeout);
                        schedule_pacing(&mut pacing, token, connection);
                        continue;
                    }
                    if connection.recover_request(poll.registry(), token)? {
                        summary.socket_errors.write += 1;
                        schedule_deadline(&mut deadlines, token, connection, timeout);
                        schedule_pacing(&mut pacing, token, connection);
                        continue;
                    }
                    if connection.stop_after_duration_error(poll.registry())? {
                        summary.socket_errors.write += 1;
                        active -= 1;
                        continue;
                    }
                    return Err(error);
                }
                connection.refresh_interest(poll.registry(), token)?;
                if connection.generation() != generation {
                    schedule_deadline(&mut deadlines, token, connection, timeout);
                    schedule_pacing(&mut pacing, token, connection);
                }
            }
            if event.is_readable() || event.is_read_closed() {
                let response = match connection.read_response() {
                    Ok(response) => response,
                    Err(RunError::Io(error))
                        if !connection.has_started()
                            && error.kind() == std::io::ErrorKind::NotConnected =>
                    {
                        connection.refresh_interest(poll.registry(), token)?;
                        continue;
                    }
                    Err(RunError::Io(error)) if !connection.has_started() => {
                        summary.socket_errors.connect += 1;
                        connection.retry_address(error, poll.registry(), token)?;
                        schedule_deadline(&mut deadlines, token, connection, timeout);
                        schedule_pacing(&mut pacing, token, connection);
                        continue;
                    }
                    Err(RunError::Io(error)) => {
                        if connection.recover_request(poll.registry(), token)? {
                            summary.socket_errors.read += 1;
                            schedule_deadline(&mut deadlines, token, connection, timeout);
                            schedule_pacing(&mut pacing, token, connection);
                            continue;
                        }
                        if connection.stop_after_duration_error(poll.registry())? {
                            summary.socket_errors.read += 1;
                            active -= 1;
                            continue;
                        }
                        return Err(RunError::Io(error));
                    }
                    Err(error) => return Err(error),
                };
                if let Some(completed) = response {
                    summary.response_body_bytes += completed.body_length as u64;
                    summary.record_response(
                        completed.method,
                        std::str::from_utf8(&completed.request[completed.uri])
                            .expect("validated request URIs are UTF-8"),
                        completed.status,
                        completed.latency,
                    );
                    if connection.finish_response(
                        completed.connection_close,
                        poll.registry(),
                        token,
                    )? {
                        active -= 1;
                    } else {
                        schedule_deadline(&mut deadlines, token, connection, timeout);
                        schedule_pacing(&mut pacing, token, connection);
                    }
                } else {
                    connection.refresh_interest(poll.registry(), token)?;
                }
            }
        }
        let now = Instant::now();
        while let Some(Reverse((deadline, token, generation))) = deadlines.peek().copied() {
            if deadline > now {
                break;
            }
            deadlines.pop();
            let connection = &mut connections[token];
            if connection.is_done() || connection.generation() != generation {
                continue;
            }
            match connection.expire(poll.registry())? {
                Expiration::Stopped => active -= 1,
                Expiration::RequestTimeout => {
                    summary.socket_errors.timeout += 1;
                    if connection.recover_request(poll.registry(), Token(token))? {
                        schedule_deadline(&mut deadlines, Token(token), connection, timeout);
                        schedule_pacing(&mut pacing, Token(token), connection);
                    } else if connection.stop_after_duration_error(poll.registry())? {
                        active -= 1;
                    } else {
                        return Err(RunError::Io(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "request timed out",
                        )));
                    }
                }
                Expiration::ConnectionTimeout => {
                    summary.socket_errors.connect += 1;
                    let timeout_error =
                        std::io::Error::new(std::io::ErrorKind::TimedOut, "connection timed out");
                    if let Err(error) =
                        connection.retry_address(timeout_error, poll.registry(), Token(token))
                    {
                        if connection.stop_after_duration_error(poll.registry())? {
                            active -= 1;
                        } else {
                            return Err(error);
                        }
                    } else {
                        schedule_deadline(&mut deadlines, Token(token), connection, timeout);
                        schedule_pacing(&mut pacing, Token(token), connection);
                    }
                }
            }
        }
        let now = Instant::now();
        while let Some(Reverse((deadline, token, generation))) = pacing.peek().copied() {
            if deadline > now {
                break;
            }
            pacing.pop();
            let connection = &mut connections[token];
            if connection.is_done() || connection.generation() != generation {
                continue;
            }
            if connection.stop_if_expired(poll.registry())? {
                active -= 1;
                continue;
            }
            connection.refresh_interest(poll.registry(), Token(token))?;
        }
    }
    Ok(summary)
}

type DeadlineQueue = BinaryHeap<Reverse<(Instant, usize, u64)>>;
type PacingQueue = BinaryHeap<Reverse<(Instant, usize, u64)>>;

fn schedule_deadline(
    deadlines: &mut DeadlineQueue,
    token: Token,
    connection: &Connection,
    timeout: Duration,
) {
    if let Some(deadline) = connection.next_deadline(timeout) {
        deadlines.push(Reverse((deadline, token.0, connection.generation())));
    }
}

fn schedule_pacing(pacing: &mut PacingQueue, token: Token, connection: &Connection) {
    if let Some(deadline) = connection.pacing_deadline() {
        pacing.push(Reverse((deadline, token.0, connection.generation())));
    }
}

fn discard_stale_deadlines(deadlines: &mut DeadlineQueue, connections: &[Connection]) {
    while let Some(Reverse((_, token, generation))) = deadlines.peek().copied() {
        let connection = &connections[token];
        if connection.is_done() || connection.generation() != generation {
            deadlines.pop();
        } else {
            break;
        }
    }
}

fn discard_stale_pacing(pacing: &mut PacingQueue, connections: &[Connection]) {
    while let Some(Reverse((deadline, token, generation))) = pacing.peek().copied() {
        let connection = &connections[token];
        if connection.is_done()
            || connection.generation() != generation
            || connection.pacing_deadline() != Some(deadline)
        {
            pacing.pop();
        } else {
            break;
        }
    }
}

#[derive(Clone, Copy)]
enum ConnectionLimit {
    Requests(u64),
    Deadline(Instant),
}

impl ConnectionLimit {
    fn should_continue(self, completed: u64) -> bool {
        match self {
            Self::Requests(requests) => completed < requests,
            Self::Deadline(deadline) => Instant::now() < deadline,
        }
    }

    fn deadline(self) -> Option<Instant> {
        match self {
            Self::Requests(_) => None,
            Self::Deadline(deadline) => Some(deadline),
        }
    }
}
