use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rload::{Method, RunConfig, RunError, RunLimit, run};

fn spawn_server(response: &'static [u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 1024];
        let size = stream.read(&mut request).unwrap();
        assert!(request[..size].starts_with(b"GET /health HTTP/1.1\r\n"));
        stream.write_all(response).unwrap();
    });

    format!("http://{address}/health")
}

fn read_request_head(stream: &mut std::net::TcpStream) -> std::io::Result<()> {
    let mut request = Vec::new();
    let mut byte = [0];
    while !request.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte)?;
        request.push(byte[0]);
    }
    Ok(())
}

#[test]
fn run_sends_request_and_records_content_length_response() {
    let url = spawn_server(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK");
    let config = RunConfig {
        url,
        method: Method::Get,
        limit: RunLimit::Requests(1),
        connections: 1,
        threads: 1,
        timeout: Duration::from_secs(1),
    };

    let summary = run(config).unwrap();

    assert_eq!(summary.completed, 1);
    assert_eq!(summary.response_body_bytes, 2);
    assert_eq!(summary.status_errors, 0);
    assert_eq!(summary.latencies.len(), 1);
    assert!(summary.latencies.mean() > Duration::ZERO);
    assert!(summary.latencies.percentile(99.0).unwrap() > Duration::ZERO);
    assert!(summary.runtime > Duration::ZERO);
    assert_eq!(summary.load_runtime, summary.runtime);
    assert_eq!(summary.drain_runtime, Duration::ZERO);
    assert!(summary.coordinated_omission_interval.is_some());
}

#[test]
fn run_counts_error_statuses() {
    let url = spawn_server(
        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let config = RunConfig {
        url,
        method: Method::Get,
        limit: RunLimit::Requests(1),
        connections: 1,
        threads: 1,
        timeout: Duration::from_secs(1),
    };

    let summary = run(config).unwrap();

    assert_eq!(summary.completed, 1);
    assert_eq!(summary.status_errors, 1);
    assert_eq!(summary.method(Method::Get).completed, 1);
    assert_eq!(summary.method(Method::Get).status_errors, 1);
    assert_eq!(summary.status_count(503), 1);
    assert_eq!(summary.status_count(200), 0);
}

#[test]
fn run_reuses_connection_for_multiple_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        for _ in 0..2 {
            read_request_head(&mut stream).unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK")
                .unwrap();
        }
    });
    let config = RunConfig {
        url: format!("http://{address}/health"),
        method: Method::Get,
        limit: RunLimit::Requests(2),
        connections: 1,
        threads: 1,
        timeout: Duration::from_secs(1),
    };

    let summary = run(config).unwrap();

    assert_eq!(summary.completed, 2);
    assert_eq!(summary.response_body_bytes, 4);
    server.join().unwrap();
}

#[test]
fn run_reads_chunked_response_body() {
    let url = spawn_server(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\nConnection: keep-alive, close\r\n\r\n2\r\nOK\r\n0\r\n\r\n",
    );
    let config = RunConfig {
        url,
        method: Method::Get,
        limit: RunLimit::Requests(1),
        connections: 1,
        threads: 1,
        timeout: Duration::from_secs(1),
    };

    let summary = run(config).unwrap();

    assert_eq!(summary.completed, 1);
    assert_eq!(summary.response_body_bytes, 2);
}

#[test]
fn run_rejects_invalid_status_line() {
    let url = spawn_server(b"not-http\r\n\r\nOK");
    let config = RunConfig {
        url,
        method: Method::Get,
        limit: RunLimit::Requests(1),
        connections: 1,
        threads: 1,
        timeout: Duration::from_secs(1),
    };

    let error = run(config).unwrap_err();

    assert!(matches!(error, RunError::InvalidResponse(_)));
    assert!(error.to_string().contains("status line is invalid"));
}

