pub mod ascii;
pub mod html;
pub mod logo;

use clap::ValueEnum;

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum Paper {
    #[value(name = "58")]
    Mm58,
    #[default]
    #[value(name = "80")]
    Mm80,
}

impl Paper {
    pub fn columns(self) -> usize {
        match self {
            Self::Mm58 => 32,
            Self::Mm80 => 42,
        }
    }
    pub fn millimeters(self) -> usize {
        match self {
            Self::Mm58 => 58,
            Self::Mm80 => 80,
        }
    }
}

pub fn safe_text(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            }
        })
        .collect()
}

pub fn model_name(model: &str) -> String {
    match model {
        "gpt-6-astra" => "GPT-6 Astra",
        "gpt-6.1-sol" => "GPT-6.1 Sol",
        "gpt-6-luna" => "GPT-6 Luna",
        "gpt-5.6-sol" => "GPT-5.6 Sol",
        "gpt-5.3-codex" => "GPT-5.3 Codex",
        other => other,
    }
    .to_owned()
}

pub fn usage_rows(tokens: crate::receipt::Tokens) -> [(&'static str, u64); 4] {
    [
        (
            "Input (uncached)",
            tokens.input_tokens - tokens.cached_input_tokens - tokens.cache_write_input_tokens,
        ),
        ("Output tokens", tokens.output_tokens),
        ("Cache write", tokens.cache_write_input_tokens),
        ("Cache read", tokens.cached_input_tokens),
    ]
}
