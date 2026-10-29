use std::collections::HashMap;

use syn::visit::{self, Visit};

use super::common::{is_test_only, is_test_path};
use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity, Span};

pub struct UnboundedChannel;

/// Crates (or modules) whose zero-argument `unbounded()` makes an unbounded channel.
const UNBOUNDED_CRATES: &[&str] = &[
    "crossbeam_channel",
    "crossbeam",
    "flume",
    "async_channel",
    "futures",
];

impl Rule for UnboundedChannel {
    fn id(&self) -> &'static str {
        "unbounded-channel"
    }

    fn category(&self) -> Category {
        Category::Concurrency
    }

    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn summary(&self) -> &'static str {
        "Unbounded channel (`unbounded_channel()`, `unbounded()`, `std::sync::mpsc::channel()`) without backpressure"
    }

    fn explain(&self) -> &'static str {
        EXPLAIN
    }

    fn check(&self, ctx: &FileContext<'_>, out: &mut Vec<Finding>) {
        if is_test_path(ctx.path) {
            return;
        }
        let mut imports = Imports::default();
        imports.visit_file(ctx.ast);
        Visitor {
            rule: self,
            path: ctx.path,
            imports: &imports.0,
            out,
        }
        .visit_file(ctx.ast);
    }
}

/// Constructors that make an unbounded channel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// tokio's `mpsc::unbounded_channel()`.
    UnboundedChannel,
    /// crossbeam / flume / async-channel / futures `unbounded()`.
    Unbounded,
    /// `std::sync::mpsc::channel()`.
    StdChannel,
}

/// Classifies a full `use` path (`["std", "sync", "mpsc", "channel"]`).
fn kind_of_path(segments: &[String]) -> Option<Kind> {
    let (last, prefix) = segments.split_last()?;
    match last.as_str() {
        "unbounded_channel" => Some(Kind::UnboundedChannel),
        "unbounded"
            if prefix
                .iter()
                .any(|s| UNBOUNDED_CRATES.contains(&s.as_str())) =>
        {
            Some(Kind::Unbounded)
        }
        "channel"
            if prefix.first().is_some_and(|s| s == "std")
                && prefix.last().is_some_and(|s| s == "mpsc") =>
        {
            Some(Kind::StdChannel)
        }
        _ => None,
    }
}

/// Local names imported for unbounded constructors, e.g. `use crossbeam_channel::unbounded;`.
#[derive(Default)]
struct Imports(HashMap<String, Kind>);

impl Imports {
    fn collect(&mut self, tree: &syn::UseTree, prefix: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(p) => {
                prefix.push(p.ident.to_string());
                self.collect(&p.tree, prefix);
                prefix.pop();
            }
            syn::UseTree::Name(n) => self.record(prefix, &n.ident, &n.ident),
            syn::UseTree::Rename(r) => self.record(prefix, &r.ident, &r.rename),
            syn::UseTree::Group(g) => {
                for t in &g.items {
                    self.collect(t, prefix);
                }
            }
            syn::UseTree::Glob(_) => {}
        }
    }

    fn record(&mut self, prefix: &mut Vec<String>, original: &syn::Ident, local: &syn::Ident) {
        prefix.push(original.to_string());
        if let Some(kind) = kind_of_path(prefix) {
            self.0.insert(local.to_string(), kind);
        }
        prefix.pop();
    }
}

impl<'ast> Visit<'ast> for Imports {
    fn visit_item_use(&mut self, u: &'ast syn::ItemUse) {
        self.collect(&u.tree, &mut Vec::new());
    }
}

struct Visitor<'a, 'o> {
    rule: &'a UnboundedChannel,
    path: &'a str,
    imports: &'a HashMap<String, Kind>,
    out: &'o mut Vec<Finding>,
}

