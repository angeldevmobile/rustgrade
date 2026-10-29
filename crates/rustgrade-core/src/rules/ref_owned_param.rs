use syn::visit::{self, Visit};

use super::common::{is_test_only, is_test_path};
use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity, Span};

pub struct RefOwnedParam;

/// Owned type behind `&` and the borrowed type to take instead.
const BORROWED_FORM: &[(&str, &str)] = &[
    ("String", "&str"),
    ("Vec", "&[T]"),
    ("Box", "&T"),
    ("PathBuf", "&Path"),
    ("OsString", "&OsStr"),
];

impl Rule for RefOwnedParam {
    fn id(&self) -> &'static str {
        "ref-owned-param"
    }

    fn category(&self) -> Category {
        Category::ApiDesign
    }

    fn default_severity(&self) -> Severity {
        Severity::Low
    }

    fn summary(&self) -> &'static str {
        "Parameter takes `&String` / `&Vec<T>` / `&Box<T>` / `&PathBuf` instead of the borrowed form"
    }

    fn explain(&self) -> &'static str {
        EXPLAIN
    }

    fn check(&self, ctx: &FileContext<'_>, out: &mut Vec<Finding>) {
        if is_test_path(ctx.path) {
            return;
        }
        Visitor {
            rule: self,
            path: ctx.path,
            out,
        }
        .visit_file(ctx.ast);
    }
}

struct Visitor<'a, 'o> {
    rule: &'a RefOwnedParam,
    path: &'a str,
    out: &'o mut Vec<Finding>,
}

impl Visitor<'_, '_> {
    fn check_signature(&mut self, sig: &syn::Signature) {
        for input in &sig.inputs {
            let syn::FnArg::Typed(arg) = input else {
                continue;
            };
            // `&mut Vec<T>` / `&mut String` are fine: the callee may grow them.
            let syn::Type::Reference(r) = &*arg.ty else {
                continue;
            };
            if r.mutability.is_some() {
                continue;
            }
            let syn::Type::Path(p) = &*r.elem else {
                continue;
            };
            let Some(seg) = p.path.segments.last() else {
                continue;
            };
            let Some((owned, borrowed)) = BORROWED_FORM.iter().find(|(o, _)| seg.ident == o) else {
                continue;
            };
            self.out.push(Finding {
                rule: self.rule.id(),
                severity: self.rule.default_severity(),
                category: self.rule.category(),
                message: format!("`&{owned}` parameter in `{}`", sig.ident),
                span: Span::from_proc_macro(self.path, seg.ident.span()),
                help: Some(format!(
                    "take `{borrowed}`: `&{owned}` coerces to it, and it also accepts borrowed data"
                )),
            });
        }
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
            self.check_signature(&f.sig);
            visit::visit_item_fn(self, f);
        }
    }

    // Trait impls must match the trait's signature, so only inherent impls are checked.
    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if i.trait_.is_none() && !is_test_only(&i.attrs) {
            visit::visit_item_impl(self, i);
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if !is_test_only(&f.attrs) {
            self.check_signature(&f.sig);
            visit::visit_impl_item_fn(self, f);
        }
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        self.check_signature(&f.sig);
        visit::visit_trait_item_fn(self, f);
    }
}

const EXPLAIN: &str = r#"Why: `&String` forces every caller to own a `String`. `&str` accepts a
`String` (by deref), a literal or a slice of a larger buffer. The same holds
for `&Vec<T>` -> `&[T]`, `&Box<T>` -> `&T`, `&PathBuf` -> `&Path` and
`&OsString` -> `&OsStr`. `&mut Vec<T>` / `&mut String` are fine: the callee
may grow them. Trait impls are skipped; the trait fixes their signature.

Before:
    fn greet(name: &String) -> String { format!("hello {name}") }

After:
    fn greet(name: &str) -> String { format!("hello {name}") }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::run;

    fn messages(src: &str) -> Vec<String> {
        run(&RefOwnedParam, src)
            .into_iter()
            .map(|f| f.message)
            .collect()
    }

    #[test]
    fn flags_each_owned_type_with_location() {
        let src =
            "fn f(a: &String, b: &Vec<u8>, c: &Box<T>, d: &std::path::PathBuf, e: &OsString) {}";
        assert_eq!(messages(src).len(), 5);

        let found = run(&RefOwnedParam, "fn greet(\n    name: &String,\n) {}");
        assert_eq!((found[0].span.line, found[0].span.column), (2, 12));
        assert_eq!(
            found[0]
                .help
                .as_deref()
                .map(|h| h.starts_with("take `&str`")),
            Some(true)
        );
    }

    #[test]
    fn accepts_borrowed_forms_owned_values_and_mut_refs() {
        let src =
            "fn f(a: &str, b: &[u8], c: String, d: Vec<u8>, e: &mut Vec<u8>, g: &mut String) {}";
        assert_eq!(messages(src), Vec::<String>::new());
    }

    #[test]
    fn checks_inherent_methods_and_trait_definitions_but_not_trait_impls() {
        let src = "
            impl S { fn a(&self, v: &Vec<u8>) {} }
            trait T { fn b(&self, s: &String); }
            impl T for S { fn b(&self, s: &String) {} }
        ";
        assert_eq!(
            messages(src),
            ["`&Vec` parameter in `a`", "`&String` parameter in `b`"]
        );
    }

    #[test]
    fn ignores_test_code() {
        assert_eq!(
            messages("#[cfg(test)] mod t { fn h(s: &String) {} }"),
            Vec::<String>::new()
        );
    }
}
