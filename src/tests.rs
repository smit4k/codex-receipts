use crate::{
    pricing::{Pricing, money},
    receipt::Tokens,
    render::{Paper, ascii, html},
    session::parser,
};
use std::io::Cursor;

const FIXTURE: &str = include_str!("../tests/fixtures/session.jsonl");

#[test]
fn cumulative_snapshots_are_not_added_and_idle_time_is_excluded() {
    let receipt = parser::parse_reader(Cursor::new(FIXTURE)).unwrap();
    assert_eq!(
        receipt.total.unwrap(),
        Tokens {
            input_tokens: 42891,
            cached_input_tokens: 28000,
            output_tokens: 12304,
            cache_write_input_tokens: 0
        }
    );
    assert_eq!(receipt.models.len(), 1);
    assert_eq!(receipt.models[0].tokens, receipt.total.unwrap());
    assert_eq!(receipt.active_duration.unwrap().num_seconds(), 11);
    assert_eq!((receipt.ended_at - receipt.started_at).num_seconds(), 10800);
    assert_eq!(money(Pricing::bundled().unwrap().total(&receipt)), "$0.20");
}

#[test]
fn model_switches_receive_only_their_incremental_usage() {
    let extra = concat!(
        "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-6.1-sol\"}}\n",
        "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":43891,\"cached_input_tokens\":28500,\"output_tokens\":12404}}}}\n"
    );
    let r = parser::parse_reader(Cursor::new(format!("{FIXTURE}{extra}"))).unwrap();
    assert_eq!(
        r.models[1].tokens,
        Tokens {
            input_tokens: 1000,
            cached_input_tokens: 500,
            output_tokens: 100,
            cache_write_input_tokens: 0
        }
    );
    assert_eq!(r.models[0].model, "gpt-5.3-codex");
    assert_eq!(Pricing::bundled().unwrap().total(&r), Some(205_265_250));
}

#[test]
fn counter_corrections_keep_latest_total_without_fabricating_model_costs() {
    let extra = "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":100,\"cached_input_tokens\":50,\"output_tokens\":10}}}}\n";
    let r = parser::parse_reader(Cursor::new(format!("{FIXTURE}{extra}"))).unwrap();
    assert_eq!(r.total.unwrap().input_tokens, 100);
    assert_eq!(Pricing::bundled().unwrap().total(&r), None);
    assert!(r.warnings.iter().any(|w| w.contains("corrected")));
}

#[test]
fn incomplete_tail_is_tolerated_but_corrupt_complete_records_fail() {
    let r = parser::parse_reader(Cursor::new(format!("{FIXTURE}{{\"type\":"))).unwrap();
    assert_eq!(r.total.unwrap().output_tokens, 12304);
    assert!(r.warnings.iter().any(|w| w.contains("Incomplete")));
    assert!(parser::parse_reader(Cursor::new(format!("{FIXTURE}{{invalid}}\n"))).is_err());
}

#[test]
fn invalid_usage_and_missing_metadata_are_rejected() {
    assert!(
        Tokens {
            input_tokens: 1,
            cached_input_tokens: 2,
            ..Tokens::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        parser::parse_reader(Cursor::new("{\"type\":\"turn_context\",\"payload\":{}}\n")).is_err()
    );
    let invalid = FIXTURE.replace("\"input_tokens\":42891", "\"input_tokens\":0");
    assert!(parser::parse_reader(Cursor::new(invalid)).is_err());
}

#[test]
fn missing_usage_and_unknown_models_do_not_show_zero_cost() {
    let r = parser::parse_reader(Cursor::new(FIXTURE.lines().next().unwrap())).unwrap();
    assert!(r.total.is_none());
    assert_eq!(money(Pricing::bundled().unwrap().total(&r)), "N/A");
    assert!(r.active_duration.is_none());
    let unknown = FIXTURE.replace("gpt-5.3-codex", "unknown-new-model");
    let r = parser::parse_reader(Cursor::new(unknown)).unwrap();
    assert_eq!(Pricing::bundled().unwrap().total(&r), None);
}

#[test]
fn unpriced_cache_writes_do_not_receive_normal_input_pricing() {
    let fixture = FIXTURE.replace(
        "\"output_tokens\":12304",
        "\"cache_write_input_tokens\":1,\"output_tokens\":12304",
    );
    let r = parser::parse_reader(Cursor::new(fixture)).unwrap();
    assert_eq!(Pricing::bundled().unwrap().total(&r), None);
}

#[test]
fn renderers_respect_width_escape_metadata_and_exclude_prompts() {
    let mut r = parser::parse_reader(Cursor::new(FIXTURE)).unwrap();
    r.project = "<script>alert('x')</script>\u{1b}\n雪".into();
    let pricing = Pricing::bundled().unwrap();
    for paper in [Paper::Mm58, Paper::Mm80] {
        let text = ascii::render(&r, &pricing, paper);
        assert!(text.lines().all(|line| line.len() <= paper.columns()));
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains("Synthetic private prompt"));
        let page = html::render(&r, &pricing, paper);
        assert!(!page.contains("<script>"));
        assert!(page.contains("&lt;script&gt;"));
    }
}

