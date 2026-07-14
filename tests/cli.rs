use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use std::{env, fs};

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n")
}

#[test]
fn line_ending_normalization_accepts_windows_fixtures() {
    assert_eq!(normalize_line_endings("one\r\ntwo\r\n"), "one\ntwo\n");
}

#[test]
fn cli_runs_http_load_and_prints_summary() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK")
            .unwrap();
    });

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args(["--requests", "1", &format!("http://{address}/health")])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("1 requests completed"));
    assert!(stdout.contains("2 response body B read"));
    assert!(stdout.contains("Requests/sec:"));
    assert!(stdout.contains("Load requests/sec:"));
    assert!(stdout.contains("Drain time:"));
    assert!(stdout.contains("Latency Distribution"));
    assert!(stdout.contains("99%"));
}

#[test]
fn cli_outputs_machine_readable_json_summary() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        stream
            .write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\nOK")
            .unwrap();
    });

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "1",
            "--output-format",
            "json",
            &format!("http://{address}/json"),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["schema_version"], 1);
    assert_eq!(result["summary"]["completed_requests"], 1);
    assert_eq!(result["summary"]["response_body_bytes"], 2);
    assert!(result["summary"]["read_bytes"].as_u64().unwrap() > 2);
    assert_eq!(result["methods"]["GET"]["requests"], 1);
    assert_eq!(result["http_statuses"]["201"], 1);
    assert!(result["latency"]["p99_us"].as_u64().is_some());
    assert_eq!(result["replay"]["configured_rate"], serde_json::Value::Null);
    server.join().unwrap();
}

#[test]
fn cli_prints_opt_in_beauty_output_without_changing_default_mode() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK")
            .unwrap();
    });

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "1",
            "--connections",
            "1",
            "--output-beauty",
            &format!("http://{address}/beauty"),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "rload result",
        "Summary",
        "Bytes read",
        "Response body bytes",
        "Throughput",
        "Latency",
        "Errors",
        "Breakdowns",
        "GET",
        "/beauty",
    ] {
        assert!(
            stdout.contains(expected),
            "missing {expected:?} in:\n{stdout}"
        );
    }
    let normalized = stdout
        .lines()
        .map(|line| {
            line.split_whitespace()
                .map(|token| {
                    if token.bytes().any(|byte| byte.is_ascii_digit()) {
                        "<value>"
                    } else {
                        token
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let expected = normalize_line_endings(include_str!("fixtures/beauty-output.txt"));
    assert_eq!(format!("{normalized}\n"), expected);
    assert!(!stdout.contains("1 requests completed"));
    server.join().unwrap();
}

#[test]
fn cli_rejects_beauty_with_json_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--output-beauty",
            "--output-format",
            "json",
            "http://127.0.0.1:1/",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("--output-beauty cannot be used with --output-format json")
    );
}

#[test]
fn cli_rejects_unknown_output_format() {
    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args(["--output-format", "xml", "http://127.0.0.1:1/"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("expected text or json"));
}

#[test]
fn cli_accepts_attached_request_count() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args(["-n1", &format!("http://{address}/")])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("1 requests completed")
    );
    server.join().unwrap();
}

#[test]
fn cli_replays_access_log_requests_in_order() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut paths = Vec::new();
        for index in 0..5 {
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            paths.push(String::from_utf8(request).unwrap());
            let response = if index % 2 == 0 {
                &b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n"[..]
            } else {
                &b"HTTP/1.1 200 OK\r\nContent-Length: 123\r\n\r\n"[..]
            };
            stream.write_all(response).unwrap();
        }
        paths
    });
    let path = env::temp_dir().join(format!("r-wrk-access-{}.log", std::process::id()));
    fs::write(
        &path,
        "127.0.0.1 - - [10/Oct/2000:13:55:36 -0700] \"GET /first?x=1 HTTP/1.1\" 200 10 \"-\" \"agent\"\n\
         127.0.0.1 - - [10/Oct/2000:13:55:37 -0700] \"HEAD /second HTTP/1.1\" 200 0 \"-\" \"agent\"\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "5",
            "--connections",
            "1",
            "--access-log",
            path.to_str().unwrap(),
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let requests = server.join().unwrap();
    assert!(requests[0].starts_with("GET /first?x=1 HTTP/1.1\r\n"));
    assert!(requests[1].starts_with("HEAD /second HTTP/1.1\r\n"));
    assert!(requests[2].starts_with("GET /first?x=1 HTTP/1.1\r\n"));
    assert!(requests[3].starts_with("HEAD /second HTTP/1.1\r\n"));
    assert!(requests[4].starts_with("GET /first?x=1 HTTP/1.1\r\n"));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("GET          3"));
    assert!(stdout.contains("HEAD         2"));
    assert!(stdout.contains("200          5"));
    assert!(stdout.contains("estimated        3, maximum overcount 0        /first?x=1"));
    assert!(stdout.contains("estimated        2, maximum overcount 0        /second"));
}

