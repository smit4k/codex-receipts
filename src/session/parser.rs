use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::receipt::{ModelUsage, Receipt, Tokens, project_name};

#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub id: String,
    pub cwd: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Deserialize)]
struct Event {
    #[serde(default)]
    timestamp: Option<DateTime<Utc>>,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    payload: Value,
}

pub fn metadata(path: &Path) -> Result<Metadata> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let event: Event = serde_json::from_str(&line).context("invalid session header")?;
    ensure!(event.kind == "session_meta", "missing session_meta header");
    let meta: Metadata =
        serde_json::from_value(event.payload).context("invalid session metadata")?;
    ensure!(!meta.id.trim().is_empty(), "empty session ID");
    Ok(meta)
}

pub fn parse(path: &Path) -> Result<Receipt> {
    let reader =
        BufReader::new(File::open(path).with_context(|| format!("reading {}", path.display()))?);
    parse_reader(reader).with_context(|| format!("parsing {}", path.display()))
}

pub fn parse_reader(mut reader: impl BufRead) -> Result<Receipt> {
    let mut receipt: Option<Receipt> = None;
    let mut current_model = "unknown".to_owned();
    let mut previous = Tokens::default();
    let mut active_start = None;
    let mut active = Duration::zero();
    let mut saw_active = false;
    let mut line = String::new();
    let mut line_number = 0;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        line_number += 1;
        if line.trim().is_empty() {
            continue;
        }
        let event: Event = match serde_json::from_str(&line) {
            Ok(event) => event,
            Err(error) if error.is_eof() && !line.ends_with('\n') => {
                if let Some(r) = receipt.as_mut() {
                    r.warnings
                        .push("Incomplete final log record ignored".into());
                    break;
                }
                return Err(error.into());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("invalid JSON at line {line_number}"));
            }
        };
        if event.kind == "session_meta" {
            ensure!(
                receipt.is_none(),
                "duplicate session metadata at line {line_number}"
            );
            let meta: Metadata =
                serde_json::from_value(event.payload).context("invalid session metadata")?;
            ensure!(!meta.id.trim().is_empty(), "empty session ID");
            let project = project_name(&meta.cwd).to_owned();
            receipt = Some(Receipt {
                session_id: meta.id,
                project,
                started_at: meta.timestamp,
                ended_at: meta.timestamp,
                active_duration: None,
                models: Vec::new(),
                total: None,
                warnings: Vec::new(),
            });
            continue;
        }
        let r = receipt
            .as_mut()
            .context("missing session_meta before events")?;
        if let Some(time) = event.timestamp {
            r.ended_at = r.ended_at.max(time);
        }
        if event.kind == "turn_context" {
            current_model = event
                .payload
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
        }
        if event.kind != "event_msg" {
            continue;
        }
        match event.payload.get("type").and_then(Value::as_str) {
            Some("model_reroute") => {
                current_model = event
                    .payload
                    .get("to_model")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_owned();
            }
            Some("task_started" | "turn_started") => {
                if let Some(time) = event.timestamp {
                    active_start.get_or_insert(time.max(r.started_at));
                    saw_active = true;
                }
            }
            Some("task_complete" | "turn_complete" | "turn_aborted") => {
                if let (Some(time), Some(start)) = (event.timestamp, active_start) {
                    active_start = None;
                    active += (time - start).max(Duration::zero());
                }
            }
            Some("token_count") => {
                let Some(value) = event
                    .payload
                    .pointer("/info/total_token_usage")
                    .filter(|v| !v.is_null())
                else {
                    continue;
                };
                let total: Tokens = serde_json::from_value(value.clone())
                    .with_context(|| format!("invalid token counts at line {line_number}"))?;
                total
                    .validate()
                    .with_context(|| format!("invalid token counts at line {line_number}"))?;
                let delta = total.delta(previous).filter(|d| d.validate().is_ok());
                if let Some(delta) = delta {
                    if delta != Tokens::default() {
                        if let Some(usage) = r
                            .models
                            .iter_mut()
                            .find(|usage| usage.model == current_model)
                        {
                            usage.tokens.add(delta)?;
                        } else {
                            r.models.push(ModelUsage {
                                model: current_model.clone(),
                                tokens: delta,
                            });
                        }
                    }
                } else {
                    // Counter corrections cannot reliably be attributed across models.
                    r.models = vec![ModelUsage {
                        model: "unknown (counter correction)".into(),
                        tokens: total,
                    }];
                    if !r.warnings.iter().any(|w| w.starts_with("Usage counters")) {
                        r.warnings
                            .push("Usage counters corrected; model attribution unavailable".into());
                    }
                }
                previous = total;
                r.total = Some(total);
            }
            _ => {}
        }
    }
    let Some(mut r) = receipt else {
        bail!("no session metadata found");
    };
    if let Some(start) = active_start {
        active += (r.ended_at - start).max(Duration::zero());
        r.warnings
            .push("Turn still open; receipt is a snapshot".into());
    }
    if saw_active {
        r.active_duration = Some(active);
    }
    if r.total.is_none() {
        r.warnings.push("No token usage recorded".into());
    }
    Ok(r)
}
