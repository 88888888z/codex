use codex_mcp::CODEX_APPS_MCP_SERVER_NAME;
use codex_protocol::protocol::APPS_INSTRUCTIONS_CLOSE_TAG;
use codex_protocol::protocol::APPS_INSTRUCTIONS_OPEN_TAG;

use super::ContextualUserFragment;
use codex_prompts::PromptLanguage;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::input_text;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AppsInstructions {
    pub(crate) language: PromptLanguage,
}

impl ContextualUserFragment for AppsInstructions {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("apps.instructions".to_string())
    }

    fn role(&self) -> &'static str {
        "developer"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        (APPS_INSTRUCTIONS_OPEN_TAG, APPS_INSTRUCTIONS_CLOSE_TAG)
    }

    fn body(&self) -> ContentItem {
        input_text(match self.language {
            PromptLanguage::English => format!(
                "\n## Apps (Connectors)\nApps (Connectors) can be explicitly triggered in user messages in the format `[$app-name](app://{{connector_id}})`. Apps can also be implicitly triggered as long as the context suggests usage of available apps.\nAn app is equivalent to a set of MCP tools within the `{CODEX_APPS_MCP_SERVER_NAME}` MCP.\nAn installed app's MCP tools are either provided to you already, or can be lazy-loaded through the `tool_search` tool. If `tool_search` is available, the apps that are searchable by `tools_search` will be listed by it.\nDo not additionally call list_mcp_resources or list_mcp_resource_templates for apps.\n"
            ),
            PromptLanguage::Chinese => format!(
                "\n## 应用（Connectors）\n用户可以通过 `[$app-name](app://{{connector_id}})` 格式在消息中显式调用应用。只要上下文表明需要使用可用应用，也可以隐式触发。\n应用等价于 `{CODEX_APPS_MCP_SERVER_NAME}` MCP 中的一组 MCP 工具。\n已安装应用的 MCP 工具可能已经提供，也可以通过 `tool_search` 延迟加载。如果 `tool_search` 可用，可由其搜索的应用会列在其中。\n不要额外为应用调用 `list_mcp_resources` 或 `list_mcp_resource_templates`。\n"
            ),
        })
    }
}
