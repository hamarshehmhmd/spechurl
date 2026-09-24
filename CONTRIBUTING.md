# Contributing to spechurl

Thanks for helping spechurl stay small and correct. The tool converts an OpenAPI document into Hurl files and checks that those files still match. Changes should keep that output deterministic.

## Setup

Install Rust 1.85 or newer. The repository pins a toolchain in `rust-toolchain.toml`; `rustup` will install it on the first `cargo` command.

```bash
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

`cargo test` is the bar for a change. Add a fixture under `fixtures/` when the behavior depends on a realistic spec, and a focused `generate_from_str` case in `tests/generate.rs` when it is a single edge.

## What to change

- Parser and renderer live in `src/`. `generate` and `check` must keep agreeing: `check` compares the directory to an in-memory `generate`, so do not special-case one command.
- Sort object keys and file names. Two runs on the same spec must be byte-for-byte equal aside from newline normalization (`\r\n` is accepted by `check`).
- Prefer a clear error over a partial file. Spec problems name the operation (`POST /pets`) and the `$ref` when there is one.
- Keep v0.1 inside OpenAPI → Hurl contract suites. Security schemes, callbacks, webhooks, and live servers are out of scope until a later version says otherwise.

## Pull requests

1. Fork the repository and branch from `main`.
2. Describe the user-visible change in the pull request. Link an issue when there is one.
3. Update `CHANGELOG.md` under `Unreleased` when the behavior changes.
4. Make sure `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` pass.

## Reporting issues

Use the bug and feature templates. Security reports go through [GitHub security advisories](https://github.com/hamarshehmhmd/spechurl/security/advisories/new), not public issues. See [SECURITY.md](SECURITY.md).

This project is released under the [MIT license](LICENSE). By contributing, you agree that your contributions will be licensed the same way.
