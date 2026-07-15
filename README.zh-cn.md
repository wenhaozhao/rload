# rload

[English](./README.md) | 中文

`rload` 是一个 Rust 编写的 HTTP 压测/负载生成工具，具有与 wrk 兼容的 CLI 语义、Nginx 访问日志回放以及结构化 JSONL 请求回放功能。

官方网站: [wenhaozhao.github.io/rload](https://wenhaozhao.github.io/rload/)

当前的垂直切片（vertical slice）提供了：

- 使用固定工作线程，通过 HTTP 或 HTTPS 发送非阻塞 HTTP/1.1 GET 请求；
- TLS SNI、Mozilla 根证书验证以及加密连接复用；
- 按照顺序、循环回放来自 Nginx 通用或组合访问日志的 `GET` 和 `HEAD` 请求；
- 精确的单方法延迟/错误摘要和 HTTP 状态码计数；
- 有界限的 URI 前 20 位重度访问（heavy-hitter）估算，并附带明确的最大误差；
- 具有请求数或持续时间限制的持久连接；
- 在持续时间受限的运行期间进行套接字错误统计和连接恢复；
- `Content-Length`、分块（chunked）和连接关闭（connection-close）响应分帧；
- 输出已完成的请求数、套接字读取字节数、响应体字节数、状态错误和平均延迟。

Lua 和 LuaJIT 兼容性被明确声明为不在此项目范围内。

## 与 wrk 的对比

对比基线为 wrk 4.2.0。下述性能数据来自在 macOS arm64 上使用相同 URL、服务器、线程数、连接数和持续时间的配对运行。这些数据是该环境下的回归证据，并不保证任何一个客户端在每台机器上都能达到相同的吞吐量。

### 性能与准确性

| 维度 | wrk 基线 | rload 结果 | 评估 |
|---|---|---|---|
| 吞吐量 | 参考客户端 | 在 10/100/400 连接下的 15 次配对运行中，RPS 平均绝对误差（MAE）为 0.986% | 在 3% 判定门槛内等效 |
| 平均延迟 | 参考客户端 | MAE 1.171% | 在 3% 判定门槛内等效 |
| P50 / P75 延迟 | 参考百分位数 | MAE 1.064% / 0.961% | 在 3% 判定门槛内等效 |
| P90 延迟 | 参考百分位数 | MAE 0.944% | 在 5% 判定门槛内等效 |
| 带有 1 毫秒延迟 + 确定性抖动的 P99 | 参考百分位数 | 中位数绝对误差 0.567% | 通过 5% 判定门槛 |
| 零延迟回环 P99 | 对调度器噪声敏感 | 中位数绝对误差 5.128% | 略微超过 5% 的门槛；不声称无条件对等 |
| 静态路径 RSS | 参考进程 | 全新 100 连接运行：wrk ~3.47 MiB, rload ~3.55 MiB | 在测量的运行中具有可比性 |
| 访问日志回放吞吐量 | 无原生访问日志回放功能 | 100k: +1.68%; 500k: -1.81%（相比于配对的静态运行） | 回放开销保持在配置的 10% 判定门槛内 |
| 访问日志回放内存 | 无原生访问日志回放功能 | RSS 增长斜率为每个加载条目 248.7 字节 | 通过了配置的 0–256 字节/条目的判定门槛 |

正式的准确性判定方法、原始结果目录以及判定门槛定义记录在 [`benchmarks/VALIDATION_2026-07-11.md`](benchmarks/VALIDATION_2026-07-11.md) 和 [`benchmarks/ACCURACY.md`](benchmarks/ACCURACY.md) 中。零延迟 P99 结果被有意指出，而非四舍五入为通过。

### 功能覆盖范围

| 能力 | wrk 4.2.0 | rload 0.2.2 |
|---|---|---|
| HTTP/1.1 静态请求负载 | 支持 | 支持 |
| 具有连接复用的 HTTP 和 HTTPS | 支持 | 支持，包括 TLS 验证和 SNI |
| 工作线程、连接数、持续时间、请求数 | 支持 | 支持；核心 CLI 形式兼容 |
| 超时和延迟报告 | 支持 | 支持；接受 `--latency` 参数并且总是打印延迟 |
| HTTP 状态和套接字错误统计 | 支持 | 支持，分为 connect/read/write/timeout 类别 |
| 类 Curl 方法、请求头和请求体 | 通过 Lua 脚本支持 | 支持，适用于文档中记录的类 curl 兼容子集 |
| Lua/LuaJIT 请求脚本编写 | 支持 | 在第一个版本系列中特意不予支持 |
| Nginx 访问日志回放 | 无原生模式 | 支持；通用/组合日志，`GET`/`HEAD`，顺序/洗牌（shuffle）/随机顺序；跳过不支持的方法并报告 |
| JSONL 请求回放 | 无原生模式 | 支持；方法、请求头、UTF-8 请求体以及单记录限制 |
| 回放种子与方法/URI 白名单 | 无原生模式 | 支持；确定性种子外加交集过滤器 |
| 回放频率/时间戳步调（pacing）/突发配置 | 仅通过自定义脚本支持 | 已实现固定全局速率、时间戳速度步调（timestamp-speed pacing）以及定时速率阶段 |
| 从访问日志条目自动推导目标 | 无原生模式 | 仅作为未来候选特性；目标 URL 目前需显式指定 |
| GUI 配置界面 | 无原生模式 | 未来可选特性，分层在 rload 引擎之上 |

本项目的目标是有意打造一个与 wrk 兼容的负载生成器，而不是作为每个 wrk 扩展的无缝替代品：核心命令行行为和静态 HTTP 负载得到了覆盖，而 Lua 兼容性超出了首个版本的发布范围。额外的回放模式是 rload 提供的主要功能扩展。未来的 GUI（如果构建的话）将是一个在同一引擎之上的配置和可观测性层；它不会替代独立的 CLI，也不会复制负载生成逻辑。

## 构建与安装

0.2.x 版本系列已在 macOS arm64、Linux 和 Windows 上使用稳定的 Rust 1.96.1 进行了验证。Windows CI 还额外覆盖了 PowerShell 调用、路径处理和套接字恢复。

直接从此签出目录构建或安装：

```sh
cargo build --release
cargo install --path .
rload --help
```

当已经处于 crate 目录中时，可以省略 manifest/path 前缀。
Rload 采用 `MIT OR Apache-2.0` 许可；请参阅 `LICENSE-MIT` 和 `LICENSE-APACHE`。第三方属性声明在依赖项要求时单独记录。

## 使用方法

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

普通请求使用类 curl 兼容子集，同时保留 wrk 的选项含义：`-X/--request`、可重复的 `-H/--header`、可重复的 `--data` 以及 `--data-binary @FILE`。短选项 `-d` 仍代表 wrk 的持续时间选项；它从不表示请求数据。多个 `--data` 值会使用 `&` 连接，而在不指定 `-X` 的情况下指定数据会选择 POST。二进制文件会以字节对字节（byte-for-byte）的方式发送。JSONL 所使用的托管请求头、URI 以及 512 KiB 请求体限制同样适用于此处。

常见的 wrk 命令行依然有效：接受紧凑的 `-t2`、`-c100`、`-n1000`、`-d30s` 和 `-T2s` 格式；时间支持纯数字秒数以及 `s`、`m`、`h`；`-T/--timeout` 控制连接/请求超时，并且 `--latency` 作为兼容性标志被接受（延迟分布总是会被打印）。在没有显式负载选项的情况下，wrk 兼容的默认值是：使用两个工作线程和十个连接运行 10 秒。

在持续时间受限的运行中，请求超时、重置或提前 EOF 会被计为套接字错误，并且只要时间允许，受影响的连接就会被重建。请求数受限的运行在发生这些故障时仍会返回错误，从而使永久不可用的目标不会导致有限的运行无限期等待。套接字错误分为 wrk 风格的 `connect`、`read`、`write` 和 `timeout` 类别进行报告；故障和恢复被隔离到受影响的连接。如果受限制持续时间的目标在整个运行期间都无法响应，则在配置的持续时间过期后，该命令仍会返回一个包含零个已完成请求和累计超时数的有效摘要。当每次连接尝试都被拒绝时，也适用相同的有界行为：这些尝试会被计为 `connect` 错误，并且运行在达到持续时间限制时正常结束。如果目标在限制之前再次可用，受影响的连接会在不重启负载测试进程的情况下恢复发送请求。

访问日志回放会读取带引号的 Nginx `$request` field，保留其原始格式的 URI（包括查询字符串），并在日志中按顺序循环，直到达到请求数或持续时间限制。空日志和格式错误的请求行会失败并报告源行号。除了 `GET` 或 `HEAD` 之外的方法会被跳过，并在最终摘要中通过总计数器和特定方法计数器进行报告；它们不会被发送，也不包含在请求延迟/吞吐量统计数据中。访问日志模式不支持请求体。

默认的回放顺序为 `sequential`（顺序）。`--replay-rate <RPS>` 会在所有回放工作线程中应用一个全局请求速率，并报告配置速率和测量速率。`shuffle`（洗牌）在每轮中精确访问每个条目一次，并在下一轮开始前重新洗牌；`random`（随机）在每次请求时独立对一个条目进行抽样，可能导致重复条目。`--seed` 可使任何随机分配序列具有可重现性。在使用多个连接时，分配序列保持确定性，但网络到达顺序可能会有所不同。

`--replay-rounds <N>` 将过滤后的顺序或洗牌序列完整回放 `N` 轮，不能与 `--requests`、`--duration` 或 `random` 回放顺序组合。

`--replay-timestamps` 保留相邻 Nginx 访问日志或 JSONL 时间戳之间的间隔。第一个请求会立即发送，而 `--replay-speed <N>` 会缩放随后的间隔（`2` 表示两倍速，`0.5` 表示半速）。访问日志接受标准 `$time_local` 值以及最高微秒精度的分数秒。JSONL 可以通过可选的 request schema 定义时间格式；未提供 schema 时，从顶层 `timestamp_micros`、`time` 或 `_time` 字段提取，并接受默认 Nginx 与 RFC3339 格式。时间戳模式要求顺序回放，并且与 `--replay-rate` 互斥；缺失或递减的时间戳将被拒绝。循环回放时不会添加输入中不存在的跨轮间隔。

`--stages <DURATION:RPS,...>` 为普通请求或回放输入定义定时速率配置，例如 `--stages 10s:100,5s:1000,10s:100` 表示基线、峰值和恢复三个阶段。阶段转换发生在配置的边界上；在配置结束之后，最后的速率将保持活跃。阶段可与顺序、洗牌或随机回放选择以及任一回放输入格式一起使用，并与 `--replay-rate` 和 `--replay-timestamps` 互斥。现有 `--replay-stages` 继续作为回放输入的兼容别名；两个名称不能同时使用。

使用 `--version` 输出已安装的 rload 版本并退出。

### JSONL request schema

`--request-schema <FILE>` 为嵌套 JSONL 记录配置字段抽取路径。顶层 `fields` 对象及其中每个映射都是可选的；省略时，该字段继续使用当前顶层字段抽取逻辑。schema 只改变抽取路径，不改变字段默认值或现有校验规则。

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

字段路径使用点分隔对象字段，当前不支持数组索引和表达式。时间格式使用 strftime/chrono 占位符；未指定格式时同时支持 Nginx 格式 `%d/%b/%Y:%H:%M:%S %z` 和 RFC3339（例如 `2026-07-03T08:41:17Z`）。也可以通过 `%+` 显式指定 RFC3339。request schema 仅在需要自定义字段路径或格式时使用。schema 和时间字符串仅在文件加载阶段解析，压测热路径只使用物化后的请求及微秒时间戳。

### 机器可读的结果

在将 rload 与 CI、仪表板或计划中的 GUI 集成时，请使用 `--output-format json`：

```bash
rload -t2 -c100 -d30s --output-format json http://127.0.0.1/ > result.json
```

JSON 输出是标准输出上的一个文档，其 `schema_version` 为 `1`。持续时间和延迟使用整型微秒。它包括总吞吐量、延迟百分位数、HTTP 状态和方法统计、套接字错误、URI 前 20 位估算、过滤/跳过的回放记录以及激活的固定速率、时间戳或阶段步调配置。配置和运行时错误仍保留在标准错误中，并返回非零退出状态。默认的 `text`（文本）格式保持不变。

如需输出具有明确分节的、人类可读的报告，请使用 `--output-beauty`。此模式与 JSON 输出互斥，且不会改变基准测试脚本所使用的现有默认文本解析锚点（parser anchors）：

```bash
rload -t2 -c100 -d30s --output-beauty http://127.0.0.1/
```

文本和 JSON 摘要均会暴露两个字节计数器。`read_bytes` 统计成功读取并交给 HTTP 解析器的每个响应字节，包括请求头、解码后的响应体输入以及分块分帧。`response_body_bytes` 仅统计解码后的响应有效负载。在发生后续套接字故障前读取的字节仍包含在 `read_bytes` 中。

URI 前 20 位计数使用有界 Space-Saving 估计算法。对于每个条目，真实的请求计数在 `estimated_requests - maximum_error` 和 `estimated_requests` 之间；因此，报告的误差是单侧最大多算，而非对称置信区间。

结构化请求回放接受每行一个 JSON 对象。它特意兼容了导出的应用程序日志：未知的字段会被忽略，而在省略或为 `null` 时，`method` 默认使用 `GET`：

```json
{"method":"POST","uri":"/api/items","args":"source=web","headers":{"content-type":"application/json"},"body":"{\"id\":1}","extra_log_field":"ignored"}
```

支持的方法包括 `GET`、`HEAD`、`POST`、`PUT`、`PATCH`、`DELETE` 和 `OPTIONS`。请求体为 UTF-8 字符串。当存在 `args` 时，它会作为查询字符串附加到 `uri`。允许带有前导 `?`；当 `uri` 没有查询字符串时，前导 `&` 会保留为 `?&`，而无论带有哪种前导前缀，在与已有查询字符串合并时都会使用 `&` 进行连接。`Host`、`Connection` 和 `Content-Length` 由引擎管理，不能出现在 JSON 请求头中。JSONL 和访问日志输入是互斥的；两者都支持相同的回放顺序选项。每个 JSONL 记录限制为 1 MiB，其中包含最大 8 KiB 的 URI、64 KiB 的请求头和 512 KiB 的 UTF-8 请求体。`Transfer-Encoding`、`Trailer` 和 `Expect` 也会被拒绝，因为此版本发送固定长度的请求体，而不进行 HTTP/1.1 continue 握手。

可以使用方法和 URI 白名单来缩减回放输入：

```sh
--allowed-methods GET,POST --allowed-uris '/api/*,/health'
```

URI 模式使用小型的确定性 glob 语法，其中 `*` 匹配任何字符序列，其他所有字符均为字面量。方法和 URI 过滤器形成一个交集。被过滤的条目会计入摘要，而排除了全部输入的白名单会报错。白名单选项在普通的单个请求中无效。最多可提供 32 个 URI 模式，每个模式长度不超过 256 字节，这限制了大型日志的通配符匹配工作量。

### 未来候选特性

下述能力被记录用于后续评估，并不属于当前实现或验收范围：

- 针对显式记录了协议（scheme）、主机和端口的自定义 Nginx 日志格式的目标自动推导；这仍是未来候选特性，尚未安排在 0.2.0 中。

通过以下命令测量相对于静态请求路径的回放开销：

```sh
ENTRIES=100000 CONNECTIONS=100 DURATION=5 ./benchmarks/replay.sh
REPLAY_ORDER=shuffle SEED=42 ./benchmarks/replay.sh
./benchmarks/replay_matrix.sh
```

可重复的基准测试使用至少三次具有交替顺序的配对运行，并报告中位数吞吐量损失、其范围、总 RSS 增长以及每个加载日志条目的字节数。该矩阵还额外验证了 100k 到 500k 条目之间的 RSS 斜率。当前判定的门槛是最多 10% 的吞吐量损失，以及每个条目介于 0 到 256 字节之间的增量 RSS。

2026-07-11 的顺序回放验收矩阵通过了这两个尺度的测试。在 100k 条目下，中位数吞吐量差异为 +1.68%，增量 RSS 为 252.5 字节/条目。在 500k 条目下，对应的吞吐量差异为 -1.81%，测得的 RSS 增长斜率为 248.7 字节/条目。这些本地结果是回归证据，而非跨平台内存保证。

运行测试与 linter：

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

运行完整的本地发布判定门槛（release gate）：

```sh
./scripts/release-check.sh
```

## 基准测试

使用以下命令运行针对 wrk 的可重复本地对比测试：

```sh
./benchmarks/run.sh
```

通过环境变量覆盖 `DURATION`、`THREADS`、`CONNECTIONS` 或 `RUNS`。`DELAY_US` 增加了一个固定的服务器延迟，而 `JITTER_US` 增加了确定性的均匀抖动，从而使尾部延迟（tail-latency）对比具有可重复性。原始命令输出、CPU 时间、最大 RSS 以及环境细节将写入 `benchmarks/results/` 下。

使用以下命令分析一个或多个结果目录：

```sh
python3 benchmarks/accuracy.py benchmarks/results/<timestamp> [...]
```

检查器将报告配对的相对偏差、平均绝对误差、标准差、95% 置信区间以及范围。它对吞吐量和中心延迟强制执行 3% 的 MAE（平均绝对误差），对 P90 强制执行 5% 的 MAE，对 P99 强制执行 5% 的中位数绝对误差。要求至少进行三次配对运行。有关具体方法，请参见 [`benchmarks/ACCURACY.md`](benchmarks/ACCURACY.md)。
