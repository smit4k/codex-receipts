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
fn repeated_metadata_preserves_usage_counters_and_active_turns() {
    let header = FIXTURE.lines().next().unwrap();
    let log = format!("{header}\n{FIXTURE}{header}\n");
    let r = parser::parse_reader(Cursor::new(log)).unwrap();
    assert_eq!(r.total.unwrap().input_tokens, 42891);
    assert_eq!(r.models.len(), 1);
    assert_eq!(r.models[0].tokens, r.total.unwrap());
    assert_eq!(r.active_duration.unwrap().num_seconds(), 11);
}

#[test]
fn fork_ancestor_headers_preserve_the_child_identity_and_period_accounting() {
    let mut child: serde_json::Value =
        serde_json::from_str(FIXTURE.lines().next().unwrap()).unwrap();
    let mut parent = child.clone();
    child["payload"]["id"] = "child-session".into();
    child["payload"]["cwd"] = "/home/example/child-project".into();
    child["payload"]["forked_from_id"] = "parent-session".into();
    parent["payload"]["id"] = "parent-session".into();
    parent["payload"]["timestamp"] = "2026-10-23T19:00:00Z".into();
    parent["payload"]["forked_from_id"] = "019e0000-0000-7000-8000-000000000001".into();
    let log = format!("{child}\n{parent}\n{FIXTURE}");
    for r in [
        parser::parse_reader(Cursor::new(&log)).unwrap(),
        period_receipt(&log, "2026-10-24T19:00:00Z", "2026-10-25T00:00:00Z").unwrap(),
    ] {
        assert_eq!(r.session_id, "child-session");
        assert_eq!(r.project, "child-project");
        assert_eq!(r.started_at.to_rfc3339(), "2026-10-24T19:00:00+00:00");
        assert_eq!(r.total.unwrap().input_tokens, 42891);
        assert_eq!(r.active_duration.unwrap().num_seconds(), 11);
    }
}

#[test]
fn unrelated_or_invalid_repeated_metadata_is_rejected() {
    let header = FIXTURE.lines().next().unwrap();
    let unrelated = header.replace("000000000001", "000000000002");
    let error = parser::parse_reader(Cursor::new(format!("{header}\n{unrelated}\n"))).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unrelated session metadata at line 2")
    );
    let invalid = header.replace("019e0000-0000-7000-8000-000000000001", "");
    assert!(parser::parse_reader(Cursor::new(format!("{header}\n{invalid}\n"))).is_err());
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
fn auto_review_is_excluded_without_charging_its_tokens_to_the_next_model() {
    let extra = concat!(
        "{\"timestamp\":\"2026-10-24T22:00:01Z\",\"type\":\"turn_context\",\"payload\":{\"model\":\"codex-auto-review\"}}\n",
        "{\"timestamp\":\"2026-10-24T22:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":43891,\"cached_input_tokens\":28500,\"output_tokens\":12404}}}}\n",
        "{\"timestamp\":\"2026-10-24T22:00:02Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"model_reroute\",\"to_model\":\"gpt-6.1-sol\"}}\n",
        "{\"timestamp\":\"2026-10-24T22:00:02Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":44891,\"cached_input_tokens\":29000,\"output_tokens\":12504}}}}\n"
    );
    let log = format!("{FIXTURE}{extra}");
    let pricing = Pricing::bundled().unwrap();
    for receipt in [
        parser::parse_reader(Cursor::new(&log)).unwrap(),
        period_receipt(&log, "2026-10-24T19:00:00Z", "2026-10-25T00:00:00Z").unwrap(),
    ] {
        assert_eq!(receipt.models.len(), 2);
        assert_eq!(receipt.models[1].model, "gpt-6.1-sol");
        assert_eq!(receipt.models[1].tokens.input_tokens, 1000);
        assert_eq!(receipt.total.unwrap().input_tokens, 43891);
        assert_eq!(receipt.total.unwrap().cached_input_tokens, 28500);
        assert_eq!(receipt.total.unwrap().output_tokens, 12404);
        assert_eq!(pricing.total(&receipt), Some(205_265_250));
        assert!(!pricing.is_partial(&receipt));
        for paper in [Paper::Mm58, Paper::Mm80] {
            assert!(!ascii::render(&receipt, &pricing, paper).contains("codex-auto-review"));
            assert!(!html::render(&receipt, &pricing, paper).contains("codex-auto-review"));
        }
    }
    let receipt = period_receipt(&log, "2026-10-24T22:00:02Z", "2026-10-25T00:00:00Z").unwrap();
    assert_eq!(receipt.total.unwrap().input_tokens, 1000);
    assert_eq!(receipt.models.len(), 1);
}

