# codex-receipts

Turn a local Codex coding session into a thermal-style receipt: plain text or a self-contained, print-ready HTML file. One Rust binary, no service or API key.

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

codex-receipts export --format html -o receipt.html
codex-receipts export --format text -o receipt.txt
codex-receipts --help
```

Session prefixes must be unique; exact IDs take precedence. Selection and `--paper` options work before or after the subcommand. Exports never overwrite files. `-o -` writes to stdout. All displayed dates are UTC.

HTML uses a narrow paper layout with a shared vector mark, model-by-model token counts and subtotals, and a prominent total. Input rows exclude cache reads and writes, which have their own rows. Open HTML exports in a browser to print or save as PDF; select the matching paper size in the print dialog. HTML embeds its CSS and has no external assets or scripts. The receipt contains only the project folder's basename, session ID, timestamps, usage, models, and estimates; prompts, full paths, and tool output are excluded. Receipt metadata uses printable ASCII, replacing other characters with `?`, to keep columns stable and prevent terminal control sequences. Interactive terminals get a block mark and bold/dim typography; redirected output and text exports stay plain ASCII. Set `NO_COLOR` or `TERM=dumb` to disable terminal styling. Review project names and session IDs before sharing.

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
Active turns                        0m 11s
CASHIER: Codex

         Thank you for building!

            Estimated API cost
       Standard short-context rates
    Excludes premiums, tools & extras
        Not a subscription charge
            Prices: 2026-10-24
```

Illustrative session data; this estimate uses the bundled pricing table.

## Usage and cost accounting

The parser reads JSONL `session_meta`, `turn_context`, and `event_msg` records. It ignores unrelated records and uses the latest cumulative `total_token_usage`, without adding repeated snapshots or adding reasoning tokens again to output. `token_count` records with null/missing info are skipped. Counts must be nonnegative and cache categories must fit within input totals. A corrupt complete record produces an error with a line number; an incomplete final JSON record is ignored with a receipt note so live logs can be read.

Model switches and `model_reroute` events attribute cumulative increments to the current model. If counters decrease or a correction makes increments inconsistent, totals follow the latest snapshot and previous model attribution becomes unknown; a partial cost is never presented as a complete total. Forked transcripts may include inherited usage; estimates cover the recorded cumulative history. Parallel child sessions are separate rollouts and are not automatically added.

`ELAPSED` runs from session start to the latest recorded event. `ACTIVE TURNS` sums recorded `task_started`/`task_complete` (or `turn_started`/`turn_complete`) intervals, including tool waits. It is not CPU time. It shows `N/A` when lifecycle records are absent. An unfinished turn counts through the last recorded event and adds a snapshot note.

[The pricing table](data/pricing.json) is versioned **2026-10-24**, sourced from [OpenAI API pricing](https://developers.openai.com/api/docs/pricing). It contains exact model IDs for GPT-6 Astra, GPT-6.1 Sol, GPT-6 Luna, GPT-5.6 Sol, and GPT-5.3 Codex. Rates are integer milli-USD per million tokens, and arithmetic uses integer nano-USD, rounding only the displayed result to the nearest cent (half up).

```text
cost = ((input - cached) × input_rate
        + cached × cached_rate
        + output × output_rate) / 1,000,000
```

This is a **standard short-context API-equivalent estimate**, not an amount charged. It excludes service-tier and long-context premiums, regional processing, tool fees, taxes, and subscription accounting. Unknown models, unknown attribution, missing usage, and nonzero cache-write usage show `N/A` for the total. Model aliases or snapshots are not guessed. Small estimates can round to `$0.00`. Pricing is bundled, so exports work offline; edit the table and rebuild when rates change. GPT-5.6 Sol pricing is promotional and may change.

Physical ESC/POS export and network printing are deferred to the `codex/thermal-printer` working branch for a later release. The current release supports text and printable HTML, with 80 mm and 58 mm layouts.

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

On macOS and Linux, the integration suite uses the system `script` utility to run the binary in a real pseudoterminal and verify styling, `NO_COLOR`, and `TERM=dumb`. Windows still runs the renderer and redirected-output tests. Other cases cover unknown models, empty receipts, long metadata, maximum token counts, HTML escaping in every metadata field, and the absence of deferred printer commands. No printer or network access is needed for this release's tests.

Themes, QR codes, daily summaries, Git metadata, interactive selection, and additional agent formats are intentionally left for later.
