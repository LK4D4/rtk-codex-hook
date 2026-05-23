# UpdatedInput Hook Rewrites Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Codex `PreToolUse` `updatedInput.command` output for safe RTK rewrites while preserving deny/noop behavior for risky or non-equivalent cases.

**Architecture:** Keep command parsing and rewrite construction in `src/rewrite.rs`, but classify suggestions into `AutoRewrite`, `DenySuggestion`, or `Noop` before serializing hook output. `src/hook.rs` owns Codex JSON envelopes: `allow + updatedInput.command` for automatic rewrites, `deny + permissionDecisionReason` for visible guidance, and no output for fail-open cases.

**Tech Stack:** Rust 2024, `serde`, `serde_json`, integration tests in `tests/pretool.rs`, docs in Markdown.

---

## File Structure

- Modify `src/rewrite.rs`: add `HookAction`, expose `action(command)`, keep existing suggestion functions as helpers, classify invalid `rtk read`, local deterministic rewrites, delegated rewrites, wrapper-heavy rewrites, and noops.
- Modify `src/hook.rs`: call `rewrite::action`, serialize allow JSON and deny JSON.
- Modify `tests/pretool.rs`: add `assert_rewrite`, keep `assert_deny`, update representative cases per lane.
- Modify `README.md`: describe rewrite-first behavior.
- Modify `CODEX_INSTALL.md`: remove stale `updatedInput` warning and update status message.
- Modify `docs/DEVELOPMENT.md`: document hybrid contract, testing, and rollout.
- Optional no change `src/main.rs`: `--explain` can keep using `rewrite::suggest`.

### Task 1: Add Rewrite/Deny Classification

**Files:**
- Modify: `src/rewrite.rs`
- Test: `tests/pretool.rs`

- [ ] **Step 1: Write failing tests for allow, deny, and noop lanes**

In `tests/pretool.rs`, add helper:

```rust
fn assert_rewrite(command: &str, rewritten: &str) {
    let output = run_command(command);
    let value: serde_json::Value =
        serde_json::from_str(&output).unwrap_or_else(|err| panic!("invalid json {err}: {output}"));
    let hook = &value["hookSpecificOutput"];

    assert_eq!("PreToolUse", hook["hookEventName"].as_str().expect("hookEventName"));
    assert_eq!("allow", hook["permissionDecision"].as_str().expect("permissionDecision"));
    assert_eq!(
        rewritten,
        hook["updatedInput"]["command"].as_str().expect("updatedInput.command")
    );
    assert!(
        hook.get("permissionDecisionReason").is_none(),
        "rewrite output should not include permissionDecisionReason: {output}"
    );
}
```

Update one deterministic local case to fail under old code:

```rust
assert_rewrite("gc README.md", "rtk read README.md");
```

Keep invalid RTK read as deny:

```rust
assert_deny("rtk read src/ui.lua --line 1-120", "rtk read --help");
```

Keep delegated rewrite as deny in first rollout:

```rust
assert_deny("git status --short", "rtk git status --short");
```

- [ ] **Step 2: Run focused test and verify RED**

Run:

```powershell
rtk cargo test --test pretool get_content_redirects_to_rtk_read
```

Expected: FAIL because old hook output has `permissionDecision: "deny"` and no `updatedInput.command`.

- [ ] **Step 3: Add `HookAction` and classifier**

