//! `~/.rustgrade/manifest.json`: every file `install` wrote, with its hash, so
//! `uninstall` removes exactly those files and never one the user changed.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use super::Scope;

const VERSION: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub installs: Vec<Install>,
}

/// One `rustgrade install` of one agent at one root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Install {
    pub agent: String,
    pub scope: Scope,
    pub root: PathBuf,
    /// rustgrade version that wrote the files.
    pub rustgrade: String,
    pub files: Vec<FileRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: PathBuf,
    pub hash: String,
}

impl Manifest {
    pub fn path(home: &Path) -> PathBuf {
        home.join(".rustgrade").join("manifest.json")
    }

    pub fn load(home: &Path) -> anyhow::Result<Self> {
        let path = Self::path(home);
        match fs::read_to_string(&path) {
            Ok(text) => {
                let m: Self = serde_json::from_str(&text)
                    .with_context(|| format!("parsing {}", path.display()))?;
                anyhow::ensure!(
                    m.version == VERSION,
                    "{} has version {}, this rustgrade understands {VERSION}",
                    path.display(),
                    m.version
                );
                Ok(m)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                version: VERSION,
                installs: Vec::new(),
            }),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn save(&self, home: &Path) -> anyhow::Result<()> {
        let path = Self::path(home);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let text = serde_json::to_string_pretty(self)? + "\n";
        fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
    }

    pub fn find(&self, agent: &str, root: &Path) -> Option<&Install> {
        self.installs
            .iter()
            .find(|i| i.agent == agent && i.root == root)
    }

    /// Replaces (or adds) the record for the same agent and root.
    pub fn upsert(&mut self, install: Install) {
        self.installs
            .retain(|i| !(i.agent == install.agent && i.root == install.root));
        self.installs.push(install);
    }

    pub fn remove(&mut self, agent: &str, root: &Path) {
        self.installs
            .retain(|i| !(i.agent == agent && i.root == root));
    }

    /// The hash any install recorded for `path`, if it was written by rustgrade.
    pub fn recorded_hash(&self, path: &Path) -> Option<&str> {
        self.installs
            .iter()
            .flat_map(|i| &i.files)
            .find(|f| f.path == path)
            .map(|f| f.hash.as_str())
    }

    /// Whether an install other than (`agent`, `root`) also owns `path`
    /// (Codex and Cursor share `.agents/skills/`).
    pub fn shared_with_others(&self, path: &Path, agent: &str, root: &Path) -> bool {
        self.installs
            .iter()
            .filter(|i| !(i.agent == agent && i.root == root))
            .any(|i| i.files.iter().any(|f| f.path == path))
    }
}
