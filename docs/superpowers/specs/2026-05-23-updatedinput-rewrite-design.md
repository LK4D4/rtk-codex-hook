# UpdatedInput Rewrite Design

## Goal

Update `rtk-codex-hook` to use Codex's current `PreToolUse` rewrite contract for
commands that are safe to execute automatically: when the hook finds a proven
equivalent RTK replacement for a Bash command, it should allow the tool call and
return the replacement command in `updatedInput.command` instead of denying the
original command with a suggestion message.

The hook remains a token-saving convenience layer, not a security boundary.
Official Codex hooks docs state that `PreToolUse` interception is incomplete for
some shell paths and should be treated as a guardrail.

Source: <https://developers.openai.com/codex/hooks>

## Current State

Current hook behavior lives in:

- `src/hook.rs`: parses `PreToolUse` JSON, calls `rewrite::suggest`, and emits
  compact deny JSON.
- `src/rewrite.rs`: returns conservative RTK command strings. This layer already
  owns Windows/PowerShell, Unix shell, `rg`, and delegated `rtk rewrite` logic.
- `tests/pretool.rs`: integration tests assert `permissionDecision: "deny"` and
  `permissionDecisionReason`.
- `README.md`, `CODEX_INSTALL.md`, and `docs/DEVELOPMENT.md`: describe the old
  deny-and-suggest behavior. `CODEX_INSTALL.md` currently says not to use
  `updatedInput`, which is now stale.

Baseline before this spec: `cargo test` passed with 19 tests.

## Target Hook Contract

