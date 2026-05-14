use std::io::Write;
use std::process::{Command, Stdio};

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

fn run_hook(payload: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rtk-codex-hook"))
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
    assert_no_output(
        r#"rtk pwsh -NoProfile -Command '$env:PATH="$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted spec'"#,
    );
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
    assert_no_output("Get-Content -LiteralPath input.txt > output.txt");
}

#[test]
fn generic_rtk_rewrite_fallbacks_apply_to_common_tools() {
    assert_deny("git status --short", "rtk git status --short");
}

#[test]
fn get_content_redirects_to_rtk_read() {
    assert_deny(
        r#"Get-Content -TotalCount 14 'src\main.rs'"#,
        r#"rtk read src\main.rs --max-lines 14"#,
    );
    assert_deny(
        r#"powershell -NoProfile -Command Get-Content -LiteralPath 'src\main.rs' -TotalCount 80"#,
        r#"rtk read src\main.rs --max-lines 80"#,
    );
    assert_deny(
        r#"powershell -NoProfile -Command Get-Content -LiteralPath 'src\main.rs' -TotalCount 140"#,
        r#"rtk read src\main.rs --max-lines 140"#,
    );
    assert_deny(
        r#"powershell -NoProfile -Command Get-Content -LiteralPath 'README.md' -TotalCount 80"#,
        "rtk read README.md --max-lines 80",
    );
    assert_deny(
        "Get-Content -Tail 30 README.md",
        "rtk read README.md --tail-lines 30",
    );
    assert_deny(
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
        r#"rtk pwsh -NoProfile -Command '$env:PATH = "$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted spec'"#,
    );
    assert_deny(
        r#"rtk powershell -NoProfile -Command $env:PATH = "$env:APPDATA\luarocks\bin;$env:PATH"; busted spec"#,
        r#"rtk pwsh -NoProfile -Command '$env:PATH = "$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted spec'"#,
    );
    assert_deny(
        r#"powershell -NoProfile -Command $env:PATH = "$env:APPDATA\luarocks\bin;$env:PATH"; luacheck --codes spec src main.rs"#,
        r#"rtk pwsh -NoProfile -Command '$env:PATH = "$env:APPDATA\luarocks\bin;$env:PATH"; rtk luacheck --codes spec src main.rs'"#,
    );
    assert_deny(
        r#"pwsh -NoProfile -Command $env:PATH="$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted spec"#,
        r#"rtk pwsh -NoProfile -Command '$env:PATH="$env:APPDATA\luarocks\bin;$env:PATH"; rtk busted spec'"#,
    );
}

#[test]
fn select_string_redirects_to_rtk_rg() {
    assert_deny(
        r#"powershell -NoProfile -Command Select-String -Path 'src\main.rs' -Pattern 'function'"#,
        r#"rtk rg -n "function" src\main.rs"#,
    );
    assert_deny(
        r#"powershell -NoProfile -Command Select-String -Path 'src\main.rs' -Pattern 'function' -Context 2"#,
        r#"rtk rg -n -C 2 "function" src\main.rs"#,
    );
}

#[test]
fn get_child_item_redirects_to_rg_files() {
    assert_deny(
        "powershell -NoProfile -Command Get-ChildItem -Path src -Recurse -File",
        "rtk rg --files src",
    );
}

#[test]
fn raw_rg_redirects_to_rtk_rg_with_quoted_patterns() {
    assert_deny(
        "rg -n showExtensionsMenu src tests",
        r#"rtk rg -n "showExtensionsMenu" src tests"#,
    );
    assert_deny(
        "rg -n showExtensionsMenu|updateExtensionsMenu src tests",
        r#"rtk rg -n "showExtensionsMenu|updateExtensionsMenu" src tests"#,
    );
    assert_deny("rg --files -g '*.rs'", "rtk rg --files -g *.rs");
}
