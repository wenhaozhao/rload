use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use std::{env, fs};

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
