//! The grading rubric from `skills/rustgrade/references/rubric.md`, in code.
//! Keep the two in sync: the rubric's worked example is a test below.

use std::collections::HashMap;

use serde::Serialize;

use crate::finding::{Category, Finding, Severity};

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

/// Rubric category. Several scanner [`Category`]s can map to one area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Area {
    Errors,
    Unsafe,
    Concurrency,
    Performance,
    OwnershipApi,
}

impl Area {
    pub const ALL: [Self; 5] = [
        Self::Errors,
        Self::Unsafe,
        Self::Concurrency,
        Self::Performance,
        Self::OwnershipApi,
    ];

    #[must_use]
    pub fn weight(self) -> u32 {
        match self {
            Self::Errors => 25,
            Self::Unsafe | Self::Concurrency | Self::OwnershipApi => 20,
            Self::Performance => 15,
        }
    }

    #[must_use]
    pub fn of(category: Category) -> Self {
        match category {
            Category::Errors | Category::Meta => Self::Errors,
            Category::Unsafe => Self::Unsafe,
            Category::Concurrency => Self::Concurrency,
            Category::Performance => Self::Performance,
            Category::Ownership | Category::ApiDesign => Self::OwnershipApi,
        }
    }

    /// Unsafe and concurrency only apply to crates that use them.
    fn always_applicable(self) -> bool {
        !matches!(self, Self::Unsafe | Self::Concurrency)
    }
}

/// Why a grade was limited below its average.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Cap {
    OneHigh,
    ManyHigh,
    Critical,
    TestsFailing,
}

