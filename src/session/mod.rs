pub mod discovery;
pub mod parser;

use crate::receipt::{Receipt, Tokens};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Duration, Utc};
use std::{collections::HashSet, ops::Range};

pub fn summarize(sessions: &[discovery::Session], range: Range<DateTime<Utc>>) -> Result<Receipt> {
    let mut summary = Receipt {
        session_id: String::new(),
        session_count: Some(0),
        project: "All projects".into(),
        started_at: range.start,
        ended_at: range.end,
        active_duration: Some(Duration::zero()),
        models: Vec::new(),
        total: Some(Tokens::default()),
        warnings: Vec::new(),
    };
    let mut ids = HashSet::new();
    // shortcut: forked histories may overlap; deduplicate when rollout lineage is supported.
    for session in sessions {
        let Some(receipt) = parser::parse_in_period(&session.path, &range)? else {
            continue;
        };
        ensure!(
            ids.insert(receipt.session_id),
            "duplicate session ID in period: {}",
            session.meta.id
        );
        summary.total = match (summary.total, receipt.total) {
            (Some(mut total), Some(tokens)) => {
                total.add(tokens)?;
                Some(total)
            }
            _ => None,
        };
        summary.active_duration = match (summary.active_duration, receipt.active_duration) {
            (Some(total), Some(active)) => Some(
                total
                    .checked_add(&active)
                    .context("active duration overflow")?,
            ),
            _ => None,
        };
        for usage in receipt.models {
            summary.add_usage(&usage.model, usage.tokens)?;
        }
        for warning in receipt.warnings {
            if !summary.warnings.contains(&warning) {
                summary.warnings.push(warning);
            }
        }
    }
    ensure!(
        !ids.is_empty(),
        "no Codex session activity found in requested period"
    );
    summary.session_count = Some(ids.len());
    Ok(summary)
}
