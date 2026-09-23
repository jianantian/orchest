//! 非 Minimax LLM provider 用来把 Minimax-only `Role` variant 降级到通用 `Role`
//! 语义,并记录 `OptionAdjustment`(对齐设计文档 §1e / 研究文档 §2.3 决策)。
//!
//! 5 个 Minimax-only role 的降级规则:
//! - `UserSystem` → `System`(设定用户角色,语义最接近系统 prompt)
//! - `Group` / `SampleMessageUser` / `SampleMessageAi` → `User`(few-shot 示例 / 分组,
//!   通用 LLM 无对应,统一塞 user 角色)
//!
//! 返回类型 `CompatibleRole` 只覆盖通用 4 角色,使下游 `match` 自动穷尽 ——
//! 编译器担保 Minimax-only role 已在此处被消解。
//!
//! Minimax(`MinimaxProfile.messages_wire_role`)直接序列化原 `Role`,**不**走此 helper。

use crate::{OptionAdjustment, Role};
use serde_json::json;

/// 通用 LLM 兼容的 4 角色子集 —— Minimax-only role 已被 `downgrade_minimax_role` 折叠。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompatibleRole {
    System,
    User,
    Assistant,
    Tool,
}

/// 把 Minimax-only `Role` 降级到 `CompatibleRole`;若发生降级,push 一条
/// `OptionAdjustment` 到 `adjustments`。
pub(crate) fn downgrade_minimax_role(
    role: Role,
    adjustments: &mut Vec<OptionAdjustment>,
) -> CompatibleRole {
    let (applied, requested_str) = match role {
        Role::System => return CompatibleRole::System,
        Role::User => return CompatibleRole::User,
        Role::Assistant => return CompatibleRole::Assistant,
        Role::Tool => return CompatibleRole::Tool,
        Role::UserSystem => (CompatibleRole::System, "user_system"),
        Role::Group => (CompatibleRole::User, "group"),
        Role::SampleMessageUser => (CompatibleRole::User, "sample_message_user"),
        Role::SampleMessageAi => (CompatibleRole::User, "sample_message_ai"),
        // Roles added to the protocol later downgrade to `user`.
        _ => (CompatibleRole::User, "unknown"),
    };
    let applied_str = match applied {
        CompatibleRole::System => "system",
        CompatibleRole::User => "user",
        CompatibleRole::Assistant => "assistant",
        CompatibleRole::Tool => "tool",
    };
    adjustments.push(OptionAdjustment {
        option: "role".into(),
        requested: json!(requested_str),
        applied: json!(applied_str),
        reason: "minimax_only_role_unsupported".into(),
    });
    applied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_roles_passthrough_without_adjustment() {
        for (role, expected) in [
            (Role::System, CompatibleRole::System),
            (Role::User, CompatibleRole::User),
            (Role::Assistant, CompatibleRole::Assistant),
            (Role::Tool, CompatibleRole::Tool),
        ] {
            let mut adj = Vec::new();
            assert_eq!(downgrade_minimax_role(role, &mut adj), expected);
            assert!(adj.is_empty(), "{role:?} should not record adjustment");
        }
    }

    #[test]
    fn user_system_downgrades_to_system() {
        let mut adj = Vec::new();
        let out = downgrade_minimax_role(Role::UserSystem, &mut adj);
        assert_eq!(out, CompatibleRole::System);
        assert_eq!(adj.len(), 1);
        assert_eq!(adj[0].requested, json!("user_system"));
        assert_eq!(adj[0].applied, json!("system"));
        assert_eq!(adj[0].reason, "minimax_only_role_unsupported");
    }

    #[test]
    fn group_and_samples_downgrade_to_user() {
        for (role, expected_str) in [
            (Role::Group, "group"),
            (Role::SampleMessageUser, "sample_message_user"),
            (Role::SampleMessageAi, "sample_message_ai"),
        ] {
            let mut adj = Vec::new();
            assert_eq!(downgrade_minimax_role(role, &mut adj), CompatibleRole::User);
            assert_eq!(adj.len(), 1);
            assert_eq!(adj[0].requested, json!(expected_str));
            assert_eq!(adj[0].applied, json!("user"));
        }
    }
}
