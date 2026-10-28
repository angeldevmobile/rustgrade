use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity, Span};

pub struct UndocumentedUnsafe;

impl Rule for UndocumentedUnsafe {
    fn id(&self) -> &'static str {
        "undocumented-unsafe"
    }

    fn category(&self) -> Category {
        Category::Unsafe
    }

    fn default_severity(&self) -> Severity {
        Severity::Critical
    }

    fn summary(&self) -> &'static str {
        "`unsafe` block or impl without a `// SAFETY:` comment"
    }

    // Applies to test code too: unsound tests are still unsound.
    fn check(&self, ctx: &FileContext<'_>, out: &mut Vec<Finding>) {
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut v = Visitor {
            rule: self,
            path: ctx.path,
            lines: &lines,
            stmt_lines: Vec::new(),
            out,
        };
        v.visit_file(ctx.ast);
    }
}

struct Visitor<'a, 'o> {
    rule: &'a UndocumentedUnsafe,
    path: &'a str,
    lines: &'a [&'a str],
    /// Start line of each enclosing statement, innermost last. A comment above
    /// a multi-line `let` covers an `unsafe` block further down in it.
    stmt_lines: Vec<usize>,
    out: &'o mut Vec<Finding>,
}

impl Visitor<'_, '_> {
    fn check_token(&mut self, token: proc_macro2::Span, what: &str) {
        let start = token.start();
        let documented = has_safety_comment(self.lines, start.line, start.column)
            || self
                .stmt_lines
                .last()
                .is_some_and(|&l| l != start.line && has_safety_comment(self.lines, l, 0));
        if !documented {
            self.out.push(Finding {
                rule: self.rule.id(),
                severity: self.rule.default_severity(),
                category: self.rule.category(),
                message: format!("`unsafe` {what} without a `// SAFETY:` comment"),
                span: Span::from_proc_macro(self.path, token),
                help: Some(
                    "state the invariants that make this sound, e.g. `// SAFETY: idx < len, checked above`"
                        .into(),
                ),
            });
        }
    }
}

impl<'ast> Visit<'ast> for Visitor<'_, '_> {
    fn visit_stmt(&mut self, s: &'ast syn::Stmt) {
        self.stmt_lines.push(s.span().start().line);
        visit::visit_stmt(self, s);
        self.stmt_lines.pop();
    }

    fn visit_expr_unsafe(&mut self, e: &'ast syn::ExprUnsafe) {
        self.check_token(e.unsafe_token.span, "block");
        visit::visit_expr_unsafe(self, e);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if let Some(token) = i.unsafety {
            self.check_token(token.span, "impl");
        }
        visit::visit_item_impl(self, i);
    }
}

/// `line` is 1-based, `column` 0-based in chars (as `proc_macro2` reports them).
///
/// Accepts `SAFETY:` earlier on the same line, or in the run of comment and
/// attribute lines directly above. A blank line breaks the run.
fn has_safety_comment(lines: &[&str], line: usize, column: usize) -> bool {
    let Some(current) = line.checked_sub(1).and_then(|i| lines.get(i)) else {
        return false;
    };
    let prefix_end = current
        .char_indices()
        .nth(column)
        .map_or(current.len(), |(i, _)| i);
    if current[..prefix_end].contains("SAFETY:") {
        return true;
    }
    lines[..line - 1]
        .iter()
        .rev()
        .map(|l| l.trim_start())
        .take_while(|l| {
            l.starts_with("//") || l.starts_with("/*") || l.starts_with('*') || l.starts_with("#[")
        })
        .any(|l| l.contains("SAFETY:"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::run;

    #[test]
    fn flags_bare_block_with_location() {
        let found = run(
            &UndocumentedUnsafe,
            "fn f(p: *const u8) -> u8 {\n    unsafe { *p }\n}",
        );
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].span.line, found[0].span.column), (2, 5));
    }

    #[test]
    fn accepts_comment_directly_above() {
        let src = "fn f(p: *const u8) -> u8 {
            // SAFETY: caller guarantees `p` is valid
            // and aligned.
            unsafe { *p }
        }";
        assert!(run(&UndocumentedUnsafe, src).is_empty());
    }

    #[test]
    fn accepts_comment_above_multiline_statement() {
        let src = "fn f(v: &[u8]) -> u8 {
            // SAFETY: `v` is non-empty, checked by the caller.
            let x = g(
                unsafe { *v.get_unchecked(0) },
            );
            x
        }";
        assert!(run(&UndocumentedUnsafe, src).is_empty());
    }

    #[test]
    fn accepts_inline_comment_on_same_line() {
        let src = "fn f(p: *const u8) -> u8 { /* SAFETY: valid */ unsafe { *p } }";
        assert!(run(&UndocumentedUnsafe, src).is_empty());
    }

    #[test]
    fn blank_line_breaks_the_comment() {
        let src = "fn f(p: *const u8) -> u8 {
            // SAFETY: valid

            unsafe { *p }
        }";
        assert_eq!(run(&UndocumentedUnsafe, src).len(), 1);
    }

    #[test]
    fn other_comments_do_not_count() {
        let src = "fn f(p: *const u8) -> u8 {
            // this is fine, trust me
            unsafe { *p }
        }";
        assert_eq!(run(&UndocumentedUnsafe, src).len(), 1);
    }

    #[test]
    fn checks_unsafe_impls_through_attributes() {
        let bare = "struct S; unsafe impl Send for S {}";
        assert_eq!(
            run(&UndocumentedUnsafe, bare)[0].message,
            "`unsafe` impl without a `// SAFETY:` comment"
        );

        let documented = "struct S;
            // SAFETY: S owns no thread-bound data.
            /// Docs in between are fine.
            #[allow(clippy::non_send_fields_in_send_ty)]
            unsafe impl Send for S {}";
        assert!(run(&UndocumentedUnsafe, documented).is_empty());
    }

    #[test]
    fn each_nested_block_needs_its_own_comment() {
        let src = "fn f() {
            // SAFETY: outer is fine
            unsafe {
                let a = 1;
                unsafe { g() }
            }
        }";
        assert_eq!(run(&UndocumentedUnsafe, src).len(), 1);
    }
}
