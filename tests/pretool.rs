use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

fn hook_payload(command: &str) -> String {
    serde_json::json!({
        "session_id": "test-session",
        "transcript_path": null,
        "cwd": "C:\\work",
        "hook_event_name": "PreToolUse",
        "turn_id": "test-turn",
        "tool_name": "Bash",
        "tool_use_id": "test-tool",
        "tool_input": {
            "command": command
        }
    })
    .to_string()
}

fn fake_rtk_path() -> &'static PathBuf {
    static FAKE_RTK: OnceLock<PathBuf> = OnceLock::new();
    FAKE_RTK.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("rtk-codex-hook-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("fake rtk dir");
        let path = dir.join(if cfg!(windows) { "rtk.cmd" } else { "rtk" });

        if cfg!(windows) {
            std::fs::write(
                &path,
                r#"@echo off
if not "%~1"=="rewrite" exit /b 1
if "%~2"=="ls src" (
  <nul set /p=rtk ls src
  exit /b 3
)
if "%~2"=="uv pip install pytest" (
  <nul set /p=rtk uv pip install pytest
  exit /b 3
)
if "%~2"=="cat README.md > copy.md" (
  <nul set /p=rtk read README.md ^> copy.md
  exit /b 3
)
if "%~2"=="cat README.md | tee copy.md" (
  <nul set /p=rtk read README.md ^| tee copy.md
  exit /b 3
)
if "%~2"=="grep -r tokenize src" (
  <nul set /p=rtk grep -r tokenize src
  exit /b 3
)
if "%~2"=="grep -v tokenize src\rewrite.rs" (
  <nul set /p=rtk grep -v tokenize src\rewrite.rs
  exit /b 3
)
if "%~2"=="find src -delete" (
  <nul set /p=rtk find src -delete
  exit /b 3
)
if "%~2"=="gh pr view --json title" exit /b 1
exit /b 1
"#,
            )
            .expect("write fake rtk");
        } else {
            std::fs::write(
                &path,
                r#"#!/bin/sh
if [ "$1" != "rewrite" ]; then exit 1; fi
case "$2" in
  "ls src")
    printf '%s' 'rtk ls src'
    exit 3
    ;;
  "uv pip install pytest")
    printf '%s' 'rtk uv pip install pytest'
    exit 3
    ;;
  "cat README.md > copy.md")
    printf '%s' 'rtk read README.md > copy.md'
    exit 3
    ;;
  "cat README.md | tee copy.md")
    printf '%s' 'rtk read README.md | tee copy.md'
    exit 3
    ;;
  "grep -r tokenize src")
    printf '%s' 'rtk grep -r tokenize src'
    exit 3
    ;;
  "grep -v tokenize src/rewrite.rs")
    printf '%s' 'rtk grep -v tokenize src/rewrite.rs'
    exit 3
    ;;
  "find src -delete")
    printf '%s' 'rtk find src -delete'
    exit 3
    ;;
  "gh pr view --json title")
    exit 1
    ;;
esac
exit 1
"#,
            )
            .expect("write fake rtk");

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut permissions = std::fs::metadata(&path)
                    .expect("fake rtk metadata")
                    .permissions();
                permissions.set_mode(0o755);
                std::fs::set_permissions(&path, permissions).expect("chmod fake rtk");
            }
        }

        path
    })
}

