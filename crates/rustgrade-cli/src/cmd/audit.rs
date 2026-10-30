//! `rustgrade audit`: scanner + clippy pedantic + tests, graded with the rubric
//! in `rustgrade_core::score` (documented in `skills/rustgrade/references/rubric.md`).

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;

use rustgrade_core::score::{self, Deduction};
use rustgrade_core::{Area, Cap, Finding, Report, SCHEMA_VERSION, Scorecard, Severity};
use serde_json::json;

use crate::cargo::{self, Clippy, ClippyWarning, Tests};
use crate::cmd::scan;
use crate::output::sarif;
use crate::output::text::write_findings;
use crate::{EXIT_FINDINGS, EXIT_OK, FailOn, Format, Globals};

pub struct Options {
    pub clippy: bool,
    pub tests: bool,
}

enum Outcome {
    Graded {
        card: Scorecard,
        clippy: Option<Vec<ClippyWarning>>,
        tests: Option<Tests>,
    },
    DoesNotCompile {
        error: String,
    },
}

pub fn run(path: Option<&Path>, opts: &Options, globals: &Globals) -> anyhow::Result<u8> {
    if matches!(globals.format, Format::Markdown) {
        anyhow::bail!("`audit` supports --format text, json or sarif");
    }
    let root = path.unwrap_or(Path::new("."));
    scan::ensure_exists(root)?;
    let manifest = root.join("Cargo.toml");
    if !manifest.is_file() {
        anyhow::bail!(
            "no Cargo.toml in {}: `audit` needs a package or workspace (use `rustgrade scan` for loose files)",
            root.display()
        );
    }

    // Grading uses every finding; `--min-severity` only filters what is shown.
    let report = scan::build_report(root, None)?;
    let outcome = grade_crate(&report, &manifest, opts)?;

    match globals.format {
        Format::Json => {
            let value = to_json(&report, &outcome);
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Format::Sarif => {
            let grade = sarif_properties(&report, &outcome);
            sarif::write(&sarif::to_sarif(&report, root, Some(grade)))?;
        }
        Format::Text | Format::Markdown => {
            render_text(&report, &outcome, globals.min_severity)?;
        }
    }

    let failed = match (&globals.fail_on, &outcome) {
        (None, _) => false,
        // An audit that cannot build cannot vouch for anything.
        (Some(_), Outcome::DoesNotCompile { .. }) => true,
        (Some(FailOn::Grade(g)), Outcome::Graded { card, .. }) => card.grade >= *g,
        (Some(FailOn::Severity(s)), Outcome::Graded { .. }) => {
            report.findings.iter().any(|f| f.severity >= *s)
        }
    };
    Ok(if failed { EXIT_FINDINGS } else { EXIT_OK })
}

fn grade_crate(report: &Report, manifest: &Path, opts: &Options) -> anyhow::Result<Outcome> {
    let clippy = if opts.clippy {
        eprintln!("rustgrade: running cargo clippy (pedantic)...");
        match cargo::clippy(manifest)? {
            Clippy::Ran(warnings) => Some(warnings),
            Clippy::CompileError(error) => return Ok(Outcome::DoesNotCompile { error }),
        }
    } else {
        None
    };
    let tests = if opts.tests {
        eprintln!("rustgrade: running cargo test...");
        Some(cargo::tests(manifest)?)
    } else {
        None
    };

    let mut issues: Vec<Deduction<'_>> = report.findings.iter().map(Deduction::from).collect();
    if let Some(warnings) = &clippy {
        issues.extend(warnings.iter().map(|w| Deduction {
            rule: &w.lint,
            category: w.category,
            severity: Severity::Low,
        }));
    }
    let mut applicable = Vec::new();
    if report.usage.unsafe_code {
        applicable.push(Area::Unsafe);
    }
    if report.usage.concurrency {
        applicable.push(Area::Concurrency);
    }
    let tests_failed = tests.as_ref().is_some_and(|t| !t.ok);
    let card = score::grade(&issues, &applicable, tests_failed, report.usage.code_lines);

    Ok(Outcome::Graded {
        card,
        clippy,
        tests,
    })
}

fn to_json(report: &Report, outcome: &Outcome) -> serde_json::Value {
    match outcome {
        Outcome::DoesNotCompile { error } => json!({
            "schema": SCHEMA_VERSION,
            "grade": "F",
            "score": 0,
            "cap": "does-not-compile",
            "error": error,
            "scan": report,
        }),
        Outcome::Graded {
            card,
            clippy,
            tests,
        } => {
            let tests = match tests {
                None => json!({ "status": "skipped" }),
                Some(t) => json!({
                    "status": test_status(t),
                    "passed": t.passed,
                    "failed": t.failed,
                }),
            };
            let clippy = match clippy {
                None => json!({ "status": "skipped" }),
                Some(w) => json!({
                    "status": "ran",
                    "warnings": w.len(),
                    "lints": lint_counts(w),
                }),
            };
            json!({
                "schema": SCHEMA_VERSION,
                "grade": card.grade,
                "score": card.score,
                "cap": card.cap,
                "code_lines": report.usage.code_lines,
                "highs_per_kloc": round1(card.highs_per_kloc),
                "areas": card.areas,
                "tests": tests,
                "clippy": clippy,
                "scan": report,
            })
        }
    }
}

/// The grade rides on the SARIF run; clippy warnings carry no location, so
/// only their count goes along.
fn sarif_properties(report: &Report, outcome: &Outcome) -> serde_json::Value {
    match outcome {
        Outcome::DoesNotCompile { error } => json!({
            "grade": "F",
            "score": 0,
            "cap": "does-not-compile",
            "error": error,
        }),
        Outcome::Graded {
            card,
            clippy,
            tests,
        } => json!({
            "grade": card.grade,
            "score": card.score,
            "cap": card.cap,
            "codeLines": report.usage.code_lines,
            "clippyWarnings": clippy.as_ref().map(Vec::len),
            "tests": tests.as_ref().map(test_status),
        }),
    }
}

fn render_text(report: &Report, outcome: &Outcome, min: Option<Severity>) -> io::Result<()> {
    let mut out = io::stdout().lock();
    let (card, clippy, tests) = match outcome {
        Outcome::DoesNotCompile { error } => {
            return writeln!(
                out,
                "rustgrade audit: F, the crate does not compile\n\n{error}"
            );
        }
        Outcome::Graded {
            card,
            clippy,
            tests,
        } => (card, clippy, tests),
    };

    write!(out, "rustgrade audit: {:?} ({})", card.grade, card.score)?;
    let highs = count(&report.findings, Severity::High);
    let plural = if highs == 1 { "" } else { "s" };
    // Small crates are graded on counts; only large ones need the ratio.
    let density = if report.usage.code_lines > 1000 {
        format!(" ({:.1} per 1000 lines)", card.highs_per_kloc)
    } else {
        String::new()
    };
    match card.cap {
        None => writeln!(out)?,
        Some(Cap::OneHigh | Cap::ManyHigh) => {
            writeln!(out, ", capped by {highs} high finding{plural}{density}")?;
        }
        Some(Cap::Critical) => writeln!(out, ", capped by a critical finding")?,
        Some(Cap::TestsFailing) => writeln!(out, ", capped by failing tests")?,
    }
    writeln!(out)?;

    for a in &card.areas {
        write!(out, "  {:<22}", area_label(a.area))?;
        match a.score {
            Some(score) => write!(out, "{score:>4}  ")?,
            None => write!(out, " n/a  ")?,
        }
        write_area_note(
            &mut out,
            a.area,
            a.score.is_some(),
            report,
            clippy.as_deref(),
        )?;
        writeln!(out)?;
    }
    writeln!(out)?;

    match tests {
        None => writeln!(out, "  Tests    skipped (--no-tests)")?,
        Some(t) => writeln!(
            out,
            "  Tests    {} passed, {} failed{}",
            t.passed,
            t.failed,
            if test_status(t) == "did-not-build" {
                " (tests did not build)"
            } else {
                ""
            }
        )?,
    }
    match clippy {
        None => writeln!(out, "  Clippy   skipped (--no-clippy)")?,
        Some(w) => {
            write!(out, "  Clippy   {} warning(s), pedantic", w.len())?;
            let mut top: Vec<_> = lint_counts(w).into_iter().collect();
            top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            for (i, (lint, n)) in top.iter().take(5).enumerate() {
                let sep = if i == 0 { ": " } else { ", " };
                write!(out, "{sep}{lint} x{n}")?;
            }
            writeln!(out)?;
        }
    }
    writeln!(
        out,
        "  Scanner  {} finding(s), {} suppressed, {} file(s), {} lines of code",
        report.findings.len(),
        report.suppressed.len(),
        report.files_scanned,
        report.usage.code_lines
    )?;

    let shown = report
        .findings
        .iter()
        .filter(|f| min.is_none_or(|m| f.severity >= m));
    let hidden = report.findings.len() - shown.clone().count();
    if hidden < report.findings.len() {
        writeln!(out, "\nFindings:")?;
        write_findings(&mut out, shown)?;
    }
    if hidden > 0 {
        writeln!(out, "({hidden} below --min-severity not shown)")?;
    }
    Ok(())
}

fn area_label(area: Area) -> &'static str {
    match area {
        Area::Errors => "Correctness & errors",
        Area::Unsafe => "Unsafe",
        Area::Concurrency => "Concurrency",
        Area::Performance => "Performance",
        Area::OwnershipApi => "Ownership & API",
    }
}

