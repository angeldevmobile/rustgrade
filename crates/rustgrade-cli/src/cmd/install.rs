//! `rustgrade install` / `uninstall`: copy the skill and commands into agent
//! directories, tracked in the manifest so every change is reversible.

use std::borrow::Cow;
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
        let install = install_agent(
            &mut out,
            agent,
            &all,
            &root,
            &manifest,
            opts,
            &mut conflicts,
        )?;
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
    all: &[Agent],
    root: &Path,
    manifest: &Manifest,
    opts: &Options<'_>,
    conflicts: &mut usize,
) -> anyhow::Result<Install> {
    header(out, agent, opts.scope, root)?;
    let planned = agents::plan(agent, root, opts.scope);
    let skill_dir = agents::join_rel(root, &agent.skill_dir);
    let mut records = Vec::new();

    // Decide and write first, so an untouched skill can be reported in one line.
    let mut decided = Vec::with_capacity(planned.len());
    for (path, contents) in &planned {
        let hash = content_hash(contents.as_bytes());
        let action = decide(path, &hash, manifest, opts.force)?;
        if matches!(action, Action::Create | Action::Update) && !opts.dry_run {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
            }
            fs::write(path, contents.as_bytes())
                .with_context(|| format!("writing {}", path.display()))?;
        }
        decided.push((path, hash, action));
    }
    let skill_untouched = decided
        .iter()
        .filter(|(p, ..)| p.starts_with(&skill_dir))
        .all(|(.., a)| *a == Action::Unchanged);
    if skill_untouched {
        let n = decided
            .iter()
            .filter(|(p, ..)| p.starts_with(&skill_dir))
            .count();
        let others = owners(manifest, all, root, &agent.id, &skill_dir);
        let verb = if others.is_empty() {
            "unchanged"
        } else {
            "shared"
        };
        write_dir_summary(out, verb, &skill_dir, root, n, "with", &others)?;
    }

    for (path, hash, action) in decided {
        if skill_untouched && path.starts_with(&skill_dir) {
            records.push(FileRecord {
                path: path.clone(),
                hash,
            });
            continue;
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
        let agent = all.iter().find(|a| a.id == inst.agent);
        match agent {
            Some(agent) => header(&mut out, agent, inst.scope, &root)?,
            None => writeln!(out, "{} ({})", inst.agent, root.display())?,
        }
        // A skill another agent still uses is kept whole; say so in one line.
        let shared_skill = agent
            .map(|a| agents::join_rel(&root, &a.skill_dir))
            .filter(|dir| {
                let mut in_dir = inst
                    .files
                    .iter()
                    .filter(|f| f.path.starts_with(dir))
                    .peekable();
                in_dir.peek().is_some()
                    && in_dir.all(|f| manifest.shared_with_others(&f.path, &inst.agent, &root))
            });
        if let Some(dir) = &shared_skill {
            let n = inst
                .files
                .iter()
                .filter(|f| f.path.starts_with(dir))
                .count();
            let others = owners(&manifest, &all, &root, &inst.agent, dir);
            write_dir_summary(&mut out, "kept", dir, &root, n, "still used by", &others)?;
        }
        for f in &inst.files {
            if shared_skill
                .as_ref()
                .is_some_and(|dir| f.path.starts_with(dir))
            {
                continue;
            }
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
                        prune_empty_parents(&f.path, &root);
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
    planned: &[(PathBuf, Cow<'static, str>)],
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
                prune_empty_parents(&f.path, root);
            }
            write_entry(out, "removed", &f.path, root)?;
            writeln!(out)?;
        }
    }
    Ok(())
}

/// Names of the other agents installed at `root` with files under `dir`.
fn owners<'a>(
    manifest: &Manifest,
    all: &'a [Agent],
    root: &Path,
    except: &str,
    dir: &Path,
) -> Vec<&'a str> {
    all.iter()
        .filter(|a| a.id != except)
        .filter(|a| {
            manifest
                .find(&a.id, root)
                .is_some_and(|i| i.files.iter().any(|f| f.path.starts_with(dir)))
        })
        .map(|a| a.name.as_str())
        .collect()
}

/// "  shared    .agents/skills/rustgrade/ (7 files, with Codex CLI, Cursor)".
fn write_dir_summary(
    out: &mut impl Write,
    verb: &str,
    dir: &Path,
    root: &Path,
    files: usize,
    relation: &str,
    others: &[&str],
) -> io::Result<()> {
    write_entry(out, verb, dir, root)?;
    write!(out, "/ ({files} files")?;
    if let Some((first, rest)) = others.split_first() {
        write!(out, ", {relation} {first}")?;
        for name in rest {
            write!(out, ", {name}")?;
        }
    }
    writeln!(out, ")")
}

/// After removing `file`, removes its parent directories while they are empty,
/// stopping at the first non-empty one and never at or above `root`.
fn prune_empty_parents(file: &Path, root: &Path) {
    for dir in file.ancestors().skip(1) {
        // `remove_dir` fails (and we stop) as soon as a directory has contents.
        if dir == root || !dir.starts_with(root) || fs::remove_dir(dir).is_err() {
            break;
        }
    }
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
