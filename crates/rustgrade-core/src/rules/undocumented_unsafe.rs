use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity};

pub struct UndocumentedUnsafe;

impl Rule for UndocumentedUnsafe {
    fn id(&self) -> &'static str {
        "undocumented-unsafe"
    }

    fn category(&self) -> Category {
        Category::Unsafe
    }

    fn default_severity(&self) -> Severity {
        Severity::Critical
    }

    fn summary(&self) -> &'static str {
        "`unsafe` block without a `// SAFETY:` comment"
    }

    fn check(&self, _ctx: &FileContext<'_>, _out: &mut Vec<Finding>) {
        // TODO: implement.
    }
}
