---
description: Full graded Rust review of a crate (scanner + clippy + tests, A–F)
argument-hint: "[path]"
---

Run a full rustgrade audit of the Rust crate at `$ARGUMENTS` (current directory
if empty). Use the rustgrade skill: its SKILL.md for the workflow and report
format, and `references/rubric.md` for how grades are computed.

Run shell commands **one at a time**, never chained or in parallel: cargo holds
file locks, and on Windows a concurrent build fails with `os error 32`.

1. **Deterministic grade.** Run `rustgrade --version`, then
   `rustgrade audit <path> --format json`. It runs the scanner, clippy pedantic
   and the tests, and returns `grade`, `score`, `cap`, per-area scores
   (`null` = not applicable), `tests`, `clippy` and the full `scan` report.
   This grade is authoritative: do not recompute it.
   - Exit code `2` is a tool error (no `Cargo.toml`, cargo missing): report
     stderr.
   - `"cap": "does-not-compile"`: stop, report grade F and the `error` field.
   - If `rustgrade audit` is not available (older CLI), run
     `rustgrade scan <path> --format json`, `cargo clippy` and `cargo test`
     yourself and grade by hand with the rubric; say so in the report.
   - If `rustgrade` is not installed at all, review manually as SKILL.md
     describes and say the grade comes from manual review.
2. **Judgment pass.** Read the references for every area with findings, then
   review what the tools cannot see: API shape, error taxonomy, lock
   granularity, algorithmic complexity, missing tests around risky code. Record
   each as a finding with a severity, marked `(judgment)`. Use
   `rustgrade rules --explain <id>` if a rule's rationale is unclear.
3. **Verify** the scanner's heuristic findings against the code (see
   Limitations in SKILL.md). For a false positive, propose a suppression with a
   reason; don't count it in your summary.
4. **Report** in the rubric's format:
   - Title: the CLI grade and its cap, e.g. `B (89), capped by 1 high finding`.
   - The area table from `areas`, the test and clippy summary.
   - If any judgment finding is high or critical, add one line:
     `With judgment findings: <grade>` using the rubric's caps. Never change
     the CLI grade itself.
   - Top 3 fixes, then every finding (scanner and judgment) in the SKILL.md
     line format.

Do not change any code during the audit. End by offering to apply the top 3
fixes.
