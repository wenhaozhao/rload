# rload Competitor Analysis, Strategic Positioning, and Technical Roadmap

As cloud-native architectures, microservices, and high-concurrency systems become ubiquitous, production traffic exhibits high dynamics, burstiness, and heterogeneity. Traditional **static single-endpoint load testing** (such as hitting a single static URL in a loop) is no longer sufficient to simulate actual bottleneck behaviors of systems under realistic production workloads.

This document conducts an in-depth market research of mainstream load generators, identifies `rload`'s **unique competitive edge (Sweet Spot)**, and establishes a customized, actionable, and long-term technical roadmap.

---

## 1. Competitor Landscape and Multi-Dimensional Benchmark

We compare five of the most representative open-source load-testing tools in the industry, ranging from ultra-lightweight load engines to fully scriptable test suites.

### 1.1 Competitor Profiles
1. **wrk / wrk2 (C)**
   * **Designation**: Ultra-high-throughput benchmark utility for HTTP/1.1.
   * **Core Architecture**: Single-threaded multiplexing (epoll/kqueue), scriptable via LuaJIT.
   * **Pros & Cons**: Industry gold standard for raw performance; extremely low resource footprint. However, it lacks native log replay capabilities, has a steep learning curve for Lua scripting, does not support modern protocols (HTTP/2, gRPC), and suffers from poor portability/reproducibility in modern CI/CD container environments.
2. **k6 (Go / JavaScript)**
   * **Designation**: Modern developer-centric load and functional testing framework.
   * **Core Architecture**: Go runtime utilizing an embedded ES6 engine (goja) to run independent JS runtimes for each Virtual User (VU).
   * **Pros & Cons**: Excellent scripting experience, rich ecosystem, and native support for HTTP/2, WebSockets, and gRPC. However, it suffers from **extremely heavy resource consumption**. CPU-intensive JS parsing and garbage collection (GC) pauses make single-machine scalability poor and introduce measurement noise.
3. **Vegeta (Go)**
   * **Designation**: High-precision constant rate (Constant RPS) load generator.
   * **Core Architecture**: Goroutine-based task scheduling.
   * **Pros & Cons**: Highly accurate rate limiting/shaping, readable output formats (JSON/CSV), and extensible Go library interface. However, it lacks support for state machines or advanced request scripting, uses rigid input schemas, and exhibits a relatively large RSS footprint under high concurrency due to Go runtime overhead.
4. **Locust (Python)**
   * **Designation**: Distributed user-behavior modeling and load testing framework.
   * **Core Architecture**: Gevent-based coroutine loop, allowing scenarios to be written fully in Python.
   * **Pros & Cons**: Unparalleled expressiveness for complex business logic and robust distributed orchestration. However, due to Python's inherent execution bottlenecks, **single-machine RPS throughput is extremely low** (typically only hundreds to a few thousand RPS per core), requiring substantial agent clusters to generate massive concurrency.
5. **Oha (Rust)**
   * **Designation**: An interactive, Terminal UI (TUI) driven load test utility in the Rust ecosystem.
   * **Core Architecture**: Built on top of the async tokio and reqwest stacks.
   * **Pros & Cons**: Outstanding real-time latency percentiles visualization on the console. However, due to the heavy network and async stack, its static RSS memory footprint and CPU cache locality are less optimized compared to raw bare-metal multi-plexing, and it lacks advanced log replay and pacing features.

---

### 1.2 Multi-Dimensional Comparison Matrix

