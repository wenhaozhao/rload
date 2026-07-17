# rload 0.3.0 development validation — 2026-07-17

## Local release gate

The required local release gate completed successfully on macOS arm64:

- `cargo fmt --check`
- Clippy for all targets with warnings denied
- all library, binary, CLI, and HTTP integration tests
- release build
- crate packaging and packaged-crate verification
- license and third-party-notice checks

The test suite covered 43 library tests, one binary test, 50 CLI tests, and
31 HTTP integration tests.

## Runtime-failure coverage

The current development branch validates the following fixed-request recovery
behaviour:

- an unavailable target is retried only within a bounded initial connection
  budget, then the remaining requests are reported as abandoned;
- an asynchronous connection failure abandons unfinished fixed requests when
  no next address is available;
- TLS certificate failures remain terminal configuration/runtime TLS failures;
- TLS-handshake I/O failures may advance to a subsequent resolved address.

`abandoned_requests` is exposed in the text, beauty, JSON, HTML, and assertion
outputs, including profile assertions.

## Result and remaining gates

PASS for the local release gate. This is not release sign-off: the v0.3.0
release still requires the cross-platform CI matrix, a three-way throughput and
latency benchmark, replay RSS validation, final version and documentation
alignment, and release-candidate preparation.
