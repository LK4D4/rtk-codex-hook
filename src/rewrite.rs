use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    start: usize,
    end: usize,
}

pub fn suggest(command: &str) -> Option<String> {
    let command = command.trim();
    if command.is_empty() || starts_with_rtk(command) && preferred_rtk_command(command) {
        return None;
    }

    direct_powershell_redirect(command)
        .or_else(|| powershell_redirect(command))
        .or_else(|| rg_redirect(command))
        .or_else(|| common_rewrite(command))
        .or_else(|| rtk_rewrite(command))
}

fn preferred_rtk_command(command: &str) -> bool {
    let tokens = tokenize(command);
    let second = tokens.get(1).map(|token| command_name(&token.text));
    matches!(
        second.as_deref(),
        Some("git" | "rg" | "read" | "busted" | "luacheck")
    ) || is_preferred_pwsh_wrapper(command)
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
        Some("get-content") => content_redirect(without_rtk),
        _ => None,
    }
}

fn powershell_redirect(command: &str) -> Option<String> {
    let had_rtk = starts_with_rtk(command);
    let without_rtk = strip_rtk_prefix(command).unwrap_or(command);
    let preferred_pwsh = had_rtk
        && tokenize(without_rtk)
            .first()
            .is_some_and(|token| command_name(&token.text) == "pwsh");
    let inner = inner_powershell_command(without_rtk)?;

    if contains_mutating_powershell(&inner) {
        return None;
    }

    content_redirect(&inner)
        .or_else(|| test_tool_redirect(&inner, preferred_pwsh))
        .or_else(|| select_string_redirect(&inner))
        .or_else(|| get_child_item_redirect(&inner))
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
    let Some(without_rtk) = strip_rtk_prefix(command) else {
        return false;
    };
    if tokenize(without_rtk)
        .first()
        .is_none_or(|token| command_name(&token.text) != "pwsh")
    {
        return false;
    }

    inner_powershell_command(without_rtk).is_some_and(|inner| {
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

fn content_redirect(command: &str) -> Option<String> {
    let tokens = tokenize(command);
    if !tokens
        .iter()
        .any(|token| command_name(&token.text) == "get-content")
    {
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
            .position(|token| command_name(&token.text) == "get-content")?;
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

fn test_tool_redirect(inner: &str, preferred_pwsh: bool) -> Option<String> {
    if preferred_pwsh {
        return None;
    }

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
                "rtk pwsh -NoProfile -Command {}",
                quote_powershell_command_arg(&format!("{setup}{rtk_tool}"))
            ));
        }
        return Some(rtk_tool);
    }
    None
}

fn find_test_tool(tokens: &[Token]) -> Option<(usize, String)> {
    for (index, token) in tokens.iter().enumerate() {
        let name = command_name(&token.text);
        if matches!(name.as_str(), "busted" | "luacheck") {
            return Some((index, name));
        }
        if name == "rtk"
            && let Some(next) = tokens.get(index + 1)
        {
            let next_name = command_name(&next.text);
            if matches!(next_name.as_str(), "busted" | "luacheck") {
                return Some((index + 1, next_name));
            }
        }
    }
    None
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

fn select_string_redirect(inner: &str) -> Option<String> {
    if !tokenize(inner)
        .iter()
        .any(|token| command_name(&token.text) == "select-string")
    {
        return None;
    }
    let path = path_argument(inner, &["-Path"])?;
    let pattern = path_argument(inner, &["-Pattern"])?;
    let mut suggestion = "rtk rg -n".to_string();
    if let Some(context) = context_number(inner) {
        suggestion.push_str(&format!(" -C {context}"));
    }
    suggestion.push_str(&format!(
        " {} {}",
        quote_pattern(&pattern),
        quote_arg(&path)
    ));
    Some(suggestion)
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
    if !tokenize(inner)
        .iter()
        .any(|token| command_name(&token.text) == "get-childitem")
    {
        return None;
    }
    let path = path_argument(inner, &["-LiteralPath", "-Path"]).unwrap_or_else(|| ".".to_string());
    Some(format!("rtk rg --files {}", quote_arg(&path)))
}

fn rg_redirect(command: &str) -> Option<String> {
    let tokens = tokenize(command);
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
    if args.iter().any(|arg| arg == "--files") {
        return Some(format!(
            "rtk rg {}",
            args.iter()
                .map(|arg| quote_arg(arg))
                .collect::<Vec<_>>()
                .join(" ")
        ));
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

    let mut options = Vec::new();
    let mut index = 0;
    while index < args.len() && args[index].starts_with('-') {
        options.push(args[index].clone());
        if value_options.contains(&args[index].as_str()) && index + 1 < args.len() {
            index += 1;
            options.push(args[index].clone());
        }
        index += 1;
    }
    if index >= args.len() {
        return Some(format!("rtk rg {}", args.join(" ")));
    }

    let path_start = args
        .iter()
        .enumerate()
        .skip(index + 1)
        .find_map(|(idx, arg)| is_search_path_token(arg).then_some(idx))
        .unwrap_or(args.len());

    let pattern = args[index..path_start].join(" ").replace("\\|", "|");
    let mut parts = vec!["rtk".to_string(), "rg".to_string()];
    parts.extend(options);
    parts.push(quote_pattern(&pattern));
    parts.extend(args[path_start..].iter().map(|arg| quote_arg(arg)));
    Some(parts.join(" "))
}

fn common_rewrite(command: &str) -> Option<String> {
    let tokens = tokenize(command);
    let first = tokens.first().map(|token| command_name(&token.text));
    match first.as_deref() {
        Some("git" | "cargo" | "gh" | "npm" | "pytest" | "busted" | "luacheck") => {
            Some(format!("rtk {command}"))
        }
        _ => None,
    }
}

fn rtk_rewrite(command: &str) -> Option<String> {
    if starts_with_rtk(command) {
        return None;
    }
    let output = Command::new("rtk")
        .arg("rewrite")
        .arg(command)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let rewrite = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if rewrite.is_empty() || rewrite == command {
        None
    } else {
        Some(rewrite)
    }
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
                if c.is_whitespace() || c == ';' {
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
        || token.eq_ignore_ascii_case("README.md")
        || token.eq_ignore_ascii_case("AGENTS.md")
        || token.rsplit('.').next().is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "rs" | "md" | "txt" | "toml" | "yml" | "yaml" | "json"
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
    format!("\"{}\"", value.replace('"', "\\\""))
}

fn quote_powershell_command_arg(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_keeps_quotes_and_pipes_inside_words() {
        assert_eq!(
            tokenize(r#"rg -n "foo bar" foo|bar src"#)
                .into_iter()
                .map(|token| token.text)
                .collect::<Vec<_>>(),
            vec!["rg", "-n", "foo bar", "foo|bar", "src"]
        );
    }
}