impl Cap {
    #[must_use]
    pub fn max_score(self) -> u8 {
        match self {
            Self::OneHigh => 89,
            Self::ManyHigh => 79,
            Self::Critical | Self::TestsFailing => 69,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AreaScore {
    pub area: Area,
    /// `None` when the area does not apply to the crate (`n/a`).
    pub score: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Scorecard {
    pub score: u8,
    pub grade: Grade,
    pub areas: Vec<AreaScore>,
    /// The cap that lowered the score, if any.
    pub cap: Option<Cap>,
}

/// Most one rule can deduct from one area.
const PER_RULE_LIMIT: u32 = 30;

fn deduction(severity: Severity) -> u32 {
    match severity {
        Severity::Info => 0,
        Severity::Low => 2,
        Severity::Medium => 5,
        Severity::High => 15,
        Severity::Critical => 40,
    }
}

/// Grades unsuppressed findings.
///
/// `applicable` lists the optional areas the crate uses (`Unsafe`,
/// `Concurrency`); an area with findings is always applicable.
#[must_use]
pub fn grade(findings: &[Finding], applicable: &[Area], tests_failed: bool) -> Scorecard {
    let mut per_rule: HashMap<(Area, &str), u32> = HashMap::new();
    for f in findings {
        *per_rule.entry((Area::of(f.category), f.rule)).or_default() += deduction(f.severity);
    }
    let mut deducted: HashMap<Area, u32> = HashMap::new();
    for ((area, _), d) in per_rule {
        *deducted.entry(area).or_default() += d.min(PER_RULE_LIMIT);
    }

    let areas: Vec<AreaScore> = Area::ALL
        .into_iter()
        .map(|area| {
            let applies = area.always_applicable()
                || applicable.contains(&area)
                || deducted.contains_key(&area);
            let score = applies.then(|| {
                let lost = deducted.get(&area).copied().unwrap_or(0).min(100);
                u8::try_from(100 - lost).unwrap_or(0)
            });
            AreaScore { area, score }
        })
        .collect();

    let (sum, weights) = areas
        .iter()
        .filter_map(|a| {
            a.score
                .map(|s| (u32::from(s) * a.area.weight(), a.area.weight()))
        })
        .fold((0, 0), |(s, w), (ds, dw)| (s + ds, w + dw));
    // Integer rounding to nearest; the always-applicable areas keep `weights` > 0.
    let average = u8::try_from((sum + weights / 2) / weights).unwrap_or(100);

    let highs = findings
        .iter()
        .filter(|f| f.severity == Severity::High)
        .count();
    let caps = [
        (highs == 1, Cap::OneHigh),
        (highs >= 2, Cap::ManyHigh),
        (
            findings.iter().any(|f| f.severity == Severity::Critical),
            Cap::Critical,
        ),
        (tests_failed, Cap::TestsFailing),
    ];
    let cap = caps
        .into_iter()
        .filter(|&(hit, cap)| hit && cap.max_score() < average)
        .map(|(_, cap)| cap)
        .min_by_key(|cap| cap.max_score());
    let score = cap.map_or(average, Cap::max_score);

    Scorecard {
        score,
        grade: Grade::from_score(score),
        areas,
        cap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::Span;

    fn finding(rule: &'static str, category: Category, severity: Severity) -> Finding {
        Finding {
            rule,
            severity,
            category,
            message: String::new(),
            span: Span {
                file: "src/lib.rs".into(),
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 1,
            },
            help: None,
        }
    }

    fn area(card: &Scorecard, area: Area) -> Option<u8> {
        card.areas
            .iter()
            .find(|a| a.area == area)
            .and_then(|a| a.score)
    }

    #[test]
    fn clean_crate_is_a_100() {
        let card = grade(&[], &[], false);
        assert_eq!((card.score, card.grade, card.cap), (100, Grade::A, None));
        assert_eq!(area(&card, Area::Unsafe), None);
    }

    /// The worked example in rubric.md (evals/fixtures/unwrap-daemon).
    #[test]
    fn rubric_example_is_b_89() {
        let findings = [
            finding("unwrap-outside-tests", Category::Errors, Severity::High),
            finding("unwrap-outside-tests", Category::Errors, Severity::Medium),
        ];
        let card = grade(&findings, &[], false);
        assert_eq!(area(&card, Area::Errors), Some(80));
        assert_eq!(area(&card, Area::Unsafe), None);
        assert_eq!(area(&card, Area::Concurrency), None);
        assert_eq!(area(&card, Area::Performance), Some(100));
        assert_eq!(
            (card.score, card.grade, card.cap),
            (89, Grade::B, Some(Cap::OneHigh))
        );
    }

    #[test]
    fn caps_get_stricter_with_more_and_worse_findings() {
        let high = || finding("lock-across-await", Category::Concurrency, Severity::High);
        assert_eq!(
            grade(&[high(), high()], &[], false).cap,
            Some(Cap::ManyHigh)
        );

        let critical = finding("undocumented-unsafe", Category::Unsafe, Severity::Critical);
        let card = grade(&[high(), critical], &[], false);
        assert_eq!((card.cap, card.grade), (Some(Cap::Critical), Grade::D));

        let card = grade(&[], &[], true);
        assert_eq!((card.score, card.cap), (69, Some(Cap::TestsFailing)));
    }

    #[test]
    fn one_rule_deducts_at_most_30_per_area() {
        let findings: Vec<_> = (0..5)
            .map(|_| finding("alloc-in-loop", Category::Performance, Severity::Medium))
            .chain(
                (0..3).map(|_| finding("clone-in-loop", Category::Performance, Severity::Medium)),
            )
            .collect();
        // alloc: 25, clone: 15 -> 60. Five more allocs would still be 60.
        assert_eq!(
            area(&grade(&findings, &[], false), Area::Performance),
            Some(60)
        );

        let many: Vec<_> = (0..10)
            .map(|_| finding("alloc-in-loop", Category::Performance, Severity::Medium))
            .collect();
        assert_eq!(area(&grade(&many, &[], false), Area::Performance), Some(70));
    }

    #[test]
    fn applicable_optional_areas_count_in_the_average() {
        let findings = [finding(
            "ref-owned-param",
            Category::ApiDesign,
            Severity::Low,
        )];
        let without = grade(&findings, &[], false);
        let with = grade(&findings, &[Area::Unsafe, Area::Concurrency], false);
        assert_eq!(area(&with, Area::Unsafe), Some(100));
        assert!(with.score >= without.score);
        assert_eq!(area(&with, Area::OwnershipApi), Some(98));
    }

    #[test]
    fn meta_findings_land_in_errors() {
        let findings = [finding("unused-suppression", Category::Meta, Severity::Low)];
        assert_eq!(area(&grade(&findings, &[], false), Area::Errors), Some(98));
    }
}
