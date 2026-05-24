# Auto-Rewrite Safe Classifier Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auto-allow simple RTK wrapper rewrites from external `rtk rewrite` and local fallback paths while keeping uncertain suggestions as visible deny guidance.

**Architecture:** Keep existing rewrite order in `src/rewrite.rs`; only classify suggestions produced by `safe_external_rtk_rewrite()` and `local_rtk_miss_fallback()`. A new helper proves command shape, safe wrapper tool membership, and argument preservation before returning `HookAction::AutoRewrite`; otherwise it returns `HookAction::DenySuggestion`. `tests/pretool.rs` remains the Codex hook behavior contract.

**Tech Stack:** Rust 2024, `serde_json`, integration tests in `tests/pretool.rs`, Markdown docs.

---

## File Structure

- Modify `src/rewrite.rs`: add safe-wrapper classifier helpers near `local_rtk_miss_fallback()`, share the safe tool list with fallback behavior, and route external/fallback suggestions through the classifier.
- Modify `tests/pretool.rs`: change simple wrapper expectations from deny to allow, and add negative coverage proving risky external suggestions stay deny or no output.
- Modify `docs/DEVELOPMENT.md`: update rewrite model docs so they describe the classifier instead of stale "external/fallback stay deny" behavior.
- No change `src/hook.rs`: it already serializes `HookAction::AutoRewrite` as compact `allow` plus `updatedInput.command` and `HookAction::DenySuggestion` as compact `deny` guidance.
- No change `src/main.rs`: `--explain` still prints the suggestion string for both action variants.

### Task 1: Pin Desired Hook Behavior In Tests

**Files:**
- Modify: `tests/pretool.rs`
- Test: `tests/pretool.rs`

- [ ] **Step 1: Replace generic fallback test**

In `tests/pretool.rs`, replace the full `generic_rtk_rewrite_fallbacks_apply_to_common_tools` test with:

```rust
#[test]
fn generic_rtk_rewrite_fallbacks_apply_to_common_tools() {
    assert_rewrite("git status --short", "rtk git status --short");
    assert_rewrite("ls src", "rtk ls src");
    assert_rewrite("cargo test", "rtk cargo test");
    assert_rewrite("npm test", "rtk npm test");
    assert_rewrite("pytest -q", "rtk pytest -q");
    assert_rewrite("docker ps", "rtk docker ps");
    assert_rewrite("curl --version", "rtk curl --version");

    assert_no_output("git diff -- src/rewrite.rs tests/pretool.rs");
    assert_no_output("gh pr view --json title");
}
```

- [ ] **Step 2: Update PowerShell alias `ls` expectation**

In `tests/pretool.rs`, inside `get_child_item_redirects_to_rtk_find`, replace:

```rust
assert_deny("ls src", "rtk ls src");
```

with:

```rust
assert_rewrite("ls src", "rtk ls src");
```

- [ ] **Step 3: Update noisy tool allowlist expectations**

In `tests/pretool.rs`, inside `noisy_tool_allowlist_redirects_to_rtk`, replace this loop body:

```rust
assert_deny(
    &format!("{tool} --version"),
    &format!("rtk {tool} --version"),
);
```

with:

```rust
assert_rewrite(
    &format!("{tool} --version"),
    &format!("rtk {tool} --version"),
);
```

Then replace:

```rust
assert_deny("python -m pytest tests", "rtk pytest tests");
assert_deny("uv run pytest tests", "rtk pytest tests");
assert_no_output("python -m pip install pytest");
assert_deny("uv pip install pytest", "rtk uv pip install pytest");
```

with:

```rust
assert_rewrite("python -m pytest tests", "rtk pytest tests");
assert_rewrite("uv run pytest tests", "rtk pytest tests");
assert_no_output("python -m pip install pytest");
assert_deny("uv pip install pytest", "rtk uv pip install pytest");
```

- [ ] **Step 4: Add classifier negative test**

In `tests/pretool.rs`, add this test immediately after `generic_rtk_rewrite_fallbacks_apply_to_common_tools`:

```rust
#[test]
fn unsafe_external_rewrite_suggestions_stay_visible_or_fail_open() {
    assert_no_output("cat README.md > copy.md");
    assert_no_output("cat README.md | tee copy.md");
    assert_no_output("grep -r tokenize src");
    assert_no_output("grep -v tokenize src/rewrite.rs");
    assert_no_output("find src -delete");
    assert_deny("uv pip install pytest", "rtk uv pip install pytest");
    assert_no_output("unknown-tool --version");
    assert_no_output("rtk git status --short");
}
```

