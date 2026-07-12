# Shared-server HTTP/1.1 regression baseline

Date: 2026-07-10
Host: Apple arm64, 16 logical CPUs, Darwin 25.5.0
Rust: 1.96.1
wrk: 4.2.0, kqueue
Scenario: local benchmark server, 2 worker threads, 100 connections, 10 seconds,
3 measured runs after warm-up.

## Current median result

After replacing raw latency samples with a fixed-memory HDR histogram:

| Client | Requests/sec | Average latency | P99 latency | Maximum RSS |
|---|---:|---:|---:|---:|
| wrk | 75,565.56 | 1.32 ms | 1.58 ms | 3.64 MB |
| rload | 74,553.58 | 1.34 ms | 1.64 ms | 2.77 MB |

rload throughput was 1.34% below wrk in this run, while maximum RSS was 24%
lower. Individual throughput runs varied by roughly 2–3%, reinforcing that this
shared-server scenario is primarily a regression check.

## Initial median result

| Client | Requests/sec | Average latency | Maximum RSS | User CPU | System CPU |
|---|---:|---:|---:|---:|---:|
| wrk | 75,311.63 | 1.32 ms | 3.60 MB | 0.69 s | 10.89 s |
| rload | 75,240.82 | 1.33 ms | 39.55 MB | 0.96 s | 10.99 s |

rload throughput was 0.09% below wrk in this setup. The nearly identical
throughput and system CPU indicate that the single-threaded benchmark server or
loopback networking may be the limiting resource, so this result is a regression
baseline rather than evidence that the clients have equal maximum capacity.
It only detects regressions that lower a client below the shared server's
approximately 75k RPS ceiling. Client-capacity claims require a multi-worker
server or a server on a separate host.

The initial rload implementation stored one `Duration` for every completed
request, causing memory to grow with test duration and request rate. The current
histogram implementation removes this growth, as shown above.

## Initial raw requests/sec

| Run | wrk | rload |
|---|---:|---:|
| 1 | 75,311.63 | 75,497.98 |
| 2 | 75,454.22 | 75,236.26 |
| 3 | 75,028.60 | 75,240.82 |

## Current raw requests/sec

| Run | wrk | rload |
|---|---:|---:|
| 1 | 74,528.93 | 73,713.60 |
| 2 | 75,565.56 | 74,553.58 |
| 3 | 76,280.00 | 76,062.59 |

Run the same scenario with:

```sh
DURATION=10 RUNS=3 THREADS=2 CONNECTIONS=100 ./benchmarks/run.sh
```

## Long-running stability smoke test

On 2026-07-11, a 30-second run with two worker threads and 400 connections
completed 2,091,796 requests without an HTTP error or premature exit. It
reported 69,712.16 requests/sec, 5.73 ms average latency, 6.66 ms P99 latency,
and 8.05 MiB maximum RSS. This shared local server remains throughput-bound, so
the run is a lifecycle and bounded-memory smoke test rather than a capacity
measurement.

The client command was:

```sh
/usr/bin/time -l target/release/rload -t2 -c400 -d30s http://127.0.0.1:18130/
```
