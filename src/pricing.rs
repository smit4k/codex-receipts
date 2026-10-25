use crate::receipt::{ModelUsage, Receipt};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
pub struct Pricing {
    pub models: BTreeMap<String, Rate>,
}

// Milli-USD per million tokens equals nano-USD per token.
#[derive(Deserialize)]
pub struct Rate {
    input: u64,
    cached: u64,
    output: u64,
}

impl Pricing {
    pub fn bundled() -> Result<Self> {
        serde_json::from_str(include_str!("../data/pricing.json"))
            .context("invalid bundled pricing table")
    }

    pub fn model_cost(&self, usage: &ModelUsage) -> Option<u128> {
        let rate = self.models.get(&usage.model)?;
        let tokens = usage.tokens;
        if tokens.cache_write_input_tokens != 0 {
            return None;
        }
        Some(
            u128::from(
                tokens
                    .input_tokens
                    .checked_sub(tokens.cached_input_tokens)?,
            ) * u128::from(rate.input)
                + u128::from(tokens.cached_input_tokens) * u128::from(rate.cached)
                + u128::from(tokens.output_tokens) * u128::from(rate.output),
        )
    }

    pub fn total(&self, receipt: &Receipt) -> Option<u128> {
        receipt
            .models
            .iter()
            .filter_map(|usage| self.model_cost(usage))
            .try_fold(0_u128, u128::checked_add)
    }

    pub fn is_partial(&self, receipt: &Receipt) -> bool {
        receipt.total.is_none()
            || receipt
                .models
                .iter()
                .any(|usage| self.model_cost(usage).is_none())
    }

    pub fn total_display(&self, receipt: &Receipt) -> String {
        let mut total = money(self.total(receipt));
        if self.is_partial(receipt) {
            total.push('*');
        }
        total
    }
}

pub fn money(nanodollars: Option<u128>) -> String {
    match nanodollars {
        Some(amount) => {
            let cents = (amount + 5_000_000) / 10_000_000;
            format!("${}.{:02}", cents / 100, cents % 100)
        }
        None => "N/A".into(),
    }
}
