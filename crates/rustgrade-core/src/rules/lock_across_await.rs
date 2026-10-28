use syn::visit::{self, Visit};

use super::common::{is_test_only, is_test_path};
use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity, Span};

pub struct LockAcrossAwait;

impl Rule for LockAcrossAwait {
    fn id(&self) -> &'static str {
        "lock-across-await"
    }

    fn category(&self) -> Category {
        Category::Concurrency
    }

    fn default_severity(&self) -> Severity {
        Severity::High
    }

    fn summary(&self) -> &'static str {
        "Synchronous lock guard held across `.await`"
    }

    fn explain(&self) -> &'static str {
        EXPLAIN
    }

    // Syntax only. A lock is "synchronous" when its `.lock()` / `.read()` /
    // `.write()` is not directly awaited: `m.lock().await` is tokio's async
    // mutex, built to be held across awaits; `m.lock().unwrap()` is std or
    // parking_lot, which blocks the executor thread and can deadlock.
    // Known gap: a guard moved into a function (other than `drop`) still counts.
    fn check(&self, ctx: &FileContext<'_>, out: &mut Vec<Finding>) {
        if is_test_path(ctx.path) {
            return;
        }
        Visitor {
            rule: self,
            path: ctx.path,
            in_async: false,
            out,
        }
        .visit_file(ctx.ast);
    }
}

struct Visitor<'a, 'o> {
    rule: &'a LockAcrossAwait,
    path: &'a str,
    in_async: bool,
    out: &'o mut Vec<Finding>,
}

impl Visitor<'_, '_> {
    fn with_async(&mut self, is_async: bool, walk: impl FnOnce(&mut Self)) {
        let outer = std::mem::replace(&mut self.in_async, is_async);
        walk(self);
        self.in_async = outer;
    }

    /// `guard` is the binding name, or `None` for a scrutinee temporary.
    fn report(
        &mut self,
        lock: &syn::Ident,
        guard: Option<&syn::Ident>,
        await_span: proc_macro2::Span,
    ) {
        let line = await_span.start().line;
        let message = match guard {
            Some(name) => format!(
                "guard `{name}` from `.{lock}()` is held across the `.await` on line {line}"
            ),
            None => format!(
                "temporary guard in scrutinee from `.{lock}()` is held across the `.await` on line {line}"
            ),
        };
        self.out.push(Finding {
            rule: self.rule.id(),
            severity: self.rule.default_severity(),
            category: self.rule.category(),
            message,
            span: Span::from_proc_macro(self.path, lock.span()),
            help: Some(
                "release it before awaiting (scope it in a block or `drop(guard)`), or use an async lock (`tokio::sync::Mutex`, `.lock().await`) if it must be held"
                    .into(),
            ),
        });
    }

    /// Pattern 1: `let g = m.lock().unwrap();` ... `.await` in the same block.
    fn check_block(&mut self, block: &syn::Block) {
        let mut live: Vec<(&syn::Ident, &syn::Ident)> = Vec::new();
        for stmt in &block.stmts {
            if !live.is_empty()
                && let Some(await_span) = first_await_in(|f| f.visit_stmt(stmt))
            {
                for (name, lock) in std::mem::take(&mut live) {
                    self.report(lock, Some(name), await_span);
                }
            }
            if let Some(dropped) = dropped_name(stmt) {
                live.retain(|(name, _)| *name != dropped);
            }
            if let syn::Stmt::Local(local) = stmt
                && let Some(name) = binding_name(&local.pat)
                && let Some(init) = &local.init
                && init.diverge.is_none()
                && let Some(lock) = sync_guard(&init.expr)
            {
                live.push((name, lock));
            }
        }
    }

    /// Pattern 2: a lock temporary in a `match` / `if let` / `while let`
    /// scrutinee lives until the end of the whole expression.
    /// `find_await` walks whatever runs while the scrutinee is alive.
    fn check_scrutinee(
        &mut self,
        scrutinee: &syn::Expr,
        find_await: impl FnOnce(&mut AwaitFinder),
    ) {
        let Some(lock) = sync_lock_in(scrutinee) else {
            return;
        };
        if let Some(await_span) = first_await_in(find_await) {
            self.report(lock, None, await_span);
        }
    }
}

impl<'ast> Visit<'ast> for Visitor<'_, '_> {
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

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if !is_test_only(&f.attrs) {
            self.with_async(f.sig.asyncness.is_some(), |v| visit::visit_item_fn(v, f));
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if !is_test_only(&f.attrs) {
            self.with_async(f.sig.asyncness.is_some(), |v| {
                visit::visit_impl_item_fn(v, f);
            });
        }
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        self.with_async(f.sig.asyncness.is_some(), |v| {
            visit::visit_trait_item_fn(v, f);
        });
    }