#[test]
fn cli_replays_access_log_for_configured_rounds() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut lines = Vec::new();
        for _ in 0..4 {
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            lines.push(
                String::from_utf8(request)
                    .unwrap()
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned(),
            );
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        }
        lines
    });
    let path = env::temp_dir().join(format!("rload-rounds-{}.log", std::process::id()));
    fs::write(
        &path,
        "127.0.0.1 - - [date] \"GET /one HTTP/1.1\" 200 0\n\
         127.0.0.1 - - [date] \"HEAD /two HTTP/1.1\" 200 0\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--connections",
            "1",
            "--access-log",
            path.to_str().unwrap(),
            "--replay-rounds",
            "2",
            "--replay-stages",
            "1ms:10000",
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        server.join().unwrap(),
        [
            "GET /one HTTP/1.1",
            "HEAD /two HTTP/1.1",
            "GET /one HTTP/1.1",
            "HEAD /two HTTP/1.1",
        ]
    );
}

#[test]
fn cli_skips_unsupported_access_log_methods_and_reports_them() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        request
    });
    let path = env::temp_dir().join(format!("r-wrk-invalid-access-{}.log", std::process::id()));
    fs::write(
        &path,
        "127.0.0.1 - - [date] \"POST /items HTTP/1.1\" 200 0\n\
         127.0.0.1 - - [date] \"GET /health HTTP/1.1\" 200 0\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--access-log",
            path.to_str().unwrap(),
            "--requests",
            "1",
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(server.join().unwrap())
            .unwrap()
            .starts_with("GET /health HTTP/1.1\r\n")
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Access-log entries skipped: 1"));
    assert!(stdout.contains("POST         1 unsupported method"));
}

#[test]
fn cli_rejects_invalid_replay_options() {
    for arguments in [
        vec!["--replay-order", "unknown", "http://localhost/"],
        vec!["--replay-order", "shuffle", "http://localhost/"],
        vec!["--seed", "not-a-number", "http://localhost/"],
        vec!["--allowed-methods", "GET", "http://localhost/"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rload"))
            .args(arguments)
            .output()
            .unwrap();

        assert!(!output.status.success());
    }
}

#[test]
fn cli_accepts_seeded_random_replay() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });
    let path = env::temp_dir().join(format!("r-wrk-random-access-{}.log", std::process::id()));
    fs::write(
        &path,
        "127.0.0.1 - - [date] \"GET /random HTTP/1.1\" 200 0\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "1",
            "--access-log",
            path.to_str().unwrap(),
            "--replay-order",
            "random",
            "--seed",
            "42",
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    server.join().unwrap();
}

#[test]
fn cli_limits_replay_to_global_request_rate() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        for _ in 0..3 {
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        }
    });
    let path = env::temp_dir().join(format!("rload-rate-{}.log", std::process::id()));
    fs::write(&path, "127.0.0.1 - - [date] \"GET /rate HTTP/1.1\" 200 0\n").unwrap();

    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "3",
            "--connections",
            "1",
            "--access-log",
            path.to_str().unwrap(),
            "--replay-rate",
            "5",
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    let elapsed = started.elapsed();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed >= Duration::from_millis(350),
        "elapsed: {elapsed:?}"
    );
    assert!(elapsed < Duration::from_secs(5), "elapsed: {elapsed:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Configured replay rate: 5 requests/sec"));
    assert!(stdout.contains("Measured replay rate:"));
    server.join().unwrap();
}

#[test]
fn replay_pacing_does_not_block_duration_shutdown() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let path = env::temp_dir().join(format!("rload-nonblocking-rate-{}.log", std::process::id()));
    fs::write(&path, "127.0.0.1 - - [date] \"GET /rate HTTP/1.1\" 200 0\n").unwrap();
    let started = Instant::now();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--duration",
            "50ms",
            "--timeout",
            "20ms",
            "--connections",
            "10",
            "--access-log",
            path.to_str().unwrap(),
            "--replay-rate",
            "1",
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    let elapsed = started.elapsed();
    fs::remove_file(path).unwrap();
    drop(listener);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(elapsed < Duration::from_secs(1), "elapsed: {elapsed:?}");
}

