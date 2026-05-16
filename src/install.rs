use serde_json::{Map, Value};
use std::path::PathBuf;

pub fn install_codex_hook() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let codex_home = codex_home()?;
    std::fs::create_dir_all(&codex_home)?;
    let hooks_path = codex_home.join("hooks.json");
    let existed = hooks_path.try_exists()?;
    let original = if existed {
        Some(std::fs::read_to_string(&hooks_path)?)
    } else {
        None
    };

    let mut config = match original.as_deref() {
        Some(contents) => serde_json::from_str::<Value>(contents)?,
        None => Value::Object(Map::new()),
    };
    let changed = add_hook_entry(&mut config, &current_exe_command()?)?;
    if !changed {
        return Ok(hooks_path);
    }

    let next = serde_json::to_string_pretty(&config)?;
    if let Some(contents) = original {
        std::fs::write(hooks_path.with_extension("json.bak"), contents)?;
    }
    std::fs::write(&hooks_path, format!("{next}\n"))?;
    Ok(hooks_path)
}

fn add_hook_entry(config: &mut Value, command: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let root = config
        .as_object_mut()
        .ok_or("hooks.json must contain a JSON object")?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let hooks = hooks
        .as_object_mut()
        .ok_or("hooks.json field `hooks` must contain a JSON object")?;
    let pretool = hooks
        .entry("PreToolUse")
        .or_insert_with(|| Value::Array(Vec::new()));
    let pretool = pretool
        .as_array_mut()
        .ok_or("hooks.json field `hooks.PreToolUse` must contain an array")?;

    if pretool
        .iter()
        .any(|entry| entry.get("command").and_then(Value::as_str) == Some(command))
    {
        return Ok(false);
    }

    let mut entry = Map::new();
    entry.insert("command".to_string(), Value::String(command.to_string()));
    pretool.push(Value::Object(entry));
    Ok(true)
}

fn current_exe_command() -> Result<String, Box<dyn std::error::Error>> {
    Ok(std::env::current_exe()?.to_string_lossy().into_owned())
}

fn codex_home() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = std::env::var_os("CODEX_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or("CODEX_HOME, HOME, or USERPROFILE must be set")?;
    Ok(PathBuf::from(home).join(".codex"))
}
