use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity};

pub struct RefOwnedParam;

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
        "Parameter takes `&String` / `&Vec<T>` / `&Box<T>` instead of a slice or `&T`"
    }

    fn check(&self, _ctx: &FileContext<'_>, _out: &mut Vec<Finding>) {
        // TODO: implement.
    }
}
