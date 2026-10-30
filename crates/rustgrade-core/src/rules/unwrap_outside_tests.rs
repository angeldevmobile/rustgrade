use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use super::common::{comments_above, is_test_only, is_test_path};
use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity, Span};

pub struct UnwrapOutsideTests;

impl Rule for UnwrapOutsideTests {
    fn id(&self) -> &'static str {
        "unwrap-outside-tests"
    }

    fn category(&self) -> Category {
        Category::Errors
    }

    fn default_severity(&self) -> Severity {
        Severity::High
    }

    fn summary(&self) -> &'static str {
        "`.unwrap()` / `.expect()` outside tests"
    }

    fn explain(&self) -> &'static str {
        EXPLAIN
    }

    // Known gap: syn does not parse macro bodies, so `println!("{}", x.unwrap())`
    // is not seen.
    fn check(&self, ctx: &FileContext<'_>, out: &mut Vec<Finding>) {
        if is_test_path(ctx.path) {
            return;
        }
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
    rule: &'a UnwrapOutsideTests,
    path: &'a str,
    lines: &'a [&'a str],
    /// Start line of each enclosing statement, innermost last.
    stmt_lines: Vec<usize>,
    out: &'o mut Vec<Finding>,
}

impl Visitor<'_, '_> {
    /// A comment at the end of the call's line, above it, or above its
    /// statement that says why it cannot fail.
    fn is_justified(&self, call: proc_macro2::Span) -> bool {
        let end = call.end();
        let trailing = end
            .line
            .checked_sub(1)
            .and_then(|i| self.lines.get(i))
            .and_then(|l| l.char_indices().nth(end.column).map(|(i, _)| &l[i..]))
            .and_then(|rest| rest.find("//").map(|c| &rest[c..]));
        trailing.is_some_and(explains_why)
            || [call.start().line, end.line]
                .into_iter()
                .chain(self.stmt_lines.last().copied())
                .any(|line| comments_above(self.lines, line).any(explains_why))
    }
}

/// `m.lock()`, `rw.read()`, `rw.write()` (no arguments, unlike `io::Read`) or
/// `cvar.wait*(guard, ..)`: their only error is a poisoned lock.
fn is_lock_poisoning(receiver: &syn::Expr) -> bool {
    let syn::Expr::MethodCall(inner) = receiver else {
        return false;
    };
    match inner.method.to_string().as_str() {
        "lock" | "read" | "write" => inner.args.is_empty(),
        "wait" | "wait_while" | "wait_timeout" | "wait_timeout_while" => !inner.args.is_empty(),
        _ => false,
    }
}

/// Words that show a comment argues the call cannot fail ("this unwrap is OK
/// since...", "invariant: non-empty", "checked above"), rather than describing
/// what the code does.
fn explains_why(comment: &str) -> bool {
    const WORDS: &[&str] = &[
        "unwrap",
        "expect",
        "panic",
        "panics",
        "invariant",
        "infallible",
        "cannot",
        "can't",
        "never",
        "always",
        "guaranteed",
        "guarantees",
        "ok",
        "okay",
        "safe",
        "checked",
        "validated",
        "impossible",
        "unreachable",
    ];
    comment
        .to_ascii_lowercase()
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '\''))
        .any(|w| WORDS.contains(&w))
}

