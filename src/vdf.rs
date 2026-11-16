use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawLoginUser {
    pub steam_id64: String,
    pub account_name: String,
    pub persona_name: String,
    pub remember_password: bool,
    pub auto_login: bool,
    pub timestamp: String,
}

/// Unescapes standard Valve KeyValues string escapes (`\\`, `\"`, `\n`, `\t`, `\r`).
pub fn unescape_vdf(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn parse_quoted_token(s: &str) -> Option<(String, &str)> {
    let rest = s.strip_prefix('"')?;
    let mut escaped = false;
    for (idx, ch) in rest.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            let raw = &rest[..idx];
            return Some((unescape_vdf(raw), &rest[idx + 1..]));
        }
    }
    None
}

/// Parses `"key"   "value"` from a trimmed VDF line (supporting empty values and escaped quotes/backslashes).
fn parse_kv_line(trimmed: &str) -> Option<(String, String)> {
    let (key, after_key) = parse_quoted_token(trimmed)?;
    if key.is_empty() {
        return None;
    }
    let after_ws = after_key.trim_start();
    if after_ws.len() == after_key.len() {
        // Require at least one whitespace character between key and value
        return None;
    }
    let (val, remainder) = parse_quoted_token(after_ws)?;
    let rem = remainder.trim_start();
    if !rem.is_empty() && !rem.starts_with("//") {
        return None;
    }
    Some((key, val))
}

/// Checks if a trimmed line opens a 15..=25 digit SteamID64 user block.
fn parse_steam_id_header(trimmed: &str) -> Option<(&str, bool)> {
    let rest = trimmed.strip_prefix('"')?;
    let end = rest.find('"')?;
    let id = &rest[..end];
    if !(15..=25).contains(&id.len()) || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let trailing = rest[end + 1..].trim();
    if trailing.is_empty() {
        Some((id, false))
    } else if trailing == "{" {
        Some((id, true))
    } else {
        None
    }
}

/// Extracts Steam library paths from `libraryfolders.vdf`.
pub fn parse_library_folders(content: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in content.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)) {
        let trimmed = line.trim();
        if let Some((key, val)) = parse_kv_line(trimmed)
            && key.eq_ignore_ascii_case("path")
            && !val.is_empty()
        {
            paths.push(PathBuf::from(val));
        }
    }
    paths
}

/// Parses accounts from Steam's `loginusers.vdf`.
pub fn parse_login_users(content: &str) -> Vec<RawLoginUser> {
    let mut accounts = Vec::new();
    let mut current: Option<RawLoginUser> = None;
    let mut depth: usize = 0;
    let mut awaiting_open_brace = false;

    for raw_line in content.split('\n') {
        let trimmed = raw_line.strip_suffix('\r').unwrap_or(raw_line).trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(ref mut user) = current {
            if awaiting_open_brace && trimmed == "{" {
                depth = 1;
                awaiting_open_brace = false;
                continue;
            }
            if trimmed == "{" {
                depth += 1;
                continue;
            }
            if trimmed == "}" {
                if depth > 1 {
                    depth -= 1;
                    continue;
                }
                let finished = current.take().unwrap();
                if !finished.account_name.is_empty() {
                    accounts.push(finished);
                }
                depth = 0;
                awaiting_open_brace = false;
                continue;
            }

            if let Some((key, val)) = parse_kv_line(trimmed) {
                match key.to_ascii_lowercase().as_str() {
                    "accountname" => user.account_name = val,
                    "personaname" => user.persona_name = val,
                    "rememberpassword" => user.remember_password = val == "1",
                    "autologin" => user.auto_login = val == "1",
                    "mostrecent" if !user.auto_login => user.auto_login = val == "1",
                    "timestamp" => user.timestamp = val,
                    _ => {}
                }
            }
            continue;
        }

        if trimmed.eq_ignore_ascii_case("\"users\"") || trimmed == "{" || trimmed == "}" {
            continue;
        }

        if let Some((steam_id64, inline_brace)) = parse_steam_id_header(trimmed) {
            current = Some(RawLoginUser {
                steam_id64: steam_id64.to_string(),
                account_name: String::new(),
                persona_name: String::new(),
                remember_password: false,
                auto_login: false,
                timestamp: "0".to_string(),
            });
            depth = if inline_brace { 1 } else { 0 };
            awaiting_open_brace = !inline_brace;
        }
    }

    accounts
}