#[test]
fn duration_run_does_not_recover_invalid_http_responses() {
    let url = spawn_server(b"not HTTP\r\n\r\n");
    let config = RunConfig {
        url,
        method: Method::Get,
        limit: RunLimit::Duration(Duration::from_millis(50)),
        connections: 1,
        threads: 1,
        timeout: Duration::from_millis(20),
    };

    let error = run(config).unwrap_err();

    assert!(matches!(error, RunError::InvalidResponse(_)));
    assert!(error.to_string().contains("status line is invalid"));
}

#[test]
fn run_reads_body_delimited_by_connection_close() {
    let url = spawn_server(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nOK");
    let config = RunConfig {
        url,
        method: Method::Get,
        limit: RunLimit::Requests(1),
        connections: 1,
        threads: 1,
        timeout: Duration::from_secs(1),
    };

    let summary = run(config).unwrap();

    assert_eq!(summary.response_body_bytes, 2);
}

#[test]
fn run_uses_multiple_connections_concurrently() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut streams = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            read_request_head(&mut stream).unwrap();
            streams.push(stream);
        }
        for mut stream in streams {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK")
                .unwrap();
        }
    });
    let config = RunConfig {
        url: format!("http://{address}/health"),
        method: Method::Get,
        limit: RunLimit::Requests(2),
        connections: 2,
        threads: 1,
        timeout: Duration::from_secs(1),
    };

    let summary = run(config).unwrap();

    assert_eq!(summary.completed, 2);
    assert_eq!(summary.response_body_bytes, 4);
    assert_eq!(summary.latencies.len(), 2);
    server.join().unwrap();
}

#[test]
fn run_sends_requests_until_duration_expires() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        loop {
            if read_request_head(&mut stream).is_err() {
                return;
            }
            if stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .is_err()
            {
                return;
            }
        }
    });
    let config = RunConfig {
        url: format!("http://{address}/health"),
        method: Method::Get,
        limit: RunLimit::Duration(Duration::from_millis(30)),
        connections: 1,
        threads: 1,
        timeout: Duration::from_secs(1),
    };
    let started = std::time::Instant::now();

    let summary = run(config).unwrap();

    assert!(summary.completed > 1);
    assert!(started.elapsed() < Duration::from_millis(500));
    server.join().unwrap();
}

#[test]
fn run_distributes_uneven_request_count_and_merges_worker_metrics() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let seen = Arc::new(AtomicU64::new(0));
    let server_seen = Arc::clone(&seen);
    let server = thread::spawn(move || {
        let mut handlers = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let seen = Arc::clone(&server_seen);
            handlers.push(thread::spawn(move || {
                while read_request_head(&mut stream).is_ok() {
                    let sequence = seen.fetch_add(1, Ordering::Relaxed);
                    let status = if sequence.is_multiple_of(2) {
                        "503 Service Unavailable"
                    } else {
                        "200 OK"
                    };
                    let response = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n");
                    if stream.write_all(response.as_bytes()).is_err() {
                        break;
                    }
                }
            }));
        }
        for handler in handlers {
            handler.join().unwrap();
        }
    });
    let config = RunConfig {
        url: format!("http://{address}/health"),
        method: Method::Get,
        limit: RunLimit::Requests(5),
        connections: 2,
        threads: 1,
        timeout: Duration::from_secs(1),
    };

    let summary = run(config).unwrap();

    assert_eq!(summary.completed, 5);
    assert_eq!(summary.status_errors, 3);
    assert_eq!(summary.latencies.len(), 5);
    assert_eq!(seen.load(Ordering::Relaxed), 5);
    server.join().unwrap();
}

#[test]
fn run_caps_connections_at_fixed_request_count() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut streams = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            read_request_head(&mut stream).unwrap();
            streams.push(stream);
        }
        for mut stream in streams {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        }
    });
    let config = RunConfig {
        url: format!("http://{address}/health"),
        method: Method::Get,
        limit: RunLimit::Requests(2),
        connections: 5,
        threads: 2,
        timeout: Duration::from_millis(200),
    };

    let summary = run(config).unwrap();

    assert_eq!(summary.completed, 2);
    server.join().unwrap();
}