#[test]
fn auto_review_only_receipts_have_no_usage_rows_or_cost() {
    let log = FIXTURE.replace("gpt-5.3-codex", "codex-auto-review");
    for receipt in [
        parser::parse_reader(Cursor::new(&log)).unwrap(),
        period_receipt(&log, "2026-10-24T19:00:00Z", "2026-10-25T00:00:00Z").unwrap(),
    ] {
        assert!(receipt.models.is_empty());
        assert_eq!(receipt.total, Some(Tokens::default()));
        assert_eq!(Pricing::bundled().unwrap().total_display(&receipt), "$0.00");
    }
}

#[test]
fn counter_corrections_keep_latest_total_without_fabricating_model_costs() {
    let extra = "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":100,\"cached_input_tokens\":50,\"output_tokens\":10}}}}\n";
    let r = parser::parse_reader(Cursor::new(format!("{FIXTURE}{extra}"))).unwrap();
    assert_eq!(r.total.unwrap().input_tokens, 100);
    assert_eq!(Pricing::bundled().unwrap().total_display(&r), "$0.00*");
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
fn missing_usage_and_unknown_models_mark_the_total_as_partial() {
    let r = parser::parse_reader(Cursor::new(FIXTURE.lines().next().unwrap())).unwrap();
    assert!(r.total.is_none());
    assert_eq!(Pricing::bundled().unwrap().total_display(&r), "$0.00*");
    assert!(r.active_duration.is_none());
    let unknown = FIXTURE.replace("gpt-5.3-codex", "unknown-new-model");
    let r = parser::parse_reader(Cursor::new(unknown)).unwrap();
    assert_eq!(Pricing::bundled().unwrap().total_display(&r), "$0.00*");
}

#[test]
fn unpriced_cache_writes_do_not_receive_normal_input_pricing() {
    let fixture = FIXTURE.replace(
        "\"output_tokens\":12304",
        "\"cache_write_input_tokens\":1,\"output_tokens\":12304",
    );
    let r = parser::parse_reader(Cursor::new(fixture)).unwrap();
    let pricing = Pricing::bundled().unwrap();
    assert_eq!(pricing.model_cost(&r.models[0]), None);
    assert_eq!(pricing.total_display(&r), "$0.00*");
}

#[test]
fn partial_totals_sum_known_costs_and_explain_the_asterisk() {
    let mut receipt = parser::parse_reader(Cursor::new(FIXTURE)).unwrap();
    let pricing = Pricing::bundled().unwrap();
    receipt
        .add_usage(
            "unpriced-model",
            Tokens {
                input_tokens: 1000,
                ..Tokens::default()
            },
        )
        .unwrap();
    for missing_total in [false, true] {
        if missing_total {
            receipt.total = None;
        }
        assert_eq!(pricing.total_display(&receipt), "$0.20*");
        for paper in [Paper::Mm58, Paper::Mm80] {
            let text = ascii::render(&receipt, &pricing, paper);
            let page = html::render(&receipt, &pricing, paper);
            assert!(
                text.lines()
                    .any(|line| line.starts_with("TOTAL (API EST.)") && line.ends_with("$0.20*"))
            );
            assert!(text.contains("* Partial total: known costs"));
            assert!(text.lines().all(|line| line.len() <= paper.columns()));
            assert!(page.contains("<span class=\"amount\">$0.20*</span>"));
            assert!(page.contains("* Partial total: known costs."));
            assert!(page.contains("<span class=\"amount\">N/A</span>"));
        }
    }
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
fn empty_and_unpriced_receipts_show_a_starred_zero_total() {
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
            assert!(text.contains("$0.00*"));
            assert!(page.contains("<span class=\"amount\">$0.00*</span>"));
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

fn period_receipt(log: &str, start: &str, end: &str) -> Option<crate::receipt::Receipt> {
    let range = start.parse().unwrap()..end.parse().unwrap();
    parser::parse_reader_in_period(Cursor::new(log), Some(&range)).unwrap()
}

#[test]
fn periods_use_a_rolling_week_and_calendar_month_with_month_end_clamping() {
    use crate::cli::Period;
    for (period, end, start) in [
        (Period::Week, "2026-10-25T12:34:56Z", "2026-10-18T12:34:56Z"),
        (
            Period::Month,
            "2026-10-25T12:34:56Z",
            "2026-09-25T12:34:56Z",
        ),
        (
            Period::Month,
            "2026-03-31T12:34:56Z",
            "2026-02-28T12:34:56Z",
        ),
        (
            Period::Month,
            "2024-03-31T12:34:56Z",
            "2024-02-29T12:34:56Z",
        ),
    ] {
        let range = period.range(end.parse().unwrap()).unwrap();
        assert_eq!(range, start.parse().unwrap()..end.parse().unwrap());
    }
}

#[test]
fn period_usage_subtracts_prior_snapshots_and_clips_active_time_at_both_ends() {
    let r = period_receipt(FIXTURE, "2026-10-24T19:00:06Z", "2026-10-24T19:00:11Z").unwrap();
    assert_eq!(
        r.total.unwrap(),
        Tokens {
            input_tokens: 41891,
            cached_input_tokens: 27800,
            output_tokens: 12204,
            cache_write_input_tokens: 0,
        }
    );
    assert_eq!(r.models[0].tokens, r.total.unwrap());
    assert_eq!(r.active_duration.unwrap().num_seconds(), 5);
    assert!(!r.warnings.iter().any(|w| w.contains("still open")));
}

#[test]
fn period_start_is_inclusive_and_end_is_exclusive() {
    let r = period_receipt(FIXTURE, "2026-10-24T19:00:00Z", "2026-10-24T19:00:10Z").unwrap();
    assert_eq!(r.total.unwrap().input_tokens, 1000);
    let r = period_receipt(FIXTURE, "2026-10-24T19:00:10Z", "2026-10-24T19:00:11Z").unwrap();
    assert_eq!(r.total.unwrap().input_tokens, 41891);
    for (start, end) in [
        ("2026-10-23T00:00:00Z", "2026-10-24T19:00:00Z"),
        ("2026-10-24T22:00:01Z", "2026-10-26T00:00:00Z"),
    ] {
        assert!(period_receipt(FIXTURE, start, end).is_none());
    }
}

#[test]
fn period_cost_is_partial_when_the_initial_baseline_or_timestamps_are_missing() {
    let missing_baseline = FIXTURE
        .lines()
        .filter(|line| !line.contains("19:00:05Z"))
        .collect::<Vec<_>>()
        .join("\n");
    let untimed = FIXTURE.replace("\"timestamp\":\"2026-10-24T19:00:10Z\",", "");
    for (log, warning) in [(&missing_baseline, "baseline"), (&untimed, "Untimed")] {
        let r = period_receipt(log, "2026-10-24T19:00:06Z", "2026-10-25T00:00:00Z").unwrap();
        assert!(r.total.is_none());
        assert!(r.warnings.iter().any(|w| w.contains(warning)));
        assert!(Pricing::bundled().unwrap().is_partial(&r));
    }
}

#[test]
fn counter_corrections_in_a_period_do_not_include_the_entire_session_total() {
    let log = FIXTURE.replace(
        "\"input_tokens\":42891,\"cached_input_tokens\":28000,\"output_tokens\":12304",
        "\"input_tokens\":100,\"cached_input_tokens\":50,\"output_tokens\":10",
    );
    let r = period_receipt(&log, "2026-10-24T19:00:06Z", "2026-10-25T00:00:00Z").unwrap();
    assert!(r.total.is_none());
    assert!(r.models.is_empty());
    assert!(r.warnings.iter().any(|w| w.contains("corrected")));
}

#[test]
fn counter_corrections_before_the_period_only_update_its_baseline() {
    let r = period_receipt(
        &FIXTURE.replacen("\"input_tokens\":1000", "\"input_tokens\":5000", 1),
        "2026-10-24T19:00:06Z",
        "2026-10-25T00:00:00Z",
    )
    .unwrap();
    assert_eq!(r.total.unwrap().input_tokens, 41891);
    assert!(Pricing::bundled().unwrap().total(&r).is_some());
    assert!(!r.warnings.iter().any(|w| w.contains("corrected")));
}

#[test]
fn period_receipts_combine_project_and_session_count_in_both_formats() {
    let mut r = parser::parse_reader(Cursor::new(FIXTURE)).unwrap();
    let pricing = Pricing::bundled().unwrap();
    for count in [0, 1, 42, usize::MAX] {
        r.session_count = Some(count);
        let header = format!(
            "All projects • {count} session{}",
            if count == 1 { "" } else { "s" }
        );
        for paper in [Paper::Mm58, Paper::Mm80] {
            let text = ascii::render(&r, &pricing, paper);
            let page = html::render(&r, &pricing, paper);
            assert!(!text.contains("PERIOD RECEIPT"));
            assert!(!page.contains("PERIOD RECEIPT"));
            assert!(
                text.lines()
                    .all(|line| line.chars().count() <= paper.columns())
            );
            if header.chars().count() <= paper.columns() {
                assert!(text.lines().any(|line| line.trim() == header));
            } else {
                assert!(text.contains(&count.to_string()));
            }
            assert!(page.contains(&format!("<p>{header}</p>")));
        }
    }
}

#[test]
fn period_receipts_display_days_at_both_widths_in_both_formats() {
    let mut receipt = parser::parse_reader(Cursor::new(FIXTURE)).unwrap();
    receipt.session_count = Some(1);
    let pricing = Pricing::bundled().unwrap();
    for days in [7, 28, 29, 30, 31] {
        receipt.ended_at = receipt.started_at + chrono::Duration::days(days);
        for paper in [Paper::Mm58, Paper::Mm80] {
            let text = ascii::render(&receipt, &pricing, paper);
            let page = html::render(&receipt, &pricing, paper);
            assert!(text.lines().any(|line| {
                line.starts_with("Period")
                    && line.split_whitespace().last() == Some(format!("{days}d").as_str())
            }));
            assert!(page.contains(&format!("<dt>Period</dt><dd>{days}d</dd>")));
        }
    }
}

#[test]
fn period_model_switches_preserve_attribution_and_open_turns_are_clipped() {
    let log = FIXTURE.replace("{\"timestamp\":\"2026-10-24T19:00:10Z\"", "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-6.1-sol\"}}\n{\"timestamp\":\"2026-10-24T19:00:10Z\"");
    let log = log
        .lines()
        .filter(|line| !line.contains("task_complete"))
        .collect::<Vec<_>>()
        .join("\n");
    let r = period_receipt(&log, "2026-10-24T19:00:06Z", "2026-10-24T19:00:11Z").unwrap();
    assert_eq!(r.models.len(), 1);
    assert_eq!(r.models[0].model, "gpt-6.1-sol");
    assert_eq!(r.active_duration.unwrap().num_seconds(), 5);
    assert!(r.warnings.iter().any(|w| w.contains("still open")));
}

#[test]
fn an_open_turn_after_the_period_does_not_add_a_snapshot_warning() {
    let log = format!(
        "{FIXTURE}{{\"timestamp\":\"2026-10-25T01:00:00Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\"}}}}\n"
    );
    let r = period_receipt(&log, "2026-10-24T19:00:00Z", "2026-10-25T00:00:00Z").unwrap();
    assert_eq!(r.active_duration.unwrap().num_seconds(), 11);
    assert!(!r.warnings.iter().any(|w| w.contains("still open")));
}

#[test]
fn combining_model_usage_rejects_token_overflow() {
    let mut r = parser::parse_reader(Cursor::new(FIXTURE)).unwrap();
    assert!(
        r.add_usage(
            "gpt-5.3-codex",
            Tokens {
                input_tokens: u64::MAX,
                ..Tokens::default()
            }
        )
        .is_err()
    );
}
