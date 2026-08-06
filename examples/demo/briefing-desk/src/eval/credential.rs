//! Shared semantic credential detection for eval configuration preflight.
//!
//! Inputs may arrive as JSON, opaque MCP text, or URLs. Keep one bounded,
//! fail-closed scanner so every layer applies identical name/value semantics.

use serde_json::Value;

/// Maximum nesting for recursively encoded JSON/configuration strings.
const MAX_CREDENTIAL_SCAN_DEPTH: usize = 16;

/// Return whether structured configuration contains credential material.
pub fn value_has_credentials(value: &Value) -> bool {
    json_has_credentials(value, 0)
}

/// Return whether opaque command/configuration text contains credential material.
pub fn text_has_credentials(value: &str) -> bool {
    opaque_text_has_credential(value, 0)
}

/// Return whether a URL contains userinfo or credential-bearing query data.
pub fn url_has_credentials(url: &str) -> bool {
    url_has_credentials_at_depth(url, 0)
}

/// Redact credential values from opaque text while retaining safe diagnostics.
///
/// This shares the same credential-name semantics as preflight detection so an
/// error string cannot expose a secret form that configuration validation
/// would reject. It intentionally does not read the environment.
pub fn redact_credentials_in_text(value: &str, replacement: &str) -> String {
    let detector_found_credentials = text_has_credentials(value);
    let named_values_redacted = redact_named_credential_values(value, replacement);
    let bearer_values_redacted = redact_bearer_values(&named_values_redacted, replacement);
    let redacted = redact_url_userinfo(&bearer_values_redacted, replacement);

    // Keep diagnostics only when the detector can prove their residual text is
    // credential-free. Encoded carriers and split CLI flags are not safely
    // span-redactable, so fail closed rather than risking an artifact leak.
    let redacted = if detector_found_credentials && text_has_credentials(&redacted) {
        "[REDACTED]".to_string()
    } else {
        redacted
    };
    debug_assert!(!detector_found_credentials || !text_has_credentials(&redacted));
    redacted
}

/// Classify a configuration key or carrier name after case/separator folding.
pub fn is_sensitive_configuration_name(name: &str) -> bool {
    let name = name.trim().trim_start_matches('-');
    is_credential_name(name) || is_credential_injection_name(name)
}

fn url_has_credentials_at_depth(url: &str, depth: usize) -> bool {
    if depth >= MAX_CREDENTIAL_SCAN_DEPTH {
        return true;
    }
    if let Some(after_scheme) = url.split("://").nth(1) {
        let host_part = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
        if host_part.contains('@') {
            return true;
        }
    }
    url.split_once('?')
        .map(|(_, query_and_fragment)| {
            query_and_fragment
                .split('#')
                .next()
                .unwrap_or_default()
                .split(['&', ';'])
                .any(|parameter| {
                    let decoded = percent_decode_query_component(parameter);
                    let (name, nested) = decoded
                        .split_once('=')
                        .map_or((decoded.as_str(), None), |(name, nested)| {
                            (name, Some(nested))
                        });
                    is_sensitive_configuration_name(name)
                        || nested
                            .is_some_and(|nested| opaque_text_has_credential(nested, depth + 1))
                })
        })
        .unwrap_or(false)
}

