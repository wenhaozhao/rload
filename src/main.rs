use std::env;
use std::fs;
use std::io::Read;
use std::process::ExitCode;
use std::time::Duration;

use rload::{
    MAX_REQUEST_BODY_BYTES, Method, ReplayFilter, ReplayOptions, ReplayOrder, ReplayRunOptions,
    ReplayStage, RequestFileReplayOptions, RequestOptions, RunConfig, RunLimit, RunSummary, run,
    run_access_log_with_run_options, run_request_file_with_run_options, run_with_request,
    run_with_request_and_stages, run_with_stages,
};

#[derive(Clone, Copy, Eq, PartialEq)]
enum OutputFormat {
    Text,
    Beauty,
    Json,
}

fn main() -> ExitCode {
    match execute(env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn execute(args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut profile_path = None;
    let mut assertions = Vec::new();
    let mut requests = 1;
    let mut requests_were_set = false;
    let mut duration = None;
    let mut connections = 10;
    let mut connections_were_set = false;
    let mut threads = 2;
    let mut threads_were_set = false;
    let mut timeout = Duration::from_secs(2);
    let mut timeout_was_set = false;
    let mut url = None;
    let mut access_log = None;
    let mut request_file = None;
    let mut request_schema = None;
    let mut skip_invalid_records = false;
    let mut replay_order = ReplayOrder::Sequential;
    let mut replay_option_was_set = false;
    let mut seed = None;
    let mut replay_rate = None;
    let mut replay_timestamps = false;
    let mut replay_speed = 1.0;
    let mut replay_speed_was_set = false;
    let mut stages = Vec::new();
    let mut stages_were_set = false;
    let mut replay_stages_were_set = false;
    let mut replay_rounds = None;
    let mut output_format = OutputFormat::Text;
    let mut output_format_was_set = false;
    let mut output_beauty = false;
    let mut output_html = None;
    let mut version = false;
    let mut method = Method::Get;
    let mut method_was_set = false;
    let mut headers = Vec::new();
    let mut data = Vec::new();
    let mut data_binary = None;
    let mut allowed_methods = Vec::new();
    let mut allowed_uris = Vec::new();
    let mut args = args.peekable();

    while let Some(argument) = args.next() {
        let (argument, attached_value) = split_attached(argument);
        match argument.as_str() {
            "--profile" => {
                profile_path = Some(
                    args.next()
                        .ok_or_else(|| "--profile requires a file path".to_owned())?,
                )
            }
            "--assert" => assertions.push(
                args.next()
                    .ok_or_else(|| "--assert requires an expression".to_owned())?,
            ),
            "-s" | "--script" => {
                return Err("Lua scripting is not supported\n\nUse an access log or JSONL request file for dynamic request sequences.".into());
            }
            value if value.starts_with("--script=") || value.starts_with("-s") => {
                return Err("Lua scripting is not supported\n\nUse an access log or JSONL request file for dynamic request sequences.".into());
            }
            "-n" | "--requests" => {
                let value = attached_value
                    .or_else(|| args.next())
                    .ok_or_else(|| "--requests requires a value".to_owned())?;
                requests = value
                    .parse::<u64>()
                    .map_err(|_| format!("invalid request count: {value}"))?;
                if requests == 0 {
                    return Err("request count must be greater than zero".into());
                }
                requests_were_set = true;
            }
            "--access-log" => {
                access_log = Some(
                    args.next()
                        .ok_or_else(|| "--access-log requires a file path".to_owned())?,
                );
            }
            "--request-file" => {
                request_file = Some(
                    args.next()
                        .ok_or_else(|| "--request-file requires a file path".to_owned())?,
                );
            }
            "--request-schema" => {
                request_schema = Some(
                    args.next()
                        .ok_or_else(|| "--request-schema requires a file path".to_owned())?,
                );
            }
            "--skip-invalid-records" => skip_invalid_records = true,
            "--replay-order" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--replay-order requires a value".to_owned())?;
                replay_order = match value.as_str() {
                    "sequential" => ReplayOrder::Sequential,
                    "shuffle" => ReplayOrder::Shuffle,
                    "random" => ReplayOrder::Random,
                    _ => {
                        return Err(format!(
                            "invalid replay order: {value}; expected sequential, shuffle, or random"
                        ));
                    }
                };
                replay_option_was_set = true;
            }
            "--seed" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--seed requires a value".to_owned())?;
                seed = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| format!("invalid replay seed: {value}"))?,
                );
                replay_option_was_set = true;
            }
            "--replay-rate" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--replay-rate requires a value".to_owned())?;
                let rate = value
                    .parse::<u64>()
                    .map_err(|_| format!("invalid replay rate: {value}"))?;
                if rate == 0 {
                    return Err("replay rate must be greater than zero".into());
                }
                replay_rate = Some(rate);
            }
            "--replay-timestamps" => replay_timestamps = true,
            "--replay-speed" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--replay-speed requires a value".to_owned())?;
                replay_speed = value
                    .parse::<f64>()
                    .map_err(|_| format!("invalid replay speed: {value}"))?;
                if !replay_speed.is_finite() || replay_speed <= 0.0 {
                    return Err("replay speed must be a finite number greater than zero".into());
                }
                replay_speed_was_set = true;
            }
            "--stages" | "--replay-stages" => {
                let value = args
                    .next()
                    .ok_or_else(|| format!("{argument} requires a value"))?;
                stages = parse_stages(&value, &argument)?;
                if argument == "--stages" {
                    stages_were_set = true;
                } else {
                    replay_stages_were_set = true;
                }
            }
            "--replay-rounds" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--replay-rounds requires a value".to_owned())?;
                let rounds = value
                    .parse::<u64>()
                    .map_err(|_| format!("invalid replay round count: {value}"))?;
                if rounds == 0 {
                    return Err("replay round count must be greater than zero".into());
                }
                replay_rounds = Some(rounds);
            }
            "--output-format" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--output-format requires a value".to_owned())?;
                output_format = match value.as_str() {
                    "text" => OutputFormat::Text,
                    "json" => OutputFormat::Json,
                    _ => {
                        return Err(format!(
                            "invalid output format: {value}; expected text or json"
                        ));
                    }
                };
                output_format_was_set = true;
            }
            "--output-beauty" => output_beauty = true,
            "--output-html" => {
                output_html = Some(
                    args.next()
                        .ok_or_else(|| "--output-html requires a file path".to_owned())?,
                )
            }
            "-X" | "--request" => {
                let value = attached_value
                    .or_else(|| args.next())
                    .ok_or_else(|| "--request requires a method".to_owned())?;
                method = parse_method(&value)?;
                method_was_set = true;
            }
            "-H" | "--header" => {
                let value = attached_value
                    .or_else(|| args.next())
                    .ok_or_else(|| "--header requires a value".to_owned())?;
                let (name, value) = value
                    .split_once(':')
                    .ok_or_else(|| "header must use 'Name: Value' syntax".to_owned())?;
                headers.push((name.trim().to_owned(), value.trim().to_owned()));
            }
            "--data" => {
                data.push(
                    args.next()
                        .ok_or_else(|| "--data requires a value".to_owned())?,
                );
            }
            "--data-binary" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--data-binary requires @FILE".to_owned())?;
                let path = value
                    .strip_prefix('@')
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| "--data-binary requires @FILE".to_owned())?;
                if data_binary.replace(path.to_owned()).is_some() {
                    return Err("--data-binary may only be supplied once".into());
                }
            }
            "--allowed-methods" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--allowed-methods requires a value".to_owned())?;
                for value in comma_separated(&value, "--allowed-methods")? {
                    let method = parse_method(value)?;
                    if !allowed_methods.contains(&method) {
                        allowed_methods.push(method);
                    }
                }
            }
            "--allowed-uris" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--allowed-uris requires a value".to_owned())?;
                allowed_uris.extend(
                    comma_separated(&value, "--allowed-uris")?
                        .into_iter()
                        .map(str::to_owned),
                );
            }
            "-d" | "--duration" => {
                let value = attached_value
                    .or_else(|| args.next())
                    .ok_or_else(|| "--duration requires a value".to_owned())?;
                duration = Some(parse_duration(&value)?);
            }
            "-T" | "--timeout" => {
                let value = attached_value
                    .or_else(|| args.next())
                    .ok_or_else(|| "--timeout requires a value".to_owned())?;
                timeout = parse_duration(&value)?;
                timeout_was_set = true;
            }
            "--latency" => {}
            "-c" | "--connections" => {
                let value = attached_value
                    .or_else(|| args.next())
                    .ok_or_else(|| "--connections requires a value".to_owned())?;
                connections = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid connection count: {value}"))?;
                if connections == 0 {
                    return Err("connection count must be greater than zero".into());
                }
                connections_were_set = true;
            }
            "-t" | "--threads" => {
                let value = attached_value
                    .or_else(|| args.next())
                    .ok_or_else(|| "--threads requires a value".to_owned())?;
                threads = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid thread count: {value}"))?;
                if threads == 0 {
                    return Err("thread count must be greater than zero".into());
                }
                threads_were_set = true;
            }
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "--version" => version = true,
            value if value.starts_with('-') => {
                return Err(format!("unknown option: {value}"));
            }
            value => {
                if url.replace(value.to_owned()).is_some() {
                    return Err("only one URL may be supplied".into());
                }
            }
        }
    }

    let mut ordinary_request_was_set =
        method_was_set || !headers.is_empty() || !data.is_empty() || data_binary.is_some();
    if let Some(path) = profile_path {
        let profile = rload::profile::load(path)?;
        assertions.extend(
            profile
                .assertions
                .into_iter()
                .map(|assertion| assertion.expression),
        );
        if url.is_none() {
            url = Some(profile.target.url);
        }
        if !requests_were_set
            && duration.is_none()
            && let Some(value) = profile.runner.duration
        {
            duration = Some(parse_duration(&value)?);
        }
        if !connections_were_set {
            connections = profile.runner.connections;
        }
        if !threads_were_set {
            threads = profile.runner.threads;
        }
        if !timeout_was_set {
            timeout = parse_duration(&profile.runner.timeout)?;
        }
        if !output_format_was_set && let Some(value) = profile.observability.output_format {
            output_format = match value.as_str() {
                "text" => OutputFormat::Text,
                "json" => OutputFormat::Json,
                _ => {
                    return Err(format!(
                        "profile observability.output_format must be text or json, got {value}"
                    ));
                }
            };
        }
        if output_html.is_none() {
            output_html = profile.observability.output_html;
        }
        if access_log.is_none()
            && request_file.is_none()
            && !ordinary_request_was_set
            && let Some(request) = profile.load_profile.static_request
        {
            method = parse_method(&request.method)?;
            method_was_set = true;
            headers = request.headers.into_iter().collect();
            data = request.body.into_iter().collect();
            ordinary_request_was_set = true;
        }
        if access_log.is_none()
            && request_file.is_none()
            && !ordinary_request_was_set
            && let Some(replay) = profile.load_profile.log_replay
        {
            match replay.format.as_str() {
                "nginx" => access_log = Some(replay.path),
                "jsonl" => request_file = Some(replay.path),
                _ => unreachable!("profile format is validated before use"),
            }
            if !replay_option_was_set {
                if let Some(order) = replay.order {
                    replay_order = match order.as_str() {
                        "sequential" => ReplayOrder::Sequential,
                        "shuffle" => ReplayOrder::Shuffle,
                        "random" => ReplayOrder::Random,
                        _ => {
                            return Err(format!(
                                "profile load_profile.log_replay.order must be sequential, shuffle, or random, got {order}"
                            ));
                        }
                    };
                }
                seed = replay.seed;
            }
            if replay_rounds.is_none() {
                replay_rounds = replay.rounds;
            }
            if request_schema.is_none() {
                request_schema = replay.schema;
            }
            if !skip_invalid_records {
                skip_invalid_records = replay.skip_invalid_records;
            }
            if allowed_methods.is_empty() {
                allowed_methods = replay
                    .filter
                    .allowed_methods
                    .iter()
                    .map(|value| parse_method(value))
                    .collect::<Result<_, _>>()?;
            }
            if allowed_uris.is_empty() {
                allowed_uris = replay.filter.allowed_uris;
            }
            if replay_rate.is_none()
                && !replay_timestamps
                && stages.is_empty()
                && let Some(pacing) = replay.pacing
            {
                match pacing.mode.as_str() {
                    "none" => {}
                    "rate" => replay_rate = pacing.rate,
                    "timestamp" => {
                        replay_timestamps = true;
                        replay_speed = pacing.speed.unwrap_or(1.0);
                    }
                    "stages" => {
                        stages = pacing
                            .stages
                            .into_iter()
                            .map(|stage| {
                                let duration = parse_duration(&stage.duration)?;
                                if duration.is_zero() {
                                    return Err(
                                        "profile replay stage durations must be greater than zero"
                                            .into(),
                                    );
                                }
                                Ok(ReplayStage {
                                    duration,
                                    rate: stage.target_rps,
                                })
                            })
                            .collect::<Result<_, String>>()?;
                    }
                    _ => unreachable!("profile pacing mode is validated before use"),
                }
            }
        }
    }

    if version {
        println!("rload {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if duration.is_some() && requests_were_set {
        return Err("--duration and --requests cannot be used together".into());
    }
    if output_beauty && output_format == OutputFormat::Json {
        return Err("--output-beauty cannot be used with --output-format json".into());
    }
    if output_beauty {
        output_format = OutputFormat::Beauty;
    }
    if access_log.is_some() && request_file.is_some() {
        return Err("--access-log and --request-file cannot be used together".into());
    }
    if request_schema.is_some() && request_file.is_none() {
        return Err("--request-schema requires --request-file".into());
    }
    if skip_invalid_records && request_file.is_none() {
        return Err("--skip-invalid-records requires --request-file".into());
    }
    if replay_rounds.is_some() && access_log.is_none() && request_file.is_none() {
        return Err("--replay-rounds requires --access-log or --request-file".into());
    }
    if replay_rounds.is_some() && (requests_were_set || duration.is_some()) {
        return Err("--replay-rounds cannot be combined with --requests or --duration".into());
    }
    if replay_rounds.is_some() && replay_order == ReplayOrder::Random {
        return Err("--replay-rounds cannot be combined with random replay order".into());
    }
    if ordinary_request_was_set && (access_log.is_some() || request_file.is_some()) {
        return Err("ordinary request options cannot be combined with replay inputs".into());
    }
    if !data.is_empty() && data_binary.is_some() {
        return Err("--data and --data-binary cannot be used together".into());
    }
    let body_was_set = !data.is_empty() || data_binary.is_some();
    let body = if let Some(path) = data_binary {
        Some(read_body_file(&path)?)
    } else if !data.is_empty() {
        Some(data.join("&").into_bytes())
    } else {
        None
    };
    if !method_was_set && body_was_set {
        method = Method::Post;
    }
    if replay_option_was_set && access_log.is_none() && request_file.is_none() {
        return Err("--replay-order and --seed require --access-log or --request-file".into());
    }
    if replay_rate.is_some() && access_log.is_none() && request_file.is_none() {
        return Err("--replay-rate requires --access-log or --request-file".into());
    }
    if replay_timestamps && access_log.is_none() && request_file.is_none() {
        return Err("--replay-timestamps requires --access-log or --request-file".into());
    }
    if replay_speed_was_set && !replay_timestamps {
        return Err("--replay-speed requires --replay-timestamps".into());
    }
    if replay_timestamps && replay_rate.is_some() {
        return Err("--replay-timestamps cannot be combined with --replay-rate".into());
    }
    if replay_timestamps && replay_order != ReplayOrder::Sequential {
        return Err("--replay-timestamps requires sequential replay order".into());
    }
    if stages_were_set && replay_stages_were_set {
        return Err("--stages cannot be combined with --replay-stages".into());
    }
    if replay_stages_were_set && access_log.is_none() && request_file.is_none() {
        return Err("--replay-stages requires --access-log or --request-file".into());
    }
    if !stages.is_empty() && (replay_rate.is_some() || replay_timestamps) {
        return Err("--stages cannot be combined with --replay-rate or --replay-timestamps".into());
    }
    if (!allowed_methods.is_empty() || !allowed_uris.is_empty())
        && access_log.is_none()
        && request_file.is_none()
    {
        return Err("replay whitelist options require --access-log or --request-file".into());
    }
    if seed.is_some() && replay_order == ReplayOrder::Sequential {
        return Err("--seed requires --replay-order shuffle or random".into());
    }
    let whitelist_was_set = !allowed_methods.is_empty() || !allowed_uris.is_empty();
    let config = RunConfig {
        url: url.ok_or_else(|| "a target URL is required".to_owned())?,
        method,
        limit: if requests_were_set {
            RunLimit::Requests(requests)
        } else {
            RunLimit::Duration(duration.unwrap_or(Duration::from_secs(10)))
        },
        connections,
        threads,
        timeout,
    };
    let replay_options = ReplayOptions {
        order: replay_order,
        seed,
        rate: replay_rate,
        timestamps: replay_timestamps,
        speed: replay_speed,
        stages: stages.clone(),
    };
    let replay_filter = ReplayFilter {
        allowed_methods,
        allowed_uris,
    };
    let is_replay = access_log.is_some() || request_file.is_some();
    let summary = match (access_log, request_file) {
        (Some(path), None) => run_access_log_with_run_options(
            config,
            path,
            ReplayRunOptions {
                replay: replay_options.clone(),
                rounds: replay_rounds,
            },
            replay_filter,
        ),
        (None, Some(path)) => run_request_file_with_run_options(
            config,
            path,
            RequestFileReplayOptions {
                replay: replay_options.clone(),
                rounds: replay_rounds,
                schema: request_schema.map(Into::into),
                skip_invalid_records,
            },
            replay_filter,
        ),
        (None, None) if ordinary_request_was_set => {
            let options = RequestOptions { headers, body };
            if stages.is_empty() {
                run_with_request(config, options)
            } else {
                run_with_request_and_stages(config, options, stages)
            }
        }
        (None, None) if stages.is_empty() => run(config),
        (None, None) => run_with_stages(config, stages),
        (Some(_), Some(_)) => unreachable!("replay inputs are mutually exclusive"),
    }
    .map_err(|error| error.to_string())?;
    for expression in assertions {
        rload::assertions::evaluate(&summary, &expression)?;
    }
    if let Some(path) = output_html {
        let result = json_result(&summary, &replay_options, whitelist_was_set);
        let report = rload::report::render(&result)?;
        fs::write(&path, report)
            .map_err(|error| format!("cannot write HTML report {path}: {error}"))?;
    }
    if output_format == OutputFormat::Json {
        print_json(&summary, &replay_options, whitelist_was_set)?;
        return Ok(());
    }
    if output_format == OutputFormat::Beauty {
        print_beauty(&summary, &replay_options, whitelist_was_set, is_replay);
        return Ok(());
    }
    println!("{} requests completed", summary.completed);
    println!("{} B read", summary.read_bytes);
    println!("{} response body B read", summary.response_body_bytes);
    print_optional_duration("Average latency", summary.latencies.average());
    print_optional_duration("Minimum latency", summary.latencies.minimum());
    print_optional_duration("Maximum latency", summary.latencies.maximum());
    print_optional_duration("Median latency", summary.latencies.median());
    println!("Latency Distribution");
    for percentile in [50.0, 75.0, 90.0, 99.0] {
        if let Some(value) = summary.latencies.percentile(percentile) {
            println!("  {percentile:>3.0}% {value:>10.2?}");
        } else {
            println!("  {percentile:>3.0}% {:>10}", "N/A");
        }
    }
    if summary.latencies.overflow_count() > 0 {
        println!(
            "Latency samples above one hour: {}",
            summary.latencies.overflow_count()
        );
    }
    println!(
        "Requests/sec: {:.2}",
        summary.completed as f64 / summary.runtime.as_secs_f64()
    );
    println!(
        "Load requests/sec: {:.2}",
        summary.completed as f64 / summary.load_runtime.as_secs_f64()
    );
    println!("Load window: {:.2?}", summary.load_runtime);
    println!("Drain time: {:.2?}", summary.drain_runtime);
    if replay_options.timestamps {
        println!("Timestamp replay speed: {:.3}x", replay_options.speed);
    }
    if let Some(rounds) = summary.configured_replay_rounds {
        println!("Configured replay rounds: {rounds}");
        println!(
            "Completed replay rounds: {}",
            summary.completed_replay_rounds.unwrap_or_default()
        );
    }
    if !replay_options.stages.is_empty() {
        let profile = replay_options
            .stages
            .iter()
            .map(|stage| format!("{:.3?}:{}", stage.duration, stage.rate))
            .collect::<Vec<_>>()
            .join(",");
        if is_replay {
            println!("Replay stages: {profile}");
        } else {
            println!("Rate stages: {profile}");
        }
    }
    if let Some(rate) = summary.configured_replay_rate {
        println!("Configured replay rate: {rate} requests/sec");
        println!(
            "Measured replay rate: {:.2} requests/sec",
            summary.completed as f64 / summary.load_runtime.as_secs_f64()
        );
    }
    if let Some(interval) = summary.coordinated_omission_interval {
        println!("Latency correction interval: {:.2?}", interval);
    }
    if summary.status_errors > 0 {
        println!("HTTP status errors: {}", summary.status_errors);
    }
    if summary.socket_errors.total() > 0 {
        println!(
            "Socket errors: connect {}, read {}, write {}, timeout {}",
            summary.socket_errors.connect,
            summary.socket_errors.read,
            summary.socket_errors.write,
            summary.socket_errors.timeout
        );
    }
    if whitelist_was_set {
        println!(
            "Replay entries filtered by whitelist: {}",
            summary.filtered_replay_entries
        );
    }
    let skipped_access_log_entries: u64 = summary.skipped_access_log_methods.values().sum();
    if skipped_access_log_entries > 0 {
        println!("Access-log entries skipped: {skipped_access_log_entries}");
        for (method, count) in &summary.skipped_access_log_methods {
            println!("  {method:<5} {count:>8} unsupported method");
        }
    }
    let skipped_request_file_records = summary.skipped_request_file_records.total();
    if skipped_request_file_records > 0 {
        println!("JSONL records skipped: {skipped_request_file_records}");
        for (reason, count) in summary.skipped_request_file_records.iter() {
            println!("  {count:>8} {reason}");
        }
    }
    println!("Method Statistics");
    for method in Method::ALL {
        let name = method.as_str();
        let method = summary.method(method);
        if method.completed > 0 {
            println!(
                "  {:<5} {:>8} requests, {:>8} errors, min {:.2?}, max {:.2?}, average {:.2?}, median {:.2?}",
                name,
                method.completed,
                method.status_errors,
                method
                    .latencies
                    .minimum()
                    .expect("completed method has latency"),
                method
                    .latencies
                    .maximum()
                    .expect("completed method has latency"),
                method
                    .latencies
                    .average()
                    .expect("completed method has latency"),
                method
                    .latencies
                    .median()
                    .expect("completed method has latency")
            );
        }
    }
    println!("HTTP Status Statistics");
    for (status, count) in summary.observed_statuses() {
        println!("  {:<5} {:>8} responses", status, count);
    }
    println!("URI Top 20 (estimated)");
    for uri in summary.top_uris() {
        println!(
            "  estimated {:>8}, maximum overcount {:<8} {}",
            uri.estimated_requests, uri.maximum_error, uri.uri
        );
    }
    Ok(())
}

