//! `rustgrade hook`: a Claude Code `PostToolUse` hook. Reads the hook's JSON from
//! stdin, scans the Rust file the agent just edited and hands the findings back
//! to the agent. A hook must never get in the way of the edit, so anything that
//! goes wrong is a warning on stderr and exit 0.

use std::io::Read;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use rustgrade_core::Severity;
use serde::Deserialize;
use serde_json::json;

use crate::{EXIT_OK, cmd::scan, output::text};

/// Low findings are nits: not worth interrupting the agent on every edit.
const FLOOR: Severity = Severity::Medium;
/// A legacy file can have hundreds; the first ones are the most severe.
const MAX_SHOWN: usize = 10;

#[derive(Deserialize)]
struct Input {
    #[serde(default)]
    cwd: PathBuf,
    tool_input: ToolInput,
}

#[derive(Deserialize)]
struct ToolInput {
    file_path: Option<PathBuf>,
    /// Set by `Edit`; `Write` replaces the whole file.
    new_string: Option<String>,
}

pub fn run() -> u8 {
    let mut stdin = String::new();
    let result = std::io::stdin()
        .read_to_string(&mut stdin)
        .map_err(anyhow::Error::from)
        .and_then(|_| respond(&stdin));
    match result {
        Ok(Some(response)) => println!("{response}"),
        Ok(None) => {}
        Err(e) => eprintln!("rustgrade hook: {e:#}"),
    }
    EXIT_OK
}

/// The JSON to print, or `None` when the agent has nothing to hear about.
fn respond(stdin: &str) -> anyhow::Result<Option<String>> {
    let input: Input = serde_json::from_str(stdin)?;
    let Some(file) = input.tool_input.file_path else {
        return Ok(None);
    };
    if file.extension().is_none_or(|e| e != "rs") {
        return Ok(None);
    }
    let file = input.cwd.join(file);
    if !Path::new(&file).is_file() {
        return Ok(None);
    }
    let Some(mut report) = scan::report_for_file(&file, FLOOR)? else {
        return Ok(None);
    };
    if let Some(new_string) = &input.tool_input.new_string {
        // Old problems elsewhere in the file are not this edit's business: without
        // this, every edit of a legacy file would repeat them. If the text is not
        // there (a format-on-save hook rewrote it), report the whole file instead.
        let content = std::fs::read_to_string(&file)?;
        if let Some(lines) = edited_lines(&content, new_string) {
            report
                .findings
                .retain(|f| lines.iter().any(|l| l.contains(&f.span.line)));
        }
    }
    if report.findings.is_empty() {
        return Ok(None);
    }

    let mut shown = Vec::new();
    text::write_findings(&mut shown, report.findings.iter().take(MAX_SHOWN))?;
    let total = report.findings.len();
    let more = total.saturating_sub(MAX_SHOWN);
    let more = if more > 0 {
        format!("...and {more} more (`rustgrade scan`).\n")
    } else {
        String::new()
    };
    let reason = format!(
        "rustgrade found {total} issue(s) in the code you just wrote:\n{}{more}\
         Fix them before finishing. Only if the code is right and the scanner cannot \
         see why, add `// rustgrade: allow(rule-id) reason`.",
        String::from_utf8_lossy(&shown),
    );
    Ok(Some(
        json!({ "decision": "block", "reason": reason }).to_string(),
    ))
}

/// The 1-based line ranges where `new_string` sits in `content` (several with
/// `replace_all`), or `None` if it is not there. Line endings don't count: the
/// agent sends LF even when the file is CRLF.
fn edited_lines(content: &str, new_string: &str) -> Option<Vec<RangeInclusive<usize>>> {
    let (content, new_string) = (
        content.replace("\r\n", "\n"),
        new_string.replace("\r\n", "\n"),
    );
    let trimmed = new_string.trim_end_matches('\n');
    if trimmed.is_empty() {
        // A deletion wrote nothing to scan.
        return Some(Vec::new());
    }
    let extra_lines = trimmed.matches('\n').count();
    let mut line = 1;
    let mut scanned = 0;
    let ranges: Vec<_> = content
        .match_indices(trimmed)
        .map(|(at, _)| {
            line += content
                .get(scanned..at)
                .map_or(0, |s| s.matches('\n').count());
            scanned = at;
            line..=line + extra_lines
        })
        .collect();
    (!ranges.is_empty()).then_some(ranges)
}