fn count_braces_outside_quotes(line: &str) -> (usize, usize) {
    let mut open = 0usize;
    let mut close = 0usize;
    let mut in_quotes = false;
    let mut escaped = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_quotes = false;
            }
        } else {
            match ch {
                '"' => in_quotes = true,
                '{' => open += 1,
                '}' => close += 1,
                '/' if chars.peek() == Some(&'/') => break,
                _ => {}
            }
        }
    }
    (open, close)
}

fn flush_user_block(
    block_lines: &[String],
    block_account: Option<&str>,
    target_account_name: &str,
    uses_most_recent: bool,
    out_lines: &mut Vec<String>,
) {
    let Some(account_name) = block_account else {
        out_lines.extend(block_lines.iter().cloned());
        return;
    };

    let is_target = account_name.eq_ignore_ascii_case(target_account_name);
    let flag = if is_target { "1" } else { "0" };

    let default_indent = block_lines
        .iter()
        .find_map(|l| {
            let t = l.trim();
            parse_kv_line(t)?;
            let indent_len = l.len() - l.trim_start().len();
            Some(&l[..indent_len])
        })
        .unwrap_or("\t\t");

    let mut saw_autologin = false;
    let mut saw_mostrecent = false;
    let mut saw_remember = false;
    let mut saw_wants_offline = false;

    for line in block_lines {
        let trimmed = line.trim();
        if let Some((key, _)) = parse_kv_line(trimmed) {
            let indent_len = line.len() - line.trim_start().len();
            let indent = &line[..indent_len];
            match key.to_ascii_lowercase().as_str() {
                "autologin" => {
                    saw_autologin = true;
                    out_lines.push(format!("{indent}\"{key}\"\t\t\"{flag}\""));
                    continue;
                }
                "allowautologin" => {
                    out_lines.push(format!("{indent}\"{key}\"\t\t\"{flag}\""));
                    continue;
                }
                "mostrecent" => {
                    saw_mostrecent = true;
                    out_lines.push(format!("{indent}\"{key}\"\t\t\"{flag}\""));
                    continue;
                }
                "rememberpassword" if is_target => {
                    saw_remember = true;
                    out_lines.push(format!("{indent}\"{key}\"\t\t\"1\""));
                    continue;
                }
                "wantsofflinemode" if is_target => {
                    saw_wants_offline = true;
                    out_lines.push(format!("{indent}\"{key}\"\t\t\"0\""));
                    continue;
                }
                _ => {}
            }
        }
        out_lines.push(line.to_string());
    }

    if !saw_autologin {
        out_lines.push(format!("{default_indent}\"AutoLogin\"\t\t\"{flag}\""));
    }
    if uses_most_recent && !saw_mostrecent {
        out_lines.push(format!("{default_indent}\"MostRecent\"\t\t\"{flag}\""));
    }
    if is_target && !saw_remember {
        out_lines.push(format!("{default_indent}\"RememberPassword\"\t\t\"1\""));
    }
    if is_target && !saw_wants_offline {
        out_lines.push(format!("{default_indent}\"WantsOfflineMode\"\t\t\"0\""));
    }
}

