# rtk-codex-hook

[![CI](https://github.com/LK4D4/rtk-codex-hook/actions/workflows/ci.yml/badge.svg)](https://github.com/LK4D4/rtk-codex-hook/actions/workflows/ci.yml)

Cross-platform Codex `PreToolUse` hook for suggesting lower-token RTK commands.

The binary reads Codex hook JSON from stdin. It prints JSON only when it wants
Codex to deny a command and show a better RTK-shaped command. Every no-op,
unsupported command, malformed payload, and internal error exits `0` with no
output so the hook fails open.

## Install

Ask Codex:

```text
Install rtk-codex-hook from https://github.com/LK4D4/rtk-codex-hook on this machine.
```

Codex can install from the GitHub release without a local repo checkout.

Agent-facing install steps live in [`CODEX_INSTALL.md`](CODEX_INSTALL.md).

## Behavior

`rtk-codex-hook` reads Codex `PreToolUse` JSON from stdin and stays quiet unless
it can suggest a safer RTK-shaped command. Ambiguous, unsupported, malformed, or
mutating commands fail open with no output.

Developer docs, release steps, debugging, and full rewrite details live in
[`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md).
