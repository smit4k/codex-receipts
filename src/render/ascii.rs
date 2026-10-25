use super::{Paper, logo, model_name, safe_text, usage_rows};
use crate::{
    pricing::{Pricing, money},
    receipt::Receipt,
};
use chrono::Duration;

pub fn number(value: u64) -> String {
    let digits = value.to_string();
    let mut formatted = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(c);
    }
    formatted
}

pub fn duration(value: Duration) -> String {
    let seconds = value.num_seconds().max(0);
    if seconds >= 3600 {
        format!(
            "{}h {}m {}s",
            seconds / 3600,
            seconds % 3600 / 60,
            seconds % 60
        )
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}

#[derive(Clone, Copy)]
pub enum Style {
    Normal,
    Muted,
    Bold,
    Total,
}

pub struct Line {
    pub text: String,
    pub style: Style,
}

struct Text {
    width: usize,
    lines: Vec<Line>,
}

impl Text {
    fn line(&mut self, value: &str, style: Style) {
        let text = safe_text(value);
        let mut rest = text.as_str();
        while rest.len() > self.width {
            let split = rest[..=self.width]
                .rfind(' ')
                .filter(|&i| i > 0)
                .unwrap_or(self.width);
            self.lines.push(Line {
                text: rest[..split].to_owned(),
                style,
            });
            rest = rest[split..].trim_start();
        }
        self.lines.push(Line {
            text: rest.to_owned(),
            style,
        });
    }
    fn center(&mut self, value: &str, style: Style) {
        let start = self.lines.len();
        self.line(value, style);
        for line in &mut self.lines[start..] {
            line.text = format!(
                "{:padding$}{}",
                "",
                line.text,
                padding = (self.width - line.text.len()) / 2
            );
        }
    }
    fn row(&mut self, left: &str, right: &str, style: Style) {
        let (left, right) = (safe_text(left), safe_text(right));
        if left.len() + right.len() < self.width {
            self.lines.push(Line {
                text: format!("{left}{right:>padding$}", padding = self.width - left.len()),
                style,
            });
        } else {
            self.line(&left, style);
            self.lines.push(Line {
                text: format!("{right:>width$}", width = self.width),
                style,
            });
        }
    }
    fn rule(&mut self, c: char) {
        self.line(&c.to_string().repeat(self.width), Style::Muted);
    }
    fn space(&mut self) {
        self.line("", Style::Normal);
    }
}

pub fn lines(receipt: &Receipt, pricing: &Pricing, paper: Paper) -> Vec<Line> {
    use Style::{Bold, Muted, Normal, Total};
    let mut text = Text {
        width: paper.columns(),
        lines: Vec::new(),
    };
    text.space();
    text.center("C O D E X  /  R E C E I P T S", Bold);
    text.space();
    text.center(&format!("Project: {}", receipt.project), Normal);
    text.center("SESSION RECEIPT", Muted);
    text.center(&receipt.session_id, Normal);
    text.center(
        &receipt
            .started_at
            .format("%Y-%m-%d %H:%M:%S UTC")
            .to_string(),
        Normal,
    );
    text.space();

    for usage in &receipt.models {
        text.rule('=');
        text.row(
            &model_name(&usage.model),
            &money(pricing.model_cost(usage)),
            Bold,
        );
        text.rule('-');
        for (label, count) in usage_rows(usage.tokens) {
            text.row(&format!("  {label}"), &number(count), Normal);
        }
        text.space();
    }
    if receipt.total.is_none() {
        text.line("No token usage recorded", Normal);
        text.space();
    }
    text.rule('=');
    text.row("TOTAL (API EST.)", &money(pricing.total(receipt)), Total);
    text.rule('=');
    text.space();
    text.row(
        "Elapsed",
        &duration(receipt.ended_at - receipt.started_at),
        Normal,
    );
    text.row(
        "Active turns",
        &receipt
            .active_duration
            .map(duration)
            .unwrap_or_else(|| "N/A".into()),
        Normal,
    );
    text.line("CASHIER: Codex", Normal);
    text.space();
    text.center("Thank you for building!", Bold);
    text.space();
    text.center("Estimated API cost", Muted);
    text.center("Standard short-context rates", Muted);
    text.center("Excludes premiums, tools & extras", Muted);
    text.center("Not a subscription charge", Muted);
    text.center(&format!("Prices: {}", pricing.version), Muted);
    if pricing.total(receipt).is_none() && receipt.total.is_some() {
        text.space();
        text.line("N/A: unknown model or unpriced usage", Muted);
    }
    for warning in &receipt.warnings {
        text.line(&format!("NOTE: {warning}"), Muted);
    }
    text.space();
    text.lines
}

pub fn render(receipt: &Receipt, pricing: &Pricing, paper: Paper) -> String {
    let mut output = String::from("\n");
    output.push_str(&logo::text(paper.columns(), false));
    for line in lines(receipt, pricing, paper) {
        output.push_str(&line.text);
        output.push('\n');
    }
    output
}

pub fn terminal(receipt: &Receipt, pricing: &Pricing, paper: Paper) -> String {
    let mut output = format!("\n\x1b[1m{}\x1b[0m", logo::text(paper.columns(), true));
    for line in lines(receipt, pricing, paper) {
        let style = match line.style {
            Style::Normal => "0",
            Style::Muted => "2",
            Style::Bold | Style::Total => "1",
        };
        output.push_str(&format!("\x1b[{style}m{}\x1b[0m\n", line.text));
    }
    output
}
