# codex-receipts

Turn local Codex coding sessions into a thermal-style receipt: one session, the past week, or the past month, as plain text or a self-contained, print-ready HTML file. One Rust binary, no service or API key.

## Install and run

Requires Rust 1.87 or newer (edition 2024).

```sh
cargo install --path . --locked
codex-receipts
```

The default shows the session with the newest **start timestamp** under `$CODEX_HOME/sessions`, or `~/.codex/sessions` when `CODEX_HOME` is unset. Both plain `.jsonl` and Zstandard-compressed `.jsonl.zst` rollouts are supported, including direct `--file` reads. Compressed logs are streamed without unpacking or changing files on disk. Resuming an older session does not make it the newest. Use its ID or rollout path to select it explicitly.

```sh
codex-receipts sessions
codex-receipts show 019e0000-0000-7000-8000-000000000001
codex-receipts --session 019e0000
codex-receipts --file /path/to/rollout.jsonl
codex-receipts --sessions-dir /path/to/sessions
codex-receipts --period week
codex-receipts --period month
codex-receipts print
codex-receipts print --period week --paper 58

codex-receipts export --format html -o receipt.html
codex-receipts export --format text -o receipt.txt
codex-receipts export --period week --format html -o week.html
codex-receipts export --period month --format text -o month.txt
codex-receipts --help
```

Session prefixes must be unique; exact IDs take precedence. Selection and `--paper` options work before or after the subcommand. Exports never overwrite files. `-o -` writes to stdout. All displayed dates are UTC.

`--period week` combines activity from the trailing 7 days; `--period month` goes back one calendar month from the current UTC time, clamping to the last day of a shorter month. These are rolling windows, not the previous calendar week or month. Period receipts use a single `All projects • 42 sessions` header and label duration in days (for example, `7d` or `30d`) and show the UTC start and end, the number of sessions with recorded activity, and combined token counts and costs grouped by model. Both paper widths and export formats work. Period selection cannot be combined with `--file`, `--session`, `--latest`, or the `show`, `sessions`, and `hook` commands.

Period accounting uses token snapshot timestamps, including activity from sessions started before the window. It subtracts the preceding cumulative snapshot, includes increments recorded at or after the start, and excludes records at or after the end. An increment belongs to the time its snapshot was recorded; usage between snapshots cannot be split more finely.

To include resumed sessions, a period receipt scans all discovered plain and compressed rollouts. Default period receipts also include the sibling `archived_sessions` directory when present; an explicit `--sessions-dir` stays limited to that directory. Missing token usage, untimed usage, a missing baseline for an older session, or counter corrections inside the window make the combined total partial: available model rows are retained, known costs are summed, and the total gets an asterisk. Empty windows, duplicate session IDs, and corrupt complete records fail clearly. Forks can still contain inherited usage, as described below.

HTML uses a narrow paper layout with a shared vector mark, model-by-model token counts and subtotals, and a prominent total. Input rows exclude cache reads and writes, which have their own rows. Open HTML exports in a browser to print or save as PDF; select the matching paper size in the print dialog. HTML embeds its CSS and has no external assets or scripts. The receipt contains only the project folder's basename, session ID, timestamps, usage, models, and estimates; prompts, full paths, and tool output are excluded. Receipt metadata supports printable ASCII and `•`, replacing other characters with `?`, to keep columns stable and prevent terminal control sequences. Interactive terminals get a block mark and bold/dim typography; redirected output and text exports stay unstyled. Set `NO_COLOR` or `TERM=dumb` to disable terminal styling. Review project names and session IDs before sharing.

## Example

```text
               ###
                 ##
               ###    #####

      C O D E X  /  R E C E I P T S

         Project: codex-receipts
             SESSION RECEIPT
   019e0000-0000-7000-8000-000000000001
         2026-10-24 19:00:00 UTC

==========================================
GPT-5.3 Codex                        $0.20
------------------------------------------
  Input (uncached)                  14,891
  Output tokens                     12,304
  Cache write                            0
  Cache read                        28,000

==========================================
TOTAL (API EST.)                     $0.20
==========================================

Elapsed                           3h 0m 0s
CASHIER: Codex

         Thank you for building!
```

Illustrative session data; this estimate uses the bundled pricing table.

## Usage and cost accounting

The parser reads JSONL `session_meta`, `turn_context`, and `event_msg` records. It ignores unrelated records and uses the latest cumulative `total_token_usage`, without adding repeated snapshots or adding reasoning tokens again to output. `token_count` records with null/missing info are skipped. Counts must be nonnegative and cache categories must fit within input totals. A corrupt complete record produces an error with a line number; an incomplete final JSON record is ignored with a receipt note so live logs can be read.

The first metadata header determines the receipt's session identity, project, and start time. Repeated headers for that session or its declared `forked_from_id` ancestor chain are validated and ignored without resetting usage. Headers for unrelated sessions fail clearly.

Model switches and `model_reroute` events attribute cumulative increments to the current model. `codex-auto-review` is a free safety call; its attributed tokens and model row are excluded from receipts. If counters decrease or a correction makes increments inconsistent, single-session totals follow the latest snapshot and previous model attribution becomes unknown; partial costs are marked with an asterisk. Forked transcripts may include inherited usage; estimates cover the recorded cumulative history. Single-session receipts do not add parallel child sessions. Period receipts include all discovered sessions with activity in the window.