For safe equivalent rewrites, hook output should be compact JSON:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":{"command":"rtk git status --short"}}}
```

Rules:

- Emit JSON only for a concrete, safe command rewrite.
- Use `permissionDecision: "allow"` with `updatedInput.command` for Bash command
  rewrites.
- Do not include `permissionDecisionReason` for normal allowed rewrites. It is
  not needed and would make output noisier.
- Keep `--explain` unchanged: it prints only the suggested command string for
  local debugging.
- Keep fail-open behavior unchanged: malformed JSON, non-`PreToolUse` events,
  missing `tool_input.command`, empty commands, unsupported commands, parser
  uncertainty, `rtk rewrite` misses, and internal errors exit `0` with no
  output.
- Keep normal hook execution quiet on stderr.
- Continue best-effort logging only when `RTK_CODEX_HOOK_LOG` is set.
- Do not use `updatedInput` for rewrites that are merely plausible, delegated,
  wrapper-heavy, or not execution-proved.

## Deny Versus Allow

The current deny model is forgiving: a bad suggestion is visible feedback and the
original command does not run. `updatedInput` is less forgiving: a bad rewrite
executes automatically. The implementation should therefore be hybrid.

Outcomes:

- `AutoRewrite`: output `permissionDecision: "allow"` plus
  `updatedInput.command`.
- `DenySuggestion`: output `permissionDecision: "deny"` plus
  `permissionDecisionReason`.
- `Noop`: output nothing and fail open.

Only locally parsed, equivalence-proved, execution-probed command shapes should
start as `AutoRewrite`. Other suggestions stay `DenySuggestion` or `Noop` until
they have direct evidence.

The special invalid `rtk read` flag case needs explicit product decision:

- Current behavior: deny invented flags such as `rtk read --line` and suggest
  `rtk read --help`.
- Problem: `rtk read --help` is not semantically equivalent to the original
  command. Auto-rewriting to help output would execute a different operation.
- Spec decision: preserve deny for this non-equivalent correction. This keeps
  accidental bad RTK invocations visible and prevents silent execution of help
  output.

Implementation should split rewrite results into explicit actions:

```rust
enum HookAction {
    AutoRewrite(String),
    DenyWithReason(String),
    Noop,
}
```

The final implementation must have one clear path for automatic equivalent
rewrites, one clear path for visible non-equivalent deny guidance, and one clear
path for fail-open no output. If preserving `rewrite::suggest` as a low-level
string helper makes the first patch smaller, add a classifier above it rather
than letting all suggestions auto-execute.

Recommended first-pass classification:

- `AutoRewrite`: local deterministic read/search/discovery rewrites with direct
  test and probe coverage.
- `DenySuggestion`: invalid `rtk read` flag help; external `rtk rewrite` results;
  broad noisy-tool fallback suggestions; wrapper-heavy rewrites until execution
  probes cover them on the target platform.
- `Noop`: destructive, mutating, redirected, ambiguous, unsupported, or parse
  uncertain commands.

## Windows And Unix Implications

Windows-specific behavior must stay conservative:

- Direct PowerShell reads such as `Get-Content`, `gc`, `cat`, and `type` rewrite
  to `rtk read` only for safe read shapes.
- PowerShell `Select-String` and `Get-Content | Select-String` rewrite to
  `rtk grep -n`.
- `Get-ChildItem`/`gci`/`dir` with explicit path plus `-Recurse -File` rewrites
  to `rtk find`.
- PowerShell wrappers around `busted` and `luacheck` must keep pasteable
  PowerShell quoting around `$env:...` setup and insert `rtk` only around the
  noisy inner command. They should remain `DenySuggestion` until a Windows
  execution probe proves the rewritten wrapper is valid in real `pwsh`.
- Mutating PowerShell commands and output redirects keep failing open with no
  output.

Unix-specific behavior must also stay conservative:

- `cat file`, top-of-file `head`, tail reads, simple `sed -n '1,Np'`, and
  `nl -ba file` rewrite to `rtk read` forms only when semantics match.
- Simple `grep -n pattern path...` rewrites to `rtk grep -n`.
- Simple `find path -type f` rewrites to `rtk find path`.
- `bash -c`/`bash -lc` and `env VAR=...` wrappers around supported test tools
  preserve the wrapper and environment setup, rewriting only the inner noisy
  command. They should remain `DenySuggestion` until a Unix execution probe
  proves the rewritten wrapper is valid in real `bash`.
- Complex control flow, pipes, redirects, non-top line ranges, recursive grep,
  inverted grep, complex find predicates, and mutating `sed` forms keep failing
  open with no output.

Cross-platform behavior:

- The generated `updatedInput.command` must be exactly the command Codex should
  run in the same shell context it was about to use.
- Do not normalize Windows paths into Unix paths or vice versa.
- Do not add a shell wrapper just to use `updatedInput`.
- Do not rewrite if quoting preservation is uncertain.

## External `rtk rewrite`

The hook may continue delegating generic rewrites to upstream `rtk rewrite`, but
delegated output should not auto-execute in the first `updatedInput` release.

Requirements:

- Trust stdout, not exit status alone, because prior evidence shows
  `rtk rewrite` can print a valid rewrite with a non-zero exit code.
- Accept only non-empty rewrites that differ from the original and start with
  `rtk `.
- Keep local guards that block unsafe delegated POSIX rewrites for unsupported
  `cat`, `head`, `tail`, `grep`, `find`, and `rg` shapes.
- Treat acceptable delegated rewrites as `DenySuggestion` until there is a small,
  reviewed allowlist or live canary evidence for auto-execution.
- If `rtk rewrite` is unavailable or returns no acceptable rewrite, fail open
  with no output.

## Install And Trust

`hooks.json` config shape does not need to change for `updatedInput`. The hook is
still a command handler under a `PreToolUse` matcher group.

Docs should keep current install guidance:

- Read official Codex hooks docs before editing config.
- Use `hooks.PreToolUse[].hooks[].command`, not a flattened command entry.
- Use stable absolute installed binary paths.
- Preserve existing hooks and unknown fields.
- Leave hook trust as a user action through `/hooks`; do not edit trust metadata.

Docs must remove the stale warning that says `updatedInput` is unsupported.

Status message should change from suggestion language to rewrite language, for
example:

```json
"statusMessage": "Checking RTK command rewrite"
```

Existing trusted hook hashes may need user review again when the binary changes,
even if `hooks.json` command identity stays the same. Release notes must call out
that behavior changes from visible suggestions to automatic command rewrites for
the covered subset.

## Testing Requirements

Update `tests/pretool.rs` helpers:

- Replace covered auto-execution cases with
  `assert_rewrite(command, rewritten_command)`.
- `assert_rewrite` should parse stdout JSON and assert:
  - `hookSpecificOutput.hookEventName == "PreToolUse"`
  - `hookSpecificOutput.permissionDecision == "allow"`
  - `hookSpecificOutput.updatedInput.command == rewritten_command`
  - no `permissionDecisionReason` for rewrite cases
- Keep `assert_no_output` for fail-open cases.
- Keep a separate `assert_deny` for invalid `rtk read` flag cases and for any
  delegated or wrapper-heavy suggestion that is not yet auto-execution-proved.

Coverage must include at least one representative case in each lane:

- Windows direct read: `Get-Content`/`gc` -> `rtk read`.
- Windows search: `Select-String` and `findstr /N` -> `rtk grep -n`.
- Windows discovery: `Get-ChildItem -Recurse -File` -> `rtk find`.
- Unix read: `cat`, `head`, `tail`, or `sed -n '1,Np'` -> `rtk read`.
- Unix search/discovery: `grep -n` and `find -type f`.
- Raw `rg` search as auto-rewrite, with plain `rg --files` kept as deny
  guidance because `rtk find --max 50` is intentionally bounded.
- DenySuggestion cases: delegated rewrite such as `git status --short`, wrapper
  rewrite such as `$env:PATH=...; busted spec` or
  `bash -lc 'PATH=... busted spec'`, and invalid `rtk read --line`.
- No-output safety cases for mutation, redirect, pipe, ambiguous quoting, complex
  `rg --files`, complex `find`, and unsupported `grep`.

Recommended exact verification before merge:

```powershell
rtk cargo fmt --check
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk cargo test
rtk cargo build --release
```

Manual probes after build:

```powershell
target\release\rtk-codex-hook.exe --explain "git status --short"
```

Pipe Codex-style `PreToolUse` JSON payloads into the built binary and confirm:

- AutoRewrite cases contain `permissionDecision: "allow"` and
  `updatedInput.command`.
- DenySuggestion cases contain `permissionDecision: "deny"` and no
  `updatedInput`.
- Noop cases print no output.

Run wrapper execution probes before promoting wrappers to `AutoRewrite`:

```powershell
pwsh -NoProfile -Command '$env:PATH="$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted --help'
```

```bash
bash -lc 'PATH="$HOME/.luarocks/bin:$PATH" rtk busted --help'
```

## Documentation Updates

Update:

- `README.md`: describe rewriting instead of denying. Keep human install prompt
  short.
- `CODEX_INSTALL.md`: remove stale `updatedInput` warning, update statusMessage,
  and mention no config shape change is needed for command rewriting.
- `docs/DEVELOPMENT.md`: update architecture, hook contract, hook mode, and
  examples from deny-only to hybrid rewrite-first behavior with explicit deny
  and fail-open lanes.

Do not update `RTK.md` unless implementation changes user-facing RTK command
guidance. Current `RTK.md` is command-usage guidance, not hook-output contract.

## Rollout Plan

1. Implement and test in a feature worktree.
2. Commit code, tests, and docs together because behavior and guidance must stay
   in sync.
3. Run a live Codex hook canary for one auto-rewrite and one deny-suggestion
   case before broad rollout.
4. Merge through normal repo flow.
5. Build release binary.
6. Deploy the built binary to the active local hook path when requested:
   `C:\Users\lk4d4\.local\bin\rtk-codex-hook.exe`.
7. For WSL deployment requests, also deploy to
   `/home/lk4d4/.codex/hooks/rtk-codex-hook` and verify WSL-local `RTK.md` only
   if guidance changed.
8. Probe installed binary with `--version`, `--explain`, and a live
   Codex-style JSON payload.
9. Ask user to review/trust hook through `/hooks` if Codex marks the changed
   binary for review.

## Possible Problems

- Codex compatibility drift: docs say full wire schemas live in generated Codex
  schemas. If implementation hits an unexpected runtime rejection, verify
  against the generated schema or a live Codex hook probe.
- Silent semantic change: `updatedInput` runs the rewrite automatically, so any
  bad rewrite becomes more costly than old deny/suggest behavior. Mitigation:
  only auto-rewrite equivalent commands; keep non-equivalent guidance as deny or
  no output.
- Incomplete interception: Codex may not run `PreToolUse` for every shell path,
  especially newer unified exec paths. Mitigation: docs must avoid promising
  universal enforcement.
- Multiple matching hooks run concurrently. Another hook may also return a
  decision. Mitigation: keep this hook narrow, deterministic, and quiet when it
  has no rewrite. Supported deployments should assume one rewrite-capable Bash
  `PreToolUse` hook until mixed-hook precedence is tested.
- Trust review: binary changes may require re-trusting the hook. Mitigation:
  keep install docs explicit and leave trust to user.
- Active deployment lag: source changes do not affect this machine until the
  installed binary is overwritten. Mitigation: deployment checklist includes
  installed binary probe.

## Open Decisions

- Whether to keep the invalid `rtk read` flag case as deny guidance. This spec
  recommends yes because it is not an equivalent command rewrite.
- Whether to add a CLI flag that prints full hook JSON for debugging. This spec
  recommends no for first pass; `--explain` plus integration tests are enough.
- Whether to auto-execute delegated `rtk rewrite` output in a later release. This
  spec recommends keeping it as deny guidance until separately proved.
- What minimum Codex client build should be documented for `updatedInput`.
- Whether to inspect generated Codex schemas before coding. This spec recommends
  doing so if live/docs behavior disagrees during implementation or if minimum
  client support needs exact schema evidence.