- [ ] **Step 5: Run focused tests and verify RED**

Run:

```bash
rtk cargo test --test pretool generic_rtk_rewrite_fallbacks_apply_to_common_tools
rtk cargo test --test pretool noisy_tool_allowlist_redirects_to_rtk
```

Expected: both commands FAIL because current code still serializes these fallback/external suggestions as `permissionDecision: "deny"`.

- [ ] **Step 6: Commit test-only changes**

Run:

```bash
rtk git status --short
rtk git add tests/pretool.rs
rtk git commit -m "test: pin safe auto rewrite classifier behavior"
```

Expected: commit succeeds with only `tests/pretool.rs` staged.

### Task 2: Add Classifier Helpers

**Files:**
- Modify: `src/rewrite.rs`
- Test: `tests/pretool.rs`

- [ ] **Step 1: Add shared safe wrapper helpers**

In `src/rewrite.rs`, add this code immediately before `local_rtk_miss_fallback`:

```rust
fn is_safe_wrapper_tool(name: &str) -> bool {
    matches!(
        name,
        "git"
            | "cargo"
            | "npm"
            | "pytest"
            | "busted"
            | "luacheck"
            | "dotnet"
            | "pnpm"
            | "pip"
            | "go"
            | "docker"
            | "npx"
            | "vitest"
            | "jest"
            | "tsc"
            | "ruff"
            | "mypy"
            | "playwright"
            | "gradlew"
            | "curl"
            | "ls"
    )
}

fn has_unquoted_shell_control(command: &str) -> bool {
    let mut quote = None;
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(ch),
            (None, '|' | '<' | '>' | ';') => return true,
            (None, '&') if chars.peek() == Some(&'&') => return true,
            _ => {}
        }
    }
    false
}

fn is_single_simple_command(command: &str) -> bool {
    !command.contains('\n') && !has_unquoted_shell_control(command)
}

fn token_texts(tokens: &[Token]) -> Vec<&str> {
    tokens.iter().map(|token| token.text.as_str()).collect()
}

fn same_command_tokens(original: &[Token], suggestion: &[Token]) -> bool {
    suggestion.len() == original.len() + 1
        && command_name(&suggestion[0].text) == "rtk"
        && command_name(&suggestion[1].text) == command_name(&original[0].text)
        && token_texts(&suggestion[2..]) == token_texts(&original[1..])
}

fn same_rtk_pytest_tokens(args: &[Token], suggestion: &[Token]) -> bool {
    suggestion.len() == args.len() + 2
        && command_name(&suggestion[0].text) == "rtk"
        && command_name(&suggestion[1].text) == "pytest"
        && token_texts(&suggestion[2..]) == token_texts(args)
}
```

- [ ] **Step 2: Add classifier helper**

In `src/rewrite.rs`, add this code immediately after `same_rtk_pytest_tokens`:

```rust
fn classify_external_rewrite(original: &str, suggestion: String) -> HookAction {
    if is_safe_external_rewrite(original, &suggestion) {
        HookAction::AutoRewrite(suggestion)
    } else {
        HookAction::DenySuggestion(suggestion)
    }
}

fn is_safe_external_rewrite(original: &str, suggestion: &str) -> bool {
    if is_git_diff_pathspec_command(original)
        || !suggestion.starts_with("rtk ")
        || !is_single_simple_command(original)
        || !is_single_simple_command(suggestion)
    {
        return false;
    }

    let original_tokens = tokenize(original);
    let suggestion_tokens = tokenize(suggestion);
    let Some(first) = original_tokens.first().map(|token| command_name(&token.text)) else {
        return false;
    };

    if is_safe_wrapper_tool(&first) && same_command_tokens(&original_tokens, &suggestion_tokens) {
        return true;
    }

    if first == "python"
        && python_pytest_args(&original_tokens).is_some()
        && same_rtk_pytest_tokens(&original_tokens[3..], &suggestion_tokens)
    {
        return true;
    }

    if first == "uv"
        && uv_pytest_args(&original_tokens).is_some()
        && same_rtk_pytest_tokens(&original_tokens[3..], &suggestion_tokens)
    {
        return true;
    }

    false
}
```

- [ ] **Step 3: Share safe list with local fallback**

In `src/rewrite.rs`, inside `local_rtk_miss_fallback`, replace this match arm:

