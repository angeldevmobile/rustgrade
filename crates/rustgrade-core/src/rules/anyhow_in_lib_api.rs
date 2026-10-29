use std::collections::HashSet;

use syn::visit::{self, Visit};

use super::common::{is_test_only, is_test_path};
use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity, Span};

pub struct AnyhowInLibApi;

impl Rule for AnyhowInLibApi {
    fn id(&self) -> &'static str {
        "anyhow-in-lib-api"
    }

    fn category(&self) -> Category {
        Category::Errors
    }

    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn summary(&self) -> &'static str {
        "Public library API returns `anyhow::Result` / `anyhow::Error`"
    }

    fn explain(&self) -> &'static str {
        EXPLAIN
    }

    // Approximations: `use` imports are collected file-wide (ignoring scope),
    // and a `pub` item in a file reached through a private `mod foo;` still
    // counts, because one file cannot see how its module is declared.
    fn check(&self, ctx: &FileContext<'_>, out: &mut Vec<Finding>) {
        if !ctx.in_lib || is_test_path(ctx.path) {
            return;
        }
        let mut imports = Imports::default();
        imports.visit_file(ctx.ast);
        Visitor {
            rule: self,
            path: ctx.path,
            aliases: &imports.0,
            out,
        }
        .visit_file(ctx.ast);
    }
}

/// Local names that refer to `anyhow::Result` / `anyhow::Error`
/// (`use anyhow::Result;`, `use anyhow::{Error as E};`, `use anyhow::*;`).
#[derive(Default)]
struct Imports(HashSet<String>);

impl Imports {
    fn collect(&mut self, tree: &syn::UseTree, under_anyhow: bool, at_root: bool) {
        match tree {
            syn::UseTree::Path(p) => {
                let under = under_anyhow || (at_root && p.ident == "anyhow");
                self.collect(&p.tree, under, false);
            }
            syn::UseTree::Name(n) if under_anyhow && is_error_type(&n.ident) => {
                self.0.insert(n.ident.to_string());
            }
            syn::UseTree::Rename(r) if under_anyhow && is_error_type(&r.ident) => {
                self.0.insert(r.rename.to_string());
            }
            syn::UseTree::Glob(_) if under_anyhow => {
                self.0.extend(["Result".to_owned(), "Error".to_owned()]);
            }
            syn::UseTree::Group(g) => {
                for t in &g.items {
                    self.collect(t, under_anyhow, at_root);
                }
            }
            _ => {}
        }
    }
}

fn is_error_type(ident: &syn::Ident) -> bool {
    ident == "Result" || ident == "Error"
}

impl<'ast> Visit<'ast> for Imports {
    fn visit_item_use(&mut self, u: &'ast syn::ItemUse) {
        // A leading `::anyhow::` is the same crate.
        self.collect(&u.tree, false, true);
    }
}

/// Finds any `anyhow::...` path or imported alias inside a type.
struct MentionsAnyhow<'a> {
    aliases: &'a HashSet<String>,
    found: bool,
}

impl<'ast> Visit<'ast> for MentionsAnyhow<'_> {
    fn visit_type_path(&mut self, t: &'ast syn::TypePath) {
        let segs = &t.path.segments;
        let first = segs.first().map(|s| s.ident.to_string());
        if first.as_deref() == Some("anyhow")
            || (t.qself.is_none()
                && segs.len() == 1
                && first.is_some_and(|f| self.aliases.contains(&f)))
        {
            self.found = true;
        }
        visit::visit_type_path(self, t);
    }
}

struct Visitor<'a, 'o> {
    rule: &'a AnyhowInLibApi,
    path: &'a str,
    aliases: &'a HashSet<String>,
    out: &'o mut Vec<Finding>,
}

impl Visitor<'_, '_> {
    fn check_signature(&mut self, sig: &syn::Signature) {
        let syn::ReturnType::Type(_, ty) = &sig.output else {
            return;
        };
        let mut m = MentionsAnyhow {
            aliases: self.aliases,
            found: false,
        };
        m.visit_type(ty);
        if m.found {
            self.out.push(Finding {
                rule: self.rule.id(),
                severity: self.rule.default_severity(),
                category: self.rule.category(),
                message: format!("public fn `{}` returns an `anyhow` error", sig.ident),
                span: Span::from_proc_macro(self.path, sig.ident.span()),
                help: Some(
                    "expose a typed error (e.g. a `thiserror` enum) so callers can match on failures; keep `anyhow` for binaries"
                        .into(),
                ),
            });
        }
    }
}

