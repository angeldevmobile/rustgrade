//! Which optional rubric areas a crate uses, so `audit` can mark the others `n/a`.

use syn::visit::{self, Visit};

/// Features found while scanning. Merged across files with [`Usage::merge`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// `unsafe` blocks, fns, impls or traits, or `extern` blocks.
    pub unsafe_code: bool,
    /// Async code, threads, locks, channels, atomics or a concurrency crate.
    pub concurrency: bool,
}

impl Usage {
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        Self {
            unsafe_code: self.unsafe_code || other.unsafe_code,
            concurrency: self.concurrency || other.concurrency,
        }
    }
}

/// Identifiers that only appear in concurrent code. Matching is by name, so a
/// local variable called `thread` also counts; that only makes an area
/// applicable, which never raises a grade.
const CONCURRENCY_IDENTS: &[&str] = &[
    "thread",
    "Mutex",
    "RwLock",
    "Condvar",
    "Barrier",
    "mpsc",
    "tokio",
    "async_std",
    "smol",
    "rayon",
    "crossbeam",
    "crossbeam_channel",
    "flume",
    "async_channel",
    "parking_lot",
];

#[must_use]
pub fn detect(file: &syn::File) -> Usage {
    let mut v = Detector::default();
    v.visit_file(file);
    v.0
}

#[derive(Default)]
struct Detector(Usage);

impl<'ast> Visit<'ast> for Detector {
    fn visit_expr_unsafe(&mut self, e: &'ast syn::ExprUnsafe) {
        self.0.unsafe_code = true;
        visit::visit_expr_unsafe(self, e);
    }

    fn visit_signature(&mut self, s: &'ast syn::Signature) {
        self.0.unsafe_code |= s.unsafety.is_some();
        self.0.concurrency |= s.asyncness.is_some();
        visit::visit_signature(self, s);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        self.0.unsafe_code |= i.unsafety.is_some();
        visit::visit_item_impl(self, i);
    }

    fn visit_item_trait(&mut self, t: &'ast syn::ItemTrait) {
        self.0.unsafe_code |= t.unsafety.is_some();
        visit::visit_item_trait(self, t);
    }

    fn visit_item_foreign_mod(&mut self, m: &'ast syn::ItemForeignMod) {
        self.0.unsafe_code = true;
        visit::visit_item_foreign_mod(self, m);
    }

    fn visit_expr_async(&mut self, e: &'ast syn::ExprAsync) {
        self.0.concurrency = true;
        visit::visit_expr_async(self, e);
    }

    fn visit_expr_await(&mut self, e: &'ast syn::ExprAwait) {
        self.0.concurrency = true;
        visit::visit_expr_await(self, e);
    }

    fn visit_ident(&mut self, i: &'ast syn::Ident) {
        if !self.0.concurrency {
            let name = i.to_string();
            self.0.concurrency =
                name.starts_with("Atomic") || CONCURRENCY_IDENTS.contains(&name.as_str());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(src: &str) -> Usage {
        detect(&syn::parse_file(src).expect("parses"))
    }

    #[test]
    fn plain_code_uses_neither() {
        let u = usage("fn add(a: u32, b: u32) -> u32 { a + b } struct S { v: Vec<String> }");
        assert_eq!(u, Usage::default());
    }

    #[test]
    fn detects_every_form_of_unsafe() {
        for src in [
            "fn f(p: *const u8) -> u8 { unsafe { *p } }",
            "unsafe fn f() {}",
            "struct S; unsafe impl Send for S {}",
            "unsafe trait T {}",
            "extern \"C\" { fn abs(x: i32) -> i32; }",
        ] {
            assert!(usage(src).unsafe_code, "{src}");
        }
    }

    #[test]
    fn detects_async_threads_locks_channels_and_atomics() {
        for src in [
            "async fn f() {}",
            "fn f() -> impl Future { async {} }",
            "fn f() { std::thread::spawn(|| {}); }",
            "use std::sync::Mutex;",
            "fn f(x: &parking_lot::RwLock<u8>) {}",
            "fn f() { let (tx, rx) = std::sync::mpsc::channel::<u8>(); }",
            "static N: AtomicUsize = AtomicUsize::new(0);",
            "fn f(v: &[u8]) { v.par_iter(); rayon::join(|| {}, || {}); }",
        ] {
            assert!(usage(src).concurrency, "{src}");
        }
    }

    #[test]
    fn merge_is_an_or() {
        let a = Usage {
            unsafe_code: true,
            concurrency: false,
        };
        let b = Usage {
            unsafe_code: false,
            concurrency: true,
        };
        assert_eq!(
            a.merge(b),
            Usage {
                unsafe_code: true,
                concurrency: true
            }
        );
    }
}
