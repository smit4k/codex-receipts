mod cli;
mod pricing;
mod receipt;
mod render;
mod session;

use anyhow::{Context, Result, ensure};
use clap::Parser;
use cli::{Cli, Command, Format};
use pricing::Pricing;
use receipt::project_name;
use render::{ascii, html, safe_text};
use session::{discovery, parser};
use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
};

fn sessions_dir(cli: &Cli) -> Result<PathBuf> {
    if let Some(path) = &cli.sessions_dir {
        return Ok(path.clone());
    }
    if let Some(home) = env::var_os("CODEX_HOME") {
        return Ok(PathBuf::from(home).join("sessions"));
    }
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .context("set HOME, CODEX_HOME, or --sessions-dir")?;
    Ok(PathBuf::from(home).join(".codex/sessions"))
}

fn output(path: &Path, bytes: &[u8]) -> Result<()> {
    if path == Path::new("-") {
        io::stdout().lock().write_all(bytes)?;
    } else {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| {
                format!(
                    "creating {} (existing files are not overwritten)",
                    path.display()
                )
            })?;
        file.write_all(bytes)
            .with_context(|| format!("writing {}", path.display()))?;
        eprintln!("Saved {}", path.display());
    }
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    ensure!(
        cli.period.is_none() || matches!(cli.command, None | Some(Command::Export { .. })),
        "--period can only be used for display or export"
    );
    let pricing = Pricing::bundled()?;
    if let Some(Command::Hook { output_dir }) = &cli.command {
        #[derive(serde::Deserialize)]
        struct Hook {
            session_id: String,
            transcript_path: PathBuf,
            hook_event_name: String,
        }
        let mut json = String::new();
        io::stdin().take(1_048_577).read_to_string(&mut json)?;
        ensure!(json.len() <= 1_048_576, "hook input exceeds 1 MiB");
        let event: Hook = serde_json::from_str(&json).context("invalid SessionEnd hook input")?;
        ensure!(
            event.hook_event_name == "SessionEnd",
            "expected a SessionEnd event"
        );
        ensure!(
            !event.session_id.is_empty()
                && event
                    .session_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "invalid hook session ID"
        );
        let receipt = parser::parse(&event.transcript_path)?;
        ensure!(
            receipt.session_id == event.session_id,
            "hook session ID does not match transcript"
        );
        fs::create_dir_all(output_dir)?;
        return output(
            &output_dir.join(format!("{}.html", event.session_id)),
            html::render(&receipt, &pricing, cli.paper).as_bytes(),
        );
    }
    if matches!(cli.command, Some(Command::Sessions)) {
        let sessions = discovery::discover(&sessions_dir(&cli)?)?;
        ensure!(!sessions.is_empty(), "no Codex sessions found");
        let stdout = io::stdout();
        let mut out = stdout.lock();
        writeln!(
            out,
            "STARTED (UTC)         SESSION                               PROJECT"
        )?;
        for session in sessions {
            let project = project_name(&session.meta.cwd);
            writeln!(
                out,
                "{}  {}  {}",
                session.meta.timestamp.format("%Y-%m-%d %H:%M:%S"),
                safe_text(&session.meta.id),
                safe_text(project)
            )?;
        }
        return Ok(());
    }
    let show_id = match &cli.command {
        Some(Command::Show { id }) => Some(id.as_str()),
        _ => None,
    };
    ensure!(
        show_id.is_none() || (cli.file.is_none() && cli.session.is_none() && !cli.latest),
        "show <id> cannot be combined with --file, --session, or --latest"
    );
    let receipt = if let Some(period) = cli.period {
        let root = sessions_dir(&cli)?;
        let mut sessions = discovery::discover(&root)?;
        if cli.sessions_dir.is_none() {
            let archived = root.with_file_name("archived_sessions");
            if archived.exists() {
                sessions.extend(discovery::discover(&archived)?);
            }
        }
        session::summarize(&sessions, period.range(chrono::Utc::now())?)?
    } else {
        let path = match &cli.file {
            Some(path) => path.clone(),
            None => discovery::select(
                &discovery::discover(&sessions_dir(&cli)?)?,
                show_id.or(cli.session.as_deref()),
            )?,
        };
        parser::parse(&path)?
    };
    match cli.command {
        None | Some(Command::Show { .. }) => {
            let styled = io::stdout().is_terminal()
                && env::var_os("NO_COLOR").is_none()
                && env::var("TERM").as_deref() != Ok("dumb");
            let text = if styled {
                ascii::terminal(&receipt, &pricing, cli.paper)
            } else {
                ascii::render(&receipt, &pricing, cli.paper)
            };
            output(Path::new("-"), text.as_bytes())
        }
        Some(Command::Export {
            format,
            output: path,
        }) => {
            let text = match format {
                Format::Text => ascii::render(&receipt, &pricing, cli.paper),
                Format::Html => html::render(&receipt, &pricing, cli.paper),
            };
            output(&path, text.as_bytes())
        }
        Some(Command::Sessions | Command::Hook { .. }) => unreachable!("handled above"),
    }
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        if error
            .downcast_ref::<io::Error>()
            .is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe)
        {
            return;
        }
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests;