| Feature / Metric | wrk / wrk2 | k6 | Vegeta | Locust | Oha | **rload (Current)** |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Language** | C | Go / JS | Go | Python | Rust | **Rust** |
| **Core I/O Engine** | epoll / kqueue | epoll (Go net) | epoll (Go net) | epoll (Gevent) | Tokio Epoll | **mio (Bare Non-blocking I/O)** |
| **Single-Host Throughput** | 🥇 **Extreme (100k+)** | 🥉 Low | 🥈 Medium | ❌ Very Low | 🥈 Medium | 🥇 **Extreme (wrk Parity)** |
| **Static Memory (RSS)** | **~3.5 MiB** | > 100 MiB | > 50 MiB | > 80 MiB | > 15 MiB | 🥈 **~3.55 MiB** |
| **Native Nginx Replay** | ❌ No (requires Lua) | ❌ No (heavy JS load) | ❌ No | ❌ No | ❌ No | 🏆 **Yes (Shuffle, Random, Filter)** |
| **Rate Pacing (Fixed RPS)** | 🥈 Yes (wrk2 only) | 🥇 Yes (Executors) | 🥇 Yes | 🥇 Yes | ❌ No | ⏳ **Scheduled for 0.2.0** |
| **Modern Protocols** | ❌ HTTP/1.1 Only | 🥇 H2, gRPC, WS | 🥈 Partial | 🥈 Partial | 🥈 Partial | ❌ HTTP/1.1 Only |
| **CI/CD Friendliness** | ❌ Poor (dynamic deps) | 🥈 Good | 🥈 Good | ❌ Poor (heavyweight) | 🥇 Good | 🏆 **Excellent (Single Static Bin)** |
| **Coordinated Omission** | 🥇 Yes | ❌ No | ❌ No | ❌ No | ❌ No | 🥇 **Yes** |

---

## 2. Rload Strategic Positioning (The "Sweet Spot")

Based on the multi-dimensional analysis, `rload` occupies a highly competitive and unique niche:

> **"High-fidelity production traffic reproduction with an ultra-lightweight footprint."**

### Core Competitive Advantages
1. **Zero-Overhead Production Log Replay**:
   Engineers frequently want to replay actual production traffic in staging/UAT environments. Doing this in `k6` or `Locust` by loading millions of log rows into a JS/Python runtime consumes gigabytes of memory and burns CPU on string parsing rather than network I/O. `rload` provides C-level execution speed with an incredibly lightweight log replay loader (**merely ~248 bytes of memory growth per parsed log entry**), making it the only utility capable of massive log replay with zero performance degradation.
2. **Perfect Fit for Cloud-Native CI/CD Performance Gates**:
   Performance regression verification is moving left. Running heavy tools like `k6` or `JMeter` on a micro-runner container (such as GitHub Actions or GitLab CI) is slow and memory-prohibitive. `rload` compiles into a **single, dependency-free static binary with microsecond start times and a tiny 3MB baseline memory footprint**. Performance regression test jobs can be launched and completed in seconds, even on the cheapest virtualized container nodes.

---

## 3. High-Value Future Feature Evaluation

To transition `rload` from a lightweight `wrk` replacement to a cloud-native traffic reproduction and chaos engineering engine, we prioritize the following high-impact features:

### 1. Timeline-Based Traffic Shaping and Waveform Reproduction
* **Value**: Production requests are not sent at constant rates; they contain peaks, troughs, and sudden spikes. Replaying traffic using the original timestamps recorded in Nginx logs (with adjustable multiplier speed, e.g. `--replay-speed 2.0`) allows engineers to conduct highly realistic capacity planning.
* **Feasibility**: High feasibility. Implementing microsecond-level time wheels or priority queues inside the `mio` reaction loop requires moderate efforts with virtually zero CPU overhead.

### 2. CI/CD Performance Assertion Gates (`--assert`)
* **Value**: Provides standard validation schemas in command execution or YAML configurations (e.g., `fail_on: p99 > 50ms || rps_drop_ratio > 5%`). It terminates the process with a non-zero exit code if performance falls below specified thresholds, immediately failing CI pipelines on performance regressions.
* **Feasibility**: Very easy. Requires reading assertions from CLI/YAML and performing rule evaluations on the accumulated `RunSummary` metrics.

### 3. HTTP/2 and Connection Multiplexing
* **Value**: More than 70% of modern internet traffic is HTTP/2, and internal microservice communications (such as gRPC) rely heavily on it. HTTP/2 enables multiplexing hundreds of parallel streams over a single TCP connection. Supporting HTTP/2 allows `rload` to mimic modern clients accurately and generate higher load with fewer socket resources.
* **Feasibility**: Moderate-High. Requires implementing a lightweight, zero-copy HTTP/2 frame pack/unpack state machine on top of the non-blocking TLS layers provided by `rustls`.

