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
    /// At least one `high` finding per 1000 lines of code.
    OneHigh,
    /// At least two `high` findings per 1000 lines of code.
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
    /// `high` findings per 1000 lines of code, what the high caps look at.
    pub highs_per_kloc: f64,
}

/// Most one rule can deduct from one area.
const PER_RULE_LIMIT: u32 = 30;

/// Deductions and high caps are per this many lines of code; smaller crates
/// count as this size, so they are graded on raw counts.
const SIZE_UNIT: u32 = 1000;

fn deduction(severity: Severity) -> u32 {
    match severity {
        Severity::Info => 0,
        Severity::Low => 2,
        Severity::Medium => 5,
        Severity::High => 15,
        Severity::Critical => 40,
    }
}

/// One graded issue: a scanner finding, a clippy warning or a judgment call.
/// Borrowed, so clippy's dynamic lint names need no `'static` strings.
#[derive(Debug, Clone, Copy)]
pub struct Deduction<'a> {
    /// Rule or lint ID; the per-rule limit groups by it.
    pub rule: &'a str,
    pub category: Category,
    pub severity: Severity,
}

impl<'a> From<&'a Finding> for Deduction<'a> {
    fn from(f: &'a Finding) -> Self {
        Self {
            rule: f.rule,
            category: f.category,
            severity: f.severity,
        }
    }
}

/// Grades unsuppressed issues with the rubric.
///
/// `applicable` lists the optional areas the crate uses (`Unsafe`,
/// `Concurrency`); an area with deductions is always applicable.
/// `code_lines` is the crate's production code size ([`crate::Usage`]):
/// findings are weighed per 1000 lines, so a large crate is not graded on
/// raw counts.
#[must_use]
pub fn grade(
    issues: &[Deduction<'_>],
    applicable: &[Area],
    tests_failed: bool,
    code_lines: usize,
) -> Scorecard {
    let size = u32::try_from(code_lines).unwrap_or(u32::MAX).max(SIZE_UNIT);
    // Per SIZE_UNIT lines, rounded to nearest. u64: raw * 1000 can exceed u32.
    let per_unit =
        |raw: u32| (u64::from(raw) * u64::from(SIZE_UNIT) + u64::from(size / 2)) / u64::from(size);

    let mut per_rule: HashMap<(Area, &str), u32> = HashMap::new();
    for i in issues {
        *per_rule.entry((Area::of(i.category), i.rule)).or_default() += deduction(i.severity);
    }
    let mut deducted: HashMap<Area, u32> = HashMap::new();
    for ((area, _), d) in per_rule {
        let d = u32::try_from(per_unit(d)).unwrap_or(u32::MAX);
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

    let highs = issues
        .iter()
        .filter(|i| i.severity == Severity::High)
        .count();
    // `highs / size >= n / SIZE_UNIT`, in integers.
    let highs_at_least = |n: u64| {
        u64::try_from(highs).unwrap_or(u64::MAX) * u64::from(SIZE_UNIT) >= n * u64::from(size)
    };
    let caps = [
        (highs > 0 && highs_at_least(1), Cap::OneHigh),
        (highs > 0 && highs_at_least(2), Cap::ManyHigh),
        (
            issues.iter().any(|i| i.severity == Severity::Critical),
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

    #[allow(clippy::cast_precision_loss)] // a ratio for display, not arithmetic
    let highs_per_kloc = highs as f64 * f64::from(SIZE_UNIT) / f64::from(size);
    Scorecard {
        score,
        grade: Grade::from_score(score),
        areas,
        cap,
        highs_per_kloc,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(rule: &'static str, category: Category, severity: Severity) -> Deduction<'static> {
        Deduction {
            rule,
            category,
            severity,
        }
    }

    /// Below `SIZE_UNIT`: graded on raw counts.
    const SMALL: usize = 200;

    fn area(card: &Scorecard, area: Area) -> Option<u8> {
        card.areas
            .iter()
            .find(|a| a.area == area)
            .and_then(|a| a.score)
    }

    #[test]
    fn clean_crate_is_a_100() {
        let card = grade(&[], &[], false, SMALL);
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
        let card = grade(&findings, &[], false, SMALL);
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
            grade(&[high(), high()], &[], false, SMALL).cap,
            Some(Cap::ManyHigh)
        );

        let critical = finding("judgment", Category::Unsafe, Severity::Critical);
        let card = grade(&[high(), critical], &[], false, SMALL);
        assert_eq!((card.cap, card.grade), (Some(Cap::Critical), Grade::D));

        let card = grade(&[], &[], true, SMALL);
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
            area(&grade(&findings, &[], false, SMALL), Area::Performance),
            Some(60)
        );

        let many: Vec<_> = (0..10)
            .map(|_| finding("alloc-in-loop", Category::Performance, Severity::Medium))
            .collect();
        assert_eq!(
            area(&grade(&many, &[], false, SMALL), Area::Performance),
            Some(70)
        );
    }

    #[test]
    fn applicable_optional_areas_count_in_the_average() {
        let findings = [finding(
            "ref-owned-param",
            Category::ApiDesign,
            Severity::Low,
        )];
        let without = grade(&findings, &[], false, SMALL);
        let with = grade(&findings, &[Area::Unsafe, Area::Concurrency], false, SMALL);
        assert_eq!(area(&with, Area::Unsafe), Some(100));
        assert!(with.score >= without.score);
        assert_eq!(area(&with, Area::OwnershipApi), Some(98));
    }

    #[test]
    fn meta_findings_land_in_errors() {
        let findings = [finding("unused-suppression", Category::Meta, Severity::Low)];
        assert_eq!(
            area(&grade(&findings, &[], false, SMALL), Area::Errors),
            Some(98)
        );
    }

    #[test]
    fn large_crates_are_graded_per_1000_lines() {
        let unwraps: Vec<_> = (0..20)
            .map(|_| finding("unwrap-outside-tests", Category::Errors, Severity::High))
            .collect();
        // 20 highs in 200 lines: errors at 70 (rule limit), capped at C.
        let small = grade(&unwraps, &[], false, SMALL);
        assert_eq!(
            (area(&small, Area::Errors), small.cap),
            (Some(70), Some(Cap::ManyHigh))
        );

        // The same 20 in 40k lines: 300 / 40 = 7.5 -> 8 deducted, 0.5 per 1k: no cap.
        let large = grade(&unwraps, &[], false, 40_000);
        assert_eq!((area(&large, Area::Errors), large.cap), (Some(92), None));
        assert!((large.highs_per_kloc - 0.5).abs() < f64::EPSILON);

        // 1 per 1k caps at B, 2 per 1k at C.
        assert_eq!(grade(&unwraps, &[], false, 20_000).cap, Some(Cap::OneHigh));
        assert_eq!(grade(&unwraps, &[], false, 10_000).cap, Some(Cap::ManyHigh));
    }

    #[test]
    fn critical_caps_regardless_of_size() {
        let critical = [finding("judgment", Category::Unsafe, Severity::Critical)];
        assert_eq!(
            grade(&critical, &[], false, 1_000_000).cap,
            Some(Cap::Critical)
        );
    }
}
