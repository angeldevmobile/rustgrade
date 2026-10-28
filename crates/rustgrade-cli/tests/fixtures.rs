//! Snapshot tests: `rustgrade scan evals/fixtures/<name> --format json` must
//! produce exactly `evals/expected/<name>.json`.
//!
//! After an intended change, regenerate and review the diff:
//! `UPDATE_EXPECTED=1 cargo test -p rustgrade --test fixtures`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scan_json(dir: &Path) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_rustgrade"))
        .arg("scan")
        .arg(dir)
        .args(["--format", "json"])
        .output()
        .expect("run rustgrade");
    assert!(
        output.status.success(),
        "rustgrade failed on {}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("valid JSON")
}

#[test]
fn fixtures_match_expected() {
    let root = repo_root();
    let expected_dir = root.join("evals/expected");
    let update = std::env::var_os("UPDATE_EXPECTED").is_some();

    let mut fixtures: Vec<PathBuf> = fs::read_dir(root.join("evals/fixtures"))
        .expect("evals/fixtures exists")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.is_dir())
        .collect();
    fixtures.sort();
    assert!(!fixtures.is_empty(), "no fixtures found");

    let mut failures = Vec::new();
    for dir in &fixtures {
        let name = dir.file_name().expect("named").to_string_lossy();
        let actual = scan_json(dir);
        let path = expected_dir.join(format!("{name}.json"));

        if update {
            fs::create_dir_all(&expected_dir).expect("create evals/expected");
            let text = serde_json::to_string_pretty(&actual).expect("serialize") + "\n";
            fs::write(&path, text).expect("write snapshot");
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            failures.push(format!(
                "{name}: missing {}; run with UPDATE_EXPECTED=1",
                path.display()
            ));
            continue;
        };
        let expected: Value = serde_json::from_str(&text).expect("snapshot is valid JSON");
        if expected != actual {
            failures.push(format!(
                "{name}: output differs from {}\n--- actual ---\n{}",
                path.display(),
                serde_json::to_string_pretty(&actual).expect("serialize")
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
