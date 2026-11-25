//! The `rusty-lints.toml` file at the root of the checked repository. It has a table per rule.
//! The scan reads it here; the Dylint lints read it from the `DYLINT_TOML` environment variable,
//! which [`dylint_toml`] writes.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use toml::{Table, Value};

use crate::rule::{Rule, Tool};

pub const FILE_NAME: &str = "rusty-lints.toml";

const MIN_SEGMENTS: usize = 3;

const CLAP_DERIVES: [&str; 4] = ["Parser", "Subcommand", "Args", "ValueEnum"];

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub exclude: Vec<String>,
    pub field_comment: FieldComment,
    pub use_in_function: RuleConfig,
    pub code_in_mod_rs: RuleConfig,
    pub version_number: VersionNumber,
    pub qualified_call: QualifiedCall,
    pub error_text_match: RuleConfig,
    pub single_use_closure: RuleConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuleConfig {
    pub enabled: bool,
    pub exclude: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FieldComment {
    pub enabled: bool,
    pub exclude: Vec<String>,
    pub exempt_derives: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VersionNumber {
    pub enabled: bool,
    pub exclude: Vec<String>,
    pub names: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct QualifiedCall {
    pub enabled: bool,
    pub exclude: Vec<String>,
    pub min_segments: usize,
    pub allow: Vec<String>,
}

impl Default for RuleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            exclude: Vec::new(),
        }
    }
}

impl Default for FieldComment {
    fn default() -> Self {
        Self {
            enabled: true,
            exclude: Vec::new(),
            exempt_derives: CLAP_DERIVES.map(String::from).to_vec(),
        }
    }
}

impl Default for VersionNumber {
    fn default() -> Self {
        Self {
            enabled: true,
            exclude: Vec::new(),
            names: Vec::new(),
        }
    }
}

impl Default for QualifiedCall {
    fn default() -> Self {
        Self {
            enabled: true,
            exclude: Vec::new(),
            min_segments: MIN_SEGMENTS,
            allow: Vec::new(),
        }
    }
}

impl Config {
    /// Reads the config at `path`, or the defaults when there is no file there.
    pub fn load(path: &Path) -> Result<Self> {
        toml::from_str(&read(path)?).with_context(|| format!("parsing {}", path.display()))
    }

    /// Whether `rule` is switched on.
    pub fn enabled(&self, rule: Rule) -> bool {
        match rule {
            Rule::FieldComment => self.field_comment.enabled,
            Rule::UseInFunction => self.use_in_function.enabled,
            Rule::CodeInModRs => self.code_in_mod_rs.enabled,
            Rule::VersionNumber => self.version_number.enabled,
            Rule::QualifiedCall => self.qualified_call.enabled,
            Rule::ErrorTextMatch => self.error_text_match.enabled,
            Rule::SingleUseClosure => self.single_use_closure.enabled,
        }
    }

    /// Whether `path`, relative to the repository root, is left out of `rule`'s scan.
    pub fn excluded(&self, rule: Rule, path: &str) -> bool {
        let rule_patterns = match rule {
            Rule::FieldComment => &self.field_comment.exclude,
            Rule::UseInFunction => &self.use_in_function.exclude,
            Rule::CodeInModRs => &self.code_in_mod_rs.exclude,
            Rule::VersionNumber => &self.version_number.exclude,
            Rule::QualifiedCall => &self.qualified_call.exclude,
            Rule::ErrorTextMatch => &self.error_text_match.exclude,
            Rule::SingleUseClosure => &self.single_use_closure.exclude,
        };
        self.exclude
            .iter()
            .chain(rule_patterns)
            .any(|pattern| matches_prefix(pattern, path))
    }
}

