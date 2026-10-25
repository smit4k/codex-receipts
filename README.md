# codex-receipts

Turn local Codex sessions into plain-text or HTML receipts.

## Install

Requires Rust 1.87 or later.

```sh
cargo install --path . --locked
```

## Usage

```sh
# Show the latest session
codex-receipts

# List sessions
codex-receipts sessions

# Show a session
codex-receipts show <session-id>

# Show recent activity
codex-receipts --period week
codex-receipts --period month

# Export a receipt
codex-receipts export --format html -o receipt.html
codex-receipts export --format text -o receipt.txt

# Open the print preview
codex-receipts print
```

Use `codex-receipts --help` for all options.

## Codex skill

Install the optional Codex skill to generate receipts from a session:

```sh
mkdir -p ~/.agents/skills
cp -R skills/codex-receipts ~/.agents/skills/
```

Then ask Codex to generate a receipt, or invoke `$codex-receipts` directly.

## Development

```sh
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
```

## License

See the repository license for details.
