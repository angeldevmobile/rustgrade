use std::path::Path;

use rustgrade_core::Report;

use crate::Format;

mod json;
pub mod sarif;
pub mod text;

/// `root` is the scan root as given, which SARIF needs to build file URIs.
pub fn render(report: &Report, root: &Path, format: Format) -> anyhow::Result<()> {
    match format {
        Format::Text => text::render(report),
        Format::Json => json::render(report),
        Format::Sarif => sarif::render(report, root),
        // TODO: markdown (for PR comments).
        Format::Markdown => anyhow::bail!("this output format is not implemented yet"),
    }
}
