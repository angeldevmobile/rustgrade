use std::io::{self, Write};

use rustgrade_core::{Finding, Report};

pub fn render(report: &Report) -> anyhow::Result<()> {
    let mut out = io::stdout().lock();
    write_findings(&mut out, &report.findings)?;
    write!(
        out,
        "{} finding(s) in {} file(s)",
        report.findings.len(),
        report.files_scanned
    )?;
    if report.suppressed.is_empty() {
        writeln!(out)?;
    } else {
        writeln!(
            out,
            ", {} suppressed (see --format json)",
            report.suppressed.len()
        )?;
    }
    Ok(())
}

/// One `file:line:col: severity [rule] message` line per finding, plus help.
pub fn write_findings<'a>(
    out: &mut impl Write,
    findings: impl IntoIterator<Item = &'a Finding>,
) -> io::Result<()> {
    for f in findings {
        writeln!(
            out,
            "{}:{}:{}: {} [{}] {}",
            f.span.file, f.span.line, f.span.column, f.severity, f.rule, f.message
        )?;
        if let Some(help) = &f.help {
            writeln!(out, "    help: {help}")?;
        }
    }
    Ok(())
}
