//! Files `install` writes, embedded at build time so the binary is self-contained.
//!
//! `assets/` is a copy of the repo's `skills/`, `commands/` and
//! `targets/targets.toml` (the Claude Code plugin reads the originals; a
//! published crate cannot include files outside its directory). Edit the
//! originals, then `UPDATE_ASSETS=1 cargo test -p rustgrade assets`.

/// A file to install: its path relative to the target directory, and contents.
pub struct Asset {
    pub path: &'static str,
    pub contents: &'static str,
}

/// `skills/rustgrade/`, installed into each agent's skill directory.
pub const SKILL: &[Asset] = &[
    Asset {
        path: "SKILL.md",
        contents: include_str!("../../assets/skills/rustgrade/SKILL.md"),
    },
    Asset {
        path: "references/concurrency.md",
        contents: include_str!("../../assets/skills/rustgrade/references/concurrency.md"),
    },
    Asset {
        path: "references/errors.md",
        contents: include_str!("../../assets/skills/rustgrade/references/errors.md"),
    },
    Asset {
        path: "references/ownership.md",
        contents: include_str!("../../assets/skills/rustgrade/references/ownership.md"),
    },
    Asset {
        path: "references/performance.md",
        contents: include_str!("../../assets/skills/rustgrade/references/performance.md"),
    },
    Asset {
        path: "references/rubric.md",
        contents: include_str!("../../assets/skills/rustgrade/references/rubric.md"),
    },
    Asset {
        path: "references/unsafe.md",
        contents: include_str!("../../assets/skills/rustgrade/references/unsafe.md"),
    },
];

/// `commands/`, installed for agents that have a commands directory.
pub const COMMANDS: &[Asset] = &[
    Asset {
        path: "audit.md",
        contents: include_str!("../../assets/commands/audit.md"),
    },
    Asset {
        path: "hot-path.md",
        contents: include_str!("../../assets/commands/hot-path.md"),
    },
];

pub const TARGETS: &str = include_str!("../../assets/targets.toml");

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;

    fn crate_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    /// Files under `root`, as sorted `/`-separated relative paths.
    fn files_under(root: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for entry in fs::read_dir(&d).expect("readable dir") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path.strip_prefix(root).expect("under root");
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        out.sort();
        out
    }

    /// Read + write instead of `fs::copy`, which keeps the source's mtime on
    /// Windows: cargo would then miss the change and keep the stale
    /// `include_str!`.
    fn copy(from: &Path, to: &Path) {
        fs::write(to, fs::read(from).expect("read original")).expect("write asset");
    }

    /// Every file in `assets/<dir>` must be embedded, so a new reference
    /// can't be forgotten here. Skipped while `UPDATE_ASSETS` rewrites them.
    fn assert_embeds_all(dir: &str, assets: &[Asset]) {
        if std::env::var_os("UPDATE_ASSETS").is_some() {
            return;
        }
        let root = crate_dir().join("assets").join(dir);
        for rel in files_under(&root) {
            let on_disk = fs::read_to_string(root.join(&rel)).expect("utf-8 file");
            let asset = assets.iter().find(|a| a.path == rel);
            assert!(
                asset.is_some(),
                "assets/{dir}/{rel} is not embedded in assets.rs"
            );
            assert_eq!(asset.map(|a| a.contents), Some(on_disk.as_str()));
        }
    }

    #[test]
    fn embeds_every_skill_and_command_file() {
        assert_embeds_all("skills/rustgrade", SKILL);
        assert_embeds_all("commands", COMMANDS);
    }

    /// `assets/` must mirror the repo's originals. Skipped in a packaged
    /// crate, where the originals don't exist.
    #[test]
    fn assets_mirror_the_repo_originals() {
        let repo = crate_dir().join("../..");
        let assets = crate_dir().join("assets");
        if !repo.join("skills").is_dir() {
            return;
        }
        let update = std::env::var_os("UPDATE_ASSETS").is_some();
        let mut stale = Vec::new();
        for (original, mirrored) in [
            ("skills/rustgrade", "skills/rustgrade"),
            ("commands", "commands"),
        ] {
            let (src, dst) = (repo.join(original), assets.join(mirrored));
            if update {
                // Recreate so files deleted upstream go away too.
                let _ = fs::remove_dir_all(&dst);
                for rel in files_under(&src) {
                    let to = dst.join(&rel);
                    fs::create_dir_all(to.parent().expect("has parent")).expect("mkdir");
                    copy(&src.join(&rel), &to);
                }
            } else if files_under(&src) != files_under(&dst) {
                stale.push(format!("{original}/ (file list differs)"));
            } else {
                for rel in files_under(&src) {
                    let read = |p: PathBuf| fs::read(p).expect("readable");
                    if read(src.join(&rel)) != read(dst.join(&rel)) {
                        stale.push(format!("{original}/{rel}"));
                    }
                }
            }
        }
        let targets = repo.join("targets/targets.toml");
        let targets_copy = assets.join("targets.toml");
        if update {
            copy(&targets, &targets_copy);
        } else if fs::read(&targets).ok() != fs::read(&targets_copy).ok() {
            stale.push("targets/targets.toml".into());
        }
        assert!(
            stale.is_empty(),
            "assets/ is out of date: {}\nrun `UPDATE_ASSETS=1 cargo test -p rustgrade assets`",
            stale.join(", ")
        );
    }
}
