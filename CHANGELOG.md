# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/). Until 1.0, the JSON schema, rule IDs
and rubric may change between minor versions; each change is listed here.

The Claude Code plugin's version (`.claude-plugin/plugin.json`) is bumped on
every change to `skills/` or `commands/`: Claude Code only updates an
installed plugin when that version changes.

## [Unreleased] - 0.2.0

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
- `rustgrade uninstall`: removes exactly what `install` wrote, tracked with
  hashes in `~/.rustgrade/manifest.json`; never touches files you created or
  edited, and keeps files another agent still uses.
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
