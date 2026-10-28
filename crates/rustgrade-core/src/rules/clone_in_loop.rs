use super::common::{for_each_expr_in_loop, is_test_path};
use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity, Span};

pub struct CloneInLoop;

impl Rule for CloneInLoop {
    fn id(&self) -> &'static str {
        "clone-in-loop"
    }

    fn category(&self) -> Category {
        Category::Performance
    }

    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn summary(&self) -> &'static str {
        "`.clone()` / `.to_owned()` inside a loop body"
    }

    // Syntax only: cannot tell a cheap `Arc` or `Copy` clone from a deep one.
    // `Arc::clone(&x)` is not flagged, which is the idiom for "this is cheap".
    fn check(&self, ctx: &FileContext<'_>, out: &mut Vec<Finding>) {
        if is_test_path(ctx.path) {
            return;
        }
        let severity = if ctx.is_hot {
            Severity::High
        } else {
            self.default_severity()
        };
        for_each_expr_in_loop(ctx.ast, &[], |expr| {
            let syn::Expr::MethodCall(call) = expr else {
                return;
            };
            if !call.args.is_empty() || !(call.method == "clone" || call.method == "to_owned") {
                return;
            }
            out.push(Finding {
                rule: self.id(),
                severity,
                category: self.category(),
                message: format!("`.{}()` runs on every iteration", call.method),
                span: Span::from_proc_macro(ctx.path, call.method.span()),
                help: Some(
                    "borrow instead, hoist the clone out of the loop, or use `Arc::clone(&x)` if it is a cheap refcount bump"
                        .into(),
                ),
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::{run, run_hot};

    #[test]
    fn flags_clone_in_for_body() {
        let src = "fn f(v: &[String]) {\n    for s in v {\n        take(s.clone());\n    }\n}";
        let found = run(&CloneInLoop, src);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].span.line, 3);
        assert_eq!(found[0].severity, Severity::Medium);
    }

    #[test]
    fn flags_while_condition_loop_body_and_to_owned() {
        let src = "fn f() {
            while q.clone().is_empty() {}
            loop { let s = name.to_owned(); }
        }";
        assert_eq!(run(&CloneInLoop, src).len(), 2);
    }

    #[test]
    fn for_iterator_expression_runs_once() {
        assert!(run(&CloneInLoop, "fn f() { for x in v.clone() { use_it(x); } }").is_empty());
    }

    #[test]
    fn ignores_code_outside_loops_and_arc_clone_idiom() {
        let src = "fn f() {
            let a = x.clone();
            for _ in 0..3 { let b = Arc::clone(&shared); }
        }";
        assert!(run(&CloneInLoop, src).is_empty());
    }

    #[test]
    fn nested_fn_resets_loop_context() {
        let src = "fn f() { for _ in 0..3 { fn g() { x.clone(); } } }";
        assert!(run(&CloneInLoop, src).is_empty());
    }

    #[test]
    fn ignores_tests_and_raises_severity_when_hot() {
        let test = "#[test] fn t() { for _ in 0..3 { x.clone(); } }";
        assert!(run(&CloneInLoop, test).is_empty());

        let hot = run_hot(&CloneInLoop, "fn f() { loop { x.clone(); } }");
        assert_eq!(hot[0].severity, Severity::High);
    }
}
