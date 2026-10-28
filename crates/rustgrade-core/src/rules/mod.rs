//! Rule trait and registry. One file per rule; register new rules in `RULES`.

use crate::finding::{Category, Finding, Severity};

mod alloc_in_loop;
mod anyhow_in_lib_api;
mod clone_in_loop;
mod common;
mod lock_across_await;
mod ref_owned_param;
mod unbounded_channel;
mod undocumented_unsafe;
mod unwrap_outside_tests;

/// What a rule sees for one file.
pub struct FileContext<'a> {
    pub path: &'a str,
    /// Raw text, for what the AST drops (comments).
    pub source: &'a str,
    pub ast: &'a syn::File,
    /// File matches a configured hot path; performance rules may raise severity.
    pub is_hot: bool,
    /// File belongs to a library target, so its `pub` items are a public API.
    pub in_lib: bool,
}

pub trait Rule: Sync {
    /// Stable kebab-case ID. Part of the public contract once v1.0 ships.
    fn id(&self) -> &'static str;
    fn category(&self) -> Category;
    fn default_severity(&self) -> Severity;
    /// One-line description for `rustgrade rules`.
    fn summary(&self) -> &'static str;
    /// Rationale plus before/after code, for `rustgrade rules --explain`.
    fn explain(&self) -> &'static str;
    fn check(&self, ctx: &FileContext<'_>, out: &mut Vec<Finding>);
}

static RULES: &[&dyn Rule] = &[
    &clone_in_loop::CloneInLoop,
    &unwrap_outside_tests::UnwrapOutsideTests,
    &undocumented_unsafe::UndocumentedUnsafe,
    &lock_across_await::LockAcrossAwait,
    &alloc_in_loop::AllocInLoop,
    &ref_owned_param::RefOwnedParam,
    &anyhow_in_lib_api::AnyhowInLibApi,
    &unbounded_channel::UnboundedChannel,
];

#[must_use]
pub fn registry() -> &'static [&'static dyn Rule] {
    RULES
}

#[must_use]
pub fn find(id: &str) -> Option<&'static dyn Rule> {
    RULES.iter().copied().find(|r| r.id() == id)
}

#[cfg(test)]
#[allow(dead_code)] // used by rule tests as they get implemented
pub(crate) mod test_util {
    use super::{FileContext, Rule};
    use crate::finding::Finding;

    /// Runs one rule over a source snippet, as if it lived in `src/lib.rs`.
    pub fn run(rule: &dyn Rule, src: &str) -> Vec<Finding> {
        run_at(rule, "src/lib.rs", src)
    }

    /// Runs one rule over a source snippet at a given path.
    pub fn run_at(rule: &dyn Rule, path: &str, src: &str) -> Vec<Finding> {
        run_with(rule, path, src, false, true)
    }

    /// Runs one rule over a source snippet in a file marked hot.
    pub fn run_hot(rule: &dyn Rule, src: &str) -> Vec<Finding> {
        run_with(rule, "src/lib.rs", src, true, true)
    }

    /// Runs one rule over a source snippet in a binary target.
    pub fn run_bin(rule: &dyn Rule, src: &str) -> Vec<Finding> {
        run_with(rule, "src/main.rs", src, false, false)
    }

    fn run_with(
        rule: &dyn Rule,
        path: &str,
        src: &str,
        is_hot: bool,
        in_lib: bool,
    ) -> Vec<Finding> {
        let ast = syn::parse_file(src).expect("test source must parse");
        let ctx = FileContext {
            path,
            source: src,
            ast: &ast,
            is_hot,
            in_lib,
        };
        let mut out = Vec::new();
        rule.check(&ctx, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_kebab_case() {
        let mut seen = HashSet::new();
        for r in registry() {
            assert!(seen.insert(r.id()), "duplicate rule id {}", r.id());
            assert!(
                r.id().chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "rule id {} is not kebab-case",
                r.id()
            );
        }
    }

    #[test]
    fn every_rule_explains_itself_with_before_and_after() {
        for r in registry() {
            let text = r.explain();
            for part in ["Why:", "Before:", "After:"] {
                assert!(text.contains(part), "{} explain() lacks `{part}`", r.id());
            }
        }
    }
}