#[test]
fn cli_replays_access_log_with_scaled_timestamp_gaps() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut arrivals = Vec::new();
        for _ in 0..2 {
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            arrivals.push(Instant::now());
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        }
        arrivals
    });
    let path = env::temp_dir().join(format!("rload-timestamp-{}.log", std::process::id()));
    fs::write(
        &path,
        "127.0.0.1 - - [10/Oct/2000:13:55:36.000 -0700] \"GET /one HTTP/1.1\" 200 0\n\
         127.0.0.1 - - [10/Oct/2000:13:55:36.200 -0700] \"GET /two HTTP/1.1\" 200 0\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "2",
            "--connections",
            "1",
            "--access-log",
            path.to_str().unwrap(),
            "--replay-timestamps",
            "--replay-speed",
            "2",
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let arrivals = server.join().unwrap();
    let gap = arrivals[1].duration_since(arrivals[0]);
    assert!(gap >= Duration::from_millis(80), "gap: {gap:?}");
    assert!(gap < Duration::from_millis(500), "gap: {gap:?}");
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("Timestamp replay speed: 2.000x")
    );
}

#[test]
fn cli_replays_jsonl_with_schema_timestamp_gaps() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut arrivals = Vec::new();
        for _ in 0..2 {
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            arrivals.push(Instant::now());
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        }
        arrivals
    });
    let suffix = std::process::id();
    let path = env::temp_dir().join(format!("rload-timestamp-{suffix}.jsonl"));
    let schema = env::temp_dir().join(format!("rload-timestamp-schema-{suffix}.yaml"));
    fs::write(
        &path,
        "{\"uri\":\"/one\",\"event\":{\"at\":\"2026-07-03T08:41:17.000Z\"}}\n\
         {\"uri\":\"/two\",\"event\":{\"at\":\"2026-07-03T08:41:17.200Z\"}}\n",
    )
    .unwrap();
    fs::write(
        &schema,
        "schema_version: 1\nfields:\n  timestamp:\n    path: event.at\n    format: '%+'\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--connections",
            "1",
            "--request-file",
            path.to_str().unwrap(),
            "--request-schema",
            schema.to_str().unwrap(),
            "--replay-rounds",
            "1",
            "--replay-timestamps",
            "--replay-speed",
            "2",
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    fs::remove_file(schema).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let arrivals = server.join().unwrap();
    let gap = arrivals[1].duration_since(arrivals[0]);
    assert!(gap >= Duration::from_millis(80), "gap: {gap:?}");
    assert!(gap < Duration::from_millis(500), "gap: {gap:?}");
}

#[test]
fn cli_replays_jsonl_default_timestamps_without_schema() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut arrivals = Vec::new();
        for _ in 0..2 {
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            arrivals.push(Instant::now());
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        }
        arrivals
    });
    let path = env::temp_dir().join(format!(
        "rload-default-timestamp-{}.jsonl",
        std::process::id()
    ));
    fs::write(
        &path,
        "{\"uri\":\"/one\",\"time\":\"2026-07-03T08:41:17.000Z\"}\n\
         {\"uri\":\"/two\",\"time\":\"2026-07-03T08:41:17.200Z\"}\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--connections",
            "1",
            "--request-file",
            path.to_str().unwrap(),
            "--requests",
            "2",
            "--replay-timestamps",
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let arrivals = server.join().unwrap();
    let gap = arrivals[1].duration_since(arrivals[0]);
    assert!(gap >= Duration::from_millis(180), "gap: {gap:?}");
    assert!(gap < Duration::from_millis(700), "gap: {gap:?}");
}