In `src/rewrite.rs`, add public enum near `suggest`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookAction {
    AutoRewrite(String),
    DenySuggestion(String),
}
```

Add:

```rust
pub fn action(command: &str) -> Option<HookAction> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }

    if let Some(help) = invalid_rtk_read_redirect(command) {
        return Some(HookAction::DenySuggestion(help));
    }

    if starts_with_rtk(command) && preferred_rtk_command(command)
        || is_preferred_pwsh_wrapper(command)
        || is_preferred_bash_wrapper(command)
    {
        return None;
    }

    direct_powershell_redirect(command)
        .or_else(|| powershell_redirect(command))
        .or_else(|| posix_redirect(command))
        .or_else(|| rg_redirect(command))
        .map(HookAction::AutoRewrite)
        .or_else(|| {
            env_redirect(command)
                .or_else(|| bash_redirect(command))
                .or_else(|| safe_external_rtk_rewrite(command))
                .or_else(|| local_rtk_miss_fallback(command))
                .map(HookAction::DenySuggestion)
        })
}
```

Then update `suggest` to preserve CLI behavior:

```rust
pub fn suggest(command: &str) -> Option<String> {
    match action(command)? {
        HookAction::AutoRewrite(command) | HookAction::DenySuggestion(command) => Some(command),
    }
}
```

- [ ] **Step 4: Run focused test and verify still fails on hook serializer**

Run:

```powershell
rtk cargo test --test pretool get_content_redirects_to_rtk_read
```

Expected: still FAIL until `src/hook.rs` serializes `AutoRewrite` as allow.

- [ ] **Step 5: Commit classifier after serializer task passes**

Commit with Task 2 because classifier and hook serializer are one behavior slice:

```powershell
rtk git add src\rewrite.rs tests\pretool.rs
rtk git commit -m "feat: classify hook rewrite actions"
```

### Task 2: Serialize `allow + updatedInput.command`

**Files:**
- Modify: `src/hook.rs`
- Test: `tests/pretool.rs`

- [ ] **Step 1: Update hook serializer test expectations**

Update all deterministic local rewrite cases in `tests/pretool.rs` from `assert_deny` to `assert_rewrite`, including:

```rust
assert_rewrite("gc README.md", "rtk read README.md");
assert_rewrite(
    r#"Select-String -Path 'src\main.rs' -Pattern 'foo' -Context 2"#,
    r#"rtk grep -n "foo" src\main.rs -- -C 2"#,
);
assert_rewrite("Get-ChildItem -Path src -Recurse -File", "rtk find src");
assert_rewrite("cat README.md", "rtk read README.md");
assert_rewrite("grep -n tokenize src/rewrite.rs", r#"rtk grep -n "tokenize" src/rewrite.rs"#);
```

Keep these as `assert_deny`:

```rust
assert_deny("git status --short", "rtk git status --short");
assert_deny("ls src", "rtk ls src");
assert_deny("rg --files", r#"rtk find "*" . --max 50 --file-type f"#);
assert_deny(
    r#"powershell -NoProfile -Command $env:PATH = "$env:APPDATA\luarocks\bin;$env:PATH"; busted spec"#,
    r#"pwsh -NoProfile -Command '$env:PATH = "$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted spec'"#,
);
assert_deny(
    r#"bash -lc 'PATH="$HOME/.luarocks/bin:$PATH" busted spec'"#,
    r#"bash -lc 'PATH="$HOME/.luarocks/bin:$PATH" rtk busted spec'"#,
);
```

- [ ] **Step 2: Run integration tests and verify RED**

Run:

```powershell
rtk cargo test --test pretool
```

Expected: FAIL because `src/hook.rs` still emits deny JSON for all suggestions.

- [ ] **Step 3: Add allow serializer**

In `src/hook.rs`, replace `handle_stdin` final section with:

```rust
let action = crate::rewrite::action(command)?;
match action {
    crate::rewrite::HookAction::AutoRewrite(rewrite) => {
        log(&format!("rewrite original=[{command}] updated=[{rewrite}]"));
        Some(allow_rewrite_json(&rewrite))
    }
    crate::rewrite::HookAction::DenySuggestion(suggestion) => {
        log(&format!("deny original=[{command}] suggestion=[{suggestion}]"));
        Some(deny_json(&suggestion))
    }
}
```

Add:

```rust
fn allow_rewrite_json(command: &str) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "updatedInput": {
                "command": command,
            },
        }
    })
    .to_string()
}
```

- [ ] **Step 4: Run integration tests and verify GREEN**

Run:

```powershell
rtk cargo test --test pretool
```

Expected: PASS.

- [ ] **Step 5: Run full tests**

Run:

```powershell
rtk cargo test
```

Expected: PASS.

- [ ] **Step 6: Commit behavior**

```powershell
rtk git add src\hook.rs src\rewrite.rs tests\pretool.rs
rtk git commit -m "feat: use updatedInput for safe rewrites"
```

### Task 3: Update Documentation

**Files:**
- Modify: `README.md`
- Modify: `CODEX_INSTALL.md`
- Modify: `docs/DEVELOPMENT.md`

- [ ] **Step 1: Update README wording**

Replace deny-only wording with:

```markdown
Cross-platform Codex `PreToolUse` hook for rewriting noisy commands into lower-token RTK commands.

