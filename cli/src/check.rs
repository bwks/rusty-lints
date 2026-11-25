//! `rusty-lints check`: the Dylint lints, which see types and name resolution. They run inside
//! the compiler through `cargo dylint`, built from this release of rusty-lints with the nightly
//! it pins.

use std::io::{BufRead, BufReader};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::config;
use crate::json::Value;
use crate::report::{Hit, Report};
use crate::rule::Rule;

const REPOSITORY: &str = "https://github.com/bwks/rusty-lints";
const RELEASE_TAG: &str = concat!("v", env!("CARGO_PKG_VERSION"));
const DYLINT_VERSION: &str = "6.4.0";
const NIGHTLY: &str = "nightly-2026-08-20";
const NIGHTLY_COMPONENTS: [&str; 4] = ["clippy", "llvm-tools-preview", "rustc-dev", "rustfmt"];
const LINT_PATTERN: &str = "dylint/*";

/// Options of `check`, and of `all` for its check.
#[derive(Debug, Clone, Default, Args)]
pub struct CheckArgs {
    /// Install what the check needs and is missing: cargo-dylint and dylint-link with
    /// `cargo install`, and the pinned nightly with rustup
    #[arg(long)]
    pub install: bool,
    /// Build the lints from this rusty-lints checkout instead of the release that matches this
    /// binary
    #[arg(long, value_name = "DIR")]
    pub path: Option<PathBuf>,
    /// Check only this package; repeat for more [default: the whole workspace]
    #[arg(short, long, value_name = "SPEC")]
    pub package: Vec<String>,
    /// Check every package of the workspace
    #[arg(long)]
    pub workspace: bool,
    /// Further arguments for `cargo check`, after `--`
    #[arg(last = true, value_name = "CARGO_ARGS")]
    pub cargo_args: Vec<String>,
}

/// Runs the Dylint lints in `rules` on the Cargo workspace at `root`, with the config at
/// `config_path`.
pub fn run(root: &Path, config_path: &Path, rules: &[Rule], args: &CheckArgs) -> Result<Report> {
    ensure_dylint(args.install)?;
    ensure_nightly(args.install)?;
    let dylint_toml = config::dylint_toml(config_path, rules)?;
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .env_remove("RUSTUP_TOOLCHAIN")
        .env("DYLINT_TOML", dylint_toml)
        .arg("dylint");
    match &args.path {
        Some(path) => command.arg("--path").arg(path),
        None => command.args(["--git", REPOSITORY, "--rev", RELEASE_TAG]),
    };
    command.args(["--pattern", LINT_PATTERN, "--all", "--"]);
    command.args(cargo_check_args(args));
    let mut child = command
        .stdout(Stdio::piped())
        .spawn()
        .context("running cargo dylint")?;
    let stdout = child.stdout.take().context("reading cargo dylint")?;
    let mut report = Report::default();
    let mut finished = false;
    for line in BufReader::new(stdout).lines() {
        let line = line.context("reading cargo dylint")?;
        let Some(message) = Value::parse(&line) else {
            continue;
        };
        match message.get("reason").and_then(Value::as_str) {
            Some("compiler-message") => {
                if let Some(message) = message.get("message") {
                    read_diagnostic(root, message, &mut report);
                }
            }
            Some("build-finished") => {
                finished = message.get("success").is_some_and(Value::is_true);
            }
            _ => {}
        }
    }
    let status = child.wait().context("waiting for cargo dylint")?;
    if !status.success() || !finished {
        report.failures.push(format!(
            "cargo dylint failed ({status}); its output is above"
        ));
    }
    Ok(report)
}

/// The arguments `cargo dylint` hands to `cargo check`: the packages, every target (the rules
/// hold in tests too), JSON diagnostics and the caller's own.
fn cargo_check_args(args: &CheckArgs) -> Vec<String> {
    let mut check_args = Vec::new();
    if args.workspace || args.package.is_empty() {
        check_args.push("--workspace".to_owned());
    }
    for package in &args.package {
        check_args.push("--package".to_owned());
        check_args.push(package.clone());
    }
    check_args.push("--all-targets".to_owned());
    check_args.push("--message-format=json".to_owned());
    check_args.extend(args.cargo_args.iter().cloned());
    check_args
}

