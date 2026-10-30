use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use super::common::{comments_above, is_test_only, is_test_path};
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

    // Medium, not critical: a missing comment is a review gap, not proof of
    // unsoundness (clippy keeps `undocumented_unsafe_blocks` opt-in).
    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn summary(&self) -> &'static str {
        "`unsafe` block or impl without a `// SAFETY:` comment"
    }

    fn explain(&self) -> &'static str {
        EXPLAIN
    }

    // Applies to test code too (unsound tests are still unsound), but as
    // `low`: it is not shipped, so it should not cap a crate's grade.
    fn check(&self, ctx: &FileContext<'_>, out: &mut Vec<Finding>) {
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut v = Visitor {
            rule: self,
            path: ctx.path,
            lines: &lines,
            stmt_lines: Vec::new(),
            test_depth: usize::from(is_test_path(ctx.path)),
            fn_contracts: Vec::new(),
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
    /// Number of enclosing test-only items (or 1 for a test file).
    test_depth: usize,
    /// Per enclosing fn, innermost last: its blocks are covered by the fn's
    /// documented contract (see [`contract_covers_body`]).
    fn_contracts: Vec<bool>,
    out: &'o mut Vec<Finding>,
}

impl Visitor<'_, '_> {
    fn check_token(&mut self, token: proc_macro2::Span, what: &str) {
        let start = token.start();
        let covered_by_fn = what == "block" && self.fn_contracts.last() == Some(&true);
        let documented = covered_by_fn
            || has_safety_comment(self.lines, start.line, start.column)
            || self
                .stmt_lines
                .last()
                .is_some_and(|&l| l != start.line && has_safety_comment(self.lines, l, 0));
        if !documented {
            let severity = if self.test_depth > 0 {
                Severity::Low
            } else {
                self.rule.default_severity()
            };
            self.out.push(Finding {
                rule: self.rule.id(),
                severity,
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

impl Visitor<'_, '_> {
    fn in_test_if(&mut self, attrs: &[syn::Attribute], visit: impl FnOnce(&mut Self)) {
        let test = is_test_only(attrs);
        self.test_depth += usize::from(test);
        visit(self);
        self.test_depth -= usize::from(test);
    }

    fn in_fn(
        &mut self,
        attrs: &[syn::Attribute],
        sig: &syn::Signature,
        body: &syn::Block,
        visit: impl FnOnce(&mut Self),
    ) {
        self.fn_contracts
            .push(contract_covers_body(attrs, sig, body));
        self.in_test_if(attrs, visit);
        self.fn_contracts.pop();
    }
}

/// An `unsafe fn` must wrap unsafe operations in `unsafe {}` (edition 2024's
/// `unsafe_op_in_unsafe_fn`), but its callers uphold the contract. Its blocks
/// are covered when the contract is written down (a `# Safety` doc section),
/// or when the whole body is one `unsafe {}` (the wrapper is mechanical).
fn contract_covers_body(attrs: &[syn::Attribute], sig: &syn::Signature, body: &syn::Block) -> bool {
    if sig.unsafety.is_none() {
        return false;
    }
    let only_unsafe_block = matches!(
        body.stmts.as_slice(),
        [syn::Stmt::Expr(syn::Expr::Unsafe(_), None)]
    );
    only_unsafe_block || attrs.iter().any(is_safety_doc_section)
}

/// `/// # Safety`, in any case.
fn is_safety_doc_section(attr: &syn::Attribute) -> bool {
    let syn::Meta::NameValue(nv) = &attr.meta else {
        return false;
    };
    let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(doc),
        ..
    }) = &nv.value
    else {
        return false;
    };
    nv.path.is_ident("doc") && doc.value().trim().eq_ignore_ascii_case("# safety")
}

impl<'ast> Visit<'ast> for Visitor<'_, '_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        self.in_test_if(&m.attrs, |v| visit::visit_item_mod(v, m));
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.in_fn(&f.attrs, &f.sig, &f.block, |v| visit::visit_item_fn(v, f));
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.in_fn(&f.attrs, &f.sig, &f.block, |v| {
            visit::visit_impl_item_fn(v, f);
        });
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        match &f.default {
            Some(body) => self.in_fn(&f.attrs, &f.sig, body, |v| {
                visit::visit_trait_item_fn(v, f);
            }),
            None => visit::visit_trait_item_fn(self, f),
        }
    }

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
        self.in_test_if(&i.attrs, |v| {
            if let Some(token) = i.unsafety {
                v.check_token(token.span, "impl");
            }
            visit::visit_item_impl(v, i);
        });
    }
}

/// `line` is 1-based, `column` 0-based in chars (as `proc_macro2` reports them).
///
/// Accepts `SAFETY:` (any case, like clippy) earlier on the same line, or in
/// the run of comment and attribute lines directly above. A blank line breaks
/// the run.
fn has_safety_comment(lines: &[&str], line: usize, column: usize) -> bool {
    let Some(current) = line.checked_sub(1).and_then(|i| lines.get(i)) else {
        return false;
    };
    let prefix_end = current
        .char_indices()
        .nth(column)
        .map_or(current.len(), |(i, _)| i);
    if mentions_safety(&current[..prefix_end]) {
        return true;
    }
    comments_above(lines, line).any(mentions_safety)
}

fn mentions_safety(text: &str) -> bool {
    text.to_ascii_uppercase().contains("SAFETY:")
}

const EXPLAIN: &str = r"Why: `unsafe` makes the author responsible for invariants the compiler can
no longer check. A `// SAFETY:` comment states which invariant holds and why,
so a reviewer can decide whether the block is sound. Without it, nobody can.
Any case is accepted (`// Safety:`). Blocks inside an `unsafe fn` are covered
by its `/// # Safety` section, or when the block is the whole body. In test code
the finding is `low`.

Before:
    fn first(v: &[u8]) -> u8 {
        unsafe { *v.get_unchecked(0) }
    }

After:
    fn first(v: &[u8]) -> Option<u8> {
        if v.is_empty() {
            return None;
        }
        // SAFETY: `v` is non-empty (checked above), so index 0 is in bounds.
        Some(unsafe { *v.get_unchecked(0) })
    }
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::{run, run_at};

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
    fn safety_is_case_insensitive() {
        for comment in [
            "// Safety: valid",
            "// safety: valid",
            "/* SAFETY: valid */",
        ] {
            let src = format!(
                "fn f(p: *const u8) -> u8 {{
{comment}
unsafe {{ *p }}
}}"
            );
            assert!(run(&UndocumentedUnsafe, &src).is_empty(), "{comment}");
        }
    }

    #[test]
    fn test_code_is_reported_as_low() {
        let src = "#[cfg(test)]
            mod tests {
                #[test]
                fn t() { unsafe { g() } }
            }
            fn prod() { unsafe { g() } }";
        let severities: Vec<_> = run(&UndocumentedUnsafe, src)
            .iter()
            .map(|f| f.severity)
            .collect();
        assert_eq!(severities, [Severity::Low, Severity::Medium]);
        let in_test_file = run_at(
            &UndocumentedUnsafe,
            "src/sync/tests.rs",
            "fn t() { unsafe { g() } }",
        );
        assert_eq!(in_test_file[0].severity, Severity::Low);
    }

    #[test]
    fn unsafe_fn_contract_covers_its_blocks() {
        let documented = "/// Reads `p`.
            ///
            /// # Safety
            ///
            /// `p` must be valid for reads.
            unsafe fn read(p: *const u8) -> u8 {
                let x = unsafe { *p };
                x + unsafe { *p.add(1) }
            }";
        assert!(run(&UndocumentedUnsafe, documented).is_empty());

        let wrapper = "/// UB if `p` is dangling.
            unsafe fn read(p: *const u8) -> u8 { unsafe { *p } }";
        assert!(run(&UndocumentedUnsafe, wrapper).is_empty());

        let undocumented = "unsafe fn read(p: *const u8) -> u8 {
                let x = unsafe { *p };
                x
            }
            fn safe(p: *const u8) -> u8 { unsafe { *p } }";
        assert_eq!(run(&UndocumentedUnsafe, undocumented).len(), 2);
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
