use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use rustgrade_core::{Config, Report, Severity, SourceFile};

use crate::{EXIT_FINDINGS, EXIT_OK, FailOn, Globals, output};

const CONFIG_FILE: &str = "rustgrade.toml";

pub fn run(path: Option<&Path>, globals: &Globals) -> anyhow::Result<u8> {
    let threshold = match globals.fail_on {
        None => None,
        Some(FailOn::Severity(s)) => Some(s),
        Some(FailOn::Grade(_)) => {
            anyhow::bail!("`--fail-on <grade>` applies to `audit`; use a severity with `scan`")
        }
    };
    let root = path.unwrap_or(Path::new("."));
    let report = build_report(root, globals.min_severity)?;

    output::render(&report, globals.format)?;

    let failed = threshold.is_some_and(|t| report.findings.iter().any(|f| f.severity >= t));
    Ok(if failed { EXIT_FINDINGS } else { EXIT_OK })
}

/// Scans every Rust file under `root` with its `rustgrade.toml`. Files that
/// fail to parse are reported on stderr and skipped.
pub fn build_report(root: &Path, min_severity: Option<Severity>) -> anyhow::Result<Report> {
    ensure_exists(root)?;
    let mut config = load_config(root)?;
    if let Some(min) = min_severity {
        config.min_severity = min;
    }

    let files = collect_rs_files(root, &config.exclude)?;
    let mut crate_dirs = HashMap::new();
    let sources = files
        .iter()
        .map(|p| {
            let src = fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?;
            let in_lib = in_library(p, &mut crate_dirs);
            Ok((display_path(root, p), src, in_lib))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let (report, errors) = rustgrade_core::scan(
        sources.iter().map(|(path, source, in_lib)| SourceFile {
            path,
            source,
            in_lib: *in_lib,
        }),
        &config,
    );
    for e in &errors {
        eprintln!("warning: {e}");
    }
    Ok(report)
}

/// A clear error for a mistyped path, instead of a walker or manifest error.
pub fn ensure_exists(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }
    anyhow::bail!(
        "{} does not exist (relative paths start at {})",
        path.display(),
        std::env::current_dir().map_or_else(
            |_| "the current directory".into(),
            |d| d.display().to_string()
        )
    )
}

fn load_config(root: &Path) -> anyhow::Result<Config> {
    let path = root.join(CONFIG_FILE);
    match fs::read_to_string(&path) {
        Ok(src) => Config::from_toml(&src).with_context(|| format!("parsing {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Recursively finds `.rs` files the way ripgrep would: honoring `.gitignore`,
/// `.ignore` and git's global excludes (even outside a git repo), skipping
/// hidden entries, `target/` and the config's `exclude` globs.
// TODO: walk workspace members from cargo metadata instead of the filesystem.
fn collect_rs_files(root: &Path, exclude: &[String]) -> anyhow::Result<Vec<PathBuf>> {
    if root.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    let mut overrides = ignore::overrides::OverrideBuilder::new(root);
    for glob in exclude {
        overrides
            // rustgrade: allow(alloc-in-loop) once per config entry; the builder needs a negated &str
            .add(&format!("!{glob}"))
            .with_context(|| format!("invalid exclude glob `{glob}` in {CONFIG_FILE}"))?;
    }
    let walker = ignore::WalkBuilder::new(root)
        .require_git(false)
        .overrides(overrides.build()?)
        // Not every repo gitignores it, and it is never source.
        .filter_entry(|e| e.file_name() != "target")
        .build();

    let mut out = Vec::new();
    for entry in walker {
        let entry = entry.with_context(|| format!("walking {}", root.display()))?;
        let is_file = entry.file_type().is_some_and(|t| t.is_file());
        if is_file && entry.path().extension().is_some_and(|e| e == "rs") {
            out.push(entry.into_path());
        }
    }
    out.sort();
    Ok(out)
}

/// True if `file` is part of a library target: its crate has `src/lib.rs` and
/// the file is under `src/` but is not `src/main.rs` or under `src/bin/`.
// TODO: honor custom `[lib] path` / `[[bin]] path` via cargo metadata.
fn in_library(file: &Path, crate_dirs: &mut HashMap<PathBuf, Option<PathBuf>>) -> bool {
    let file = fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let Some(dir) = file.parent() else {
        return false;
    };
    let crate_dir = crate_dirs
        .entry(dir.to_path_buf())
        .or_insert_with(|| {
            dir.ancestors()
                .find(|d| d.join("Cargo.toml").is_file())
                .map(Path::to_path_buf)
        })
        .as_deref();
    let Some(crate_dir) = crate_dir else {
        return false;
    };
    let src = crate_dir.join("src");
    if !src.join("lib.rs").is_file() {
        return false;
    }
    file.strip_prefix(&src)
        .is_ok_and(|rel| rel != Path::new("main.rs") && !rel.starts_with("bin"))
}

/// Paths relative to the scan root with forward slashes, so output is stable across OSes.
fn display_path(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}
