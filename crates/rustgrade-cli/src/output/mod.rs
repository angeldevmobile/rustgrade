use rustgrade_core::Report;

use crate::Format;

mod json;
mod text;

pub fn render(report: &Report, format: Format) -> anyhow::Result<()> {
    match format {
        Format::Text => text::render(report),
        Format::Json => json::render(report),
        // TODO: sarif (for code scanning) and markdown (for PR comments).
        Format::Sarif | Format::Markdown => {
            anyhow::bail!("this output format is not implemented yet")
        }
    }
}
