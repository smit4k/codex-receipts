---
name: codex-receipts
description: Generate text or print-ready HTML receipts for Codex sessions with the codex-receipts CLI. Use when the user asks for a session receipt, weekly or monthly receipt, or session token cost estimate.
---

Use the installed `codex-receipts` binary. If it is missing, report that it can be installed from the project with `cargo install --path . --locked`.

Prefer the current session's explicit transcript path (`--file PATH`) or session ID (`--session ID`) from trusted runtime context. `CODEX_THREAD_ID`, when supplied by the runtime, can identify the current session. Do not infer the current conversation from modification times. If the current session cannot be identified, ask which session to use or offer `codex-receipts sessions`; use the default/latest session only when the user requests it.

For a receipt covering the past 7 days or one month across sessions, use `--period week` or `--period month` without a session selector. These rolling UTC windows include recorded activity from resumed sessions, combine usage by model, and show the range and session count. Discovery reads both `.jsonl` and `.jsonl.zst` logs; default period receipts also include `archived_sessions`. An explicit `--sessions-dir` stays limited to that directory. Month means one calendar month back, clamped for shorter months. No current-session ID is needed. Missing usage or uncertain attribution makes the total partial: known costs are summed and marked with `*`.

Examples:

```sh
codex-receipts --file /absolute/path/to/rollout.jsonl
codex-receipts export --session SESSION_ID
codex-receipts export --session SESSION_ID --paper 58 -o receipt-58.html
codex-receipts --period week
codex-receipts export --period month -o month.html
codex-receipts print --period week
```

When the user asks to print, use `codex-receipts print` with the same session or period selector and optional `--paper 58`. It opens a native receipt window and print dialog; the user chooses print or cancel and closes the receipt window when finished. A graphical desktop and the platform WebView runtime are required.

Exports default to HTML in `receipt.html` in the current working directory. `--format text` (or `ascii`) defaults to `receipt.txt`; `-o PATH` overrides the filename and `-o -` writes to stdout. Exports refuse to overwrite existing files. Select another filename if needed. Return a link to the exported file.

Describe costs as standard-rate API estimates. Free `codex-auto-review` safety calls are excluded. Unknown models and cache-write usage produce `N/A` on model rows; totals sum known costs and add `*` for unavailable costs or usage, with `$0.00*` when none are known. The estimate excludes context/service premiums and tool fees and does not measure a ChatGPT subscription charge. An open session receipt is a snapshot. Receipts include a project basename and session ID, but no prompts, tool output, or full working directory.
