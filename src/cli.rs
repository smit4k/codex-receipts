use crate::render::Paper;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "A receipt for your Codex session.",
    before_help = "CODEX / RECEIPTS",
    after_help = "Try it:\n  codex-receipts\n  codex-receipts export --format html -o receipt.html"
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
    /// Paper width in mm (32 / 42 text columns)
    #[arg(long, global = true, value_enum, default_value = "80")]
    pub paper: Paper,
    #[command(subcommand)]
    pub command: Option<Command>,
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
    /// Export a receipt; existing files are never overwritten
    Export {
        #[arg(long, value_enum, default_value = "text")]
        format: Format,
        /// Output file, or - for stdout
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Read a Codex SessionEnd JSON event from stdin and export HTML
    Hook {
        #[arg(long)]
        output_dir: PathBuf,
    },
}
