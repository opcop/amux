#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedArgs {
    pub(crate) positional: Vec<String>,
    pub(crate) flags: std::collections::HashMap<String, String>,
}

pub(crate) fn parse_shell_words(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

pub(crate) fn parse_args(raw: &str) -> ParsedArgs {
    let words = parse_shell_words(raw);
    let mut positional = Vec::new();
    let mut flags = std::collections::HashMap::new();
    let mut i = 0;
    while i < words.len() {
        let word = &words[i];
        if let Some(flag) = word.strip_prefix("--") {
            let value = words
                .get(i + 1)
                .filter(|next| !next.starts_with("--"))
                .cloned()
                .unwrap_or_else(|| "true".to_string());
            if value != "true" {
                i += 1;
            }
            flags.insert(flag.to_string(), value);
        } else {
            positional.push(word.clone());
        }
        i += 1;
    }
    ParsedArgs { positional, flags }
}

pub(crate) fn csv(value: Option<&String>) -> Vec<String> {
    value
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_flags() {
        let parsed = parse_args(r#"create "Fix auth" --description "handle 401" --tags rust,ui"#);
        assert_eq!(parsed.positional, vec!["create", "Fix auth"]);
        assert_eq!(
            parsed.flags.get("description").map(String::as_str),
            Some("handle 401")
        );
        assert_eq!(csv(parsed.flags.get("tags")), vec!["rust", "ui"]);
    }
}