#[test]
fn cli_rejects_invalid_timestamp_replay_combinations() {
    let path = env::temp_dir().join(format!(
        "rload-timestamp-invalid-{}.log",
        std::process::id()
    ));
    fs::write(&path, "127.0.0.1 - - [date] \"GET / HTTP/1.1\" 200 0\n").unwrap();
    for arguments in [
        vec!["--replay-timestamps", "--replay-rate", "1"],
        vec!["--replay-timestamps", "--replay-order", "random"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rload"))
            .args(arguments)
            .args([
                "--access-log",
                path.to_str().unwrap(),
                "http://127.0.0.1:1/",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "1",
            "--access-log",
            path.to_str().unwrap(),
            "--replay-timestamps",
            "http://127.0.0.1:1/",
        ])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("valid timestamp"));

    let request_file = env::temp_dir().join(format!(
        "rload-timestamp-invalid-{}.jsonl",
        std::process::id()
    ));
    fs::write(
        &request_file,
        "{\"uri\":\"/\",\"time\":\"2026-07-03T08:41:17Z\"}\n",
    )
    .unwrap();
    for arguments in [
        vec!["--replay-timestamps", "--replay-rate", "1"],
        vec!["--replay-rate", "1", "--replay-timestamps"],
        vec!["--replay-timestamps", "--replay-order", "shuffle"],
        vec!["--replay-order", "shuffle", "--replay-timestamps"],
        vec!["--replay-timestamps", "--replay-stages", "1s:10"],
        vec!["--replay-stages", "1s:10", "--replay-timestamps"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rload"))
            .args(arguments)
            .args([
                "--request-file",
                request_file.to_str().unwrap(),
                "http://127.0.0.1:1/",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
    fs::remove_file(request_file).unwrap();
}

#[test]
fn cli_applies_replay_rate_stages() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut arrivals = Vec::new();
        for _ in 0..5 {
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            arrivals.push(Instant::now());
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        }
        arrivals
    });
    let path = env::temp_dir().join(format!("rload-stages-{}.log", std::process::id()));
    fs::write(&path, "127.0.0.1 - - [date] \"GET / HTTP/1.1\" 200 0\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "5",
            "--connections",
            "1",
            "--access-log",
            path.to_str().unwrap(),
            "--replay-stages",
            "200ms:5,200ms:20",
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let arrivals = server.join().unwrap();
    let baseline_gap = arrivals[1].duration_since(arrivals[0]);
    let burst_gap = arrivals[2].duration_since(arrivals[1]);
    assert!(
        baseline_gap >= Duration::from_millis(150),
        "baseline gap: {baseline_gap:?}"
    );
    assert!(
        burst_gap < Duration::from_millis(150),
        "burst gap: {burst_gap:?}"
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Replay stages:"));
}

#[test]
fn cli_rejects_invalid_replay_stages() {
    for arguments in [
        vec!["--replay-stages", "1s:0"],
        vec!["--replay-stages", "invalid"],
        vec!["--replay-stages", "1s:10", "--replay-rate", "10"],
        vec!["--replay-stages", "1s:10", "--replay-timestamps"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rload"))
            .args(arguments)
            .args([
                "--access-log",
                "/does/not/matter.log",
                "http://127.0.0.1:1/",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
}

#[test]
fn cli_replays_jsonl_post_with_headers_and_body() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        let mut body = [0; 7];
        stream.read_exact(&mut body).unwrap();
        stream
            .write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        (String::from_utf8(request).unwrap(), body)
    });
    let path = env::temp_dir().join(format!("r-wrk-request-{}.jsonl", std::process::id()));
    fs::write(
        &path,
        r#"{"method":"POST","uri":"/items?source=test","headers":{"content-type":"application/json","x-test":"yes"},"body":"{\"a\":1}"}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "1",
            "--request-file",
            path.to_str().unwrap(),
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (head, body) = server.join().unwrap();
    assert!(head.starts_with("POST /items?source=test HTTP/1.1\r\n"));
    assert!(head.contains("content-type: application/json\r\n"));
    assert!(head.contains("x-test: yes\r\n"));
    assert!(head.contains("Content-Length: 7\r\n"));
    assert_eq!(&body, br#"{"a":1}"#);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("POST         1"));
    assert!(stdout.contains("201          1"));
}

#[test]
fn cli_extracts_nested_jsonl_fields_with_partial_schema_fallback() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        String::from_utf8(request).unwrap()
    });
    let suffix = std::process::id();
    let request_path = env::temp_dir().join(format!("rload-schema-request-{suffix}.jsonl"));
    let schema_path = env::temp_dir().join(format!("rload-schema-{suffix}.yaml"));
    fs::write(
        &request_path,
        r#"{"method":"POST","request":{"path":"/nested"},"args":"a=1"}
"#,
    )
    .unwrap();
    fs::write(
        &schema_path,
        "schema_version: 1\nfields:\n  uri:\n    path: request.path\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "1",
            "--request-file",
            request_path.to_str().unwrap(),
            "--request-schema",
            schema_path.to_str().unwrap(),
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    fs::remove_file(request_path).unwrap();
    fs::remove_file(schema_path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        server
            .join()
            .unwrap()
            .starts_with("POST /nested?a=1 HTTP/1.1\r\n")
    );
}

#[test]
fn cli_replays_the_filtered_sequence_for_configured_rounds() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request_lines = Vec::new();
        for _ in 0..4 {
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            request_lines.push(
                String::from_utf8(request)
                    .unwrap()
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned(),
            );
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        }
        request_lines
    });
    let path = env::temp_dir().join(format!("rload-rounds-{}.jsonl", std::process::id()));
    fs::write(
        &path,
        "{\"uri\":\"/one\"}\n{\"uri\":\"/filtered\"}\n{\"uri\":\"/two\"}\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--connections",
            "1",
            "--request-file",
            path.to_str().unwrap(),
            "--replay-rounds",
            "2",
            "--replay-order",
            "shuffle",
            "--seed",
            "42",
            "--allowed-uris",
            "/one,/two",
            "--replay-rate",
            "10000",
            "--output-format",
            "json",
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request_lines = server.join().unwrap();
    for round in request_lines.chunks_exact(2) {
        let mut round = round.to_vec();
        round.sort();
        assert_eq!(round, ["GET /one HTTP/1.1", "GET /two HTTP/1.1"]);
    }
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["summary"]["completed_requests"], 4);
    assert_eq!(result["replay"]["configured_rounds"], 2);
    assert_eq!(result["replay"]["completed_rounds"], 2);
}

#[test]
fn cli_rejects_multiple_replay_inputs() {
    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--access-log",
            "access.log",
            "--request-file",
            "requests.jsonl",
            "http://localhost/",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("cannot be used together")
    );
}

#[test]
fn cli_rejects_invalid_schema_and_round_combinations() {
    for arguments in [
        vec!["--request-schema", "schema.yaml", "http://localhost/"],
        vec![
            "--request-file",
            "requests.jsonl",
            "--replay-rounds",
            "2",
            "--replay-order",
            "random",
            "http://localhost/",
        ],
        vec![
            "--request-file",
            "requests.jsonl",
            "--replay-rounds",
            "2",
            "--requests",
            "10",
            "http://localhost/",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rload"))
            .args(arguments)
            .output()
            .unwrap();

        assert!(!output.status.success());
    }
}

#[test]
fn cli_uses_curl_style_headers_and_joined_data() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            head.push(byte[0]);
        }
        let mut body = [0; 7];
        stream.read_exact(&mut body).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        (String::from_utf8(head).unwrap(), body)
    });

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "1",
            "-XPOST",
            "-Hx-test: yes",
            "--data",
            "a=1",
            "--data",
            "b=2",
            &format!("http://{address}/submit"),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (head, body) = server.join().unwrap();
    assert!(head.starts_with("POST /submit HTTP/1.1\r\n"));
    assert!(head.contains("x-test: yes\r\n"));
    assert!(head.contains("Content-Length: 7\r\n"));
    assert_eq!(&body, b"a=1&b=2");
}