#[test]
fn duration_run_drains_response_started_before_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request_head(&mut stream).unwrap();
        thread::sleep(Duration::from_millis(50));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });
    let config = RunConfig {
        url: format!("http://{address}/health"),
        method: Method::Get,
        limit: RunLimit::Duration(Duration::from_millis(10)),
        connections: 1,
        threads: 1,
        timeout: Duration::from_secs(1),
    };
    let started = std::time::Instant::now();

    let summary = run(config).unwrap();

    assert_eq!(summary.completed, 1);
    assert_eq!(summary.load_runtime, Duration::from_millis(10));
    assert!(summary.drain_runtime >= Duration::from_millis(40));
    assert!(started.elapsed() >= Duration::from_millis(50));
    server.join().unwrap();
}

#[test]
fn run_rejects_invalid_limits_connections_threads_and_timeout() {
    for config in [
        RunConfig {
            url: "http://localhost/".into(),
            method: Method::Get,
            limit: RunLimit::Requests(0),
            connections: 1,
            threads: 1,
            timeout: Duration::from_secs(1),
        },
        RunConfig {
            url: "http://localhost/".into(),
            method: Method::Get,
            limit: RunLimit::Requests(1),
            connections: 1,
            threads: 1,
            timeout: Duration::from_secs(60 * 60 + 1),
        },
        RunConfig {
            url: "http://localhost/".into(),
            method: Method::Get,
            limit: RunLimit::Duration(Duration::ZERO),
            connections: 1,
            threads: 1,
            timeout: Duration::from_secs(1),
        },
        RunConfig {
            url: "http://localhost/".into(),
            method: Method::Get,
            limit: RunLimit::Requests(1),
            connections: 0,
            threads: 1,
            timeout: Duration::from_secs(1),
        },
        RunConfig {
            url: "http://localhost/".into(),
            method: Method::Get,
            limit: RunLimit::Requests(1),
            connections: 1,
            threads: 0,
            timeout: Duration::from_secs(1),
        },
    ] {
        assert!(matches!(run(config), Err(RunError::InvalidConfig(_))));
    }
}

#[test]
fn run_uses_fixed_worker_threads_for_many_connections() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut streams = Vec::new();
        for _ in 0..8 {
            let (mut stream, _) = listener.accept().unwrap();
            read_request_head(&mut stream).unwrap();
            streams.push(stream);
        }
        for mut stream in streams {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        }
    });
    let config = RunConfig {
        url: format!("http://{address}/health"),
        method: Method::Get,
        limit: RunLimit::Requests(8),
        connections: 8,
        threads: 2,
        timeout: Duration::from_secs(1),
    };

    let summary = run(config).unwrap();

    assert_eq!(summary.completed, 8);
    server.join().unwrap();
}

#[test]
fn run_reconnects_after_http_1_0_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            read_request_head(&mut stream).unwrap();
            stream
                .write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
            thread::sleep(Duration::from_millis(20));
        }
    });
    let config = RunConfig {
        url: format!("http://{address}/health"),
        method: Method::Get,
        limit: RunLimit::Requests(2),
        connections: 1,
        threads: 1,
        timeout: Duration::from_secs(1),
    };

    let summary = run(config).unwrap();

    assert_eq!(summary.completed, 2);
    server.join().unwrap();
}

#[test]
fn run_connects_when_localhost_has_an_unreachable_address() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request_head(&mut stream).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });
    let config = RunConfig {
        url: format!("http://localhost:{port}/health"),
        method: Method::Get,
        limit: RunLimit::Requests(1),
        connections: 1,
        threads: 1,
        timeout: Duration::from_secs(1),
    };

    let summary = run(config).unwrap();

    assert_eq!(summary.completed, 1);
    server.join().unwrap();
}

