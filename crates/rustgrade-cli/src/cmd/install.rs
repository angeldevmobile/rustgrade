//! `rustgrade install` / `uninstall`: copy the skill and commands into agent
//! directories, tracked in the manifest so every change is reversible.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::agents::manifest::{FileRecord, Install, Manifest};
use crate::agents::{self, Agent, Scope, content_hash};
use crate::{EXIT_FINDINGS, EXIT_OK};

pub struct Options<'a> {
    /// Agent id from targets.toml, or `all`.
    pub agent: &'a str,
    pub scope: Scope,
    /// Project directory for `--scope project` (default: current directory).
    pub dir: Option<&'a Path>,
    pub force: bool,
    pub dry_run: bool,
}

/// What happens to one file.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Create,
    Update,
    Unchanged,
    /// The file exists and is not rustgrade's unmodified copy.
    Conflict,
}

pub fn install(opts: &Options<'_>) -> anyhow::Result<u8> {
    let home = agents::home()?;
    let root = install_root(opts, &home)?;
    let all = agents::targets()?;
    let chosen = select(&all, opts.agent, &home)?;
    let mut manifest = Manifest::load(&home)?;
    let mut out = io::stdout().lock();
    let mut conflicts = 0;

    for agent in chosen {
        let install = install_agent(&mut out, agent, &root, &manifest, opts, &mut conflicts)?;
        manifest.upsert(install);
    }

    if opts.dry_run {
        writeln!(out, "\nDry run: nothing was written.")?;
    } else {
        manifest.save(&home)?;
        writeln!(
            out,
            "\nDone. Start a new agent session to load the skill. Undo with `rustgrade uninstall --agent {}`{}.",
            opts.agent,
            scope_flag(opts.scope)
        )?;
    }
    Ok(if conflicts > 0 {
        EXIT_FINDINGS
    } else {
        EXIT_OK
    })
}

/// Writes one agent's files and returns the manifest record for them. Files
/// left untouched because of a conflict are counted and not recorded.
fn install_agent(
    out: &mut impl Write,
    agent: &Agent,
    root: &Path,
    manifest: &Manifest,
    opts: &Options<'_>,
    conflicts: &mut usize,
) -> anyhow::Result<Install> {
    header(out, agent, opts.scope, root)?;
    let planned = agents::plan(agent, root);
    let mut records = Vec::new();
    for (path, contents) in &planned {
        let hash = content_hash(contents.as_bytes());
        let action = decide(path, &hash, manifest, opts.force)?;
        if matches!(action, Action::Create | Action::Update) && !opts.dry_run {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
            }
            fs::write(path, contents).with_context(|| format!("writing {}", path.display()))?;
        }
        let verb = match action {
            Action::Create => "created",
            Action::Update => "updated",
            Action::Unchanged => "unchanged",
            Action::Conflict => "kept",
        };
        write_entry(out, verb, path, root)?;
        if action == Action::Conflict {
            *conflicts += 1;
            writeln!(
                out,
                "  (not written by rustgrade, or edited; --force to overwrite)"
            )?;
        } else {
            writeln!(out)?;
            records.push(FileRecord {
                path: path.clone(),
                hash,
            });
        }
    }
    remove_stale(out, manifest, agent, root, &planned, opts)?;
    Ok(Install {
        agent: agent.id.clone(),
        scope: opts.scope,
        root: root.to_path_buf(),
        rustgrade: env!("CARGO_PKG_VERSION").to_owned(),
        files: records,
    })
}

pub fn uninstall(opts: &Options<'_>) -> anyhow::Result<u8> {
    let home = agents::home()?;
    let root = install_root(opts, &home)?;
    let all = agents::targets()?;
    let mut manifest = Manifest::load(&home)?;
    let mut out = io::stdout().lock();

    let installs: Vec<Install> = manifest
        .installs
        .iter()
        .filter(|i| i.root == root && (opts.agent == "all" || i.agent == opts.agent))
        .cloned()
        .collect();
    if installs.is_empty() {
        writeln!(
            out,
            "Nothing installed for `{}` at {}.",
            opts.agent,
            root.display()
        )?;
        return Ok(EXIT_OK);
    }

    let mut kept = 0;
    for inst in &installs {
        match all.iter().find(|a| a.id == inst.agent) {
            Some(agent) => header(&mut out, agent, inst.scope, &root)?,
            None => writeln!(out, "{} ({})", inst.agent, root.display())?,
        }
        for f in &inst.files {
            if manifest.shared_with_others(&f.path, &inst.agent, &root) {
                write_entry(&mut out, "kept", &f.path, &root)?;
                writeln!(out, "  (also installed for another agent)")?;
                continue;
            }
            let verb = match fs::read(&f.path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => "gone",
                Err(e) => return Err(e).with_context(|| format!("reading {}", f.path.display())),
                Ok(bytes) if content_hash(&bytes) == f.hash || opts.force => {
                    if !opts.dry_run {
                        fs::remove_file(&f.path)
                            .with_context(|| format!("removing {}", f.path.display()))?;
                    }
                    "removed"
                }
                Ok(_) => {
                    kept += 1;
                    "kept"
                }
            };
            write_entry(&mut out, verb, &f.path, &root)?;
            if verb == "kept" {
                writeln!(out, "  (edited since install; --force to remove)")?;
            } else {
                writeln!(out)?;
            }
        }
        if let Some(agent) = all.iter().find(|a| a.id == inst.agent)
            && !opts.dry_run
        {
            remove_empty_dirs(&root.join(&agent.skill_dir));
        }
        if !opts.dry_run {
            manifest.remove(&inst.agent, &root);
        }
    }

    if opts.dry_run {
        writeln!(out, "\nDry run: nothing was removed.")?;
    } else {
        manifest.save(&home)?;
    }
    Ok(if kept > 0 { EXIT_FINDINGS } else { EXIT_OK })
}

