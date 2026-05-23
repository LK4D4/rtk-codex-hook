# rtk-codex-hook

[![CI](https://github.com/LK4D4/rtk-codex-hook/actions/workflows/ci.yml/badge.svg)](https://github.com/LK4D4/rtk-codex-hook/actions/workflows/ci.yml)

Cross-platform Codex `PreToolUse` hook for rewriting noisy commands into
lower-token RTK commands.

The binary reads Codex hook JSON from stdin. For safe equivalent rewrites, it
lets Codex run the RTK-shaped command through `updatedInput.command`.
Unsupported, ambiguous, malformed, and mutating commands exit `0` with no output
so the hook fails open. Non-equivalent guidance may still deny with a visible
reason.

## Install

Ask Codex:

```text
Install rtk-codex-hook from https://github.com/LK4D4/rtk-codex-hook on this machine.
```

Codex can install from the GitHub release without a local repo checkout.

Agent-facing install steps live in [`CODEX_INSTALL.md`](CODEX_INSTALL.md).

## Behavior

`rtk-codex-hook` reads Codex `PreToolUse` JSON from stdin and stays quiet unless
it can safely rewrite or explicitly guide a command. Ambiguous, unsupported,
malformed, or mutating commands fail open with no output.

Repo-tested Codex usage guidance lives in [`RTK.md`](RTK.md).

Developer docs, release steps, debugging, and full rewrite details live in
[`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md).
