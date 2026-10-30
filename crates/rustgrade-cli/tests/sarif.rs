//! `--format sarif`: the contract GitHub code scanning reads.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// Runs from the repo root with a relative path, as CI would.
fn sarif(args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_rustgrade"))
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .args(args)
        .args(["--format", "sarif"])
        .output()
        .expect("run rustgrade");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("valid JSON")
}

fn results(log: &Value) -> &Vec<Value> {
    log["runs"][0]["results"].as_array().expect("results")
}

#[test]
fn scan_results_point_at_their_rule_and_file() {
    let log = sarif(&["scan", "evals/fixtures/mutex-everywhere"]);
    assert_eq!(log["version"], "2.1.0");
    let run = &log["runs"][0];
    assert_eq!(run["tool"]["driver"]["name"], "rustgrade");
    let rules = run["tool"]["driver"]["rules"].as_array().expect("rules");

    assert_ne!(results(&log).len(), 0, "the fixture has findings");
    for r in results(&log) {
        let index = r["ruleIndex"].as_u64().expect("ruleIndex");
        let index = usize::try_from(index).expect("small index");
        assert_eq!(rules[index]["id"], r["ruleId"]);
        let uri = r["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
            .as_str()
            .expect("uri");
        assert!(
            uri.starts_with("evals/fixtures/mutex-everywhere/src/"),
            "{uri}"
        );
        assert!(["error", "warning", "note"].contains(&r["level"].as_str().expect("level")));
    }
}

#[test]
fn inline_suppressions_carry_their_reason() {
    let log = sarif(&["scan", "evals/fixtures/mutex-everywhere"]);
    let suppressed: Vec<_> = results(&log)
        .iter()
        .filter_map(|r| r["suppressions"].as_array())
        .collect();
    assert_ne!(suppressed.len(), 0, "the fixture has a suppression");
    for s in suppressed {
        assert_eq!(s[0]["kind"], "inSource");
        assert!(
            s[0]["justification"]
                .as_str()
                .is_some_and(|j| !j.is_empty())
        );
    }
}

#[test]
fn audit_puts_the_grade_on_the_run() {
    let log = sarif(&[
        "audit",
        "evals/fixtures/unwrap-daemon",
        "--no-clippy",
        "--no-tests",
    ]);
    let props = &log["runs"][0]["properties"];
    assert_eq!(
        (&props["grade"], &props["score"]),
        (&"B".into(), &89.into())
    );
    assert_eq!(results(&log).len(), 2);
}
