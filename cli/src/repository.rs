//! The git repository `rusty-lints` checks.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The root of the git repository that holds `path`.
pub fn root(path: &Path) -> Result<PathBuf> {
    let output = git(path, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(output.trim_end_matches(['\n', '\r'])))
}

/// The Rust files git tracks under `root`, relative to it, with `/` between components.
pub fn tracked_rust_files(root: &Path) -> Result<Vec<String>> {
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
