---
description: Run cargo fmt --all and cargo clippy --all --no-deps -- -D warnings. Also runs tome-web lint if requested.
allowed-tools: Bash
---

Run the following checks in order and report results:

1. `cargo fmt --all`
2. `cargo clippy --all --no-deps -- -D warnings`

If `$ARGUMENTS` contains "web" or "all", also run:

3. `cd tome-web && npm run format && npm run lint`

Stop immediately and report any failures with the full error output.