`Elapsed` runs from session start to the latest recorded event. An unfinished turn adds a snapshot note.

[The pricing table](data/pricing.json) is versioned **2026-10-24**, sourced from [OpenAI API pricing](https://developers.openai.com/api/docs/pricing). It contains exact model IDs for GPT-6 Astra, GPT-6.1 Sol, GPT-6 Luna, GPT-5.6 Sol, and GPT-5.3 Codex. Rates are integer milli-USD per million tokens, and arithmetic uses integer nano-USD, rounding only the displayed result to the nearest cent (half up).

```text
cost = ((input - cached) × input_rate
        + cached × cached_rate
        + output × output_rate) / 1,000,000
```

This is a **standard short-context API-equivalent estimate**, not an amount charged. It excludes service-tier and long-context premiums, regional processing, tool fees, taxes, and subscription accounting. Unknown models, unknown attribution, and nonzero cache-write usage show `N/A` on their model rows. The total sums known costs and adds `*` when any costs or usage are unavailable, for example `$49.08*`, with a note explaining that it is partial. If no costs are known, the partial total is `$0.00*`. Model aliases or snapshots are not guessed. Small estimates can round to `$0.00`. Pricing is bundled, so exports work offline; edit the table and rebuild when rates change. GPT-5.6 Sol pricing is promotional and may change.

`codex-receipts print` opens the receipt in a native preview window and automatically opens the print dialog after the HTML loads. It accepts the same `--file`, `--session`, `--latest`, `--period`, and `--paper` selectors as display and export. Choose your printer and matching paper size in the dialog, then print or cancel. Close the receipt window when finished to return to the shell. Receipt HTML stays in memory, and exported HTML is unchanged.

Printing uses macOS WebKit (macOS 11+), the Windows WebView2 Runtime, or Linux GTK 3 and WebKitGTK 4.1. Windows installations missing WebView2 must install [Microsoft's WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/). Linux requires a graphical desktop (X11 or Wayland) and the GTK/WebKitGTK libraries. On Debian/Ubuntu, install build dependencies before `cargo install`:

```sh
sudo apt install build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libxdo-dev libdbus-1-dev
```

Native printing adds desktop-library dependencies to Linux builds; text and HTML exports still work without a running desktop. Physical ESC/POS export and direct network printing remain deferred to the `codex/thermal-printer` working branch.

## Codex integration

A packaged [Codex skill](skills/codex-receipts/SKILL.md) uses the current session's explicit ID or transcript path. Install it into your user skill directory, then restart Codex if needed:

```sh
mkdir -p ~/.agents/skills
cp -R skills/codex-receipts ~/.agents/skills/
```

Invoke it explicitly with `$codex-receipts`, or ask Codex: “Generate a receipt for this session and export it as HTML.” See the official [skills documentation](https://learn.chatgpt.com/docs/build-skills).

The optional `hook` subcommand reads the standard [SessionEnd JSON input](https://learn.chatgpt.com/docs/hooks) from stdin and writes `<session_id>.html` to the specified directory. It checks that the event ID matches the transcript. It does not discover sessions automatically.

```sh
codex-receipts hook --output-dir /absolute/path/to/receipts
```

For example, merge the following into `~/.codex/hooks.json`, replacing the output path:

```json
{
  "hooks": {
    "SessionEnd": [{
      "hooks": [{
        "type": "command",
        "command": "codex-receipts hook --output-dir /absolute/path/to/receipts",
        "timeout": 3
      }]
    }]
  }
}
```

Use an installed binary on `PATH` or its absolute path. Review and trust the hook in Codex. `SessionEnd` hooks have a three-second maximum timeout; very large transcripts can exceed that budget, so run an explicit export for those sessions. The hook receives `session_id`, `transcript_path`, and `hook_event_name: "SessionEnd"` on stdin. Existing receipts are preserved; a repeated event for the same ID fails clearly. No Codex settings are changed by installing this binary.

## Development

```sh
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Tests use sanitized synthetic fixtures modeled on local Codex rollout records. Coverage includes cumulative snapshots, multiple models, missing usage, counter corrections, idle time, malformed/truncated input, precise costs, safe renderers, session discovery, exports, and hook handling.

Full-output snapshots in `tests/fixtures/` pin the approved 58 mm and 80 mm plain text, styled terminal output, and HTML (including its embedded CSS and SVG mark). Terminal snapshots store escape bytes as literal `\x1b` so diffs remain readable. The multi-model fixture exercises repeated model blocks and subtotals. Review rendered output before updating these snapshots; an unexpected change should fail the tests.

On macOS and Linux, the integration suite uses the system `script` utility to run the binary in a real pseudoterminal and verify styling, `NO_COLOR`, and `TERM=dumb`. Windows still runs the renderer and redirected-output tests. Other cases cover unknown models, empty receipts, long metadata, maximum token counts, HTML escaping in every metadata field, print command input validation, and the absence of raw printer exports. The interactive print smoke test is ignored by default; run `cargo test --test cli native_print_dialog_opens_with_a_fixture_receipt -- --ignored` and close the receipt window to finish. No printer or network access is needed for this release's tests.

Themes, QR codes, arbitrary date ranges, Git metadata, interactive selection, and additional agent formats are intentionally left for later.
