#!/usr/bin/env python3
"""Does rustgrade make a coding agent write better Rust? Runs Claude Code
headless on the tasks in tasks/ under three conditions and measures the result
with checks rustgrade does not control: hidden tests and clippy lints.

    A  baseline: no skill, no CLI
    B  skill and commands in .claude/, no `rustgrade` on PATH
    C  skill, commands and the CLI (what `rustgrade install` gives a user)
    D  C plus a short rustgrade section in the project's CLAUDE.md

Usage, from the repo root (needs `cargo build --release` and `claude` on PATH):

    python evals/agents/run.py check                    # references pass the hidden tests
    python evals/agents/run.py run --reps 1             # every task x condition
    python evals/agents/run.py run --tasks frames --conditions A,C
    python evals/agents/run.py report > evals/agents/RESULTS.md

Runs live in target/agent-evals/<run id>/ (transcript, final src/, metrics).
Each one works in a fresh copy of the task's starter crate outside the repo, so
no project or user configuration leaks in. Standard library only.
"""

import argparse
import collections
import datetime
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
TASKS = HERE / "tasks"
RUNS = ROOT / "target" / "agent-evals"
EXE = ".exe" if sys.platform == "win32" else ""
RUSTGRADE = ROOT / "target" / "release" / f"rustgrade{EXE}"
CONDITIONS = {
    "A": "baseline (no skill, no CLI)",
    "B": "skill only (no CLI)",
    "C": "skill + CLI",
    "D": "skill + CLI + CLAUDE.md section",
}
# What condition D adds to CLAUDE.md: the section `rustgrade install` writes,
# from the same file. Project instructions are read on every request, while a
# skill only loads when the model decides its description applies.
BEGIN = "<!-- rustgrade:begin (managed by `rustgrade install`) -->"
BODY = (ROOT / "targets" / "instructions.md").read_text(encoding="utf-8").rstrip()
PROJECT_INSTRUCTIONS = f"{BEGIN}\n{BODY}\n<!-- rustgrade:end -->\n"
# Independent of rustgrade: clippy's own lints for the same risks.
LINTS = [
    "unwrap_used",
    "expect_used",
    "panic",
    "indexing_slicing",
    "await_holding_lock",
    "undocumented_unsafe_blocks",
    "redundant_clone",
    "ptr_arg",
]
TOOLS = [
    "Read", "Write", "Edit", "Glob", "Grep", "Skill", "TodoWrite",
    "Bash(cargo *)", "Bash(cargo:*)", "Bash(rustgrade *)", "Bash(rustgrade:*)",
    "PowerShell(cargo *)", "PowerShell(rustgrade *)",
]
AGENT_TIMEOUT = 25 * 60


def tasks():
    return sorted(p.name for p in TASKS.iterdir() if (p / "prompt.md").is_file())


def sh(cmd, cwd, env=None, timeout=600):
    """Runs a command; returns (exit code, stdout + stderr). Never raises on failure."""
    try:
        out = subprocess.run(
            cmd, cwd=cwd, env=env, capture_output=True, text=True,
            encoding="utf-8", errors="replace", timeout=timeout,
        )
        return out.returncode, out.stdout + out.stderr
    except subprocess.TimeoutExpired as e:
        return -1, f"timeout after {timeout}s: {e}"


def copy_starter(task, dest):
    shutil.copytree(TASKS / task / "starter", dest)


def install_hidden_tests(task, crate):
    """Copies hidden/hidden_*.rs into tests/ and support folders beside them."""
    tests = crate / "tests"
    tests.mkdir(exist_ok=True)
    files = []
    for p in sorted((TASKS / task / "hidden").iterdir()):
        if p.is_dir():
            shutil.copytree(p, tests / p.name, dirs_exist_ok=True)
        elif p.name.startswith("hidden_"):
            shutil.copy(p, tests / p.name)
            files.append(p)
    return files


TEST_RESULT = re.compile(r"test result: \w+\. (\d+) passed; (\d+) failed")
TEST_ATTR = re.compile(r"#\[(tokio::)?test")


