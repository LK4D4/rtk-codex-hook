use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookAction {
    AutoRewrite(String),
    DenySuggestion(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    start: usize,
    end: usize,
}

pub fn action(command: &str) -> Option<HookAction> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }

    invalid_rtk_read_redirect(command)
        .map(HookAction::DenySuggestion)
        .or_else(|| invalid_rtk_grep_redirect(command).map(HookAction::DenySuggestion))
        .or_else(|| {
            if starts_with_rtk(command) && preferred_rtk_command(command)
                || is_preferred_pwsh_wrapper(command)
                || is_preferred_bash_wrapper(command)
            {
                return None;
            }

            let lossy_read_window = select_skip_first(command).is_some();
            let rg_files = is_rg_files_command(command);

            direct_powershell_redirect(command)
                .map(|suggestion| {
                    if lossy_read_window {
                        HookAction::DenySuggestion(suggestion)
                    } else {
                        HookAction::AutoRewrite(suggestion)
                    }
                })
                .or_else(|| {
                    powershell_safe_redirect(command).map(|suggestion| {
                        if lossy_read_window {
                            HookAction::DenySuggestion(suggestion)
                        } else {
                            HookAction::AutoRewrite(suggestion)
                        }
                    })
                })
                .or_else(|| powershell_wrapper_redirect(command).map(HookAction::DenySuggestion))
                .or_else(|| env_redirect(command).map(HookAction::DenySuggestion))
                .or_else(|| bash_redirect(command).map(HookAction::DenySuggestion))
                .or_else(|| posix_redirect(command).map(HookAction::AutoRewrite))
                .or_else(|| {
                    rg_redirect(command).map(|suggestion| {
                        if rg_files {
                            HookAction::DenySuggestion(suggestion)
                        } else {
                            HookAction::AutoRewrite(suggestion)
                        }
                    })
                })
                .or_else(|| {
                    safe_external_rtk_rewrite(command)
                        .map(|suggestion| classify_external_rewrite(command, suggestion))
                })
                .or_else(|| {
                    local_rtk_miss_fallback(command)
                        .map(|suggestion| classify_external_rewrite(command, suggestion))
                })
        })
}

pub fn suggest(command: &str) -> Option<String> {
    action(command).map(|action| match action {
        HookAction::AutoRewrite(command) | HookAction::DenySuggestion(command) => command,
    })
}

fn invalid_rtk_read_redirect(command: &str) -> Option<String> {
    let tokens = tokenize(command);
    if command_name(&tokens.first()?.text) != "rtk" || command_name(&tokens.get(1)?.text) != "read"
    {
        return None;
    }

    tokens
        .iter()
        .skip(2)
        .any(|token| {
            matches!(
                token.text.as_str(),
                "--line" | "--lines" | "--range" | "--start" | "--from" | "--to" | "--line-number"
            ) || token.text.starts_with("--line=")
                || token.text.starts_with("--lines=")
                || token.text.starts_with("--range=")
                || token.text == "--start-line"
                || token.text.starts_with("--start-line=")
                || token.text.starts_with("--start=")
                || token.text.starts_with("--from=")
                || token.text.starts_with("--to=")
                || token.text.starts_with("--line-number=")
                || looks_like_rtk_read_path_range(&token.text)
        })
        .then(|| "rtk read --help".to_string())
}

fn looks_like_rtk_read_path_range(value: &str) -> bool {
    let Some((path, range)) = value.rsplit_once(':') else {
        return false;
    };
    !path.is_empty()
        && path != "."
        && range.split_once('-').map_or_else(
            || range.chars().all(|ch| ch.is_ascii_digit()),
            |(start, end)| {
                !start.is_empty()
                    && !end.is_empty()
                    && start.chars().all(|ch| ch.is_ascii_digit())
                    && end.chars().all(|ch| ch.is_ascii_digit())
            },
        )
}

fn invalid_rtk_grep_redirect(command: &str) -> Option<String> {
    let tokens = tokenize(command);
    if command_name(&tokens.first()?.text) != "rtk" || command_name(&tokens.get(1)?.text) != "grep"
    {
        return None;
    }

    let passthrough_with_values = [
        "-C",
        "--context",
        "-A",
        "--after-context",
        "-B",
        "--before-context",
    ];
    let mut rtk_options = Vec::new();
    let mut passthrough = Vec::new();
    let mut index = 2;
    while index < tokens.len() && tokens[index].text.starts_with('-') && tokens[index].text != "--"
    {
        let option = tokens[index].text.as_str();
        if matches!(option, "-n" | "--line-number") {
            rtk_options.push("-n".to_string());
            index += 1;
            continue;
        }
        if passthrough_with_values.contains(&option) && index + 1 < tokens.len() {
            passthrough.push(tokens[index].text.clone());
            index += 1;
            passthrough.push(tokens[index].text.clone());
            index += 1;
            continue;
        }
        return None;
    }

    if passthrough.is_empty() || index >= tokens.len() || tokens[index].text == "--" {
        return None;
    }

    let pattern = &tokens[index].text;
    let paths = tokens[index + 1..]
        .iter()
        .take_while(|token| token.text != "--")
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>();
    if paths.iter().any(|path| path.starts_with('-')) {
        return None;
    }

    let mut parts = vec!["rtk".to_string(), "grep".to_string()];
    parts.extend(rtk_options);
    parts.push(quote_pattern(pattern));
    if paths.is_empty() {
        parts.push(".".to_string());
    } else {
        parts.extend(paths.into_iter().map(quote_arg));
    }
    parts.push("--".to_string());
    parts.extend(passthrough.into_iter().map(|arg| quote_arg(&arg)));
    Some(parts.join(" "))
}