```rust
Some(
    "git" | "cargo" | "npm" | "pytest" | "busted" | "luacheck" | "dotnet" | "pnpm" | "pip"
    | "go" | "docker" | "npx" | "vitest" | "jest" | "tsc" | "ruff" | "mypy" | "playwright"
    | "gradlew" | "curl",
) => Some(format!("rtk {command}")),
```

with:

```rust
Some(name) if is_safe_wrapper_tool(name) && name != "ls" => Some(format!("rtk {command}")),
```

`ls` is classifier-safe only when external `rtk rewrite` provides the suggestion. The local fallback must not invent `rtk ls ...`.

- [ ] **Step 4: Run formatter and focused RED check**

Run:

```bash
cargo fmt
rtk cargo fmt --check
rtk cargo test --test pretool generic_rtk_rewrite_fallbacks_apply_to_common_tools
```

Expected: `fmt --check` PASS. The focused test still FAILS because `action()` does not call `classify_external_rewrite()` yet.

- [ ] **Step 5: Commit helper code**

Run:

```bash
rtk git add src/rewrite.rs
rtk git commit -m "feat: add safe rewrite classifier helpers"
```

Expected: commit succeeds with only `src/rewrite.rs` staged.

### Task 3: Route Suggestions Through Classifier

**Files:**
- Modify: `src/rewrite.rs`
- Test: `tests/pretool.rs`

- [ ] **Step 1: Update `action()`**

In `src/rewrite.rs`, inside `action()`, replace:

```rust
.or_else(|| safe_external_rtk_rewrite(command).map(HookAction::DenySuggestion))
.or_else(|| local_rtk_miss_fallback(command).map(HookAction::DenySuggestion))
```

with:

```rust
.or_else(|| {
    safe_external_rtk_rewrite(command)
        .map(|suggestion| classify_external_rewrite(command, suggestion))
})
.or_else(|| {
    local_rtk_miss_fallback(command)
        .map(|suggestion| classify_external_rewrite(command, suggestion))
})
```

- [ ] **Step 2: Run focused GREEN checks**

Run:

```bash
rtk cargo test --test pretool generic_rtk_rewrite_fallbacks_apply_to_common_tools
rtk cargo test --test pretool noisy_tool_allowlist_redirects_to_rtk
rtk cargo test --test pretool unsafe_external_rewrite_suggestions_stay_visible_or_fail_open
```

Expected: all three commands PASS.

- [ ] **Step 3: Run full hook integration tests**

Run:

```bash
rtk cargo test --test pretool
```

Expected: PASS. Existing POSIX, PowerShell, `rg --files`, invalid RTK, and fail-open tests must keep their previous behavior.

- [ ] **Step 4: Commit routing change**

Run:

```bash
rtk git add src/rewrite.rs tests/pretool.rs
rtk git commit -m "feat: auto-allow safe rtk wrapper rewrites"
```

Expected: commit succeeds with `src/rewrite.rs` and `tests/pretool.rs`.

### Task 4: Sync Development Docs

**Files:**
- Modify: `docs/DEVELOPMENT.md`
- Test: `docs/DEVELOPMENT.md`

- [ ] **Step 1: Update rewrite action order**

In `docs/DEVELOPMENT.md`, replace:

```markdown
4. Keep external `rtk rewrite`, wrapper-heavy rewrites, and broad noisy-tool
   fallback suggestions as deny guidance until they are execution-proved.
```

with:

```markdown
4. Classify external `rtk rewrite` and broad noisy-tool fallback suggestions.
   Simple argument-preserving `rtk <tool> ...` wrappers for known safe wrapper
   tools become auto-rewrites; wrapper-heavy or unproved suggestions stay deny
   guidance.
```

- [ ] **Step 2: Update external rewrite docs**

In `docs/DEVELOPMENT.md`, replace:

```markdown
- Generic cross-platform tools are delegated to `rtk rewrite`, such as
  `git status --short` to `rtk git status --short` and `ls src` to `rtk ls src`.
  Delegated rewrites stay as deny guidance in the first `updatedInput` release.
  Raw `git diff -- path...` is left alone because the current RTK git wrapper
  does not preserve Git pathspec separator behavior for that form.
```

with:

```markdown
- Generic cross-platform tools are delegated to `rtk rewrite`, such as
  `git status --short` to `rtk git status --short` and `ls src` to `rtk ls src`.
  Simple single-command, argument-preserving wrappers for the safe wrapper tool
  set are auto-rewritten through `updatedInput.command`. Raw `git diff --
  path...` is left alone because the current RTK git wrapper does not preserve
  Git pathspec separator behavior for that form.
```