### 4. Real-time Metric Exporting (Prometheus & OpenTelemetry)
* **Value**: Combines client-side measurements (RPS, socket errors, latency percentiles) with server-side resource metrics (CPU/RAM) into centralized dashboards like Grafana.
* **Feasibility**: Easy. Launching an lightweight, non-blocking Prometheus scrapable endpoint or pushing metric streams to an OTel collector adds minimal overhead to the worker coordinator.

---

## 4. Four-Phase Technical Roadmap

We establish a concrete four-phase roadmap to drive `rload` towards its target positioning:

### Phase 1: v0.2.0 - Core Pacing, Stability, and Cross-Platform CI [Completed]
*Focus: Establish parity with industrial-grade traffic shaping tools and build robust cross-platform baselines.*

* **Key Deliverables**:
  1. **[x] Cross-Platform Actions**: Deploy fully automated GitHub Actions CI/CD to build, lint, and test across macOS, Linux, and Windows (validating path syntax, socket recovery, and PowerShell execution).
  2. **[x] Fixed Rate Pacing (Constant RPS)**: Integrate a high-precision pacing timer to maintain steady target throughput (e.g., exactly 10,000 RPS).
  3. **[x] Nginx Timestamp Replay**: Parse and reproduce the relative temporal gaps between requests based on Nginx log timestamps, supporting scaling factors (e.g. `--replay-speed 0.5` or `3.0`).
  4. **[x] Traffic Shaping Stages (Burst & Stage)**：Support programmatic load profile configurations, e.g., "Ramp up to 1000 RPS in 10s, hold 5000 RPS for 60s, and step-down to 100 RPS".

---

### Maintenance: v0.2.1 - Metrics and Replay Usability [Completed]
*Focus: Align byte accounting with wrk while improving real-log ingestion and
human-readable reporting without changing benchmark defaults.*

* **Key Deliverables**:
  1. **[x] Dual Byte Counters**: Retain decoded `response_body_bytes` and add
     wrk-compatible `read_bytes`, including bytes read before later failures.
  2. **[x] Tolerant JSONL Exports**: Ignore unknown fields, default missing or
     null methods to GET, and append the `args` query string safely.
  3. **[x] Optional Beauty Output**: Add `--output-beauty` while preserving the
     default text format and JSON schema version 1.

### Maintenance: v0.2.2 - Replay Cycles and JSONL Timestamp Pacing [Completed]
*Focus: Make finite replay workloads explicit and extend timestamp pacing from
Nginx access logs to structured JSONL exports without changing default replay
behavior.*

* **Key Deliverables**:
  1. **Replay cycle limit**: Add `--replay-rounds <N>` for replay inputs. One
     round means one complete traversal of the filtered request sequence;
     `N` must be a positive integer. Define and test its interaction with
     duration/request limits, sequential/shuffle/random order, `--seed`, and
     all pacing modes. Report configured and completed rounds where known.
  2. **JSONL timestamp pacing**: Allow `--replay-timestamps` with
     `--request-file` and a schema-defined timestamp format. Read
     `timestamp_micros` as the canonical field and accept `time` and `_time`
     as aliases. The schema supplies a strftime/chrono-style format (default
     Nginx format: `%d/%b/%Y:%H:%M:%S %z`), preserving fractional-second
     precision when requested. Define alias precedence and reject malformed,
     missing, or decreasing timestamps in timestamp mode.
  3. **Shared pacing validation**: Reuse access-log timestamp semantics for
     JSONL, including `--replay-speed`, sequential-order requirements, mutual
     exclusion with `--replay-rate`/`--replay-stages`, and zero delay at a
     cycle boundary. Preserve ordinary replay behavior when pacing is absent.
  4. **Compatibility and validation**: Keep unknown JSONL fields ignored,
     retain the existing method/args defaults, document the timestamp schema,
     and add parser, pacing, CLI, compatibility, and three-way benchmark
     regressions for both replay sources.
  5. **Schema-driven JSONL extraction**: Add an optional schema file whose
     mapping keys are all optional. An omitted mapping falls back to the
     standard top-level extraction logic for that field. The schema changes
     extraction paths only; it does not change record-value defaults or
     validation (`method`, `args`, `headers`, `body`, and required `uri`).
     Timestamp is optional for ordinary replay but required for
     `--replay-timestamps`.
     The schema owns timestamp format configuration; no timestamp-format CLI
     option is introduced.
  6. **Load-time materialization**: Adopt the load-time expansion design as a
     hard performance requirement. Parse and compile schema paths, resolve
     types, and parse timestamps while loading JSONL; materialize every record
     into the existing `ReplayRequest` representation. The request/response
     hot path must not retain dynamic JSON values, resolve schema paths, or
     parse timestamp strings.

