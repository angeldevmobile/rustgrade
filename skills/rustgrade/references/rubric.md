# Grading rubric

Used by `/audit` to turn findings into category scores and an A–F grade. The
same rubric will drive `rustgrade audit` (roadmap v0.4); until then the agent
applies it by hand to the scanner's JSON.

## Inputs

1. `rustgrade scan --format json`: unsuppressed `findings` only.
2. `cargo clippy --all-targets -- -W clippy::pedantic`: each warning counts as
   a `low` finding in the closest category (`perf` lints → performance,
   `correctness` / `suspicious` → errors, the rest → api-design).
3. `cargo test`: pass / fail.

## Categories and weights

| Category | Weight | Scanner categories |
|---|---|---|
| Correctness & errors | 25 | `errors` |
| Unsafe | 20 | `unsafe` |
| Concurrency | 20 | `concurrency` |
| Performance | 15 | `performance` |
| Ownership & API | 20 | `ownership`, `api-design` |

A category with no applicable code (no `unsafe`, no threads or async) scores
100.

## Category score

Start at 100 and subtract per finding:

| Severity | Deduction |
|---|---|
| critical | 40 |
| high | 15 |
| medium | 5 |
| low | 2 |
| info | 0 |

- One rule can deduct at most **30** from a category, so a single noisy rule
  cannot sink a grade.
- Floor at 0.
- `meta` findings (`invalid-suppression`, `unused-suppression`) deduct 2 each
  from Correctness & errors: stale annotations rot.

## Overall score and grade

Overall = weighted average of the category scores, rounded.

| Score | Grade |
|---|---|
| 90–100 | A |
| 80–89 | B |
| 70–79 | C |
| 60–69 | D |
| < 60 | F |

**Caps** (applied after the average):

- Any `critical` finding: grade at most **D**.
- Failing tests: grade at most **D**.
- Code does not compile: **F**, stop the audit and report the error.

## Report

```
## rustgrade audit: B (84)

| Category | Score | Findings |
|---|---|---|
| Correctness & errors | 70 | 2 high |
| Unsafe | 100 | — |
| Concurrency | 85 | 1 high |
| Performance | 90 | 2 medium |
| Ownership & API | 92 | 4 low |

Tests: 48 passed. Clippy pedantic: 3 warnings.

### Top 3 fixes
1. ...
2. ...
3. ...

### All findings
...
```

- **Top 3 fixes** are ranked by grade impact per unit of effort, each with
  the concrete change.
- Honest grading: never round up, never pad with nits, never hide a cap.
  If the crate is excellent, an A with an empty findings list is the report.
- Suppressed findings don't count, but mention any suppression whose reason
  doesn't hold up.
