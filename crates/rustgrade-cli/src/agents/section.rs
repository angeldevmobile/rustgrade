//! The rustgrade section of an agent's instructions file (`CLAUDE.md`,
//! `AGENTS.md`, ...): a marker-delimited block that `install` adds or updates
//! and `uninstall` removes, never touching the rest of the user's file.
//!
//! Agents read their instructions file on every request, while a skill only
//! loads when the model decides its description applies; evals/agents measured
//! the skill activating in 0 of 6 runs without this section and 3 of 3 with it.

use std::ops::Range;

const BEGIN: &str = "<!-- rustgrade:begin (managed by `rustgrade install`) -->";
/// Matched without the note, so a hand-trimmed marker still counts.
const BEGIN_PREFIX: &str = "<!-- rustgrade:begin";
const END: &str = "<!-- rustgrade:end -->";

/// The block for `body`, markers and trailing newline included.
pub fn block(body: &str) -> String {
    format!("{BEGIN}\n{}\n{END}\n", body.trim_end())
}

/// Byte range of the block in `text`, with the newline after the end marker.
fn range(text: &str) -> Option<Range<usize>> {
    let start = text.find(BEGIN_PREFIX)?;
    let mut end = start + text[start..].find(END)? + END.len();
    for nl in ["\r\n", "\n"] {
        if text[end..].starts_with(nl) {
            end += nl.len();
            break;
        }
    }
    Some(start..end)
}

/// The block as it is in `text`, if there is one.
pub fn extract(text: &str) -> Option<&str> {
    range(text).map(|r| &text[r])
}

/// `existing` (or a new file) with `block` in place of the old one, or
/// appended after a blank line.
pub fn upsert(existing: Option<&str>, block: &str) -> String {
    let Some(text) = existing.filter(|t| !t.trim().is_empty()) else {
        return block.to_owned();
    };
    if let Some(r) = range(text) {
        return format!("{}{block}{}", &text[..r.start], &text[r.end..]);
    }
    let sep = if text.ends_with("\n\n") {
        ""
    } else if text.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!("{text}{sep}{block}")
}

/// `text` without the block and the blank line `upsert` put before it. Empty
/// when the block was all there was.
pub fn remove(text: &str) -> String {
    let Some(r) = range(text) else {
        return text.to_owned();
    };
    let before = text[..r.start].trim_end_matches(['\r', '\n']);
    let after = text[r.end..].trim_start_matches(['\r', '\n']);
    match (before.is_empty(), after.is_empty()) {
        (true, _) => after.to_owned(),
        (false, true) => format!("{before}\n"),
        (false, false) => format!("{before}\n\n{after}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "## Rust\n\nUse rustgrade.\n";

    #[test]
    fn new_file_is_just_the_block() {
        let b = block(BODY);
        assert_eq!(upsert(None, &b), b);
        assert_eq!(upsert(Some("\n"), &b), b);
        assert_eq!(remove(&b), "");
    }

    #[test]
    fn appends_after_a_blank_line_and_removes_cleanly() {
        let b = block(BODY);
        for original in ["# Project\n\nNotes.\n", "# Project", "# Project\n\n"] {
            let with = upsert(Some(original), &b);
            assert!(with.starts_with(original.trim_end()), "{original:?}");
            assert!(with.contains("\n\n<!-- rustgrade:begin"), "{with:?}");
            assert_eq!(extract(&with), Some(b.as_str()));
            assert_eq!(remove(&with).trim_end(), original.trim_end());
        }
        assert_eq!(remove(&upsert(Some("# P\n"), &b)), "# P\n");
    }

    #[test]
    fn replaces_in_place_keeping_what_surrounds_it() {
        let old = block("## Rust\n\nOld text.");
        let text = format!("# Top\n\n{old}\n## After\n");
        let new = block(BODY);
        let updated = upsert(Some(&text), &new);
        assert_eq!(updated, format!("# Top\n\n{new}\n## After\n"));
        assert_eq!(remove(&updated), "# Top\n\n## After\n");
    }

    #[test]
    fn handles_crlf_files() {
        let b = block(BODY).replace('\n', "\r\n");
        let text = format!("# Top\r\n\r\n{b}");
        assert_eq!(extract(&text), Some(b.as_str()));
        assert_eq!(remove(&text), "# Top\n");
    }

    #[test]
    fn text_without_a_block_is_left_alone() {
        assert_eq!(extract("# Notes\n"), None);
        assert_eq!(remove("# Notes\n"), "# Notes\n");
        let unterminated = "<!-- rustgrade:begin -->\nhalf";
        assert_eq!(extract(unterminated), None);
    }
}
