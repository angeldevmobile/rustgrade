//! rustgrade rule engine.
//!
//! Pure library: callers hand in source text, the engine hands back findings.
//! Reading files, walking directories and rendering output belong to the adapters.

pub mod config;
pub mod finding;
pub mod rules;
pub mod score;

pub use config::Config;
pub use finding::{Category, Finding, Severity, Span};
pub use score::{Grade, Scorecard};

use serde::Serialize;

/// Version of the JSON contract. Bump on any breaking change to [`Report`].
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("failed to parse {path}: {source}")]
    Parse { path: String, source: syn::Error },
}

/// One source file to scan. `path` is only used for reporting and hot-path matching.
pub struct SourceFile<'a> {
    pub path: &'a str,
    pub source: &'a str,
}

/// Result of scanning a set of files.
#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u32,
    pub files_scanned: usize,
    pub findings: Vec<Finding>,
}

/// Scans one file with every enabled rule.
///
/// # Errors
/// Returns [`ScanError::Parse`] if the file is not valid Rust.
pub fn scan_file(file: &SourceFile<'_>, config: &Config) -> Result<Vec<Finding>, ScanError> {
    let ast = syn::parse_file(file.source).map_err(|source| ScanError::Parse {
        path: file.path.to_owned(),
        source,
    })?;
    let ctx = rules::FileContext {
        path: file.path,
        source: file.source,
        ast: &ast,
        is_hot: config.is_hot(file.path),
    };

    let mut findings = Vec::new();
    for rule in rules::registry() {
        if config.is_enabled(rule.id()) {
            rule.check(&ctx, &mut findings);
        }
    }
    findings.retain(|f| f.severity >= config.min_severity);
    Ok(findings)
}

/// Scans many files. Files that fail to parse are returned alongside the report
/// instead of aborting the whole scan.
#[must_use]
pub fn scan<'a, I>(files: I, config: &Config) -> (Report, Vec<ScanError>)
where
    I: IntoIterator<Item = SourceFile<'a>>,
{
    let mut findings = Vec::new();
    let mut errors = Vec::new();
    let mut files_scanned = 0;
    for file in files {
        files_scanned += 1;
        match scan_file(&file, config) {
            Ok(mut f) => findings.append(&mut f),
            Err(e) => errors.push(e),
        }
    }
    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.span.cmp(&b.span))
    });
    let report = Report {
        schema: SCHEMA_VERSION,
        files_scanned,
        findings,
    };
    (report, errors)
}
