use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity};

pub struct AllocInLoop;

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
        "Allocation (`Vec::new`, `String::new`, `format!`, ...) inside a loop body"
    }

    fn check(&self, _ctx: &FileContext<'_>, _out: &mut Vec<Finding>) {
        // TODO: implement.
    }
}
