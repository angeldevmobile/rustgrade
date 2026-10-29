use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use rustgrade_core::{Grade, Severity};

mod agents;
mod cargo;
mod cmd;
mod output;

/// Exit codes are part of the CLI contract.
pub const EXIT_OK: u8 = 0;
pub const EXIT_FINDINGS: u8 = 1;
pub const EXIT_TOOL_ERROR: u8 = 2;

#[derive(Parser)]
#[command(
    name = "rustgrade",
    version,
    about = "Systems-level Rust review for AI coding agents and for CI"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(long, global = true, value_enum, default_value_t = Format::Text)]
    format: Format,

    #[arg(long, global = true, value_enum)]
    min_severity: Option<SeverityArg>,

    /// Exit with code 1 if any finding is at or above this severity
    /// (info|low|medium|high|critical), or, for `audit`, if the grade is this
    /// letter or worse (A-F).
    #[arg(long, global = true, value_parser = parse_fail_on)]
    fail_on: Option<FailOn>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the rule engine and print findings by severity.
    Scan { path: Option<PathBuf> },
    /// Scan + clippy pedantic + tests, graded A-F.
    Audit {
        path: Option<PathBuf>,
        /// Skip `cargo clippy`.
        #[arg(long)]
        no_clippy: bool,
        /// Skip `cargo test` (the failing-tests cap then cannot apply).
        #[arg(long)]
        no_tests: bool,
    },
    /// Install the skill (and commands, where supported) for one or every agent.
    Install {
        #[command(flatten)]
        target: InstallTarget,
        /// Overwrite files that rustgrade did not write or that were edited.
        #[arg(long)]
        force: bool,
    },
    /// Remove what `install` wrote (edited files are kept unless --force).
    Uninstall {
        #[command(flatten)]
        target: InstallTarget,
        /// Also remove files edited since install.
        #[arg(long)]
        force: bool,
    },
    /// Show detected agents, installed versions and toolchain.
    Doctor,
    /// List rules, or explain one.
    Rules {
        #[arg(long)]
        explain: Option<String>,
    },
    /// Write a `rustgrade.toml` with defaults.
    Init,
    /// Start the MCP server on stdio (requires `--features mcp`).
    #[cfg(feature = "mcp")]
    Mcp,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    Text,
    Json,
    Sarif,
    Markdown,
}

#[derive(Clone, Copy, ValueEnum)]
enum SeverityArg {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl From<SeverityArg> for Severity {
    fn from(s: SeverityArg) -> Self {
        match s {
            SeverityArg::Info => Self::Info,
            SeverityArg::Low => Self::Low,
            SeverityArg::Medium => Self::Medium,
            SeverityArg::High => Self::High,
            SeverityArg::Critical => Self::Critical,
        }
    }
}

/// Shared `install` / `uninstall` options.
#[derive(clap::Args)]
struct InstallTarget {
    /// Agent from targets/targets.toml (claude, codex, cursor, gemini, copilot,
    /// windsurf), or `all` for every agent detected on this machine.
    #[arg(long)]
    agent: String,
    #[arg(long, value_enum, default_value_t = agents::Scope::Project)]
    scope: agents::Scope,
    /// Project directory for `--scope project` (default: current directory).
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Show what would change without writing anything.
    #[arg(long)]
    dry_run: bool,
}

impl InstallTarget {
    fn options(&self, force: bool) -> cmd::install::Options<'_> {
        cmd::install::Options {
            agent: &self.agent,
            scope: self.scope,
            dir: self.dir.as_deref(),
            force,
            dry_run: self.dry_run,
        }
    }
}

/// `--fail-on` threshold.
#[derive(Clone, Copy, Debug)]
pub enum FailOn {
    Severity(Severity),
    /// Fail when the audit grade is this letter or worse.
    Grade(Grade),
}

fn parse_fail_on(s: &str) -> Result<FailOn, String> {
    let severity = match s.to_ascii_lowercase().as_str() {
        "info" => Severity::Info,
        "low" => Severity::Low,
        "medium" => Severity::Medium,
        "high" => Severity::High,
        "critical" => Severity::Critical,
        "a" => return Ok(FailOn::Grade(Grade::A)),
        "b" => return Ok(FailOn::Grade(Grade::B)),
        "c" => return Ok(FailOn::Grade(Grade::C)),
        "d" => return Ok(FailOn::Grade(Grade::D)),
        "f" => return Ok(FailOn::Grade(Grade::F)),
        _ => {
            return Err(
                "expected a severity (info|low|medium|high|critical) or a grade (A-F)".into(),
            );
        }
    };
    Ok(FailOn::Severity(severity))
}

/// Global options passed down to every command.
pub struct Globals {
    pub format: Format,
    pub min_severity: Option<Severity>,
    pub fail_on: Option<FailOn>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let globals = Globals {
        format: cli.format,
        min_severity: cli.min_severity.map(Into::into),
        fail_on: cli.fail_on,
    };

    let result = match cli.command {
        Command::Scan { path } => cmd::scan::run(path.as_deref(), &globals),
        Command::Rules { explain } => cmd::rules::run(explain.as_deref(), &globals),
        Command::Audit {
            path,
            no_clippy,
            no_tests,
        } => cmd::audit::run(
            path.as_deref(),
            &cmd::audit::Options {
                clippy: !no_clippy,
                tests: !no_tests,
            },
            &globals,
        ),
        Command::Install { target, force } => cmd::install::install(&target.options(force)),
        Command::Uninstall { target, force } => cmd::install::uninstall(&target.options(force)),
        Command::Doctor => cmd::doctor::run(),
        Command::Init => cmd::not_yet("init"),
        #[cfg(feature = "mcp")]
        Command::Mcp => rustgrade_mcp::serve_stdio().map(|()| EXIT_OK),
    };

    match result {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("rustgrade: {e:#}");
            ExitCode::from(EXIT_TOOL_ERROR)
        }
    }
}
