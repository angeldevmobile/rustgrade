use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity};

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
        "Mutex guard held across `.await`"
    }

    fn check(&self, _ctx: &FileContext<'_>, _out: &mut Vec<Finding>) {
        // TODO: implement.
    }
}
