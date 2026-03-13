# AGENTS.md

## Project Overview

This will be a Rust implementation of the [Accord](https://cwiki.apache.org/confluence/download/attachments/188744725/Accord.pdf)
consensus algorithm. It will form the foundation of a distributed OLTP database, released as open
source.

## Style Guide

### Documentation

* Modules should have a brief module-level doc comment with a general overview of its purpose.
* Important internal invariants should be documented on fields/params with `INVARIANT:` prefix.
* All data structures and APIs should have concise and clear doc comments, even internal ones.
* References to fields or functions should use rustdoc syntax, e.g. [`Struct::field`].

### APIs

* Only use pub visibility for items that must be exposed to crate users. Default to private, and
  use pub(crate) for private items that must be exported to the rest of the crate.
* Only use panic/unwrap/expect for internal invariants, never for input validation. Using it for
  cases that will never happen in practice is okay (e.g. certain u64 overflow scenarios).
* Don't use getters/setters for struct fields, unless it's an API exposed outside of the crate and
  the field must be exposed read-only or mutated while enforcing invariants.

### Imports

* Prefer `use` over qualified paths unless there are name conflicts.
* Always import as `use` at the module level, not scope level.

### Syntax

* Use a 100 column text width, even for comments.

## Methodology

* Favor incremental steps that keep the crate compiling.
* Don't write new tests until you're explicitly asked to.

## Verification

Run standard Rust checks after substantive changes:

```bash
cargo +nightly fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```