/// Adds the compiler diagnostic `message` to `report`: a hit when one of the lints wrote it, a
/// failure when it is an error.
fn read_diagnostic(root: &Path, message: &Value, report: &mut Report) {
    let level = message.get("level").and_then(Value::as_str);
    let rule = message
        .get("code")
        .and_then(|code| code.get("code"))
        .and_then(Value::as_str)
        .and_then(Rule::from_name);
    let span = message
        .get("spans")
        .map_or(&[][..], Value::items)
        .iter()
        .find(|span| span.get("is_primary").is_some_and(Value::is_true));
    if let (Some(rule), Some(span)) = (rule, span) {
        let file = span.get("file_name").and_then(Value::as_str);
        let line = span.get("line_start").and_then(Value::as_usize);
        let column = span.get("column_start").and_then(Value::as_usize);
        if let (Some(file), Some(line)) = (file, line) {
            let text = span
                .get("text")
                .map_or(&[][..], Value::items)
                .first()
                .and_then(|text| text.get("text"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            report.hits.push(Hit {
                path: relative_path(root, file),
                line,
                column,
                rule,
                text: text.trim().to_owned(),
                message: message
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            });
            return;
        }
    }
    if level == Some("error") {
        let rendered = message
            .get("rendered")
            .or_else(|| message.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("an error without text");
        report
            .failures
            .push(format!("the compiler failed: {}", rendered.trim_end()));
    }
}

/// `file`, as the compiler names it, relative to `root` with `/` between components and `..`
/// resolved: a module included with `#[path]` is named through the including file's folder.
fn relative_path(root: &Path, file: &str) -> String {
    let path = Path::new(file);
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut parts: Vec<String> = Vec::new();
    for component in relative.components() {
        match component {
            Component::ParentDir if parts.last().is_some_and(|part| part != "..") => {
                parts.pop();
            }
            Component::CurDir => {}
            component => parts.push(component.as_os_str().to_string_lossy().into_owned()),
        }
    }
    parts.join("/")
}

/// Makes sure `cargo-dylint` and `dylint-link` are installed, at the pinned version where the tool
/// tells its version, installing them with `cargo install` when `install` is set.
fn ensure_dylint(install: bool) -> Result<()> {
    let cargo_dylint = tool_output("cargo-dylint", &["dylint", "--version"], None);
    let cargo_dylint_ok = cargo_dylint
        .as_deref()
        .is_some_and(|version| version.split_whitespace().last() == Some(DYLINT_VERSION));
    let dylint_link_ok = tool_output("dylint-link", &["--version"], Some(NIGHTLY)).is_some();
    if cargo_dylint_ok && dylint_link_ok {
        return Ok(());
    }
    let install_command =
        format!("cargo install --locked cargo-dylint dylint-link --version {DYLINT_VERSION}");
    if !install {
        let found = cargo_dylint.map_or_else(
            || "cargo-dylint is not installed".to_owned(),
            |version| format!("found {}", version.trim()),
        );
        let link = if dylint_link_ok {
            ""
        } else {
            ", dylint-link is not installed"
        };
        bail!(
            "check needs cargo-dylint and dylint-link {DYLINT_VERSION} ({found}{link}). Install them with \
             `{install_command}`, or with mise: `mise use cargo:cargo-dylint@{DYLINT_VERSION} \
             cargo:dylint-link@{DYLINT_VERSION}`. Or run again with --install"
        );
    }
    eprintln!("rusty-lints: {install_command}");
    let status = Command::new("cargo")
        .env_remove("RUSTUP_TOOLCHAIN")
        .args([
            "install",
            "--locked",
            "cargo-dylint",
            "dylint-link",
            "--version",
            DYLINT_VERSION,
        ])
        .status()
        .context("running cargo install")?;
    if !status.success() {
        bail!("`{install_command}` failed ({status})");
    }
    Ok(())
}

/// Makes sure rustup has the pinned nightly and the components Dylint needs, installing them when
/// `install` is set.
fn ensure_nightly(install: bool) -> Result<()> {
    let installed = tool_output(
        "rustup",
        &["component", "list", "--installed", "--toolchain", NIGHTLY],
        None,
    )
    .unwrap_or_default();
    let complete = NIGHTLY_COMPONENTS.into_iter().all(|component| {
        let prefix = component.trim_end_matches("-preview");
        installed.lines().any(|line| line.starts_with(prefix))
    });
    if complete {
        return Ok(());
    }
    let components = NIGHTLY_COMPONENTS.join(",");
    let install_command =
        format!("rustup toolchain install {NIGHTLY} --profile minimal --component {components}");
    if !install {
        bail!(
            "check needs the Rust toolchain {NIGHTLY} with {components}. Install it with \
             `{install_command}`. Or run again with --install"
        );
    }
    eprintln!("rusty-lints: {install_command}");
    let status = Command::new("rustup")
        .args(["toolchain", "install", NIGHTLY, "--profile", "minimal"])
        .args(["--component", &components])
        .status()
        .context("running rustup")?;
    if !status.success() {
        bail!("`{install_command}` failed ({status})");
    }
    Ok(())
}

/// What `program` prints on stdout when run with `args`, or `None` when it cannot run or fails.
/// It runs with `RUSTUP_TOOLCHAIN` set to `toolchain`, or unset. `dylint-link` refuses to run
/// without it, and hands `--version` to the system linker, so its version is not known.
fn tool_output(program: &str, args: &[&str], toolchain: Option<&str>) -> Option<String> {
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null());
    match toolchain {
        Some(toolchain) => command.env("RUSTUP_TOOLCHAIN", toolchain),
        None => command.env_remove("RUSTUP_TOOLCHAIN"),
    };
    let output = command.output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{
        CheckArgs, NIGHTLY, NIGHTLY_COMPONENTS, cargo_check_args, read_diagnostic, relative_path,
    };
    use toml::{Table, Value as TomlValue};

    use crate::json::Value;
    use crate::report::Report;
    use crate::rule::Rule;

    #[test]
    fn nightly_matches_the_toolchain_the_lints_pin() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../dylint/rust-toolchain.toml");
        let toolchain: Table = toml::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        let toolchain = toolchain["toolchain"].as_table().unwrap();
        assert_eq!(toolchain["channel"].as_str(), Some(NIGHTLY));
        let components: Vec<&str> = toolchain["components"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(TomlValue::as_str)
            .collect();
        assert_eq!(components, NIGHTLY_COMPONENTS);
    }

    #[test]
    fn packages_replace_the_workspace() {
        let args = CheckArgs {
            package: vec!["db".into()],
            cargo_args: vec!["--features=x".into()],
            ..CheckArgs::default()
        };
        assert_eq!(
            cargo_check_args(&args),
            [
                "--package",
                "db",
                "--all-targets",
                "--message-format=json",
                "--features=x"
            ]
        );
    }

    #[test]
    fn a_lint_diagnostic_is_a_hit_and_an_error_a_failure() {
        let mut report = Report::default();
        let lint = Value::parse(
            r#"{"level":"warning","message":"test checks an error by its text","code":{"code":"error_text_match"},"spans":[{"file_name":"/repo/tests/a.rs","line_start":7,"column_start":5,"is_primary":true,"text":[{"text":"    assert!(x);"}]}]}"#,
        )
        .unwrap();
        let other = Value::parse(
            r#"{"level":"warning","message":"unused","code":{"code":"unused_variables"},"spans":[]}"#,
        )
        .unwrap();
        let error = Value::parse(
            r#"{"level":"error","message":"mismatched types","code":null,"spans":[],"rendered":"error: mismatched types\n"}"#,
        )
        .unwrap();
        for message in [&lint, &other, &error] {
            read_diagnostic(Path::new("/repo"), message, &mut report);
        }
        assert_eq!(report.hits.len(), 1);
        let hit = &report.hits[0];
        assert_eq!(
            (hit.path.as_str(), hit.line, hit.column, hit.rule),
            ("tests/a.rs", 7, Some(5), Rule::ErrorTextMatch)
        );
        assert_eq!(hit.text, "assert!(x);");
        assert_eq!(hit.message, "test checks an error by its text");
        assert_eq!(
            report.failures,
            ["the compiler failed: error: mismatched types"]
        );
    }

    #[test]
    fn a_path_through_a_parent_folder_is_resolved() {
        assert_eq!(
            relative_path(Path::new("/repo"), "crates/db/src/../build/placeholders.rs"),
            "crates/db/build/placeholders.rs"
        );
        assert_eq!(
            relative_path(Path::new("/repo"), "/repo/src/./a.rs"),
            "src/a.rs"
        );
        assert_eq!(relative_path(Path::new("/repo"), "../b.rs"), "../b.rs");
    }
}