#[test]
fn run_rejects_response_headers_larger_than_limit() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request_head(&mut stream).unwrap();
        let oversized = "a".repeat(65 * 1024);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nX-Oversized: {oversized}\r\nContent-Length: 0\r\n\r\n"
        )
        .unwrap();
    });
    let config = RunConfig {
        url: format!("http://{address}/health"),
        method: Method::Get,
        limit: RunLimit::Requests(1),
        connections: 1,
        threads: 1,
        timeout: Duration::from_secs(1),
    };

    let error = run(config).unwrap_err();

    assert!(matches!(error, RunError::InvalidResponse(_)));
    assert!(error.to_string().contains("headers exceed 64 KiB"));
    server.join().unwrap();
}

#[test]
fn run_counts_large_body_without_changing_protocol_behavior() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request_head(&mut stream).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1048576\r\n\r\n")
            .unwrap();
        let block = [b'x'; 4096];
        for _ in 0..256 {
            stream.write_all(&block).unwrap();
        }
    });
    let config = RunConfig {
        url: format!("http://{address}/large"),
        method: Method::Get,
        limit: RunLimit::Requests(1),
        connections: 1,
        threads: 1,
        timeout: Duration::from_secs(2),
    };

    let summary = run(config).unwrap();

    assert_eq!(summary.response_body_bytes, 1_048_576);
    server.join().unwrap();
}

#[test]
fn run_decodes_chunked_response_split_across_every_byte() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request_head(&mut stream).unwrap();
        let response = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nOK\r\n0\r\n\r\n";
        for byte in response {
            stream.write_all(std::slice::from_ref(byte)).unwrap();
        }
    });
    let config = RunConfig {
        url: format!("http://{address}/chunked"),
        method: Method::Get,
        limit: RunLimit::Requests(1),
        connections: 1,
        threads: 1,
        timeout: Duration::from_secs(1),
    };

    let summary = run(config).unwrap();

    assert_eq!(summary.response_body_bytes, 2);
    server.join().unwrap();
}

#[test]
fn run_times_out_unresponsive_request_near_configured_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request_head(&mut stream).unwrap();
        thread::sleep(Duration::from_millis(100));
    });
    let config = RunConfig {
        url: format!("http://{address}/slow"),
        method: Method::Get,
        limit: RunLimit::Requests(1),
        connections: 1,
        threads: 1,
        timeout: Duration::from_millis(20),
    };
    let started = std::time::Instant::now();

    let error = run(config).unwrap_err();

    assert!(matches!(error, RunError::Io(_)));
    assert!(started.elapsed() >= Duration::from_millis(20));
    assert!(started.elapsed() < Duration::from_millis(80));
    server.join().unwrap();
}

#[test]
fn duration_run_recovers_after_connection_drops_mid_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut dropped, _) = listener.accept().unwrap();
        let mut buffer = [0; 1024];
        let _ = dropped.read(&mut buffer);
        drop(dropped);

        let (mut stream, _) = listener.accept().unwrap();
        while stream.read(&mut buffer).is_ok_and(|read| read > 0) {
            if stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .is_err()
            {
                break;
            }
        }
    });
    let config = RunConfig {
        url: format!("http://{address}/"),
        method: Method::Get,
        limit: RunLimit::Duration(Duration::from_millis(50)),
        connections: 1,
        threads: 1,
        timeout: Duration::from_millis(20),
    };

    let summary = run(config).unwrap();

    assert!(summary.completed > 0);
    assert_eq!(summary.socket_errors.total(), 1);
    assert_eq!(summary.socket_errors.read, 1);
    assert_eq!(summary.socket_errors.connect, 0);
    assert_eq!(summary.socket_errors.write, 0);
    assert_eq!(summary.socket_errors.timeout, 0);
    server.join().unwrap();
}