def run_hidden_tests(task, crate, env):
    """Per hidden file: passed, failed, and whether it compiled."""
    results = {}
    for f in install_hidden_tests(task, crate):
        code, out = sh(["cargo", "test", "--test", f.stem, "--no-fail-fast"], crate, env, 600)
        m = TEST_RESULT.search(out)
        total = len(TEST_ATTR.findall(f.read_text(encoding="utf-8")))
        if m:
            results[f.stem] = {"passed": int(m[1]), "failed": int(m[2]), "compiled": True}
        else:
            results[f.stem] = {"passed": 0, "failed": total, "compiled": False, "log": out[-3000:]}
    return results


def clippy_counts(crate, env):
    args = ["cargo", "clippy", "--lib", "--message-format=json", "--", "-A", "clippy::all"]
    for lint in LINTS:
        args += ["-W", f"clippy::{lint}"]
    _, out = sh(args, crate, env)
    counts = collections.Counter()
    for line in out.splitlines():
        if not line.startswith("{"):
            continue
        msg = json.loads(line)
        code = ((msg.get("message") or {}).get("code") or {}).get("code") or ""
        if msg.get("reason") == "compiler-message" and code.startswith("clippy::"):
            counts[code.removeprefix("clippy::")] += 1
    return dict(counts)


def rustgrade_counts(crate):
    _, out = sh([str(RUSTGRADE), "scan", str(crate / "src"), "--format", "json"], crate)
    try:
        findings = json.loads(out)["findings"]
    except (json.JSONDecodeError, KeyError):
        return {"error": out[-500:]}
    return {
        "by_severity": dict(collections.Counter(f["severity"] for f in findings)),
        "by_rule": dict(collections.Counter(f["rule"] for f in findings)),
    }


def toolchain_env(with_cli, bin_dir):
    """PATH without ~/.cargo/bin (where an installed `rustgrade` lives), with
    the stable toolchain's own bin dir instead, plus `bin_dir` for condition C."""
    _, cargo = sh(["rustup", "which", "cargo"], ROOT)
    toolchain_bin = str(Path(cargo.strip()).parent)
    cargo_home_bin = os.path.normcase(str(Path.home() / ".cargo" / "bin"))
    path = [p for p in os.environ["PATH"].split(os.pathsep)
            if os.path.normcase(p.rstrip("\\/")) != cargo_home_bin]
    path.insert(0, toolchain_bin)
    if with_cli:
        path.insert(0, str(bin_dir))
    env = dict(os.environ, PATH=os.pathsep.join(path))
    env.pop("RUSTGRADE", None)
    return env


def install_skill(crate):
    """What `rustgrade install --agent claude` writes, without touching the
    user's install manifest."""
    shutil.copytree(ROOT / "skills" / "rustgrade", crate / ".claude" / "skills" / "rustgrade")
    commands = crate / ".claude" / "commands"
    commands.mkdir(parents=True)
    for p in (ROOT / "commands").glob("*.md"):
        shutil.copy(p, commands / p.name)


def summarize_transcript(lines):
    """Tool use and the final result line of a stream-json transcript."""
    tools = collections.Counter()
    skill_used = cli_used = False
    result = {}
    for line in lines:
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(msg, dict):
            continue
        if msg.get("type") == "result":
            result = msg
        message = msg.get("message")
        content = message.get("content") if isinstance(message, dict) else None
        for block in content if isinstance(content, list) else []:
            if not isinstance(block, dict) or block.get("type") != "tool_use":
                continue
            tools[block.get("name")] += 1
            text = json.dumps(block.get("input"))
            skill_used |= block.get("name") == "Skill" and "rustgrade" in text
            skill_used |= ".claude/skills/rustgrade" in text.replace("\\\\", "/")
            cli_used |= block.get("name") in ("Bash", "PowerShell") and "rustgrade" in text
    return {
        "tools": dict(tools),
        "skill_used": skill_used,
        "cli_used": cli_used,
        "is_error": result.get("is_error"),
        "duration_s": round((result.get("duration_ms") or 0) / 1000),
        "cost_usd": result.get("total_cost_usd"),
        "turns": result.get("num_turns"),
    }


