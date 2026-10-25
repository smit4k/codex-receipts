use super::parser::{self, Metadata};
use anyhow::{Context, Result, bail, ensure};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct Session {
    pub path: PathBuf,
    pub meta: Metadata,
}

pub fn discover(root: &Path) -> Result<Vec<Session>> {
    let mut sessions = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.with_context(|| format!("searching {}", root.display()))?;
        let path = entry.path();
        let rollout = path.extension().is_some_and(|ext| ext == "jsonl")
            || (path.extension().is_some_and(|ext| ext == "zst")
                && path.file_stem().is_some_and(|stem| {
                    Path::new(stem)
                        .extension()
                        .is_some_and(|ext| ext == "jsonl")
                }));
        if !entry.file_type().is_file() || !rollout {
            continue;
        }
        match parser::metadata(entry.path()) {
            Ok(meta) => sessions.push(Session {
                path: entry.into_path(),
                meta,
            }),
            Err(error) => eprintln!("warning: skipping {}: {error:#}", entry.path().display()),
        }
    }
    sessions.sort_by(|a, b| {
        b.meta
            .timestamp
            .cmp(&a.meta.timestamp)
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(sessions)
}

pub fn select(sessions: &[Session], id: Option<&str>) -> Result<PathBuf> {
    let Some(id) = id else {
        return sessions
            .first()
            .map(|s| s.path.clone())
            .context("no Codex sessions found");
    };
    ensure!(!id.is_empty(), "session ID must not be empty");
    if let Some(session) = sessions.iter().find(|s| s.meta.id == id) {
        return Ok(session.path.clone());
    }
    let mut matches = sessions.iter().filter(|s| s.meta.id.starts_with(id));
    let Some(session) = matches.next() else {
        bail!("no session matching {id}");
    };
    if matches.next().is_some() {
        bail!("ambiguous session ID {id}; use a longer prefix");
    }
    Ok(session.path.clone())
}