/// The text of the config at `path`, empty when there is no file there.
fn read(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

/// The config at `path` as the Dylint lints read it from `DYLINT_TOML`: the file, with each
/// Dylint rule switched on when `rules` holds it and off otherwise.
pub fn dylint_toml(path: &Path, rules: &[Rule]) -> Result<String> {
    let mut table: Table =
        toml::from_str(&read(path)?).with_context(|| format!("parsing {}", path.display()))?;
    for rule in Rule::ALL
        .into_iter()
        .filter(|rule| rule.tool() == Tool::Check)
    {
        let entry = table
            .entry(rule.name())
            .or_insert_with(|| Value::Table(Table::new()));
        let Value::Table(rule_table) = entry else {
            bail!("{}: `{rule}` is not a table", path.display());
        };
        rule_table.insert("enabled".to_owned(), Value::Boolean(rules.contains(&rule)));
    }
    Ok(toml::to_string(&table)?)
}

/// Whether `pattern` names `path` or a folder holding it. The pattern is compared component by
/// component, and a component `*` matches any one component.
pub fn matches_prefix(pattern: &str, path: &str) -> bool {
    let mut path_parts = path.split('/').filter(|part| !part.is_empty());
    pattern
        .split('/')
        .filter(|part| !part.is_empty())
        .all(|wanted| {
            path_parts
                .next()
                .is_some_and(|part| wanted == "*" || wanted == part)
        })
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process};

    use super::{Config, dylint_toml, matches_prefix};
    use crate::rule::Rule;

    #[test]
    fn pattern_matches_whole_components_from_the_root() {
        assert!(matches_prefix("crates/a", "crates/a/src/lib.rs"));
        assert!(matches_prefix("crates/a/", "crates/a/src/lib.rs"));
        assert!(matches_prefix("crates/*/ui", "crates/b/ui/x.rs"));
        assert!(matches_prefix("src/lib.rs", "src/lib.rs"));
        assert!(!matches_prefix("crates/a", "crates/ab/src/lib.rs"));
        assert!(!matches_prefix("src", "crates/a/src/lib.rs"));
        assert!(!matches_prefix("src/lib.rs/x", "src/lib.rs"));
    }

    #[test]
    fn absent_tables_take_the_defaults() {
        let config: Config = toml::from_str("[version_number]\nenabled = false\n").unwrap();
        assert!(!config.enabled(Rule::VersionNumber));
        assert!(config.enabled(Rule::FieldComment));
        assert_eq!(
            config.field_comment.exempt_derives,
            ["Parser", "Subcommand", "Args", "ValueEnum"]
        );
    }

    #[test]
    fn global_and_rule_excludes_both_apply() {
        let config: Config = toml::from_str(
            r#"
            exclude = ["vendor"]
            [field_comment]
            exclude = ["gen"]
            "#,
        )
        .unwrap();
        assert!(config.excluded(Rule::FieldComment, "vendor/a.rs"));
        assert!(config.excluded(Rule::FieldComment, "gen/a.rs"));
        assert!(!config.excluded(Rule::UseInFunction, "gen/a.rs"));
    }

    #[test]
    fn unknown_key_is_refused() {
        assert!(toml::from_str::<Config>("[field_comment]\nenable = true\n").is_err());
    }

    #[test]
    fn dylint_toml_switches_on_only_the_chosen_dylint_rules() {
        let path = env::temp_dir().join(format!("rusty-lints-{}.toml", process::id()));
        fs::write(
            &path,
            r#"
            exclude = ["vendor"]
            [single_use_closure]
            exclude = ["gen"]
            "#,
        )
        .unwrap();
        let text = dylint_toml(&path, &[Rule::ErrorTextMatch, Rule::FieldComment]).unwrap();
        fs::remove_file(&path).unwrap();
        let config: Config = toml::from_str(&text).unwrap();
        assert!(config.enabled(Rule::ErrorTextMatch));
        assert!(!config.enabled(Rule::SingleUseClosure));
        assert_eq!(config.single_use_closure.exclude, ["gen"]);
        assert_eq!(config.exclude, ["vendor"]);
    }
}
