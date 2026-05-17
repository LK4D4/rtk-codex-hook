use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_codex_home(name: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "rtk-codex-hook-cli-{name}-{}-{suffix}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("create temp codex home");
    path
}

#[test]
fn legacy_install_flag_does_not_write_codex_config() {
    let codex_home = temp_codex_home("legacy-install-flag");

    let output = Command::new(env!("CARGO_BIN_EXE_rtk-codex-hook"))
        .arg("--install-codex-hook")
        .env("CODEX_HOME", &codex_home)
        .output()
        .expect("run legacy install flag");

    assert!(
        output.status.success(),
        "legacy flag should fail open, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!("", String::from_utf8_lossy(&output.stdout));
    assert_eq!("", String::from_utf8_lossy(&output.stderr));
    assert!(
        !codex_home.join("hooks.json").exists(),
        "agent owns hooks.json edits"
    );
}
