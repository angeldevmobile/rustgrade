use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity};

pub struct UnboundedChannel;

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
        "Unbounded channel without backpressure"
    }

    fn check(&self, _ctx: &FileContext<'_>, _out: &mut Vec<Finding>) {
        // TODO: implement.
    }
}
