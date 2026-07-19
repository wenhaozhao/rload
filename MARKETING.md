# rload 推广发布包

## 核心定位

面向需要压测真实流量的 Rust、SRE 和后端工程师：rload 保留 wrk 兼容的命令行体验，同时原生重放 Nginx access log 与 JSONL 请求序列，让线上真实请求可以在 staging、回归测试和容量评估中复现。

一句话：**用真实流量重放，而不是只压一个 URL。**

## 首轮目标（30 天）

- GitHub：50 个 star、10 个真实 issue/讨论、3 个外部使用反馈。
- 安装：访客在 60 秒内完成安装并跑通第一条命令。
- 内容：发布 3 篇短文，覆盖 wrk 迁移、Nginx 日志重放、CI 性能回归。

## GitHub Release 文案

### rload v0.3.0-rc.1 — Reproducible load validation.

rload is a Rust HTTP/1.1 load generator with wrk-compatible CLI semantics,
native Nginx access-log replay, structured JSONL request replay, versioned YAML
workloads, CI assertions, and deterministic offline HTML reports.

```sh
cargo install rload
rload --requests 1000 https://example.com/
rload --duration 30s --access-log ./access.log --replay-order shuffle --seed 42 https://staging.example.com/
rload --profile rload.yaml --assert 'p95 < 50ms'
```

## 社交媒体短文

如果你还在用一个固定 URL 代表全部线上流量，可能会错过真正的缓存、队列和尾延迟问题。rload 是一个 Rust HTTP 压测工具：兼容 wrk CLI，并能原生重放 Nginx access log 和 JSONL 请求序列。把真实请求带到 staging，复制流量形状，再看系统真正的极限。

Most load tests reduce production traffic to one URL. rload keeps wrk-compatible CLI semantics while replaying Nginx access logs and structured JSONL requests natively. Bring production-shaped traffic to staging and find the real bottleneck with a small Rust binary.

## 发布顺序

1. 已发布 v0.3.0-rc.1 Release，附安装方式、验证报告和可复制命令。
2. 发布中文文章：`用 YAML profile 和 Nginx access log 验证真实流量`。
3. 发布英文文章：`Reproducible load validation with YAML profiles and CI assertions`。
4. 在 Rust、SRE、性能工程社区用可复现实验参与讨论，并明确该版本为 RC 发布。