#[test]
fn monetary_rounding_is_exact_at_half_cent_boundaries() {
    assert_eq!(money(Some(4_999_999)), "$0.00");
    assert_eq!(money(Some(5_000_000)), "$0.01");
    assert_eq!(money(Some(999_999_999)), "$1.00");
}

#[test]
fn text_receipt_matches_reviewed_fixture() {
    let r = parser::parse_reader(Cursor::new(FIXTURE)).unwrap();
    assert_eq!(
        ascii::render(&r, &Pricing::bundled().unwrap(), Paper::Mm80),
        include_str!("../tests/fixtures/receipt.txt")
    );
}

#[test]
fn working_directory_never_leaks_parent_paths_across_platforms() {
    for cwd in [
        "C:\\Users\\private-person\\project\\",
        "/home/private-person/project/",
    ] {
        let fixture = FIXTURE.replace("/home/example/codex-receipts", &cwd.replace('\\', "\\\\"));
        let r = parser::parse_reader(Cursor::new(fixture)).unwrap();
        assert_eq!(r.project, "project");
        assert!(
            !ascii::render(&r, &Pricing::bundled().unwrap(), Paper::Mm80)
                .contains("private-person")
        );
    }
}

#[test]
fn terminal_styles_preserve_columns_and_exports_stay_plain() {
    let r = parser::parse_reader(Cursor::new(FIXTURE)).unwrap();
    let pricing = Pricing::bundled().unwrap();
    for paper in [Paper::Mm58, Paper::Mm80] {
        let plain = ascii::render(&r, &pricing, paper);
        let terminal = ascii::terminal(&r, &pricing, paper);
        assert!(plain.is_ascii());
        assert!(!plain.contains('\u{1b}'));
        assert!(terminal.contains("\x1b[1mTOTAL (API EST.)"));
        assert!(terminal.contains("\x1b[2m"));
        assert!(terminal.contains('█'));
        let visible = terminal
            .replace("\x1b[0m", "")
            .replace("\x1b[1m", "")
            .replace("\x1b[2m", "");
        assert!(
            visible
                .lines()
                .all(|line| line.chars().count() <= paper.columns())
        );
    }
}

#[test]
fn model_line_items_keep_cached_tokens_separate() {
    let r = parser::parse_reader(Cursor::new(FIXTURE)).unwrap();
    let rows = crate::render::usage_rows(r.models[0].tokens);
    assert_eq!(
        rows,
        [
            ("Input (uncached)", 14891),
            ("Output tokens", 12304),
            ("Cache write", 0),
            ("Cache read", 28000)
        ]
    );
    let pricing = Pricing::bundled().unwrap();
    let page = html::render(&r, &pricing, Paper::Mm80);
    assert!(page.contains("<dt>Input (uncached)</dt><dd>14,891</dd>"));
    assert!(!page.contains("<pre>"));
}

#[test]
fn narrow_text_receipt_matches_approved_snapshot() {
    let receipt = parser::parse_reader(Cursor::new(FIXTURE)).unwrap();
    assert_eq!(
        ascii::render(&receipt, &Pricing::bundled().unwrap(), Paper::Mm58),
        include_str!("../tests/fixtures/receipt-58.txt")
    );
}