fn opaque_text_has_credential(value: &str, depth: usize) -> bool {
    if depth >= MAX_CREDENTIAL_SCAN_DEPTH {
        return true;
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    if has_compact_credential_flag(trimmed) || looks_like_api_key_value(trimmed) {
        return true;
    }
    if trimmed.contains("://") && url_has_credentials_at_depth(trimmed, depth + 1) {
        return true;
    }
    if let Ok(structured) = serde_json::from_str::<Value>(trimmed) {
        return json_has_credentials(&structured, depth + 1);
    }
    if let Some((name, nested)) = trimmed.split_once('=') {
        return is_sensitive_configuration_name(name)
            || opaque_text_has_credential(nested, depth + 1);
    }
    if let Some((name, nested)) = trimmed.split_once(':') {
        return is_sensitive_configuration_name(name)
            || opaque_text_has_credential(nested, depth + 1);
    }
    let mut words = trimmed.split_ascii_whitespace();
    if words.next().is_some() && words.next().is_some() {
        return trimmed
            .split_ascii_whitespace()
            .any(|word| opaque_text_has_credential(word, depth + 1));
    }
    !is_bare_short_credential_flag(trimmed) && is_sensitive_configuration_name(trimmed)
}

fn json_has_credentials(value: &Value, depth: usize) -> bool {
    if depth >= MAX_CREDENTIAL_SCAN_DEPTH {
        return true;
    }
    match value {
        Value::Object(map) => map.iter().any(|(name, nested)| {
            is_sensitive_configuration_name(name) || json_has_credentials(nested, depth + 1)
        }),
        Value::Array(items) => items
            .iter()
            .any(|nested| json_has_credentials(nested, depth + 1)),
        Value::String(nested) => opaque_text_has_credential(nested, depth + 1),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

fn has_compact_credential_flag(value: &str) -> bool {
    let mut words = value.split_ascii_whitespace();
    while let Some(word) = words.next() {
        if matches!(word, "-H" | "-b" | "-u") {
            if words.next().is_some() {
                return true;
            }
            continue;
        }
        if let Some(header) = word.strip_prefix("-H") {
            if header
                .split_once(':')
                .is_some_and(|(name, _)| !name.is_empty())
            {
                return true;
            }
        }
        if let Some(cookie) = word.strip_prefix("-b") {
            if cookie
                .split_once('=')
                .is_some_and(|(name, _)| !name.is_empty())
            {
                return true;
            }
        }
        if let Some(user_password) = word.strip_prefix("-u") {
            if user_password
                .split_once(':')
                .is_some_and(|(user, _)| !user.is_empty())
            {
                return true;
            }
        }
    }
    false
}

fn is_bare_short_credential_flag(value: &str) -> bool {
    matches!(value, "-H" | "-b" | "-u")
}

fn redact_named_credential_values(value: &str, replacement: &str) -> String {
    let mut redacted = String::with_capacity(value.len());
    let mut copied_until = 0;

    for (delimiter_index, delimiter) in value.char_indices() {
        if (delimiter != '=' && delimiter != ':') || delimiter_index < copied_until {
            continue;
        }
        let name_start = value[..delimiter_index]
            .char_indices()
            .rev()
            .find_map(|(index, character)| {
                (!is_credential_name_character(character)).then_some(index + character.len_utf8())
            })
            .unwrap_or(0);
        let name = &value[name_start..delimiter_index];
        if name.is_empty() || !is_sensitive_configuration_name(name) {
            continue;
        }

        let value_start = skip_ascii_whitespace(value, delimiter_index + delimiter.len_utf8());
        let value_end = credential_value_end(value, value_start, false);
        if value_end == value_start {
            continue;
        }
        redacted.push_str(&value[copied_until..value_start]);
        redacted.push_str(replacement);
        copied_until = value_end;
    }

    redacted.push_str(&value[copied_until..]);
    redacted
}

fn redact_bearer_values(value: &str, replacement: &str) -> String {
    let mut redacted = String::with_capacity(value.len());
    let mut copied_until = 0;
    let mut search_from = 0;

    while let Some(relative_index) = find_ascii_case_insensitive(&value[search_from..], "bearer") {
        let bearer_start = search_from + relative_index;
        let bearer_end = bearer_start + "bearer".len();
        let before_is_boundary = bearer_start == 0
            || !value[..bearer_start]
                .chars()
                .next_back()
                .is_some_and(is_credential_name_character);
        let whitespace_len = value[bearer_end..]
            .chars()
            .take_while(|character| character.is_ascii_whitespace())
            .map(char::len_utf8)
            .sum::<usize>();
        if !before_is_boundary || whitespace_len == 0 {
            search_from = bearer_end;
            continue;
        }
        let value_start = bearer_end + whitespace_len;
        let value_end = credential_value_end(value, value_start, true);
        if value_end == value_start {
            search_from = bearer_end;
            continue;
        }
        redacted.push_str(&value[copied_until..value_start]);
        redacted.push_str(replacement);
        copied_until = value_end;
        search_from = value_end;
    }

    redacted.push_str(&value[copied_until..]);
    redacted
}

fn redact_url_userinfo(value: &str, replacement: &str) -> String {
    let mut redacted = value.to_string();
    let mut search_from = 0;
    while let Some(relative_scheme) = redacted[search_from..].find("://") {
        let authority_start = search_from + relative_scheme + 3;
        let authority_end = redacted[authority_start..]
            .find(['/', '?', '#', ' ', '\t', '\r', '\n', '\"', '\''])
            .map_or(redacted.len(), |index| authority_start + index);
        if let Some(userinfo_end) = redacted[authority_start..authority_end].find('@') {
            let userinfo_end = authority_start + userinfo_end;
            redacted.replace_range(authority_start..userinfo_end, replacement);
            search_from = authority_start + replacement.len() + 1;
        } else {
            search_from = authority_end;
        }
    }
    redacted
}

fn credential_value_end(value: &str, value_start: usize, stop_at_whitespace: bool) -> usize {
    let remainder = &value[value_start..];
    if let Some(quote) = remainder
        .chars()
        .next()
        .filter(|quote| matches!(quote, '\"' | '\''))
    {
        let content_start = value_start + quote.len_utf8();
        return value[content_start..]
            .find(quote)
            .map_or(value.len(), |index| {
                content_start + index + quote.len_utf8()
            });
    }
    let end = if stop_at_whitespace {
        remainder.find([' ', '\t', '\r', '\n', ',', '&', ';', '#'])
    } else {
        remainder.find([',', '&', ';', '#'])
    };
    end.map_or(value.len(), |index| value_start + index)
}

fn skip_ascii_whitespace(value: &str, start: usize) -> usize {
    start
        + value[start..]
            .chars()
            .take_while(|character| character.is_ascii_whitespace())
            .map(char::len_utf8)
            .sum::<usize>()
}

fn find_ascii_case_insensitive(value: &str, needle: &str) -> Option<usize> {
    value.char_indices().map(|(index, _)| index).find(|&index| {
        value[index..].starts_with(needle)
            || value[index..]
                .get(..needle.len())
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(needle))
    })
}

fn is_credential_name_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}

fn is_credential_injection_name(name: &str) -> bool {
    let normalized = normalize_credential_name(name);
    let benign_header_metadata = normalized == "signedheaders"
        || (normalized.contains("header")
            && ends_with_any(&normalized, &["size", "count", "limit"]));
    let benign_cookie_metadata =
        normalized.contains("cookie") && ends_with_any(&normalized, &["policy", "mode"]);
    let benign_credential_metadata = normalized.contains("credential")
        && ends_with_any(&normalized, &["type", "kind", "scheme", "format"]);
    let benign_signature_metadata = normalized.contains("signature")
        && ends_with_any(&normalized, &["algorithm", "type", "version", "format"]);
    let auth_carrier = normalized == "auth"
        || normalized.ends_with("auth")
        || normalized.contains("authentication")
        || normalized.contains("authorization")
        || (normalized.contains("auth")
            && ends_with_any(
                &normalized,
                &[
                    "config",
                    "json",
                    "token",
                    "key",
                    "header",
                    "headers",
                    "cookie",
                    "credential",
                    "signature",
                    "bearer",
                ],
            ));
    matches!(
        normalized.as_str(),
        "h" | "b" | "u" | "user" | "userpwd" | "proxyuser" | "cookiedata"
    ) || (normalized.contains("header") && !benign_header_metadata)
        || (normalized.contains("cookie") && !benign_cookie_metadata)
        || (normalized.contains("credential") && !benign_credential_metadata)
        || (normalized.contains("signature") && !benign_signature_metadata)
        || auth_carrier
        || normalized.contains("bearer")
}

fn is_credential_name(name: &str) -> bool {
    if name.contains(['/', '\\']) {
        return false;
    }
    let normalized = normalize_credential_name(name);
    matches!(
        normalized.as_str(),
        "apikey"
            | "authorization"
            | "cookie"
            | "setcookie"
            | "xapikey"
            | "accesstoken"
            | "clientsecret"
            | "secret"
            | "password"
            | "passwd"
            | "token"
            | "bearertoken"
            | "key"
            | "auth"
            | "credential"
            | "credentials"
            | "sig"
            | "signature"
    ) || normalized.ends_with("apikey")
        || normalized.ends_with("accesstoken")
        || normalized.contains("authorization")
        || normalized.ends_with("token")
        || (normalized.ends_with("key") && !normalized.ends_with("monkey"))
        || normalized.ends_with("credential")
        || normalized.ends_with("credentials")
        || normalized.ends_with("signature")
        || normalized.ends_with("accesskeyid")
        || normalized.ends_with("secretaccesskey")
}

fn normalize_credential_name(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn ends_with_any(value: &str, suffixes: &[&str]) -> bool {
    suffixes.iter().any(|suffix| value.ends_with(suffix))
}

fn percent_decode_query_component(component: &str) -> String {
    let mut decoded = component.to_owned();
    for _ in 0..3 {
        let next = percent_decode_once(&decoded);
        if next == decoded {
            break;
        }
        decoded = next;
    }
    decoded
}

fn percent_decode_once(component: &str) -> String {
    let bytes = component.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                decoded.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        decoded.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn looks_like_api_key_value(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("sk-")
        || trimmed.starts_with("sk-ant-")
        || (trimmed.len() >= 32
            && trimmed.chars().all(|character| {
                character.is_ascii_alphanumeric() || character == '-' || character == '_'
            })
            && (trimmed.contains("sk") || trimmed.starts_with("key")))
}

#[cfg(test)]
mod tests {
    use super::{redact_credentials_in_text, text_has_credentials};

    #[test]
    fn short_credential_flags_require_a_real_value_syntax() {
        for benign_option in ["-build", "-bind-address", "-unix-socket"] {
            assert!(
                !text_has_credentials(benign_option),
                "{benign_option} is not a credential flag"
            );
        }

        for credential_flag in [
            "-b session=authenticated",
            "-u agent:password",
            "-H Authorization: Bearer token",
            "-bsession=authenticated",
            "-uagent:password",
            "-HAuthorization: Bearer token",
        ] {
            assert!(
                text_has_credentials(credential_flag),
                "{credential_flag} carries credentials"
            );
        }
    }

    #[test]
    fn redacts_split_flags_and_encoded_credential_carriers_fail_closed() {
        let canary = "CREDENTIAL-CANARY-9e97d2";
        let cases = [
            format!("curl --api-key {canary}"),
            format!("curl --client-secret {canary}"),
            format!("request failed: https://api.example.test/run?carrier=--api-key%20{canary}"),
        ];

        for input in cases {
            assert!(text_has_credentials(&input), "detector missed {input}");
            let redacted = redact_credentials_in_text(&input, "[REDACTED]");
            assert_eq!(redacted, "[REDACTED]");
            assert!(
                !text_has_credentials(&redacted),
                "detector still sees credentials in {redacted}"
            );
            assert!(!redacted.contains(canary), "canary leaked in {redacted}");
        }
    }

    #[test]
    fn redaction_preserves_benign_text() {
        let benign = "retry after timeout; config=release, service: healthy";
        assert_eq!(redact_credentials_in_text(benign, "[REDACTED]"), benign);
    }
}
