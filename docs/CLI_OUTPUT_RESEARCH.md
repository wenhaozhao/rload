# rload CLI 文本输出可读性研究

日期：2026-07-13  
范围：只研究默认 text 输出；不修改实现。机器可读输出（`--output-format json`）继续作为脚本和 CI 的稳定接口。

## 结论摘要

当前输出的信息覆盖面已经比 wrk 更完整，但字段连续打印、分组不明显，且同一概念使用了多种命名（例如 `Requests/sec`、`Load requests/sec`、`Configured replay rate`）。建议在 0.2.1 设计一个“人类可读、脚本可迁移”的文本布局：

1. 保留现有核心锚点（`Requests/sec:`, `Latency Distribution`, 百分位行），避免破坏现有基准解析器。
2. 加入固定的 `Run`、`Throughput`、`Latency`、`Errors`、`Breakdowns` 五个区块，并在区块之间留空行。
3. 所有计数使用右对齐、速率统一 `req/s`、字节同时显示原始整数和 IEC/十进制人类单位；延迟固定使用一个单位或同时打印原始微秒。
4. 默认只展示有意义的错误/回放行；成功路径不输出空错误区块。
5. 任何需要给脚本消费的场景继续推荐 JSON；文本格式只承诺稳定的字段名和列顺序，不承诺自由文本句子。

## 现状与问题

