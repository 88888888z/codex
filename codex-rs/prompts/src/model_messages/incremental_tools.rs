//! Resolves incremental catalog wording without changing catalog identity or update timing.
//! Text is owned because world-state sections outlive the borrowed model metadata.

use codex_protocol::openai_models::IncrementalToolMessages;
use crate::PromptLanguage;

/// Resolved static wording; consumers retain ownership of runtime names and instructions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedIncrementalToolMessages {
    pub tool_update_hint: String,
    pub removed_tools_header: String,
    pub removed_namespaces_header: String,
    pub namespace_instructions_prefix: String,
    pub namespace_instructions_cleared: String,
}

impl Default for ResolvedIncrementalToolMessages {
    fn default() -> Self {
        Self::new(/*messages*/ None, PromptLanguage::English)
    }
}

impl ResolvedIncrementalToolMessages {
    pub(crate) fn new(
        messages: Option<&IncrementalToolMessages>,
        language: PromptLanguage,
    ) -> Self {
        let resolve = |text: Option<&String>, bundled: &str| {
            text.filter(|text| text.len() <= 512)
                .map_or_else(|| bundled.to_owned(), Clone::clone)
        };
        let (
            tool_update_hint,
            removed_tools_header,
            removed_namespaces_header,
            namespace_instructions_prefix,
            namespace_instructions_cleared,
        ) = match language {
            PromptLanguage::English => (
                "This is an incremental tools update. Previously declared tools remain available for direct calls unless explicitly marked unavailable. If a tool is redefined here, its latest definition replaces the earlier one.",
                "The following tools are no longer available. Do not call them:",
                "The following namespaces are no longer available. Do not call tools in them unless those tools are declared in a later update:",
                "Updated instructions for the {name} namespace:\n",
                "The {name} namespace no longer has additional instructions.",
            ),
            PromptLanguage::Chinese => (
                "这是增量工具更新。之前声明的工具仍可直接调用，除非明确标记为不可用。如果某工具在此重新定义，最新定义会替换旧定义。",
                "以下工具已不可用，不要调用：",
                "以下命名空间已不可用。除非后续更新重新声明其中的工具，否则不要调用这些命名空间中的工具：",
                "`{name}` 命名空间的更新说明：\n",
                "`{name}` 命名空间不再包含额外说明。",
            ),
        };
        Self {
            tool_update_hint: resolve(
                messages.and_then(|m| m.tool_update_hint.as_ref()),
                tool_update_hint,
            ),
            removed_tools_header: resolve(
                messages.and_then(|m| m.removed_tools_header.as_ref()),
                removed_tools_header,
            ),
            removed_namespaces_header: resolve(
                messages.and_then(|m| m.removed_namespaces_header.as_ref()),
                removed_namespaces_header,
            ),
            namespace_instructions_prefix: resolve(
                messages.and_then(|m| m.namespace_instructions_prefix.as_ref()),
                namespace_instructions_prefix,
            ),
            namespace_instructions_cleared: resolve(
                messages.and_then(|m| m.namespace_instructions_cleared.as_ref()),
                namespace_instructions_cleared,
            ),
        }
    }
}
