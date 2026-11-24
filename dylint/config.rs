//! A lint's table in `rusty-lints.toml`, which Dylint reads from the `DYLINT_TOML` environment
//! variable, and the top-level `exclude` list. Each lint library includes this file.

use std::env;
use std::path::Path;

use rustc_lint::{LateContext, LintContext};
use rustc_span::{FileName, Span};
use serde::Deserialize;

const EXCLUDE_KEY: &str = "exclude";

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuleConfig {
    pub enabled: bool,
    pub exclude: Vec<String>,
}

#[derive(Clone)]
pub struct Config {
    pub enabled: bool,
    exclude: Vec<String>,
}

impl Default for RuleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            exclude: Vec::new(),
        }
    }
}

impl Config {
    /// Reads the table named `name` and the top-level `exclude` list, or the defaults where
    /// there is no config.
    pub fn load(name: &str) -> Self {
        let rule: RuleConfig = dylint_linting::config_or_default(name);
        let mut exclude: Vec<String> = dylint_linting::config_or_default(EXCLUDE_KEY);
        exclude.extend(rule.exclude);
        Self {
            enabled: rule.enabled,
            exclude,
        }
    }

    /// Whether the file that holds `span` is excluded. Its path is taken relative to the
    /// directory the compiler runs in, the workspace root.
    pub fn excluded(&self, cx: &LateContext<'_>, span: Span) -> bool {
        if self.exclude.is_empty() {
            return false;
        }
        let FileName::Real(name) = cx.sess().source_map().span_to_filename(span) else {
            return false;
        };
        let Some(path) = name.local_path() else {
            return false;
        };
        let relative = env::current_dir()
            .ok()
            .and_then(|root| path.strip_prefix(root).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| path.to_path_buf());
        let relative = relative.to_string_lossy().replace('\\', "/");
        self.exclude
            .iter()
            .any(|pattern| matches_prefix(pattern, &relative))
    }
}

/// Whether `pattern` names `path` or a folder holding it. The pattern is compared component by
/// component, and a component `*` matches any one component.
fn matches_prefix(pattern: &str, path: &str) -> bool {
    let mut path_parts = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".");
    pattern
        .split('/')
        .filter(|part| !part.is_empty())
        .all(|wanted| {
            path_parts
                .next()
                .is_some_and(|part| wanted == "*" || wanted == part)
        })
}
