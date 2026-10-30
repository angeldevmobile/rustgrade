//! End-to-end tests for `install`, `uninstall` and `doctor`, in throwaway
//! directories with `RUSTGRADE_HOME` standing in for the home directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh home + project pair, removed when dropped.
struct Sandbox {
    base: PathBuf,
}

impl Sandbox {
    fn new(name: &str, agents: &[&str]) -> Self {
        let base =
            std::env::temp_dir().join(format!("rustgrade-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("proj")).expect("create project dir");
        for agent in agents {
            fs::create_dir_all(base.join("home").join(agent)).expect("create agent dir");
        }
        Self { base }
    }

    fn proj(&self, rel: &str) -> PathBuf {
        self.base.join("proj").join(rel)
    }

    /// Runs rustgrade in the project dir; returns (exit code, stdout + stderr).
    fn run(&self, args: &[&str]) -> (Option<i32>, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_rustgrade"))
            .args(args)
            .current_dir(self.base.join("proj"))
            .env("RUSTGRADE_HOME", self.base.join("home"))
            .output()
            .expect("run rustgrade");
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.code(), text)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn exists(p: &Path) -> bool {
    p.exists()
}

#[test]
fn installs_skill_and_commands_for_claude_and_is_idempotent() {
    let sb = Sandbox::new("claude", &[".claude"]);
    let (code, out) = sb.run(&["install", "--agent", "claude"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(exists(&sb.proj(".claude/skills/rustgrade/SKILL.md")));
    assert!(exists(
        &sb.proj(".claude/skills/rustgrade/references/rubric.md")
    ));
    assert!(exists(&sb.proj(".claude/commands/audit.md")));
    assert!(
        out.contains("created   .claude/skills/rustgrade/SKILL.md"),
        "paths use `/`: {out}"
    );
    assert!(
        exists(&sb.base.join("home/.rustgrade/manifest.json")),
        "manifest written"
    );

    let (code, out) = sb.run(&["install", "--agent", "claude"]);
    assert_eq!(code, Some(0));
    assert!(
        !out.contains("created"),
        "second install changes nothing: {out}"
    );

    let (_, doctor) = sb.run(&["doctor"]);
    assert!(doctor.contains("up to date (10 files)"), "{doctor}");
}

#[test]
fn never_overwrites_or_removes_a_file_it_did_not_write() {
    let sb = Sandbox::new("conflict", &[".claude"]);
    fs::create_dir_all(sb.proj(".claude/commands")).expect("mkdir");
    fs::write(sb.proj(".claude/commands/audit.md"), "my own audit command").expect("write");

    let (code, out) = sb.run(&["install", "--agent", "claude"]);
    assert_eq!(code, Some(1), "a kept file means a partial install: {out}");
    assert!(out.contains("kept      .claude/commands/audit.md"), "{out}");
    let mine = fs::read_to_string(sb.proj(".claude/commands/audit.md")).expect("read");
    assert_eq!(mine, "my own audit command");

    let (_, doctor) = sb.run(&["doctor"]);
    assert!(doctor.contains("1 file(s) not installed"), "{doctor}");

    sb.run(&["uninstall", "--agent", "claude"]);
    assert!(
        exists(&sb.proj(".claude/commands/audit.md")),
        "user file survives"
    );
    assert!(!exists(&sb.proj(".claude/skills")), "emptied dirs pruned");
    assert!(
        !exists(&sb.proj(".claude/skills/rustgrade")),
        "our files are gone"
    );

    let (code, _) = sb.run(&["install", "--agent", "claude", "--force"]);
    assert_eq!(code, Some(0));
    let replaced = fs::read_to_string(sb.proj(".claude/commands/audit.md")).expect("read");
    assert_ne!(replaced, "my own audit command");
}

#[test]
fn uninstall_keeps_files_edited_after_install() {
    let sb = Sandbox::new("edited", &[".claude"]);
    sb.run(&["install", "--agent", "claude"]);
    let skill = sb.proj(".claude/skills/rustgrade/SKILL.md");
    fs::write(&skill, "edited").expect("edit");

    let (code, out) = sb.run(&["uninstall", "--agent", "claude"]);
    assert_eq!(code, Some(1), "{out}");
    assert!(exists(&skill), "edited file kept");
    assert!(
        !exists(&sb.proj(".claude/commands/audit.md")),
        "unedited file removed"
    );
}

#[test]
fn codex_and_cursor_share_the_agents_skill_dir() {
    let sb = Sandbox::new("shared", &[".codex", ".cursor"]);
    let (code, out) = sb.run(&["install", "--agent", "all"]);
    assert_eq!(code, Some(0), "{out}");
    let skill = sb.proj(".agents/skills/rustgrade/SKILL.md");
    assert!(exists(&skill));
    assert!(
        !exists(&sb.proj(".claude")),
        "claude not detected, not installed"
    );

    assert!(
        out.contains("shared    .agents/skills/rustgrade/ (7 files, with Codex CLI)"),
        "the shared skill is one line: {out}"
    );

    let (_, out) = sb.run(&["uninstall", "--agent", "codex"]);
    assert!(exists(&skill), "still needed by cursor");
    assert!(
        out.contains("kept      .agents/skills/rustgrade/ (7 files, still used by Cursor)"),
        "{out}"
    );
    sb.run(&["uninstall", "--agent", "cursor"]);
    assert!(!exists(&skill), "removed with its last owner");
}

#[test]
fn dry_run_writes_nothing_and_errors_are_clear() {
    let sb = Sandbox::new("dry", &[".claude"]);
    let (code, out) = sb.run(&["install", "--agent", "claude", "--dry-run"]);
    assert_eq!(code, Some(0));
    assert!(out.contains("Dry run"), "{out}");
    assert!(!exists(&sb.proj(".claude")));
    assert!(!exists(&sb.base.join("home/.rustgrade")));

    let (code, out) = sb.run(&["install", "--agent", "vscode"]);
    assert_eq!(code, Some(2));
    assert!(out.contains("unknown agent `vscode`"), "{out}");

    let empty = Sandbox::new("none", &[]);
    let (code, out) = empty.run(&["install", "--agent", "all"]);
    assert_eq!(code, Some(2));
    assert!(out.contains("no supported agent detected"), "{out}");
}

#[test]
fn user_scope_installs_under_home() {
    let sb = Sandbox::new("user", &[".claude"]);
    let (code, out) = sb.run(&["install", "--agent", "claude", "--scope", "user"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(exists(
        &sb.base.join("home/.claude/skills/rustgrade/SKILL.md")
    ));
    assert!(!exists(&sb.proj(".claude")));

    let (code, _) = sb.run(&[
        "install", "--agent", "claude", "--scope", "user", "--dir", ".",
    ]);
    assert_eq!(code, Some(2), "--dir is for project scope");
}

#[test]
fn every_agent_gets_the_shared_skill_and_its_own_command_format() {
    // Explicit ids: detection depends on this machine's PATH.
    let sb = Sandbox::new("every", &[]);
    for agent in ["claude", "codex", "cursor", "gemini", "copilot", "windsurf"] {
        let (code, out) = sb.run(&["install", "--agent", agent]);
        assert_eq!(code, Some(0), "{agent}: {out}");
    }

    // One skill copy serves Codex, Cursor, Gemini, Copilot and Windsurf.
    assert!(exists(&sb.proj(".agents/skills/rustgrade/SKILL.md")));
    assert!(exists(&sb.proj(".claude/skills/rustgrade/SKILL.md")));

    let read = |rel: &str| fs::read_to_string(sb.proj(rel)).expect(rel);
    assert!(read(".claude/commands/audit.md").contains("$ARGUMENTS"));
    assert!(read(".cursor/commands/audit.md").starts_with("# "));
    assert!(read(".devin/workflows/audit.md").starts_with("# "));
    let gemini: toml::Table = toml::from_str(&read(".gemini/commands/audit.toml")).expect("TOML");
    assert!(
        gemini["prompt"]
            .as_str()
            .is_some_and(|p| p.contains("{{args}}"))
    );
    assert!(read(".github/prompts/audit.prompt.md").starts_with("---\ndescription: "));

    // Uninstalling everything leaves no rustgrade file behind.
    let (code, out) = sb.run(&["uninstall", "--agent", "all"]);
    assert_eq!(code, Some(0), "{out}");
    for rel in [
        ".agents/skills/rustgrade",
        ".gemini/commands/audit.toml",
        ".github/prompts/audit.prompt.md",
    ] {
        assert!(!exists(&sb.proj(rel)), "{rel} removed");
    }
    // ...and no empty directories it created.
    for dir in [
        ".agents", ".gemini", ".github", ".devin", ".cursor", ".claude",
    ] {
        assert!(!exists(&sb.proj(dir)), "{dir} pruned");
    }
    assert!(exists(&sb.proj("")), "the project itself stays");
}

#[test]
fn user_scope_skips_commands_where_the_agent_keeps_them_elsewhere() {
    let sb = Sandbox::new("user-copilot", &[".copilot"]);
    let (code, out) = sb.run(&["install", "--agent", "copilot", "--scope", "user"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(exists(
        &sb.base.join("home/.agents/skills/rustgrade/SKILL.md")
    ));
    assert!(
        !out.contains("prompt.md"),
        "Copilot user prompts live in VS Code's profile: {out}"
    );
}

#[test]
fn ambiguous_folders_do_not_count_as_installed_agents() {
    // Antigravity creates ~/.gemini and the Codeium extension ~/.codeium/windsurf;
    // neither means the Gemini CLI or Windsurf is installed.
    let sb = Sandbox::new("ambiguous", &[".codex", ".gemini", ".codeium/windsurf"]);
    let (code, out) = sb.run(&["install", "--agent", "all"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(
        exists(&sb.proj(".agents/skills/rustgrade/SKILL.md")),
        "codex installed"
    );
    assert!(
        !exists(&sb.proj(".gemini")),
        "no gemini without its CLI: {out}"
    );
    assert!(
        !exists(&sb.proj(".devin")),
        "no windsurf without its CLI: {out}"
    );
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).expect("readable")
}

#[test]
fn adds_a_section_to_claude_md_and_removes_only_that() {
    let sb = Sandbox::new("section", &[".claude"]);
    let claude_md = sb.proj("CLAUDE.md");
    let mine = "# My project\n\nUse tabs.\n";
    fs::write(&claude_md, mine).expect("write CLAUDE.md");

    let (code, out) = sb.run(&["install", "--agent", "claude"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains("added     CLAUDE.md section"), "{out}");
    let text = read(&claude_md);
    assert!(
        text.starts_with(mine),
        "user content first, untouched: {text}"
    );
    assert!(text.contains("<!-- rustgrade:begin"), "{text}");
    assert!(text.contains("`rustgrade` skill"), "{text}");

    let (_, again) = sb.run(&["install", "--agent", "claude"]);
    assert!(again.contains("unchanged CLAUDE.md section"), "{again}");
    assert_eq!(read(&claude_md), text, "idempotent");

    let (code, out) = sb.run(&["uninstall", "--agent", "claude"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains("removed   CLAUDE.md section"), "{out}");
    assert_eq!(read(&claude_md), mine, "back to the user's file");
}

#[test]
fn a_section_rustgrade_created_goes_with_its_file() {
    let sb = Sandbox::new("section-file", &[".claude"]);
    sb.run(&["install", "--agent", "claude"]);
    assert!(exists(&sb.proj("CLAUDE.md")));
    sb.run(&["uninstall", "--agent", "claude"]);
    assert!(!exists(&sb.proj("CLAUDE.md")), "nothing else was in it");
}

#[test]
fn an_edited_section_is_kept_unless_forced() {
    let sb = Sandbox::new("section-edit", &[".claude"]);
    sb.run(&["install", "--agent", "claude"]);
    let claude_md = sb.proj("CLAUDE.md");
    let edited = read(&claude_md).replace("Before you finish", "When asked");
    fs::write(&claude_md, &edited).expect("edit section");

    let (code, out) = sb.run(&["uninstall", "--agent", "claude"]);
    assert_eq!(code, Some(1), "{out}");
    assert_eq!(read(&claude_md), edited, "edited section kept");

    sb.run(&["install", "--agent", "claude"]);
    let (code, out) = sb.run(&["install", "--agent", "claude", "--force"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(
        read(&claude_md).contains("Before you finish"),
        "forced back"
    );
}

#[test]
fn no_instructions_skips_the_section_and_a_later_run_drops_it() {
    let sb = Sandbox::new("section-off", &[".claude"]);
    let (_, out) = sb.run(&["install", "--agent", "claude", "--no-instructions"]);
    assert!(!exists(&sb.proj("CLAUDE.md")), "{out}");
    let (_, doctor) = sb.run(&["doctor"]);
    assert!(doctor.contains("up to date (9 files)"), "{doctor}");

    sb.run(&["install", "--agent", "claude"]);
    assert!(exists(&sb.proj("CLAUDE.md")));
    let (_, out) = sb.run(&["install", "--agent", "claude", "--no-instructions"]);
    assert!(out.contains("removed   CLAUDE.md section"), "{out}");
    assert!(!exists(&sb.proj("CLAUDE.md")));
}

#[test]
fn agents_md_section_is_shared_like_the_skill() {
    let sb = Sandbox::new("section-shared", &[".codex", ".cursor"]);
    sb.run(&["install", "--agent", "all"]);
    let agents_md = sb.proj("AGENTS.md");
    assert_eq!(read(&agents_md).matches("rustgrade:begin").count(), 1);

    sb.run(&["uninstall", "--agent", "codex"]);
    assert!(exists(&agents_md), "still used by cursor");
    sb.run(&["uninstall", "--agent", "cursor"]);
    assert!(!exists(&agents_md), "removed with its last owner");
}