fn preferred_rtk_command(command: &str) -> bool {
    let tokens = tokenize(command);
    let second = tokens.get(1).map(|token| command_name(&token.text));
    matches!(
        second.as_deref(),
        Some(
            "git"
                | "grep"
                | "find"
                | "read"
                | "cargo"
                | "gh"
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
        )
    ) || is_preferred_pwsh_wrapper(command)
        || is_preferred_bash_wrapper(command)
}

fn starts_with_rtk(command: &str) -> bool {
    tokenize(command)
        .first()
        .is_some_and(|token| command_name(&token.text) == "rtk")
}

fn command_name(value: &str) -> String {
    value
        .trim_end_matches(".exe")
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(value)
        .to_ascii_lowercase()
}

fn direct_powershell_redirect(command: &str) -> Option<String> {
    let without_rtk = strip_rtk_prefix(command).unwrap_or(command);
    if contains_mutating_powershell(without_rtk) {
        return None;
    }

    let tokens = tokenize(without_rtk);
    let first = tokens.first().map(|token| command_name(&token.text));
    match first.as_deref() {
        Some("get-content" | "gc") => {
            pipeline_select_string_redirect(without_rtk).or_else(|| content_redirect(without_rtk))
        }
        Some("cat" | "type") if has_powershell_read_option(&tokens) => {
            pipeline_select_string_redirect(without_rtk).or_else(|| content_redirect(without_rtk))
        }
        Some("select-string" | "sls") => select_string_redirect(without_rtk),
        Some("get-childitem" | "gci" | "dir") => get_child_item_redirect(without_rtk),
        Some("findstr") => findstr_redirect(without_rtk),
        _ => None,
    }
}

fn powershell_safe_redirect(command: &str) -> Option<String> {
    let without_rtk = strip_rtk_prefix(command).unwrap_or(command);
    let inner = inner_powershell_command(without_rtk)?;

    if contains_mutating_powershell(&inner) {
        return None;
    }

    pipeline_select_string_redirect(&inner)
        .or_else(|| content_redirect(&inner))
        .or_else(|| select_string_redirect(&inner))
        .or_else(|| get_child_item_redirect(&inner))
}

fn powershell_wrapper_redirect(command: &str) -> Option<String> {
    let without_rtk = strip_rtk_prefix(command).unwrap_or(command);
    let inner = inner_powershell_command(without_rtk)?;

    if contains_mutating_powershell(&inner) {
        return None;
    }

    test_tool_redirect(&inner)
}

fn strip_rtk_prefix(command: &str) -> Option<&str> {
    let tokens = tokenize(command);
    if tokens
        .first()
        .is_some_and(|token| command_name(&token.text) == "rtk")
    {
        Some(command[tokens[0].end..].trim_start())
    } else {
        None
    }
}

fn is_preferred_pwsh_wrapper(command: &str) -> bool {
    if starts_with_rtk(command) {
        return false;
    }
    if tokenize(command)
        .first()
        .is_none_or(|token| command_name(&token.text) != "pwsh")
    {
        return false;
    }

    inner_powershell_command(command).is_some_and(|inner| {
        command_segments(&inner).iter().any(|segment| {
            let tokens = tokenize(segment);
            tokens.len() >= 2
                && command_name(&tokens[0].text) == "rtk"
                && matches!(
                    command_name(&tokens[1].text).as_str(),
                    "busted" | "luacheck"
                )
        })
    })
}

fn is_preferred_bash_wrapper(command: &str) -> bool {
    if starts_with_rtk(command) {
        return false;
    }
    if tokenize(command)
        .first()
        .is_none_or(|token| command_name(&token.text) != "bash")
    {
        return false;
    }

    inner_bash_command(command).is_some_and(|(_, inner)| {
        let tokens = tokenize(&inner);
        find_test_tool_invocation(&tokens).is_some_and(|(_, _, _, already_rtk)| already_rtk)
    })
}

fn inner_powershell_command(command: &str) -> Option<String> {
    let tokens = tokenize(command);
    if !tokens
        .first()
        .is_some_and(|token| matches!(command_name(&token.text).as_str(), "pwsh" | "powershell"))
    {
        return None;
    }

    let command_token = tokens
        .iter()
        .position(|token| matches!(token.text.to_ascii_lowercase().as_str(), "-command" | "-c"))?;
    let start = tokens.get(command_token + 1)?.start;
    let mut inner = command[start..].trim().to_string();
    inner = strip_outer_quotes(&inner).to_string();
    if inner.starts_with("& {") && inner.ends_with('}') {
        inner = inner[3..inner.len() - 1].trim().to_string();
    }
    Some(inner)
}

fn inner_bash_command(command: &str) -> Option<(String, String)> {
    let tokens = tokenize(command);
    if tokens
        .first()
        .is_none_or(|token| command_name(&token.text) != "bash")
    {
        return None;
    }

    let command_token = tokens.iter().position(|token| {
        matches!(token.text.as_str(), "-c" | "-lc" | "-cl")
            || token.text.starts_with('-') && token.text.contains('c') && !token.text.contains('n')
    })?;
    let option = tokens[command_token].text.clone();
    let start = tokens.get(command_token + 1)?.start;
    let inner = strip_outer_quotes(command[start..].trim()).to_string();
    Some((option, inner))
}