#[test]
fn terminal_receipts_match_approved_snapshots_at_both_widths() {
    let receipt = parser::parse_reader(Cursor::new(include_str!(
        "../tests/fixtures/multi-model.jsonl"
    )))
    .unwrap();
    let pricing = Pricing::bundled().unwrap();
    for (paper, expected) in [
        (
            Paper::Mm58,
            include_str!("../tests/fixtures/terminal-58.txt"),
        ),
        (
            Paper::Mm80,
            include_str!("../tests/fixtures/terminal-80.txt"),
        ),
    ] {
        assert_eq!(
            ascii::terminal(&receipt, &pricing, paper).replace('\u{1b}', "\\x1b"),
            expected
        );
    }
}

#[test]
fn printable_html_matches_approved_snapshots_at_both_widths() {
    let receipt = parser::parse_reader(Cursor::new(include_str!(
        "../tests/fixtures/multi-model.jsonl"
    )))
    .unwrap();
    let pricing = Pricing::bundled().unwrap();
    for (paper, expected) in [
        (
            Paper::Mm58,
            include_str!("../tests/fixtures/receipt-58.html"),
        ),
        (
            Paper::Mm80,
            include_str!("../tests/fixtures/receipt-80.html"),
        ),
    ] {
        assert_eq!(html::render(&receipt, &pricing, paper), expected);
    }
}

#[test]
fn empty_and_unpriced_receipts_show_na_instead_of_a_zero_total() {
    let missing = parser::parse_reader(Cursor::new(FIXTURE.lines().next().unwrap())).unwrap();
    let unknown = parser::parse_reader(Cursor::new(
        FIXTURE.replace("gpt-5.3-codex", "future-model"),
    ))
    .unwrap();
    let pricing = Pricing::bundled().unwrap();
    for receipt in [missing, unknown] {
        for paper in [Paper::Mm58, Paper::Mm80] {
            let text = ascii::render(&receipt, &pricing, paper);
            let page = html::render(&receipt, &pricing, paper);
            assert!(text.contains("N/A"));
            assert!(page.contains("<span class=\"amount\">N/A</span>"));
            assert!(!text.contains("$0.00"));
            assert!(!page.contains("$0.00"));
        }
    }
}

#[test]
fn long_metadata_and_maximum_counts_preserve_data_without_overwide_text() {
    let mut receipt = parser::parse_reader(Cursor::new(FIXTURE)).unwrap();
    receipt.project = "a-very-long-project-name".repeat(8);
    receipt.models[0].model = "gpt-6-astra".into();
    let tokens = Tokens {
        input_tokens: u64::MAX,
        output_tokens: u64::MAX,
        ..Tokens::default()
    };
    receipt.models[0].tokens = tokens;
    receipt.total = Some(tokens);
    let pricing = Pricing::bundled().unwrap();
    for paper in [Paper::Mm58, Paper::Mm80] {
        let text = ascii::render(&receipt, &pricing, paper);
        let page = html::render(&receipt, &pricing, paper);
        assert!(text.lines().all(|line| line.len() <= paper.columns()));
        assert!(text.contains("18,446,744,073,709,551,615"));
        assert!(page.contains("18,446,744,073,709,551,615"));
        assert!(page.contains(&receipt.project));
        assert!(page.contains(&money(pricing.total(&receipt))));
    }
}

#[test]
fn every_untrusted_receipt_field_is_escaped_in_html() {
    let mut receipt = parser::parse_reader(Cursor::new(FIXTURE)).unwrap();
    let markup = "<img src=x onerror=\"alert('x')\">&\u{1b}";
    receipt.project = markup.into();
    receipt.session_id = markup.into();
    receipt.models[0].model = markup.into();
    receipt.warnings.push(markup.into());
    let page = html::render(&receipt, &Pricing::bundled().unwrap(), Paper::Mm58);
    assert!(!page.contains("<img"));
    assert!(!page.contains('\u{1b}'));
    assert!(page.contains("&lt;img src=x onerror=&quot;alert(&#39;x&#39;)&quot;&gt;&amp;?"));
    assert!(!page.contains("Synthetic private prompt"));
}
