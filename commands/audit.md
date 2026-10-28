---
description: Full graded Rust review of a crate (scanner + clippy + tests, A–F)
argument-hint: "[path]"
---

Run a full rustgrade audit of the Rust crate at `$ARGUMENTS` (current directory
if empty). Use the rustgrade skill: its SKILL.md for the workflow and report
format, and `references/rubric.md` for grading.

Run every shell command **one at a time**, never chained or in parallel: cargo
holds file locks, and on Windows a concurrent build fails with `os error 32`.
If a command fails with `os error 32` or "file is being used by another
process", wait a moment and retry it once; it is not a compile error. Point
cargo at the crate with `--manifest-path <path>/Cargo.toml`.

1. **Scanner.** Run `rustgrade --version`. If installed, run
   `rustgrade scan <path> --format json` and keep the unsuppressed `findings`.
   If not installed, review manually as SKILL.md describes and say in the
   report that the grade comes from manual review.
2. **Clippy.** Run
   `cargo clippy --manifest-path <path>/Cargo.toml --all-targets -- -W clippy::pedantic`
   and map each warning to a category as the rubric says.
3. **Tests.** Run `cargo test --manifest-path <path>/Cargo.toml`. Record the
   pass/fail counts. If the crate does not compile, stop: grade F and report
   the compiler error.
4. **Applicable categories.** Unsafe applies only if the crate contains
   `unsafe`; Concurrency only if it uses threads, async, channels or locks.
   Check with a quick search. Non-applicable categories are `n/a` and left
   out of the average.
5. **Judgment pass.** Read the references for every category with findings,
   then review what the scanner cannot see (API shape, error taxonomy, lock
   granularity, algorithmic complexity, missing tests around risky code). Add
   those as findings with a severity, marked `(judgment)`. Use
   `rustgrade rules --explain <id>` if a scanner rule's rationale is unclear.
6. **Verify** heuristic findings against the code before counting them (see
   Limitations in SKILL.md). Drop false positives and suggest a suppression
   with a reason for each.
7. **Grade** with `references/rubric.md`: category scores, weighted average of
   applicable categories, then caps (1 high → B, 2+ high → C, critical or
   failing tests → D). Show the arithmetic only if asked.
8. **Report** in the rubric's format: grade (and the cap, if one applied),
   category table, test and clippy summary, top 3 fixes, then every finding in
   the SKILL.md line format.

Do not change any code during the audit. End by offering to apply the top 3
fixes.