### Maintenance: v0.2.3 - Generic Rate Stages and Version Output [Completed]

*Focus: Reuse staged global pacing for ordinary requests while preserving the
existing replay CLI.*

* **Key Deliverables**:
  1. **Generic staged pacing**: Add `--stages` for ordinary requests and both
     replay inputs while retaining `--replay-stages` as a compatibility alias.
  2. **Version discovery**: Add `--version` with stable stdout output suitable
     for installation checks and packaging scripts.

---

### Phase 2: v0.3.0 - CI-first Test Profiles and Reports [Stable release pending]
*Focus: Make an existing rload workload repeatable in local development and CI,
with machine-readable pass/fail results and an offline report.*

#### Release objective

A repository can commit a small `rload.yaml`, run a workload without rebuilding
the command line by hand, enforce latency/error budgets, and archive one
deterministic HTML report. The existing CLI and load engine remain the source of
truth.

#### Prioritized scope

1. **P0 — Declarative profile loading (`rload.yaml`)**: cover only capabilities
   already supported by v0.2.4: target, request/duration limit, threads,
   connections, timeout, ordinary request options, replay source/order/seed,
   filters, rounds, pacing, and output settings. Define explicit precedence:
   CLI overrides profile, profile overrides defaults.
2. **P0 — Typed assertions (`--assert`)**: evaluate a small grammar against the
   final `RunSummary`, including latency percentiles, throughput, error rates,
   status errors, socket errors, and completed requests. Failed assertions must
   have stable diagnostics and exit with status 1.
3. **P0 — Standalone HTML report (`--output-html`)**: render from the existing
   JSON result model into one deterministic, dependency-free file. It must not
   add work to the request/response hot path.
4. **P0 — Complete latency summary statistics**: add maximum, minimum, mean,
   and median latency to the aggregate result and human-readable reports. The
   median is the existing P50 measurement; preserve `p50_us` for compatibility
   while adding an explicit median field to make the summary self-describing.
   Define the same statistics for per-method summaries where those summaries
   are emitted.
5. **P0 — Failure-tolerant execution**: record every runtime failure without
   aborting the load run. Connection, read/write, timeout, TLS, and invalid
   response failures are counted with stable categories; the affected request
   or connection is isolated and the remaining workload continues. Startup
   validation and unreadable/malformed input remain fail-fast. Fixed-request
   runs must have bounded recovery so continuation cannot become an infinite
   retry loop.
6. **P1/stretch — Prometheus export**: implement only after the first five
   deliverables have stable contracts and a measured snapshot design. It is
   opt-in and must not be required for ordinary runs.

#### Milestones and gates

* **M1 — Contracts**: freeze profile v1, assertion grammar, metric units, JSON
  result compatibility, and HTML data contract.
* **M2 — Config vertical slice**: load a static and replay profile, validate
  cross-field conflicts, and prove CLI precedence with integration tests.
* **M3 — CI gate**: implement assertion evaluation, stable failure output, and
  pass/fail tests without changing measured load behavior.
* **M4 — Metrics and report**: add min/max/mean/median statistics, deterministic
  HTML output, and compatibility fixtures.