/// "1 high, 2 medium, 3 clippy" for an area, or why it is `n/a`.
fn write_area_note(
    out: &mut impl Write,
    area: Area,
    applies: bool,
    report: &Report,
    clippy: Option<&[ClippyWarning]>,
) -> io::Result<()> {
    if !applies {
        let why = match area {
            Area::Unsafe => "no unsafe code",
            Area::Concurrency => "no async, threads, locks or channels",
            _ => "",
        };
        return write!(out, "{why}");
    }
    let in_area: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| Area::of(f.category) == area)
        .collect();
    let mut first = true;
    for sev in [
        Severity::Critical,
        Severity::High,
        Severity::Medium,
        Severity::Low,
    ] {
        let n = in_area.iter().filter(|f| f.severity == sev).count();
        if n > 0 {
            write!(out, "{}{n} {sev}", if first { "" } else { ", " })?;
            first = false;
        }
    }
    let lints = clippy.map_or(0, |w| {
        w.iter().filter(|w| Area::of(w.category) == area).count()
    });
    if lints > 0 {
        write!(out, "{}{lints} clippy", if first { "" } else { ", " })?;
        first = false;
    }
    if first {
        write!(out, "-")?;
    }
    Ok(())
}

fn count(findings: &[Finding], severity: Severity) -> usize {
    findings.iter().filter(|f| f.severity == severity).count()
}

fn lint_counts(warnings: &[ClippyWarning]) -> BTreeMap<&str, u32> {
    let mut counts = BTreeMap::new();
    for w in warnings {
        *counts.entry(w.lint.as_str()).or_default() += 1;
    }
    counts
}

fn test_status(t: &Tests) -> &'static str {
    match (t.ok, t.passed + t.failed) {
        (true, _) => "passed",
        (false, 0) => "did-not-build",
        (false, _) => "failed",
    }
}

/// One decimal, for JSON readers.
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}
