use std::io::{self, Write};

use rustgrade_core::Report;

pub fn render(report: &Report) -> anyhow::Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, report)?;
    writeln!(out)?;
    Ok(())
}