def run_one(task, cond, rep, model, run_dir, claude):
    name = f"{task}-{cond}-{rep}"
    out_dir = run_dir / name
    out_dir.mkdir(parents=True)
    work = Path(tempfile.mkdtemp(prefix=f"rg-eval-{name}-"))
    crate = work / task
    copy_starter(task, crate)
    if cond in ("B", "C", "D"):
        install_skill(crate)
    if cond == "D":
        (crate / "CLAUDE.md").write_text(PROJECT_INSTRUCTIONS, encoding="utf-8")
    env = toolchain_env(cond in ("C", "D"), run_dir / "bin")
    env["CARGO_TARGET_DIR"] = str(run_dir / f"target-{task}")

    prompt = (TASKS / task / "prompt.md").read_text(encoding="utf-8")
    cmd = [
        claude, "-p", prompt, "--model", model,
        "--output-format", "stream-json", "--verbose",
        "--setting-sources", "project", "--strict-mcp-config", "--no-session-persistence",
        "--permission-mode", "dontAsk", "--allowedTools", ",".join(TOOLS),
    ]
    print(f"[{name}] agent running in {crate}", file=sys.stderr)
    started = time.time()
    code, transcript = sh(cmd, crate, env, AGENT_TIMEOUT)
    (out_dir / "transcript.jsonl").write_text(transcript, encoding="utf-8")
    agent = summarize_transcript(transcript.splitlines())
    agent["exit_code"] = code
    agent["wall_s"] = round(time.time() - started)

    shutil.copytree(crate / "src", out_dir / "src")
    print(f"[{name}] measuring", file=sys.stderr)
    build_code, build_log = sh(["cargo", "build", "--quiet"], crate, env)
    metrics = {
        "task": task, "condition": cond, "rep": rep, "model": model,
        "agent": agent,
        "builds": build_code == 0,
        "clippy": clippy_counts(crate, env) if build_code == 0 else None,
        "rustgrade": rustgrade_counts(crate),
        "hidden": run_hidden_tests(task, crate, env),
    }
    if build_code != 0:
        metrics["build_log"] = build_log[-3000:]
    (out_dir / "metrics.json").write_text(json.dumps(metrics, indent=2), encoding="utf-8")
    shutil.rmtree(work, ignore_errors=True)
    passed = sum(h["passed"] for h in metrics["hidden"].values())
    total = passed + sum(h["failed"] for h in metrics["hidden"].values())
    print(f"[{name}] hidden tests {passed}/{total}, skill={agent['skill_used']} "
          f"cli={agent['cli_used']}, ${agent['cost_usd']}", file=sys.stderr)


def cmd_check(_args):
    """Each reference passes its hidden tests, and each starter compiles."""
    env = dict(os.environ)
    ok = True
    for task in tasks():
        with tempfile.TemporaryDirectory(prefix=f"rg-check-{task}-") as tmp:
            crate = Path(tmp) / task
            copy_starter(task, crate)
            env["CARGO_TARGET_DIR"] = str(RUNS / "check-target" / task)
            code, log = sh(["cargo", "build", "--quiet"], crate, env)
            if code != 0:
                ok = False
                print(f"{task}: starter does not build\n{log[-2000:]}")
                continue
            shutil.copy(TASKS / task / "reference" / "lib.rs", crate / "src" / "lib.rs")
            for test, r in run_hidden_tests(task, crate, env).items():
                good = r["compiled"] and r["failed"] == 0 and r["passed"] > 0
                ok &= good
                print(f"{task}/{test}: {r['passed']} passed, {r['failed']} failed"
                      + ("" if good else f"  <-- FAIL\n{r.get('log', '')}"))
    sys.exit(0 if ok else 1)


def cmd_run(args):
    claude = shutil.which("claude") or str(Path.home() / ".local" / "bin" / f"claude{EXE}")
    if not RUSTGRADE.is_file():
        sys.exit(f"build the CLI first: cargo build --release ({RUSTGRADE} missing)")
    run_id = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    run_dir = RUNS / run_id
    (run_dir / "bin").mkdir(parents=True)
    shutil.copy(RUSTGRADE, run_dir / "bin" / RUSTGRADE.name)
    selected = args.tasks.split(",") if args.tasks else tasks()
    for rep in range(1, args.reps + 1):
        for task in selected:
            for cond in args.conditions.split(","):
                run_one(task, cond, rep, args.model, run_dir, claude)
    print(f"done: {run_dir}", file=sys.stderr)


def load_runs(run_ids):
    dirs = [RUNS / r for r in run_ids] if run_ids else [
        d for d in sorted(RUNS.iterdir()) if d.is_dir() and d.name[:1].isdigit()
    ][-1:]
    return [json.loads(p.read_text(encoding="utf-8"))
            for d in dirs for p in sorted(d.glob("*/metrics.json"))]