/// Rewrites `AutoLogin` (and `MostRecent` / `AllowAutoLogin` / target `RememberPassword` &
/// `WantsOfflineMode`) in `loginusers.vdf` so only `target_account_name` logs in automatically.
/// Preserves original line endings (`\r\n` vs `\n`), trailing newline, key order, and
/// handles `AccountName` appearing anywhere inside a user block.
pub fn rewrite_login_users_autologin(content: &str, target_account_name: &str) -> String {
    if content.is_empty() {
        return String::new();
    }

    let line_ending = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let has_trailing_newline = content.ends_with('\n');
    let body = content
        .strip_suffix("\r\n")
        .or_else(|| content.strip_suffix('\n'))
        .unwrap_or(content);

    let raw_lines: Vec<&str> = body
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();

    let uses_most_recent = raw_lines.iter().any(|l| {
        parse_kv_line(l.trim()).is_some_and(|(k, _)| k.eq_ignore_ascii_case("MostRecent"))
    });

    let mut out_lines: Vec<String> = Vec::with_capacity(raw_lines.len() + 8);
    let mut depth: usize = 0;
    let mut in_user_block = false;
    let mut block_lines: Vec<String> = Vec::new();
    let mut block_account: Option<String> = None;

    for line in raw_lines {
        let trimmed = line.trim();
        let (open_braces, close_braces) = count_braces_outside_quotes(trimmed);
        let prev_depth = depth;
        let next_depth = (prev_depth + open_braces).saturating_sub(close_braces);

        if !in_user_block {
            out_lines.push(line.to_string());
            if prev_depth == 1 && next_depth >= 2 {
                in_user_block = true;
                block_lines.clear();
                block_account = None;
            }
        } else if next_depth <= 1 {
            flush_user_block(
                &block_lines,
                block_account.as_deref(),
                target_account_name,
                uses_most_recent,
                &mut out_lines,
            );
            out_lines.push(line.to_string());
            in_user_block = false;
            block_lines.clear();
            block_account = None;
        } else {
            if prev_depth == 2
                && let Some((key, val)) = parse_kv_line(trimmed)
                && key.eq_ignore_ascii_case("AccountName")
            {
                block_account = Some(val);
            }
            block_lines.push(line.to_string());
        }

        depth = next_depth;
    }

    if in_user_block {
        flush_user_block(
            &block_lines,
            block_account.as_deref(),
            target_account_name,
            uses_most_recent,
            &mut out_lines,
        );
    }

    let mut result = out_lines.join(line_ending);
    if has_trailing_newline {
        result.push_str(line_ending);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_LOGIN_USERS: &str = "\"users\"\r\n\
{\r\n\
\t\"76561198000000001\"\r\n\
\t{\r\n\
\t\t\"AccountName\"\t\t\"main_acc\"\r\n\
\t\t\"PersonaName\"\t\t\"Yuēhàn 约翰\"\r\n\
\t\t\"RememberPassword\"\t\t\"1\"\r\n\
\t\t\"AutoLogin\"\t\t\"1\"\r\n\
\t\t\"Timestamp\"\t\t\"1700000001\"\r\n\
\t}\r\n\
\t\"76561198000000002\"\r\n\
\t{\r\n\
\t\t\"AccountName\"\t\t\"alt_acc\"\r\n\
\t\t\"PersonaName\"\t\t\"Smurf\"\r\n\
\t\t\"RememberPassword\"\t\t\"1\"\r\n\
\t\t\"AutoLogin\"\t\t\"0\"\r\n\
\t\t\"Timestamp\"\t\t\"1700000002\"\r\n\
\t}\r\n\
}\r\n";

    #[test]
    fn parses_login_users_preserving_unicode() {
        let users = parse_login_users(SAMPLE_LOGIN_USERS);
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].steam_id64, "76561198000000001");
        assert_eq!(users[0].account_name, "main_acc");
        assert_eq!(users[0].persona_name, "Yuēhàn 约翰");
        assert!(users[0].remember_password);
        assert!(users[0].auto_login);
        assert_eq!(users[1].account_name, "alt_acc");
        assert!(!users[1].auto_login);
    }

    #[test]
    fn rewrites_autologin_preserving_crlf_and_resetting_others() {
        let rewritten = rewrite_login_users_autologin(SAMPLE_LOGIN_USERS, "ALT_ACC");
        assert!(rewritten.contains("\r\n"));
        let users = parse_login_users(&rewritten);
        assert_eq!(users.len(), 2);
        assert!(!users[0].auto_login);
        assert!(users[1].auto_login);
    }

    #[test]
    fn parses_library_folders_unescaping_windows_paths() {
        let sample = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
	}
}"#;
        let libs = parse_library_folders(sample);
        assert_eq!(
            libs,
            vec![
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"D:\SteamLibrary"),
            ]
        );
    }

    #[test]
    fn rewrites_lf_only_with_lowercase_timestamp_and_most_recent() {
        let input = concat!(
            "\"users\"\n",
            "{\n",
            "\t\"76561198000000001\"\n",
            "\t{\n",
            "\t\t\"AccountName\"\t\t\"first_user\"\n",
            "\t\t\"PersonaName\"\t\t\"First\"\n",
            "\t\t\"RememberPassword\"\t\t\"1\"\n",
            "\t\t\"MostRecent\"\t\t\"1\"\n",
            "\t\t\"timestamp\"\t\t\"1700000111\"\n",
            "\t}\n",
            "\t\"76561198000000002\"\n",
            "\t{\n",
            "\t\t\"AccountName\"\t\t\"second_user\"\n",
            "\t\t\"PersonaName\"\t\t\"Second\"\n",
            "\t\t\"RememberPassword\"\t\t\"0\"\n",
            "\t\t\"WantsOfflineMode\"\t\t\"1\"\n",
            "\t\t\"timestamp\"\t\t\"1700000222\"\n",
            "\t}\n",
            "}\n",
        );

        let rewritten = rewrite_login_users_autologin(input, "second_user");
        assert!(!rewritten.contains("\r\n"), "Expected LF-only output");
        assert!(rewritten.ends_with('\n'));
        assert!(rewritten.contains("\"timestamp\"\t\t\"1700000111\""));
        assert!(rewritten.contains("\"timestamp\"\t\t\"1700000222\""));

        let parsed = parse_login_users(&rewritten);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].timestamp, "1700000111");
        assert!(!parsed[0].auto_login);
        assert_eq!(parsed[1].timestamp, "1700000222");
        assert!(parsed[1].auto_login);
        assert!(parsed[1].remember_password);
        assert!(rewritten.contains("\"MostRecent\"\t\t\"1\""));
        assert!(rewritten.contains("\"WantsOfflineMode\"\t\t\"0\""));
    }

    #[test]
    fn parses_escaped_quotes_and_rewrites_reordered_account_name() {
        let input = concat!(
            "\"users\"\n",
            "{\n",
            "\t\"76561198000000003\"\n",
            "\t{\n",
            "\t\t\"AutoLogin\"\t\t\"1\"\n",
            "\t\t\"PersonaName\"\t\t\"Quote \\\"Name\\\" \\\\ {brace}\"\n",
            "\t\t\"AccountName\"\t\t\"acc_one\"\n",
            "\t}\n",
            "\t\"76561198000000004\"\n",
            "\t{\n",
            "\t\t\"AutoLogin\"\t\t\"0\"\n",
            "\t\t\"PersonaName\"\t\t\"Plain\"\n",
            "\t\t\"AccountName\"\t\t\"acc_two\"\n",
            "\t}\n",
            "}\n",
        );

        let parsed_before = parse_login_users(input);
        assert_eq!(parsed_before.len(), 2);
        assert_eq!(parsed_before[0].persona_name, r#"Quote "Name" \ {brace}"#);

        let rewritten = rewrite_login_users_autologin(input, "acc_two");
        let parsed_after = parse_login_users(&rewritten);
        assert_eq!(parsed_after.len(), 2);
        assert!(!parsed_after[0].auto_login);
        assert!(parsed_after[1].auto_login);
    }
}
