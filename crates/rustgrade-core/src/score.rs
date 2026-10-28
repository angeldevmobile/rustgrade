use serde::Serialize;

use crate::finding::{Finding, Severity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Grade {
    A,
    B,
    C,
    D,
    F,
}

impl Grade {
    #[must_use]
    pub fn from_score(score: u8) -> Self {
        match score {
            90.. => Self::A,
            80..=89 => Self::B,
            70..=79 => Self::C,
            60..=69 => Self::D,
            _ => Self::F,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Scorecard {
    pub score: u8,
    pub grade: Grade,
}

// TODO: per-category weights and caps from skills/rustgrade/references/rubric.md.
// Placeholder: flat penalty per severity; any critical finding caps the grade at D.
#[must_use]
pub fn grade(findings: &[Finding]) -> Scorecard {
    let penalty: u32 = findings
        .iter()
        .map(|f| match f.severity {
            Severity::Info => 0,
            Severity::Low => 1,
            Severity::Medium => 3,
            Severity::High => 8,
            Severity::Critical => 20,
        })
        .sum();
    let mut score = u8::try_from(100u32.saturating_sub(penalty)).unwrap_or(0);
    if findings.iter().any(|f| f.severity == Severity::Critical) {
        score = score.min(69);
    }
    Scorecard {
        score,
        grade: Grade::from_score(score),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_findings_is_a() {
        assert_eq!(grade(&[]).grade, Grade::A);
    }
}
