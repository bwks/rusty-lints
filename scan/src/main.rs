//! `rusty-scan`: syntax-only Rust lints that Clippy does not have. It parses every Rust file git
//! tracks in a repository with `syn`, without compiling it, so code that `cfg` leaves out is
//! checked too.

mod config;
mod lex;
mod rules;
mod source;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use clap::Parser;

use config::Config;
use rules::{Hit, Rule};
use source::Source;

const EXIT_HITS: u8 = 1;
const EXIT_ERROR: u8 = 2;

/// Checks the Rust files of a git repository against syntax-only lints.
///
/// Prints each hit as `file:line: rule: text`, where the text is the source line. Exits with 1
/// when there is a hit and 2 when the scan fails or a file does not parse.
#[derive(Debug, Parser)]
#[command(name = "rusty-scan", version)]
struct Cli {
    /// A path inside the repository to scan; the whole repository is scanned
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Check only this rule, even when the config switches it off; repeat for more
    #[arg(long, value_enum)]
    rule: Vec<Rule>,
    /// The config file [default: rusty-lints.toml at the repository root]
    #[arg(long)]
    config: Option<PathBuf>,
}

struct Outcome {
    hits: Vec<Hit>,
    parse_errors: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(outcome) => report(&outcome),
        Err(error) => {
            eprintln!("rusty-scan: {error:#}");
            ExitCode::from(EXIT_ERROR)
        }
    }
}

/// Scans the repository that holds `cli.path` with the rules the command line and the config
/// select.
fn run(cli: &Cli) -> Result<Outcome> {
    let root = repository_root(&cli.path)?;
    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(|| root.join(config::FILE_NAME));
    let config = Config::load(&config_path)?;
    let rules: Vec<Rule> = if cli.rule.is_empty() {
        Rule::ALL
            .into_iter()
            .filter(|&rule| config.enabled(rule))
            .collect()
    } else {
        cli.rule.clone()
    };
    let mut outcome = Outcome {
        hits: Vec::new(),
        parse_errors: Vec::new(),
    };
    for path in tracked_rust_files(&root)? {
        let full_path = root.join(&path);
        if !full_path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&full_path)
            .with_context(|| format!("reading {}", full_path.display()))?;
        let source = Source::new(path, text);
        let (hits, error) = rules::check(&source, &rules, &config);
        outcome.hits.extend(hits);
        if let Some(error) = error {
            let start = error.span().start();
            outcome.parse_errors.push(format!(
                "{}:{}: does not parse: {error}",
                source.path, start.line
            ));
        }
    }
    outcome.hits.sort();
    Ok(outcome)
}

/// Prints the hits and the parse errors, and returns the exit code they call for.
fn report(outcome: &Outcome) -> ExitCode {
    for hit in &outcome.hits {
        println!("{hit}");
    }
    for error in &outcome.parse_errors {
        eprintln!("rusty-scan: {error}");
    }
    let mut counts: BTreeMap<Rule, usize> = BTreeMap::new();
    for hit in &outcome.hits {
        *counts.entry(hit.rule).or_default() += 1;
    }
    for (rule, count) in &counts {
        eprintln!("rusty-scan: {rule}: {count} hits; {}", rule.help());
    }
    if !outcome.parse_errors.is_empty() {
        ExitCode::from(EXIT_ERROR)
    } else if outcome.hits.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_HITS)
    }
}

/// The root of the git repository that holds `path`.
fn repository_root(path: &Path) -> Result<PathBuf> {
    let output = git(path, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(output.trim_end_matches(['\n', '\r'])))
}

/// The Rust files git tracks under `root`, relative to it, with `/` between components.
fn tracked_rust_files(root: &Path) -> Result<Vec<String>> {
    let output = git(root, &["ls-files", "-z", "--", "*.rs"])?;
    Ok(output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Runs git in `dir` with `args` and returns what it prints.
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("running git")?;
    if !output.status.success() {
        bail!(
            "git {} in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("git printed text that is not UTF-8")
}
