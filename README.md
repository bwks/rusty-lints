# rusty-lints

Rust lints not found in Clippy, to help AI agents unslop code.

One command line tool, `rusty-lints`, runs two kinds of rule:

| Command | Rules | Toolchain | Sees |
|---|---|---|---|
| `rusty-lints scan` | `field_comment`, `use_in_function`, `code_in_mod_rs`, `version_number` | none | Syntax only. Parses every `.rs` file git tracks with `syn`, without compiling the repository, so code that `cfg` leaves out is checked too |
| `rusty-lints check` | `error_text_match`, `single_use_closure` | pinned nightly | Types and name resolution. Runs inside the compiler through [Dylint](https://github.com/trailofbits/dylint), so it sees only the code compiled for the host with the enabled features |
| `rusty-lints all` | all six | pinned nightly | `scan`, then `check`, in one report with one exit code |

`rusty-lints init` writes a starter `rusty-lints.toml`.

## Install

Download the archive for your platform from the [release](https://github.com/bwks/rusty-lints/releases/tag/v0.2.0), check it against `SHA256SUMS`, and put `rusty-lints` on your `PATH`:

| Platform | Archive |
|---|---|
| Linux x86_64 (glibc) | `rusty-lints-v0.2.0-x86_64-unknown-linux-gnu.tar.gz` |
| Linux x86_64 (static, musl) | `rusty-lints-v0.2.0-x86_64-unknown-linux-musl.tar.gz` |
| macOS Apple silicon | `rusty-lints-v0.2.0-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `rusty-lints-v0.2.0-x86_64-apple-darwin.tar.gz` |
| Windows x86_64 | `rusty-lints-v0.2.0-x86_64-pc-windows-msvc.zip` |

With [mise](https://mise.jdx.dev), from the same archives:

```sh
mise use github:bwks/rusty-lints@0.2.0
```

From source, with stable Rust:

```sh
cargo install --locked --git https://github.com/bwks/rusty-lints --tag v0.2.0 rusty-lints
```

`check` and `all` also need the Dylint tools and the pinned nightly; see [Running `check`](#running-check).

## Rules

### `scan`

| Rule | Refuses | Why |
|---|---|---|
| `field_comment` | A comment on a struct field, an enum variant or a field of an enum's struct variant: `///`, `//`, `/* */`, a trailing comment or `#[doc = ...]`. Types whose derives include an exempt derive are skipped | A field or a variant is named for what it holds. A comment there repeats the name or hides that the name is wrong. clap builds command line help from doc comments, so its derives are exempt by default |
| `use_in_function` | A `use` declaration inside a function body, a closure or any other block | Imports belong at the top of the file, where a reader looks for them |
| `code_in_mod_rs` | Any item in a `mod.rs` other than `mod name;` and a `use` with a visibility (a re-export) | A `mod.rs` maps the module tree. Code in it is hard to find among many files with the same name |
| `version_number` | A version number after "since", "from", "as of", "before", "after", "until" or "in", in a comment or a string literal: `since 0.7.0`, `before v2.1` | Code, comments and user-facing text describe current behaviour. History belongs in the changelog |

`version_number` details:

- After "since", "before" and "as of", a two-part number (`0.7`) counts. After the other words only a three-part number does, because a two-part number there is more often a quantity (`until 2.5 seconds`).
- The number may follow "version", a `v`, or one of the names set in `names`, as in `since Sherpa 0.7.0`.
- An IP address or a prefix (`10.0.0.0/24`) is not a version.

### `check`

| Lint | Refuses | Why |
|---|---|---|
| `error_text_match` | A decision taken from an error's text: `==`, `!=`, `match`, `assert_eq!`, `assert_ne!`, or a string test (`contains`, `starts_with`, `ends_with`, `find`, `strip_prefix`, `split`, `parse`, ...) on what `to_string()` or `format!` made of a value whose type is an error | Wording changes. The error's type or variant is the contract. Tests count too: a test asserts the type or the variant, never the wording |
| `single_use_closure` | A closure bound with `let` and used exactly once | Write the code where it is used, or make it a function if it deserves a name |

`error_text_match` details:

- A type is an error when it implements `std::error::Error`, or is `anyhow::Error` or a boxed error.
- The text is followed through `let` bindings, through methods that keep it text (`as_str`, `to_lowercase`, `trim`, `as_deref`, `unwrap`, ...), through a closure that `map` or `and_then` turns an error into text with, and into the body of a function of the same crate that returns it, such as a test helper `fn message() -> String { result.unwrap_err().to_string() }`.
- In test code (`#[test]`, `#[cfg(test)]`, files under `tests/`) the message is "test checks an error by its text". Elsewhere it is "decision taken from the text of an error".
- It follows a helper's return value only through the helper's last expression, not an early `return`, and not into a method of a type, a trait method or a function of another crate. It cannot recognise an error whose type is `String`.

`single_use_closure` skips an `async` block bound to a name and a closure that a macro writes.

## Configuration: `rusty-lints.toml`

Every command reads `rusty-lints.toml` at the root of the repository it checks, or the file `--config` names. It has one table per rule. Without the file, or without a table, every rule is on with the defaults below. `rusty-lints init` writes this file, and refuses to overwrite one.

```toml
# Path prefixes, relative to the repository root, that no rule checks.
# Matched whole component by component; `*` matches any one component.
exclude = ["vendor", "crates/*/tests/fixtures"]

[field_comment]
enabled = true
exclude = []
exempt_derives = ["Parser", "Subcommand", "Args", "ValueEnum"]  # the default

[use_in_function]
enabled = true

[code_in_mod_rs]
enabled = true

[version_number]
enabled = true
names = ["sherpa"]  # names that may sit between the word and the number; default none

[error_text_match]
enabled = true
exclude = []

[single_use_closure]
enabled = true
```

- Every table takes `enabled` and `exclude`. An unknown key is an error.
- `exempt_derives` matches the last segment of a derive's path, so `Parser` and `clap::Parser` are both exempt. A derive inside `cfg_attr` counts.
- `check` hands the file to the Dylint lints through the `DYLINT_TOML` environment variable, which is how Dylint passes config. Each lint reads the table named after it and the top-level `exclude`. Their paths are relative to the Cargo workspace root, which `check` runs at: the repository root.

## Running `rusty-lints`

```sh
rusty-lints scan                          # the syntax rules the config switches on
rusty-lints check                         # the Dylint rules the config switches on
rusty-lints all                           # both, in one report
rusty-lints all --rule field_comment      # only this rule, even if the config switches it off
rusty-lints scan -C path/to/repo --config other.toml
rusty-lints check --format json
```

Options every command takes:

| Option | Does |
|---|---|
| `-C <DIR>` | Check the repository that holds `DIR` instead of the current directory's |
| `--config <FILE>` | Read this config instead of `rusty-lints.toml` at the repository root |
| `--rule <RULE>` | Check only this rule, even when the config switches it off; repeat for more. `scan` and `check` refuse a rule the other command checks; `all` runs each where it belongs |
| `--format text\|json` | `text` (the default) prints each hit as `file:line: rule: text`, where the text is the source line, then a count and a hint per rule on stderr. `json` prints one object, `{"hits": [...], "failures": [...]}`; each hit has `file`, `line`, `column` (`null` for `scan`), `rule`, `text` and `message` |

- Exit codes: 0 when clean, 1 when there are hits, 2 when the tool itself fails: a file does not parse, the config is wrong, a tool is missing or the build fails.
- `scan` checks only the files `git ls-files` lists, so the repository must be a git checkout.
- A hit is one site. `check` can report two sites on one line; they print as two lines.

### Running `check`

`check` needs:

- `cargo-dylint` and `dylint-link` 6.4.0: `cargo install --locked cargo-dylint dylint-link --version 6.4.0`, or with mise, `mise use cargo:cargo-dylint@6.4.0 cargo:dylint-link@6.4.0`. `dylint-link` has no version flag, so only that it runs is checked.
- rustup, with the nightly the lints pin and its `clippy`, `llvm-tools-preview`, `rustc-dev` and `rustfmt` components: `rustup toolchain install nightly-2026-08-20 --profile minimal --component clippy,llvm-tools-preview,rustc-dev,rustfmt`.

When one is missing, `check` prints the command that installs it and exits with 2. `--install` runs those commands instead.

It then runs, at the repository root:

```sh
cargo dylint --git https://github.com/bwks/rusty-lints --rev v0.2.0 --pattern 'dylint/*' --all \
  -- --workspace --all-targets --message-format=json
```

with `RUSTUP_TOOLCHAIN` unset and `DYLINT_TOML` set from the config. The `--rev` is the release of the `rusty-lints` binary, so the lints always match it, and it reads the compiler's JSON diagnostics into the same report as `scan`.

| `check` option | Does |
|---|---|
| `-p, --package <SPEC>` | Check only this package; repeat for more. Without it, `--workspace` |
| `--workspace` | Check every package of the workspace |
| `--path <DIR>` | Build the lints from this rusty-lints checkout instead of the release; for developing the lints |
| `--install` | Install the missing tools and toolchain |
| `-- <ARGS>` | Further arguments for `cargo check`, such as `--features` |

Notes:

- Every target is checked, as if with `--all-targets`: the rules hold in test code too.
- The first run builds the lints and a Dylint driver for the nightly (stored in `~/.dylint_drivers`). Then it checks the whole workspace a second time, in its own target folder (`target/dylint`). Cargo's progress goes to stderr.
- Dylint warns that it found no package in `dylint/.cargo`; the warning is harmless.
- The check compiles the target workspace with the pinned nightly. If the workspace needs a newer compiler, the lints need upgrading first.
- The Cargo workspace must be at the repository root.

### Exceptions

A plain `#[expect(error_text_match)]` makes the normal build warn about an unknown lint. Use Dylint's form, and declare the `dylint_lib` cfg:

```rust
#[cfg_attr(dylint_lib = "error_text_match", expect(error_text_match))]
```

```toml
# Cargo.toml
[lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = ["cfg(dylint_lib, values(any()))"] }
```

## Developing

The command line tool, in `cli/`, on stable, from the repository root:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- all --path .   # checks this repository with the lints in this checkout
```

The Dylint lints, on the pinned nightly:

```sh
cd dylint
cargo test             # UI tests: each ui*/<case>.rs is compiled with the lint and its output compared with <case>.stderr
```

- `dylint/rust-toolchain.toml` and the `clippy_utils` revision in `dylint/Cargo.toml` are a pair. `cargo dylint upgrade dylint/<lint>` moves both. Expect an upgrade about every six weeks, and expect it to break the build when rustc's internals move. `cli/src/check.rs` names the same nightly, and a test keeps the two equal.
- Each lint is its own `cdylib` crate. `dylint/config.rs` is shared between them through `#[path]`.
- The UI test of each lint runs three suites in turn: `ui/` (the cases), `ui_disabled/` (`enabled = false`) and `ui_excluded/` (`exclude`). They run in one test because `dylint_testing` passes the config through the process environment.

## Releasing

1. Set the version in `cli/Cargo.toml` and `dylint/Cargo.toml`, and add its section to `CHANGELOG.md`.
2. Merge, then push the tag `v<version>`. The release workflow builds the archives, writes `SHA256SUMS` and publishes the GitHub Release with that section of the changelog as its notes.

## Licence

MIT. See `LICENSE`.