#[test]
fn cli_sends_data_binary_without_text_conversion() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            head.push(byte[0]);
        }
        let mut body = [0; 4];
        stream.read_exact(&mut body).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        (String::from_utf8(head).unwrap(), body)
    });
    let path = env::temp_dir().join(format!("r-wrk-body-{}.bin", std::process::id()));
    fs::write(&path, [0, 1, 13, 255]).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "1",
            "-X",
            "PUT",
            "--data-binary",
            &format!("@{}", path.display()),
            &format!("http://{address}/binary"),
        ])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(output.status.success());
    let (head, body) = server.join().unwrap();
    assert!(head.starts_with("PUT /binary HTTP/1.1\r\n"));
    assert!(head.contains("Content-Length: 4\r\n"));
    assert_eq!(body, [0, 1, 13, 255]);
}

#[test]
fn cli_rejects_explicit_empty_body_for_get_and_head() {
    let path = env::temp_dir().join(format!("r-wrk-empty-body-{}.bin", std::process::id()));
    fs::write(&path, []).unwrap();
    let cases = [
        vec!["-X".into(), "GET".into(), "--data".into(), String::new()],
        vec![
            "-X".into(),
            "HEAD".into(),
            "--data-binary".into(),
            format!("@{}", path.display()),
        ],
    ];

    for mut arguments in cases {
        arguments.push("http://localhost/".into());
        let output = Command::new(env!("CARGO_BIN_EXE_rload"))
            .args(arguments)
            .output()
            .unwrap();

        assert!(!output.status.success());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("must not contain a body")
        );
    }
    fs::remove_file(path).unwrap();
}

