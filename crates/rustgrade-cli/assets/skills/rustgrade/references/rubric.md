# Grading rubric

`rustgrade audit` computes this grade (`rustgrade-core/src/score.rs`); the
agent only applies it by hand when the CLI is unavailable, and uses it to say
what judgment findings would do to the grade.

## Inputs

1. Scanner: unsuppressed `findings` from `rustgrade scan`.
2. `cargo clippy --all-targets -- -W clippy::pedantic`: each warning counts as
   a `low` finding. Deny-by-default lints (the `correctness` group) →
   Correctness & errors; the `perf` group → Performance; everything else →
   Ownership & API. A lint reported for several targets at one location counts
   once.
3. `cargo test`: pass / fail.
4. Size: lines of production code (not blank, not comments, not tests:
   `tests/`, `benches/`, `examples/`, `tests.rs` files, `#[cfg(test)]` items).
5. Judgment findings from the review, each with a severity, marked
   `(judgment)`. They never change the CLI grade; the report states the grade
   they would produce.

## Categories and weights

| Category | Weight | Scanner categories | Applicable when |
|---|---|---|---|
| Correctness & errors | 25 | `errors`, `meta` | always |
| Unsafe | 20 | `unsafe` | the crate contains `unsafe` (blocks, fns, impls, traits, `extern` blocks) |
| Concurrency | 20 | `concurrency` | async code, threads, locks, channels, atomics or a concurrency crate |
| Performance | 15 | `performance` | always |
| Ownership & API | 20 | `ownership`, `api-design` | always |

A category that is **not applicable** is shown as `n/a` and **left out of the
average**. Counting it as 100 would let empty categories inflate the grade.

## Category score

Start at 100 and subtract per finding, **per 1000 lines of code**:

| Severity | Deduction |
|---|---|
| critical | 40 |
| high | 15 |
| medium | 5 |
| low | 2 |
| info | 0 |

- **Per 1000 lines:** a rule's total deduction is divided by the crate's size
  in thousands of lines, with a minimum of 1. A crate under 1000 lines is
  graded on raw counts; ten findings in 50 000 lines weigh like one in 5 000.
- One rule can deduct at most **30** from a category (after the division), so
  a single noisy rule cannot sink a grade.
- Floor at 0.
- `meta` findings (`invalid-suppression`, `unused-suppression`) are `low` and
  land in Correctness & errors: stale annotations rot.

## Overall score and grade

Overall = weighted average of the **applicable** category scores, rounded.

| Score | Grade |
|---|---|
| 90–100 | A |
| 80–89 | B |
| 70–79 | C |
| 60–69 | D |
| < 60 | F |

**Caps**, applied after the average (the lowest one wins):

| Condition | Grade at most |
|---|---|
| at least 1 `high` finding per 1000 lines | B (89) |
| at least 2 `high` findings per 1000 lines | C (79) |
| any `critical` finding, whatever the size | D (69) |
| failing tests | D (69) |
| code does not compile | F: stop the audit and report the error |

High caps use the same size rule, with a minimum of 1000 lines: in a crate
under 1000 lines one high caps at B and two at C. A small crate with one panic
on untrusted input is not an A, however clean the rest is; a 40 000-line crate
with a handful of unexplained unwraps can be.

`critical` is reserved for findings that are likely unsound or exploitable
(none of the current scanner rules; judgment findings can be). A missing
`// SAFETY:` comment is `medium`: a review gap, not proof of a bug.

## Calibration

`evals/real-world/` runs the scanner over popular crates (tokio, serde, regex,
syn, ...) and records the grades in `RESULTS.md`. A change to rules or rubric
that moves those grades must say why in the CHANGELOG.

## Example

`evals/fixtures/unwrap-daemon`: one high (`.unwrap()` on request input), one
medium (`.expect()` on request input). No `unsafe`, no concurrency. Under 1000
lines, so counts are used as they are.

| Category | Score |
|---|---|
| Correctness & errors | 100 − 15 − 5 = 80 |
| Unsafe | n/a |
| Concurrency | n/a |
| Performance | 100 |
| Ownership & API | 100 |

Average: (80·25 + 100·15 + 100·20) / 60 = 91.7 → 92. One high caps it at
**B (89)**.

## Report

```
## rustgrade audit: B (89), capped by 1 high finding

| Category | Score | Findings |
|---|---|---|
| Correctness & errors | 80 | 1 high, 1 medium |
| Unsafe | n/a | no unsafe code |
| Concurrency | n/a | no threads or async |
| Performance | 100 | — |
| Ownership & API | 100 | — |

Tests: 1 passed. Clippy pedantic: 0 warnings.

### Top 3 fixes
1. ...

### All findings
...
```

- State the cap in the title when one applies.
- **Top 3 fixes** are ranked by grade impact per unit of effort, each with the
  concrete change.
- Honest grading: never round up, never pad with nits, never hide a cap.
  If the crate is excellent, an A with an empty findings list is the report.
- Suppressed findings don't count, but mention any suppression whose reason
  doesn't hold up.
