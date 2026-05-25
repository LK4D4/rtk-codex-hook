use serde::Deserialize;

#[derive(Deserialize)]
struct Payload {
    hook_event_name: Option<String>,
    tool_name: Option<String>,
    tool_input: Option<serde_json::Value>,
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

    if !is_shell_like_tool(payload.tool_name.as_deref()) {
        return None;
    }

    let tool_input = payload.tool_input?;
    let command = tool_input.get("command")?.as_str()?;
    let command = command.trim();
    if command.is_empty() {
        return None;
    }

    match crate::rewrite::action(command)? {
        crate::rewrite::HookAction::AutoRewrite(rewrite) => {
            log(&format!("allow original=[{command}] rewrite=[{rewrite}]"));
            Some(allow_json(updated_input_with_command(
                &tool_input,
                &rewrite,
            )))
        }
        crate::rewrite::HookAction::DenySuggestion(suggestion) => {
            log(&format!(
                "deny original=[{command}] suggestion=[{suggestion}]"
            ));
            Some(deny_json(&suggestion))
        }
    }
}

fn is_shell_like_tool(tool_name: Option<&str>) -> bool {
    let Some(tool_name) = tool_name else {
        return false;
    };

    tool_name == "Bash"
        || tool_name == "shell_command"
        || tool_name == "exec_command"
        || tool_name.ends_with("__shell_command")
        || tool_name.ends_with("__exec_command")
        || tool_name.ends_with(".shell_command")
        || tool_name.ends_with(".exec_command")
}

fn updated_input_with_command(tool_input: &serde_json::Value, command: &str) -> serde_json::Value {
    match tool_input {
        serde_json::Value::Object(fields) => {
            let mut fields = fields.clone();
            fields.insert(
                "command".to_string(),
                serde_json::Value::String(command.to_string()),
            );
            serde_json::Value::Object(fields)
        }
        _ => serde_json::json!({ "command": command }),
    }
}

fn allow_json(updated_input: serde_json::Value) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "updatedInput": updated_input
        }
    })
    .to_string()
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
