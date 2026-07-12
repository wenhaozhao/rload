use std::env;
use std::fs;
use std::io::Read;
use std::process::ExitCode;
use std::time::Duration;

use rload::{
    MAX_REQUEST_BODY_BYTES, Method, ReplayFilter, ReplayOptions, ReplayOrder, RequestOptions,
    RunConfig, RunLimit, run, run_access_log_with_filter, run_request_file_with_filter,
    run_with_request,
};

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
    let mut requests = 1;
    let mut requests_were_set = false;
    let mut duration = None;
    let mut connections = 10;
    let mut threads = 2;
    let mut timeout = Duration::from_secs(2);
    let mut url = None;
    let mut access_log = None;
    let mut request_file = None;
    let mut replay_order = ReplayOrder::Sequential;
    let mut replay_option_was_set = false;
    let mut seed = None;
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
            }
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
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

    if duration.is_some() && requests_were_set {
        return Err("--duration and --requests cannot be used together".into());
    }
    if access_log.is_some() && request_file.is_some() {
        return Err("--access-log and --request-file cannot be used together".into());
    }
    let ordinary_request_was_set =
        method_was_set || !headers.is_empty() || !data.is_empty() || data_binary.is_some();
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
    };
    let replay_filter = ReplayFilter {
        allowed_methods,
        allowed_uris,
    };
    let summary = match (access_log, request_file) {
        (Some(path), None) => {
            run_access_log_with_filter(config, path, replay_options, replay_filter)
        }
        (None, Some(path)) => {
            run_request_file_with_filter(config, path, replay_options, replay_filter)
        }
        (None, None) if ordinary_request_was_set => {
            run_with_request(config, RequestOptions { headers, body })
        }
        (None, None) => run(config),
        (Some(_), Some(_)) => unreachable!("replay inputs are mutually exclusive"),
    }
    .map_err(|error| error.to_string())?;
    let average_latency = summary.latencies.mean();

    println!("{} requests completed", summary.completed);
    println!("{} response body B read", summary.response_body_bytes);
    println!("Average latency: {:.2?}", average_latency);
    println!("Latency Distribution");
    for percentile in [50.0, 75.0, 90.0, 99.0] {
        println!(
            "  {:>3.0}% {:>10.2?}",
            percentile,
            summary
                .latencies
                .percentile(percentile)
                .expect("built-in percentile is valid")
        );
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
    println!("Method Statistics");
    for method in Method::ALL {
        let name = method.as_str();
        let method = summary.method(method);
        if method.completed > 0 {
            println!(
                "  {:<5} {:>8} requests, {:>8} errors, average {:.2?}",
                name,
                method.completed,
                method.status_errors,
                method.latencies.mean()
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

fn print_help() {
    println!("Usage: rload [OPTIONS] <URL>");
    println!("\nOptions:");
    println!("  -n, --requests <N>  Number of requests to send instead of a duration run");
    println!("  -d, --duration <T>  Run duration [default: 10s]");
    println!("  -c, --connections <N>  Concurrent connections [default: 10]");
    println!("  -t, --threads <N>  Worker threads [default: 2]");
    println!("  -T, --timeout <T>  Socket/request timeout [default: 2s]");
    println!("      --latency      Print latency distribution (always enabled)");
    println!("      --access-log <FILE>  Replay GET/HEAD requests from an Nginx access log");
    println!("      --request-file <FILE>  Replay structured requests from a JSONL file");
    println!("      --replay-order <ORDER>  sequential, shuffle, or random [default: sequential]");
    println!("      --seed <N>       Reproducible seed for shuffle or random replay");
    println!("  -X, --request <METHOD>  HTTP method for an ordinary request");
    println!("  -H, --header <HEADER>  Request header; may be repeated");
    println!("      --data <DATA>    UTF-8 body; repeated values are joined with '&'");
    println!("      --data-binary @FILE  Raw request body from a file");
    println!("      --allowed-methods <LIST>  Replay method whitelist, comma-separated");
    println!("      --allowed-uris <GLOBS>  Replay URI whitelist, comma-separated");
    println!("  -h, --help          Print help");
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
