//! Files `install` writes, embedded at build time so the binary is self-contained.
// TODO: before publishing to crates.io, copy these into the crate (a package
// cannot include files outside its directory).

/// A file to install: its path relative to the target directory, and contents.
pub struct Asset {
    pub path: &'static str,
    pub contents: &'static str,
}

/// `skills/rustgrade/`, installed into each agent's skill directory.
pub const SKILL: &[Asset] = &[
    Asset {
        path: "SKILL.md",
        contents: include_str!("../../../../skills/rustgrade/SKILL.md"),
    },
    Asset {
        path: "references/concurrency.md",
        contents: include_str!("../../../../skills/rustgrade/references/concurrency.md"),
    },
    Asset {
        path: "references/errors.md",
        contents: include_str!("../../../../skills/rustgrade/references/errors.md"),
    },
    Asset {
        path: "references/ownership.md",
        contents: include_str!("../../../../skills/rustgrade/references/ownership.md"),
    },
    Asset {
        path: "references/performance.md",
        contents: include_str!("../../../../skills/rustgrade/references/performance.md"),
    },
    Asset {
        path: "references/rubric.md",
        contents: include_str!("../../../../skills/rustgrade/references/rubric.md"),
    },
    Asset {
        path: "references/unsafe.md",
        contents: include_str!("../../../../skills/rustgrade/references/unsafe.md"),
    },
];

/// `commands/`, installed for agents that have a commands directory.
pub const COMMANDS: &[Asset] = &[
    Asset {
        path: "audit.md",
        contents: include_str!("../../../../commands/audit.md"),
    },
    Asset {
        path: "hot-path.md",
        contents: include_str!("../../../../commands/hot-path.md"),
    },
];

pub const TARGETS: &str = include_str!("../../../../targets/targets.toml");

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    /// Every file on disk must be embedded, so a new reference can't be
    /// forgotten here.
    fn assert_embeds_all(dir: &str, assets: &[Asset]) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(dir);
        let mut stack = vec![root.clone()];
        while let Some(d) = stack.pop() {
            for entry in fs::read_dir(&d).expect("readable dir") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let rel = path
                    .strip_prefix(&root)
                    .expect("under root")
                    .to_string_lossy()
                    .replace('\\', "/");
                let asset = assets.iter().find(|a| a.path == rel);
                let on_disk = fs::read_to_string(&path).expect("utf-8 file");
                assert!(asset.is_some(), "{dir}/{rel} is not embedded in assets.rs");
                assert_eq!(asset.map(|a| a.contents), Some(on_disk.as_str()));
            }
        }
    }

    #[test]
    fn embeds_every_skill_and_command_file() {
        assert_embeds_all("skills/rustgrade", SKILL);
        assert_embeds_all("commands", COMMANDS);
    }
}
