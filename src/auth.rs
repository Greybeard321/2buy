#[derive(Clone, Debug)]
pub struct AuthPath {
    pub username: String,
    pub token: String,
    pub tail: Vec<String>,
}

/// Parse `/u/<username>/<token>[/...]` and `/api/<username>/<token>[/...]`.
/// Returns `(is_api, auth_path)`.
pub fn parse_path(path: &str) -> Option<(bool, AuthPath)> {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 3 {
        return None;
    }

    let is_api = match parts[0] {
        "u" => false,
        "api" => true,
        _ => return None,
    };

    let username = parts[1];
    let token = parts[2];
    if !valid_username(username) || !valid_token(token) {
        return None;
    }

    Some((
        is_api,
        AuthPath {
            username: username.to_string(),
            token: token.to_string(),
            tail: parts[3..].iter().map(|s| s.to_string()).collect(),
        },
    ))
}

pub fn valid_username(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 {
        return false;
    }
    let first_ok = bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit();
    first_ok
        && bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-' || *b == b'_'
        })
}

fn valid_token(value: &str) -> bool {
    let bytes = value.as_bytes();
    (20..=100).contains(&bytes.len())
        && bytes.iter().all(|b| {
            b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_'
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_rules() {
        assert!(valid_username("alice"));
        assert!(valid_username("a1_b-2"));
        assert!(!valid_username("Alice"));
        assert!(!valid_username("1a!"));
        assert!(!valid_username(""));
        assert!(!valid_username(&"a".repeat(33)));
    }

    #[test]
    fn parses_both_path_kinds() {
        let (api, p) = parse_path("/u/alice/abcDEF1234567890xyzABC/").unwrap();
        assert!(!api);
        assert_eq!(p.username, "alice");
        assert!(p.tail.is_empty());

        let (api, p) = parse_path("/api/bob/abcDEF1234567890xyzABC/items/42").unwrap();
        assert!(api);
        assert_eq!(p.username, "bob");
        assert_eq!(p.tail, vec!["items", "42"]);
    }

    #[test]
    fn rejects_bad_paths() {
        assert!(parse_path("/u/Alice/abcDEF1234567890xyzABC").is_none());
        assert!(parse_path("/u/alice/short").is_none());
        assert!(parse_path("/other/alice/abcDEF1234567890xyzABC").is_none());
    }
}
