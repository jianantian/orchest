//! Centralized, version-controlled prompt templates for the agent runtime.
//!
//! Every prompt the runtime sends to an LLM lives here — compaction summaries,
//! system hints, guardrails, and so on.
//!
//! ## Language policy
//!
//! Defaults are English. Prompts that process user-generated content instruct
//! the model to match the source language automatically. If a model consistently
//! fails at language detection, we add explicit i18n *then*, not before.

/// Prompt used to summarise old conversation history during context compaction.
///
/// The summary replaces the full history so the agent can continue with a
/// compressed but semantically equivalent context window.
pub const COMPACTION_SUMMARY_PROMPT: &str = concat!(
    "Below is a transcript of an AI agent session. Summarise the key events concisely ",
    "while preserving enough detail for the agent to continue the task uninterrupted.\n",
    "\n",
    "Include:\n",
    "- Which tools were called and what they returned\n",
    "- What information was gathered\n",
    "- What decisions or plan changes were made\n",
    "\n",
    "Match the language of the transcript. If it is mostly Chinese, reply in Chinese; ",
    "if mostly English, reply in English; otherwise use the dominant language.\n",
    "\n",
    "{history}",
);

/// Prefix wrapped around a compaction summary when it is inserted back into
/// the message list. The model sees this as a system-level note.
pub const COMPACTION_SUMMARY_PREFIX: &str = "Context summary:\n";
