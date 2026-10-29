//! Inline suppressions: `// rustgrade: allow(rule-id, ...) reason`.
//!
//! A trailing comment covers its own line. A comment on its own line covers the
//! next code line, skipping other comments and attributes; a blank line breaks it.
//! The reason is mandatory, so every silenced finding says why.

use serde::Serialize;

use crate::finding::{Category, Finding, Severity, Span};

pub const INVALID: &str = "invalid-suppression";
pub const UNUSED: &str = "unused-suppression";

/// A finding silenced by an inline comment, kept in the report for auditing.
#[derive(Debug, Clone, Serialize)]
pub struct Suppressed {
    #[serde(flatten)]
    pub finding: Finding,
    pub reason: String,
}

struct Allow<'s> {
    span: Span,
    target_line: Option<usize>,
    rules: Vec<&'s str>,
    reason: &'s str,
    used: bool,
}

/// Splits `findings` into kept and suppressed. Malformed and unused
/// suppressions are added to the kept findings as `invalid-suppression` /
/// `unused-suppression`.
pub(crate) fn apply(
    path: &str,
    source: &str,
    findings: Vec<Finding>,
    is_known: impl Fn(&str) -> bool,
    is_enabled: impl Fn(&str) -> bool,
) -> (Vec<Finding>, Vec<Suppressed>) {
    if !source.contains(MARKER) {
        return (findings, Vec::new());
    }
    let lines: Vec<&str> = source.lines().collect();
    let literals = literal_ranges(source);
    let mut kept = Vec::new();
    let mut allows = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let Some((col, body)) = find_marker(line, idx + 1, &literals) else {
            continue;
        };
        let span = Span {
            // rustgrade: allow(clone-in-loop) one owned Span per suppression comment, which are rare
            file: path.to_owned(),
            line: idx + 1,
            column: line[..col].chars().count() + 1,
            end_line: idx + 1,
            end_column: line.chars().count() + 1,
        };
        let parsed = parse(body).and_then(|(rules, reason)| {
            if let Some(bad) = rules.iter().find(|r| !is_known(r)) {
                return Err(format!("unknown rule `{bad}`"));
            }
            if reason.is_empty() {
                return Err("suppression needs a reason after `)`".to_owned());
            }
            Ok((rules, reason))
        });
        match parsed {
            Err(message) => {
                if is_enabled(INVALID) {
                    kept.push(meta(INVALID, span, message));
                }
            }
            Ok((rules, reason)) => {
                let own_line = line[..col].trim().is_empty();
                let target_line = if own_line {
                    next_code_line(&lines, idx + 1)
                } else {
                    Some(idx + 1)
                };
                allows.push(Allow {
                    span,
                    target_line,
                    rules,
                    reason,
                    used: false,
                });
            }
        }
    }

    let mut suppressed = Vec::new();
    for f in findings {
        let allow = allows
            .iter_mut()
            .find(|a| a.target_line == Some(f.span.line) && a.rules.contains(&f.rule));
        match allow {
            Some(a) => {
                a.used = true;
                suppressed.push(Suppressed {
                    finding: f,
                    reason: a.reason.to_owned(),
                });
            }
            None => kept.push(f),
        }
    }

    // A rule disabled in rustgrade.toml produces nothing, so its allows are not stale.
    if is_enabled(UNUSED) {
        for a in allows {
            if !a.used && a.rules.iter().any(|r| is_enabled(r)) {
                kept.push(meta(
                    UNUSED,
                    a.span,
                    "suppression matches no finding; remove it".to_owned(),
                ));
            }
        }
    }
    (kept, suppressed)
}

const MARKER: &str = "rustgrade:";

/// Start and end `(line, column)` of a literal: 1-based line, 0-based char column.
type Range = ((usize, usize), (usize, usize));

