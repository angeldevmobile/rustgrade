use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Ownership,
    Errors,
    Performance,
    Concurrency,
    Unsafe,
    ApiDesign,
}

/// 1-based line and column, matching what editors and SARIF expect.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Span {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl Span {
    #[must_use]
    pub fn from_proc_macro(file: &str, span: proc_macro2::Span) -> Self {
        let (start, end) = (span.start(), span.end());
        Self {
            file: file.to_owned(),
            line: start.line,
            column: start.column + 1,
            end_line: end.line,
            end_column: end.column + 1,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub rule: &'static str,
    pub severity: Severity,
    pub category: Category,
    pub message: String,
    pub span: Span,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}
