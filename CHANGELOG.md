# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/). Until 1.0, the JSON schema, rule IDs
and rubric may change between minor versions; each change is listed here.

The Claude Code plugin's version (`.claude-plugin/plugin.json`) is bumped on
every change to `skills/` or `commands/`: Claude Code only updates an
installed plugin when that version changes.

## [Unreleased]

## [0.3.0] - 2026-10-29

Calibrated on real code: rules and rubric tuned against 15 popular crates.

### Added

- `evals/real-world/`: rustgrade over 15 popular crates (tokio, serde, regex,
  syn, ...), pinned in `crates.txt`; `run.py` downloads them and writes
  `RESULTS.md` (findings by severity, per 1k lines, scanner-only grade).

### Changed

- **Rubric v2: graded per 1000 lines of code.** Deductions are divided by the
  crate's production code size (no blanks, comments or tests; minimum 1000
  lines), and the high caps are densities: at least 1 high per 1000 lines caps
  at B, 2 at C. Crates under 1000 lines grade exactly as before. `audit`
  reports `code_lines` and `highs_per_kloc`.
- `undocumented-unsafe` is `medium`, not `critical`: a missing comment is a
  review gap, not proof of unsoundness (clippy keeps the lint opt-in).
  `critical` is left for findings that are likely unsound.
- `unwrap-outside-tests`: `low` when a comment at the end of the line or right
  above it says why it cannot fail ("this unwrap is OK since...", "invariant:",
  "checked above"), and for unwrapping a lock result (`m.lock().unwrap()`,
  `cvar.wait(g).unwrap()`), which only fails if another thread panicked.
- On the 15 real-world crates: from 13 D and 2 C to 5 A, 5 B and 5 C. The Cs
  (regex-automata, rand, crossbeam-channel, parking_lot, ignore) have 2.4–5
  unexplained unwraps per 1000 lines.

### Fixed

- `undocumented-unsafe`: `// Safety:` and `// safety:` count, like clippy
  (only `SAFETY:` did). Blocks inside an `unsafe fn` are covered by its
  `/// # Safety` section, or when the block is the whole body (the wrapper
  edition 2024 requires). In test code the finding is `low`, not `critical`.
  Together: 39% fewer critical findings on the real-world crates.
- `tests.rs` / `test.rs` files count as test code for every rule.

## [0.2.0] - 2026-10-29

First release on crates.io (`rustgrade`, `rustgrade-core`).

### Added

- `rustgrade install --agent <claude|codex|cursor|all>`: writes the skill (and,
  for Claude Code, `/audit` and `/hot-path`) into each agent's directories,
  per project or `--scope user`. Paths come from `targets/targets.toml`
  (Claude Code `.claude/`, Codex and Cursor `.agents/skills/`). `--dry-run`,
  `--force`, `--dir`.
- Agents: Gemini CLI, GitHub Copilot (VS Code) and Windsurf / Devin Desktop,
  next to Claude Code, Codex and Cursor. `.agents/skills/` serves the skill to
  Codex, Cursor, Gemini, Copilot and Windsurf with one copy.
- Commands are translated from `commands/*.md` into each agent's format:
  Claude Code (as-is), Cursor and Windsurf (plain Markdown), Gemini CLI (TOML
  with `{{args}}`), Copilot (`.prompt.md`, agent mode). Per-scope command
  directories in `targets.toml` (`user_commands_dir`).
- Agent detection also checks for the agent's executable on PATH
  (`detect_commands`). Gemini CLI and Windsurf are detected only that way:
  `~/.gemini` and `~/.codeium/windsurf` are also created by Antigravity, Gemini
  Code Assist and the Codeium extension.
- `rustgrade uninstall`: removes exactly what `install` wrote, tracked with
  hashes in `~/.rustgrade/manifest.json`; never touches files you created or
  edited, keeps files another agent still uses, and prunes the directories
  it leaves empty.
- `rustgrade doctor`: cargo and clippy versions, detected agents, and each
  install's status (up to date, outdated, edited, missing).
- `rustgrade audit [path]`: scanner + `cargo clippy` (pedantic) + `cargo test`,
  graded A–F with the rubric. Areas the code doesn't use (unsafe,
  concurrency) are detected and left out of the average; a crate that does not
  compile is an F. `--format text|json`, `--no-clippy`, `--no-tests`.
- `--fail-on <grade>` for `audit` (fails on that grade or worse), next to
  `--fail-on <severity>`.
- `/rustgrade:audit` uses the CLI grade as authoritative and reports judgment
  findings next to it.
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
- `--format sarif` (SARIF 2.1.0) for `scan` and `audit`, for GitHub code
  scanning. Every rule is a descriptor with its `--explain` text as help;
  critical/high map to `error`, medium to `warning`, low/info to `note`.
  Inline suppressions are left out (code scanning opens alerts even for SARIF
  `suppressions`); they stay in the JSON output. File
  URIs include the scan path, so they resolve from the repo root; `audit` puts
  the grade, score and cap in the run's `properties`.

- `.github/workflows/rustgrade.yml`: rustgrade audits itself on every push and
  PR and uploads the SARIF to code scanning.

### Changed

- Ready for crates.io: the skill, commands and `targets.toml` that `install`
  embeds are mirrored in `crates/rustgrade-cli/assets/` (a test keeps the copy
  in sync; `UPDATE_ASSETS=1` regenerates it), plus crate metadata.
- The `mcp` feature is gone until the adapter exists (v0.6);
  `rustgrade-mcp` is not published.

### Fixed

- Scanning a single file printed an empty path (`:6:43:`); it now prints the
  path as given.
