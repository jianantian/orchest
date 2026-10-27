//! Permission policy: the single decision point for whether a tool call runs,
//! asks the host, or is refused.
//!
//! The host supplies the rules ([`PermissionPolicy`]); Orchest supplies the
//! matcher and the decision ([`RuntimeConfig::decide`]). Rules select calls
//! by tool name, command pattern, path pattern and [`ToolEffect`]s, all as
//! globs; the last rule that matches a call wins. A call no rule matches
//! falls back to [`PermissionPolicy::unmatched`] when set, otherwise to the
//! tool's own declaration, under which a tool with unknown effects asks.
//!
//! The decision is made in-process, so on its own it is a soft guarantee;
//! see [`EnforcementFacts`](super::sandbox::EnforcementFacts).
//!
//! [`RuntimeConfig::decide`]: crate::run::RuntimeConfig::decide

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{ToolEffect, ToolEffects};

/// What a rule does with the calls it matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionAction {
    Allow,
    Ask,
    Deny,
}

/// One host rule. Every selector that is set must match; an unset selector
/// matches everything.
///
/// Selectors that look at several values (the segments of a command chain,
/// several paths, several effects) are strict for `Allow` and loose for
/// `Ask` / `Deny`: an allow rule matches only if every value matches, an ask
/// or deny rule matches if any value does. So `allow command "git status*"`
/// does not let `git status && rm -rf .` through, and `deny path "/etc/**"`
/// catches a call that touches `/etc` among other paths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionRule {
    pub action: PermissionAction,
    /// Glob over the tool name. Defaults to `*`.
    #[serde(default = "any_tool")]
    pub tool: String,
    /// Glob over each command of the call (see [`PermissionSubject`]); `*`
    /// matches any text. A rule with a command selector does not match a call
    /// without a command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Glob over each normalized path of the call; `*` stays within one path
    /// segment, `**` crosses segments. A rule with a path selector does not
    /// match a call without a path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Effects the rule is about. An allow rule matches a tool whose declared
    /// effects are all in this set; an ask or deny rule matches a tool that
    /// has any of them, or whose effects are unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects: Option<BTreeSet<ToolEffect>>,
    /// Shown to the model and the host when the rule denies a call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn any_tool() -> String {
    "*".into()
}

impl PermissionRule {
    pub fn new(action: PermissionAction, tool: impl Into<String>) -> Self {
        Self {
            action,
            tool: tool.into(),
            command: None,
            path: None,
            effects: None,
            reason: None,
        }
    }

    pub fn allow(tool: impl Into<String>) -> Self {
        Self::new(PermissionAction::Allow, tool)
    }

    pub fn ask(tool: impl Into<String>) -> Self {
        Self::new(PermissionAction::Ask, tool)
    }

    pub fn deny(tool: impl Into<String>) -> Self {
        Self::new(PermissionAction::Deny, tool)
    }

    pub fn command(mut self, pattern: impl Into<String>) -> Self {
        self.command = Some(pattern.into());
        self
    }

    pub fn path(mut self, pattern: impl Into<String>) -> Self {
        self.path = Some(pattern.into());
        self
    }

    pub fn effects(mut self, effects: impl IntoIterator<Item = ToolEffect>) -> Self {
        self.effects = Some(effects.into_iter().collect());
        self
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    fn matches(&self, call: &PermissionCall<'_>, workspace: Option<&Path>) -> bool {
        let strict = self.action == PermissionAction::Allow;
        if !glob_match(&self.tool, call.tool_name, None) {
            return false;
        }
        if let Some(pattern) = &self.command {
            let segments: Vec<String> = call
                .subject
                .commands
                .iter()
                .flat_map(|c| command_segments(c))
                .collect();
            if segments.is_empty() {
                return false;
            }
            if strict && call.subject.commands.iter().any(|c| has_substitution(c)) {
                return false;
            }
            if !select(strict, &segments, |s| glob_match(pattern, s, None)) {
                return false;
            }
        }
        if let Some(pattern) = &self.path {
            let paths: Vec<String> = call
                .subject
                .paths
                .iter()
                .map(|p| normalize_path(p, workspace))
                .collect();
            if paths.is_empty() || !select(strict, &paths, |p| glob_match(pattern, p, Some('/'))) {
                return false;
            }
        }
        if let Some(selected) = &self.effects {
            let selected = ToolEffects::of(selected.iter().copied());
            let matched = if call.effects.is_unknown() {
                !strict
            } else if strict {
                call.effects.is_within(&selected)
            } else {
                call.effects.overlaps(&selected)
            };
            if !matched {
                return false;
            }
        }
        true
    }
}

fn select<T>(strict: bool, values: &[T], pred: impl Fn(&T) -> bool) -> bool {
    if strict {
        values.iter().all(pred)
    } else {
        values.iter().any(pred)
    }
}

/// The host's rules for a run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PermissionPolicy {
    /// Evaluated in order; the last matching rule wins.
    #[serde(default)]
    pub rules: Vec<PermissionRule>,
    /// Relative paths in tool input are resolved against this directory
    /// before path rules are matched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<PathBuf>,
    /// The action for calls no rule matches. `None` defers to the tool's own
    /// declaration (its [`Approval`](super::Approval) and effects).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unmatched: Option<PermissionAction>,
}