fn print_json(
    summary: &RunSummary,
    replay_options: &ReplayOptions,
    whitelist_was_set: bool,
) -> Result<(), String> {
    let result = json_result(summary, replay_options, whitelist_was_set);
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn json_result(
    summary: &RunSummary,
    replay_options: &ReplayOptions,
    whitelist_was_set: bool,
) -> serde_json::Value {
    let methods = Method::ALL
        .into_iter()
        .filter_map(|method| {
            let statistics = summary.method(method);
            (statistics.completed > 0).then(|| {
                (
                    method.as_str().to_owned(),
                    serde_json::json!({
                        "requests": statistics.completed,
                        "status_errors": statistics.status_errors,
                        "minimum_latency_us": statistics.latencies.minimum().map(duration_us),
                        "maximum_latency_us": statistics.latencies.maximum().map(duration_us),
                        "average_latency_us": statistics.latencies.average().map(duration_us),
                        "median_latency_us": statistics.latencies.median().map(duration_us),
                    }),
                )
            })
        })
        .collect::<serde_json::Map<_, _>>();
    let statuses = summary
        .observed_statuses()
        .map(|(status, count)| (status.to_string(), serde_json::json!(count)))
        .collect::<serde_json::Map<_, _>>();
    let stages = replay_options
        .stages
        .iter()
        .map(|stage| {
            serde_json::json!({
                "duration_us": duration_us(stage.duration),
                "rate": stage.rate,
            })
        })
        .collect::<Vec<_>>();
    let skipped_total: u64 = summary.skipped_access_log_methods.values().sum();
    let skipped_request_file_total = summary.skipped_request_file_records.total();
    serde_json::json!({
        "schema_version": 1,
        "summary": {
            "completed_requests": summary.completed,
            "read_bytes": summary.read_bytes,
            "response_body_bytes": summary.response_body_bytes,
            "runtime_us": duration_us(summary.runtime),
            "load_runtime_us": duration_us(summary.load_runtime),
            "drain_runtime_us": duration_us(summary.drain_runtime),
            "requests_per_sec": summary.completed as f64 / summary.runtime.as_secs_f64(),
            "load_requests_per_sec": summary.completed as f64 / summary.load_runtime.as_secs_f64(),
            "status_errors": summary.status_errors,
        },
        "latency": {
            "minimum_us": summary.latencies.minimum().map(duration_us),
            "maximum_us": summary.latencies.maximum().map(duration_us),
            "average_us": summary.latencies.average().map(duration_us),
            "median_us": summary.latencies.median().map(duration_us),
            "p50_us": summary.latencies.percentile(50.0).map(duration_us),
            "p75_us": summary.latencies.percentile(75.0).map(duration_us),
            "p90_us": summary.latencies.percentile(90.0).map(duration_us),
            "p99_us": summary.latencies.percentile(99.0).map(duration_us),
            "overflow_count": summary.latencies.overflow_count(),
            "correction_interval_us": summary.coordinated_omission_interval.map(duration_us),
        },
        "socket_errors": {
            "connect": summary.socket_errors.connect,
            "read": summary.socket_errors.read,
            "write": summary.socket_errors.write,
            "timeout": summary.socket_errors.timeout,
            "total": summary.socket_errors.total(),
        },
        "methods": methods,
        "http_statuses": statuses,
        "uri_top": summary.top_uris().into_iter().map(|uri| serde_json::json!({
            "uri": uri.uri.as_ref(),
            "estimated_requests": uri.estimated_requests,
            "maximum_error": uri.maximum_error,
        })).collect::<Vec<_>>(),
        "pacing": {
            "stages": &stages,
        },
        "replay": {
            "configured_rate": summary.configured_replay_rate,
            "measured_rate": summary.configured_replay_rate.map(|_| summary.completed as f64 / summary.load_runtime.as_secs_f64()),
            "timestamp_speed": replay_options.timestamps.then_some(replay_options.speed),
            "stages": &stages,
            "filtered_entries": whitelist_was_set.then_some(summary.filtered_replay_entries),
            "skipped_entries": skipped_total,
            "skipped_methods": summary.skipped_access_log_methods,
            "skipped_request_file_records": {
                "total": skipped_request_file_total,
                "reasons": summary.skipped_request_file_records.iter().collect::<std::collections::BTreeMap<_, _>>(),
            },
            "entries": summary.replay_entries,
            "configured_rounds": summary.configured_replay_rounds,
            "completed_rounds": summary.completed_replay_rounds,
        },
    })
}

fn print_beauty(
    summary: &RunSummary,
    replay_options: &ReplayOptions,
    whitelist_was_set: bool,
    is_replay: bool,
) {
    println!("rload result");
    println!("============");
    println!();
    println!("Summary");
    println!("  Requests completed   {:>12}", summary.completed);
    println!("  Bytes read           {:>12}", summary.read_bytes);
    println!("  Response body bytes  {:>12}", summary.response_body_bytes);
    println!("  Load window          {:>12.2?}", summary.load_runtime);
    println!("  Drain time           {:>12.2?}", summary.drain_runtime);
    println!();
    println!("Throughput");
    println!(
        "  Requests/sec         {:>12.2}",
        summary.completed as f64 / summary.runtime.as_secs_f64()
    );
    println!(
        "  Load requests/sec    {:>12.2}",
        summary.completed as f64 / summary.load_runtime.as_secs_f64()
    );
    println!();
    println!("Latency");
    print_beauty_optional_duration("Average", summary.latencies.average());
    print_beauty_optional_duration("Minimum", summary.latencies.minimum());
    print_beauty_optional_duration("Maximum", summary.latencies.maximum());
    print_beauty_optional_duration("Median", summary.latencies.median());
    println!("  Percentile                Value");
    for percentile in [50.0, 75.0, 90.0, 99.0] {
        if let Some(value) = summary.latencies.percentile(percentile) {
            println!("  {percentile:>6.0}%              {value:>12.2?}");
        } else {
            println!("  {percentile:>6.0}%              {:>12}", "N/A");
        }
    }
    if summary.latencies.overflow_count() > 0 {
        println!(
            "  Samples above 1 hour {:>12}",
            summary.latencies.overflow_count()
        );
    }
    if let Some(interval) = summary.coordinated_omission_interval {
        println!("  Correction interval  {:>12.2?}", interval);
    }
    println!();
    println!("Errors");
    println!("  HTTP status          {:>12}", summary.status_errors);
    println!(
        "  Socket total         {:>12}",
        summary.socket_errors.total()
    );
    println!(
        "    connect            {:>12}",
        summary.socket_errors.connect
    );
    println!("    read               {:>12}", summary.socket_errors.read);
    println!("    write              {:>12}", summary.socket_errors.write);
    println!(
        "    timeout            {:>12}",
        summary.socket_errors.timeout
    );

    if replay_options.timestamps
        || !replay_options.stages.is_empty()
        || summary.configured_replay_rate.is_some()
        || whitelist_was_set
        || !summary.skipped_access_log_methods.is_empty()
        || summary.configured_replay_rounds.is_some()
    {
        println!();
        if is_replay {
            println!("Replay");
        } else {
            println!("Pacing");
        }
        if replay_options.timestamps {
            println!("  Timestamp speed      {:>11.3}x", replay_options.speed);
        }
        if let Some(rounds) = summary.configured_replay_rounds {
            println!("  Configured rounds     {:>12}", rounds);
            println!(
                "  Completed rounds      {:>12}",
                summary.completed_replay_rounds.unwrap_or_default()
            );
        }
        if !replay_options.stages.is_empty() {
            let profile = replay_options
                .stages
                .iter()
                .map(|stage| format!("{:.3?}:{}", stage.duration, stage.rate))
                .collect::<Vec<_>>()
                .join(",");
            println!("  Rate stages          {profile:>12}");
        }
        if let Some(rate) = summary.configured_replay_rate {
            println!("  Configured rate      {:>12}/s", rate);
            println!(
                "  Measured rate        {:>12.2}/s",
                summary.completed as f64 / summary.load_runtime.as_secs_f64()
            );
        }
        if whitelist_was_set {
            println!(
                "  Filtered entries     {:>12}",
                summary.filtered_replay_entries
            );
        }
        let skipped: u64 = summary.skipped_access_log_methods.values().sum();
        if skipped > 0 {
            println!("  Skipped entries      {skipped:>12}");
            for (method, count) in &summary.skipped_access_log_methods {
                println!("    {method:<7} {count:>10} unsupported");
            }
        }
        let skipped_jsonl = summary.skipped_request_file_records.total();
        if skipped_jsonl > 0 {
            println!("  JSONL skipped        {skipped_jsonl:>12}");
            for (reason, count) in summary.skipped_request_file_records.iter() {
                println!("    {count:>10} {reason}");
            }
        }
    }

    println!();
    println!("Breakdowns");
    println!("  Methods");
    for method in Method::ALL {
        let statistics = summary.method(method);
        if statistics.completed > 0 {
            println!(
                "    {:<7} {:>10} requests  {:>8} errors  min {:.2?}  max {:.2?}  avg {:.2?}  median {:.2?}",
                method.as_str(),
                statistics.completed,
                statistics.status_errors,
                statistics
                    .latencies
                    .minimum()
                    .expect("completed method has latency"),
                statistics
                    .latencies
                    .maximum()
                    .expect("completed method has latency"),
                statistics
                    .latencies
                    .average()
                    .expect("completed method has latency"),
                statistics
                    .latencies
                    .median()
                    .expect("completed method has latency")
            );
        }
    }
    println!("  HTTP statuses");
    for (status, count) in summary.observed_statuses() {
        println!("    {status:<7} {count:>10} responses");
    }
    println!("  URI Top 20 (estimated)");
    for uri in summary.top_uris() {
        println!(
            "    {:>10} ±{:>8}  {}",
            uri.estimated_requests, uri.maximum_error, uri.uri
        );
    }
}

fn duration_us(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

fn print_optional_duration(label: &str, value: Option<Duration>) {
    if let Some(value) = value {
        println!("{label}: {value:.2?}");
    } else {
        println!("{label}: N/A");
    }
}

fn print_beauty_optional_duration(label: &str, value: Option<Duration>) {
    if let Some(value) = value {
        println!("  {label:<20} {value:>12.2?}");
    } else {
        println!("  {label:<20} {:>12}", "N/A");
    }
}

fn print_help() {
    println!("Usage: rload [OPTIONS] <URL>");
    println!("      --profile <FILE>  Load a v1 YAML workload profile");
    println!("      --assert <EXPR>   Fail when a final-summary assertion does not hold");
    println!("      --output-html <FILE>  Write a self-contained offline HTML report");
    println!("\nOptions:");
    println!("  -n, --requests <N>  Number of requests to send instead of a duration run");
    println!("  -d, --duration <T>  Run duration [default: 10s]");
    println!("  -c, --connections <N>  Concurrent connections [default: 10]");
    println!("  -t, --threads <N>  Worker threads [default: 2]");
    println!("  -T, --timeout <T>  Socket/request timeout [default: 2s]");
    println!("      --latency      Print latency distribution (always enabled)");
    println!("      --access-log <FILE>  Replay GET/HEAD requests from an Nginx access log");
    println!("      --request-file <FILE>  Replay structured requests from a JSONL file");
    println!("      --request-schema <FILE>  Map JSONL fields with a YAML schema");
    println!("      --skip-invalid-records  Skip invalid JSONL records while loading");
    println!("      --replay-order <ORDER>  sequential, shuffle, or random [default: sequential]");
    println!("      --seed <N>       Reproducible seed for shuffle or random replay");
    println!("      --replay-rate <RPS>  Global replay request rate");
    println!("      --replay-timestamps  Pace access-log or JSONL replay by timestamps");
    println!("      --replay-speed <N>   Timestamp playback multiplier [default: 1.0]");
    println!("      --stages <D:R,...>  Timed rate stages for ordinary or replay requests");
    println!("      --replay-stages <D:R,...>  Compatibility alias for replay requests");
    println!("      --replay-rounds <N>  Replay the filtered sequence N complete times");
    println!("      --output-format <FORMAT>  text or json [default: text]");
    println!("      --output-beauty  Print a sectioned human-readable result");
    println!("  -X, --request <METHOD>  HTTP method for an ordinary request");
    println!("  -H, --header <HEADER>  Request header; may be repeated");
    println!("      --data <DATA>    UTF-8 body; repeated values are joined with '&'");
    println!("      --data-binary @FILE  Raw request body from a file");
    println!("      --allowed-methods <LIST>  Replay method whitelist, comma-separated");
    println!("      --allowed-uris <GLOBS>  Replay URI whitelist, comma-separated");
    println!("  -h, --help          Print help");
    println!("      --version       Print version information");
}

fn parse_method(value: &str) -> Result<Method, String> {
    Method::ALL
        .into_iter()
        .find(|method| method.as_str() == value)
        .ok_or_else(|| format!("unsupported HTTP method: {value}"))
}

fn comma_separated<'a>(value: &'a str, option: &str) -> Result<Vec<&'a str>, String> {
    let values: Vec<_> = value.split(',').map(str::trim).collect();
    if values.is_empty() || values.iter().any(|value| value.is_empty()) {
        return Err(format!(
            "{option} requires non-empty comma-separated values"
        ));
    }
    Ok(values)
}

