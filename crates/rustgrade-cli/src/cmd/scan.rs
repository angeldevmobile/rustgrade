use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use rustgrade_core::{Config, SourceFile};

use crate::{EXIT_FINDINGS, EXIT_OK, Globals, output};

const CONFIG_FILE: &str = "rustgrade.toml";

pub fn run(path: Option<&Path>, globals: &Globals) -> anyhow::Result<u8> {
    let root = path.unwrap_or(Path::new("."));
    let mut config = load_config(root)?;
    if let Some(min) = globals.min_severity {
        config.min_severity = min;
    }

    let files = collect_rs_files(root)?;
    let sources = files
        .iter()
        .map(|p| {
            let src = fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?;
            Ok((display_path(root, p), src))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let (report, errors) = rustgrade_core::scan(
        sources
            .iter()
            .map(|(path, source)| SourceFile { path, source }),
        &config,
    );
    for e in &errors {
        eprintln!("warning: {e}");
    }

    output::render(&report, globals.format)?;

    let failed = globals
        .fail_on
        .is_some_and(|t| report.findings.iter().any(|f| f.severity >= t));
    Ok(if failed { EXIT_FINDINGS } else { EXIT_OK })
}

fn load_config(root: &Path) -> anyhow::Result<Config> {
    let path = root.join(CONFIG_FILE);
    match fs::read_to_string(&path) {
        Ok(src) => Config::from_toml(&src).with_context(|| format!("parsing {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Recursively finds `.rs` files, skipping `target/` and hidden directories.
// TODO: respect .gitignore and workspace members (cargo metadata).
fn collect_rs_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if root.is_file() {
        out.push(root.to_path_buf());
        return Ok(out);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if entry.file_type()?.is_dir() {
                if name != "target" && !name.starts_with('.') {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Paths relative to the scan root with forward slashes, so output is stable across OSes.
fn display_path(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}
