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
