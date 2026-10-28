use super::common::{for_each_expr_in_loop, is_test_path};
use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity, Span};

pub struct AllocInLoop;

/// Collections whose `new` / `with_capacity` inside a loop usually means a
/// buffer that should be hoisted and reused with `.clear()`.
const COLLECTIONS: &[&str] = &[
    "Vec", "String", "HashMap", "HashSet", "BTreeMap", "BTreeSet", "VecDeque",
];
const METHODS: &[&str] = &["to_string", "to_vec"];
const MACROS: &[&str] = &["format", "vec"];

impl Rule for AllocInLoop {
    fn id(&self) -> &'static str {
        "alloc-in-loop"
    }

    fn category(&self) -> Category {
        Category::Performance
    }

    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn summary(&self) -> &'static str {
        "Allocation (`Vec::new`, `String::from`, `format!`, `.to_string()`, ...) inside a loop body"
    }

    fn check(&self, ctx: &FileContext<'_>, out: &mut Vec<Finding>) {
        if is_test_path(ctx.path) {
            return;
        }
        let severity = if ctx.is_hot {
            Severity::High
        } else {
            self.default_severity()
        };
        for_each_expr_in_loop(ctx.ast, |expr| {
            let Some((what, span)) = allocation(expr) else {
                return;
            };
            out.push(Finding {
                rule: self.id(),
                severity,
                category: self.category(),
                message: format!("`{what}` allocates on every iteration"),
                span: Span::from_proc_macro(ctx.path, span),
                help: Some(
                    "hoist the buffer out of the loop and `.clear()` it, or write into an existing one (`write!`, `extend`)"
                        .into(),
                ),
            });
        });
    }
}

fn allocation(expr: &syn::Expr) -> Option<(String, proc_macro2::Span)> {
    match expr {
        syn::Expr::Call(call) => {
            let syn::Expr::Path(p) = &*call.func else {
                return None;
            };
            let mut segs = p.path.segments.iter().rev();
            let (func, ty) = (segs.next()?, segs.next()?);
            let ty_name = ty.ident.to_string();
            let func_name = func.ident.to_string();
            let allocates = match func_name.as_str() {
                "new" | "with_capacity" => {
                    COLLECTIONS.contains(&ty_name.as_str()) || ty_name == "Box"
                }
                "from" => ty_name == "String",
                _ => false,
            };
            allocates.then(|| (format!("{ty_name}::{func_name}"), func.ident.span()))
        }
        syn::Expr::MethodCall(call) if call.args.is_empty() => {
            let name = call.method.to_string();
            METHODS
                .contains(&name.as_str())
                .then(|| (format!(".{name}()"), call.method.span()))
        }
        syn::Expr::Macro(m) => {
            let ident = &m.mac.path.segments.last()?.ident;
            let name = ident.to_string();
            MACROS
                .contains(&name.as_str())
                .then(|| (format!("{name}!"), ident.span()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::{run, run_hot};

    fn messages(src: &str) -> Vec<String> {
        run(&AllocInLoop, src)
            .into_iter()
            .map(|f| f.message)
            .collect()
    }

    #[test]
    fn flags_each_kind() {
        let src = r#"fn f(items: &[u32]) {
            for i in items {
                let mut buf = Vec::new();
                let s = String::from("x");
                let t = format!("{i}");
                let u = i.to_string();
                let w = std::collections::HashMap::<u32, u32>::with_capacity(4);
                let b = Box::new(*i);
                let v = vec![0u8; 16];
            }
        }"#;
        assert_eq!(
            messages(src),
            [
                "`Vec::new` allocates on every iteration",
                "`String::from` allocates on every iteration",
                "`format!` allocates on every iteration",
                "`.to_string()` allocates on every iteration",
                "`HashMap::with_capacity` allocates on every iteration",
                "`Box::new` allocates on every iteration",
                "`vec!` allocates on every iteration",
            ]
        );
    }

    #[test]
    fn ignores_non_allocating_calls_and_code_outside_loops() {
        let src = r#"fn f() {
            let buf = Vec::new();
            let s = format!("once");
            for _ in 0..3 {
                buf.clear();
                let o = Option::from(1);
                let p = Point::new(1, 2);
                println!("{}", 1);
            }
        }"#;
        assert!(messages(src).is_empty());
    }

    #[test]
    fn ignores_lazy_error_closures_but_not_their_receiver() {
        let src = r#"fn f() {
            for p in paths {
                read(p).with_context(|| format!("reading {p}"))?;
                x.ok_or_else(|| String::from("missing"))?;
                p.to_string().map_err(|e| e.to_string())?;
            }
        }"#;
        assert_eq!(
            messages(src),
            ["`.to_string()` allocates on every iteration"]
        );
    }

    #[test]
    fn ignores_values_being_stored() {
        let src = r#"fn f() {
            for i in 0..3 {
                out.push(format!("{i}"));
                map.insert(i.to_string(), Finding { msg: format!("{i}") });
                tx.send(vec![i]);
                let tmp = format!("{i}");
            }
        }"#;
        assert_eq!(messages(src), ["`format!` allocates on every iteration"]);
    }

    #[test]
    fn raises_severity_when_hot() {
        let found = run_hot(
            &AllocInLoop,
            "fn f() { loop { let v = Vec::<u8>::new(); } }",
        );
        assert_eq!(found[0].severity, Severity::High);
    }
}