impl Visitor<'_, '_> {
    /// Returns the kind of channel and the constructor's name token.
    fn classify<'c>(&self, call: &'c syn::ExprCall) -> Option<(Kind, &'c syn::Ident)> {
        // Bounded constructors always take a capacity; any argument means "not this".
        if !call.args.is_empty() {
            return None;
        }
        let syn::Expr::Path(p) = &*call.func else {
            return None;
        };
        let mut segs = p.path.segments.iter().rev().map(|s| &s.ident);
        let last = segs.next()?;
        let kind = match segs.next() {
            None => self.imports.get(&last.to_string()).copied(),
            Some(_) if last == "unbounded_channel" => Some(Kind::UnboundedChannel),
            Some(parent)
                if last == "unbounded"
                    && (UNBOUNDED_CRATES.iter().any(|c| parent == c)
                        || parent == "mpsc"
                        || parent == "channel") =>
            {
                Some(Kind::Unbounded)
            }
            // tokio's and futures' `mpsc::channel(cap)` take a capacity; std's takes none.
            Some(parent) if last == "channel" && parent == "mpsc" => Some(Kind::StdChannel),
            Some(_) => None,
        }?;
        Some((kind, last))
    }
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

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Some((kind, ident)) = self.classify(call) {
            let (what, bounded) = match kind {
                Kind::UnboundedChannel => ("`unbounded_channel()`", "`mpsc::channel(cap)`"),
                Kind::Unbounded => ("`unbounded()`", "`bounded(cap)`"),
                Kind::StdChannel => ("`std::sync::mpsc::channel()`", "`mpsc::sync_channel(cap)`"),
            };
            self.out.push(Finding {
                rule: self.rule.id(),
                severity: self.rule.default_severity(),
                category: self.rule.category(),
                message: format!("{what} creates a channel with no capacity limit"),
                span: Span::from_proc_macro(self.path, ident.span()),
                help: Some(format!(
                    "use {bounded} so a slow consumer applies backpressure instead of growing memory without limit; if unbounded is intended, suppress with a reason"
                )),
            });
        }
        visit::visit_expr_call(self, call);
    }
}

const EXPLAIN: &str = r"Why: an unbounded channel turns a slow consumer into unbounded memory
growth instead of backpressure. The producer never slows down and the process
is OOM-killed later, far from the cause. `std::sync::mpsc::channel()` is
unbounded too. If the message count is bounded by design (one shutdown signal
per worker), suppress the finding with that reason.

Before:
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

After:
    let (tx, rx) = tokio::sync::mpsc::channel(1024);
    // a full channel makes the producer wait: tx.send(msg).await
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::run;

    fn messages(src: &str) -> Vec<String> {
        run(&UnboundedChannel, src)
            .into_iter()
            .map(|f| f.message)
            .collect()
    }

    #[test]
    fn flags_qualified_constructors_with_location() {
        let src = "fn f() {
            let (a, b) = tokio::sync::mpsc::unbounded_channel::<u8>();
            let (c, d) = crossbeam_channel::unbounded::<u8>();
            let (e, g) = flume::unbounded::<u8>();
            let (h, i) = std::sync::mpsc::channel::<u8>();
            let (j, k) = mpsc::channel::<u8>();
        }";
        assert_eq!(messages(src).len(), 5);
        let found = run(
            &UnboundedChannel,
            "fn f() {\n    let x = mpsc::unbounded_channel();\n}",
        );
        assert_eq!((found[0].span.line, found[0].span.column), (2, 19));
    }

    #[test]
    fn flags_imported_names_and_renames() {
        let src = "
            use crossbeam_channel::{unbounded, bounded};
            use std::sync::mpsc::{channel as make, sync_channel};
            use tokio::sync::mpsc::unbounded_channel;
            fn f() {
                let a = unbounded::<u8>();
                let b = make::<u8>();
                let c = unbounded_channel::<u8>();
                let d = bounded::<u8>(16);
                let e = sync_channel::<u8>(16);
            }
        ";
        assert_eq!(
            messages(src),
            [
                "`unbounded()` creates a channel with no capacity limit",
                "`std::sync::mpsc::channel()` creates a channel with no capacity limit",
                "`unbounded_channel()` creates a channel with no capacity limit",
            ]
        );
    }

    #[test]
    fn ignores_bounded_channels_and_unrelated_functions() {
        let src = "
            use my_crate::unbounded;
            fn f() {
                let a = tokio::sync::mpsc::channel::<u8>(64);
                let b = futures::channel::mpsc::channel::<u8>(64);
                let c = crossbeam_channel::bounded::<u8>(64);
                let d = unbounded();
                let e = channel();
                let f = range.unbounded();
            }
        ";
        assert!(messages(src).is_empty(), "{:?}", messages(src));
    }

    #[test]
    fn ignores_test_code() {
        let src = "#[cfg(test)] mod t { fn f() { let x = crossbeam_channel::unbounded::<u8>(); } }";
        assert_eq!(messages(src), Vec::<String>::new());
    }
}