impl PermissionPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rule(mut self, rule: PermissionRule) -> Self {
        self.rules.push(rule);
        self
    }

    pub fn workspace(mut self, dir: impl Into<PathBuf>) -> Self {
        self.workspace = Some(dir.into());
        self
    }

    pub fn unmatched(mut self, action: PermissionAction) -> Self {
        self.unmatched = Some(action);
        self
    }

    /// Whether the policy can ever change a call's outcome.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.unmatched.is_none()
    }

    /// The last rule matching `call`, with its index.
    pub fn evaluate(&self, call: &PermissionCall<'_>) -> Option<(usize, &PermissionRule)> {
        self.rules
            .iter()
            .enumerate()
            .rev()
            .find(|(_, rule)| rule.matches(call, self.workspace.as_deref()))
    }
}

/// The commands and paths a call acts on, as the permission matcher sees
/// them. [`Tool::permission_subject`](super::Tool::permission_subject)
/// extracts it from the input; the default reads well-known keys.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionSubject {
    pub commands: Vec<String>,
    pub paths: Vec<String>,
}

const COMMAND_KEYS: &[&str] = &["command", "cmd", "script"];
const PATH_KEYS: &[&str] = &[
    "path",
    "paths",
    "file_path",
    "file",
    "files",
    "directory",
    "dir",
    "cwd",
    "source",
    "destination",
    "target",
];

impl PermissionSubject {
    /// Reads string (or string array) values under the keys `command`, `cmd`,
    /// `script` as commands, and under `path`, `paths`, `file_path`, `file`,
    /// `files`, `directory`, `dir`, `cwd`, `source`, `destination`, `target`
    /// as paths. An array under a command key is one command, joined by
    /// spaces.
    pub fn from_input(input: &Value) -> Self {
        let Some(object) = input.as_object() else {
            return Self::default();
        };
        let mut subject = Self::default();
        for key in COMMAND_KEYS {
            match object.get(*key) {
                Some(Value::String(command)) => subject.commands.push(command.clone()),
                Some(Value::Array(parts)) => {
                    let parts: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
                    if !parts.is_empty() {
                        subject.commands.push(parts.join(" "));
                    }
                }
                _ => {}
            }
        }
        for key in PATH_KEYS {
            match object.get(*key) {
                Some(Value::String(path)) => subject.paths.push(path.clone()),
                Some(Value::Array(paths)) => subject
                    .paths
                    .extend(paths.iter().filter_map(Value::as_str).map(str::to_string)),
                _ => {}
            }
        }
        subject
    }
}

/// A tool call as the decision point sees it.
#[derive(Debug, Clone)]
pub struct PermissionCall<'a> {
    pub tool_name: &'a str,
    pub input: &'a Value,
    pub effects: &'a ToolEffects,
    pub subject: PermissionSubject,
}

/// Why the decision point decided what it did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
#[non_exhaustive]
pub enum DecisionBasis {
    /// The host rule at this index in [`PermissionPolicy::rules`].
    Rule { index: usize },
    /// An earlier session-scoped approval of the same call.
    SessionGrant,
    /// [`PermissionPolicy::unmatched`].
    Unmatched,
    /// The tool's own approval declaration and effects.
    ToolDeclaration,
    /// The run's [`ApprovalMode`](crate::run::ApprovalMode).
    ApprovalMode,
    /// The run's custom approval predicate.
    CustomPredicate,
}

/// The outcome of the decision point for one tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionDecision {
    Allow {
        basis: DecisionBasis,
    },
    /// Ask the host through the request channel before running.
    Ask {
        basis: DecisionBasis,
    },
    Deny {
        basis: DecisionBasis,
        reason: String,
    },
}

impl PermissionDecision {
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }
}