/// Project scope: `--dir` or the current directory. User scope: home.
fn install_root(opts: &Options<'_>, home: &Path) -> anyhow::Result<PathBuf> {
    let dir = match (opts.scope, opts.dir) {
        (Scope::User, Some(_)) => anyhow::bail!("--dir applies to --scope project only"),
        (Scope::User, None) => home.to_path_buf(),
        (Scope::Project, Some(d)) => d.to_path_buf(),
        (Scope::Project, None) => std::env::current_dir()?,
    };
    crate::cmd::scan::ensure_exists(&dir)?;
    // `absolute`, not `canonicalize`: no `\\?\` prefix on Windows.
    std::path::absolute(&dir).with_context(|| format!("resolving {}", dir.display()))
}

fn select<'a>(all: &'a [Agent], id: &str, home: &Path) -> anyhow::Result<Vec<&'a Agent>> {
    let ids = || {
        all.iter()
            .map(|a| a.id.as_str())
            .collect::<Vec<_>>()
            .join("|")
    };
    if id == "all" {
        let found: Vec<_> = all.iter().filter(|a| agents::detected(a, home)).collect();
        anyhow::ensure!(
            !found.is_empty(),
            "no supported agent detected in {}; pass --agent {}",
            home.display(),
            ids()
        );
        return Ok(found);
    }
    all.iter()
        .find(|a| a.id == id)
        .map(|a| vec![a])
        .ok_or_else(|| anyhow::anyhow!("unknown agent `{id}`; expected {} or all", ids()))
}

fn decide(path: &Path, hash: &str, manifest: &Manifest, force: bool) -> anyhow::Result<Action> {
    let existing = match fs::read(path) {
        Ok(bytes) => content_hash(&bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Action::Create),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    Ok(if existing == hash {
        Action::Unchanged
    } else if force || manifest.recorded_hash(path) == Some(existing.as_str()) {
        // Our own earlier, unedited version: safe to upgrade.
        Action::Update
    } else {
        Action::Conflict
    })
}

/// Removes files a previous install of this agent wrote that the current
/// version no longer ships, if unedited and not owned by another agent.
fn remove_stale(
    out: &mut impl Write,
    manifest: &Manifest,
    agent: &Agent,
    root: &Path,
    planned: &[(PathBuf, &str)],
    opts: &Options<'_>,
) -> anyhow::Result<()> {
    let Some(prev) = manifest.find(&agent.id, root) else {
        return Ok(());
    };
    for f in &prev.files {
        let still_shipped = planned.iter().any(|(p, _)| *p == f.path);
        if still_shipped || manifest.shared_with_others(&f.path, &agent.id, root) {
            continue;
        }
        let unedited = fs::read(&f.path).is_ok_and(|b| content_hash(&b) == f.hash);
        if unedited {
            if !opts.dry_run {
                fs::remove_file(&f.path)
                    .with_context(|| format!("removing {}", f.path.display()))?;
            }
            write_entry(out, "removed", &f.path, root)?;
            writeln!(out)?;
        }
    }
    Ok(())
}

/// Removes `dir` and its subdirectories if they are empty; never touches files.
fn remove_empty_dirs(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                remove_empty_dirs(&entry.path());
            }
        }
    }
    // Fails (and is ignored) when the directory still has files.
    let _ = fs::remove_dir(dir);
}

fn header(out: &mut impl Write, agent: &Agent, scope: Scope, root: &Path) -> io::Result<()> {
    writeln!(
        out,
        "{} ({}: {})",
        agent.name,
        scope.as_str(),
        root.display()
    )
}

/// "  created   .claude/skills/rustgrade/SKILL.md": the path relative to the
/// install root, with `/` on every platform, and no trailing newline.
fn write_entry(out: &mut impl Write, verb: &str, path: &Path, root: &Path) -> io::Result<()> {
    write!(out, "  {verb:<10}")?;
    let rel = path.strip_prefix(root).unwrap_or(path);
    for (i, part) in rel.components().enumerate() {
        let sep = if i == 0 { "" } else { "/" };
        write!(out, "{sep}{}", part.as_os_str().display())?;
    }
    Ok(())
}

fn scope_flag(scope: Scope) -> &'static str {
    match scope {
        Scope::Project => "",
        Scope::User => " --scope user",
    }
}
