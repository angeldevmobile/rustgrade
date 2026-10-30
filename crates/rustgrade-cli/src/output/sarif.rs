//! SARIF 2.1.0, for GitHub code scanning (`github/codeql-action/upload-sarif`).

use std::io::{self, Write};
use std::path::Path;

use rustgrade_core::rules::{self, Rule};
use rustgrade_core::suppress::{INVALID, UNUSED};
use rustgrade_core::{Finding, Report, Severity};
use serde_json::{Value, json};

const SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";
const INFORMATION_URI: &str = "https://github.com/angeldevmobile/rustgrade";

pub fn render(report: &Report, root: &Path) -> anyhow::Result<()> {
    write(&to_sarif(report, root, None))
}

pub fn write(log: &Value) -> anyhow::Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, log)?;
    writeln!(out)?;
    Ok(())
}

/// One run with every rule as a descriptor, findings as results and inline
/// suppressions as suppressed results. `properties` lands on the run (audit
/// puts the grade there).
pub fn to_sarif(report: &Report, root: &Path, properties: Option<Value>) -> Value {
    let registry = rules::registry();
    let mut descriptors: Vec<Value> = registry.iter().map(|r| descriptor(*r)).collect();
    descriptors.push(meta_descriptor(
        INVALID,
        "Malformed `rustgrade: allow(...)` comment (unknown rule or missing reason)",
    ));
    descriptors.push(meta_descriptor(
        UNUSED,
        "`rustgrade: allow(...)` comment that suppresses nothing",
    ));
    let index_of = |id: &str| {
        registry
            .iter()
            .map(|r| r.id())
            .chain([INVALID, UNUSED])
            .position(|r| r == id)
    };

    let mut results: Vec<Value> = report
        .findings
        .iter()
        .map(|f| result(f, root, index_of(f.rule)))
        .collect();
    results.extend(report.suppressed.iter().map(|s| {
        let mut r = result(&s.finding, root, index_of(s.finding.rule));
        r["suppressions"] = json!([{ "kind": "inSource", "justification": s.reason }]);
        r
    }));

    let mut run = json!({
        "tool": {
            "driver": {
                "name": "rustgrade",
                "version": env!("CARGO_PKG_VERSION"),
                "informationUri": INFORMATION_URI,
                "rules": descriptors,
            }
        },
        "columnKind": "unicodeCodePoints",
        "results": results,
    });
    if let Some(p) = properties {
        run["properties"] = p;
    }
    json!({ "$schema": SCHEMA, "version": "2.1.0", "runs": [run] })
}

fn descriptor(rule: &dyn Rule) -> Value {
    let explain = rule.explain().trim_end();
    json!({
        "id": rule.id(),
        "shortDescription": { "text": rule.summary() },
        "help": { "text": explain, "markdown": to_markdown(explain) },
        "defaultConfiguration": { "level": level(rule.default_severity()) },
        "properties": {
            "tags": [rule.category().as_str()],
            "severity": rule.default_severity().as_str(),
        },
    })
}

fn meta_descriptor(id: &str, summary: &str) -> Value {
    json!({
        "id": id,
        "shortDescription": { "text": summary },
        "defaultConfiguration": { "level": "warning" },
        "properties": { "tags": ["meta"] },
    })
}

fn result(f: &Finding, root: &Path, rule_index: Option<usize>) -> Value {
    let text = match &f.help {
        Some(help) => format!("{}. Help: {help}", f.message),
        None => f.message.clone(),
    };
    let mut r = json!({
        "ruleId": f.rule,
        "level": level(f.severity),
        "message": { "text": text },
        "locations": [{
            "physicalLocation": {
                "artifactLocation": { "uri": artifact_uri(root, &f.span.file) },
                "region": {
                    "startLine": f.span.line,
                    "startColumn": f.span.column,
                    "endLine": f.span.end_line,
                    "endColumn": f.span.end_column,
                },
            }
        }],
        "properties": {
            "severity": f.severity.as_str(),
            "category": f.category.as_str(),
        },
    });
    if let Some(i) = rule_index {
        r["ruleIndex"] = json!(i);
    }
    r
}

/// `--explain` texts indent code under a `Before:` line, but an indented block
/// cannot interrupt a Markdown paragraph: give each block a blank line first.
fn to_markdown(explain: &str) -> String {
    let mut out = String::with_capacity(explain.len() + 8);
    let mut prev_is_prose = false;
    for line in explain.lines() {
        let is_code = line.starts_with("    ");
        if is_code && prev_is_prose {
            out.push('\n');
        }
        out.push_str(line);
        out.push('\n');
        prev_is_prose = !is_code && !line.is_empty();
    }
    out.truncate(out.trim_end().len());
    out
}

/// SARIF has three useful levels; rustgrade has five severities.
fn level(s: Severity) -> &'static str {
    match s {
        Severity::Critical | Severity::High => "error",
        Severity::Medium => "warning",
        Severity::Low | Severity::Info => "note",
    }
}

/// Finding paths are relative to the scan root; code scanning resolves URIs
/// against the checkout, so prefix the root as given (CI runs from the repo
/// root). Absolute roots become `file://` URIs.
fn artifact_uri(root: &Path, file: &str) -> String {
    let path = if root.is_file() || root == Path::new(".") {
        file.to_owned()
    } else {
        let root = root.to_string_lossy().replace('\\', "/");
        format!("{}/{file}", root.trim_end_matches('/'))
    };
    let path = path.strip_prefix("./").unwrap_or(&path);
    if root.is_absolute() {
        let path = path.replace(' ', "%20");
        if path.starts_with('/') {
            format!("file://{path}")
        } else {
            format!("file:///{path}")
        }
    } else {
        path.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severities_map_to_three_levels() {
        assert_eq!(level(Severity::Critical), "error");
        assert_eq!(level(Severity::High), "error");
        assert_eq!(level(Severity::Medium), "warning");
        assert_eq!(level(Severity::Low), "note");
        assert_eq!(level(Severity::Info), "note");
    }

    #[test]
    fn explain_code_becomes_markdown_code_blocks() {
        assert_eq!(
            to_markdown("Why: x.\n\nBefore:\n    a();\n    b();\n\nAfter:\n    c();"),
            "Why: x.\n\nBefore:\n\n    a();\n    b();\n\nAfter:\n\n    c();"
        );
    }

    #[test]
    fn uris_are_relative_to_where_rustgrade_runs() {
        assert_eq!(artifact_uri(Path::new("."), "src/lib.rs"), "src/lib.rs");
        assert_eq!(
            artifact_uri(Path::new("crates/core"), "src/lib.rs"),
            "crates/core/src/lib.rs"
        );
        assert_eq!(
            artifact_uri(Path::new("./crates/core/"), "src/lib.rs"),
            "crates/core/src/lib.rs"
        );
        assert_eq!(
            artifact_uri(Path::new(r"crates\core"), "src/lib.rs"),
            "crates/core/src/lib.rs"
        );
    }
}
