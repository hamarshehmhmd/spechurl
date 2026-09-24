# Changelog

All notable changes to spechurl are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-09-24

### Added

- `spechurl generate` writes one Hurl file per OpenAPI operation, plus a README in the output directory.
- `spechurl check` exits non-zero when a committed Hurl suite drifts from the spec.
- OpenAPI 3.0 and 3.1 support for paths, parameters, request bodies, and success status assertions.
- Sample JSON, form, and text bodies from examples or a minimal schema.
- Fixture specs and tests for a pet store, path and header edge cases, and an OpenAPI 3.1 document.

[Unreleased]: https://github.com/hamarshehmhmd/spechurl/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/hamarshehmhmd/spechurl/releases/tag/v0.1.0