fn strip_outer_quotes(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"'))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn contains_mutating_powershell(command: &str) -> bool {
    tokenize(command).iter().any(|token| {
        matches!(
            command_name(&token.text).as_str(),
            "remove-item"
                | "set-content"
                | "add-content"
                | "new-item"
                | "move-item"
                | "copy-item"
                | "out-file"
        )
    }) || contains_output_redirection(command)
}

fn contains_output_redirection(command: &str) -> bool {
    let mut quote = None;
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (None, '\'' | '"') => quote = Some(ch),
            (None, '>') => return true,
            (None, '2') if chars.peek() == Some(&'>') => return true,
            _ => {}
        }
    }
    false
}

fn env_redirect(command: &str) -> Option<String> {
    let had_rtk_prefix = strip_rtk_prefix(command).is_some();
    let without_rtk = strip_rtk_prefix(command).unwrap_or(command);
    let tokens = tokenize(without_rtk);
    if tokens
        .first()
        .is_none_or(|token| command_name(&token.text) != "env")
    {
        return None;
    }

    let mut index = 1;
    while tokens
        .get(index)
        .is_some_and(|token| is_env_assignment(&token.text))
    {
        index += 1;
    }
    if index == 1 || index >= tokens.len() {
        return None;
    }

    let (command_index, tool_index, tool, already_rtk) = find_test_tool_invocation(
        &tokens[index..],
    )
    .map(|(command_index, tool_index, tool, already_rtk)| {
        (command_index + index, tool_index + index, tool, already_rtk)
    })?;
    if command_index != index || already_rtk && !had_rtk_prefix {
        return None;
    }
    if tokens[tool_index + 1..]
        .iter()
        .any(|token| matches!(token.text.as_str(), "|" | ";"))
    {
        return None;
    }

    let assignments = tokens[1..index]
        .iter()
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let args = join_args(&tokens[tool_index + 1..]);
    Some(format!("env {assignments} rtk {tool}{args}"))
}

fn bash_redirect(command: &str) -> Option<String> {
    let had_rtk_prefix = strip_rtk_prefix(command).is_some();
    let without_rtk = strip_rtk_prefix(command).unwrap_or(command);
    let (option, inner) = inner_bash_command(without_rtk)?;
    let rewritten = shell_inner_test_tool_redirect(&inner)?;
    if rewritten == inner && !had_rtk_prefix {
        return None;
    }
    Some(format!(
        "bash {option} {}",
        quote_bash_command_arg(&rewritten)
    ))
}

fn shell_inner_test_tool_redirect(inner: &str) -> Option<String> {
    if shell_inner_has_unsupported_control(inner) {
        return None;
    }
    let tokens = tokenize(inner);
    let (command_index, tool_index, tool, already_rtk) = find_test_tool_invocation(&tokens)?;
    if tokens[..command_index]
        .iter()
        .any(|token| !is_env_assignment(&token.text))
    {
        return None;
    }

    let prefix = inner[..tokens[command_index].start].trim_end();
    let args = join_args(&tokens[tool_index + 1..]);
    let mut rewritten = String::new();
    if !prefix.is_empty() {
        rewritten.push_str(prefix);
        rewritten.push(' ');
    }
    rewritten.push_str("rtk ");
    rewritten.push_str(&tool);
    rewritten.push_str(&args);
    if already_rtk && rewritten == inner {
        return Some(inner.to_string());
    }
    Some(rewritten)
}

fn shell_inner_has_unsupported_control(inner: &str) -> bool {
    inner.contains('\n')
        || inner.contains('|')
        || inner.contains("&&")
        || inner.contains("||")
        || inner.contains('<')
        || inner.contains('>')
        || inner.contains(';')
}

fn is_env_assignment(value: &str) -> bool {
    let Some((name, _)) = value.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        && !name.as_bytes()[0].is_ascii_digit()
}

fn content_redirect(command: &str) -> Option<String> {
    let tokens = tokenize(command);
    if !tokens
        .iter()
        .any(|token| is_get_content_command(&command_name(&token.text)))
    {
        return None;
    }
    if piped_select_string(&tokens) {
        return None;
    }

    let path = get_content_path(command)?;
    let mut suggestion = format!("rtk read {}", quote_arg(&path));
    if let Some(max_lines) = read_max_lines(command) {
        suggestion.push_str(&format!(" --max-lines {max_lines}"));
    } else if let Some(tail_lines) = read_tail_lines(command) {
        suggestion.push_str(&format!(" --tail-lines {tail_lines}"));
    }
    Some(suggestion)
}

fn get_content_path(command: &str) -> Option<String> {
    path_argument(command, &["-LiteralPath", "-Path"]).or_else(|| {
        let tokens = tokenize(command);
        let content_index = tokens
            .iter()
            .position(|token| is_get_content_command(&command_name(&token.text)))?;
        let value_options = [
            "-totalcount",
            "-tail",
            "-readcount",
            "-encoding",
            "-delimiter",
            "-stream",
        ];
        let mut skip_next = false;
        for token in tokens.iter().skip(content_index + 1) {
            if token.text == "|" || token.text == ";" {
                break;
            }
            if skip_next {
                skip_next = false;
                continue;
            }
            if token.text.starts_with('-') {
                if value_options.contains(&token.text.to_ascii_lowercase().as_str()) {
                    skip_next = true;
                }
                continue;
            }
            return Some(token.text.clone());
        }
        None
    })
}