- [ ] **Step 3: Update fallback and pytest docs**

In `docs/DEVELOPMENT.md`, replace:

```markdown
- If `rtk rewrite` is unavailable or returns no suggestion, a small local
  fallback preserves legacy suggestions for common noisy tools such as `git`,
  `cargo`, `npm`, `pytest`, `busted`, `luacheck`, `dotnet`, `pnpm`, `pip`, `go`,
  `docker`, `npx`, `vitest`, `jest`, `tsc`, `ruff`, `mypy`, `playwright`,
  `gradlew`, and `curl`.
```

with:

```markdown
- If `rtk rewrite` is unavailable or returns no suggestion, a small local
  fallback preserves legacy suggestions for common noisy tools such as `git`,
  `cargo`, `npm`, `pytest`, `busted`, `luacheck`, `dotnet`, `pnpm`, `pip`, `go`,
  `docker`, `npx`, `vitest`, `jest`, `tsc`, `ruff`, `mypy`, `playwright`,
  `gradlew`, and `curl`. Fallback suggestions pass through the same safe
  wrapper classifier as external `rtk rewrite` suggestions.
```

Then replace:

```markdown
- `python -m pytest ...` and `uv run pytest ...` become direct `rtk pytest ...`
  suggestions.
```

with:

```markdown
- `python -m pytest ...` and `uv run pytest ...` become direct `rtk pytest ...`
  auto-rewrites when the classifier proves the pytest arguments are preserved.
```

- [ ] **Step 4: Run docs checks**

Run:

```bash
rtk git diff --check
rtk grep -n "Delegated rewrites stay as deny guidance|broad noisy-tool fallback suggestions as deny guidance" docs/DEVELOPMENT.md
```

Expected: `git diff --check` exits 0. `rtk grep` reports `0 matches`.

- [ ] **Step 5: Commit docs change**

Run:

```bash
rtk git add docs/DEVELOPMENT.md
rtk git commit -m "docs: describe safe rewrite classifier"
```

Expected: commit succeeds with only `docs/DEVELOPMENT.md` staged.

### Task 5: Final Verification

**Files:**
- Verify: `src/rewrite.rs`
- Verify: `tests/pretool.rs`
- Verify: `docs/DEVELOPMENT.md`

- [ ] **Step 1: Run required code gates**

Run:

```bash
rtk cargo fmt --check
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk cargo test
rtk cargo build --release
```

Expected: all commands PASS.

- [ ] **Step 2: Run manual explain probes**

Run:

```bash
cargo run -- --explain "git status --short"
cargo run -- --explain "cargo test"
cargo run -- --explain "python -m pytest tests"
cargo run -- --explain "git diff -- src/rewrite.rs"
```

Expected:

```text
rtk git status --short
rtk cargo test
rtk pytest tests
```

The fourth command prints no output. `ls src` and unsafe external suggestions are covered by the fake `rtk rewrite` integration tests so this manual probe does not depend on the installed RTK version.

- [ ] **Step 3: Verify hook JSON lanes**

Run:

```bash
rtk cargo test --test pretool generic_rtk_rewrite_fallbacks_apply_to_common_tools
rtk cargo test --test pretool noisy_tool_allowlist_redirects_to_rtk
rtk cargo test --test pretool unsafe_external_rewrite_suggestions_stay_visible_or_fail_open
```

Expected: all three commands PASS. The first two prove `allow` plus `updatedInput.command`; the third proves unsafe external suggestions stay deny or no output.

- [ ] **Step 4: Verify final git scope**

Run:

```bash
rtk git status --short
rtk git log --oneline -4
```

Expected: working tree clean. Latest commits are test pin, classifier helper, classifier routing, and docs sync commits from this plan.

## Self-Review

- Spec coverage: Task 1 covers positive commands (`git status --short`, `ls src`, `cargo test`, `npm test`, `pytest -q`, `docker ps`, `curl --version`) and negative commands (`git diff -- path...`, pipelines, redirects, mutating find, unknown command, already-good RTK command). Tasks 2-3 implement classifier logic for external and fallback suggestions. Task 4 syncs behavior docs.
- Placeholder scan: no unfinished markers or deferred code remain; every code-changing step includes concrete code.
- Type consistency: helper names match across tasks: `classify_external_rewrite`, `is_safe_external_rewrite`, `is_safe_wrapper_tool`, `is_single_simple_command`, `same_command_tokens`, and `same_rtk_pytest_tokens`.
