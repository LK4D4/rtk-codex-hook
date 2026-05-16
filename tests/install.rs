use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_codex_home(name: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "rtk-codex-hook-install-{name}-{}-{suffix}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("create temp codex home");
    path
}

fn install_with_home(codex_home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rtk-codex-hook"))
        .arg("--install-codex-hook")
        .env("CODEX_HOME", codex_home)
        .output()
        .expect("run installer")
}

fn read_hooks(path: &std::path::Path) -> serde_json::Value {
    let hooks_json = std::fs::read_to_string(path.join("hooks.json")).expect("read hooks.json");
    serde_json::from_str(&hooks_json).expect("parse hooks.json")
}

#[test]
fn install_creates_codex_hooks_json() {
    let codex_home = temp_codex_home("create");

    let output = install_with_home(&codex_home);

    assert!(
        output.status.success(),
        "installer should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "installer should keep stderr quiet on success, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let hooks = read_hooks(&codex_home);
    let entries = hooks["hooks"]["PreToolUse"]
        .as_array()
        .expect("PreToolUse entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["command"], env!("CARGO_BIN_EXE_rtk-codex-hook"));
    assert!(!codex_home.join("hooks.json.bak").exists());
}

#[test]
fn install_preserves_existing_hooks_and_writes_backup_once() {
    let codex_home = temp_codex_home("preserve");
    let hooks_path = codex_home.join("hooks.json");
    let original = serde_json::json!({
        "description": "existing config",
        "hooks": {
            "Stop": [
                { "command": "echo stop" }
            ],
            "PreToolUse": [
                { "command": "existing-pretool" }
            ]
        }
    })
    .to_string();
    std::fs::write(&hooks_path, &original).expect("write existing hooks");

    let first = install_with_home(&codex_home);
    assert!(
        first.status.success(),
        "installer should succeed, stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let hooks = read_hooks(&codex_home);
    assert_eq!(hooks["description"], "existing config");
    assert_eq!(hooks["hooks"]["Stop"][0]["command"], "echo stop");
    let pretool = hooks["hooks"]["PreToolUse"]
        .as_array()
        .expect("PreToolUse entries");
    assert_eq!(pretool.len(), 2);
    assert_eq!(pretool[0]["command"], "existing-pretool");
    assert_eq!(pretool[1]["command"], env!("CARGO_BIN_EXE_rtk-codex-hook"));
    assert_eq!(
        std::fs::read_to_string(codex_home.join("hooks.json.bak")).expect("read backup"),
        original
    );

    let second = install_with_home(&codex_home);
    assert!(
        second.status.success(),
        "second installer run should succeed, stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let hooks = read_hooks(&codex_home);
    let pretool = hooks["hooks"]["PreToolUse"]
        .as_array()
        .expect("PreToolUse entries");
    assert_eq!(pretool.len(), 2, "installer should not duplicate entries");
}
