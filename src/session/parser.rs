use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader},
    ops::Range,
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
    pub forked_from_id: Option<String>,
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

fn open(path: &Path) -> Result<Box<dyn BufRead>> {
    let file = File::open(path).with_context(|| format!("reading {}", path.display()))?;
    if path.extension().is_some_and(|ext| ext == "zst") {
        let decoder = zstd::stream::read::Decoder::new(file)
            .with_context(|| format!("decompressing {}", path.display()))?;
        Ok(Box::new(BufReader::new(decoder)))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

pub fn metadata(path: &Path) -> Result<Metadata> {
    let mut reader = open(path)?;
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
    parse_reader(open(path)?).with_context(|| format!("parsing {}", path.display()))
}

pub fn parse_reader(reader: impl BufRead) -> Result<Receipt> {
    parse_reader_in_period(reader, None)?.context("no session metadata found")
}

pub fn parse_in_period(path: &Path, range: &Range<DateTime<Utc>>) -> Result<Option<Receipt>> {
    parse_reader_in_period(open(path)?, Some(range))
        .with_context(|| format!("parsing {}", path.display()))
}

pub fn parse_reader_in_period(
    mut reader: impl BufRead,
    range: Option<&Range<DateTime<Utc>>>,
) -> Result<Option<Receipt>> {
    let mut receipt: Option<Receipt> = None;
    let mut session_ids = HashSet::new();
    let mut current_model = "unknown".to_owned();
    let mut previous = Tokens::default();
    let mut active_start = None;
    let mut active = Duration::zero();
    let mut saw_active = false;
    let mut saw_event = false;
    let mut saw_usage = false;
    let mut incomplete_usage = false;
    let clipped_duration = |start: DateTime<Utc>, end: DateTime<Utc>| {
        let (start, end) = range.map_or((start, end), |range| {
            (start.max(range.start), end.min(range.end))
        });
        (end - start).max(Duration::zero())
    };
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
            let meta: Metadata =
                serde_json::from_value(event.payload).context("invalid session metadata")?;
            ensure!(!meta.id.trim().is_empty(), "empty session ID");
            if receipt.is_some() {
                ensure!(
                    session_ids.contains(&meta.id),
                    "unrelated session metadata at line {line_number}"
                );
                session_ids.extend(meta.forked_from_id);
                continue;
            }
            // Forked rollouts can embed ancestor headers; the first header owns the receipt.
            session_ids.insert(meta.id.clone());
            session_ids.extend(meta.forked_from_id);
            let project = project_name(&meta.cwd).to_owned();
            receipt = Some(Receipt {
                session_id: meta.id,
                session_count: None,
                project,
                started_at: meta.timestamp,
                ended_at: meta.timestamp,
                active_duration: None,
                models: Vec::new(),
                total: None,
                warnings: Vec::new(),
            });
            saw_event = range.is_none_or(|range| range.contains(&meta.timestamp));
            continue;
        }
        let r = receipt
            .as_mut()
            .context("missing session_meta before events")?;
        if let Some(time) = event.timestamp {
            r.ended_at = r.ended_at.max(time);
        }
        let in_period =
            range.is_none_or(|range| event.timestamp.is_some_and(|time| range.contains(&time)));
        saw_event |= in_period;
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
                    saw_active |= in_period;
                }
            }
            Some("task_complete" | "turn_complete" | "turn_aborted") => {
                if let (Some(time), Some(start)) = (event.timestamp, active_start) {
                    active_start = None;
                    let duration = clipped_duration(start, time);
                    active += duration;
                    saw_active |= in_period || duration > Duration::zero();
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
                if !in_period {
                    if range.is_some() && event.timestamp.is_none() {
                        incomplete_usage = true;
                        if !r.warnings.iter().any(|w| w.starts_with("Untimed")) {
                            r.warnings
                                .push("Untimed usage cannot be assigned to the period".into());
                        }
                    }
                    previous = total;
                    saw_usage = true;
                    continue;
                }
                let delta = total.delta(previous).filter(|d| d.validate().is_ok());
                if range.is_some_and(|range| r.started_at < range.start) && !saw_usage {
                    incomplete_usage = true;
                    r.warnings
                        .push("No usage baseline at period start; total unavailable".into());
                } else if let Some(delta) = delta {
                    if range.is_some() {
                        r.total.get_or_insert_default().add(delta)?;
                    }
                    if delta != Tokens::default() {
                        r.add_usage(&current_model, delta)?;
                    }
                } else {
                    // Counter corrections cannot reliably be attributed across models.
                    if range.is_some() {
                        incomplete_usage = true;
                    } else {
                        r.models = vec![ModelUsage {
                            model: "unknown (counter correction)".into(),
                            tokens: total,
                        }];
                    }
                    if !r.warnings.iter().any(|w| w.starts_with("Usage counters")) {
                        r.warnings
                            .push("Usage counters corrected; model attribution unavailable".into());
                    }
                }
                previous = total;
                saw_usage = true;
                if range.is_none() {
                    r.total = Some(total);
                }
            }
            _ => {}
        }
    }
    let Some(mut r) = receipt else {
        bail!("no session metadata found");
    };
    if let Some(start) = active_start {
        let duration = clipped_duration(start, r.ended_at);
        active += duration;
        saw_active |= duration > Duration::zero();
        if range.is_none_or(|range| start < range.end && r.ended_at >= range.start) {
            r.warnings
                .push("Turn still open; receipt is a snapshot".into());
        }
    }
    if !saw_event && !saw_active {
        return Ok(None);
    }
    if saw_active {
        r.active_duration = Some(active);
    }
    if r.total.is_none() && !incomplete_usage {
        r.warnings.push("No token usage recorded".into());
    }
    if incomplete_usage {
        r.total = None;
    }
    // Safety auto-review usage is free and does not belong on the receipt.
    if let Some(index) = r
        .models
        .iter()
        .position(|usage| usage.model == "codex-auto-review")
    {
        let excluded = r.models.remove(index).tokens;
        r.total = r.total.and_then(|total| total.delta(excluded));
    }
    Ok(Some(r))
}
