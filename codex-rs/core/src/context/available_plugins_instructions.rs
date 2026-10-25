use codex_protocol::protocol::PLUGINS_INSTRUCTIONS_CLOSE_TAG;
use codex_protocol::protocol::PLUGINS_INSTRUCTIONS_OPEN_TAG;

use super::ContextualUserFragment;
use codex_prompts::PromptLanguage;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::input_text;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AvailablePluginsInstructions {
    pub(crate) language: PromptLanguage,
}

impl ContextualUserFragment for AvailablePluginsInstructions {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("plugins.usage_instructions".to_string())
    }

    fn role(&self) -> &'static str {
        "developer"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        (
            PLUGINS_INSTRUCTIONS_OPEN_TAG,
            PLUGINS_INSTRUCTIONS_CLOSE_TAG,
        )
    }

    fn body(&self) -> ContentItem {
        if self.language == PromptLanguage::Chinese {
            return input_text(
                "\n## 插件\n插件是技能、MCP 服务器和应用组成的本地集合。\n\n### 如何使用插件\n- 技能命名：插件提供的技能会在技能列表中带有 `plugin_name:` 前缀。\n- MCP 命名：插件提供的 MCP 工具仍使用 `mcp__server__tool` 等标准标识符；通过工具来源判断其所属插件。\n- 触发规则：用户明确提到某个插件时，本轮优先使用该插件关联的能力。\n- 能力关系：插件不会被直接调用，应使用其底层技能、MCP 工具和应用工具完成任务。\n- 相关性：根据用户明确提及，或本轮其他位置提供的插件技能、MCP 工具和应用判断插件能力。\n- 缺失或阻塞：如果用户请求的插件没有可调用能力，应简要说明并采用最佳替代方案。\n",
            );
        }
        let mut lines = vec![
            "## Plugins".to_string(),
            "A plugin is a local bundle of skills, MCP servers, and apps.".to_string(),
        ];

        lines.push("### How to use plugins".to_string());
        lines.push(
            r###"- Skill naming: If a plugin contributes skills, those skill entries are prefixed with `plugin_name:` in the Skills list.
- MCP naming: Plugin-provided MCP tools keep standard MCP identifiers such as `mcp__server__tool`; use tool provenance to tell which plugin they come from.
- Trigger rules: If the user explicitly names a plugin, prefer capabilities associated with that plugin for that turn.
- Relationship to capabilities: Plugins are not invoked directly. Use their underlying skills, MCP tools, and app tools to help solve the task.
- Relevance: Determine what a plugin can help with from explicit user mention or from the plugin-associated skills, MCP tools, and apps exposed elsewhere in this turn.
- Missing/blocked: If the user requests a plugin that does not have relevant callable capabilities for the task, say so briefly and continue with the best fallback."###
                .to_string(),
        );

        input_text(format!("\n{}\n", lines.join("\n")))
    }
}
