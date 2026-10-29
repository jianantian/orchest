use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::Approval;

/// One kind of effect a tool call can have outside its own return value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolEffect {
    /// Reads files or state without changing them.
    Read,
    /// Writes inside the host's workspace.
    WriteWorkspace,
    /// Writes outside the host's workspace.
    WriteOutside,
    /// Talks to the network.
    Network,
    /// Reads or uses credentials.
    Credential,
    /// Deletes or overwrites in a way that cannot be undone.
    Destructive,
    /// Starts processes.
    Process,
}

impl ToolEffect {
    pub const ALL: [ToolEffect; 7] = [
        ToolEffect::Read,
        ToolEffect::WriteWorkspace,
        ToolEffect::WriteOutside,
        ToolEffect::Network,
        ToolEffect::Credential,
        ToolEffect::Destructive,
        ToolEffect::Process,
    ];

    const fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// What a tool call may do: either a known set of [`ToolEffect`]s (empty for
/// a pure computation) or unknown. Unknown is the default and counts as
/// having side effects, so a tool that declares nothing fails closed.
///
/// The classification is a declaration by the tool author (or, for MCP
/// tools, by the server). It decides what the permission policy asks about;
/// it is not an execution boundary. Serialized as a list of effects, or
/// `null` when unknown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ToolEffects {
    known: bool,
    bits: u8,
}

impl ToolEffects {
    /// The effects are not declared.
    pub const UNKNOWN: Self = Self {
        known: false,
        bits: 0,
    };
    /// A pure computation: no effects.
    pub const NONE: Self = Self {
        known: true,
        bits: 0,
    };
    /// Only reads.
    pub const READ_ONLY: Self = Self::NONE.with(ToolEffect::Read);

    /// Adds `effect` (making the set known).
    pub const fn with(self, effect: ToolEffect) -> Self {
        Self {
            known: true,
            bits: self.bits | effect.bit(),
        }
    }

    pub fn of(effects: impl IntoIterator<Item = ToolEffect>) -> Self {
        effects.into_iter().fold(Self::NONE, Self::with)
    }

    pub fn is_unknown(&self) -> bool {
        !self.known
    }

    /// The declared effects in order, or `None` when unknown.
    pub fn effects(&self) -> Option<Vec<ToolEffect>> {
        self.known.then(|| {
            ToolEffect::ALL
                .into_iter()
                .filter(|e| self.bits & e.bit() != 0)
                .collect()
        })
    }

    pub fn contains(&self, effect: ToolEffect) -> bool {
        self.known && self.bits & effect.bit() != 0
    }

    /// Whether a call may change anything: unknown, or any effect other than
    /// [`ToolEffect::Read`].
    pub fn has_side_effects(&self) -> bool {
        !self.known || self.bits & !ToolEffect::Read.bit() != 0
    }

    /// Known and every effect is in `other`.
    pub(crate) fn is_within(&self, other: &ToolEffects) -> bool {
        self.known && self.bits & !other.bits == 0
    }

    /// Known and shares an effect with `other`.
    pub(crate) fn overlaps(&self, other: &ToolEffects) -> bool {
        self.known && self.bits & other.bits != 0
    }
}

impl Serialize for ToolEffects {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.effects().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ToolEffects {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Option::<Vec<ToolEffect>>::deserialize(deserializer)?.map_or(Self::UNKNOWN, Self::of))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    #[serde(default)]
    pub effects: ToolEffects,
    pub approval: Approval,
    #[serde(default)]
    pub execution_mode: ToolExecutionMode,
    #[serde(default)]
    pub parallelism: ToolParallelism,
    pub cost_hint: Option<CostHint>,
    pub timeout: Option<Duration>,
    pub max_output_tokens: Option<u64>,
    pub source: ToolSource,
}

impl Default for ToolMetadata {
    fn default() -> Self {
        Self {
            effects: ToolEffects::UNKNOWN,
            approval: Approval::default(),
            execution_mode: ToolExecutionMode::default(),
            parallelism: ToolParallelism::default(),
            cost_hint: None,
            timeout: None,
            max_output_tokens: None,
            source: ToolSource::InProcess,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolExecutionMode {
    #[default]
    Normal,
    Draft {
        commit_tool: String,
    },
    Commit {
        draft_tool: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolParallelism {
    #[default]
    Serial,
    ParallelSafe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ToolSource {
    InProcess,
    McpServer { server_id: String },
    Skill { skill_name: String },
    Builtin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CostHint {
    Free,
    Low,
    Medium,
    High,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_metadata_defaults_to_normal_serial() {
        let metadata = ToolMetadata::default();

        assert_eq!(metadata.execution_mode, ToolExecutionMode::Normal);
        assert_eq!(metadata.parallelism, ToolParallelism::Serial);
    }

    #[test]
    fn tool_metadata_deserializes_legacy_json_with_defaults() {
        let metadata: ToolMetadata = serde_json::from_value(json!({
            "approval": "Never",
            "cost_hint": null,
            "timeout": null,
            "max_output_tokens": null,
            "source": "InProcess"
        }))
        .expect("legacy metadata should deserialize");

        assert_eq!(metadata.execution_mode, ToolExecutionMode::Normal);
        assert_eq!(metadata.parallelism, ToolParallelism::Serial);
        assert!(metadata.effects.is_unknown());
    }

    #[test]
    fn effects_serialize_as_a_list_or_null() {
        let effects = ToolEffects::of([ToolEffect::WriteWorkspace, ToolEffect::Read]);
        assert_eq!(
            serde_json::to_value(effects).unwrap(),
            json!(["read", "write_workspace"])
        );
        assert_eq!(
            serde_json::to_value(ToolEffects::UNKNOWN).unwrap(),
            json!(null)
        );
        assert!(effects.has_side_effects());
        assert!(!ToolEffects::READ_ONLY.has_side_effects());
        assert!(!ToolEffects::NONE.has_side_effects());
        assert!(ToolEffects::UNKNOWN.has_side_effects());
        let back: ToolEffects = serde_json::from_value(json!(["write_workspace", "read"])).unwrap();
        assert_eq!(back, effects);
        let unknown: ToolEffects = serde_json::from_value(json!(null)).unwrap();
        assert!(unknown.is_unknown());
    }

    #[test]
    fn tool_execution_mode_serde_is_tagged_and_stable() {
        let mode = ToolExecutionMode::Draft {
            commit_tool: "write_file".into(),
        };

        let value = serde_json::to_value(mode).expect("serialize");

        assert_eq!(
            value,
            json!({
                "kind": "draft",
                "commit_tool": "write_file"
            })
        );
    }
}