fn read_max_lines(command: &str) -> Option<u64> {
    select_skip_first(command)
        .map(|(skip, first)| skip + first)
        .or_else(|| option_number(command, &["-TotalCount"]))
        .or_else(|| select_first(command))
}

fn read_tail_lines(command: &str) -> Option<u64> {
    option_number(command, &["-Tail"]).or_else(|| select_last(command))
}

fn select_skip_first(command: &str) -> Option<(u64, u64)> {
    let tokens = tokenize(command);
    let select_index = tokens
        .iter()
        .position(|token| command_name(&token.text) == "select-object")?;
    let window = &tokens[select_index + 1..];
    Some((
        number_after(window, "-Skip")?,
        number_after(window, "-First")?,
    ))
}

fn select_first(command: &str) -> Option<u64> {
    let tokens = tokenize(command);
    let select_index = tokens
        .iter()
        .position(|token| command_name(&token.text) == "select-object")?;
    let window = &tokens[select_index + 1..];
    if number_after(window, "-Skip").is_some() {
        return None;
    }
    number_after(window, "-First")
}

fn select_last(command: &str) -> Option<u64> {
    let tokens = tokenize(command);
    let select_index = tokens
        .iter()
        .position(|token| command_name(&token.text) == "select-object")?;
    number_after(&tokens[select_index + 1..], "-Last")
}

fn test_tool_redirect(inner: &str) -> Option<String> {
    for segment in command_segments(inner) {
        let tokens = tokenize(&segment);
        let Some((tool_index, tool)) = find_test_tool(&tokens) else {
            continue;
        };
        let args = segment[tokens[tool_index].end..].trim();
        let mut rtk_tool = format!("rtk {tool}");
        if !args.is_empty() {
            rtk_tool.push(' ');
            rtk_tool.push_str(args);
        }

        if let Some(setup) = path_setup_prefix(inner) {
            return Some(format!(
                "pwsh -NoProfile -Command {}",
                quote_powershell_command_arg(&format!("{setup}{rtk_tool}"))
            ));
        }
        return Some(rtk_tool);
    }
    None
}

fn find_test_tool(tokens: &[Token]) -> Option<(usize, String)> {
    let (_, tool_index, tool, _) = find_test_tool_invocation(tokens)?;
    Some((tool_index, tool))
}

fn find_test_tool_invocation(tokens: &[Token]) -> Option<(usize, usize, String, bool)> {
    for (index, token) in tokens.iter().enumerate() {
        let name = command_name(&token.text);
        if matches!(name.as_str(), "busted" | "luacheck") {
            return Some((index, index, name, false));
        }
        if name == "rtk"
            && let Some(next) = tokens.get(index + 1)
        {
            let next_name = command_name(&next.text);
            if matches!(next_name.as_str(), "busted" | "luacheck") {
                return Some((index, index + 1, next_name, true));
            }
        }
    }
    None
}

fn is_get_content_command(name: &str) -> bool {
    matches!(name, "get-content" | "gc" | "cat" | "type")
}

fn is_select_string_command(name: &str) -> bool {
    matches!(name, "select-string" | "sls")
}

fn piped_select_string(tokens: &[Token]) -> bool {
    let Some(pipe_index) = tokens.iter().position(|token| token.text == "|") else {
        return false;
    };
    tokens[pipe_index + 1..]
        .iter()
        .any(|token| is_select_string_command(&command_name(&token.text)))
}

fn path_setup_prefix(inner: &str) -> Option<String> {
    let segments = command_segments(inner);
    let first = segments.first()?.trim();
    if first.to_ascii_lowercase().starts_with("$env:path") && first.contains('=') {
        Some(format!("{first}; "))
    } else {
        None
    }
}

fn pipeline_select_string_redirect(inner: &str) -> Option<String> {
    let tokens = tokenize(inner);
    let pipe_index = tokens.iter().position(|token| token.text == "|")?;
    if has_unsupported_options(
        &tokens[..pipe_index],
        &["-literalpath", "-path", "-totalcount", "-tail"],
    ) {
        return None;
    }
    let content_index = tokens[..pipe_index]
        .iter()
        .position(|token| is_get_content_command(&command_name(&token.text)))?;
    let select_index = tokens[pipe_index + 1..]
        .iter()
        .position(|token| is_select_string_command(&command_name(&token.text)))?
        + pipe_index
        + 1;
    if tokens[pipe_index + 1..select_index]
        .iter()
        .any(|token| token.text == "|")
    {
        return None;
    }
    if tokens[select_index..]
        .iter()
        .skip(1)
        .any(|token| token.text == "|")
    {
        return None;
    }
    if has_unsupported_options(&tokens[select_index..], &["-path", "-pattern", "-context"]) {
        return None;
    }

    let path = get_content_path(&inner[tokens[content_index].start..tokens[pipe_index].start])?;
    let search_command = &inner[tokens[select_index].start..];
    let pattern = select_string_pattern(search_command)?;
    let mut suggestion = format!(
        "rtk grep -n {} {}",
        quote_pattern(&pattern),
        quote_arg(&path)
    );
    if let Some(context) = context_number(search_command) {
        append_grep_extra(&mut suggestion, ["-C".to_string(), context.to_string()]);
    }
    Some(suggestion)
}

