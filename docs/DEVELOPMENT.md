# Development

Developer notes for `rtk-codex-hook`.

## Build From Source

```powershell
cargo build --release
```

The binary is written to:

```text
target/release/rtk-codex-hook
target\release\rtk-codex-hook.exe
```

Useful local checks:

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

## Release

Release artifacts are built by `.github/workflows/release.yml` when a tag like
`v0.1.0` is pushed. The workflow publishes platform archives and `SHA256SUMS` to
the GitHub release. `Cargo.toml` and `Cargo.lock` are the version source of
truth. The release workflow verifies that the tag, such as `v0.1.0`, matches the
package version, such as `0.1.0`, and fails before building if they diverge.
Builds run with `--locked`.

Before tagging, bump the package version and lockfile together:

```powershell
cargo update -p rtk-codex-hook --precise 0.1.0
```

```powershell
git tag v0.1.0
git push origin v0.1.0
```

## Architecture

- `src/main.rs` handles CLI args, `--version`, `--explain`, stdin reads, and
  top-level fail-open error handling.
- `src/hook.rs` parses Codex hook JSON, extracts the submitted command, and
  formats deny JSON.
- `src/rewrite.rs` tokenizes shell-ish command strings and returns conservative
  RTK suggestions.
- `tests/pretool.rs` exercises Codex-style hook payloads end-to-end.
- `tests/cli.rs` covers CLI behavior that should stay outside hook payloads.

The hook has two public modes:

- Hook mode: read Codex JSON from stdin and print deny JSON only for a concrete
  suggestion.
- Explain mode: `rtk-codex-hook --explain "<command>"` prints the suggestion
  directly for local debugging.

## Hook Contract

The hook must fail open. Malformed JSON, missing fields, unsupported commands,
parser uncertainty, and internal errors exit `0` with no output.

Deny output stays compact:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Use RTK-optimized command instead: <suggestion>"}}
```

Normal hook execution should not write stderr. Logging is best-effort and must
not make the hook fail closed.

## Rewrite Model

The rewrite layer is intentionally generic. It should not add repo-specific or
language-specific policy unless that behavior is designed as a general rule with
tests.

Rewrite order:

1. Skip commands that are already good RTK forms.
2. Reject destructive or mutating command shapes.
3. Apply local high-confidence Windows/PowerShell and Unix read/search rewrites.
4. Delegate generic commands to `rtk rewrite` when available.
5. Use small local fallbacks for known noisy tools if delegation is unavailable.
6. Return no suggestion when parsing is ambiguous.

## What It Rewrites

- Already preferred RTK commands are left alone, such as `rtk git status --short`.
  Invalid `rtk read` forms with invented window flags such as `--line`,
  `--lines`, `--range`, `--start-line`, `--start`, `--from`, `--to`, or
  `--line-number` are denied with `rtk read --help` so agents do not confuse RTK
  parse fallback errors with a missing `rtk read` command.
- Mutating PowerShell commands are left alone, including `Remove-Item`,
  `Set-Content`, `Add-Content`, `New-Item`, `Move-Item`, and `Copy-Item`.
- Windows/PowerShell and Unix shell reads/searches are handled locally first.
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
- Unix shell reads become `rtk read` for conservative single-file forms:
  `cat file`, `head -n N file`, `head -N file`, `tail -n N file`, and
  `tail -N file`. Simple top-of-file `sed -n '1,Np' file` reads become
  `rtk read --max-lines N`, and `nl -ba file` becomes `rtk read -n file`.
  Multi-file reads, redirects, mutating `sed` forms, and non-top line ranges are
  left alone because `rtk read` has no start-line option.
- Unix shell `grep -n pattern path...` and `grep --line-number pattern path...`
  become `rtk grep -n`; recursive, inverted, or otherwise complex grep forms
  are left alone.
- Unix shell `find path -type f` becomes `rtk find path`. More complex `find`
  predicates and mutating actions are left alone.
- `Get-Content ... | Select-String ...` pipelines, including alias forms, become
  `rtk grep -n` searches rather than `rtk read`.
- PowerShell wrappers around `busted` and `luacheck` keep the `pwsh` wrapper
  for environment setup while applying `rtk` to the noisy inner tool. Existing
  unsupported `rtk pwsh ...` commands are rewritten back to `pwsh ...`.
- Unix `bash -c`/`bash -lc` and `env VAR=...` wrappers around `busted` and
  `luacheck` keep the wrapper or environment setup raw while applying `rtk` to
  the noisy inner tool. Complex shell control flow, pipes, redirects, and syntax
  checks such as `bash -n` are left alone.
- Direct and wrapped PowerShell `Select-String`/`sls` becomes `rtk grep -n` for
  simple `-Path`/`-Pattern` forms, with optional `-Context N` mapped to
  ripgrep passthrough args after `--`, for example `-- -C N`. Other switches are
  left alone.
- Direct and wrapped PowerShell `Get-ChildItem`/`gci`/`dir` file discovery with
  an explicit path plus both `-Recurse` and `-File` becomes `rtk find`.
  Other switches such as `-Directory` or `-Filter` are left alone, as are bare
  Windows aliases such as `ls` or `dir` with no path.
- Simple non-recursive `findstr /N pattern` searches become `rtk grep -n`;
  recursive `/S` searches and other complex `findstr` modes are left alone.
- Raw `rg` content searches become `rtk grep`, and plain `rg --files` file
  discovery becomes `rtk find "*" <path> --max 50 --file-type f`. Piped,
  redirected, hidden, glob-filtered, multi-root, and otherwise flagged
  `rg --files` forms are left alone because `rtk find` output and hidden-file
  semantics are not equivalent. Supported ripgrep search flags are passed after
  `--` so `rtk grep` does not parse them as RTK options. Unsupported `rg` modes
  such as JSON output or multiple explicit `-e` patterns are left alone.

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

Useful probes:

```powershell
cargo run -- --explain "git status --short"
cargo run -- --explain "Get-Content -LiteralPath input.txt | Out-File output.txt"
```
