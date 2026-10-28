use std::io::{self, Write};

use rustgrade_core::Report;

pub fn render(report: &Report) -> anyhow::Result<()> {
    let mut out = io::stdout().lock();
    for f in &report.findings {
        writeln!(
            out,
            "{}:{}:{}: {:?} [{}] {}",
            f.span.file, f.span.line, f.span.column, f.severity, f.rule, f.message
        )?;
        if let Some(help) = &f.help {
            writeln!(out, "    help: {help}")?;
        }
    }
    writeln!(
        out,
        "{} finding(s) in {} file(s)",
        report.findings.len(),
        report.files_scanned
    )?;
    Ok(())
}