fn parse_stages(value: &str, option: &str) -> Result<Vec<ReplayStage>, String> {
    let stage_name = if option == "--replay-stages" {
        "replay stage"
    } else {
        "rate stage"
    };
    comma_separated(value, option)?
        .into_iter()
        .map(|stage| {
            let (duration, rate) = stage
                .split_once(':')
                .ok_or_else(|| format!("invalid {stage_name} {stage}; expected DURATION:RPS"))?;
            let duration = parse_duration(duration)?;
            let rate = rate
                .parse::<u64>()
                .map_err(|_| format!("invalid {stage_name} rate: {rate}"))?;
            if duration.is_zero() || rate == 0 {
                return Err(format!(
                    "{stage_name} durations and rates must be greater than zero"
                ));
            }
            Ok(ReplayStage { duration, rate })
        })
        .collect()
}

fn split_attached(argument: String) -> (String, Option<String>) {
    for option in ["-T", "-X", "-H", "-t", "-c", "-d", "-n"] {
        if let Some(value) = argument
            .strip_prefix(option)
            .filter(|value| !value.is_empty())
        {
            return (option.to_owned(), Some(value.to_owned()));
        }
    }
    (argument, None)
}

fn read_body_file(path: &str) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("unable to read body file {path}: {error}"))?;
    if file
        .metadata()
        .map_err(|error| format!("unable to inspect body file {path}: {error}"))?
        .len()
        > MAX_REQUEST_BODY_BYTES as u64
    {
        return Err(format!("body file {path} exceeds 512 KiB"));
    }
    let mut body = Vec::new();
    file.take(MAX_REQUEST_BODY_BYTES as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|error| format!("unable to read body file {path}: {error}"))?;
    if body.len() > MAX_REQUEST_BODY_BYTES {
        return Err(format!("body file {path} exceeds 512 KiB"));
    }
    Ok(body)
}

fn parse_duration(value: &str) -> Result<Duration, String> {
    let (number, multiplier) = if let Some(number) = value.strip_suffix("ms") {
        (number, 1)
    } else if let Some(number) = value.strip_suffix('s') {
        (number, 1_000)
    } else if let Some(number) = value.strip_suffix('m') {
        (number, 60_000)
    } else if let Some(number) = value.strip_suffix('h') {
        (number, 3_600_000)
    } else {
        (value, 1_000)
    };
    let milliseconds = number
        .parse::<u64>()
        .ok()
        .and_then(|number| number.checked_mul(multiplier))
        .filter(|milliseconds| *milliseconds > 0)
        .ok_or_else(|| format!("invalid duration: {value}"))?;
    Ok(Duration::from_millis(milliseconds))
}

#[cfg(test)]
mod tests {
    use super::parse_duration;
    use std::time::Duration;

    #[test]
    fn duration_supports_wrk_time_units() {
        assert_eq!(parse_duration("30").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("250ms").unwrap(), Duration::from_millis(250));
        assert_eq!(parse_duration("2s").unwrap(), Duration::from_secs(2));
        assert_eq!(parse_duration("3m").unwrap(), Duration::from_secs(180));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3_600));
    }
}
