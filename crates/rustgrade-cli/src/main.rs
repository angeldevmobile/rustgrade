use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use rustgrade_core::Severity;

mod cmd;
mod output;

/// Exit codes are part of the CLI contract.
pub const EXIT_OK: u8 = 0;
pub const EXIT_FINDINGS: u8 = 1;
pub const EXIT_TOOL_ERROR: u8 = 2;

#[derive(Parser)]
#[command(name = "rustgrade", version, about = "Systems-level Rust review for AI coding agents and for CI")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(long, global = true, value_enum, default_value_t = Format::Text)]
    format: Format,

    #[arg(long, global = true, value_enum)]
    min_severity: Option<SeverityArg>,

    /// Exit with code 1 if any finding is at or above this severity.
    // TODO: also accept a grade (A-F) once `audit` grades.
    #[arg(long, global = true, value_enum)]
    fail_on: Option<SeverityArg>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the rule engine and print findings by severity.
    Scan { path: Option<PathBuf> },
    /// Scan + clippy pedantic + tests, graded A-F.
    Audit { path: Option<PathBuf> },
    /// Install the skill and commands for one or every agent.
    Install {
        #[arg(long)]
        agent: String,
        #[arg(long)]
        with_mcp: bool,
    },
    /// Remove what `install` wrote.
    Uninstall {
        #[arg(long)]
        agent: String,
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

/// Global options passed down to every command.
pub struct Globals {
    pub format: Format,
    pub min_severity: Option<Severity>,
    pub fail_on: Option<Severity>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let globals = Globals {
        format: cli.format,
        min_severity: cli.min_severity.map(Into::into),
        fail_on: cli.fail_on.map(Into::into),
    };

    let result = match cli.command {
        Command::Scan { path } => cmd::scan::run(path.as_deref(), &globals),
        Command::Rules { explain } => cmd::rules::run(explain.as_deref(), &globals),
        Command::Audit { .. } => cmd::not_yet("audit"),
        Command::Install { .. } => cmd::not_yet("install"),
        Command::Uninstall { .. } => cmd::not_yet("uninstall"),
        Command::Doctor => cmd::not_yet("doctor"),
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
