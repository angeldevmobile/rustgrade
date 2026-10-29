//! End-to-end tests for `rustgrade audit`, running real cargo on the fixtures.
//! The expected grades are the rubric's own numbers (see rubric.md).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../evals/fixtures")
        .join(name)
}

fn rustgrade(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rustgrade"))
        .args(args)
        .output()
        .expect("run rustgrade")
}

fn audit_json(name: &str, extra: &[&str]) -> Value {
    let dir = fixture(name);
    let mut args = vec![
        "audit",
        dir.to_str().expect("utf-8 path"),
        "--format",
        "json",
    ];
    args.extend_from_slice(extra);
    let out = rustgrade(&args);
    assert!(
        out.status.success(),
        "audit failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("valid JSON")
}

fn area<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["areas"]
        .as_array()
        .and_then(|areas| areas.iter().find(|a| a["area"] == name))
        .map_or(&Value::Null, |a| &a["score"])
}

#[test]
fn rubric_example_grades_b_89_from_the_scanner_alone() {
    let r = audit_json("unwrap-daemon", &["--no-clippy", "--no-tests"]);
    assert_eq!(r["schema"], 1);
    assert_eq!(
        (&r["grade"], &r["score"], &r["cap"]),
        (&"B".into(), &89.into(), &"one-high".into())
    );
    assert_eq!(area(&r, "errors"), 80);
    assert_eq!(area(&r, "unsafe"), &Value::Null, "no unsafe code: n/a");
    assert_eq!(area(&r, "concurrency"), &Value::Null, "no concurrency: n/a");
    assert_eq!(r["tests"]["status"], "skipped");
    assert_eq!(r["clippy"]["status"], "skipped");
    assert_eq!(r["scan"]["findings"].as_array().map(Vec::len), Some(2));
}

#[test]
fn full_audit_runs_clippy_and_tests() {
    let r = audit_json("unwrap-daemon", &[]);
    assert_eq!(r["clippy"]["status"], "ran");
    assert_eq!(r["tests"]["status"], "passed");
    assert!(r["tests"]["passed"].as_u64() >= Some(1), "{}", r["tests"]);
    assert_eq!(r["grade"], "B");
}

#[test]
fn concurrency_applies_when_the_crate_uses_it() {
    let r = audit_json("mutex-everywhere", &["--no-clippy", "--no-tests"]);
    assert!(area(&r, "concurrency").is_u64());
    assert_eq!((&r["grade"], &r["cap"]), (&"C".into(), &"many-high".into()));
}

#[test]
fn a_crate_that_does_not_compile_is_an_f() {
    // The fixture uses tokio types without depending on tokio.
    let r = audit_json("mutex-everywhere", &["--no-tests"]);
    assert_eq!(
        (&r["grade"], &r["cap"]),
        (&"F".into(), &"does-not-compile".into())
    );
    assert!(
        r["error"].as_str().is_some_and(|e| e.contains("tokio")),
        "{}",
        r["error"]
    );
}

#[test]
fn fail_on_accepts_grades_for_audit_only() {
    let dir = fixture("mutex-everywhere");
    let dir = dir.to_str().expect("utf-8 path");
    let code = |args: &[&str]| rustgrade(args).status.code();
    let audit = |grade: &str| {
        code(&[
            "audit",
            dir,
            "--no-clippy",
            "--no-tests",
            "--fail-on",
            grade,
        ])
    };

    assert_eq!(audit("C"), Some(1), "C is at the threshold");
    assert_eq!(audit("b"), Some(1), "C is worse than B");
    assert_eq!(audit("D"), Some(0), "C is better than D");
    assert_eq!(
        code(&["scan", dir, "--fail-on", "B"]),
        Some(2),
        "grades need audit"
    );
}

#[test]
fn audit_needs_a_cargo_manifest() {
    let src = fixture("unwrap-daemon").join("src");
    let out = rustgrade(&["audit", src.to_str().expect("utf-8 path")]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no Cargo.toml"));
}

#[test]
fn a_missing_path_says_so() {
    for cmd in ["audit", "scan"] {
        let out = rustgrade(&[cmd, "no/such/dir"]);
        assert_eq!(out.status.code(), Some(2), "{cmd}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("does not exist"), "{cmd}: {err}");
    }
}
