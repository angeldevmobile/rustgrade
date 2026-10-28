use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity};

pub struct CloneInLoop;

impl Rule for CloneInLoop {
    fn id(&self) -> &'static str {
        "clone-in-loop"
    }

    fn category(&self) -> Category {
        Category::Performance
    }

    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn summary(&self) -> &'static str {
        "`.clone()` inside a loop body"
    }

    fn check(&self, _ctx: &FileContext<'_>, _out: &mut Vec<Finding>) {
        // TODO: implement.
    }
}
