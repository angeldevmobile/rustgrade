# rustgrade

**Systems-level Rust review for AI coding agents and for CI.**

AI-written Rust compiles, but it often reads like tutorial Rust: `.clone()` to silence
the borrow checker, `.unwrap()` in request handlers, `Arc<Mutex<_>>` as the default
concurrency tool, allocations in every loop iteration. rustgrade fixes this with two
core pieces and one optional adapter:

1. **Skill**: teaches any coding agent the defaults a senior systems engineer would choose,
   and how to drive the CLI.
2. **CLI**: a fast, deterministic scanner written in Rust that works without an LLM,
   locally or in CI. Agents call it from the terminal and reason over its JSON output.
3. **MCP server (optional)**: the same scanner exposed as MCP tools, for clients that
   have no terminal access.

One source of truth (`skills/rustgrade/`) is installed into every supported agent by
the CLI.

## Why CLI + skill first, MCP second

rustgrade runs locally, on the user's own code, with no auth and no remote service.
That is the case where **CLI + skill** beats an MCP server:

- **Cheaper context.** An MCP server loads every tool schema into the agent's context
  up front. A skill is loaded only when the task needs it, and the CLI costs nothing
  until it runs.
- **Fewer, cheaper calls.** In Arize's eval comparing an MCP server against CLI +
  skills on the same tasks, accuracy was about equal, but on the hardest tasks MCP cost
  over 6× more, took about 5× longer and made more than twice as many tool calls.
- **Works everywhere.** Any agent with a shell can run `rustgrade scan --format json`.
  No per-client server configuration.
- **Composable.** Output pipes into `jq`, `grep`, CI and scripts like any Unix tool.

MCP still wins for remote or proprietary services, OAuth, enterprise access control and
clients without a terminal. So rustgrade ships it as an **optional adapter** behind a
Cargo feature, not as the primary integration.

**Transport-agnostic by design.** `rustgrade-core` knows nothing about how it is
called. The CLI and the MCP server are thin layers over it, so if MCP is superseded,
one crate is deleted and nothing else changes. A new protocol is one new adapter crate.

---

## Quick start

**Available now (v0.2):**

```bash
# CLI (not on crates.io yet)
cargo install --git https://github.com/angeldevmobile/rustgrade rustgrade

rustgrade scan                     # deterministic scan of the current crate
rustgrade scan --format json       # what agents read
rustgrade audit                    # scan + clippy + tests → A–F grade
rustgrade rules --explain lock-across-await
```

Claude Code plugin (skill + commands), inside a Claude Code session:

```
/plugin marketplace add angeldevmobile/rustgrade
/plugin install rustgrade@rustgrade
```

```
/rustgrade:audit [path]       full graded review (scanner + clippy + tests → A–F)
/rustgrade:hot-path <fn>      optimize a critical path, proven with a benchmark
```

Plugin commands are namespaced by Claude Code. Once `rustgrade install` copies
them into a project's `.claude/commands/`, they are plain `/audit` and `/hot-path`.

**Planned:** `cargo install rustgrade` from crates.io and `rustgrade install --agent all`
(v0.3).

---

## The three layers

### 1. Skill (works in any agent)