    fn visit_expr_async(&mut self, e: &'ast syn::ExprAsync) {
        self.with_async(true, |v| visit::visit_expr_async(v, e));
    }

    fn visit_expr_closure(&mut self, c: &'ast syn::ExprClosure) {
        self.with_async(c.asyncness.is_some(), |v| visit::visit_expr_closure(v, c));
    }

    fn visit_block(&mut self, b: &'ast syn::Block) {
        if self.in_async {
            self.check_block(b);
        }
        visit::visit_block(self, b);
    }

    fn visit_expr_match(&mut self, m: &'ast syn::ExprMatch) {
        if self.in_async {
            self.check_scrutinee(&m.expr, |f| {
                for arm in &m.arms {
                    f.visit_arm(arm);
                }
            });
        }
        visit::visit_expr_match(self, m);
    }

    fn visit_expr_if(&mut self, i: &'ast syn::ExprIf) {
        if self.in_async
            && let syn::Expr::Let(cond) = &*i.cond
        {
            self.check_scrutinee(&cond.expr, |f| f.visit_block(&i.then_branch));
        }
        visit::visit_expr_if(self, i);
    }

    fn visit_expr_while(&mut self, w: &'ast syn::ExprWhile) {
        if self.in_async
            && let syn::Expr::Let(cond) = &*w.cond
        {
            self.check_scrutinee(&cond.expr, |f| f.visit_block(&w.body));
        }
        visit::visit_expr_while(self, w);
    }
}

fn is_lock_call(call: &syn::ExprMethodCall) -> bool {
    call.args.is_empty()
        && (call.method == "lock" || call.method == "read" || call.method == "write")
}

/// `m.lock()`, optionally followed by `.unwrap()`, `.expect(..)`,
/// `.unwrap_or_else(..)` or `?`: the expression's value *is* the guard.
/// Returns the `lock` / `read` / `write` token.
fn sync_guard(mut expr: &syn::Expr) -> Option<&syn::Ident> {
    loop {
        match expr {
            syn::Expr::Try(t) => expr = &t.expr,
            syn::Expr::MethodCall(m)
                if (m.method == "unwrap" && m.args.is_empty())
                    || ((m.method == "expect" || m.method == "unwrap_or_else")
                        && m.args.len() == 1) =>
            {
                expr = &m.receiver;
            }
            syn::Expr::MethodCall(m) if is_lock_call(m) => return Some(&m.method),
            _ => return None,
        }
    }
}

/// First synchronous lock call anywhere in `expr` (not directly awaited).
fn sync_lock_in(expr: &syn::Expr) -> Option<&syn::Ident> {
    struct Finder<'a> {
        found: Option<&'a syn::Ident>,
    }
    impl<'a> Visit<'a> for Finder<'a> {
        fn visit_expr_await(&mut self, a: &'a syn::ExprAwait) {
            match &*a.base {
                syn::Expr::MethodCall(m) if is_lock_call(m) => self.visit_expr(&m.receiver),
                _ => visit::visit_expr_await(self, a),
            }
        }
        fn visit_expr_method_call(&mut self, m: &'a syn::ExprMethodCall) {
            if self.found.is_none() && is_lock_call(m) {
                self.found = Some(&m.method);
            }
            visit::visit_expr_method_call(self, m);
        }
        fn visit_expr_closure(&mut self, _: &'a syn::ExprClosure) {}
        fn visit_expr_async(&mut self, _: &'a syn::ExprAsync) {}
    }
    let mut f = Finder { found: None };
    f.visit_expr(expr);
    f.found
}

/// Span of the first `.await` that runs as part of the walked node. Awaits in
/// nested async blocks, closures and items belong to another future.
fn first_await_in(walk: impl FnOnce(&mut AwaitFinder)) -> Option<proc_macro2::Span> {
    let mut f = AwaitFinder { found: None };
    walk(&mut f);
    f.found
}

struct AwaitFinder {
    found: Option<proc_macro2::Span>,
}

