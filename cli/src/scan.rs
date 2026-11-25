//! `rusty-lints scan`: the rules that read syntax alone. Every Rust file git tracks is parsed
//! with `syn`, without compiling the repository, so code that `cfg` leaves out is checked too.

mod code_in_mod_rs;
mod field_comment;
mod qualified_call;
mod use_in_function;
mod version_number;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::report::{Hit, Report};
use crate::repository;
use crate::rule::Rule;
use crate::source::Source;

/// Scans the Rust files git tracks in the repository at `root` with `rules`, each of which the
/// scan checks.
pub fn run(root: &Path, rules: &[Rule], config: &Config) -> Result<Report> {
    let mut report = Report::default();
    let crates = if rules.contains(&Rule::QualifiedCall) {
        qualified_call::crate_names(root, config)?
    } else {
        BTreeSet::new()
    };
    for path in repository::tracked_files(root, &["*.rs"])? {
        let full_path = root.join(&path);
        if !full_path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&full_path)
            .with_context(|| format!("reading {}", full_path.display()))?;
        let source = Source::new(path, text);
        let (hits, error) = check(&source, rules, config, &crates);
        report.hits.extend(hits);
        if let Some(error) = error {
            let start = error.span().start();
            report.failures.push(format!(
                "{}:{}: does not parse: {error}",
                source.path, start.line
            ));
        }
    }
    Ok(report)
}

/// Checks `source` against each of `rules` that does not exclude its path; `crates` are the crate
/// names of the repository. Returns the hits, one per rule and line, and the parse error when a
/// rule needs the syntax tree and the file does not parse; the rules that read tokens alone still
/// run on such a file.
fn check(
    source: &Source,
    rules: &[Rule],
    config: &Config,
    crates: &BTreeSet<String>,
) -> (Vec<Hit>, Option<syn::Error>) {
    let rules: Vec<Rule> = rules
        .iter()
        .copied()
        .filter(|&rule| !config.excluded(rule, &source.path))
        .collect();
    let (file, error) = if rules.iter().any(|&rule| needs_syntax(rule)) {
        match source.parse() {
            Ok(file) => (Some(file), None),
            Err(error) => (None, Some(error)),
        }
    } else {
        (None, None)
    };
    let mut hits = Vec::new();
    for rule in rules {
        let mut lines = lines(rule, source, file.as_ref(), config, crates);
        lines.sort_unstable();
        lines.dedup();
        hits.extend(lines.into_iter().map(|line| Hit {
            path: source.path.clone(),
            line,
            column: None,
            rule,
            text: source.line_text(line).to_owned(),
            message: rule.help().to_owned(),
        }));
    }
    (hits, error)
}

/// Whether `rule` needs the syntax tree, rather than the tokens alone.
fn needs_syntax(rule: Rule) -> bool {
    !matches!(rule, Rule::VersionNumber)
}

/// The 1-based lines of `source` that break `rule`. `file` is the syntax tree, when the file
/// parsed, and `crates` are the crate names of the repository.
fn lines(
    rule: Rule,
    source: &Source,
    file: Option<&syn::File>,
    config: &Config,
    crates: &BTreeSet<String>,
) -> Vec<usize> {
    match (rule, file) {
        (Rule::FieldComment, Some(file)) => {
            field_comment::lines(source, file, &config.field_comment.exempt_derives)
        }
        (Rule::UseInFunction, Some(file)) => use_in_function::lines(source, file),
        (Rule::CodeInModRs, Some(file)) => code_in_mod_rs::lines(source, file),
        (Rule::QualifiedCall, Some(file)) => {
            qualified_call::lines(source, file, crates, &config.qualified_call)
        }
        (Rule::VersionNumber, _) => version_number::lines(source, &config.version_number.names),
        _ => Vec::new(),
    }
}
