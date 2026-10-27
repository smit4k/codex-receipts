use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "codex-receipts-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codex-receipts"))
        .args(args)
        .output()
        .unwrap()
}
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/session.jsonl")
}
fn ok(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn exports_all_formats_and_refuses_to_overwrite() {
    let dir = Temp::new();
    for (format, extension) in [("text", "txt"), ("html", "html")] {
        let path = dir.0.join(format!("receipt.{extension}"));
        ok(&cli(&[
            "--file",
            fixture().to_str().unwrap(),
            "export",
            "--format",
            format,
            "-o",
            path.to_str().unwrap(),
        ]));
        let bytes = fs::read(&path).unwrap();
        assert!(!bytes.is_empty());
        let duplicate = cli(&[
            "--file",
            fixture().to_str().unwrap(),
            "export",
            "-o",
            path.to_str().unwrap(),
        ]);
        assert!(!duplicate.status.success());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn exports_default_to_files_in_the_working_directory_without_overwriting() {
    for (options, filename) in [
        (vec![], "receipt.html"),
        (vec!["--format", "html"], "receipt.html"),
        (vec!["--format", "text"], "receipt.txt"),
        (vec!["--format", "ascii"], "receipt.txt"),
        (vec!["-o", "custom.html"], "custom.html"),
    ] {
        let dir = Temp::new();
        fs::create_dir(dir.0.join("sessions")).unwrap();
        fs::copy(fixture(), dir.0.join("sessions/rollout.jsonl")).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_codex-receipts"));
        command
            .current_dir(&dir.0)
            .env("CODEX_HOME", &dir.0)
            .arg("export")
            .args(options);
        let output = command.output().unwrap();
        ok(&output);
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(&format!("Saved {filename}")));
        let bytes = fs::read(dir.0.join(filename)).unwrap();
        if filename.ends_with(".html") {
            assert!(bytes.starts_with(b"<!doctype html>"));
        } else {
            assert!(String::from_utf8_lossy(&bytes).contains("TOTAL (API EST.)"));
        }
        let duplicate = command.output().unwrap();
        assert!(!duplicate.status.success());
        assert!(String::from_utf8_lossy(&duplicate.stderr).contains("not overwritten"));
        assert_eq!(fs::read(dir.0.join(filename)).unwrap(), bytes);
    }
}

#[test]
fn discovers_nested_sessions_and_selects_unique_ids() {
    let dir = Temp::new();
    let nested = dir.0.join("2026/10/24");
    fs::create_dir_all(&nested).unwrap();
    let original = fs::read_to_string(fixture()).unwrap();
    fs::write(nested.join("old.jsonl"), &original).unwrap();
    fs::write(
        nested.join("new.jsonl"),
        original
            .replace("2026-10-24", "2026-10-25")
            .replace("000000000001", "000000000002"),
    )
    .unwrap();
    let root = dir.0.to_str().unwrap();
    let listing = cli(&["--sessions-dir", root, "sessions"]);
    ok(&listing);
    let text = String::from_utf8(listing.stdout).unwrap();
    assert!(text.find("000000000002").unwrap() < text.find("000000000001").unwrap());
    let latest = cli(&["--sessions-dir", root]);
    ok(&latest);
    assert!(String::from_utf8_lossy(&latest.stdout).contains("000000000002"));
    let selected = cli(&[
        "--sessions-dir",
        root,
        "show",
        "019e0000-0000-7000-8000-000000000001",
    ]);
    ok(&selected);
    assert!(String::from_utf8_lossy(&selected.stdout).contains("000000000001"));
    let ambiguous = cli(&["--sessions-dir", root, "show", "019e"]);
    assert!(!ambiguous.status.success());
    assert!(String::from_utf8_lossy(&ambiguous.stderr).contains("ambiguous"));
    assert!(
        !cli(&["--sessions-dir", root, "show", "missing"])
            .status
            .success()
    );
}

#[test]
fn codex_home_and_global_options_work_after_subcommands() {
    let dir = Temp::new();
    fs::create_dir(dir.0.join("sessions")).unwrap();
    fs::copy(fixture(), dir.0.join("sessions/rollout.jsonl")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_codex-receipts"))
        .env("CODEX_HOME", &dir.0)
        .args([
            "export", "--format", "text", "-o", "-", "--latest", "--paper", "58",
        ])
        .output()
        .unwrap();
    ok(&output);
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .all(|line| line.len() <= 32)
    );
}

#[test]
fn session_end_hook_uses_transcript_and_rejects_mismatches() {
    let dir = Temp::new();
    for id in [
        "019e0000-0000-7000-8000-000000000001",
        "different-session",
        "../escape",
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_codex-receipts"))
            .arg("hook")
            .arg("--output-dir")
            .arg(&dir.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let event = serde_json::json!({"session_id":id,"transcript_path":fixture(),"hook_event_name":"SessionEnd"});
        child
            .stdin
            .take()
            .unwrap()
            .write_all(event.to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.success(), id.starts_with("019e"));
    }
    let page = fs::read_to_string(dir.0.join("019e0000-0000-7000-8000-000000000001.html")).unwrap();
    assert!(page.contains("ESTIMATED API COST"));
}

#[test]
fn conflicting_selection_and_empty_directories_fail_clearly() {
    let dir = Temp::new();
    assert!(
        !cli(&["--sessions-dir", dir.0.to_str().unwrap()])
            .status
            .success()
    );
    assert!(
        !cli(&["--file", fixture().to_str().unwrap(), "--latest"])
            .status
            .success()
    );
    assert!(!cli(&["--paper", "72"]).status.success());
}

#[test]
fn text_alias_and_stdout_exports_match_the_requested_format() {
    for paper in ["58", "80"] {
        let plain = cli(&["--file", fixture().to_str().unwrap(), "--paper", paper]);
        ok(&plain);
        for format in ["text", "ascii"] {
            let exported = cli(&[
                "export",
                "--file",
                fixture().to_str().unwrap(),
                "--paper",
                paper,
                "--format",
                format,
                "-o",
                "-",
            ]);
            ok(&exported);
            assert_eq!(exported.stdout, plain.stdout);
            assert!(exported.stderr.is_empty());
            assert!(!exported.stdout.contains(&0x1b));
        }
        let page = cli(&[
            "export",
            "--file",
            fixture().to_str().unwrap(),
            "--paper",
            paper,
            "-o",
            "-",
        ]);
        ok(&page);
        assert!(page.stdout.starts_with(b"<!doctype html>"));
        assert!(page.stderr.is_empty());
    }
}

#[test]
fn print_command_is_available_and_raw_printer_exports_remain_deferred() {
    let help = cli(&["--help"]);
    ok(&help);
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .lines()
            .any(|line| line.starts_with("  print "))
    );
    let help = cli(&["print", "--help"]);
    ok(&help);
    assert!(String::from_utf8_lossy(&help.stdout).contains("native print dialog"));
    assert!(
        !cli(&["export", "--format", "escpos", "-o", "-"])
            .status
            .success()
    );
}

#[test]
fn print_validates_session_inputs_before_opening_a_window() {
    let dir = Temp::new();
    for period in [None, Some("week"), Some("month")] {
        let mut args = vec!["print", "--sessions-dir", dir.0.to_str().unwrap()];
        if let Some(period) = period {
            args.extend(["--period", period]);
        }
        let output = cli(&args);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("no Codex session"));
    }
    let path = dir.0.join("broken.jsonl");
    fs::write(&path, "{invalid}\n").unwrap();
    let output = cli(&["print", "--file", path.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid JSON"));
}

#[test]
#[ignore = "opens a native print dialog; close the receipt window to finish"]
fn native_print_dialog_opens_with_a_fixture_receipt() {
    ok(&cli(&["print", "--file", fixture().to_str().unwrap()]));
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn real_terminal_respects_no_color_and_dumb_terminal_modes() {
    let binary = env!("CARGO_BIN_EXE_codex-receipts");
    for (no_color, term, styled) in [
        (false, "xterm-256color", true),
        (true, "xterm-256color", false),
        (false, "dumb", false),
    ] {
        let mut command = Command::new("script");
        #[cfg(target_os = "macos")]
        command.args([
            "-q",
            "/dev/null",
            binary,
            "--file",
            fixture().to_str().unwrap(),
        ]);
        #[cfg(target_os = "linux")]
        {
            let shell_command = format!(
                "'{}' --file '{}'",
                binary.replace('\'', "'\\''"),
                fixture().to_str().unwrap().replace('\'', "'\\''")
            );
            command.args(["-q", "-e", "-c", &shell_command, "/dev/null"]);
        }
        command
            .env_remove("NO_COLOR")
            .env("TERM", term)
            .stdin(Stdio::null());
        if no_color {
            command.env("NO_COLOR", "1");
        }
        let output = command.output().unwrap();
        ok(&output);
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("TOTAL (API EST.)"));
        assert_eq!(text.contains("\x1b[1m"), styled);
        if !styled {
            assert!(!text.contains('\u{1b}'));
            assert!(text.is_ascii());
        }
    }
}

fn session_days_ago(days: i64, id: &str) -> String {
    use chrono::{DateTime, Duration, Utc};
    let start = Utc::now() - Duration::days(days);
    let original_start: DateTime<Utc> = "2026-10-24T19:00:00Z".parse().unwrap();
    let mut log = String::new();
    for line in fs::read_to_string(fixture()).unwrap().lines() {
        let mut event: serde_json::Value = serde_json::from_str(line).unwrap();
        let time: DateTime<Utc> = event["timestamp"].as_str().unwrap().parse().unwrap();
        event["timestamp"] = (start + (time - original_start)).to_rfc3339().into();
        if event["type"] == "session_meta" {
            event["payload"]["timestamp"] = start.to_rfc3339().into();
            event["payload"]["id"] = id.into();
        }
        log.push_str(&event.to_string());
        log.push('\n');
    }
    log
}

#[test]
fn period_receipts_combine_models_and_resumed_sessions_in_text_and_html() {
    let dir = Temp::new();
    let root = dir.0.to_str().unwrap();
    for (name, days, model) in [
        ("recent", 1, "gpt-5.3-codex"),
        ("second", 2, "gpt-5.3-codex"),
        ("safety", 1, "codex-auto-review"),
        ("month-only", 10, "gpt-6.1-sol"),
        ("old", 40, "gpt-5.3-codex"),
        ("future", -2, "gpt-5.3-codex"),
    ] {
        let log = session_days_ago(days, name).replace("gpt-5.3-codex", model);
        let (extension, bytes) = if days == 10 {
            (
                "jsonl.zst",
                zstd::stream::encode_all(log.as_bytes(), 0).unwrap(),
            )
        } else {
            ("jsonl", log.into_bytes())
        };
        fs::write(dir.0.join(format!("{name}.{extension}")), bytes).unwrap();
    }
    let old = session_days_ago(40, "resumed");
    let recent = session_days_ago(1, "resumed");
    let baseline = old
        .lines()
        .find(|line| line.contains("token_count"))
        .unwrap();
    let resumed = format!(
        "{}\n{baseline}\n{}\n",
        old.lines().next().unwrap(),
        recent.lines().skip(1).collect::<Vec<_>>().join("\n")
    );
    fs::write(dir.0.join("resumed.jsonl"), resumed).unwrap();

    for (period, sessions) in [("week", 4), ("month", 5)] {
        for (paper, columns) in [("58", 32), ("80", 42)] {
            let output = cli(&["--sessions-dir", root, "--period", period, "--paper", paper]);
            ok(&output);
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(!text.contains("PERIOD RECEIPT"));
            assert!(text.contains(&format!("All projects • {sessions} sessions")));
            assert!(text.contains(&format!("{sessions} sessions")));
            assert!(text.contains("43,873"));
            assert!(text.contains("83,800"));
            assert_eq!(text.matches("GPT-5.3 Codex").count(), 1);
            assert_eq!(text.contains("GPT-6.1 Sol"), period == "month");
            assert!(
                text.contains(if period == "week" { "$0.61" } else { "$0.76" }),
                "{text}"
            );
            assert!(!text.contains("N/A"));
            assert!(!text.contains("codex-auto-review"));
            assert!(text.lines().all(|line| line.chars().count() <= columns));
            assert!(!text.contains("Synthetic private prompt"));
            assert!(!text.contains("/home/example"));
            let output = cli(&[
                "export",
                "--sessions-dir",
                root,
                "--period",
                period,
                "--paper",
                paper,
                "--format",
                "html",
                "-o",
                "-",
            ]);
            ok(&output);
            let page = String::from_utf8(output.stdout).unwrap();
            assert!(page.contains("Codex period receipt"));
            assert!(page.contains(&format!("{sessions} sessions")));
            assert!(page.contains("<dd>43,873</dd>"));
            assert!(page.contains("<dd>83,800</dd>"));
            assert_eq!(page.matches("title=\"gpt-5.3-codex\"").count(), 1);
            assert!(!page.contains("codex-auto-review"));
        }
    }
}

#[test]
fn period_receipts_refuse_empty_ranges_duplicates_and_corrupt_rollouts() {
    let dir = Temp::new();
    let root = dir.0.to_str().unwrap();
    let output = cli(&["--sessions-dir", root, "--period", "week"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no Codex session activity"));
    let log = session_days_ago(1, "duplicate");
    fs::write(dir.0.join("one.jsonl"), &log).unwrap();
    fs::write(dir.0.join("two.jsonl"), &log).unwrap();
    let output = cli(&["--sessions-dir", root, "--period", "week"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate session ID"));
    fs::write(
        dir.0.join("two.jsonl"),
        format!("{}{{invalid}}\n", log.replace("duplicate", "corrupt")),
    )
    .unwrap();
    let output = cli(&["--sessions-dir", root, "--period", "week"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid JSON at line"));
}

#[test]
fn period_receipts_show_known_costs_with_a_star_for_partial_totals() {
    let dir = Temp::new();
    let root = dir.0.to_str().unwrap();
    fs::write(dir.0.join("known.jsonl"), session_days_ago(1, "known")).unwrap();
    for log in [
        session_days_ago(2, "unknown").replace("gpt-5.3-codex", "unknown-model"),
        format!(
            "{}\n",
            session_days_ago(2, "missing").lines().next().unwrap()
        ),
    ] {
        fs::write(dir.0.join("other.jsonl"), log).unwrap();
        let output = cli(&["--sessions-dir", root, "--period", "week"]);
        ok(&output);
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(
            text.lines()
                .any(|line| line.starts_with("TOTAL (API EST.)") && line.ends_with("$0.20*"))
        );
        assert!(text.contains("* Partial total: known costs"));
        assert!(text.contains("2 sessions"));
    }
}

#[test]
fn forked_rollouts_with_parent_headers_work_for_single_and_period_receipts() {
    let dir = Temp::new();
    let parent = session_days_ago(1, "parent-session");
    let mut child: serde_json::Value =
        serde_json::from_str(parent.lines().next().unwrap()).unwrap();
    child["payload"]["id"] = "child-session".into();
    child["payload"]["forked_from_id"] = "parent-session".into();
    fs::write(dir.0.join("fork.jsonl"), format!("{child}\n{parent}")).unwrap();
    let root = dir.0.to_str().unwrap();
    let output = cli(&["--sessions-dir", root, "--session", "child-session"]);
    ok(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("child-session"));
    for period in ["week", "month"] {
        let output = cli(&["--sessions-dir", root, "--period", period]);
        ok(&output);
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("1 session"));
        assert!(text.contains("$0.20"));
    }
}

#[test]
fn compressed_rollouts_are_discovered_and_export_like_plain_rollouts() {
    let dir = Temp::new();
    let path = dir.0.join("rollout.jsonl.zst");
    let log = fs::read(fixture()).unwrap();
    let split = log.len() / 2;
    let mut compressed = zstd::stream::encode_all(&log[..split], 0).unwrap();
    compressed.extend(zstd::stream::encode_all(&log[split..], 0).unwrap());
    fs::write(&path, &compressed).unwrap();
    let listing = cli(&["--sessions-dir", dir.0.to_str().unwrap(), "sessions"]);
    ok(&listing);
    assert!(
        String::from_utf8_lossy(&listing.stdout).contains("019e0000-0000-7000-8000-000000000001")
    );
    for format in ["text", "html"] {
        let plain = cli(&[
            "export",
            "--file",
            fixture().to_str().unwrap(),
            "--format",
            format,
            "-o",
            "-",
        ]);
        let zipped = cli(&[
            "export",
            "--file",
            path.to_str().unwrap(),
            "--format",
            format,
            "-o",
            "-",
        ]);
        ok(&plain);
        ok(&zipped);
        assert_eq!(plain.stdout, zipped.stdout);
    }
    assert_eq!(fs::read(&path).unwrap(), compressed);
}

#[test]
fn corrupt_or_truncated_compressed_rollouts_fail_with_the_file_path() {
    let dir = Temp::new();
    let path = dir.0.join("broken.jsonl.zst");
    let mut truncated =
        zstd::stream::encode_all(fs::read(fixture()).unwrap().as_slice(), 0).unwrap();
    truncated.truncate(truncated.len() - 4);
    for bytes in [b"not zstd".to_vec(), truncated] {
        fs::write(&path, bytes).unwrap();
        let output = cli(&["--file", path.to_str().unwrap()]);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("broken.jsonl.zst"));
    }
}

#[test]
fn default_periods_include_archived_sessions_and_custom_directories_stay_scoped() {
    let dir = Temp::new();
    let active = dir.0.join("sessions");
    let archived = dir.0.join("archived_sessions");
    fs::create_dir(&active).unwrap();
    fs::create_dir(&archived).unwrap();
    fs::write(active.join("recent.jsonl"), session_days_ago(1, "recent")).unwrap();
    let log = session_days_ago(10, "archived");
    fs::write(
        archived.join("old.jsonl.zst"),
        zstd::stream::encode_all(log.as_bytes(), 0).unwrap(),
    )
    .unwrap();
    for (period, count) in [("week", 1), ("month", 2)] {
        let output = Command::new(env!("CARGO_BIN_EXE_codex-receipts"))
            .env("CODEX_HOME", &dir.0)
            .args(["--period", period])
            .output()
            .unwrap();
        ok(&output);
        assert!(String::from_utf8_lossy(&output.stdout).contains(&format!("{count} session")));
    }
    let output = cli(&[
        "--sessions-dir",
        active.to_str().unwrap(),
        "--period",
        "month",
    ]);
    ok(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 session"));
}

#[test]
fn periods_reject_conflicting_selectors_and_unrelated_commands() {
    for args in [
        vec!["--period", "week", "--latest"],
        vec!["--period", "week", "--session", "id"],
        vec!["--period", "month", "--file", "rollout.jsonl"],
        vec!["--period", "month", "show", "id"],
        vec!["--period", "week", "sessions"],
        vec!["hook", "--period", "month", "--output-dir", "."],
        vec!["--period", "year"],
    ] {
        let output = cli(&args);
        assert!(!output.status.success(), "unexpected success: {args:?}");
        assert!(output.stdout.is_empty());
    }
}
