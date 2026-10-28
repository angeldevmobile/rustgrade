# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/). Until 1.0, the JSON schema, rule IDs
and rubric may change between minor versions; each change is listed here.

## [Unreleased] - 0.2.0

### Added

- `rustgrade-core`: pure rule engine (no I/O) with a versioned JSON report
  (`"schema": 1`, `findings` + `suppressed`).
- Eight rules: `undocumented-unsafe`, `unwrap-outside-tests`,
  `lock-across-await`, `unbounded-channel`, `anyhow-in-lib-api`,
  `clone-in-loop`, `alloc-in-loop`, `ref-owned-param`.
- `rustgrade scan` with `--format text|json`, `--min-severity`,
  `--fail-on <severity>` and exit codes 0 / 1 / 2.
- `rustgrade rules` and `rules --explain <id>` (rationale with before/after
  code; `--format json` supported).
- Inline suppressions `// rustgrade: allow(rule-id) reason` with a mandatory
  reason; `invalid-suppression` and `unused-suppression` meta findings.
- `rustgrade.toml`: `allow`, `exclude`, `hot`, `min_severity`.
- File discovery honors `.gitignore`, `.ignore` and global git excludes.
- Grading rubric in code (`score.rs`): weighted categories, per-rule deduction
  limit, `n/a` categories excluded from the average, caps for high, critical
  and failing tests.
- Eval fixtures with JSON snapshot tests (`UPDATE_EXPECTED=1` regenerates).
- Skill (`skills/rustgrade/`) with references for ownership, errors,
  performance, concurrency, unsafe and grading; `/rustgrade:audit` and
  `/rustgrade:hot-path` commands; Claude Code plugin and marketplace manifests.
- CI on Linux and Windows: fmt, clippy pedantic, tests, self-scan, MSRV 1.88.
