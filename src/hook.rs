use serde::Deserialize;

#[derive(Deserialize)]
struct Payload {
    hook_event_name: Option<String>,
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    command: Option<String>,
}

pub fn handle_stdin(stdin: &str) -> Option<String> {
    let stdin = stdin.trim();
    if stdin.is_empty() {
        return None;
    }

    let payload: Payload = serde_json::from_str(stdin).ok()?;
    if payload.hook_event_name.as_deref() != Some("PreToolUse") {
        return None;
    }

    let command = payload.tool_input?.command?;
    let command = command.trim();
    if command.is_empty() {
        return None;
    }

    let suggestion = crate::rewrite::suggest(command)?;
    log(&format!(
        "deny original=[{command}] suggestion=[{suggestion}]"
    ));
    Some(deny_json(&suggestion))
}

fn deny_json(suggestion: &str) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": format!(
                "Use RTK-optimized command instead: {suggestion}"
            )
        }
    })
    .to_string()
}

pub fn log(message: &str) {
    if std::env::var_os("RTK_CODEX_HOOK_LOG").is_none() {
        return;
    }

    let path = std::env::var_os("RTK_CODEX_HOOK_LOG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("rtk-codex-hook.log"));
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| {
            use std::io::Write;
            writeln!(file, "{message}")
        });
}
