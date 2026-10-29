//! `rustgrade doctor`: toolchain, detected agents and the state of every install.

use std::fs;
use std::io::{self, Write};
use std::process::Command;

use crate::EXIT_OK;
use crate::agents::manifest::{Install, Manifest};
use crate::agents::{self, Agent, content_hash};

pub fn run() -> anyhow::Result<u8> {
    let home = agents::home()?;
    let all = agents::targets()?;
    let manifest = Manifest::load(&home)?;
    let mut out = io::stdout().lock();

    writeln!(out, "rustgrade {}", env!("CARGO_PKG_VERSION"))?;
    tool(
        &mut out,
        "cargo",
        &["--version"],
        "install Rust from https://rustup.rs",
    )?;
    tool(
        &mut out,
        "clippy",
        &["clippy", "--version"],
        "`rustup component add clippy` (needed by `rustgrade audit`)",
    )?;

    writeln!(out, "\nAgents (home: {})", home.display())?;
    for agent in &all {
        let detected = agents::detected(agent, &home);
        writeln!(
            out,
            "  {:<12} {}",
            agent.name,
            if detected { "detected" } else { "not detected" }
        )?;
        for inst in manifest.installs.iter().filter(|i| i.agent == agent.id) {
            write!(out, "    {} {}: ", inst.scope.as_str(), inst.root.display())?;
            write_status(&mut out, agent, inst)?;
            writeln!(out)?;
        }
    }

    // Cursor also reads .claude/skills, so it sees two copies of the skill.
    for inst in &manifest.installs {
        let claude_too = inst.agent == "cursor" && manifest.find("claude", &inst.root).is_some();
        if claude_too {
            writeln!(
                out,
                "\nNote: {} has rustgrade for both Claude Code and Cursor; Cursor reads .claude/skills too and will list the skill twice.",
                inst.root.display()
            )?;
        }
    }
    if manifest.installs.is_empty() {
        writeln!(
            out,
            "\nNothing installed yet: `rustgrade install --agent all` (this project) or add `--scope user`."
        )?;
    }
    Ok(EXIT_OK)
}

/// "up to date (9 files)", or what is wrong and the command that fixes it.
fn write_status(out: &mut impl Write, agent: &Agent, inst: &Install) -> io::Result<()> {
    let (mut missing, mut edited) = (0, 0);
    for f in &inst.files {
        match fs::read(&f.path) {
            Ok(bytes) if content_hash(&bytes) == f.hash => {}
            Ok(_) => edited += 1,
            Err(_) => missing += 1,
        }
    }
    // Shipped files that were never written (an existing file was kept), and
    // files written by an older rustgrade.
    let (mut blocked, mut outdated) = (0, 0);
    for (path, contents) in agents::plan(agent, &inst.root, inst.scope) {
        match inst.files.iter().find(|f| f.path == path) {
            None => blocked += 1,
            Some(f) if f.hash != content_hash(contents.as_bytes()) => outdated += 1,
            Some(_) => {}
        }
    }

    if missing + edited + blocked + outdated == 0 {
        return write!(out, "up to date ({} files)", inst.files.len());
    }
    let id = &agent.id;
    let mut sep = "";
    if outdated > 0 {
        write!(
            out,
            "outdated, from rustgrade {}: `rustgrade install --agent {id}`",
            inst.rustgrade
        )?;
        sep = "; ";
    }
    if missing > 0 {
        write!(
            out,
            "{sep}{missing} file(s) deleted: `rustgrade install --agent {id}`"
        )?;
        sep = "; ";
    }
    if blocked > 0 {
        write!(
            out,
            "{sep}{blocked} file(s) not installed, an existing file was kept: `--force` to replace"
        )?;
        sep = "; ";
    }
    if edited > 0 {
        write!(out, "{sep}{edited} file(s) edited by you")?;
    }
    Ok(())
}

/// Runs a cargo subcommand and prints its first line, or how to fix it.
fn tool(out: &mut impl Write, label: &str, args: &[&str], fix: &str) -> io::Result<()> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let version = Command::new(cargo)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            String::from_utf8(o.stdout)
                .ok()?
                .lines()
                .next()
                .map(str::to_owned)
        });
    match version {
        Some(v) => writeln!(out, "  {label:<8} {v}"),
        None => writeln!(out, "  {label:<8} missing: {fix}"),
    }
}
