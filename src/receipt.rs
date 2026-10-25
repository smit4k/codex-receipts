use anyhow::{Result, ensure};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Tokens {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_write_input_tokens: u64,
}

impl Tokens {
    pub fn validate(self) -> Result<Self> {
        ensure!(
            self.cached_input_tokens <= self.input_tokens,
            "cached input exceeds input tokens"
        );
        ensure!(
            self.cache_write_input_tokens <= self.input_tokens - self.cached_input_tokens,
            "cache-write input exceeds uncached input tokens"
        );
        Ok(self)
    }

    pub fn delta(self, previous: Self) -> Option<Self> {
        Some(Self {
            input_tokens: self.input_tokens.checked_sub(previous.input_tokens)?,
            cached_input_tokens: self
                .cached_input_tokens
                .checked_sub(previous.cached_input_tokens)?,
            output_tokens: self.output_tokens.checked_sub(previous.output_tokens)?,
            cache_write_input_tokens: self
                .cache_write_input_tokens
                .checked_sub(previous.cache_write_input_tokens)?,
        })
    }

    pub fn add(&mut self, other: Self) -> Result<()> {
        self.input_tokens = self
            .input_tokens
            .checked_add(other.input_tokens)
            .ok_or_else(|| anyhow::anyhow!("input token overflow"))?;
        self.cached_input_tokens = self
            .cached_input_tokens
            .checked_add(other.cached_input_tokens)
            .ok_or_else(|| anyhow::anyhow!("cached token overflow"))?;
        self.output_tokens = self
            .output_tokens
            .checked_add(other.output_tokens)
            .ok_or_else(|| anyhow::anyhow!("output token overflow"))?;
        self.cache_write_input_tokens = self
            .cache_write_input_tokens
            .checked_add(other.cache_write_input_tokens)
            .ok_or_else(|| anyhow::anyhow!("cache-write token overflow"))?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ModelUsage {
    pub model: String,
    pub tokens: Tokens,
}

#[derive(Debug)]
pub struct Receipt {
    pub session_id: String,
    pub session_count: Option<usize>,
    pub project: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub active_duration: Option<Duration>,
    pub models: Vec<ModelUsage>,
    pub total: Option<Tokens>,
    pub warnings: Vec<String>,
}

impl Receipt {
    pub fn add_usage(&mut self, model: &str, tokens: Tokens) -> Result<()> {
        if let Some(usage) = self.models.iter_mut().find(|usage| usage.model == model) {
            usage.tokens.add(tokens)?;
        } else {
            self.models.push(ModelUsage {
                model: model.to_owned(),
                tokens,
            });
        }
        Ok(())
    }
}

pub fn project_name(cwd: &str) -> &str {
    cwd.rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or("unknown")
}
