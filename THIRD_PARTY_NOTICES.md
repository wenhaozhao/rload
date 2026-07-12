# Third-party notices

Rload is distributed under `MIT OR Apache-2.0` for its own code.

Runtime dependencies are consumed through Cargo and retain their own licenses
and notices in their upstream packages. The release crate does not include the
original C/Lua wrk implementation or its historical `NOTICE` file; rload is a
Rust implementation with wrk-compatible command semantics, not a binary
redistribution of wrk.

Before adding copied source from another project, record its copyright and
license here and include the required notice in the release package.
