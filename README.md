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

```bash
cargo install rustgrade            # or: cargo binstall rustgrade

rustgrade install --agent all      # detect installed agents and install the skill
rustgrade scan                     # deterministic scan of the current crate
rustgrade audit                    # scan + clippy + tests → A–F grade
```

Inside an agent:

```
/audit            full graded review
/hot-path <fn>    optimize a critical path, with a benchmark
```

---

## The three layers

### 1. Skill (works in any agent)

`SKILL.md` + `references/` follow the open [Agent Skills](https://agentskills.io)
format, so the same files load in Claude Code, Cursor, Codex CLI, Gemini CLI, Copilot
and others. The skill auto-activates when the agent writes or reviews Rust.

The skill tells the agent to ground every review in the scanner:

1. Run `rustgrade scan --format json` (or `audit` for a grade).
2. Read only the references matching the categories that came back.
3. Use judgment for what a scanner can't see: design, API shape, whether a clone is
   actually justified.
4. If `rustgrade` isn't installed, fall back to manual review with the same rules.

### 2. CLI (`rustgrade`)

Rust binary, no network, no LLM. It parses the crate with `syn`, runs rules, and
merges results with `cargo clippy` and `cargo test`.

| Command | What it does |
|---|---|
| `rustgrade scan [path]` | Run the rule engine; prints findings by severity |
| `rustgrade audit [path]` | `scan` + clippy pedantic + tests → category scores and A–F grade |
| `rustgrade install --agent <name\|all>` | Install the skill and commands for one or every agent |
| `rustgrade uninstall --agent <name\|all>` | Remove what `install` wrote (tracked in a manifest) |
| `rustgrade doctor` | Show detected agents, installed versions, toolchain, clippy availability |
| `rustgrade mcp` | Start the MCP server on stdio |
| `rustgrade rules [--explain <id>]` | List rules, or explain one with before/after code |
| `rustgrade init` | Write a `rustgrade.toml` with defaults for the current crate |

Global flags: `--format text|json|sarif|markdown`, `--min-severity`, `--fail-on <grade|severity>`,
`--hot <glob>` (mark paths as hot so performance rules weigh more), `--scope project|user`
for `install`.

Exit codes: `0` passed, `1` findings above `--fail-on`, `2` tool error.

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
| Claude Code | plugin + `skills/` | `/audit`, `/hot-path` | ✅ | ✅ | `CLAUDE.md` snippet |
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

```yaml
# .github/workflows/rustgrade.yml
- uses: dtolnay/rust-toolchain@stable
- run: cargo install rustgrade
- run: rustgrade audit --format sarif --fail-on D > rustgrade.sarif
- uses: github/codeql-action/upload-sarif@v3
  with: { sarif_file: rustgrade.sarif }
```

Findings appear as code-scanning annotations on the PR.

---

## Structure

✅ ships in v0.1 · 🔜 planned

```
rustgrade/
├── Cargo.toml                    🔜 workspace
│
├── crates/
│   ├── rustgrade-core/           🔜 rule engine (library, no I/O)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── finding.rs        🔜 Finding, Severity, Category, Span
│   │       ├── score.rs          🔜 rubric weights, caps, letter grade
│   │       ├── config.rs         🔜 rustgrade.toml (allow/deny, hot paths)
│   │       └── rules/            🔜 one file per rule, each with tests
│   │           ├── mod.rs        🔜 Rule trait + registry
│   │           ├── clone_in_loop.rs
│   │           ├── unwrap_outside_tests.rs
│   │           ├── undocumented_unsafe.rs
│   │           ├── lock_across_await.rs
│   │           ├── alloc_in_loop.rs
│   │           ├── ref_owned_param.rs     (&String, &Vec<T>)
│   │           ├── anyhow_in_lib_api.rs
│   │           └── unbounded_channel.rs
│   │
│   ├── rustgrade-cli/            🔜 the `rustgrade` binary (clap)
│   │   └── src/
│   │       ├── main.rs
│   │       ├── cmd/              🔜 scan, audit, install, uninstall, doctor, rules, init, mcp
│   │       ├── output/           🔜 text, json, sarif, markdown renderers
│   │       ├── cargo.rs          🔜 clippy/test runners, Cargo.toml metadata
│   │       └── agents/           🔜 detection + installers driven by targets.toml
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
│   ├── audit.md                  ✅ /audit
│   ├── hot-path.md               ✅ /hot-path
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
├── evals/                        🔜 proof that the plugin improves output
│   ├── fixtures/                 🔜 small crates with known anti-patterns
│   │   ├── clone-heavy/
│   │   ├── unwrap-daemon/
│   │   └── mutex-everywhere/
│   └── expected/                 🔜 findings each fixture must produce (snapshot tests)
│
├── examples/                     🔜 before/after diffs for README and launch post
├── .github/workflows/            🔜 ci.yml, release.yml (cargo-dist binaries)
├── README.md                     ✅
├── CHANGELOG.md                  🔜
└── LICENSE                       🔜 MIT
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
- **Honest grading.** Critical findings cap the grade; reports are never padded with nits.
- **Safe installs.** Every file `install` writes is tracked, marker-delimited and
  reversible.

---

## Roadmap

1. **v0.1:** skill, references, `/audit`, `/hot-path`, Claude Code plugin ✅
2. **v0.2:** `rustgrade-core` with 8 rules, `rustgrade scan`, versioned JSON output,
   skill updated to drive the CLI, eval fixtures
3. **v0.3:** `rustgrade install` for Claude Code, Cursor, Codex; `targets.toml`; `doctor`
4. **v0.4:** `audit` with grading; SARIF + GitHub Action
5. **v0.5:** Gemini CLI, Copilot, Windsurf targets; `/unsafe-review`, `/errors`
6. **v0.6:** optional `rustgrade mcp` adapter (`--features mcp`) for terminal-less clients
7. **v1.0:** stable rule IDs, JSON schema and rubric; published evals (skill-only vs
   skill + CLI vs MCP); cargo-dist releases; npm wrapper

## License

MIT