fn select_string_redirect(inner: &str) -> Option<String> {
    let tokens = tokenize(inner);
    let index = tokens
        .iter()
        .position(|token| is_select_string_command(&command_name(&token.text)))?;
    if has_unsupported_options(&tokens[index..], &["-path", "-pattern", "-context"]) {
        return None;
    }
    let path = select_string_path(inner, index)?;
    let pattern = select_string_pattern(inner)?;
    let mut suggestion = format!(
        "rtk grep -n {} {}",
        quote_pattern(&pattern),
        quote_arg(&path)
    );
    if let Some(context) = context_number(inner) {
        append_grep_extra(&mut suggestion, ["-C".to_string(), context.to_string()]);
    }
    Some(suggestion)
}

fn append_grep_extra(suggestion: &mut String, extras: impl IntoIterator<Item = String>) {
    let extras = extras.into_iter().collect::<Vec<_>>();
    if extras.is_empty() {
        return;
    }

    suggestion.push_str(" --");
    for extra in extras {
        suggestion.push(' ');
        suggestion.push_str(&quote_arg(&extra));
    }
}

fn select_string_path(command: &str, command_index: usize) -> Option<String> {
    path_argument(command, &["-Path"]).or_else(|| {
        let tokens = tokenize(command);
        positional_after_command(&tokens, command_index, 1)
    })
}

fn select_string_pattern(command: &str) -> Option<String> {
    path_argument(command, &["-Pattern"]).or_else(|| {
        let tokens = tokenize(command);
        let command_index = tokens
            .iter()
            .position(|token| is_select_string_command(&command_name(&token.text)))?;
        positional_after_command(&tokens, command_index, 0)
    })
}

fn context_number(inner: &str) -> Option<u64> {
    let tokens = tokenize(inner);
    let index = tokens
        .iter()
        .position(|token| token.text.eq_ignore_ascii_case("-Context"))?;
    let first = tokens.get(index + 1)?.text.parse::<u64>().ok()?;
    let second = tokens
        .get(index + 2)
        .and_then(|token| token.text.trim_start_matches(',').parse::<u64>().ok());
    Some(second.map_or(first, |second| first.max(second)))
}

fn get_child_item_redirect(inner: &str) -> Option<String> {
    let tokens = tokenize(inner);
    let command_index = tokens.iter().position(|token| {
        matches!(
            command_name(&token.text).as_str(),
            "get-childitem" | "gci" | "dir"
        )
    })?;
    if has_unsupported_options(
        &tokens[command_index..],
        &["-path", "-literalpath", "-recurse", "-file"],
    ) {
        return None;
    }
    let command_tokens = tokens[command_index..]
        .iter()
        .take_while(|token| token.text != "|" && token.text != ";")
        .cloned()
        .collect::<Vec<_>>();
    if !has_option(&command_tokens, "-recurse") || !has_option(&command_tokens, "-file") {
        return None;
    }
    let path = path_argument(inner, &["-LiteralPath", "-Path"])
        .or_else(|| positional_after_command(&tokens, command_index, 0))?;
    Some(format!("rtk find {}", quote_arg(&path)))
}

fn has_unsupported_options(tokens: &[Token], supported: &[&str]) -> bool {
    tokens
        .iter()
        .take_while(|token| token.text != "|" && token.text != ";")
        .any(|token| {
            token.text.starts_with('-')
                && !supported.contains(&token.text.to_ascii_lowercase().as_str())
        })
}

fn has_option(tokens: &[Token], name: &str) -> bool {
    tokens
        .iter()
        .any(|token| token.text.eq_ignore_ascii_case(name))
}

fn has_powershell_read_option(tokens: &[Token]) -> bool {
    tokens.iter().any(|token| {
        matches!(
            token.text.to_ascii_lowercase().as_str(),
            "-literalpath" | "-path" | "-totalcount" | "-tail"
        )
    })
}

fn posix_redirect(command: &str) -> Option<String> {
    let without_rtk = strip_rtk_prefix(command).unwrap_or(command);
    if contains_output_redirection(without_rtk) || contains_posix_mutating_pipe(without_rtk) {
        return None;
    }

    let tokens = tokenize(without_rtk);
    let first = tokens.first().map(|token| command_name(&token.text));
    match first.as_deref() {
        Some("cat") => posix_cat_redirect(&tokens),
        Some("head") => posix_head_tail_redirect(&tokens, "--max-lines"),
        Some("tail") => posix_head_tail_redirect(&tokens, "--tail-lines"),
        Some("sed") => posix_sed_redirect(&tokens),
        Some("nl") => posix_nl_redirect(&tokens),
        Some("grep") => posix_grep_redirect(&tokens),
        Some("find") => posix_find_redirect(&tokens),
        _ => None,
    }
}

fn contains_posix_mutating_pipe(command: &str) -> bool {
    command_segments(command).iter().any(|segment| {
        let tokens = tokenize(segment);
        tokens.windows(2).any(|window| {
            window[0].text == "|"
                && matches!(
                    command_name(&window[1].text).as_str(),
                    "tee" | "xargs" | "sh" | "bash" | "zsh"
                )
        })
    })
}

fn posix_cat_redirect(tokens: &[Token]) -> Option<String> {
    if tokens.len() != 2 || tokens[1].text.starts_with('-') {
        return None;
    }
    Some(format!("rtk read {}", quote_arg(&tokens[1].text)))
}

