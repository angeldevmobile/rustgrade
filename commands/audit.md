---
description: Full graded Rust review of the current crate (scanner + clippy + tests, A–F)
argument-hint: "[path]"
---

Run a full rustgrade audit of the Rust crate at `$ARGUMENTS` (current directory if empty).
Use the rustgrade skill: its SKILL.md for the workflow and report format, and
`references/rubric.md` for grading.

1. **Scanner.** Run `rustgrade --version`. If installed, run
   `rustgrade scan $ARGUMENTS --format json` and keep the unsuppressed
   `findings`. If not installed, review manually as SKILL.md describes and
   state in the report that the grade is from manual review.
2. **Clippy.** Run `cargo clippy --all-targets -- -W clippy::pedantic` and map
   each warning to a category as the rubric says.
3. **Tests.** Run `cargo test`. Record pass/fail counts. If the crate does not
   compile, stop: grade F and report the compiler error.
4. **Judgment pass.** Read the references for every category with findings,
   then review what the scanner cannot see (API shape, error taxonomy, lock
   granularity, algorithmic complexity, missing tests around risky code). Add
   those as findings with a severity, marked `(judgment)`.
5. **Verify** heuristic findings against the code before counting them (see
   Limitations in SKILL.md). Drop false positives and suggest a suppression
   with a reason for each.
6. **Grade** with `references/rubric.md`: category scores, weighted average,
   caps. Show the arithmetic only if asked.
7. **Report** in the rubric's format: grade, category table, test and clippy
   summary, top 3 fixes, then every finding in the SKILL.md line format.

Do not change any code during the audit. End by offering to apply the top 3
fixes.
