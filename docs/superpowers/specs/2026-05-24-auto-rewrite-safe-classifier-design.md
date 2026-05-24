# Auto-Rewrite Safe Classifier Design

## Goal

Reduce unnecessary Codex hook denials for simple commands that RTK can rewrite safely.
The hook should still fail open for unsupported input and should keep visible deny guidance
for rewrites where the hook cannot prove the suggestion preserves command intent.

## Current Behavior

`rewrite::action()` handles local rewrite patterns first. It then delegates generic tools
to `rtk rewrite` and falls back to a local `rtk {command}` suggestion for common tools.
Those delegated and fallback suggestions currently become `DenySuggestion`, so commands
like `git status --short` and `ls src` interrupt the user even though the intended RTK
form is straightforward.

## Proposed Behavior

Keep existing rewrite order, but classify external or fallback suggestions before choosing
`AutoRewrite` or `DenySuggestion`.

`AutoRewrite` applies when all of these are true:

- The original command is a single simple command.
- The original command has no shell control, pipeline, or redirection token such as `|`,
  `>`, `<`, `;`, `&&`, or `||`.
- The original first command is in a shared safe-wrapper tool set. This starts with the
  existing broad fallback tool list and adds explicit delegated tools that `rtk rewrite`
  already handles as simple wrappers, such as `ls`.
- The suggestion starts with `rtk `.
- The suggestion is an argument-preserving wrapper, such as `rtk {original}`, or one of the
  existing explicit equivalent forms like `python -m pytest ...` to `rtk pytest ...` and
  `uv run pytest ...` to `rtk pytest ...`.
- Existing special exclusions still win, including raw `git diff -- path...`.

`DenySuggestion` applies when `rtk rewrite` or fallback produces a suggestion but the
classifier cannot prove it is safe for automatic execution.

No output applies when no rewrite exists, the command is unsupported, or the input is
malformed. Mutating PowerShell behavior remains unchanged.

## Code Shape

Add a small classifier helper near the external rewrite path, for example:

```rust
fn classify_external_rewrite(original: &str, suggestion: String) -> HookAction
```

The helper should reuse existing tokenization helpers where possible. The safe-wrapper tool
set should be shared with `local_rtk_miss_fallback()` where the behavior overlaps, while
allowing `rtk rewrite`-only tools such as `ls` to be listed explicitly.

Update `action()` so `safe_external_rtk_rewrite()` and `local_rtk_miss_fallback()` feed
their suggestions through this classifier instead of always mapping to `DenySuggestion`.
Leave existing local exact rewrites and lossy cases untouched.

## Testing

Update `tests/pretool.rs` so simple commands become allow rewrites:

- `git status --short` -> `rtk git status --short`
- `ls src` -> `rtk ls src`
- `cargo test` -> `rtk cargo test`
- `npm test` -> `rtk npm test`
- `pytest -q` -> `rtk pytest -q`
- `docker ps` -> `rtk docker ps`
- `curl --version` -> `rtk curl --version`

Keep or add negative tests:

- `git diff -- path...` stays no output.
- Commands with pipelines or redirects stay deny guidance or no output according to existing
  handlers.
- Unknown commands stay no output.
- Already preferred RTK commands stay no output.

## Risks

The main risk is auto-running a suggestion that changes command semantics. The classifier
keeps the first implementation conservative by requiring a simple command shape and an
argument-preserving RTK wrapper. Anything outside that shape still uses visible guidance.