def cmd_report(args):
    runs = load_runs(args.runs)
    if not runs:
        sys.exit("no runs found")
    groups = collections.defaultdict(list)
    for r in runs:
        groups[(r["task"], r["condition"])].append(r)

    def mean(xs):
        xs = [x for x in xs if x is not None]
        return sum(xs) / len(xs) if xs else 0

    def hidden(r, prefix=""):
        hs = [h for name, h in r["hidden"].items() if name.startswith("hidden_" + prefix)]
        passed = sum(h["passed"] for h in hs)
        return passed, passed + sum(h["failed"] for h in hs)

    models = sorted({r["model"] for r in runs})
    reps = max(r["rep"] for r in runs)
    out = ["# Agent evals: does rustgrade improve the Rust an agent writes?\n"]
    out.append(f"Claude Code headless, model `{', '.join(models)}`, {reps} run(s) per cell. "
               "Generated by `evals/agents/run.py report`.\n")
    out.append("Conditions: " + "; ".join(f"**{k}** {v}" for k, v in CONDITIONS.items()) + ".\n")
    out.append("Hidden tests and clippy lints are independent of rustgrade; the agent never "
               "sees the tests. Numbers are means over runs.\n")
    out.append("| Task | Cond | Hidden tests | Builds | clippy: unwrap/expect/panic | "
               "indexing | await_holding_lock | undoc. unsafe | rustgrade high+ | "
               "Skill used | CLI used | Cost $ | Time s |")
    out.append("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for (task, cond), rs in sorted(groups.items()):
        passed, total = map(sum, zip(*(hidden(r) for r in rs)))
        lint = lambda *names: mean([sum((r["clippy"] or {}).get(n, 0) for n in names) for r in rs])
        sev = lambda r: sum(v for k, v in r["rustgrade"].get("by_severity", {}).items()
                            if k in ("high", "critical"))
        out.append(
            f"| {task} | {cond} | {passed}/{total} | {mean([r['builds'] for r in rs]):.0%} "
            f"| {lint('unwrap_used', 'expect_used', 'panic'):.1f} | {lint('indexing_slicing'):.1f} "
            f"| {lint('await_holding_lock'):.1f} | {lint('undocumented_unsafe_blocks'):.1f} "
            f"| {mean([sev(r) for r in rs]):.1f} "
            f"| {mean([r['agent']['skill_used'] for r in rs]):.0%} "
            f"| {mean([r['agent']['cli_used'] for r in rs]):.0%} "
            f"| {mean([r['agent']['cost_usd'] for r in rs]):.2f} "
            f"| {mean([r['agent']['duration_s'] for r in rs]):.0f} |"
        )
    out.append("\n## Hidden tests by file\n")
    files = sorted({f for r in runs for f in r["hidden"]})
    out.append("| Task | Cond | " + " | ".join(f"`{f}`" for f in files) + " |")
    out.append("|---|---|" + "---:|" * len(files))
    for (task, cond), rs in sorted(groups.items()):
        cells = []
        for f in files:
            hs = [r["hidden"][f] for r in rs if f in r["hidden"]]
            if not hs:
                cells.append("")
                continue
            p = sum(h["passed"] for h in hs)
            t = p + sum(h["failed"] for h in hs)
            bad = sum(not h["compiled"] for h in hs)
            cells.append(f"{p}/{t}" + (f" ({bad} didn't compile)" if bad else ""))
        out.append(f"| {task} | {cond} | " + " | ".join(cells) + " |")
    print("\n".join(out))


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("check", help="references pass the hidden tests").set_defaults(fn=cmd_check)
    run = sub.add_parser("run", help="run the agent")
    run.add_argument("--tasks", help="comma-separated (default: all)")
    run.add_argument("--conditions", default="A,B,C")
    run.add_argument("--reps", type=int, default=1)
    run.add_argument("--model", default="sonnet")
    run.set_defaults(fn=cmd_run)
    report = sub.add_parser("report", help="Markdown summary of runs")
    report.add_argument("runs", nargs="*", help="run ids (default: the latest)")
    report.set_defaults(fn=cmd_report)
    args = parser.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