fn posix_head_tail_redirect(tokens: &[Token], rtk_limit: &str) -> Option<String> {
    if tokens.len() != 4 && tokens.len() != 3 {
        return None;
    }

    let (count, path) = if tokens.len() == 4 && tokens[1].text == "-n" {
        (tokens[2].text.parse::<u64>().ok()?, &tokens[3].text)
    } else if tokens.len() == 3 {
        let count = tokens[1].text.strip_prefix('-')?.parse::<u64>().ok()?;
        (count, &tokens[2].text)
    } else {
        return None;
    };

    if path.starts_with('-') {
        return None;
    }
    Some(format!("rtk read {} {rtk_limit} {count}", quote_arg(path)))
}

fn posix_sed_redirect(tokens: &[Token]) -> Option<String> {
    if tokens.len() != 4 || command_name(&tokens[0].text) != "sed" || tokens[1].text != "-n" {
        return None;
    }

    let line_count = tokens[2]
        .text
        .strip_prefix("1,")?
        .strip_suffix('p')?
        .parse::<u64>()
        .ok()?;
    let path = &tokens[3].text;
    if path.starts_with('-') {
        return None;
    }
    Some(format!(
        "rtk read {} --max-lines {line_count}",
        quote_arg(path)
    ))
}

fn posix_nl_redirect(tokens: &[Token]) -> Option<String> {
    if tokens.len() != 3 || command_name(&tokens[0].text) != "nl" || tokens[1].text != "-ba" {
        return None;
    }

    let path = &tokens[2].text;
    if path.starts_with('-') {
        return None;
    }
    Some(format!("rtk read -n {}", quote_arg(path)))
}

fn posix_grep_redirect(tokens: &[Token]) -> Option<String> {
    if tokens.len() < 4 || command_name(&tokens[0].text) != "grep" {
        return None;
    }

    let mut line_number = false;
    let mut index = 1;
    while index < tokens.len() && tokens[index].text.starts_with('-') {
        match tokens[index].text.as_str() {
            "-n" | "--line-number" => line_number = true,
            _ => return None,
        }
        index += 1;
    }

    if !line_number || index + 1 >= tokens.len() {
        return None;
    }

    let pattern = &tokens[index].text;
    if pattern.starts_with('-') {
        return None;
    }
    let paths = tokens[index + 1..]
        .iter()
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>();
    if paths.iter().any(|path| path.starts_with('-')) {
        return None;
    }

    let paths = paths
        .into_iter()
        .map(quote_arg)
        .collect::<Vec<_>>()
        .join(" ");
    Some(format!("rtk grep -n {} {paths}", quote_pattern(pattern)))
}

fn posix_find_redirect(tokens: &[Token]) -> Option<String> {
    if tokens.len() != 4 || command_name(&tokens[0].text) != "find" {
        return None;
    }
    if tokens[2].text != "-type" || tokens[3].text != "f" || tokens[1].text.starts_with('-') {
        return None;
    }
    Some(format!("rtk find {}", quote_arg(&tokens[1].text)))
}

fn findstr_redirect(command: &str) -> Option<String> {
    let tokens = tokenize(command);
    if tokens
        .first()
        .is_none_or(|token| command_name(&token.text) != "findstr")
    {
        return None;
    }
    let mut args = Vec::new();
    for token in tokens.iter().skip(1) {
        let lower = token.text.to_ascii_lowercase();
        if lower == "/s" {
            return None;
        }
        if lower == "/n" {
            continue;
        }
        if lower.starts_with('/') {
            return None;
        }
        args.push(token.text.clone());
    }
    if args.len() < 2 {
        return None;
    }
    let pattern = args.remove(0);
    let paths = args
        .into_iter()
        .map(|arg| quote_arg(&arg))
        .collect::<Vec<_>>()
        .join(" ");
    Some(format!("rtk grep -n {} {}", quote_pattern(&pattern), paths))
}

fn rg_redirect(command: &str) -> Option<String> {
    let without_rtk = strip_rtk_prefix(command).unwrap_or(command);
    let tokens = tokenize(without_rtk);
    if tokens
        .first()
        .is_none_or(|token| command_name(&token.text) != "rg")
    {
        return None;
    }
    let args = tokens
        .iter()
        .skip(1)
        .map(|token| token.text.clone())
        .collect::<Vec<_>>();
    if args.is_empty() {
        return None;
    }
    if args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--json" | "-e" | "--regexp" | "-f" | "--file" | "--replace" | "-r"
        )
    }) {
        return None;
    }
    if args.iter().any(|arg| arg == "--files") {
        return rg_files_redirect(&args);
    }

    let value_options = [
        "-g",
        "--glob",
        "-C",
        "--context",
        "-A",
        "--after-context",
        "-B",
        "--before-context",
        "-m",
        "--max-count",
        "-t",
        "-T",
        "--type",
        "--type-not",
    ];

    let mut rtk_options = Vec::new();
    let mut extra_options = Vec::new();
    let mut index = 0;
    while index < args.len() && args[index].starts_with('-') {
        if args[index] == "--" {
            index += 1;
            break;
        }
        let option = args[index].clone();
        let option_takes_value = value_options.contains(&option.as_str()) && index + 1 < args.len();
        if matches!(option.as_str(), "-n" | "--line-number") {
            rtk_options.push("-n".to_string());
        } else if value_options.contains(&option.as_str()) || option == "--hidden" {
            extra_options.push(option);
        } else {
            return None;
        }
        if option_takes_value {
            index += 1;
            extra_options.push(args[index].clone());
        }
        index += 1;
    }
    if index >= args.len() {
        return None;
    }

    let path_start = args
        .iter()
        .enumerate()
        .skip(index + 1)
        .find_map(|(idx, arg)| is_search_path_token(arg).then_some(idx))
        .unwrap_or(args.len());

    let pattern = join_rg_pattern(&args[index..path_start]).replace("\\|", "|");
    let mut parts = vec!["rtk".to_string(), "grep".to_string()];
    parts.extend(rtk_options);
    parts.push(quote_pattern(&pattern));
    let has_path = path_start < args.len();
    if has_path {
        parts.extend(args[path_start..].iter().map(|arg| quote_arg(arg)));
    } else if !extra_options.is_empty() {
        parts.push(".".to_string());
    }
    if !extra_options.is_empty() {
        parts.push("--".to_string());
        parts.extend(extra_options.into_iter().map(|arg| quote_arg(&arg)));
    }
    Some(parts.join(" "))
}

