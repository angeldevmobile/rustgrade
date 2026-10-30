//! Which optional rubric areas a crate uses, so `audit` can mark the others
//! `n/a`, and how much production code there is, so grading can use density.

use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use crate::rules::common::{is_test_only, is_test_path};

/// Features found while scanning. Merged across files with [`Usage::merge`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// `unsafe` blocks, fns, impls or traits, or `extern` blocks.
    pub unsafe_code: bool,
    /// Async code, threads, locks, channels, atomics or a concurrency crate.
    pub concurrency: bool,
    /// Lines of production code: not blank, not comments, not test code.
    pub code_lines: usize,
}

impl Usage {
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        Self {
            unsafe_code: self.unsafe_code || other.unsafe_code,
            concurrency: self.concurrency || other.concurrency,
            code_lines: self.code_lines + other.code_lines,
        }
    }
}

/// Lines of production code in one file: test files count 0, and lines of
/// test-only items (`#[cfg(test)] mod tests`, `#[test] fn`) are left out.
/// Comment detection is by line prefix, which is close enough for a size.
#[must_use]
pub fn code_lines(path: &str, source: &str, file: &syn::File) -> usize {
    if is_test_path(path) {
        return 0;
    }
    let mut tests = TestRanges(Vec::new());
    tests.visit_file(file);
    source
        .lines()
        .enumerate()
        .filter(|(i, line)| {
            let l = line.trim_start();
            let n = i + 1;
            !(l.is_empty()
                || l.starts_with("//")
                || l.starts_with("/*")
                || l.starts_with('*')
                || tests
                    .0
                    .iter()
                    .any(|&(start, end)| (start..=end).contains(&n)))
        })
        .count()
}

/// Line ranges of test-only items. Does not descend into them.
struct TestRanges(Vec<(usize, usize)>);

impl TestRanges {
    fn record(&mut self, attrs: &[syn::Attribute], span: proc_macro2::Span) -> bool {
        let test = is_test_only(attrs);
        if test {
            self.0.push((span.start().line, span.end().line));
        }
        test
    }
}

impl<'ast> Visit<'ast> for TestRanges {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if !self.record(&m.attrs, m.span()) {
            visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if !self.record(&f.attrs, f.span()) {
            visit::visit_item_fn(self, f);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if !self.record(&i.attrs, i.span()) {
            visit::visit_item_impl(self, i);
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if !self.record(&f.attrs, f.span()) {
            visit::visit_impl_item_fn(self, f);
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
    fn merge_ors_areas_and_adds_lines() {
        let a = Usage {
            unsafe_code: true,
            concurrency: false,
            code_lines: 10,
        };
        let b = Usage {
            unsafe_code: false,
            concurrency: true,
            code_lines: 5,
        };
        assert_eq!(
            a.merge(b),
            Usage {
                unsafe_code: true,
                concurrency: true,
                code_lines: 15,
            }
        );
    }

    #[test]
    fn code_lines_skip_blanks_comments_and_tests() {
        let src = "//! Crate docs.
/// Adds.
fn add(a: u32, b: u32) -> u32 {
    // sum
    a + b
}

#[cfg(test)]
mod tests {
    #[test]
    fn adds() { assert_eq!(super::add(1, 2), 3); }
}
";
        let ast = syn::parse_file(src).expect("parses");
        assert_eq!(code_lines("src/lib.rs", src, &ast), 3);
        assert_eq!(code_lines("tests/api.rs", src, &ast), 0);
    }
}
