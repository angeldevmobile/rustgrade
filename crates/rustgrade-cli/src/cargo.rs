//! Runs `cargo clippy` and `cargo test` for `audit` and parses their output.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use anyhow::Context;
use rustgrade_core::Category;
use serde_json::Value;

/// A clippy warning, already mapped to a rubric category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClippyWarning {
    /// Lint name without the `clippy::` prefix.
    pub lint: String,
    pub category: Category,
}

#[derive(Debug)]
pub enum Clippy {
    Ran(Vec<ClippyWarning>),
    /// The crate does not compile; holds the first error, rendered by rustc.
    CompileError(String),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Tests {
    pub passed: u32,
    pub failed: u32,
    /// `cargo test` exited successfully.
    pub ok: bool,
}

/// Clippy's `perf` group (Rust 1.99). Lints not listed here and not
/// deny-by-default count toward Ownership & API, per the rubric.
const PERF_LINTS: &[&str] = &[
    "box_collection",
    "boxed_local",
    "cmp_owned",
    "collapsible_str_replace",
    "drain_collect",
    "expect_fun_call",
    "extend_with_drain",
    "format_collect",
    "format_in_format_args",
    "iter_overeager_cloned",
    "large_const_arrays",
    "large_enum_variant",
    "manual_memcpy",
    "manual_retain",
    "manual_str_repeat",
    "manual_try_fold",
    "map_entry",
    "missing_const_for_thread_local",
    "missing_spin_loop",
    "needless_collect",
    "readonly_write_lock",
    "redundant_allocation",
    "regex_creation_in_loops",
    "result_large_err",
    "single_char_pattern",
    "sliced_string_as_bytes",
    "slow_vector_initialization",
    "to_string_in_format_args",
    "unnecessary_to_owned",
    "useless_vec",
    "vec_init_then_push",
    "waker_clone_wake",
];

/// `cargo clippy` over every target with pedantic lints, as the rubric asks.
pub fn clippy(manifest: &Path) -> anyhow::Result<Clippy> {
    let output = cargo(
        manifest,
        "clippy",
        &[
            "--workspace",
            "--all-targets",
            "--message-format=json",
            "--",
            "-W",
            "clippy::pedantic",
        ],
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed = parse_clippy(&stdout);
    if let Clippy::Ran(_) = parsed
        && !output.status.success()
        && !stdout.contains("\"compiler-message\"")
    {
        // Failed before rustc ran (bad manifest, missing toolchain component).
        anyhow::bail!(
            "cargo clippy failed:\n{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(parsed)
}

pub fn tests(manifest: &Path) -> anyhow::Result<Tests> {
    let output = cargo(
        manifest,
        "test",
        &["--workspace", "--no-fail-fast", "--color", "never"],
    )?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(parse_tests(&text, output.status.success()))
}

/// Runs cargo against `manifest`. On Windows a concurrent build can hold a
/// file lock (`os error 32`); that is retried once, since it is not a code error.
/// `--manifest-path` goes right after the subcommand: anything after a `--` in
/// `args` belongs to the tool (clippy-driver, the test harness), not to cargo.
fn cargo(manifest: &Path, subcommand: &str, args: &[&str]) -> anyhow::Result<Output> {
    let run = || {
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .arg(subcommand)
            .arg("--manifest-path")
            .arg(manifest)
            .args(args)
            .output()
            .with_context(|| format!("running cargo {subcommand}"))
    };
    let output = run()?;
    if !output.status.success() && String::from_utf8_lossy(&output.stderr).contains("os error 32") {
        std::thread::sleep(Duration::from_secs(2));
        return run();
    }
    Ok(output)
}

/// Parses `--message-format=json` output. Clippy lints are deduplicated by
/// location, since `--all-targets` reports a lint once per target.
fn parse_clippy(stdout: &str) -> Clippy {
    let mut warnings = Vec::new();
    let mut seen = HashSet::new();
    for line in stdout.lines() {
        let Ok(msg) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if msg["reason"] != "compiler-message" {
            continue;
        }
        let msg = &msg["message"];
        let level = msg["level"].as_str().unwrap_or_default();
        let code = msg["code"]["code"].as_str();
        match code.and_then(|c| c.strip_prefix("clippy::")) {
            Some(lint) => {
                // Hash the location instead of owning it: no allocation per message.
                let span = &msg["spans"][0];
                let mut key = DefaultHasher::new();
                (
                    lint,
                    span["file_name"].as_str(),
                    span["line_start"].as_u64(),
                    span["column_start"].as_u64(),
                )
                    .hash(&mut key);
                if seen.insert(key.finish()) {
                    warnings.push(ClippyWarning {
                        lint: lint.to_owned(),
                        category: clippy_category(lint, level),
                    });
                }
            }
            None if level == "error" => {
                let rendered = msg["rendered"].as_str().unwrap_or("compile error");
                return Clippy::CompileError(rendered.trim().to_owned());
            }
            // rustc's own warnings (unused variables, ...) are not graded.
            None => {}
        }
    }
    Clippy::Ran(warnings)
}

/// Deny-by-default clippy lints are the `correctness` group.
fn clippy_category(lint: &str, level: &str) -> Category {
    if level == "error" {
        Category::Errors
    } else if PERF_LINTS.contains(&lint) {
        Category::Performance
    } else {
        Category::ApiDesign
    }
}

/// Sums every `test result:` line (unit, integration and doc tests).
fn parse_tests(text: &str, ok: bool) -> Tests {
    let mut tests = Tests {
        passed: 0,
        failed: 0,
        ok,
    };
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("test result:") else {
            continue;
        };
        for part in rest.split(';') {
            let mut words = part.split_whitespace().rev();
            let (Some(label), Some(count)) = (words.next(), words.next()) else {
                continue;
            };
            let Ok(n) = count.parse::<u32>() else {
                continue;
            };
            match label {
                "passed" => tests.passed += n,
                "failed" => tests.failed += n,
                _ => {}
            }
        }
    }
    tests
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(level: &str, code: Option<&str>, line: u32, rendered: &str) -> String {
        let code = code.map_or(Value::Null, |c| serde_json::json!({ "code": c }));
        serde_json::json!({
            "reason": "compiler-message",
            "message": {
                "level": level,
                "code": code,
                "rendered": rendered,
                "spans": [{ "file_name": "src/lib.rs", "line_start": line, "column_start": 5 }],
            }
        })
        .to_string()
    }

    #[test]
    fn maps_and_dedupes_clippy_lints() {
        let out = [
            msg("warning", Some("clippy::needless_pass_by_value"), 3, ""),
            // Same lint, same place, reported again for the test target.
            msg("warning", Some("clippy::needless_pass_by_value"), 3, ""),
            msg("warning", Some("clippy::useless_vec"), 9, ""),
            msg("error", Some("clippy::absurd_extreme_comparisons"), 12, ""),
            msg("warning", Some("unused_variables"), 4, ""),
            r#"{"reason":"build-finished","success":true}"#.to_owned(),
            "not json".to_owned(),
        ]
        .join("\n");
        let Clippy::Ran(warnings) = parse_clippy(&out) else {
            panic!("expected warnings");
        };
        let got: Vec<_> = warnings
            .iter()
            .map(|w| (w.lint.as_str(), w.category))
            .collect();
        assert_eq!(
            got,
            [
                ("needless_pass_by_value", Category::ApiDesign),
                ("useless_vec", Category::Performance),
                ("absurd_extreme_comparisons", Category::Errors),
            ]
        );
    }

    #[test]
    fn rustc_errors_mean_the_crate_does_not_compile() {
        let out = [
            msg("warning", Some("clippy::useless_vec"), 9, ""),
            msg(
                "error",
                Some("E0433"),
                1,
                "error[E0433]: failed to resolve: use of undeclared crate `tokio`\n",
            ),
        ]
        .join("\n");
        let Clippy::CompileError(e) = parse_clippy(&out) else {
            panic!("expected a compile error");
        };
        assert!(e.starts_with("error[E0433]"), "{e}");
    }

    #[test]
    fn sums_every_test_result_line() {
        let out = "\
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 2 tests
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests demo
test result: ok. 4 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.20s
";
        assert_eq!(
            parse_tests(out, false),
            Tests {
                passed: 8,
                failed: 1,
                ok: false
            }
        );
    }

    #[test]
    fn tests_that_do_not_build_report_zero_counts() {
        let out =
            "error[E0425]: cannot find value `x` in this scope\nerror: could not compile `demo`";
        let t = parse_tests(out, false);
        assert_eq!((t.passed, t.failed, t.ok), (0, 0, false));
    }
}