fn is_pub(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

impl<'ast> Visit<'ast> for Visitor<'_, '_> {
    // Inline private modules are not public API.
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if is_pub(&m.vis) && !is_test_only(&m.attrs) {
            visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if is_pub(&f.vis) && !is_test_only(&f.attrs) {
            self.check_signature(&f.sig);
        }
    }

    // Trait impls follow the trait's signature, so only inherent impls are checked.
    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if i.trait_.is_some() || is_test_only(&i.attrs) {
            return;
        }
        for item in &i.items {
            if let syn::ImplItem::Fn(f) = item
                && is_pub(&f.vis)
                && !is_test_only(&f.attrs)
            {
                self.check_signature(&f.sig);
            }
        }
    }

    fn visit_item_trait(&mut self, t: &'ast syn::ItemTrait) {
        if !is_pub(&t.vis) {
            return;
        }
        for item in &t.items {
            if let syn::TraitItem::Fn(f) = item {
                self.check_signature(&f.sig);
            }
        }
    }
}

const EXPLAIN: &str = r#"Why: callers of a library need to match on failures (retry a timeout,
return 404 on not-found). `anyhow::Error` erases the type, so they can only
print it. Libraries expose a typed error; binaries keep `anyhow`. Only `pub`
items of library targets are checked.

Before:
    pub fn load(path: &Path) -> anyhow::Result<Config> { ... }

After:
    #[derive(Debug, thiserror::Error)]
    #[non_exhaustive]
    pub enum ConfigError {
        #[error("reading {path}")]
        Io { path: PathBuf, #[source] source: std::io::Error },
        #[error("invalid config")]
        Parse(#[from] toml::de::Error),
    }

    pub fn load(path: &Path) -> Result<Config, ConfigError> { ... }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::{run, run_bin};

    fn names(src: &str) -> Vec<String> {
        run(&AnyhowInLibApi, src)
            .into_iter()
            .map(|f| f.message)
            .collect()
    }

    #[test]
    fn flags_qualified_paths_with_location() {
        let found = run(
            &AnyhowInLibApi,
            "pub fn load() -> anyhow::Result<u32> { todo!() }",
        );
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].span.line, found[0].span.column), (1, 8));

        let src = "pub fn a() -> Result<(), anyhow::Error> { todo!() }
                   pub fn b() -> Option<::anyhow::Result<()>> { todo!() }";
        assert_eq!(names(src).len(), 2);
    }

    #[test]
    fn follows_imports_and_renames() {
        let src = "
            use anyhow::{Context, Result};
            use anyhow::Error as AnyErr;
            pub fn a() -> Result<()> { todo!() }
            pub fn b() -> std::result::Result<(), AnyErr> { todo!() }
        ";
        assert_eq!(
            names(src),
            [
                "public fn `a` returns an `anyhow` error",
                "public fn `b` returns an `anyhow` error",
            ]
        );
    }

    #[test]
    fn ignores_typed_errors_and_other_crates_result() {
        let src = "
            use std::io::Result;
            use crate::anyhow::Error;
            pub fn a() -> Result<()> { todo!() }
            pub fn b() -> Result<(), MyError> { todo!() }
        ";
        assert_eq!(names(src), Vec::<String>::new());
    }

    #[test]
    fn only_public_items_count() {
        let src = "
            fn private() -> anyhow::Result<()> { todo!() }
            pub(crate) fn internal() -> anyhow::Result<()> { todo!() }
            mod hidden { pub fn f() -> anyhow::Result<()> { todo!() } }
            impl S {
                pub fn method(&self) -> anyhow::Result<()> { todo!() }
                fn helper(&self) -> anyhow::Result<()> { todo!() }
            }
            impl FromStr for S { type Err = anyhow::Error; fn from_str(s: &str) -> anyhow::Result<Self> { todo!() } }
            pub trait Store { fn get(&self) -> anyhow::Result<u8>; }
            trait Private { fn get(&self) -> anyhow::Result<u8>; }
        ";
        assert_eq!(
            names(src),
            [
                "public fn `method` returns an `anyhow` error",
                "public fn `get` returns an `anyhow` error",
            ]
        );
    }

    #[test]
    fn binaries_may_use_anyhow() {
        assert!(
            run_bin(
                &AnyhowInLibApi,
                "pub fn run() -> anyhow::Result<()> { todo!() }"
            )
            .is_empty()
        );
    }
}