`SKILL.md` + `references/` follow the open [Agent Skills](https://agentskills.io)
format, so the same files load in Claude Code, Cursor, Codex CLI, Gemini CLI, Copilot
and others. The skill auto-activates when the agent writes or reviews Rust.

The skill tells the agent to ground every review in the scanner:

1. Run `rustgrade scan --format json`.
2. Read only the references matching the categories that came back.
3. Use judgment for what a scanner can't see: design, API shape, whether a clone is
   actually justified.
4. If `rustgrade` isn't installed, fall back to manual review with the same rules.

### 2. CLI (`rustgrade`)

Rust binary, no network, no LLM. It parses each file with `syn` and runs the rules;
`audit` merges the results with `cargo clippy` and `cargo test` and grades them.

| Command | What it does | Status |
|---|---|---|
| `rustgrade scan [path]` | Run the rule engine; prints findings by severity | ✅ |
| `rustgrade rules [--explain <id>]` | List rules, or explain one with before/after code | ✅ |
| `rustgrade audit [path]` | `scan` + clippy pedantic + tests → category scores and A–F grade | ✅ |
| `rustgrade install --agent <name\|all>` | Install the skill and commands for one or every agent | 🔜 v0.3 |
| `rustgrade uninstall --agent <name\|all>` | Remove what `install` wrote (tracked in a manifest) | 🔜 v0.3 |
| `rustgrade doctor` | Show detected agents, installed versions, toolchain, clippy availability | 🔜 v0.3 |
| `rustgrade init` | Write a `rustgrade.toml` with defaults for the current crate | 🔜 |
| `rustgrade mcp` | Start the MCP server on stdio | 🔜 v0.6 |

Flags today: `--format text|json` (`sarif|markdown` 🔜), `--min-severity <sev>`,
`--fail-on <severity>` (any command) or `--fail-on <grade>` (`audit`: fails on that
grade or worse), `audit --no-clippy --no-tests`. Planned: `--hot <glob>`,
`--scope project|user`.

Exit codes: `0` passed, `1` threshold reached (`--fail-on`), `2` tool error.

`rustgrade audit` example:

```
rustgrade audit: B (89), capped by 1 high finding

  Correctness & errors    80  1 high, 1 medium
  Unsafe                 n/a  no unsafe code
  Concurrency            n/a  no async, threads, locks or channels
  Performance            100  -
  Ownership & API        100  -

  Tests    1 passed, 0 failed
  Clippy   0 warning(s), pedantic
  Scanner  2 finding(s), 0 suppressed, 1 file(s)
```

Areas that don't apply are left out of the average; one high finding caps the grade
at B, two at C, a critical one or failing tests at D, and a crate that does not
compile is an F. The full rubric is in
[skills/rustgrade/references/rubric.md](skills/rustgrade/references/rubric.md).

#### Rules

| ID | Severity | Flags |
|---|---|---|
| `undocumented-unsafe` | critical | `unsafe` block or impl without a `// SAFETY:` comment |
| `unwrap-outside-tests` | high (`expect`: medium) | `.unwrap()` / `.expect()` outside tests |
| `lock-across-await` | high | std / parking_lot guard alive at an `.await` |
| `unbounded-channel` | medium | `unbounded_channel()`, `unbounded()`, `std::sync::mpsc::channel()` |
| `anyhow-in-lib-api` | medium | public library fn returning `anyhow` errors |
| `clone-in-loop` | medium (high in hot paths) | `.clone()` / `.to_owned()` per iteration |
| `alloc-in-loop` | medium (high in hot paths) | `Vec::new`, `format!`, `.to_string()`, ... per iteration |
| `ref-owned-param` | low | `&String`, `&Vec<T>`, `&Box<T>`, `&PathBuf` parameters |

The rules read syntax, not types, so they are heuristics: `clone-in-loop` can't tell a
cheap `Arc` clone from a deep one, and macro bodies are not analyzed.
`rustgrade rules --explain <id>` documents each rule's rationale and limits.

#### Configuration and suppressions

`rustgrade.toml` at the scan root:

```toml
allow = ["ref-owned-param"]      # disable rules project-wide
exclude = ["generated/"]         # gitignore-style globs, on top of .gitignore
hot = ["src/engine/"]            # performance findings here are raised to high
min_severity = "low"
```

A single finding is silenced inline, with a mandatory reason:

```rust
// rustgrade: allow(unbounded-channel) shutdown signal, at most one message per worker
let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
```

Suppressed findings stay in the JSON (`"suppressed"`, with their reason) so CI can
audit them. Malformed or stale suppressions are reported as `invalid-suppression` /
`unused-suppression`.

### 3. MCP server (`rustgrade mcp`, optional)

Built only with `cargo install rustgrade --features mcp`. Use it for clients without
terminal access (some desktop chat apps) or teams that standardize on MCP. Agents with a
shell don't need it: the CLI path is cheaper and is what the skill uses by default.

Exposed tools:

| Tool | Returns |
|---|---|
| `scan` | Findings for a path, as structured JSON |
| `grade` | Category scores, letter grade and top 3 fixes |
| `explain_rule` | Rule rationale with before/after code |
| `hot_spots` | Functions with the most per-iteration allocations / clones |

`rustgrade install --with-mcp` also registers the server in each agent's MCP config.
Without the flag, `install` only writes the skill and commands.

---

## Supported agents

| Agent | Skill | Commands | Uses CLI | MCP (optional) | Fallback when skills are unavailable |
|---|---|---|---|---|---|
| Claude Code | plugin + `skills/` | `/rustgrade:audit`, `/rustgrade:hot-path` | ✅ | ✅ | `CLAUDE.md` snippet |
| Cursor | `SKILL.md` | `.cursor/commands/` | ✅ | ✅ | `.cursor/rules/rustgrade.mdc` |
| Codex CLI | `.codex/skills/` / `~/.codex/skills/` | prompts | ✅ | ✅ | `AGENTS.md` snippet |
| Gemini CLI | extension | `.gemini/commands/*.toml` | ✅ | ✅ | `GEMINI.md` snippet |
| GitHub Copilot (VS Code) | skills | `.github/prompts/` | ✅ | ✅ | `.github/copilot-instructions.md` |
| Windsurf | rules | workflows | ✅ | ✅ | `.windsurf/rules/rustgrade.md` |
| Any AGENTS.md tool | n/a | n/a | ✅ if it has a shell | if supported | `AGENTS.md` snippet |
| Chat apps without a terminal | n/a | n/a | ❌ | ✅ primary path | n/a |

Install paths live in one file, `targets/targets.toml`, so supporting a new agent or
fixing a changed path doesn't require touching Rust code. Fallback snippets are
inserted between `<!-- rustgrade:start -->` / `<!-- rustgrade:end -->` markers, so
`uninstall` and upgrades never clobber the user's own instructions.

---

## CI

Gate on the grade (fails on C or worse):

```yaml
# .github/workflows/rustgrade.yml
- uses: dtolnay/rust-toolchain@stable
  with: { components: clippy }
- run: cargo install --git https://github.com/angeldevmobile/rustgrade rustgrade
- run: rustgrade audit --fail-on C
```

Or, cheaper, only on scanner findings: `rustgrade scan --fail-on high`.

Planned: SARIF output, so findings appear as code-scanning annotations on the PR.

```yaml
- run: rustgrade audit --format sarif --fail-on D > rustgrade.sarif
- uses: github/codeql-action/upload-sarif@v3
  with: { sarif_file: rustgrade.sarif }
```

---

## Structure

✅ shipped · 🔜 planned

```
rustgrade/
├── Cargo.toml                    ✅ workspace
├── rustgrade.toml                ✅ the repo's own scan config
│
├── crates/
│   ├── rustgrade-core/           ✅ rule engine (library, no I/O)
│   │   └── src/
│   │       ├── lib.rs            ✅ scan API, versioned Report
│   │       ├── finding.rs        ✅ Finding, Severity, Category, Span
│   │       ├── score.rs          ✅ rubric weights, caps, letter grade
│   │       ├── config.rs         ✅ rustgrade.toml (allow, exclude, hot paths)
│   │       ├── suppress.rs       ✅ inline `rustgrade: allow(...)` comments
│   │       ├── usage.rs          ✅ unsafe / concurrency detection (n/a areas)
│   │       └── rules/            ✅ one file per rule, each with tests
│   │           ├── mod.rs        ✅ Rule trait + registry
│   │           ├── common.rs     ✅ test-code detection, loop walker
│   │           ├── clone_in_loop.rs
│   │           ├── unwrap_outside_tests.rs
│   │           ├── undocumented_unsafe.rs
│   │           ├── lock_across_await.rs
│   │           ├── alloc_in_loop.rs
│   │           ├── ref_owned_param.rs     (&String, &Vec<T>)
│   │           ├── anyhow_in_lib_api.rs
│   │           └── unbounded_channel.rs
│   │
│   ├── rustgrade-cli/            ✅ the `rustgrade` binary (clap)
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── cmd/              ✅ scan, audit, rules · 🔜 install, uninstall, doctor, init
│   │   │   ├── output/           ✅ text, json · 🔜 sarif, markdown
│   │   │   ├── cargo.rs          ✅ clippy/test runners and output parsers
│   │   │   └── agents/           🔜 detection + installers driven by targets.toml
│   │   └── tests/                ✅ fixture snapshots, audit end to end
│   │
│   └── rustgrade-mcp/            🔜 optional MCP adapter over stdio (rmcp), behind `--features mcp`
│       └── src/
│           ├── lib.rs
│           └── tools.rs          🔜 scan, grade, explain_rule, hot_spots
│
├── skills/rustgrade/             ✅ single source of truth for every agent
│   ├── SKILL.md                  ✅ rules, workflow, finding format
│   └── references/
│       ├── ownership.md          ✅
│       ├── errors.md             ✅
│       ├── performance.md        ✅
│       ├── concurrency.md        ✅
│       ├── unsafe.md             ✅
│       ├── rubric.md             ✅
│       ├── api-design.md         🔜
│       └── testing.md            🔜
│
├── commands/                     agent-neutral command definitions
│   ├── audit.md                  ✅ /rustgrade:audit
│   ├── hot-path.md               ✅ /rustgrade:hot-path
│   ├── unsafe-review.md          🔜
│   ├── errors.md                 🔜
│   ├── concurrency.md            🔜
│   └── api.md                    🔜
│
├── targets/                      per-agent packaging, generated or templated
│   ├── targets.toml              🔜 install paths, formats, MCP config locations
│   ├── claude/                   ✅ .claude-plugin/ (plugin.json, marketplace.json)
│   ├── cursor/                   🔜 rustgrade.mdc template, command wrappers
│   ├── codex/                    🔜 AGENTS.md snippet
│   ├── gemini/                   🔜 gemini-extension.json, commands/*.toml
│   ├── copilot/                  🔜 copilot-instructions snippet, *.prompt.md
│   └── windsurf/                 🔜 rules + workflows
│
├── npm/                          🔜 optional `npx rustgrade` wrapper that downloads the binary
│
├── evals/                        proof that the plugin improves output
│   ├── fixtures/                 ✅ small crates with known anti-patterns
│   │   ├── clone-heavy/
│   │   ├── lib-api/
│   │   ├── unwrap-daemon/
│   │   └── mutex-everywhere/
│   └── expected/                 ✅ findings each fixture must produce (snapshot tests)
│
├── examples/                     🔜 before/after diffs for README and launch post
├── .github/workflows/            ✅ ci.yml · 🔜 release.yml (cargo-dist binaries)
├── README.md                     ✅
├── CHANGELOG.md                  ✅
└── LICENSE                       ✅ MIT
```

> In v0.1 the Claude packaging sits at the repo root as `.claude-plugin/`. It moves to
> `targets/claude/` once the CLI generates it.

### Design principles

- **One source, many targets.** Rules live once in `skills/` and `commands/`; the CLI
  translates them into each agent's format. Never edit generated files by hand.
- **Deterministic first, LLM second.** The CLI finds what can be found mechanically;
  the skill handles what needs judgment and tells the agent to run the CLI.
- **CLI + skill is the primary integration; protocols are adapters.** MCP is optional
  and replaceable. Supporting a new protocol means adding one thin crate.
- **Core is pure.** `rustgrade-core` does no I/O and knows nothing about transports, so
  it is easy to test, embed, and later compile to WASM.
- **Stable JSON contract.** `--format json` output is versioned (`"schema": 1`), because
  agents and CI depend on it the same way they would depend on an API.
- **Measure, don't assert.** Every performance recommendation comes with a way to
  verify it (criterion, flamegraph, dhat, cargo asm).
- **Honest grading.** High and critical findings cap the grade, and categories that
  don't apply are left out instead of counting as perfect; reports are never padded
  with nits.
- **Safe installs.** Every file `install` writes is tracked, marker-delimited and
  reversible.

---

## Roadmap

1. **v0.1:** skill, references, `/audit`, `/hot-path`, Claude Code plugin ✅
2. **v0.2:** `rustgrade-core` with 8 rules, `rustgrade scan`, versioned JSON output,
   inline suppressions, `rules --explain`, skill updated to drive the CLI, eval
   fixtures with snapshot tests ✅
3. **v0.3:** `rustgrade install` for Claude Code, Cursor, Codex; `targets.toml`; `doctor`
4. **v0.4:** `audit` with grading ✅; SARIF + GitHub Action
5. **v0.5:** Gemini CLI, Copilot, Windsurf targets; `/unsafe-review`, `/errors`
6. **v0.6:** optional `rustgrade mcp` adapter (`--features mcp`) for terminal-less clients
7. **v1.0:** stable rule IDs, JSON schema and rubric; published evals (skill-only vs
   skill + CLI vs MCP); cargo-dist releases; npm wrapper

## License

MIT
