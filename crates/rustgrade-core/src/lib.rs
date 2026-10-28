//! rustgrade rule engine.
//!
//! Pure library: callers hand in source text, the engine hands back findings.
//! Reading files, walking directories and rendering output belong to the adapters.

pub mod config;
pub mod finding;
pub mod rules;
pub mod score;
pub mod suppress;

pub use config::Config;
pub use finding::{Category, Finding, Severity, Span};
pub use score::{Area, Cap, Grade, Scorecard};
pub use suppress::Suppressed;

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
    /// File belongs to a library target, so its `pub` items are a public API.
    /// The caller decides, since that needs `Cargo.toml`.
    pub in_lib: bool,
}

/// Result of scanning a set of files.
#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u32,
    pub files_scanned: usize,
    pub findings: Vec<Finding>,
    /// Findings silenced inline, with their reason. Not counted by `--fail-on`.
    pub suppressed: Vec<Suppressed>,
}

/// Findings for one file.
#[derive(Debug, Default)]
pub struct FileReport {
    pub findings: Vec<Finding>,
    pub suppressed: Vec<Suppressed>,
}

/// Scans one file with every enabled rule, then applies inline suppressions.
///
/// # Errors
/// Returns [`ScanError::Parse`] if the file is not valid Rust.
pub fn scan_file(file: &SourceFile<'_>, config: &Config) -> Result<FileReport, ScanError> {
    let ast = syn::parse_file(file.source).map_err(|source| ScanError::Parse {
        path: file.path.to_owned(),
        source,
    })?;
    let ctx = rules::FileContext {
        path: file.path,
        source: file.source,
        ast: &ast,
        is_hot: config.is_hot(file.path),
        in_lib: file.in_lib,
    };

    let mut findings = Vec::new();
    for rule in rules::registry() {
        if config.is_enabled(rule.id()) {
            rule.check(&ctx, &mut findings);
        }
    }
    let (mut findings, suppressed) = suppress::apply(
        file.path,
        file.source,
        findings,
        |id| rules::find(id).is_some(),
        |id| config.is_enabled(id),
    );
    findings.retain(|f| f.severity >= config.min_severity);
    Ok(FileReport {
        findings,
        suppressed,
    })
}

/// Scans many files. Files that fail to parse are returned alongside the report
/// instead of aborting the whole scan.
#[must_use]
pub fn scan<'a, I>(files: I, config: &Config) -> (Report, Vec<ScanError>)
where
    I: IntoIterator<Item = SourceFile<'a>>,
{
    let mut all = FileReport::default();
    let mut errors = Vec::new();
    let mut files_scanned = 0;
    for file in files {
        files_scanned += 1;
        match scan_file(&file, config) {
            Ok(mut r) => {
                all.findings.append(&mut r.findings);
                all.suppressed.append(&mut r.suppressed);
            }
            Err(e) => errors.push(e),
        }
    }
    let by_severity_then_location = |a: &Finding, b: &Finding| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.span.cmp(&b.span))
    };
    all.findings.sort_by(by_severity_then_location);
    all.suppressed
        .sort_by(|a, b| by_severity_then_location(&a.finding, &b.finding));
    let report = Report {
        schema: SCHEMA_VERSION,
        files_scanned,
        findings: all.findings,
        suppressed: all.suppressed,
    };
    (report, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_one(src: &str) -> FileReport {
        let file = SourceFile {
            path: "src/lib.rs",
            source: src,
            in_lib: true,
        };
        scan_file(&file, &Config::default()).expect("parses")
    }

    #[test]
    fn inline_allow_suppresses_a_real_rule_finding() {
        let r = scan_one(
            "// rustgrade: allow(anyhow-in-lib-api) adapter stub, typed error lands with MCP\n\
             /// Docs.\n\
             pub fn serve() -> anyhow::Result<()> { todo!() }\n",
        );
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        assert_eq!(r.suppressed.len(), 1);
        assert_eq!(r.suppressed[0].finding.rule, "anyhow-in-lib-api");
    }

    #[test]
    fn suppressed_findings_serialize_flat_with_reason() {
        let r = scan_one(
            "fn f() { x.unwrap(); } // rustgrade: allow(unwrap-outside-tests) set in new()\n",
        );
        let json = serde_json::to_value(&r.suppressed[0]).expect("serializes");
        assert_eq!(json["rule"], "unwrap-outside-tests");
        assert_eq!(json["span"]["line"], 1);
        assert_eq!(json["reason"], "set in new()");
    }
}
