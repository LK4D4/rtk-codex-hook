# AGENTS.md

Guidance for agents developing `rtk-codex-hook`.

## Project Shape

This repo is a small Rust CLI for Codex `PreToolUse` hooks on Windows.

- `src/main.rs`: CLI entry point, stdin reading, `--explain`, and `--version`.
- `src/hook.rs`: Codex hook JSON parsing, fail-open behavior, and deny JSON output.
- `src/rewrite.rs`: conservative command tokenization and RTK rewrite suggestions.
- `tests/pretool.rs`: integration fixture that feeds Codex-style JSON to the binary.
- `README.md`: user-facing build, install, behavior, and debugging docs.

## Non-Negotiable Behavior

- The hook must fail open. Malformed JSON, missing fields, unsupported commands,
  parser uncertainty, and internal errors must exit `0` with no output.
- Emit JSON only when denying a command with a specific RTK suggestion.
- Keep deny JSON compact and shaped as:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Use RTK-optimized command instead: <suggestion>"}}
```

- Do not write to stderr during normal hook execution.
- Do not rewrite destructive or mutating PowerShell commands. When in doubt,
  return no suggestion.
- Keep rewrite behavior generic. Do not add repo-specific or language-specific
  policy unless the user explicitly asks and the behavior is designed as a
  general rule with tests.
- Preserve pasteable Windows/PowerShell suggestions, especially quoting around
  `$env:...` setup in `pwsh -Command` suggestions.

## Development Workflow

- Do not work directly on `main` unless the user explicitly asks. Use a branch
  or a linked worktree for changes.
- Keep edits small and tied to observable hook behavior.
- Prefer deterministic in-process rewrite tests. If using the external
  `rtk rewrite` path, keep it as a separate integration concern so tests do not
  depend on a locally installed RTK binary.
- Add or update `tests/pretool.rs` for every behavior change. Test both:
  - the command that should produce a deny suggestion
  - nearby commands that should fail open with no output
- Keep README behavior descriptions in sync with tested behavior.
- Commit completed work; do not leave intended repo changes unstaged or
  uncommitted.

## Validation

Run these before committing code changes:

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

For docs-only changes, run at least:

```powershell
cargo test
```

Use manual probes for tricky rewrite behavior:

```powershell
cargo run -- --explain "git status --short"
cargo run -- --explain "Get-Content -LiteralPath input.txt | Out-File output.txt"
```

## Rewrite Design Notes

- `Get-Content` redirects should preserve requested read windows with
  `--max-lines` or `--tail-lines`.
- `Select-String` redirects should preserve `-Context N` as `-C N`.
- `Get-ChildItem` file discovery should prefer `rtk rg --files`.
- Raw `rg` suggestions should quote patterns that contain alternation or other
  PowerShell-sensitive characters.
- Existing good RTK commands should be left alone.
- A correct no-op is better than a clever but risky rewrite.