#[test]
fn duration_run_isolates_one_failed_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut handlers = Vec::new();
        for index in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            handlers.push(thread::spawn(move || {
                let mut buffer = [0; 1024];
                if index == 0 {
                    let _ = stream.read(&mut buffer);
                    return;
                }
                while stream.read(&mut buffer).is_ok_and(|read| read > 0) {
                    if stream
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        for handler in handlers {
            handler.join().unwrap();
        }
    });
    let config = RunConfig {
        url: format!("http://{address}/"),
        method: Method::Get,
        limit: RunLimit::Duration(Duration::from_millis(50)),
        connections: 2,
        threads: 1,
        timeout: Duration::from_millis(20),
    };

    let summary = run(config).unwrap();

    assert!(summary.completed > 1);
    assert_eq!(summary.socket_errors.total(), 1);
    assert_eq!(summary.socket_errors.read, 1);
    server.join().unwrap();
}

#[test]
fn duration_run_reports_persistent_timeouts_without_failing() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_millis(100);
        listener.set_nonblocking(true).unwrap();
        let mut streams = Vec::new();
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => streams.push(stream),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("accept failed: {error}"),
            }
        }
    });
    let config = RunConfig {
        url: format!("http://{address}/"),
        method: Method::Get,
        limit: RunLimit::Duration(Duration::from_millis(50)),
        connections: 1,
        threads: 1,
        timeout: Duration::from_millis(20),
    };
    let started = Instant::now();

    let summary = run(config).unwrap();

    assert_eq!(summary.completed, 0);
    assert!(summary.socket_errors.timeout >= 2);
    assert_eq!(summary.socket_errors.total(), summary.socket_errors.timeout);
    assert!(started.elapsed() < Duration::from_millis(100));
    server.join().unwrap();
}

#[test]
fn duration_run_reports_persistent_connection_refusals_without_failing() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let config = RunConfig {
        url: format!("http://{address}/"),
        method: Method::Get,
        limit: RunLimit::Duration(Duration::from_millis(40)),
        connections: 1,
        threads: 1,
        timeout: Duration::from_millis(10),
    };
    let started = Instant::now();

    let summary = run(config).unwrap();

    assert_eq!(summary.completed, 0);
    assert!(summary.socket_errors.connect > 0);
    assert_eq!(summary.socket_errors.total(), summary.socket_errors.connect);
    assert!(started.elapsed() < Duration::from_millis(100));
}

#[test]
fn duration_run_resumes_after_server_restart() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            first.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        first
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        drop(first);
        drop(listener);

        thread::sleep(Duration::from_millis(20));
        let listener = TcpListener::bind(address).unwrap();
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_millis(100);
        let mut handlers = Vec::new();
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => handlers.push(thread::spawn(move || {
                    let mut buffer = [0; 1024];
                    while stream.read(&mut buffer).is_ok_and(|read| read > 0) {
                        if stream
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                            .is_err()
                        {
                            break;
                        }
                    }
                })),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("accept failed: {error}"),
            }
        }
        for handler in handlers {
            handler.join().unwrap();
        }
    });
    let config = RunConfig {
        url: format!("http://{address}/"),
        method: Method::Get,
        limit: RunLimit::Duration(Duration::from_millis(80)),
        connections: 1,
        threads: 1,
        timeout: Duration::from_millis(10),
    };

    let summary = run(config).unwrap();

    assert!(summary.completed > 1);
    assert!(summary.socket_errors.total() > 0);
    assert!(summary.socket_errors.connect > 0 || summary.socket_errors.read > 0);
    server.join().unwrap();
}

#[test]
fn run_rejects_oversized_chunk_extensions_and_trailers() {
    let oversized = "a".repeat(65 * 1024);
    for body in [
        format!("1;{oversized}\r\nx\r\n0\r\n\r\n"),
        format!("1\r\nx\r\n0\r\nX-Oversized: {oversized}\r\n\r\n"),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request_head(&mut stream).unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
                .unwrap();
            stream.write_all(body.as_bytes()).unwrap();
        });
        let config = RunConfig {
            url: format!("http://{address}/chunked"),
            method: Method::Get,
            limit: RunLimit::Requests(1),
            connections: 1,
            threads: 1,
            timeout: Duration::from_secs(1),
        };

        let error = run(config).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("chunk control data exceed 64 KiB")
        );
        server.join().unwrap();
    }
}