fn run_hook(payload: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rtk-codex-hook"))
        .env("RTK_CODEX_HOOK_RTK_BIN", fake_rtk_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hook");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(payload.as_bytes())
        .expect("write stdin");

    let output = child.wait_with_output().expect("hook output");
    assert!(
        output.status.success(),
        "hook should always exit 0, got {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "hook should keep stderr quiet, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout utf8")
}

fn run_command(command: &str) -> String {
    run_hook(&hook_payload(command))
}

fn assert_no_output(command: &str) {
    let output = run_command(command);
    assert_eq!("", output.trim(), "expected no output for {command}");
}

fn assert_deny(command: &str, suggestion: &str) {
    let output = run_command(command);
    let value: serde_json::Value =
        serde_json::from_str(&output).unwrap_or_else(|err| panic!("invalid json {err}: {output}"));
    let reason = value["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .expect("permissionDecisionReason");

    assert_eq!(
        "PreToolUse",
        value["hookSpecificOutput"]["hookEventName"]
            .as_str()
            .expect("hookEventName")
    );
    assert_eq!(
        "deny",
        value["hookSpecificOutput"]["permissionDecision"]
            .as_str()
            .expect("permissionDecision")
    );
    assert_eq!(
        format!("Use RTK-optimized command instead: {suggestion}"),
        reason
    );
}

fn assert_rewrite(command: &str, rewritten: &str) {
    let output = run_command(command);
    let value: serde_json::Value =
        serde_json::from_str(&output).unwrap_or_else(|err| panic!("invalid json {err}: {output}"));
    let hook = &value["hookSpecificOutput"];

    assert_eq!(
        "PreToolUse",
        hook["hookEventName"].as_str().expect("hookEventName")
    );
    assert_eq!(
        "allow",
        hook["permissionDecision"]
            .as_str()
            .expect("permissionDecision")
    );
    assert_eq!(
        rewritten,
        hook["updatedInput"]["command"]
            .as_str()
            .expect("updatedInput.command")
    );
    assert!(
        hook.get("permissionDecisionReason").is_none(),
        "allow rewrite should not include permissionDecisionReason: {output}"
    );
}

#[test]
fn fails_open_for_non_pretooluse_and_bad_payloads() {
    let non_pretool = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_input": { "command": "git status --short" }
    })
    .to_string();
    assert_eq!("", run_hook(&non_pretool).trim());
    assert_eq!("", run_hook("{not json").trim());
    assert_eq!("", run_hook("").trim());
}

#[test]
fn already_good_commands_are_noops() {
    assert_no_output("rtk git status --short");
    assert_no_output(r#"rtk grep -n "foo" src"#);
    assert_no_output("rtk find src");
    assert_no_output("rtk read src/main.rs --max-lines 120");
    assert_no_output("rtk read -n src/main.rs --tail-lines 40");
    assert_no_output(
        r#"pwsh -NoProfile -Command '$env:PATH="$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted spec'"#,
    );
}

#[test]
fn invalid_rtk_read_flags_suggest_help() {
    for command in [
        "rtk read src/client/source.lua --line 1 --lines 650",
        "rtk read src/ui.lua --line 1-120",
        "rtk read src/ui.lua --range 130:310",
        "rtk read docs/notes.md:35-160",
        "rtk read src/ui.lua:130",
        "rtk read docs/notes.md --start-line 130 --max-lines 60",
        "rtk read docs/notes.md --start-line=130 --max-lines 60",
        "rtk read docs/notes.md --start 130 --max-lines 60",
        "rtk read docs/notes.md --from 130 --to 190",
        "rtk read docs/notes.md --line-number --max-lines 80",
    ] {
        assert_deny(command, "rtk read --help");
    }
}

#[test]
fn powershell_mutations_are_noops() {
    for verb in [
        "Remove-Item",
        "Set-Content",
        "Add-Content",
        "New-Item",
        "Move-Item",
        "Copy-Item",
    ] {
        assert_no_output(&format!(
            "powershell -NoProfile -Command {verb} -LiteralPath tmp -Force"
        ));
    }

    assert_no_output("Get-Content -LiteralPath input.txt | Set-Content -LiteralPath output.txt");
    assert_no_output("Get-Content -LiteralPath input.txt; Remove-Item -LiteralPath input.txt");
    assert_no_output("Get-Content -LiteralPath input.txt | Out-File -LiteralPath output.txt");
    assert_no_output("Get-Content input.txt|Out-File output.txt");
    assert_no_output("Get-Content -LiteralPath input.txt > output.txt");
}

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

#[test]
fn invalid_rtk_grep_passthrough_flags_are_corrected() {
    assert_deny(
        r#"rtk grep -n -C 25 "refreshChapterMenu" suwayomi spec"#,
        r#"rtk grep -n "refreshChapterMenu" suwayomi spec -- -C 25"#,
    );
    assert_deny(
        r#"rtk grep -n --context 4 "foo|bar" src tests"#,
        r#"rtk grep -n "foo|bar" src tests -- --context 4"#,
    );
}

#[test]
fn get_content_redirects_to_rtk_read() {
    assert_rewrite(
        r#"Get-Content -TotalCount 14 'src\main.rs'"#,
        r#"rtk read src\main.rs --max-lines 14"#,
    );
    assert_rewrite("gc README.md", "rtk read README.md");
    assert_rewrite(
        "cat -Tail 20 README.md",
        "rtk read README.md --tail-lines 20",
    );
    assert_rewrite(
        "type -TotalCount 10 src\\rewrite.rs",
        "rtk read src\\rewrite.rs --max-lines 10",
    );
    assert_rewrite(
        r#"powershell -NoProfile -Command Get-Content -LiteralPath 'src\main.rs' -TotalCount 80"#,
        r#"rtk read src\main.rs --max-lines 80"#,
    );
    assert_rewrite(
        r#"powershell -NoProfile -Command Get-Content -LiteralPath 'src\main.rs' -TotalCount 140"#,
        r#"rtk read src\main.rs --max-lines 140"#,
    );
    assert_rewrite(
        r#"powershell -NoProfile -Command Get-Content -LiteralPath 'README.md' -TotalCount 80"#,
        "rtk read README.md --max-lines 80",
    );
    assert_rewrite(
        "Get-Content -Tail 30 README.md",
        "rtk read README.md --tail-lines 30",
    );
    assert_rewrite(
        r#"powershell -NoProfile -Command Get-Content -LiteralPath 'README.md' -Tail 30"#,
        "rtk read README.md --tail-lines 30",
    );
    assert_deny(
        r#"powershell -NoProfile -Command Get-Content -LiteralPath 'src\main.rs' | Select-Object -Skip 100 -First 20"#,
        r#"rtk read src\main.rs --max-lines 120"#,
    );
}

#[test]
fn powershell_test_and_lint_wrappers_redirect_inner_tools() {
    assert_deny(
        r#"powershell -NoProfile -Command $env:PATH = "$env:APPDATA\luarocks\bin;$env:PATH"; busted spec"#,
        r#"pwsh -NoProfile -Command '$env:PATH = "$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted spec'"#,
    );
    assert_deny(
        r#"rtk powershell -NoProfile -Command $env:PATH = "$env:APPDATA\luarocks\bin;$env:PATH"; busted spec"#,
        r#"pwsh -NoProfile -Command '$env:PATH = "$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted spec'"#,
    );
    assert_deny(
        r#"powershell -NoProfile -Command $env:PATH = "$env:APPDATA\luarocks\bin;$env:PATH"; luacheck --codes spec src main.rs"#,
        r#"pwsh -NoProfile -Command '$env:PATH = "$env:APPDATA\luarocks\bin;$env:PATH"; rtk luacheck --codes spec src main.rs'"#,
    );
    assert_deny(
        r#"rtk pwsh -NoProfile -Command $env:PATH="$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted spec"#,
        r#"pwsh -NoProfile -Command '$env:PATH="$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted spec'"#,
    );
}

#[test]
fn unix_shell_wrappers_redirect_inner_test_tools() {
    assert_deny(
        r#"bash -lc 'PATH="$HOME/.luarocks/bin:$PATH" busted spec'"#,
        r#"bash -lc 'PATH="$HOME/.luarocks/bin:$PATH" rtk busted spec'"#,
    );
    assert_deny(
        r#"bash -c 'PATH="$HOME/.luarocks/bin:$PATH" luacheck --codes spec src main.lua _meta.lua'"#,
        r#"bash -c 'PATH="$HOME/.luarocks/bin:$PATH" rtk luacheck --codes spec src main.lua _meta.lua'"#,
    );
    assert_deny(
        r#"rtk bash -lc 'PATH="$HOME/.luarocks/bin:$PATH" rtk busted spec'"#,
        r#"bash -lc 'PATH="$HOME/.luarocks/bin:$PATH" rtk busted spec'"#,
    );
    assert_no_output(r#"bash -lc 'PATH="$HOME/.luarocks/bin:$PATH" rtk busted spec'"#);
    assert_no_output(r#"bash -lc 'echo hi | busted spec'"#);
    assert_no_output(r#"bash -n .codex/hooks/rtk-codex-pretool.sh"#);

    assert_deny(
        r#"env PATH=$HOME/.luarocks/bin:$PATH busted spec"#,
        r#"env PATH=$HOME/.luarocks/bin:$PATH rtk busted spec"#,
    );
    assert_deny(
        r#"rtk env PATH=$HOME/.luarocks/bin:$PATH rtk luacheck --codes spec"#,
        r#"env PATH=$HOME/.luarocks/bin:$PATH rtk luacheck --codes spec"#,
    );
    assert_no_output(r#"env PATH=$HOME/.luarocks/bin:$PATH rtk busted spec"#);
    assert_no_output(r#"env PATH=$HOME/.luarocks/bin:$PATH node --check hook.js"#);
}

#[test]
fn select_string_redirects_to_rtk_grep() {
    assert_rewrite(
        r#"Select-String -Path 'src\main.rs' -Pattern 'foo' -Context 2"#,
        r#"rtk grep -n "foo" src\main.rs -- -C 2"#,
    );
    assert_rewrite("sls foo src\\main.rs", r#"rtk grep -n "foo" src\main.rs"#);
    assert_rewrite(
        r#"powershell -NoProfile -Command Select-String -Path 'src\main.rs' -Pattern 'function'"#,
        r#"rtk grep -n "function" src\main.rs"#,
    );
    assert_rewrite(
        r#"powershell -NoProfile -Command Select-String -Path 'src\main.rs' -Pattern 'function' -Context 2"#,
        r#"rtk grep -n "function" src\main.rs -- -C 2"#,
    );
    assert_no_output("Select-String -Pattern foo");
    assert_no_output(r#"Select-String -Path src\main.rs -Pattern foo -SimpleMatch"#);
}

#[test]
fn get_child_item_redirects_to_rtk_find() {
    assert_rewrite("Get-ChildItem -Path src -Recurse -File", "rtk find src");
    assert_rewrite("gci src -Recurse -File", "rtk find src");
    assert_rewrite("dir src -Recurse -File", "rtk find src");
    assert_rewrite(
        "powershell -NoProfile -Command Get-ChildItem -Path src -Recurse -File",
        "rtk find src",
    );
    assert_no_output("dir");
    assert_rewrite("ls src", "rtk ls src");
    assert_no_output("Get-ChildItem -Path src -File");
    assert_no_output("dir src -Recurse");
    assert_no_output("Get-ChildItem -Path src | Out-File files.txt");
    assert_no_output("Get-ChildItem -Path src -Recurse -Directory");
    assert_no_output("Get-ChildItem -Path src -Recurse -Filter *.rs");
}

#[test]
fn pipelines_prioritize_search_over_read() {
    assert_rewrite(
        "Get-Content src\\rewrite.rs | Select-String -Pattern tokenize",
        r#"rtk grep -n "tokenize" src\rewrite.rs"#,
    );
    assert_rewrite(
        "Get-Content src\\rewrite.rs|Select-String -Pattern tokenize",
        r#"rtk grep -n "tokenize" src\rewrite.rs"#,
    );
    assert_rewrite(
        "gc src\\rewrite.rs | sls tokenize",
        r#"rtk grep -n "tokenize" src\rewrite.rs"#,
    );
    assert_no_output(
        "Get-Content src\\rewrite.rs | Select-Object -First 10 | Select-String tokenize",
    );
    assert_no_output("Get-Content input.txt | Out-File output.txt");
}

#[test]
fn noisy_tool_allowlist_redirects_to_rtk() {
    for tool in [
        "dotnet",
        "pnpm",
        "pip",
        "go",
        "docker",
        "npx",
        "vitest",
        "jest",
        "tsc",
        "ruff",
        "mypy",
        "playwright",
        "gradlew",
        "curl",
    ] {
        assert_rewrite(
            &format!("{tool} --version"),
            &format!("rtk {tool} --version"),
        );
    }

    assert_rewrite("python -m pytest tests", "rtk pytest tests");
    assert_rewrite("uv run pytest tests", "rtk pytest tests");
    assert_no_output("python -m pip install pytest");
    assert_deny("uv pip install pytest", "rtk uv pip install pytest");
}

#[test]
fn cmd_findstr_redirects_when_simple_search() {
    assert_rewrite(
        "findstr /N tokenize src\\rewrite.rs",
        r#"rtk grep -n "tokenize" src\rewrite.rs"#,
    );
    assert_no_output("findstr /S /N tokenize *.rs");
    assert_no_output("findstr /R /C:\"foo bar\" *.rs");
}

#[test]
fn raw_rg_redirects_to_rtk_grep_with_quoted_patterns() {
    assert_rewrite(
        "rg -n showExtensionsMenu src tests",
        r#"rtk grep -n "showExtensionsMenu" src tests"#,
    );
    assert_rewrite(
        "rg -n showExtensionsMenu|updateExtensionsMenu src tests",
        r#"rtk grep -n "showExtensionsMenu|updateExtensionsMenu" src tests"#,
    );
    assert_rewrite(
        "rtk rg -n staleAlias src",
        r#"rtk grep -n "staleAlias" src"#,
    );
    assert_rewrite(
        r#"rg -n -C 2 "package\.loaded\.lfs" spec"#,
        r#"rtk grep -n "package\.loaded\.lfs" spec -- -C 2"#,
    );
    assert_rewrite(
        r#"rg -n --hidden --glob "!**/.git/**" "foo" src"#,
        r#"rtk grep -n "foo" src -- --hidden --glob !**/.git/**"#,
    );
    assert_rewrite(
        r#"rg -n -- "-- Boundary:|scaleBySize" suwayomi main.lua _meta.lua"#,
        r#"rtk grep -n "\-\- Boundary:|scaleBySize" suwayomi main.lua _meta.lua"#,
    );
    assert_no_output(r#"rg -n "-- Boundary:|scaleBySize" suwayomi main.lua _meta.lua"#);
    assert_no_output("rtk rg --json staleAlias src");
    assert_no_output("rtk rg -e staleAlias -e freshAlias src");
    assert_deny("rg --files", "rtk find \"*\" . --max 50 --file-type f");
    assert_deny(
        "rg --files src",
        "rtk find \"*\" src --max 50 --file-type f",
    );
    assert_no_output("rg --files | head");
    assert_no_output("rg --files > files.txt");
    assert_no_output("rg --files --hidden");
    assert_no_output("rg --files -g '*.rs'");
}

#[test]
fn unix_shell_reads_redirect_to_rtk_read() {
    assert_rewrite("cat README.md", "rtk read README.md");
    assert_rewrite(
        "head -n 40 src/main.rs",
        "rtk read src/main.rs --max-lines 40",
    );
    assert_rewrite(
        "head -40 src/main.rs",
        "rtk read src/main.rs --max-lines 40",
    );
    assert_rewrite("tail -n 25 README.md", "rtk read README.md --tail-lines 25");
    assert_rewrite("tail -25 README.md", "rtk read README.md --tail-lines 25");
    assert_rewrite(
        "sed -n '1,120p' src/rewrite.rs",
        "rtk read src/rewrite.rs --max-lines 120",
    );
    assert_rewrite(
        "rtk sed -n 1,80p README.md",
        "rtk read README.md --max-lines 80",
    );
    assert_rewrite("nl -ba src/rewrite.rs", "rtk read -n src/rewrite.rs");
    assert_rewrite("rtk nl -ba README.md", "rtk read -n README.md");

    assert_no_output("cat src/main.rs README.md");
    assert_no_output("cat README.md > copy.md");
    assert_no_output("cat README.md | tee copy.md");
    assert_no_output("type rtk");
    assert_no_output("head README.md");
    assert_no_output("tail README.md");
    assert_no_output("sed -n '100,160p' src/rewrite.rs");
    assert_no_output("sed -i '1,10d' src/rewrite.rs");
    assert_no_output("nl src/rewrite.rs");
    assert_no_output("nl -ba src/rewrite.rs README.md");
}

#[test]
fn unix_shell_search_and_find_redirects_are_conservative() {
    assert_rewrite(
        "grep -n tokenize src/rewrite.rs",
        r#"rtk grep -n "tokenize" src/rewrite.rs"#,
    );
    assert_rewrite(
        r#"grep -n "foo|bar" src/rewrite.rs tests/pretool.rs"#,
        r#"rtk grep -n "foo|bar" src/rewrite.rs tests/pretool.rs"#,
    );
    assert_rewrite("find src -type f", "rtk find src");
    assert_rewrite("find . -type f", "rtk find .");

    assert_no_output("grep -r tokenize src");
    assert_no_output("grep -v tokenize src/rewrite.rs");
    assert_no_output("grep tokenize");
    assert_no_output("find src -type d");
    assert_no_output("find src -type f -name '*.rs'");
    assert_no_output("find src -delete");
}
