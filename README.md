# rload

English | [中文](./README.zh-cn.md)

`rload` is a Rust HTTP load generator with wrk-compatible CLI semantics,
Nginx access-log replay, and structured JSONL request replay.

Official website: [wenhaozhao.github.io/rload](https://wenhaozhao.github.io/rload/)

The current vertical slice provides:

- non-blocking HTTP/1.1 GET requests over HTTP or HTTPS using fixed worker threads;
- TLS SNI, Mozilla root certificate validation, and encrypted connection reuse;
- ordered, cyclic replay of `GET` and `HEAD` requests from Nginx common or combined access logs;
- exact per-method latency/error summaries and HTTP status-code counts;
- bounded URI Top-20 heavy-hitter estimates with explicit maximum error;
- persistent connections with request-count or duration limits;
- socket-error accounting and connection recovery during duration-limited runs;
- `Content-Length`, chunked, and connection-close response framing;
- completed request, socket-read byte, response-body byte, status error, and
  average latency output.

Lua and LuaJIT compatibility are explicitly out of scope.

## Comparison with wrk

The comparison baseline is wrk 4.2.0. Performance figures below come from
paired runs using the same URL, server, thread count, connection count, and
duration on macOS arm64. They are regression evidence for this environment,
not a guarantee that either client will reach the same throughput on every
machine.

### Performance and accuracy

| Dimension | wrk baseline | rload result | Assessment |
|---|---|---|---|
| Throughput | Reference client | RPS MAE 0.986% across 15 paired runs at 10/100/400 connections | Equivalent within the 3% gate |
| Average latency | Reference client | MAE 1.171% | Equivalent within the 3% gate |
| P50 / P75 latency | Reference percentiles | MAE 1.064% / 0.961% | Equivalent within the 3% gate |
| P90 latency | Reference percentile | MAE 0.944% | Equivalent within the 5% gate |
| P99 with 1 ms delay + deterministic jitter | Reference percentile | Median absolute error 0.567% | Passes the 5% gate |
| Zero-delay loopback P99 | Sensitive to scheduler noise | Median absolute error 5.128% | Narrowly exceeds the 5% gate; not claimed as unconditional parity |
| Static-path RSS | Reference process | Fresh 100-connection run: wrk ~3.47 MiB, rload ~3.55 MiB | Comparable in the measured run |
| Access-log replay throughput | No native access-log replay | 100k: +1.68%; 500k: -1.81% versus paired static runs | Replay overhead remained within the configured 10% gate |
| Access-log replay memory | No native access-log replay | RSS scaling slope 248.7 B per loaded entry | Passed the configured 0–256 B/entry gate |

The formal accuracy methodology, raw result directories, and gate definitions
are documented in [`benchmarks/VALIDATION_2026-07-11.md`](benchmarks/VALIDATION_2026-07-11.md)
and [`benchmarks/ACCURACY.md`](benchmarks/ACCURACY.md). The zero-delay P99
result is intentionally called out rather than rounded into a pass.

### Functional coverage

| Capability | wrk 4.2.0 | rload 0.2.2 |
|---|---|---|
| HTTP/1.1 static request load | Yes | Yes |
| HTTP and HTTPS with connection reuse | Yes | Yes, including TLS verification and SNI |
| Worker threads, connections, duration, request count | Yes | Yes; core CLI forms are compatible |
| Timeout and latency reporting | Yes | Yes; `--latency` is accepted and latency is always printed |
| HTTP status and socket-error statistics | Yes | Yes, with connect/read/write/timeout categories |
| Curl-style method, headers, and request body | Via Lua scripting | Yes for the documented curl-compatible subset |
| Lua/LuaJIT request scripting | Yes | Intentionally not supported in the first release line |
| Nginx access-log replay | No native mode | Yes; common/combined logs, `GET`/`HEAD`, sequential/shuffle/random order; unsupported methods are skipped and reported |
| JSONL request replay | No native mode | Yes; methods, headers, UTF-8 bodies, and per-record limits |
| Replay seed and method/URI whitelists | No native mode | Yes; deterministic seed plus intersection filters |
| Replay frequency/timestamp pacing/burst profiles | Custom scripting only | Fixed global rate, timestamp-speed pacing, and timed rate stages implemented |
| Automatic target inference from access-log entries | No native mode | Future candidate only; target URL is currently explicit |
| GUI configuration interface | No native mode | Future optional feature layered on the rload engine |

The result is intentionally a wrk-compatible load generator rather than a
drop-in replacement for every wrk extension: core command-line behavior and
static HTTP load are covered, while Lua compatibility is outside the first
release scope. The additional replay modes are the main functional expansion
provided by rload. A future GUI, if built, will be a configuration and
observability layer over the same engine; it will not replace the standalone
CLI or duplicate load-generation logic.

## Build and install

The 0.2.x release line is validated with stable Rust 1.96.1 on macOS
arm64, Linux, and Windows. Windows CI additionally covers PowerShell
invocation, path handling, and socket recovery.

Build or install directly from this checkout:

```sh
cargo build --release
cargo install --path .
rload --help
```

When already inside the crate directory, omit the manifest/path prefixes.
Rload is licensed under `MIT OR Apache-2.0`; see `LICENSE-MIT` and
`LICENSE-APACHE`. Third-party attribution notices are documented separately
where required by dependencies.

## Usage

```sh
cargo run --release -- --requests 10 http://127.0.0.1:8080/
cargo run --release -- --threads 2 --connections 10 --duration 30s http://127.0.0.1:8080/
cargo run --release -- --threads 2 --connections 10 --duration 30s https://example.com/
cargo run --release -- -d 30s -X POST -H 'Content-Type: application/json' \
  --data '{"id":1}' https://example.com/api/items
cargo run --release -- --threads 2 --connections 10 --duration 30s \
  --access-log /var/log/nginx/access.log https://staging.example.com/
cargo run --release -- --requests 1000 --access-log ./access.log \
  --replay-order shuffle --seed 42 https://staging.example.com/
cargo run --release -- --duration 30s --request-file ./requests.jsonl \
  --replay-order shuffle --seed 42 https://staging.example.com/
```

Ordinary requests use a curl-compatible subset while preserving wrk's option
meanings: `-X/--request`, repeatable `-H/--header`, repeatable `--data`, and
`--data-binary @FILE`. The short `-d` remains the wrk duration option; it never
means request data. Multiple `--data` values are joined with `&`, and specifying
data without `-X` selects POST. Binary files are sent byte-for-byte. The same
managed-header, URI, and 512 KiB body limits used by JSONL apply here.

Common wrk command lines remain valid: compact `-t2`, `-c100`, `-n1000`,
`-d30s`, and `-T2s` forms are accepted; times support bare seconds plus `s`, `m`, and `h`;
`-T/--timeout` controls connection/request timeout,
and `--latency` is accepted as a compatibility flag (the latency distribution
is always printed). With no explicit load options, the wrk-compatible defaults
are a 10-second run with two worker threads and ten connections.

During a duration-limited run, a request timeout, reset, or premature EOF is
counted as a socket error and the affected connection is rebuilt while time
remains. Request-count-limited runs still return an error for these failures so
a permanently unavailable target cannot make a finite run wait forever.
Socket errors are reported using wrk-style `connect`, `read`, `write`, and
`timeout` categories; failure and recovery are isolated to the affected
connection. If a duration-limited target remains unresponsive for the whole
run, the command still returns a valid summary with zero completed requests and
the accumulated timeout count after the configured duration expires. The same
bounded behavior applies when every connection attempt is refused: attempts are
counted as `connect` errors and the run ends normally at its duration limit. If
the target becomes available again before that limit, affected connections
resume sending requests without restarting the load-test process.

Access-log replay reads the quoted Nginx `$request` field, preserves its
origin-form URI (including the query string), and cycles through the log in
order until the request-count or duration limit is reached. Empty logs and
malformed request lines fail with the source line number. Methods other than
`GET` or `HEAD` are skipped and reported by total and method-specific counters
in the final summary; they are not sent or included in request
latency/throughput statistics. Request bodies are not supported in access-log
mode.

Replay order is `sequential` by default. `--replay-rate <RPS>` applies one
global request rate across all replay workers and reports both configured and
measured rates. `shuffle` visits every entry exactly
once per round and reshuffles before the next round; `random` independently
samples an entry for every request and can repeat entries. `--seed` makes either
randomized allocation sequence reproducible. With multiple connections, the
allocation sequence remains deterministic but network arrival order can vary.
`--replay-rounds <N>` runs the filtered sequential or shuffle sequence exactly
`N` complete times and cannot be combined with `--requests`, `--duration`, or
random replay order.

`--replay-timestamps` preserves gaps between adjacent Nginx access-log or JSONL
timestamps. The first request is immediate, and `--replay-speed <N>` scales
subsequent gaps (`2` is twice as fast, `0.5` is half speed). Standard
second-resolution `$time_local` values and fractional seconds up to microsecond
precision are accepted for access logs; JSONL formats are defined by the
request schema below, and JSONL timestamp pacing requires
`--request-schema <FILE>`. Records with the same timestamp are eligible to send
without an added gap. Timestamp mode requires sequential replay and
is mutually exclusive with `--replay-rate`; missing or decreasing timestamps
are rejected. When the log cycles, the next round begins immediately because
an interval from the final record back to the first is not present in the log.

`--replay-stages <DURATION:RPS,...>` defines a timed rate profile, for example
`--replay-stages 10s:100,5s:1000,10s:100` for a baseline, spike, and recovery.
Stage transitions occur on the configured boundaries; after the profile ends,
the final rate remains active. Stages work with sequential, shuffle, or random
selection and with either replay input format. They are mutually exclusive with
`--replay-rate` and `--replay-timestamps`.

### JSONL request schema

JSONL replay accepts an optional schema file for extracting fields from nested
records. The schema changes extraction paths only; it does not change existing
method, URI, header, body, query-string, or validation rules. The `fields`
object and every mapping in it are optional. When a mapping is omitted, rload
uses the current top-level JSONL field extraction for that field.

```yaml
schema_version: 1

fields:
  method:
    path: http.request.method
  uri:
    path: http.request.path
  args:
    path: http.request.query
  headers:
    path: http.request.headers
  body:
    path: http.request.body
  timestamp:
    path: event.timestamp
    format: "%d/%b/%Y:%H:%M:%S.%f %z"
```

Use it with:

```bash
rload --request-file requests.jsonl --request-schema request-schema.yaml \
  --replay-timestamps http://127.0.0.1:8080
```

Supported mappings are `method`, `uri`, `args`, `headers`, `body`, and
`timestamp`. Paths use dot-separated JSON object fields; array indexes and
expressions are not supported. `uri` remains required by the existing JSONL
validation, while an omitted `method` continues to default to `GET`.

The timestamp mapping accepts `timestamp_micros` as the canonical integer
microsecond field and `time`/`_time` as aliases. For formatted string values,
`timestamp.format` uses strftime/chrono-style placeholders. When no format is
specified, rload accepts both the Nginx format `%d/%b/%Y:%H:%M:%S %z` (for
example `03/Jul/2026:08:41:17 +0000`) and RFC3339 (for example
`2026-07-03T08:41:17Z`). A format can be specified explicitly; fractional
seconds can be parsed with `%f`, and RFC3339 can be selected with `%+`.
JSONL timestamp pacing requires a request schema even when its timestamp
mapping is omitted and the default top-level aliases are used. Timestamp pacing
requires timestamps in every record, rejects malformed or decreasing values,
and uses the same microsecond pacing semantics as access-log replay. No separate
timestamp-format CLI option is provided.

### Machine-readable results

Use `--output-format json` when integrating rload with CI, dashboards, or the
planned GUI:

```bash
rload -t2 -c100 -d30s --output-format json http://127.0.0.1/ > result.json
```

JSON output is one document on stdout with `schema_version: 1`. Durations and
latencies use integer microseconds. It includes aggregate throughput, latency
percentiles, HTTP status and method statistics, socket errors, URI Top-20
estimates, filtered/skipped replay records, and the active fixed-rate,
timestamp, or stage pacing configuration. Configuration and runtime errors
remain on stderr and return a non-zero exit status. The default `text` format
is unchanged.

For an explicitly sectioned human-readable report, use `--output-beauty`.
This mode is mutually exclusive with JSON output and does not change the
existing default-text parser anchors used by benchmark scripts:

```bash
rload -t2 -c100 -d30s --output-beauty http://127.0.0.1/
```

Text and JSON summaries expose both byte counters. `read_bytes` counts every
response byte successfully read and handed to the HTTP parser, including
headers, decoded-body input, and chunk framing. `response_body_bytes` counts
only the decoded response payload. Bytes read before a later socket failure
remain included in `read_bytes`.

URI Top-20 counts use a bounded Space-Saving estimate. For each entry, the true
request count is between `estimated_requests - maximum_error` and
`estimated_requests`; the reported error is therefore a one-sided maximum
overcount, not a symmetric confidence interval.

Structured request replay accepts one JSON object per line. It is intentionally
tolerant of exported application logs: unknown fields are ignored, and
`method` defaults to `GET` when omitted or `null`:

```json
{"method":"POST","uri":"/api/items","args":"source=web","headers":{"content-type":"application/json"},"body":"{\"id\":1}","extra_log_field":"ignored"}
```

Supported methods are `GET`, `HEAD`, `POST`, `PUT`, `PATCH`, `DELETE`, and
`OPTIONS`. Bodies are UTF-8 strings. When `args` is present, it is appended to
`uri` as the query string. A leading `?` is accepted; a leading `&` is preserved
as `?&` when `uri` has no query, while either prefix joins an existing query
with `&`. `Host`, `Connection`, and `Content-Length`
are managed by the engine and must not appear in the JSON headers. JSONL and
access-log inputs are mutually exclusive; both support the same replay-order
options. Each JSONL record is limited to 1 MiB, with an 8 KiB URI, 64 KiB of
headers, and a 512 KiB UTF-8 body. `Transfer-Encoding`, `Trailer`, and `Expect`
are also rejected because this release sends fixed-length request bodies
without an HTTP/1.1 continue handshake.

Replay inputs can be reduced with method and URI whitelists:

```sh
--allowed-methods GET,POST --allowed-uris '/api/*,/health'
```

URI patterns use a small deterministic glob syntax where `*` matches any
sequence and every other character is literal. Method and URI filters form an
intersection. Filtered entries are counted in the summary, and a whitelist that
excludes the entire input is an error. Whitelist options are not valid for an
ordinary single request. At most 32 URI patterns may be supplied, each no longer
than 256 bytes, which bounds wildcard matching work for large logs.

### Future candidate features

The following capabilities are recorded for later evaluation and are not part
of the current implementation or acceptance scope:

- target inference for custom Nginx log formats that explicitly record scheme,
  host, and port; this remains a future candidate and is not scheduled for
  0.2.0.

Measure replay overhead against the static-request path with:

```sh
ENTRIES=100000 CONNECTIONS=100 DURATION=5 ./benchmarks/replay.sh
REPLAY_ORDER=shuffle SEED=42 ./benchmarks/replay.sh
./benchmarks/replay_matrix.sh
```

The repeatable benchmark uses at least three paired runs with alternating order
and reports median throughput loss, its range, total RSS growth, and bytes per
loaded log entry. The matrix additionally validates the RSS slope between 100k
and 500k entries. Current gates are at most 10% throughput loss and between 0
and 256 bytes of incremental RSS per entry.

The 2026-07-11 sequential-replay acceptance matrix passed both scales. At 100k
entries, median throughput difference was +1.68% and incremental RSS was 252.5
B/entry. At 500k entries, the corresponding throughput difference was -1.81%
and the measured RSS scaling slope was 248.7 B/entry. These local results are
regression evidence rather than a cross-platform memory guarantee.

Run the tests and lints with:

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

Run the complete local release gate with:

```sh
./scripts/release-check.sh
```

## Benchmarking

Run the repeatable local comparison against wrk with:

```sh
./benchmarks/run.sh
```

Override `DURATION`, `THREADS`, `CONNECTIONS`, or `RUNS` through environment
variables. `DELAY_US` adds a fixed server delay and `JITTER_US` adds deterministic
uniform jitter, which makes tail-latency comparisons repeatable. Raw command
output, CPU time, maximum RSS, and environment details are written under
`benchmarks/results/`.

Analyze one or more result directories with:

```sh
python3 benchmarks/accuracy.py benchmarks/results/<timestamp> [...]
```

The checker reports paired relative bias, mean absolute error, standard
deviation, a 95% confidence interval, and range. It enforces 3% MAE for
throughput and central latency, 5% MAE for P90, and 5% median absolute error for
P99. At least three paired runs are required. See
[`benchmarks/ACCURACY.md`](benchmarks/ACCURACY.md) for the methodology.
