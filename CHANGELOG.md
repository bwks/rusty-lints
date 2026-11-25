# Changelog

## [0.2.1] - 2026-11-25

### Added
- The `scan` rule `qualified_call`: an expression, struct literal or pattern path of three or more segments that starts with a crate, such as `jiff::Timestamp::now()`. Crate names come from the repository's `Cargo.toml` files, plus `std`, `core` and `alloc`. `[qualified_call]` in `rusty-lints.toml` takes `min_segments` and `allow`.

## [0.2.0] - 2026-11-25

### Added
- `rusty-lints`, one command line tool for both kinds of rule: `scan` (the syntax rules), `check` (the Dylint lints, through `cargo-dylint`), `all` (both, one report, one exit code) and `init` (writes a starter `rusty-lints.toml`, and refuses to overwrite one).
- Every command takes `--config`, `--rule`, `-C <DIR>` and `--format text|json`, and exits with 0 when clean, 1 on hits and 2 when the tool fails.
- `check` finds `cargo-dylint` and `dylint-link` 6.4.0 and the pinned nightly. It prints how to install what is missing, or installs it with `--install`. It builds the lints from the release tag that matches the binary (`--path` for a local checkout), passes `-p`, `--workspace` and `cargo check` arguments through, and reads the compiler's JSON diagnostics into the same report as `scan`.
- Prebuilt archives on each GitHub Release, for x86_64 Linux (gnu and musl), aarch64 and x86_64 macOS and x86_64 Windows, with `SHA256SUMS`. They install with `mise use github:bwks/rusty-lints`.
- `error_text_match` follows an error's text into the return value of a function of the same crate, through a closure passed to `map` or `and_then`, and through `as_deref`, `unwrap`, `unwrap_or_default` and `expect`. It now catches a test helper that returns the message as a `String`.

### Removed
- The `rusty-scan` binary. `rusty-lints scan` replaces it; `rusty-scan <PATH>` is `rusty-lints scan -C <PATH>`.

### Changed
- The tool's crate is in `cli/` (it was `scan/`), in a Cargo workspace at the repository root. The Dylint lints stay a workspace of their own in `dylint/`.

## [0.1.0] - 2026-11-24

### Added
- `rusty-scan`, with the `field_comment`, `use_in_function`, `code_in_mod_rs` and `version_number` rules.
- The Dylint lints `error_text_match` and `single_use_closure`.
- `rusty-lints.toml`, read by both.