The binary reads Codex hook JSON from stdin. For safe equivalent rewrites, it lets
Codex run the RTK-shaped command through `updatedInput.command`. For unsupported,
ambiguous, malformed, or mutating commands it exits `0` with no output so the
hook fails open. Non-equivalent guidance may still deny with a visible reason.
```

- [ ] **Step 2: Update install docs**

In `CODEX_INSTALL.md`:

- Change status message to `"Checking RTK command rewrite"`.
- Remove lines that say not to use `updatedInput`.
- Add:

```markdown
The hook binary owns runtime `updatedInput` responses; `hooks.json` config shape
does not change for command rewrites.
```

- [ ] **Step 3: Update development docs**

In `docs/DEVELOPMENT.md`, update Hook Contract to include:

```markdown
For safe equivalent rewrites, output:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":{"command":"<rewrite>"}}}
```

For non-equivalent guidance that should stay visible, output deny JSON:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Use RTK-optimized command instead: <suggestion>"}}
```
```

Also document that external `rtk rewrite` and wrapper-heavy suggestions remain deny guidance until execution-proved.

- [ ] **Step 4: Run docs scan**

Run:

```powershell
rtk grep -n "deny a command|Do not use `updatedInput`|command suggestions|deny JSON only" README.md CODEX_INSTALL.md docs\DEVELOPMENT.md
```

Expected: no stale deny-only or unsupported-`updatedInput` guidance remains.

- [ ] **Step 5: Commit docs**

```powershell
rtk git add README.md CODEX_INSTALL.md docs\DEVELOPMENT.md
rtk git commit -m "docs: document updatedInput rewrite behavior"
```

### Task 4: Final Verification And Manual Probes

**Files:**
- No new files
- Verify entire repo

- [ ] **Step 1: Format check**

Run:

```powershell
rtk cargo fmt --check
```

Expected: PASS.

- [ ] **Step 2: Clippy**

Run:

```powershell
rtk cargo clippy --all-targets --all-features -- -D warnings
```

Expected: PASS.

- [ ] **Step 3: Full tests**

Run:

```powershell
rtk cargo test
```

Expected: PASS.

- [ ] **Step 4: Release build**

Run:

```powershell
rtk cargo build --release
```

Expected: PASS.

- [ ] **Step 5: Explain probe remains stable**

Run:

```powershell
target\release\rtk-codex-hook.exe --explain "gc README.md"
```

Expected:

```text
rtk read README.md
```

- [ ] **Step 6: Hook JSON allow probe**

Feed this payload to the release binary:

```json
{"hook_event_name":"PreToolUse","tool_input":{"command":"gc README.md"}}
```

Expected compact JSON contains:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":{"command":"rtk read README.md"}}}
```

- [ ] **Step 7: Hook JSON deny probe**

Feed this payload to the release binary:

```json
{"hook_event_name":"PreToolUse","tool_input":{"command":"git status --short"}}
```

Expected compact JSON contains:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Use RTK-optimized command instead: rtk git status --short"}}
```

- [ ] **Step 8: Worktree status**

Run:

```powershell
rtk git status --short --branch
```

Expected: clean branch after commits.
