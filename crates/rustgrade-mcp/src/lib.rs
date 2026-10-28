//! Optional MCP adapter. A thin layer over `rustgrade-core`: if MCP is ever
//! superseded, this crate is deleted and nothing else changes.

mod tools;

/// Serves the rustgrade tools over stdio.
///
/// # Errors
/// Always, until the adapter is implemented.
// rustgrade: allow(anyhow-in-lib-api) stub; gets a typed error when the adapter lands (v0.6)
pub fn serve_stdio() -> anyhow::Result<()> {
    let _ = tools::NAMES;
    anyhow::bail!("the MCP server is not implemented yet")
}