fn rg_files_redirect(args: &[String]) -> Option<String> {
    let mut path: Option<String> = None;
    for arg in args {
        match arg.as_str() {
            "--files" => {}
            arg if arg.starts_with('-') => return None,
            arg => {
                if path.is_some() {
                    return None;
                }
                path = Some(arg.to_string());
            }
        }
    }

    let path = path.unwrap_or_else(|| ".".to_string());
    Some(format!(
        "rtk find \"*\" {} --max 50 --file-type f",
        quote_arg(&path)
    ))
}

fn is_rg_files_command(command: &str) -> bool {
    let without_rtk = strip_rtk_prefix(command).unwrap_or(command);
    let tokens = tokenize(without_rtk);
    tokens
        .first()
        .is_some_and(|token| command_name(&token.text) == "rg")
        && tokens.iter().any(|token| token.text == "--files")
}

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
            (Some(_), '\\') => {
                chars.next();
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(ch),
            (None, '|' | '<' | '>' | ';' | '&' | '`') => return true,
            (None, '$') if chars.peek() == Some(&'(') => return true,
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
    let Some(first) = original_tokens
        .first()
        .map(|token| command_name(&token.text))
    else {
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

fn local_rtk_miss_fallback(command: &str) -> Option<String> {
    let tokens = tokenize(command);
    let first = tokens.first().map(|token| command_name(&token.text));
    if is_git_diff_pathspec_command(command) {
        return None;
    }
    match first.as_deref() {
        Some("python") if python_pytest_args(&tokens).is_some() => {
            let args = python_pytest_args(&tokens)?;
            Some(format!("rtk pytest{args}"))
        }
        Some("uv") if uv_pytest_args(&tokens).is_some() => {
            let args = uv_pytest_args(&tokens)?;
            Some(format!("rtk pytest{args}"))
        }
        Some(name) if is_safe_wrapper_tool(name) && name != "ls" => Some(format!("rtk {command}")),
        _ => None,
    }
}

fn is_git_diff_pathspec_command(command: &str) -> bool {
    let tokens = tokenize(command);
    tokens.len() >= 4
        && command_name(&tokens[0].text) == "git"
        && tokens.get(1).is_some_and(|token| token.text == "diff")
        && tokens.iter().skip(2).any(|token| token.text == "--")
}

fn python_pytest_args(tokens: &[Token]) -> Option<String> {
    if tokens.len() >= 3 && tokens[1].text == "-m" && command_name(&tokens[2].text) == "pytest" {
        Some(join_args(&tokens[3..]))
    } else {
        None
    }
}

fn uv_pytest_args(tokens: &[Token]) -> Option<String> {
    if tokens.len() >= 3
        && tokens[1].text.eq_ignore_ascii_case("run")
        && command_name(&tokens[2].text) == "pytest"
    {
        Some(join_args(&tokens[3..]))
    } else {
        None
    }
}

fn join_args(tokens: &[Token]) -> String {
    let args = tokens
        .iter()
        .map(|token| quote_arg(&token.text))
        .collect::<Vec<_>>()
        .join(" ");
    if args.is_empty() {
        String::new()
    } else {
        format!(" {args}")
    }
}

fn join_rg_pattern(args: &[String]) -> String {
    let mut pattern = String::new();
    for arg in args {
        if arg == "|" {
            pattern.push('|');
        } else {
            if !pattern.is_empty() && !pattern.ends_with('|') {
                pattern.push(' ');
            }
            pattern.push_str(arg);
        }
    }
    pattern
}

fn rtk_rewrite(command: &str) -> Option<String> {
    if starts_with_rtk(command) {
        return None;
    }
    let rtk = std::env::var_os("RTK_CODEX_HOOK_RTK_BIN").unwrap_or_else(|| "rtk".into());
    let output = Command::new(rtk)
        .arg("rewrite")
        .arg(command)
        .output()
        .ok()?;
    let rewrite = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if rewrite.is_empty() || rewrite == command || !rewrite.starts_with("rtk ") {
        None
    } else {
        Some(rewrite)
    }
}

fn safe_external_rtk_rewrite(command: &str) -> Option<String> {
    if blocks_external_posix_rewrite(command) || is_git_diff_pathspec_command(command) {
        return None;
    }
    rtk_rewrite(command)
}

fn blocks_external_posix_rewrite(command: &str) -> bool {
    let without_rtk = strip_rtk_prefix(command).unwrap_or(command);
    let tokens = tokenize(without_rtk);
    let first = tokens.first().map(|token| command_name(&token.text));
    if first.as_deref() == Some("rg") {
        return rg_redirect(without_rtk).is_none();
    }
    matches!(
        first.as_deref(),
        Some("cat" | "head" | "tail" | "grep" | "find")
    ) && posix_redirect(without_rtk).is_none()
}

fn positional_after_command(
    tokens: &[Token],
    command_index: usize,
    position: usize,
) -> Option<String> {
    let value_options = [
        "-path",
        "-literalpath",
        "-pattern",
        "-context",
        "-totalcount",
        "-tail",
        "-readcount",
        "-encoding",
        "-delimiter",
        "-stream",
    ];
    let mut seen = 0;
    let mut skip_next = false;
    for token in tokens.iter().skip(command_index + 1) {
        if token.text == "|" || token.text == ";" {
            break;
        }
        if skip_next {
            skip_next = false;
            continue;
        }
        if token.text.starts_with('-') {
            if value_options.contains(&token.text.to_ascii_lowercase().as_str()) {
                skip_next = true;
            }
            continue;
        }
        if seen == position {
            return Some(token.text.clone());
        }
        seen += 1;
    }
    None
}

fn path_argument(command: &str, names: &[&str]) -> Option<String> {
    let tokens = tokenize(command);
    tokens.windows(2).find_map(|window| {
        names
            .iter()
            .any(|name| window[0].text.eq_ignore_ascii_case(name))
            .then(|| window[1].text.clone())
    })
}

fn option_number(command: &str, names: &[&str]) -> Option<u64> {
    let tokens = tokenize(command);
    tokens.windows(2).find_map(|window| {
        names
            .iter()
            .any(|name| window[0].text.eq_ignore_ascii_case(name))
            .then(|| window[1].text.parse::<u64>().ok())
            .flatten()
    })
}

fn number_after(tokens: &[Token], name: &str) -> Option<u64> {
    tokens.windows(2).find_map(|window| {
        window[0]
            .text
            .eq_ignore_ascii_case(name)
            .then(|| window[1].text.parse::<u64>().ok())
            .flatten()
    })
}

fn command_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut start = 0;
    let mut quote = None;
    for (index, ch) in command.char_indices() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (None, '\'' | '"') => quote = Some(ch),
            (None, ';') => {
                let segment = command[start..index].trim();
                if !segment.is_empty() {
                    segments.push(segment.to_string());
                }
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    let segment = command[start..].trim();
    if !segment.is_empty() {
        segments.push(segment.to_string());
    }
    segments
}

fn tokenize(command: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut iter = command.char_indices().peekable();
    while let Some((start, ch)) = iter.next() {
        if ch.is_whitespace() {
            continue;
        }
        if ch == ';' || ch == '|' {
            tokens.push(Token {
                text: ch.to_string(),
                start,
                end: start + ch.len_utf8(),
            });
            continue;
        }

        let mut text = String::new();
        let mut end = start + ch.len_utf8();
        if ch == '\'' || ch == '"' {
            let quote = ch;
            for (idx, c) in iter.by_ref() {
                end = idx + c.len_utf8();
                if c == quote {
                    break;
                }
                text.push(c);
            }
        } else {
            text.push(ch);
            while let Some(&(idx, c)) = iter.peek() {
                if c.is_whitespace() || c == ';' || c == '|' {
                    break;
                }
                iter.next();
                end = idx + c.len_utf8();
                text.push(c);
            }
        }

        tokens.push(Token { text, start, end });
    }
    tokens
}

fn is_search_path_token(token: &str) -> bool {
    token == "."
        || token.contains('\\')
        || token.contains('/')
        || token.eq_ignore_ascii_case("src")
        || token.eq_ignore_ascii_case("tests")
        || token.eq_ignore_ascii_case("spec")
        || token.eq_ignore_ascii_case("docs")
        || token.eq_ignore_ascii_case("suwayomi")
        || token.eq_ignore_ascii_case("README.md")
        || token.eq_ignore_ascii_case("AGENTS.md")
        || token.rsplit('.').next().is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "rs" | "lua" | "sh" | "md" | "txt" | "toml" | "yml" | "yaml" | "json"
            )
        })
}

fn quote_arg(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".to_string();
    }
    if value.chars().any(|ch| ch.is_whitespace() || ch == '"') {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

fn quote_pattern(value: &str) -> String {
    let escaped = if value.starts_with('-') {
        let leading_hyphens = value.chars().take_while(|ch| *ch == '-').count();
        format!(
            "{}{}",
            r"\-".repeat(leading_hyphens),
            &value[leading_hyphens..]
        )
    } else {
        value.to_string()
    };
    format!("\"{}\"", escaped.replace('"', "\\\""))
}

fn quote_powershell_command_arg(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn quote_bash_command_arg(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'\''"#))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_keeps_quotes_and_pipes_inside_words() {
        assert_eq!(
            tokenize(r#"rg -n "foo|bar" foo|bar src"#)
                .into_iter()
                .map(|token| token.text)
                .collect::<Vec<_>>(),
            vec!["rg", "-n", "foo|bar", "foo", "|", "bar", "src"]
        );
    }
}
