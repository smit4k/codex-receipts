---
name: codex-receipts
description: Generate text or print-ready HTML receipts for Codex sessions with the codex-receipts CLI. Use when the user asks for a session receipt or session token cost estimate.
---

Use the installed `codex-receipts` binary. If it is missing, report that it can be installed from the project with `cargo install --path . --locked`.

Prefer the current session's explicit transcript path (`--file PATH`) or session ID (`--session ID`) from trusted runtime context. `CODEX_THREAD_ID`, when supplied by the runtime, can identify the current session. Do not infer the current conversation from modification times. If the current session cannot be identified, ask which session to use or offer `codex-receipts sessions`; use the default/latest session only when the user requests it.

Examples:

```sh
codex-receipts --file /absolute/path/to/rollout.jsonl
codex-receipts export --session SESSION_ID --format html -o receipt.html
codex-receipts export --session SESSION_ID --format html --paper 58 -o receipt-58.html
```

Exports refuse to overwrite existing files. Select another filename if needed. Return a link to the exported file.

Describe costs as standard-rate API estimates. Unknown models and cache-write usage produce `N/A`; the estimate excludes context/service premiums and tool fees and does not measure a ChatGPT subscription charge. An open session receipt is a snapshot. Receipts include a project basename and session ID, but no prompts, tool output, or full working directory.
