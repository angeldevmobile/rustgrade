//! Helpers shared by several rules.

use syn::visit::{self, Visit};

/// Integration tests, benches and examples are not production code.
pub fn is_test_path(path: &str) -> bool {
    path.split('/')
        .any(|c| matches!(c, "tests" | "benches" | "examples"))
}

/// `#[test]`, `#[tokio::test]`, `#[cfg(test)]` or `#[cfg(all(test, ...))]`.
pub fn is_test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let path = attr.path();
        if path.segments.last().is_some_and(|s| s.ident == "test") {
            return true;
        }
        if path.is_ident("cfg") {
            return attr
                .parse_args::<syn::Meta>()
                .is_ok_and(|meta| cfg_requires_test(&meta));
        }
        false
    })
}

/// True only when the cfg can hold *only* under `test`. `any(...)` and `not(...)`
/// return false so that code which also builds outside tests is still scanned.
fn cfg_requires_test(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(p) => p.is_ident("test"),
        syn::Meta::List(list) if list.path.is_ident("all") => list
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .is_ok_and(|items| items.iter().any(cfg_requires_test)),
        _ => false,
    }
}

/// Calls `on_expr` for every expression that runs once per loop iteration,
/// skipping test code.
///
/// Counts `for` bodies (not the iterator expression), `while` conditions and
/// bodies, and `loop` bodies. These start outside any loop again:
/// - a nested `fn`;
/// - `return` / `break` values, which run at most once;
/// - arguments of [`EXEMPT_ARGS_OF`]: error-path closures, and values being
///   stored, where the allocation or clone is the point.
///
/// Closures passed to iterator adapters (`.map(|x| ..)`) are not counted yet.
pub fn for_each_expr_in_loop<'ast>(file: &'ast syn::File, on_expr: impl FnMut(&'ast syn::Expr)) {
    LoopWalker { depth: 0, on_expr }.visit_file(file);
}

const EXEMPT_ARGS_OF: &[&str] = &[
    // Closure runs only on the error / `None` path.
    "with_context",
    "map_err",
    "ok_or_else",
    "unwrap_or_else",
    "or_else",
    // Argument is the value being stored.
    "push",
    "push_back",
    "push_front",
    "insert",
    "send",
];

struct LoopWalker<F> {
    depth: usize,
    on_expr: F,
}

impl<F> LoopWalker<F> {
    fn nested_fn(&mut self, attrs: &[syn::Attribute], walk: impl FnOnce(&mut Self)) {
        if is_test_only(attrs) {
            return;
        }
        self.outside_loop(walk);
    }

    fn outside_loop(&mut self, walk: impl FnOnce(&mut Self)) {
        let outer = std::mem::replace(&mut self.depth, 0);
        walk(self);
        self.depth = outer;
    }

    fn in_loop(&mut self, walk: impl FnOnce(&mut Self)) {
        self.depth += 1;
        walk(self);
        self.depth -= 1;
    }
}

impl<'ast, F: FnMut(&'ast syn::Expr)> Visit<'ast> for LoopWalker<F> {
    fn visit_expr(&mut self, e: &'ast syn::Expr) {
        if self.depth > 0 {
            (self.on_expr)(e);
        }
        visit::visit_expr(self, e);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if !EXEMPT_ARGS_OF.iter().any(|m| call.method == m) {
            visit::visit_expr_method_call(self, call);
            return;
        }
        self.visit_expr(&call.receiver);
        self.outside_loop(|w| {
            for arg in &call.args {
                w.visit_expr(arg);
            }
        });
    }

    fn visit_expr_return(&mut self, r: &'ast syn::ExprReturn) {
        self.outside_loop(|w| visit::visit_expr_return(w, r));
    }

    fn visit_expr_break(&mut self, b: &'ast syn::ExprBreak) {
        self.outside_loop(|w| visit::visit_expr_break(w, b));
    }

    fn visit_expr_for_loop(&mut self, l: &'ast syn::ExprForLoop) {
        self.visit_pat(&l.pat);
        self.visit_expr(&l.expr);
        self.in_loop(|w| w.visit_block(&l.body));
    }

    fn visit_expr_while(&mut self, l: &'ast syn::ExprWhile) {
        self.in_loop(|w| visit::visit_expr_while(w, l));
    }

    fn visit_expr_loop(&mut self, l: &'ast syn::ExprLoop) {
        self.in_loop(|w| visit::visit_expr_loop(w, l));
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.nested_fn(&f.attrs, |w| visit::visit_item_fn(w, f));
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.nested_fn(&f.attrs, |w| visit::visit_impl_item_fn(w, f));
    }

    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if !is_test_only(&m.attrs) {
            visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if !is_test_only(&i.attrs) {
            visit::visit_item_impl(self, i);
        }
    }
}