impl<'a> Visit<'a> for AwaitFinder {
    fn visit_expr_await(&mut self, a: &'a syn::ExprAwait) {
        visit::visit_expr_await(self, a);
        if self.found.is_none() {
            self.found = Some(a.await_token.span);
        }
    }
    fn visit_expr_async(&mut self, _: &'a syn::ExprAsync) {}
    fn visit_expr_closure(&mut self, _: &'a syn::ExprClosure) {}
    fn visit_item(&mut self, _: &'a syn::Item) {}
}

/// `g` for `let g`, `let mut g` and `let g: T`; `None` for `_` and destructuring.
fn binding_name(pat: &syn::Pat) -> Option<&syn::Ident> {
    match pat {
        syn::Pat::Ident(p) => Some(&p.ident),
        syn::Pat::Type(t) => binding_name(&t.pat),
        _ => None,
    }
}

/// `g` for a `drop(g);` / `std::mem::drop(g);` statement.
fn dropped_name(stmt: &syn::Stmt) -> Option<&syn::Ident> {
    let syn::Stmt::Expr(syn::Expr::Call(call), _) = stmt else {
        return None;
    };
    let syn::Expr::Path(f) = &*call.func else {
        return None;
    };
    if f.path.segments.last().is_none_or(|s| s.ident != "drop") {
        return None;
    }
    match call.args.first() {
        Some(syn::Expr::Path(arg)) => arg.path.get_ident(),
        _ => None,
    }
}

const EXPLAIN: &str = r"Why: a std or parking_lot guard held across `.await` blocks the executor
thread while the task is suspended. Another task on that thread that wants the
lock deadlocks the runtime. A lock temporary in a `match` / `if let` /
`while let` scrutinee lives through the whole body, awaits included.
`m.lock().await` (tokio's async mutex) is never flagged.

Before:
    let mut cache = state.cache.lock().unwrap();
    let fresh = fetch(key).await?;
    cache.insert(key.to_owned(), fresh);

After:
    let fresh = fetch(key).await?; // no guard held while awaiting
    state.cache.lock().unwrap_or_else(PoisonError::into_inner).insert(key.to_owned(), fresh);
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::run;

    fn messages(src: &str) -> Vec<String> {
        run(&LockAcrossAwait, src)
            .into_iter()
            .map(|f| f.message)
            .collect()
    }

    #[test]
    fn flags_std_guard_held_across_await_with_location() {
        let src = "async fn f(m: &Mutex<Vec<u8>>) {
    let mut g = m.lock().unwrap();
    g.push(1);
    flush().await;
}";
        let found = run(&LockAcrossAwait, src);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].span.line, found[0].span.column), (2, 19));
        assert_eq!(
            found[0].message,
            "guard `g` from `.lock()` is held across the `.await` on line 4"
        );
    }

    #[test]
    fn flags_every_sync_guard_shape() {
        let src = "async fn f() {
            let a = m.lock();
            let b = m.lock()?;
            let c = m.lock().expect(\"poisoned\");
            let d = m.lock().unwrap_or_else(PoisonError::into_inner);
            let e: RwLockWriteGuard<'_, u8> = rw.write().unwrap();
            let _held = rw.read().unwrap();
            x.await;
        }";
        assert_eq!(messages(src).len(), 6);
    }

    #[test]
    fn async_locks_are_fine() {
        let src = "async fn f() {
            let g = m.lock().await;
            let r = rw.read().await;
            x.await;
            match m.lock().await.get(&k) { Some(v) => send(v).await, None => {} }
        }";
        assert!(messages(src).is_empty());
    }

    #[test]
    fn guard_released_before_await_is_fine() {
        let src = "async fn f() {
            let g = m.lock().unwrap();
            use_it(&g);
            drop(g);
            x.await;
            {
                let h = m.lock().unwrap();
                h.push(1);
            }
            y.await;
            m.lock().unwrap().push(2);
            let n = m.lock().unwrap().len();
            let _ = m.lock().unwrap();
            z.await;
        }";
        assert!(messages(src).is_empty(), "{:?}", messages(src));
    }

    #[test]
    fn only_async_code_and_awaits_of_this_future_count() {
        let src = "
            fn sync() { let g = m.lock().unwrap(); }
            async fn spawns() {
                let g = m.lock().unwrap();
                tokio::spawn(async move { x.await });
            }
            fn builds_future() -> impl Future {
                async move {
                    let g = m.lock().unwrap();
                    x.await;
                }
            }
        ";
        assert_eq!(
            messages(src),
            ["guard `g` from `.lock()` is held across the `.await` on line 10"]
        );
    }

    #[test]
    fn flags_lock_temporaries_in_scrutinees() {
        let src = "async fn f() {
            match cache.lock().unwrap().get(&k) {
                Some(v) => send(v).await,
                None => {}
            }
            if let Some(v) = cache.lock().unwrap().get(&k) {
                send(v).await;
            }
            while let Some(job) = queue.lock().unwrap().pop() {
                run(job).await;
            }
            match cache.lock().unwrap().get(&k).cloned() {
                Some(v) => {}
                None => {}
            }
        }";
        let found = messages(src);
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found[0].starts_with("temporary guard in scrutinee from `.lock()`"));
    }

    #[test]
    fn ignores_test_code_and_reads_with_arguments() {
        let src = "
            #[tokio::test]
            async fn t() { let g = m.lock().unwrap(); x.await; }
            async fn io(mut f: File) { let n = f.read(&mut buf); x.await; }
        ";
        assert!(messages(src).is_empty());
    }
}
