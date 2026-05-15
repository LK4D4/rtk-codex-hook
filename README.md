# rtk-codex-hook

[![CI](https://github.com/LK4D4/rtk-windows-codex-hook/actions/workflows/ci.yml/badge.svg)](https://github.com/LK4D4/rtk-windows-codex-hook/actions/workflows/ci.yml)

Windows-first Codex `PreToolUse` hook for suggesting lower-token RTK commands.

The binary reads Codex hook JSON from stdin. It prints JSON only when it wants
Codex to deny a command and show a better RTK-shaped command. Every no-op,
unsupported command, malformed payload, and internal error exits `0` with no
output so the hook fails open.

## Build

```powershell
cargo build --release
```

The binary is written to:

```text
target\release\rtk-codex-hook.exe
```

Useful local checks:

```powershell
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## Codex Hook Setup

Point your Codex `hooks.json` entry at the compiled binary. For example:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "command": "C:\\Users\\you\\bin\\rtk-codex-hook.exe"
      }
    ]
  }
}
```

Codex sends hook JSON on stdin. When a redirect applies, the hook emits compact
JSON like:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Use RTK-optimized command instead: rtk git status --short"}}
```

The process exits `0` for both deny suggestions and no-op cases.

## What It Rewrites

- Already preferred RTK commands are left alone, such as `rtk git status --short`.
- Mutating PowerShell commands are left alone, including `Remove-Item`,
  `Set-Content`, `Add-Content`, `New-Item`, `Move-Item`, and `Copy-Item`.
- Windows and PowerShell-specific reads/searches are handled locally first.
  Generic cross-platform tools are delegated to `rtk rewrite`, such as
  `git status --short` to `rtk git status --short` and `ls src` to
  `rtk ls src`.
- If `rtk rewrite` is unavailable or returns no suggestion, a small local
  fallback preserves legacy redirects for common noisy tools such as `git`,
  `cargo`, `npm`, `pytest`, `busted`, `luacheck`, `dotnet`, `pnpm`, `pip`, `go`,
  `docker`, `npx`, `vitest`, `jest`, `tsc`, `ruff`, `mypy`, `playwright`,
  `gradlew`, and `curl`.
- RTK's generic safety skips are preserved. For example, `gh` commands with
  `--json`, `--jq`, or `--template` are left alone so structured output remains
  raw.
- `python -m pytest ...` and `uv run pytest ...` become direct `rtk pytest ...`
  suggestions.
- Direct and PowerShell-wrapped `Get-Content` reads, including `gc`, `cat`, and
  `type`, become `rtk read`, preserving top or tail windows with `--max-lines`
  and `--tail-lines`.
- `Get-Content ... | Select-String ...` pipelines, including alias forms, become
  `rtk grep -n` searches rather than `rtk read`.
- PowerShell wrappers around `busted` and `luacheck` keep the `pwsh` wrapper
  for environment setup while applying `rtk` to the noisy inner tool. Existing
  unsupported `rtk pwsh ...` commands are rewritten back to `pwsh ...`.
- Direct and wrapped PowerShell `Select-String`/`sls` becomes `rtk grep -n` for
  simple `-Path`/`-Pattern` forms, with optional `-Context N` mapped to `-C N`.
  Other switches are left alone.
- Direct and wrapped PowerShell `Get-ChildItem`/`gci`/`dir` file discovery with
  an explicit path plus both `-Recurse` and `-File` becomes `rtk find`.
  Other switches such as `-Directory` or `-Filter` are left alone, as are bare
  Windows aliases such as `ls` or `dir` with no path.
- Simple non-recursive `findstr /N pattern path` searches become `rtk grep -n`;
  recursive `/S` searches and other complex `findstr` modes are left alone.
- Raw `rg` content searches become `rtk grep`, and `rg --files` file discovery
  becomes `rtk find`. These stay local because the hook quotes PowerShell-
  sensitive patterns and maps file discovery to `rtk find`.

The rewrite rules are intentionally generic. There is no repo-specific or
language-specific policy in this hook; smarter language-aware behavior can be
added later when it has a general design.

## Debugging

Explain a single command without Codex JSON:

```powershell
rtk-codex-hook.exe --explain "git status --short"
```

Print the binary version:

```powershell
rtk-codex-hook.exe --version
```

Set `RTK_CODEX_HOOK_LOG` to a file path to enable best-effort logging:

```powershell
$env:RTK_CODEX_HOOK_LOG = "$env:TEMP\rtk-codex-hook.log"
```

Logging failures are ignored so the hook still fails open.

## Status

This is Windows/PowerShell-first. The tokenizer is conservative and recognizes
the command shapes Codex commonly emits on Windows. If a command is ambiguous,
destructive, unsupported, or cannot be parsed with confidence, the hook prints
nothing and lets Codex continue.
