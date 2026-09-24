## Summary

<!-- What changed, and why a user of spechurl would care. -->

## Test plan

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test`
- [ ] `spechurl generate` and `spechurl check` on any fixture this change affects

## Checklist

- [ ] Changelog updated under `Unreleased` if behavior changed
- [ ] Generated Hurl stays deterministic (sorted keys, stable file names)
- [ ] New failure modes include a specific error message
