//! Agent targets (`targets/targets.toml`), the files installed for each one,
//! and the install manifest that makes installs reversible.

mod assets;
pub mod manifest;

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use assets::{COMMANDS, SKILL};

#[derive(Debug, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    /// Paths under the home directory whose existence means "installed".
    pub detect: Vec<String>,
    /// Skill directory, relative to the install root.
    pub skill_dir: String,
    /// Commands directory, relative to the install root, if the agent has one.
    #[serde(default)]
    pub commands_dir: Option<String>,
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
    agent.detect.iter().any(|d| home.join(d).exists())
}

/// Every file to write for `agent` under `root`, with its contents.
pub fn plan(agent: &Agent, root: &Path) -> Vec<(PathBuf, &'static str)> {
    let skill_dir = join_rel(root, &agent.skill_dir);
    let mut files: Vec<_> = SKILL
        .iter()
        .map(|a| (join_rel(&skill_dir, a.path), a.contents))
        .collect();
    if let Some(dir) = &agent.commands_dir {
        let dir = join_rel(root, dir);
        files.extend(
            COMMANDS
                .iter()
                .map(|a| (join_rel(&dir, a.path), a.contents)),
        );
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

    #[test]
    fn only_agents_with_a_commands_dir_get_commands() {
        let agents = targets().expect("valid targets.toml");
        let root = Path::new("proj");
        for a in &agents {
            let files = plan(a, root);
            let has_audit = files.iter().any(|(p, _)| p.ends_with("audit.md"));
            assert_eq!(has_audit, a.commands_dir.is_some(), "{}", a.id);
            assert!(files.iter().any(|(p, _)| p.ends_with("SKILL.md")));
        }
    }

    #[test]
    fn hash_is_stable() {
        // Known FNV-1a 64 values; a change here would orphan every manifest.
        assert_eq!(content_hash(b""), "cbf29ce484222325");
        assert_eq!(content_hash(b"a"), "af63dc4c8601ec8c");
    }
}