#[test]
fn cli_filters_jsonl_replay_by_method_and_uri_glob() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        String::from_utf8(request).unwrap()
    });
    let path = env::temp_dir().join(format!("r-wrk-filter-{}.jsonl", std::process::id()));
    fs::write(
        &path,
        "{\"method\":\"GET\",\"uri\":\"/public\"}\n\
         {\"method\":\"POST\",\"uri\":\"/api/items\"}\n\
         {\"method\":\"POST\",\"uri\":\"/admin\"}\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "1",
            "--request-file",
            path.to_str().unwrap(),
            "--allowed-methods",
            "POST",
            "--allowed-uris",
            "/api/*",
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        server
            .join()
            .unwrap()
            .starts_with("POST /api/items HTTP/1.1\r\n")
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("Replay entries filtered by whitelist: 2")
    );
}

#[test]
fn cli_accepts_wrk_timeout_and_latency_options() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_millis(100));
    });
    let started = Instant::now();

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "--requests",
            "1",
            "-T",
            "20ms",
            "--latency",
            &format!("http://{address}/"),
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("timed out")
    );
    server.join().unwrap();
}

#[test]
fn cli_reports_connection_errors_and_succeeds_after_duration() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args(["-d20ms", "-T5ms", &format!("http://{address}/")])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("0 requests completed"));
    assert!(stdout.contains("Socket errors: connect "));
    assert!(stdout.contains(", read 0, write 0, timeout 0"));
}

#[test]
fn cli_returns_failure_for_invalid_http_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        stream.write_all(b"not HTTP\r\n\r\n").unwrap();
    });

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args(["-n1", &format!("http://{address}/")])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("status line is invalid")
    );
}

#[test]
fn cli_writes_configuration_failures_only_to_stderr() {
    for (arguments, expected) in [
        (vec!["-c1"], "a target URL is required"),
        (vec!["--unknown", "http://localhost/"], "unknown option"),
        (
            vec!["-n1", "-d1s", "http://localhost/"],
            "--duration and --requests cannot be used together",
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rload"))
            .args(arguments)
            .output()
            .unwrap();

        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8(output.stderr).unwrap().contains(expected),
            "missing {expected:?}"
        );
    }
}

#[test]
fn cli_help_reports_wrk_compatible_defaults() {
    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Run duration [default: 10s]"));
    assert!(stdout.contains("Concurrent connections [default: 10]"));
    assert!(stdout.contains("Worker threads [default: 2]"));
}

#[test]
fn cli_rejects_lua_script_option() {
    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args(["-s", "script.lua", "http://localhost/"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Lua scripting is not supported"));
}

#[test]
fn cli_rejects_attached_lua_script_options() {
    for argument in ["--script=script.lua", "-sscript.lua"] {
        let output = Command::new(env!("CARGO_BIN_EXE_rload"))
            .args([argument, "http://localhost/"])
            .output()
            .unwrap();

        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("Lua scripting is not supported"));
    }
}

#[test]
fn cli_rejects_request_count_with_duration() {
    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args(["--requests", "10", "--duration", "1s", "http://localhost/"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("--duration and --requests cannot be used together"));
}

#[test]
fn cli_runs_with_connections_and_millisecond_duration() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut handlers = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            handlers.push(thread::spawn(move || {
                loop {
                    let mut request = Vec::new();
                    let mut byte = [0];
                    while !request.ends_with(b"\r\n\r\n") {
                        if stream.read_exact(&mut byte).is_err() {
                            return;
                        }
                        request.push(byte[0]);
                    }
                    if stream
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                        .is_err()
                    {
                        return;
                    }
                }
            }));
        }
        for handler in handlers {
            handler.join().unwrap();
        }
    });

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args([
            "-c2",
            "-t1",
            "-d20ms",
            "-T2s",
            "--latency",
            &format!("http://{address}/health"),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("requests completed"));
    server.join().unwrap();
}

#[test]
fn cli_rejects_zero_connections_and_duration() {
    for arguments in [
        ["--connections", "0", "http://localhost/"],
        ["--duration", "0s", "http://localhost/"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rload"))
            .args(arguments)
            .output()
            .unwrap();

        assert!(!output.status.success());
    }

    let output = Command::new(env!("CARGO_BIN_EXE_rload"))
        .args(["--threads", "0", "http://localhost/"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}
