use crate::render::Paper;
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Months, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use std::{ops::Range, path::PathBuf};

#[derive(Parser)]
#[command(
    version,
    about = "A receipt for your Codex sessions.",
    before_help = "CODEX / RECEIPTS",
    after_help = "Try it:\n  codex-receipts\n  codex-receipts --period week\n  codex-receipts print --period week\n  codex-receipts export\n  codex-receipts export --period month -o month.html"
)]
pub struct Cli {
    /// Override $CODEX_HOME/sessions or ~/.codex/sessions
    #[arg(long, global = true)]
    pub sessions_dir: Option<PathBuf>,
    /// Read a rollout directly instead of discovering sessions
    #[arg(long, global = true, conflicts_with_all = ["session", "latest"])]
    pub file: Option<PathBuf>,
    /// Select a session by ID or unique prefix
    #[arg(long, global = true, conflicts_with = "latest")]
    pub session: Option<String>,
    /// Select the latest session (the default)
    #[arg(long, global = true)]
    pub latest: bool,
    /// Combine activity from the past 7 days or one calendar month (UTC)
    #[arg(long, global = true, value_enum, conflicts_with_all = ["file", "session", "latest"])]
    pub period: Option<Period>,
    /// Paper width in mm (32 / 42 text columns)
    #[arg(long, global = true, value_enum, default_value = "80")]
    pub paper: Paper,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Period {
    Week,
    Month,
}

impl Period {
    pub fn range(self, end: DateTime<Utc>) -> Result<Range<DateTime<Utc>>> {
        let start = match self {
            Self::Week => end.checked_sub_signed(Duration::days(7)),
            Self::Month => end.checked_sub_months(Months::new(1)),
        }
        .context("period start is out of range")?;
        Ok(start..end)
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    #[value(alias = "ascii")]
    Text,
    Html,
}

#[derive(Subcommand)]
pub enum Command {
    /// List sessions, newest start time first
    Sessions,
    /// Display a specific session
    Show { id: String },
    /// Open the receipt and the native print dialog
    Print,
    /// Export a receipt; existing files are never overwritten
    Export {
        #[arg(long, value_enum, default_value = "html")]
        format: Format,
        /// Output file (default: ./receipt.html, or ./receipt.txt for text), or - for stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Read a Codex SessionEnd JSON event from stdin and export HTML
    Hook {
        #[arg(long)]
        output_dir: PathBuf,
    },
}
