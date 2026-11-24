# rusty-lints

Rust lints not found in Clippy, to help AI agents unslop code.

There are two tools, each its own Cargo workspace:

| Tool | Folder | Toolchain | Sees |
|---|---|---|---|
| `rusty-scan` | `scan/` | stable | Syntax only. Parses every `.rs` file git tracks with `syn`, without compiling the repository, so code that `cfg` leaves out is checked too |
| Dylint lints | `dylint/` | pinned nightly | Types and name resolution. Runs inside the compiler through [Dylint](https://github.com/trailofbits/dylint), so it sees only the code compiled for the host with the enabled features |

## Rules

### `rusty-scan`

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

### Dylint

| Lint | Refuses | Why |
|---|---|---|
| `error_text_match` | A decision taken from an error's text: `==`, `!=`, `match`, `assert_eq!`, `assert_ne!`, or a string test (`contains`, `starts_with`, `ends_with`, `find`, `strip_prefix`, `split`, `parse`, ...) on what `to_string()` or `format!` made of a value whose type is an error | Wording changes. The error's type or variant is the contract. Tests count too: a test asserts the type or the variant, never the wording |
| `single_use_closure` | A closure bound with `let` and used exactly once | Write the code where it is used, or make it a function if it deserves a name |

`error_text_match` details:

- A type is an error when it implements `std::error::Error`, or is `anyhow::Error` or a boxed error.
- The text is followed through `let` bindings and through methods that keep it text (`as_str`, `to_lowercase`, `trim`, ...).
- In test code (`#[test]`, `#[cfg(test)]`, files under `tests/`) the message is "test checks an error by its text". Elsewhere it is "decision taken from the text of an error".
- It does not follow text returned by a helper function, and it cannot recognise an error whose type is `String`.

`single_use_closure` skips an `async` block bound to a name and a closure that a macro writes.

## Configuration: `rusty-lints.toml`

Both tools read `rusty-lints.toml` at the root of the repository they check. It has one table per rule. Without the file, or without a table, every rule is on with the defaults below.

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
- `rusty-scan` reads the file itself. The Dylint lints read it through Dylint's own config mechanism, which takes the TOML from the `DYLINT_TOML` environment variable. Each lint reads the table named after it and the top-level `exclude`.

## Running `rusty-scan`

Install it:

```sh
cargo install --locked --git https://github.com/bwks/rusty-lints --tag v0.1.0 rusty-scan
```

Run it in the repository to check:

```sh
rusty-scan                          # every rule the config switches on
rusty-scan --rule field_comment     # only this rule, even if the config switches it off
rusty-scan path/to/repo --config other.toml
```

- It prints each hit as `file:line: rule: text`, where the text is the source line. Then it prints a count and a hint per rule on stderr.
- Exit codes: 0 when clean, 1 when there are hits, 2 when the scan fails or a file does not parse.
- It scans only the files `git ls-files` lists, so the repository must be a git checkout.

## Running the Dylint lints

You need the following tools:

- `cargo-dylint` and `dylint-link` 6.4.0: `cargo install --locked cargo-dylint dylint-link --version 6.4.0`, or with mise, `mise use cargo:cargo-dylint@6.4.0 cargo:dylint-link@6.4.0`.
- rustup. Dylint builds the lints with the nightly pinned in `dylint/rust-toolchain.toml`, and rustup installs it on first use.

From the root of the repository to check:

```sh
DYLINT_TOML="$(cat rusty-lints.toml)" cargo dylint \
  --git https://github.com/bwks/rusty-lints --tag v0.1.0 --pattern 'dylint/*' \
  --all -- --workspace --all-targets
```

As a mise task a repository can copy:

```toml
[tasks."lint:dylint"]
run = '''DYLINT_TOML="$(cat rusty-lints.toml)" cargo dylint --git https://github.com/bwks/rusty-lints --tag v0.1.0 --pattern 'dylint/*' --all -- --workspace --all-targets'''
```

Notes:

- `--pattern 'dylint/*'` names the lint packages. `--pattern dylint` finds nothing, because `dylint/` is a virtual workspace with no package of its own. Dylint warns that it found no package in `dylint/.cargo`; the warning is harmless.
- `--rev <commit>` works in place of `--tag`.
- `--all-targets` matters. Without it the lints never see test code.
- Lints warn by default. To fail on a hit, set `DYLINT_RUSTFLAGS="-D warnings"`.
- An environment that sets `RUSTUP_TOOLCHAIN` overrides the pinned nightly, and the lint build fails. Run with `env -u RUSTUP_TOOLCHAIN`.
- The first run builds the lints and a Dylint driver for the nightly (stored in `~/.dylint_drivers`). Then it checks the whole workspace a second time, in its own target folder (`target/dylint`).
- The check compiles the target workspace with the pinned nightly. If the workspace needs a newer compiler, the lints need upgrading first.

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

`rusty-scan`, on stable:

```sh
cd scan
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- ..        # checks this repository
```

The Dylint lints, on the pinned nightly:

```sh
cd dylint
cargo test             # UI tests: each ui*/<case>.rs is compiled with the lint and its output compared with <case>.stderr
```

- `dylint/rust-toolchain.toml` and the `clippy_utils` revision in `dylint/Cargo.toml` are a pair. `cargo dylint upgrade dylint/<lint>` moves both. Expect an upgrade about every six weeks, and expect it to break the build when rustc's internals move.
- Each lint is its own `cdylib` crate. `dylint/config.rs` is shared between them through `#[path]`.
- The UI test of each lint runs three suites in turn: `ui/` (the cases), `ui_disabled/` (`enabled = false`) and `ui_excluded/` (`exclude`). They run in one test because `dylint_testing` passes the config through the process environment.

## Licence

MIT. See `LICENSE`.
