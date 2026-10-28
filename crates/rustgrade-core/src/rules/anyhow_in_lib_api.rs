use super::{FileContext, Rule};
use crate::finding::{Category, Finding, Severity};

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
        "Public library API returns `anyhow::Result`"
    }

    fn check(&self, _ctx: &FileContext<'_>, _out: &mut Vec<Finding>) {
        // TODO: implement.
    }
}