/// The key a session-scoped approval is remembered under: the tool name and
/// a digest of the call's input with object keys sorted. The input itself is
/// not kept, so the key can be logged.
pub fn grant_key(tool_name: &str, input: &Value) -> String {
    let mut canonical = String::new();
    write_canonical(input, &mut canonical);
    let digest = Sha256::digest(canonical.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("{tool_name}:{hex}")
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                write_canonical(&map[key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// Splits a shell command line into the commands of its chain (`&&`, `||`,
/// `;`, `|`, newlines), each trimmed with runs of whitespace collapsed.
fn command_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        let split = match c {
            ';' | '\n' => true,
            '&' if chars.peek() == Some(&'&') => {
                chars.next();
                true
            }
            '|' => {
                if chars.peek() == Some(&'|') {
                    chars.next();
                }
                true
            }
            _ => false,
        };
        if split {
            segments.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    segments.push(current);
    segments
        .into_iter()
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Command substitution hides commands from the matcher, so an allow rule
/// never matches a command that uses it.
fn has_substitution(command: &str) -> bool {
    command.contains("$(") || command.contains('`')
}

/// Resolves `path` against `workspace` when relative and removes `.` and
/// `..` lexically (without touching the file system).
fn normalize_path(path: &str, workspace: Option<&Path>) -> String {
    let raw = Path::new(path);
    let joined = match workspace {
        Some(base) if raw.is_relative() => base.join(raw),
        _ => raw.to_path_buf(),
    };
    let mut parts: Vec<String> = Vec::new();
    let mut absolute = false;
    for component in joined.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => absolute = true,
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.last().is_some_and(|p| p != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..".into());
                }
            }
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Literal(char),
    /// Any one character (except the separator, when there is one).
    One,
    /// Any run of characters (except the separator, when there is one).
    Star,
    /// Any run of characters.
    DoubleStar,
}

/// Glob match: `?` one character, `*` any run, `**` any run including
/// `separator`. Without a separator `*` and `**` are the same.
fn glob_match(pattern: &str, text: &str, separator: Option<char>) -> bool {
    let mut tokens = Vec::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        tokens.push(match c {
            '?' => Token::One,
            '*' if chars.peek() == Some(&'*') => {
                while chars.peek() == Some(&'*') {
                    chars.next();
                }
                Token::DoubleStar
            }
            '*' => Token::Star,
            other => Token::Literal(other),
        });
    }
    let text: Vec<char> = text.chars().collect();
    let crosses = |c: char| separator != Some(c);
    // matched[j]: tokens[..i] match text[..j].
    let mut matched = vec![false; text.len() + 1];
    matched[0] = true;
    for token in tokens {
        let mut next = vec![false; text.len() + 1];
        match token {
            Token::Star | Token::DoubleStar => {
                let any = token == Token::DoubleStar;
                for j in 0..=text.len() {
                    next[j] = matched[j] || (j > 0 && next[j - 1] && (any || crosses(text[j - 1])));
                }
            }
            Token::One => {
                for j in 1..=text.len() {
                    next[j] = matched[j - 1] && crosses(text[j - 1]);
                }
            }
            Token::Literal(c) => {
                for j in 1..=text.len() {
                    next[j] = matched[j - 1] && text[j - 1] == c;
                }
            }
        }
        matched = next;
    }
    matched[text.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call<'a>(tool: &'a str, input: &'a Value, effects: &'a ToolEffects) -> PermissionCall<'a> {
        PermissionCall {
            tool_name: tool,
            input,
            effects,
            subject: PermissionSubject::from_input(input),
        }
    }

    #[test]
    fn globs() {
        assert!(glob_match("*", "anything at all", None));
        assert!(glob_match("git status*", "git status --short", None));
        assert!(!glob_match("git status*", "git push", None));
        assert!(glob_match("/work/*.rs", "/work/main.rs", Some('/')));
        assert!(!glob_match("/work/*.rs", "/work/src/main.rs", Some('/')));
        assert!(glob_match("/work/**", "/work/src/main.rs", Some('/')));
        assert!(glob_match("mcp_?", "mcp_a", None));
    }

    #[test]
    fn command_chains_and_paths_are_normalized() {
        assert_eq!(
            command_segments("git  status && rm -rf . ; ls | wc -l || true"),
            vec!["git status", "rm -rf .", "ls", "wc -l", "true"]
        );
        assert_eq!(
            normalize_path("src/../Cargo.toml", Some(Path::new("/work"))),
            "/work/Cargo.toml"
        );
        assert_eq!(normalize_path("/a/./b/../../etc", None), "/etc");
        assert_eq!(normalize_path("../x", None), "../x");
    }

    #[test]
    fn tool_rule_last_match_wins() {
        let policy = PermissionPolicy::new()
            .rule(PermissionRule::deny("*"))
            .rule(PermissionRule::allow("read_*"));
        let effects = ToolEffects::UNKNOWN;
        let input = json!({});
        assert_eq!(
            policy
                .evaluate(&call("read_file", &input, &effects))
                .unwrap()
                .0,
            1
        );
        assert_eq!(
            policy.evaluate(&call("shell", &input, &effects)).unwrap().0,
            0
        );
    }

    #[test]
    fn command_rule_allow_is_strict_and_deny_is_loose() {
        let policy = PermissionPolicy::new()
            .rule(PermissionRule::allow("shell").command("git status*"))
            .rule(PermissionRule::deny("shell").command("rm *"));
        let effects = ToolEffects::of([ToolEffect::Process]);
        let ok = json!({"command": "git status --short"});
        let chained = json!({"command": "git status && rm -rf ."});
        let hidden = json!({"command": "git status $(curl evil)"});
        let other = json!({"command": "ls"});
        let action = |input: &Value| {
            policy
                .evaluate(&call("shell", input, &effects))
                .map(|(_, r)| r.action)
        };
        assert_eq!(action(&ok), Some(PermissionAction::Allow));
        assert_eq!(action(&chained), Some(PermissionAction::Deny));
        assert_eq!(action(&hidden), None);
        assert_eq!(action(&other), None);
    }

    #[test]
    fn path_rule_resolves_against_the_workspace() {
        let policy = PermissionPolicy::new()
            .workspace("/work")
            .rule(PermissionRule::allow("write_file").path("/work/**"))
            .rule(PermissionRule::deny("*").path("/etc/**"));
        let effects = ToolEffects::of([ToolEffect::WriteWorkspace]);
        let inside = json!({"path": "src/lib.rs"});
        let escape = json!({"path": "../../etc/passwd"});
        let mixed = json!({"paths": ["src/a.rs", "/tmp/b"]});
        let action = |tool: &str, input: &Value| {
            policy
                .evaluate(&call(tool, input, &effects))
                .map(|(_, r)| r.action)
        };
        assert_eq!(action("write_file", &inside), Some(PermissionAction::Allow));
        assert_eq!(action("write_file", &escape), Some(PermissionAction::Deny));
        assert_eq!(action("write_file", &mixed), None);
    }

    #[test]
    fn effect_rule_allows_only_declared_subsets() {
        let policy = PermissionPolicy::new()
            .rule(PermissionRule::allow("*").effects([ToolEffect::Read]))
            .rule(PermissionRule::ask("*").effects([ToolEffect::Destructive]));
        let input = json!({});
        let action = |effects: &ToolEffects| {
            policy
                .evaluate(&call("t", &input, effects))
                .map(|(_, r)| r.action)
        };
        assert_eq!(
            action(&ToolEffects::READ_ONLY),
            Some(PermissionAction::Allow)
        );
        assert_eq!(
            action(&ToolEffects::of([
                ToolEffect::Read,
                ToolEffect::Destructive
            ])),
            Some(PermissionAction::Ask)
        );
        assert_eq!(action(&ToolEffects::UNKNOWN), Some(PermissionAction::Ask));
        assert_eq!(action(&ToolEffects::of([ToolEffect::Network])), None);
    }

    #[test]
    fn grant_key_ignores_key_order_and_hides_values() {
        let a = grant_key("t", &json!({"x": 1, "y": {"b": 2, "a": "secret"}}));
        let b = grant_key("t", &json!({"y": {"a": "secret", "b": 2}, "x": 1}));
        assert_eq!(a, b);
        assert!(!a.contains("secret"));
        assert_ne!(a, grant_key("t", &json!({"x": 2})));
        assert_ne!(
            a,
            grant_key("u", &json!({"x": 1, "y": {"b": 2, "a": "secret"}}))
        );
    }

    #[test]
    fn policy_round_trips_through_json() {
        let policy = PermissionPolicy::new()
            .unmatched(PermissionAction::Ask)
            .rule(
                PermissionRule::deny("shell")
                    .command("rm *")
                    .reason("no deletes"),
            );
        let value = serde_json::to_value(&policy).unwrap();
        assert_eq!(
            value,
            json!({
                "rules": [{"action": "deny", "tool": "shell", "command": "rm *", "reason": "no deletes"}],
                "unmatched": "ask"
            })
        );
        let back: PermissionPolicy = serde_json::from_value(value).unwrap();
        assert_eq!(back, policy);
    }
}
