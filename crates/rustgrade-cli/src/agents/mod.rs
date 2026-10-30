//! Agent targets (`targets/targets.toml`), the files installed for each one,
//! and the install manifest that makes installs reversible.

mod assets;
mod convert;
pub mod manifest;
pub mod section;

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use assets::{COMMANDS, INSTRUCTIONS, SKILL};
use convert::{Command, Format};

#[derive(Debug, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    /// Paths under the home directory whose existence means "installed".
    pub detect: Vec<String>,
    /// Executables whose presence on PATH means "installed".
    #[serde(default)]
    pub detect_commands: Vec<String>,
    /// Skill directory, relative to the install root.
    pub skill_dir: String,
    /// Commands directory for `--scope project`, if the agent has commands.
    #[serde(default)]
    pub commands_dir: Option<String>,
    /// Commands directory for `--scope user`; `None`: no commands at user scope.
    #[serde(default)]
    pub user_commands_dir: Option<String>,
    /// How `commands/*.md` is translated for this agent.
    #[serde(default)]
    pub commands_format: Format,
    /// Instructions file for `--scope project` that gets the rustgrade section.
    #[serde(default)]
    pub instructions_file: Option<String>,
    /// Instructions file for `--scope user`; `None`: none at user scope.
    #[serde(default)]
    pub user_instructions_file: Option<String>,
}

impl Agent {
    pub fn commands_dir(&self, scope: Scope) -> Option<&str> {
        match scope {
            Scope::Project => self.commands_dir.as_deref(),
            Scope::User => self.user_commands_dir.as_deref(),
        }
    }

    /// Where the rustgrade section goes under `root`, if the agent has an
    /// instructions file at this scope.
    pub fn instructions_path(&self, root: &Path, scope: Scope) -> Option<PathBuf> {
        let rel = match scope {
            Scope::Project => self.instructions_file.as_deref(),
            Scope::User => self.user_instructions_file.as_deref(),
        };
        rel.map(|r| join_rel(root, r))
    }
}

/// The rustgrade section, markers included.
pub fn instructions_block() -> String {
    section::block(INSTRUCTIONS)
}

/// Hash of what rustgrade owns at `path` now: the whole file, or only the
/// section for `section` records. `None` when the file or section is gone.
pub fn current_hash(path: &Path, section: bool) -> std::io::Result<Option<String>> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if !section {
        return Ok(Some(content_hash(&bytes)));
    }
    let text = String::from_utf8_lossy(&bytes);
    Ok(section::extract(&text).map(|b| content_hash(b.as_bytes())))
}

#[derive(Deserialize)]
struct Targets {
    agent: Vec<Agent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// The current project (default): files go under the project directory.
    Project,
    /// Every project: files go under the home directory.
    User,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::User => "user",
        }
    }
}

/// The agents rustgrade can install into, from the embedded `targets.toml`.
pub fn targets() -> anyhow::Result<Vec<Agent>> {
    let t: Targets = toml::from_str(assets::TARGETS).context("parsing targets/targets.toml")?;
    Ok(t.agent)
}

/// The home directory. `RUSTGRADE_HOME` overrides it (used by tests).
pub fn home() -> anyhow::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("RUSTGRADE_HOME") {
        return Ok(PathBuf::from(dir));
    }
    std::env::home_dir().context("cannot find the home directory; set RUSTGRADE_HOME")
}

/// Whether the agent appears to be installed on this machine.
pub fn detected(agent: &Agent, home: &Path) -> bool {
    agent.detect.iter().any(|d| join_rel(home, d).exists())
        || agent.detect_commands.iter().any(|c| on_path(c))
}

/// Whether `name` is an executable in a PATH directory (with a PATHEXT
/// extension on Windows, e.g. `gemini.cmd` from npm).
fn on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let exts = if cfg!(windows) {
        std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.PS1".into())
    } else {
        String::new()
    };
    std::env::split_paths(&path).any(|dir| {
        dir.join(name).is_file()
            || exts.split(';').filter(|e| !e.is_empty()).any(|ext| {
                dir.join(format!("{name}{}", ext.to_ascii_lowercase()))
                    .is_file()
            })
    })
}

/// Every file to write for `agent` under `root`, with its contents. Commands
/// are translated to the agent's format.
pub fn plan(agent: &Agent, root: &Path, scope: Scope) -> Vec<(PathBuf, Cow<'static, str>)> {
    let skill_dir = join_rel(root, &agent.skill_dir);
    let mut files: Vec<_> = SKILL
        .iter()
        .map(|a| (join_rel(&skill_dir, a.path), Cow::Borrowed(a.contents)))
        .collect();
    if let Some(dir) = agent.commands_dir(scope) {
        let dir = join_rel(root, dir);
        files.extend(COMMANDS.iter().map(|a| {
            let cmd = Command::parse(a.path, a.contents);
            let name = convert::file_name(agent.commands_format, cmd.name);
            (dir.join(name), convert::render(agent.commands_format, &cmd))
        }));
    }
    files
}

/// Joins a `/`-separated relative path segment by segment, so the result uses
/// the platform's separator throughout (targets.toml always uses `/`).
pub fn join_rel(base: &Path, rel: &str) -> PathBuf {
    let mut path = base.to_path_buf();
    path.extend(rel.split('/'));
    path
}

/// FNV-1a 64-bit, hex. Stable across Rust versions (unlike `DefaultHasher`),
/// which matters because hashes are stored in the manifest.
pub fn content_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_parse_and_ids_are_unique() {
        let agents = targets().expect("valid targets.toml");
        let mut ids: Vec<_> = agents.iter().map(|a| a.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), agents.len());
        assert!(ids.contains(&"claude") && ids.contains(&"codex") && ids.contains(&"cursor"));
    }

    fn has_audit(files: &[(PathBuf, Cow<'static, str>)]) -> bool {
        files.iter().any(|(p, _)| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("audit."))
        })
    }

    #[test]
    fn commands_follow_each_agents_dir_and_scope() {
        let agents = targets().expect("valid targets.toml");
        let root = Path::new("proj");
        for a in &agents {
            for scope in [Scope::Project, Scope::User] {
                let files = plan(a, root, scope);
                assert_eq!(
                    has_audit(&files),
                    a.commands_dir(scope).is_some(),
                    "{} {scope:?}",
                    a.id
                );
                assert!(files.iter().any(|(p, _)| p.ends_with("SKILL.md")));
            }
            if a.commands_dir.is_some() {
                assert!(
                    a.commands_format != Format::Claude || a.id == "claude",
                    "{} format",
                    a.id
                );
            }
        }
    }

    #[test]
    fn every_agent_but_claude_shares_the_agents_skill_dir() {
        for a in targets().expect("valid targets.toml") {
            let shared = a.skill_dir == ".agents/skills/rustgrade";
            assert_eq!(shared, a.id != "claude", "{}", a.id);
        }
    }

    #[test]
    fn hash_is_stable() {
        // Known FNV-1a 64 values; a change here would orphan every manifest.
        assert_eq!(content_hash(b""), "cbf29ce484222325");
        assert_eq!(content_hash(b"a"), "af63dc4c8601ec8c");
    }
}