impl<'ast> Visit<'ast> for Visitor<'_, '_> {
    fn visit_stmt(&mut self, s: &'ast syn::Stmt) {
        self.stmt_lines.push(s.span().start().line);
        visit::visit_stmt(self, s);
        self.stmt_lines.pop();
    }

    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if !is_test_only(&m.attrs) {
            visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if !is_test_only(&f.attrs) {
            visit::visit_item_fn(self, f);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if !is_test_only(&i.attrs) {
            visit::visit_item_impl(self, i);
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if !is_test_only(&f.attrs) {
            visit::visit_impl_item_fn(self, f);
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        // `.expect("why")` documents the invariant, so it ranks one step below `.unwrap()`.
        let severity = match call.method.to_string().as_str() {
            "unwrap" if call.args.is_empty() => Some(self.rule.default_severity()),
            "expect" if call.args.len() == 1 => Some(Severity::Medium),
            _ => None,
        };
        if let Some(severity) = severity {
            let span = call
                .method
                .span()
                .join(call.paren_token.span.close())
                .unwrap_or(call.method.span());
            let (severity, message, help) = if is_lock_poisoning(&call.receiver) {
                (
                    Severity::Low,
                    format!(
                        "`.{}()` on a lock result panics only if another thread panicked holding it",
                        call.method
                    ),
                    "propagating the poisoning panic is the std convention; recover with `PoisonError::into_inner` if the data stays valid",
                )
            } else if self.is_justified(span) {
                (
                    Severity::Low,
                    format!(
                        "`.{}()` panics on the error path (a comment says why it cannot)",
                        call.method
                    ),
                    "fine if the comment holds; `.expect(\"why\")` puts the reason in the panic message",
                )
            } else {
                (
                    severity,
                    format!("`.{}()` panics on the error path", call.method),
                    "propagate with `?`, handle the error, or document why it cannot fail",
                )
            };
            self.out.push(Finding {
                rule: self.rule.id(),
                severity,
                category: self.rule.category(),
                message,
                span: Span::from_proc_macro(self.path, call.method.span()),
                help: Some(help.into()),
            });
        }
        visit::visit_expr_method_call(self, call);
    }
}

const EXPLAIN: &str = r"Why: `.unwrap()` panics on the error path. In a request handler, daemon or
library, one malformed input kills the thread or the process. `.expect()` is
reported one level lower because its message documents an invariant, but it
still panics. Tests, benches and examples are exempt.

A comment at the end of the line, or directly above the call or its statement,
that argues why it cannot fail (`// this unwrap is OK since the max is 512`,
`// invariant: non-empty`, `// checked above`) lowers either one to `low`. A comment
that only describes what the code does does not. Unwrapping a lock result
(`m.lock().unwrap()`, `cvar.wait(g).unwrap()`) is `low` too: it only fails if
another thread panicked holding the lock, and propagating that is the std
convention.

Before:
    let (name, age) = req.split_once(':').unwrap();

After:
    let (name, age) = req.split_once(':').ok_or(RequestError::MissingSeparator)?;
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::{run, run_at};

    #[test]
    fn flags_unwrap_with_location() {
        let found = run(
            &UnwrapOutsideTests,
            "fn load() -> u32 {\n    let s = read();\n    s.parse().unwrap()\n}",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::High);
        assert_eq!((found[0].span.line, found[0].span.column), (3, 15));
    }

    #[test]
    fn expect_is_medium() {
        let found = run(
            &UnwrapOutsideTests,
            r#"fn f() { x.expect("set in new()"); }"#,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Medium);
    }

    #[test]
    fn a_comment_saying_why_lowers_it_to_low() {
        for src in [
            "fn f() {
                // This unwrap is OK since the max is 512.
                g(x.unwrap());
            }",
            "fn f() {
                let n = v.first().unwrap(); // non-empty: checked above
            }",
            r#"fn f() {
                let n = v
                    .iter()
                    .max()
                    // invariant: v is never empty
                    .expect("non-empty");
            }"#,
        ] {
            let found = run(&UnwrapOutsideTests, src);
            assert_eq!(found.len(), 1, "{src}");
            assert_eq!(found[0].severity, Severity::Low, "{src}");
            assert!(found[0].message.contains("a comment says why"), "{src}");
        }
    }

    #[test]
    fn lock_poisoning_is_low() {
        let src = "fn f() {
            let a = m.lock().unwrap();
            let b = rw.read().unwrap();
            let c = rw.write().expect(\"not poisoned\");
            let d = cvar.wait(guard).unwrap();
            let e = file.read(&mut buf).unwrap();
        }";
        let severities: Vec<_> = run(&UnwrapOutsideTests, src)
            .iter()
            .map(|f| f.severity)
            .collect();
        let low = Severity::Low;
        assert_eq!(severities, [low, low, low, low, Severity::High]);
    }

    #[test]
    fn a_comment_that_only_describes_does_not_count() {
        let src = "fn f() {
            // parse the header
            let h = parse(buf).unwrap();
        }";
        assert_eq!(run(&UnwrapOutsideTests, src)[0].severity, Severity::High);
        let blank_line = "fn f() {
            // checked above

            let h = parse(buf).unwrap();
        }";
        assert_eq!(
            run(&UnwrapOutsideTests, blank_line)[0].severity,
            Severity::High
        );
    }

    #[test]
    fn flags_inside_impl_methods_and_closures() {
        let src = "impl S { fn f(&self) { v.iter().map(|x| x.get().unwrap()); } }";
        assert_eq!(run(&UnwrapOutsideTests, src).len(), 1);
    }

    #[test]
    fn ignores_unwrap_variants() {
        let src = "fn f() { a.unwrap_or(0); b.unwrap_or_default(); c.unwrap_or_else(g); }";
        assert!(run(&UnwrapOutsideTests, src).is_empty());
    }

    #[test]
    fn ignores_test_code() {
        let src = r"
            #[cfg(test)]
            mod tests { fn helper() { x.unwrap(); } }

            #[test]
            fn plain() { x.unwrap(); }

            #[tokio::test]
            async fn async_test() { x.unwrap(); }

            #[cfg(all(test, feature = 'slow'))]
            fn gated() { x.unwrap(); }
        "
        .replace('\'', "\"");
        assert!(run(&UnwrapOutsideTests, &src).is_empty());
    }

    #[test]
    fn still_flags_non_test_cfgs() {
        let src = r"
            #[cfg(not(test))]
            fn prod() { x.unwrap(); }

            #[cfg(any(test, debug_assertions))]
            fn debug() { x.unwrap(); }
        ";
        assert_eq!(run(&UnwrapOutsideTests, src).len(), 2);
    }

    #[test]
    fn ignores_integration_tests_benches_and_examples() {
        let src = "fn f() { x.unwrap(); }";
        assert!(run_at(&UnwrapOutsideTests, "tests/api.rs", src).is_empty());
        assert!(run_at(&UnwrapOutsideTests, "crates/a/benches/b.rs", src).is_empty());
        assert!(run_at(&UnwrapOutsideTests, "examples/demo.rs", src).is_empty());
        assert_eq!(run_at(&UnwrapOutsideTests, "src/tester.rs", src).len(), 1);
    }
}
