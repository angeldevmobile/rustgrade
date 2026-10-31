//! `rustgrade hook`: the JSON contract with Claude Code, in throwaway projects.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const UNWRAP: &str = "pub fn f(x: Option<u8>) -> u8 {\n    x.unwrap()\n}\n";

/// A fresh Cargo project, removed when dropped.
struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("rustgrade-hook-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).expect("create project");
        fs::write(dir.join("Cargo.toml"), "[package]\nname = \"p\"\n").expect("write manifest");
        Self(dir)
    }

    fn write(&self, rel: &str, content: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
        fs::write(path, content).expect("write file");
    }

    /// Runs the hook as if `rel` had just been edited; returns its stdout.
    fn edited(&self, rel: &str) -> String {
        let input = json!({ "cwd": self.0, "tool_input": { "file_path": rel } });
        run_hook(&self.0, &input.to_string())
    }

    /// The same, for an `Edit` that wrote `new_string`.
    fn edited_with(&self, rel: &str, new_string: &str) -> String {
        let input = json!({
            "cwd": self.0,
            "tool_input": { "file_path": rel, "old_string": "x", "new_string": new_string },
        });
        run_hook(&self.0, &input.to_string())
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Every path through the hook must exit 0: a failing hook would disturb the agent.
fn run_hook(dir: &Path, stdin: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rustgrade"))
        .arg("hook")
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run rustgrade");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait");
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).expect("utf-8")
}

#[test]
fn findings_go_back_to_the_agent() {
    let p = Project::new("findings");
    p.write("src/lib.rs", UNWRAP);
    let response: Value = serde_json::from_str(&p.edited("src/lib.rs")).expect("JSON");
    assert_eq!(response["decision"], "block");
    let reason = response["reason"].as_str().expect("reason");
    assert!(reason.contains("unwrap-outside-tests"), "{reason}");
    assert!(reason.contains("src/lib.rs:2"), "{reason}");
}

#[test]
fn absolute_paths_work_too() {
    let p = Project::new("absolute");
    p.write("src/lib.rs", UNWRAP);
    let absolute = p.0.join("src").join("lib.rs");
    assert!(
        p.edited(absolute.to_str().expect("utf-8"))
            .contains("unwrap-outside-tests")
    );
}

#[test]
fn clean_files_say_nothing() {
    let p = Project::new("clean");
    p.write("src/lib.rs", "pub fn f() -> u8 {\n    1\n}\n");
    assert_eq!(p.edited("src/lib.rs"), "");
}

#[test]
fn low_findings_do_not_interrupt() {
    let p = Project::new("low");
    p.write(
        "src/lib.rs",
        "pub fn f(s: &String) -> usize {\n    s.len()\n}\n",
    );
    assert_eq!(p.edited("src/lib.rs"), "");
}

#[test]
fn other_files_are_ignored() {
    let p = Project::new("other");
    p.write("notes.md", UNWRAP);
    assert_eq!(p.edited("notes.md"), "");
}

#[test]
fn honors_the_projects_config() {
    let p = Project::new("config");
    p.write("rustgrade.toml", "allow = [\"unwrap-outside-tests\"]\n");
    p.write("src/lib.rs", UNWRAP);
    assert_eq!(p.edited("src/lib.rs"), "");

    let p = Project::new("exclude");
    p.write("rustgrade.toml", "exclude = [\"src/gen/\"]\n");
    p.write("src/gen/out.rs", UNWRAP);
    assert_eq!(p.edited("src/gen/out.rs"), "");
    p.write("src/lib.rs", UNWRAP);
    assert!(p.edited("src/lib.rs").contains("unwrap-outside-tests"));
}

#[test]
fn skips_build_output() {
    let p = Project::new("target");
    p.write("target/debug/build/out.rs", UNWRAP);
    assert_eq!(p.edited("target/debug/build/out.rs"), "");
}

#[test]
fn caps_what_it_shows() {
    let p = Project::new("cap");
    let body = "    x.unwrap();\n".repeat(30);
    p.write(
        "src/lib.rs",
        &format!("pub fn f(x: Option<u8>) {{\n{body}}}\n"),
    );
    let response: Value = serde_json::from_str(&p.edited("src/lib.rs")).expect("JSON");
    let reason = response["reason"].as_str().expect("reason");
    assert!(reason.contains("30 issue(s)"), "{reason}");
    assert!(reason.contains("and 20 more"), "{reason}");
    assert_eq!(reason.matches("[unwrap-outside-tests]").count(), 10);
}

const LEGACY: &str = "pub fn old(x: Option<u8>) -> u8 {\n    x.unwrap()\n}\n\n";

#[test]
fn an_edit_is_not_blamed_for_old_problems() {
    let p = Project::new("legacy");
    let clean = "pub fn new() -> u8 {\n    1\n}\n";
    p.write("src/lib.rs", &format!("{LEGACY}{clean}"));
    assert_eq!(p.edited_with("src/lib.rs", clean), "");
}

#[test]
fn an_edit_hears_only_about_what_it_wrote() {
    let p = Project::new("new-finding");
    let added = "pub fn new(y: Option<u8>) -> u8 {\n    y.unwrap()\n}\n";
    p.write("src/lib.rs", &format!("{LEGACY}{added}"));
    let response: Value = serde_json::from_str(&p.edited_with("src/lib.rs", added)).expect("JSON");
    let reason = response["reason"].as_str().expect("reason");
    assert!(reason.contains("1 issue(s)"), "{reason}");
    assert!(reason.contains("src/lib.rs:6"), "{reason}");
}

#[test]
fn replace_all_covers_every_occurrence() {
    let p = Project::new("replace-all");
    let line = "    x.unwrap();\n";
    p.write(
        "src/lib.rs",
        &format!("pub fn a(x: Option<u8>) {{\n{line}}}\npub fn b(x: Option<u8>) {{\n{line}}}\n"),
    );
    assert!(p.edited_with("src/lib.rs", line).contains("2 issue(s)"));
}

#[test]
fn crlf_files_still_match() {
    let p = Project::new("crlf");
    let clean = "pub fn new() -> u8 {\n    1\n}\n";
    let file = format!("{LEGACY}{clean}").replace('\n', "\r\n");
    p.write("src/lib.rs", &file);
    assert_eq!(p.edited_with("src/lib.rs", clean), "");
}

#[test]
fn a_deletion_reports_nothing() {
    let p = Project::new("deletion");
    p.write("src/lib.rs", LEGACY);
    assert_eq!(p.edited_with("src/lib.rs", ""), "");
}

#[test]
fn text_rewritten_by_a_formatter_falls_back_to_the_whole_file() {
    let p = Project::new("formatted");
    p.write("src/lib.rs", LEGACY);
    assert!(
        p.edited_with("src/lib.rs", "pub fn old( x:Option<u8> )")
            .contains("unwrap-outside-tests")
    );
}

#[test]
fn never_fails_on_bad_input() {
    let p = Project::new("bad");
    assert_eq!(run_hook(&p.0, "not json"), "");
    assert_eq!(run_hook(&p.0, "{}"), "");
    assert_eq!(p.edited("src/missing.rs"), "");
    p.write("src/broken.rs", "fn {");
    assert_eq!(p.edited("src/broken.rs"), "");
    p.write("rustgrade.toml", "not toml at all [");
    p.write("src/lib.rs", UNWRAP);
    assert_eq!(p.edited("src/lib.rs"), "");
}
