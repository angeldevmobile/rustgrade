use syn::visit::{self, Visit};

use super::common::{is_test_only, is_test_path};
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

    // Known gap: syn does not parse macro bodies, so `println!("{}", x.unwrap())`
    // is not seen.
    fn check(&self, ctx: &FileContext<'_>, out: &mut Vec<Finding>) {
        if is_test_path(ctx.path) {
            return;
        }
        let mut v = Visitor {
            rule: self,
            path: ctx.path,
            out,
        };
        v.visit_file(ctx.ast);
    }
}

struct Visitor<'a, 'o> {
    rule: &'a UnwrapOutsideTests,
    path: &'a str,
    out: &'o mut Vec<Finding>,
}

impl<'ast> Visit<'ast> for Visitor<'_, '_> {
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
            self.out.push(Finding {
                rule: self.rule.id(),
                severity,
                category: self.rule.category(),
                message: format!("`.{}()` panics on the error path", call.method),
                span: Span::from_proc_macro(self.path, call.method.span()),
                help: Some(
                    "propagate with `?`, handle the error, or document why it cannot fail".into(),
                ),
            });
        }
        visit::visit_expr_method_call(self, call);
    }
}

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