/// Ranges of string/char literals and doc comments, so a marker quoted
/// inside them is not mistaken for a suppression.
fn literal_ranges(source: &str) -> Vec<Range> {
    fn walk(tokens: proc_macro2::TokenStream, out: &mut Vec<Range>) {
        for tt in tokens {
            match tt {
                proc_macro2::TokenTree::Literal(l) => {
                    let (s, e) = (l.span().start(), l.span().end());
                    out.push(((s.line, s.column), (e.line, e.column)));
                }
                proc_macro2::TokenTree::Group(g) => walk(g.stream(), out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    if let Ok(tokens) = source.parse() {
        walk(tokens, &mut out);
    }
    out
}

/// Finds the line's real comment (the first `//` outside a literal) and, if it
/// is a suppression, returns its byte offset and the text after `rustgrade:`.
/// Doc comments (`///`, `//!`) are not suppressions.
fn find_marker<'l>(line: &'l str, line_no: usize, literals: &[Range]) -> Option<(usize, &'l str)> {
    if !line.contains(MARKER) {
        return None;
    }
    let (c, _) = line.match_indices("//").find(|&(c, _)| {
        let pos = (line_no, line[..c].chars().count());
        !literals
            .iter()
            .any(|&(start, end)| start <= pos && pos < end)
    })?;
    line[c + 2..]
        .trim_start()
        .strip_prefix(MARKER)
        .map(|rest| (c, rest.trim_start()))
}

fn parse(body: &str) -> Result<(Vec<&str>, &str), String> {
    const USAGE: &str = "expected `// rustgrade: allow(rule-id) reason`";
    let (ids, reason) = body
        .strip_prefix("allow(")
        .and_then(|rest| rest.split_once(')'))
        .ok_or_else(|| USAGE.to_owned())?;
    let rules: Vec<&str> = ids
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if rules.is_empty() {
        return Err(USAGE.to_owned());
    }
    let reason = reason
        .trim()
        .trim_start_matches(['-', '\u{2013}', '\u{2014}', ':'])
        .trim();
    Ok((rules, reason))
}

/// 1-based line of the first code line at or after index `from`.
fn next_code_line(lines: &[&str], from: usize) -> Option<usize> {
    for (idx, line) in lines.iter().enumerate().skip(from) {
        let t = line.trim();
        if t.is_empty() {
            return None;
        }
        if !(t.starts_with("//") || t.starts_with("#[")) {
            return Some(idx + 1);
        }
    }
    None
}

fn meta(rule: &'static str, span: Span, message: String) -> Finding {
    Finding {
        rule,
        severity: Severity::Low,
        category: Category::Meta,
        message,
        span,
        help: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(rule: &'static str, line: usize) -> Finding {
        Finding {
            rule,
            severity: Severity::High,
            category: Category::Errors,
            message: String::new(),
            span: Span {
                file: "src/lib.rs".into(),
                line,
                column: 1,
                end_line: line,
                end_column: 1,
            },
            help: None,
        }
    }

    /// Returns (kept rule ids, suppressed (rule, reason)).
    fn run(src: &str, findings: Vec<Finding>) -> (Vec<&'static str>, Vec<(&'static str, String)>) {
        let (kept, suppressed) = apply(
            "src/lib.rs",
            src,
            findings,
            |id| matches!(id, "a" | "b"),
            |id| id != "disabled",
        );
        (
            kept.iter().map(|f| f.rule).collect(),
            suppressed
                .into_iter()
                .map(|s| (s.finding.rule, s.reason))
                .collect(),
        )
    }

    #[test]
    fn trailing_comment_covers_its_line() {
        let src = "x();\ny(); // rustgrade: allow(a) checked by caller\n";
        let (kept, suppressed) = run(src, vec![finding("a", 2), finding("a", 1)]);
        assert_eq!(kept, ["a"]);
        assert_eq!(suppressed, [("a", "checked by caller".to_owned())]);
    }

    #[test]
    fn own_line_comment_covers_next_code_line_through_docs_and_attrs() {
        let src =
            "// rustgrade: allow(a, b) -- stub until v0.6\n/// Docs.\n#[inline]\npub fn f() {}\n";
        let (kept, suppressed) = run(src, vec![finding("a", 4), finding("b", 4)]);
        assert_eq!(kept, Vec::<&str>::new());
        assert_eq!(suppressed.len(), 2);
        assert_eq!(suppressed[0].1, "stub until v0.6");
    }

    #[test]
    fn only_listed_rules_are_suppressed() {
        let src = "// rustgrade: allow(a) reason\nf();\n";
        let (kept, _) = run(src, vec![finding("b", 2)]);
        assert_eq!(kept, ["b", UNUSED]);
    }

    #[test]
    fn blank_line_breaks_the_link() {
        let src = "// rustgrade: allow(a) reason\n\nf();\n";
        let (kept, suppressed) = run(src, vec![finding("a", 3)]);
        assert_eq!(kept, ["a", UNUSED]);
        assert_eq!(suppressed, Vec::<(&str, String)>::new());
    }

    #[test]
    fn invalid_suppressions_do_not_apply() {
        for src in [
            "f(); // rustgrade: allow(a)\n",
            "f(); // rustgrade: allow(a) -- \n",
            "f(); // rustgrade: allow(nope) reason\n",
            "f(); // rustgrade: allow() reason\n",
            "f(); // rustgrade: deny(a) reason\n",
        ] {
            let (kept, suppressed) = run(src, vec![finding("a", 1)]);
            assert_eq!(kept, [INVALID, "a"], "{src}");
            assert!(suppressed.is_empty(), "{src}");
        }
    }

    #[test]
    fn ignores_doc_comments_and_other_comments() {
        let src = "/// rustgrade: allow(a) docs mention it\n// see https://example.com\nf();\n";
        let (kept, suppressed) = run(src, vec![finding("a", 3)]);
        assert_eq!(kept, ["a"]);
        assert_eq!(suppressed, Vec::<(&str, String)>::new());
    }

    #[test]
    fn ignores_markers_quoted_in_strings_and_docs() {
        let src = concat!(
            "//! Use `// rustgrade: allow(rule-id) reason` to silence a finding.\n",
            "/// Example: `x(); // rustgrade: allow(a)`\n",
            "const S: &str = \"f(); // rustgrade: allow(a)\";\n",
            "const R: &str = r#\"\n",
            "    // rustgrade: allow(nope)\n",
            "\"#;\n",
            "f(\"//\"); // rustgrade: allow(a) real one after a string with slashes\n",
        );
        let (kept, suppressed) = run(src, vec![finding("a", 7)]);
        assert!(kept.is_empty(), "{kept:?}");
        assert_eq!(suppressed.len(), 1);
    }

    #[test]
    fn allows_for_disabled_rules_are_not_unused() {
        let src = "f(); // rustgrade: allow(a) reason\n";
        let (kept, _) = apply("src/lib.rs", src, vec![], |_| true, |id| id != "a");
        assert!(kept.is_empty());
    }
}
