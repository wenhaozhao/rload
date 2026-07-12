# Accuracy validation

`wrk` is the behavioral baseline. Each comparison uses the same server, URL,
thread count, connection count, and load duration. Runs are paired by index and
client order alternates to reduce warm-cache and thermal bias.

Use at least five runs per scenario and cover low, medium, and high concurrency:

```sh
RUNS=5 CONNECTIONS=10  ADDRESS=127.0.0.1:18110 ./benchmarks/run.sh
RUNS=5 CONNECTIONS=100 ADDRESS=127.0.0.1:18111 ./benchmarks/run.sh
RUNS=5 CONNECTIONS=400 ADDRESS=127.0.0.1:18112 ./benchmarks/run.sh
python3 benchmarks/accuracy.py benchmarks/results/<c10> benchmarks/results/<c100> benchmarks/results/<c400>
```

For controlled tail latency, set a fixed delay and deterministic uniform jitter:

```sh
RUNS=5 DELAY_US=1000 JITTER_US=1000 ./benchmarks/run.sh
```

For every metric, relative error is `(rload / wrk - 1) * 100`. The report shows
signed mean error (bias), mean absolute error (MAE), sample standard deviation,
Student-t 95% confidence interval for the mean, and the observed range.

The acceptance gates are:

| Metric | Gate |
|---|---:|
| Requests/sec, average, P50, P75 | MAE <= 3% |
| P90 | MAE <= 5% |
| P99 | median absolute paired error <= 5% |

## Current acceptance result

The formal local matrix on 2026-07-11 used five paired 10-second runs at each
of 10, 100, and 400 connections, with two client threads and alternating client
order. All gates passed across the combined 15 pairs:

| Metric | Bias | MAE | Gate statistic | Result |
|---|---:|---:|---:|:---:|
| Requests/sec | -0.223% | 1.107% | MAE 1.107% | PASS |
| Average latency | +0.823% | 1.138% | MAE 1.138% | PASS |
| P50 | +0.655% | 0.984% | MAE 0.984% | PASS |
| P75 | +1.052% | 1.457% | MAE 1.457% | PASS |
| P90 | +1.413% | 2.018% | MAE 2.018% | PASS |
| P99 | +3.897% | 4.982% | median absolute error 3.109% | PASS |

The P99 median absolute errors for 10, 100, and 400 connections were 2.551%,
3.774%, and 4.265%, respectively. A preceding three-run, three-second smoke
matrix exceeded the combined P99 gate, demonstrating why the acceptance run
uses at least five longer pairs for tail-latency decisions.

P99 uses a robust multi-run statistic because scheduler and network jitter can
move a single tail percentile substantially. A result set with fewer than three
pairs fails validation. Resource usage is reported by the benchmark runner but
is not an accuracy gate.

Like `wrk`, `rload` post-corrects its latency histogram for coordinated omission.
The correction interval is printed in the command output. `Load window` and
`Drain time` are also printed separately, while the compatible `Requests/sec`
value continues to use total elapsed runtime.
