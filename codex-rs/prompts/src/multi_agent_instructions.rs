//! Assembles multi-agent role instructions from selected text and runtime capabilities.
//! The segment owns rendering and attribution; consumers select and capture its inputs.

use crate::without_update_plan_instructions;
use crate::PromptLanguage;
use codex_context_fragments::ContextualUserFragment;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::input_text;

const DEFAULT_MULTI_AGENT_V2_MODEL_OVERRIDE_USAGE_HINT_TEXT: &str = "Full-history forks (`fork_turns` omitted or `\"all\"`) inherit the parent model and reasoning effort and do not accept overrides. Only set `model` or `reasoning_effort` when explicitly requested by the user, applicable `AGENTS.md` instructions, or skill instructions; when doing so, set `fork_turns` to `\"none\"`.";
const DEFAULT_MULTI_AGENT_V2_MODEL_OVERRIDE_USAGE_HINT_TEXT_ZH_CN: &str = "完整历史分叉（省略 `fork_turns` 或设为 `\"all\"`）会继承父代理的模型和推理强度，不接受覆盖。只有用户、适用的 `AGENTS.md` 或技能明确要求时，才设置 `model` 或 `reasoning_effort`；此时应将 `fork_turns` 设为 `\"none\"`。";
const DEFAULT_MULTI_AGENT_V2_WAIT_AGENT_USAGE_HINT_TEXT: &str =
    "When calling `wait_agent`, prefer longer waits (minutes) to avoid busy polling.";
const DEFAULT_MULTI_AGENT_V2_WAIT_AGENT_USAGE_HINT_TEXT_ZH_CN: &str =
    "调用 `wait_agent` 时优先使用较长等待时间（分钟级），避免忙轮询。";
const DEFAULT_MULTI_AGENT_V2_SHARED_USAGE_HINT_TEXT: &str = r#"Note that collaboration tools cannot be called from inside `functions.exec`. Call `spawn_agent`, `send_message`, `followup_task`, `wait_agent`, `interrupt_agent`, and `list_agents` only as direct tool calls using the recipient shown in their tool definitions, such as `to=functions.collaboration.spawn_agent`, since they are intentionally absent from the `functions.exec` `tools.*` namespace. Available tools in `functions.exec` are explicitly described with a `tools` namespace in the developer message.

All agents share the same directory. In detail:
- All agents have access to the same container and filesystem as you.
- All agents use the same current working directory.
- As a result, edits made by one agent are immediately visible to all other agents.
"#;
const DEFAULT_MULTI_AGENT_V2_SHARED_USAGE_HINT_TEXT_ZH_CN: &str = r#"协作工具不能在 `functions.exec` 内部调用。请按照工具定义中的接收者，直接调用 `spawn_agent`、`send_message`、`followup_task`、`wait_agent`、`interrupt_agent` 和 `list_agents`，例如 `to=functions.collaboration.spawn_agent`。这些工具被有意排除在 `functions.exec` 的 `tools.*` 命名空间之外。`functions.exec` 中可用的工具会在开发者消息的 `tools` 命名空间中明确列出。

所有代理共享同一目录：
- 所有代理都可访问与你相同的容器和文件系统。
- 所有代理使用相同的工作目录。
- 一个代理的修改会立即对其他代理可见。
"#;

/// Multi-agent role text and the captured capabilities used to render its context segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultiAgentRoleInstructions {
    /// Complete configured instructions, emitted verbatim without catalog markers.
    Configured(String),
    /// Selected catalog or bundled text, composed with captured runtime guidance.
    Composed {
        base: String,
        marked: bool,
        omit_update_plan_instructions: bool,
        max_concurrency: usize,
        wait_agent_enabled: bool,
        expose_model_overrides: bool,
        language: PromptLanguage,
    },
}

impl MultiAgentRoleInstructions {
    /// Content classification shared by role producers and inherited-history filtering.
    pub const KIND: &str = "multi_agent.role_instructions";
}

impl ContextualUserFragment for MultiAgentRoleInstructions {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind(Self::KIND.to_string())
    }

    fn role(&self) -> &'static str {
        "developer"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        match self {
            Self::Composed { marked: true, .. } => Self::type_markers(),
            Self::Configured(_) | Self::Composed { marked: false, .. } => ("", ""),
        }
    }

    fn type_markers() -> (&'static str, &'static str) {
        ("<multi_agent_role>", "</multi_agent_role>")
    }

    fn body(&self) -> ContentItem {
        input_text(match self {
            Self::Configured(text) => text.clone(),
            Self::Composed {
                base,
                omit_update_plan_instructions,
                max_concurrency,
                wait_agent_enabled,
                expose_model_overrides,
                language,
                ..
            } => {
                let base = if *omit_update_plan_instructions {
                    without_update_plan_instructions(base)
                } else {
                    base.clone()
                };
                let (
                    wait_agent_hint,
                    shared,
                    concurrency_prefix,
                    concurrency_suffix,
                    model_override_hint,
                ) = match language {
                    PromptLanguage::English => (
                        DEFAULT_MULTI_AGENT_V2_WAIT_AGENT_USAGE_HINT_TEXT,
                        DEFAULT_MULTI_AGENT_V2_SHARED_USAGE_HINT_TEXT,
                        "There are ",
                        " available concurrency slots, meaning that up to ",
                        DEFAULT_MULTI_AGENT_V2_MODEL_OVERRIDE_USAGE_HINT_TEXT,
                    ),
                    PromptLanguage::Chinese => (
                        DEFAULT_MULTI_AGENT_V2_WAIT_AGENT_USAGE_HINT_TEXT_ZH_CN,
                        DEFAULT_MULTI_AGENT_V2_SHARED_USAGE_HINT_TEXT_ZH_CN,
                        "当前有 ",
                        " 个并发槽位，最多可同时运行 ",
                        DEFAULT_MULTI_AGENT_V2_MODEL_OVERRIDE_USAGE_HINT_TEXT_ZH_CN,
                    ),
                };
                let wait_agent_guidance = if *wait_agent_enabled {
                    format!("{wait_agent_hint}\n\n")
                } else {
                    String::new()
                };
                let mut text = match language {
                    PromptLanguage::English => format!(
                        "{base}\n{shared}\n{wait_agent_guidance}{concurrency_prefix}{max_concurrency}{concurrency_suffix}{max_concurrency} agents can be active at once, including you."
                    ),
                    PromptLanguage::Chinese => format!(
                        "{base}\n{shared}\n{wait_agent_guidance}{concurrency_prefix}{max_concurrency}{concurrency_suffix}{max_concurrency} 个代理，包括你自己。"
                    ),
                };
                if *expose_model_overrides {
                    text.push_str("\n\n");
                    text.push_str(model_override_hint);
                }
                text
            }
        })
    }
}

#[cfg(test)]
#[path = "multi_agent_instructions_tests.rs"]
mod tests;
