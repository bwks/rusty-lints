//! `rusty-lints`: Rust lints that Clippy does not have. `scan` reads the syntax of every Rust
//! file git tracks, without compiling; `check` runs the lints that need types through Dylint.

mod check;
mod config;
mod json;
mod lex;
mod report;
mod repository;
mod rule;
mod scan;
mod source;

use std::fs::File;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use check::CheckArgs;
use config::Config;
use report::{EXIT_FAILURE, Format, Report};
use rule::{Rule, Tool};

const STARTER_CONFIG: &str = include_str!("starter.toml");

/// Checks a Rust repository against lints that Clippy does not have.
///
/// Prints each hit as `file:line: rule: text`, where the text is the source line. Exits with 0
/// when clean, 1 when there is a hit and 2 when the tool itself fails.
#[derive(Debug, Parser)]
#[command(name = "rusty-lints", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Run in this directory instead of the current one; the whole repository holding it is checked
    #[arg(
        short = 'C',
        long,
        global = true,
        value_name = "DIR",
        default_value = "."
    )]
    directory: PathBuf,
    /// The config file [default: rusty-lints.toml at the repository root]
    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,
    /// Check only this rule, even when the config switches it off; repeat for more
    #[arg(long, global = true, value_enum)]
    rule: Vec<Rule>,
    /// How to print the report
    #[arg(long, global = true, value_enum, default_value_t = Format::Text)]
    format: Format,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check the rules that read syntax alone: `field_comment`, `use_in_function`,
    /// `code_in_mod_rs`, `version_number` and `qualified_call`. Every Rust file git tracks is parsed, compiled or not
    Scan,
    /// Check the rules that need types: `error_text_match` and `single_use_closure`. They run in
    /// the compiler through cargo-dylint, on the Cargo workspace at the repository root
    Check(CheckArgs),
    /// Run scan, then check, into one report with one exit code
    All(CheckArgs),
    /// Write a starter rusty-lints.toml at the repository root, or at --config
    Init,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(Some(report)) => report.print(cli.format),
        Ok(None) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rusty-lints: {error:#}");
            ExitCode::from(EXIT_FAILURE)
        }
    }
}

/// Runs the command of `cli`, and returns its report; `init` has none.
fn run(cli: &Cli) -> Result<Option<Report>> {
    let root = repository::root(&cli.directory)?;
    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(|| root.join(config::FILE_NAME));
    if matches!(cli.command, Command::Init) {
        init(&config_path)?;
        return Ok(None);
    }
    let config = Config::load(&config_path)?;
    let mut report = Report::default();
    match &cli.command {
        Command::Scan => {
            let rules = selected(&cli.rule, &config, Tool::Scan, true)?;
            report.extend(scan::run(&root, &rules, &config)?);
        }
        Command::Check(args) => {
            let rules = selected(&cli.rule, &config, Tool::Check, true)?;
            if !rules.is_empty() {
                report.extend(check::run(&root, &config_path, &rules, args)?);
            }
        }
        Command::All(args) => {
            let rules = selected(&cli.rule, &config, Tool::Scan, false)?;
            if !rules.is_empty() {
                report.extend(scan::run(&root, &rules, &config)?);
            }
            let rules = selected(&cli.rule, &config, Tool::Check, false)?;
            if !rules.is_empty() {
                match check::run(&root, &config_path, &rules, args) {
                    Ok(check_report) => report.extend(check_report),
                    Err(error) => report.failures.push(format!("{error:#}")),
                }
            }
        }
        Command::Init => {}
    }
    Ok(Some(report))
}

/// The rules of `tool` to check: those named on the command line, or else those the config
/// switches on. With `strict`, a rule named on the command line that another command checks is
/// an error.
fn selected(named: &[Rule], config: &Config, tool: Tool, strict: bool) -> Result<Vec<Rule>> {
    if named.is_empty() {
        return Ok(Rule::ALL
            .into_iter()
            .filter(|&rule| rule.tool() == tool && config.enabled(rule))
            .collect());
    }
    if strict && let Some(other) = named.iter().find(|rule| rule.tool() != tool) {
        let command = match other.tool() {
            Tool::Scan => "scan",
            Tool::Check => "check",
        };
        bail!("`{other}` is checked by `rusty-lints {command}`, or by `rusty-lints all`");
    }
    Ok(named
        .iter()
        .copied()
        .filter(|rule| rule.tool() == tool)
        .collect())
}

/// Writes the starter config to `path`, refusing to overwrite a file there.
fn init(path: &Path) -> Result<()> {
    let mut file = match File::create_new(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            bail!("{} exists; not overwriting it", path.display());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("creating {}", path.display()));
        }
    };
    file.write_all(STARTER_CONFIG.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    eprintln!("rusty-lints: wrote {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{STARTER_CONFIG, selected};
    use crate::config::Config;
    use crate::rule::{Rule, Tool};

    #[test]
    fn starter_config_is_the_defaults() {
        let config: Config = toml::from_str(STARTER_CONFIG).unwrap();
        assert!(Rule::ALL.into_iter().all(|rule| config.enabled(rule)));
        assert_eq!(
            config.field_comment.exempt_derives,
            Config::default().field_comment.exempt_derives
        );
    }

    #[test]
    fn a_rule_of_the_other_command_is_refused_unless_running_both() {
        let config = Config::default();
        let named = [Rule::ErrorTextMatch, Rule::FieldComment];
        assert!(selected(&named, &config, Tool::Scan, true).is_err());
        assert_eq!(
            selected(&named, &config, Tool::Scan, false).unwrap(),
            [Rule::FieldComment]
        );
        assert_eq!(
            selected(&[], &config, Tool::Check, true).unwrap(),
            [Rule::ErrorTextMatch, Rule::SingleUseClosure]
        );
    }
}