默认文本由 [`src/main.rs:370-474`](../src/main.rs#L370) 直接顺序打印。它把总请求、响应体字节、平均延迟、百分位、两种 RPS、时间窗口、replay 配置、错误、method、状态码和 URI 混在一条长列表中。主要可读性问题：

- 没有运行上下文标题：线程数、连接数、目标 URL 和测试模式不会在结果中复述；用户只能回看命令行。
- 吞吐和时间字段分散在 `Requests/sec`、`Load requests/sec`、`Load window`、`Drain time` 等行，难以区分“整个运行窗口”和“产生负载窗口”。
- `response body B read` 的 `B` 容易被理解成速率或 socket 总读取量；0.2.1 计划还要永久加入 `read_bytes`，两者应成对展示。
- Rust 的 `Duration` 调试格式（`{:.2?}`）会根据数量级切换 `ms`、`s` 等单位；对人友好，但复制、比较和脚本解析不稳定（见 [`src/main.rs:374-401`](../src/main.rs#L374)）。
- Method 行把“requests”“errors”“average”塞在一行，列间距依赖英文单词长度；状态统计和 URI 统计也没有明确的列标题（[`src/main.rs:449-472`](../src/main.rs#L449)）。
- 条件行只在非零时出现（状态错误、socket 错误、过滤和跳过），输出短但不同运行的行集合不同，不利于人工 diff。

## wrk 作为兼容基准

官方 wrk 源码在仓库 [`../src/wrk.c`](../../src/wrk.c) 中提供了值得保留的布局约定：

- 先打印 `Running ...` 和线程/连接数（[`wrk.c:138-139`](../../src/wrk.c#L138)）。
- 用表头 `Thread Stats Avg Stdev Max +/- Stdev`，统计行按固定列宽对齐（[`wrk.c:546-572`](../../src/wrk.c#L546)）。
- 汇总行把请求数、运行时间和读取字节放在一行，再打印 `Requests/sec` 与 `Transfer/sec`（[`wrk.c:178-191`](../../src/wrk.c#L178)）。这也是为什么 rload 的响应体字节不能直接和 wrk 的 `read` 比较：wrk 的 `thread->bytes` 是 socket 读取总量（[`wrk.c:141-155`](../../src/wrk.c#L141)）。
- 延迟百分位单独成组，百分位和数值固定右对齐（[`wrk.c:575-584`](../../src/wrk.c#L575)）。

rload 不应复制 wrk 的“单一吞吐指标”限制，因为 rload 还支持 drain、回放倍率、stages、跳过统计和 method/status/URI breakdown；但应采用相同的标题、空行、固定列宽和右对齐原则。

## 推荐布局（设计草案）

建议默认输出如下结构；名称是建议值，最终实现可保持兼容别名：

```text
Running 10s test @ http://127.0.0.1/index.html
  2 threads, 100 connections

Summary
  Requests completed:       470,782
  Read bytes:               258,123,456 (246.23 MiB)
  Response body bytes:      145,000,856 (138.28 MiB)
  Runtime:                       10.00s
  Load window:                  10.00s
  Drain time:                    2.58ms

Throughput
  Requests/sec:             47,066.06
  Load requests/sec:        47,078.20
  Transfer/sec:             24.62 MiB/s

Latency
  Average:                       2.44ms
  Percentile       Latency
       50%           1.95ms
       75%           2.53ms
       90%           3.40ms
       99%           5.62ms

Errors
  HTTP status errors:             0
  Socket errors:                  0 (connect 0, read 0, write 0, timeout 0)

Breakdowns
  Method          Requests    Errors    Avg latency
  GET               470,782         0         2.44ms
  HTTP status      Responses
  200               470,782
```

关键点：

- `Requests/sec:` 保留在行首，继续满足 [`benchmarks/accuracy.py:48`](../benchmarks/accuracy.py#L48) 的正则；平均延迟仍保留 `Average latency:` 兼容行，或让解析器接受 `Latency / Average` 别名后再迁移。
- 百分位行继续以 `50%`、`75%`、`90%`、`99%` 开头，保持 [`benchmarks/accuracy.py:45-47`](../benchmarks/accuracy.py#L45) 的配对基准可用。
- 新增列标题和逗号分隔数字只影响人读，不改变数字含义；若基准脚本严格匹配裸数字，应先扩展解析器再切换格式。
- 错误区块可固定输出四类计数，即使全为零；这让 CI 日志 diff 更稳定。若希望保持短输出，可提供 `--verbose` 显示零值区块。

## 单位与命名建议

| 类别 | 建议 | 原因 |
| --- | --- | --- |
| 计数 | `470,782`，JSON 保持整数 | 千位分隔提高可读性；避免把 `k` 当精确值 |
| 读取量 | `read_bytes` + `MiB`（可选十进制 MB） | 与 wrk 语义对齐，同时保留精确值 |
| 响应体 | `response_body_bytes` | 业务有效载荷，不能与 socket read 混淆 |
| 速率 | `47,066.06 req/s` | 显式单位，避免 `Requests/sec` 与 `Load requests/sec` 视觉混淆 |
| 延迟 | 默认 `ms`，极小值使用 `us`；同一段内不混用 | 便于横向比较；JSON 仍使用 `_us` |
| 时间 | `10.00s`、`2.58ms` | 保留两位小数，避免 Debug 格式变化 |

## 兼容性与实现边界

- `--output-format json` 是正式自动化接口；其 schema v1 字段不可因文本美化而改名（当前结构见 [`src/main.rs:513-556`](../src/main.rs#L513)）。
- 文本输出可以增加标题、空行、列标题和新字段，但不要删除 `Requests/sec:`、`Average latency:`、百分位行等现有锚点，直到基准脚本完成迁移。
- 建议增加内部 `print_text_summary()`，让计算和格式化分离；所有数字先在结构体中计算，再由 formatter 输出，避免在 `println!` 中重复除法和单位选择。
- 建议为 formatter 增加 golden tests：成功请求、错误请求、回放统计和零请求各一份；测试应断言关键标签和列，而不是完整字符串，以允许列宽微调。
- README/官网应展示新的完整输出，并说明文本用于交互阅读、JSON 用于 CI/脚本。

## 分阶段建议

1. 0.2.1：只加入区块、空行、列标题及 `read_bytes`/`response_body_bytes` 双字段；保留兼容锚点，更新 golden tests 和 benchmark parser。
2. 0.2.1 后续：统一单位 formatter、千位分隔和 `req/s` 标签；parser 同时接受新旧格式。
3. 后续主版本：若需要，可增加 `--output-format text-compact` 或 `--no-color`；不要把 ANSI 颜色放入默认输出，以免污染日志和正则。