* **M5 — Failure tolerance**: exercise connection refusal, timeout, reset,
  write/read failure, TLS failure, malformed response, recovery, and bounded
  retry behavior across duration and request-count runs.
* **M6 — Release**: run the existing three-way benchmark and cross-platform
  checks, then publish `v0.3.0`.

#### Acceptance criteria

* All v0.2.4 CLI forms, text output, JSON schema v1, and replay semantics remain
  compatible.
* Invalid profiles identify the field and reject invalid combinations before
  network or workload execution.
* YAML and CLI assertions evaluate the same typed metrics.
* Assertion failures return non-zero without changing the generated load.
* Aggregate and per-method results expose min/max/mean/median latency in the
  documented unit; median and `p50` are numerically equivalent within the
  histogram's documented precision.
* Runs with no completed latency samples use the documented unavailable value
  rather than fabricating a latency measurement.
* Runtime failures never terminate an otherwise valid run; every failure is
  represented in the summary, while startup/configuration/input errors remain
  fail-fast.
* Fixed-request runs have a documented maximum retry/recovery budget or a
  defined completion rule, so an unavailable target cannot cause an endless
  run.
* Identical JSON input produces byte-stable HTML output usable offline.
* No more than 3% throughput or 5% P99 regression in the fixed baseline; replay
  RSS remains within the existing documented gate.

`v0.3.0-rc.1` passed the local package gate, three-way benchmark, replay RSS
validation, and Linux/macOS/Windows CI on 2026-07-19. The stable `v0.3.0` tag
is pending publication. HTTP/2, gRPC, distributed execution, and Prometheus
remain deferred.

#### Non-goals

HTTP/2, gRPC, Lua/LuaJIT, distributed execution, target inference, scripted
hooks, TUI/GUI, and a mandatory Prometheus deployment are deferred.

---

### Phase 3: v0.4.0 - Cloud-Native Multiplexed Protocols & Web Companion
*Focus: Expand into modern cloud-native protocols and prototype the decoupled web helper.*

* **Key Deliverables**:
  1. **HTTP/2 Frame Engine**: Develop a zero-copy, stream-multiplexed HTTP/2 decoder/encoder state machine over non-blocking TLS sockets.
  2. **gRPC Stress Testing**: Support replaying structured JSONL logs as gRPC protobuf payloads over multiplexed HTTP/2 streams, enabling native gRPC performance validation.
  3. **Embedded Web Sidecar Companion (rload web)**: Embed a micro-web server hosting a lightweight, glassmorphic Single Page App (SPA) as static bytes. Provide an optional `--web` or `rload web` subcommand displaying live performance graphs from the browser.

---

### Phase 4: v1.0.0 - Interactive Dashboards and Target Autopilot
*Focus: Deliver an outstanding user experience, native desktop companions, and adaptive log-replay targeting.*

* **Key Deliverables**:
  1. **Dynamic Target Autopilot (Target Inference)**: Automatically infer `$scheme` and `$host` headers directly from replayed logs, allowing dynamic dispatching across multiple targets in mesh networks instead of restricting load to a single target URL.
  2. **Decoupled Desktop Client (rload-studio)**: Launch a standalone cross-platform GUI built with **Tauri**. It wraps the core `rload` CLI binary inside its bundle, communicates over non-blocking JSON standard pipes, and provides visual drag-and-drop log parsing, validation, and historical run regression charting.

### Future candidate: Scripted Request/Response Hooks

Provide an optional scripting language for workload preparation and result
processing without placing an interpreter on the request/response hot path:

```text
request-prepare(request) -> [pre-script filter] -> core ->
response-result(response) -> [post-script filter]
```

This feature is explicitly performance-gated and is not scheduled for the
0.2.x or 0.3.x release lines. The design must provide a zero-cost disabled
path, bounded pre-processing queues or precomputed request data, isolated
post-processing, and benchmark evidence showing no regression in unscripted
mode. The script runtime, sandboxing, failure policy, determinism, and whether
hooks run per request or in batches require a separate architecture and
security review before implementation.
