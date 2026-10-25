//! Resolves model-owned messages from catalog values and bundled defaults.
//! Resolution preserves sparse catalog data and source identity, with family-specific validation.
//! Prompt composition and runtime settings remain with consumers; each accessor
//! selects and resolves only the requested message family.

use codex_protocol::openai_models::CodeModeToolMessages;
use codex_protocol::openai_models::ConfirmationPolicies;
use codex_protocol::openai_models::IndirectDescriptionPrefixes;
use codex_protocol::openai_models::McpResourceToolMessages;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ModelMessages;
use codex_protocol::openai_models::ToolMessage;
use permissions::ResolvedApprovalMessages;
use permissions::ResolvedPermissionMessages;
use crate::PromptLanguage;

mod collaboration;
mod guardian;
mod incremental_tools;
mod multi_agent;
pub(crate) mod permissions;

pub use collaboration::ResolvedCollaborationModeMessages;
pub use guardian::ResolvedAutoReviewMessages;
pub use incremental_tools::ResolvedIncrementalToolMessages;
pub use multi_agent::ResolvedMultiAgentMessages;

/// Text together with whether it was supplied by the catalog, even when it equals the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedMessage<'a> {
    Catalog(&'a str),
    Bundled(&'static str),
}

impl<'a> ResolvedMessage<'a> {
    pub(crate) fn new(catalog: Option<&'a str>, bundled: &'static str) -> Self {
        catalog.map_or(Self::Bundled(bundled), Self::Catalog)
    }

    pub fn text(self) -> &'a str {
        match self {
            Self::Catalog(text) | Self::Bundled(text) => text,
        }
    }

    pub fn catalog_override(self) -> Option<&'a str> {
        match self {
            Self::Catalog(text) => Some(text),
            Self::Bundled(_) => None,
        }
    }
}

const REQUEST_USER_INPUT_ASYNC_DESCRIPTION: &str = "Ask the user one or more questions during ongoing work. Use this tool only to request missing information, preferences, constraints, clarification, or approval. The tool returns immediately without ending the turn or waiting for a reply; any reply arrives asynchronously as a new user message. Keep questions concise, self-contained, and easy to understand, using a level of detail appropriate to the user and task. The UI always allows a free-text answer, including when suggested options are provided. A preselected option is not submitted automatically.";
const REQUEST_USER_INPUT_ASYNC_DESCRIPTION_ZH_CN: &str = "在工作过程中向用户提出一个或多个问题。仅用于获取缺失信息、偏好、约束、澄清或审批。工具会立即返回，不会结束当前回合或等待回复；用户回复会作为新的用户消息异步到达。问题应简洁、完整、易懂，并根据用户和任务调整详细程度。即使提供了建议选项，界面也始终允许自由文本回答。预选选项不会被自动提交。";
const REMINDER_MESSAGE_TEMPLATE: &str = concat!(
    "Your context window is nearly exhausted (only {n_remaining} tokens remaining) and will be automatically reset for you soon. ",
    "Once reset, message items in current context window will be cleared in the new window, but notes and history items will be persistent across windows."
);
const REMINDER_MESSAGE_TEMPLATE_ZH_CN: &str = concat!(
    "当前上下文窗口即将耗尽，仅剩 {n_remaining} 个 token，系统很快会自动重置上下文。",
    "重置后，当前窗口中的消息项会从新窗口移除，但笔记和历史记录仍会跨窗口保留。"
);
const PERSISTENT_INSTRUCTIONS: &str = include_str!("../templates/persistent_mode.md");
const PERSISTENT_INSTRUCTIONS_ZH_CN: &str =
    include_str!("../templates/persistent_mode.zh-CN.md");
const CONTENT_FILTER_GUIDANCE: &str = "Your previous response was blocked by a content filter. Do not treat this as a transient failure or try to reproduce or work around the blocked content through repeated attempts, altered formatting, splitting, encoding, tools, subagents, or later wakes. Briefly explain the limitation and offer a permitted alternative. Continue unrelated authorized work.";
const CONTENT_FILTER_GUIDANCE_ZH_CN: &str = "你上一条回复被内容过滤器拦截。不要把这次拦截视为临时故障，也不要通过反复尝试、改写格式、拆分内容、编码、工具、子代理或后续唤醒来复现或绕过被拦截的内容。简要说明限制，并提供允许的替代方案。继续处理其他已获授权的无关工作。";
const MAX_CONTENT_FILTER_GUIDANCE_BYTES: usize = 512;

/// Resolves model-owned text from catalog overrides or bundled defaults, one family at a time.
///
/// The view borrows the consumer's captured model metadata without resolving any families.
/// Missing values select bundled text or retain delegation to consumer-owned settings.
/// Explicit empty strings remain overrides unless a message family validates them.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedModelMessages<'a> {
    catalog_messages: Option<&'a ModelMessages>,
    language: PromptLanguage,
}

impl<'a> ResolvedModelMessages<'a> {
    /// Borrows messages from the selected model; absent catalog values still use defaults.
    pub fn from_model(model_info: &'a ModelInfo) -> Self {
        Self {
            catalog_messages: model_info.model_messages.as_ref(),
            language: PromptLanguage::English,
        }
    }

    /// Borrows messages with the requested language for bundled defaults.
    pub fn from_model_with_language(model_info: &'a ModelInfo, language: PromptLanguage) -> Self {
        Self {
            catalog_messages: model_info.model_messages.as_ref(),
            language,
        }
    }

    /// Resolves only bundled defaults, independently of any selected model.
    pub fn bundled() -> Self {
        Self {
            catalog_messages: None,
            language: PromptLanguage::English,
        }
    }

    /// Resolves bundled defaults in the requested language.
    pub fn bundled_with_language(language: PromptLanguage) -> Self {
        Self {
            catalog_messages: None,
            language,
        }
    }

    /// Resolves the literal instruction template, preserving missing values.
    pub fn instructions_template(&self) -> Option<&'a str> {
        self.catalog_messages
            .and_then(|messages| messages.instructions_template.as_deref())
    }

    /// Resolves approval messages and their bundled alternatives.
    pub(crate) fn approvals(self) -> ResolvedApprovalMessages<'a> {
        ResolvedApprovalMessages::new_with_language(
            self.catalog_messages
                .and_then(|messages| messages.approvals.as_ref()),
            self.language,
        )
    }

    /// Resolves permission templates without preparing runtime permission facts.
    pub(crate) fn permissions(self) -> ResolvedPermissionMessages<'a> {
        ResolvedPermissionMessages::new_with_language(
            self.catalog_messages
                .and_then(|messages| messages.permissions.as_ref()),
            self.language,
        )
    }

    /// Resolves mode overrides while retaining bundled alternatives and absent values.
    pub fn collaboration_modes(&self) -> ResolvedCollaborationModeMessages<'a> {
        ResolvedCollaborationModeMessages::new(
            self.catalog_messages
                .and_then(|messages| messages.collaboration_modes.as_ref()),
            self.language,
        )
    }

    /// Resolves multi-agent messages while preserving their catalog or bundled source.
    pub fn multi_agent(&self) -> ResolvedMultiAgentMessages<'a> {
        ResolvedMultiAgentMessages::new(
            self.catalog_messages
                .and_then(|messages| messages.multi_agent.as_ref()),
            self.language,
        )
    }

    /// Resolves auto-review policy messages and rejection/timeout instructions.
    pub fn auto_review(&self) -> ResolvedAutoReviewMessages<'a> {
        ResolvedAutoReviewMessages::new(
            self.catalog_messages
                .and_then(|messages| messages.auto_review.as_ref()),
            self.language,
        )
    }

    /// Resolves classifier instructions from catalog text or the bundled default.
    pub fn guardian_classifier_instructions(&self) -> &'a str {
        guardian::classifier_instructions(self.catalog_messages, self.language)
    }

    /// Resolves reminder text without selecting or validating runtime budget settings.
    pub fn token_budget_reminder_template(&self) -> &'a str {
        self.catalog_messages
            .and_then(|messages| messages.token_budget.as_ref())
            .map_or_else(|| match self.language {
                PromptLanguage::English => REMINDER_MESSAGE_TEMPLATE,
                PromptLanguage::Chinese => REMINDER_MESSAGE_TEMPLATE_ZH_CN,
            }, |messages| {
                messages.reminder_message_template.as_str()
            })
    }

    /// Selects native history definitions; missing fields delegate to the tool owner.
    pub fn history(self) -> Option<&'a codex_protocol::openai_models::HistoryToolMessages> {
        self.catalog_messages?.tools.as_ref()?.history.as_ref()
    }

    /// Selects native notes definitions; missing fields delegate to the tool owner.
    pub fn notes(self) -> Option<&'a codex_protocol::openai_models::NotesToolMessages> {
        self.catalog_messages?.tools.as_ref()?.notes.as_ref()
    }

    /// Selects context-window control definitions independently of runtime tool names.
    pub fn context_management(
        self,
    ) -> Option<&'a codex_protocol::openai_models::ContextManagementToolMessages> {
        self.catalog_messages?
            .tools
            .as_ref()?
            .context_management
            .as_ref()
    }

    /// Resolves optional confirmation policies, leaving missing values to actor defaults.
    pub fn confirmation_policies(&self) -> Option<&'a ConfirmationPolicies> {
        self.catalog_messages
            .and_then(|messages| messages.confirmation_policies.as_ref())
    }

    /// Resolves the asynchronous user-input tool description.
    pub fn request_user_input_async_description(&self) -> &'a str {
        self.catalog_messages
            .and_then(|messages| messages.tools.as_ref())
            .and_then(|tools| tools.send_user_message_async.as_ref())
            .and_then(|tool| tool.description.as_deref())
            .unwrap_or(match self.language {
                PromptLanguage::English => REQUEST_USER_INPUT_ASYNC_DESCRIPTION,
                PromptLanguage::Chinese => REQUEST_USER_INPUT_ASYNC_DESCRIPTION_ZH_CN,
            })
    }

    /// Selects the asynchronous user-input schema; parsing belongs to the tool consumer.
    pub fn request_user_input_async_parameters_override(&self) -> Option<&'a str> {
        self.catalog_messages?
            .tools
            .as_ref()?
            .send_user_message_async
            .as_ref()?
            .parameters
            .as_deref()
    }

    /// Selects a V2 tool's static description by its name, independently of its runtime namespace.
    /// Missing text retains the tool's bundled description; an empty string replaces it.
    pub fn multi_agent_tool_description_override(&self, tool_name: &str) -> Option<&'a str> {
        self.multi_agent_tool(tool_name)?.description.as_deref()
    }

    /// Selects a V2 tool's complete parameter schema; parsing belongs to the tool consumer.
    pub fn multi_agent_tool_parameters_override(&self, tool_name: &str) -> Option<&'a str> {
        self.multi_agent_tool(tool_name)?.parameters.as_deref()
    }

    fn multi_agent_tool(self, tool_name: &str) -> Option<&'a ToolMessage> {
        self.catalog_messages
            .and_then(|messages| messages.tools.as_ref())
            .and_then(|tools| tools.multi_agent.as_ref())?
            .by_name(tool_name)
    }

    /// Selects resource helper messages; schema parsing belongs to the tool owner.
    pub fn mcp_resources(&self) -> Option<&'a McpResourceToolMessages> {
        self.catalog_messages?
            .tools
            .as_ref()?
            .mcp_resources
            .as_ref()
    }

    /// Selects indirect tool guidance; tool rendering owns namespace mapping and normalization.
    pub fn indirect_description_prefixes(&self) -> Option<&'a IndirectDescriptionPrefixes> {
        self.catalog_messages?
            .tools
            .as_ref()?
            .indirect_description_prefixes
            .as_ref()
    }

    /// Selects per-function guidance restricted to the default `functions` namespace.
    pub fn functions_namespace_functions_description_prefixes(
        &self,
    ) -> Option<&'a std::collections::BTreeMap<String, String>> {
        self.catalog_messages?
            .tools
            .as_ref()?
            .functions_namespace_functions_description_prefixes
            .as_ref()
    }

    /// Resolves bounded incremental catalog wording for an owned world-state section.
    pub fn incremental_tools(&self) -> ResolvedIncrementalToolMessages {
        ResolvedIncrementalToolMessages::new(
            self.catalog_messages
                .and_then(|messages| messages.tools.as_ref())
                .and_then(|tools| tools.incremental_tools.as_ref()),
            self.language,
        )
    }

    /// Selects Code Mode messages; bundled text and runtime composition belong to the tool owner.
    pub fn code_mode(&self) -> Option<&'a CodeModeToolMessages> {
        self.catalog_messages?.tools.as_ref()?.code_mode.as_ref()
    }

    /// Selects wait's complete description.
    pub fn code_mode_wait_description_override(&self) -> Option<&'a str> {
        self.code_mode()?.wait.as_ref()?.description.as_deref()
    }

    /// Selects wait's parameter schema. Exec uses a harness-owned freeform grammar.
    pub fn code_mode_wait_parameters_override(&self) -> Option<&'a str> {
        self.code_mode()?.wait.as_ref()?.parameters.as_deref()
    }

    /// Resolves persistent-mode instructions without deciding whether the mode is active.
    pub fn persistent_instructions(&self) -> &'a str {
        self.catalog_messages
            .and_then(|messages| messages.persistent_instructions.as_deref())
            .unwrap_or(match self.language {
                PromptLanguage::English => PERSISTENT_INSTRUCTIONS,
                PromptLanguage::Chinese => PERSISTENT_INSTRUCTIONS_ZH_CN,
            })
    }

    /// Resolves bounded recovery guidance, falling back without truncating instructions.
    pub fn content_filter_guidance(&self) -> &'a str {
        self.catalog_messages
            .and_then(|messages| messages.content_filter_guidance.as_deref())
            .filter(|text| {
                !text.trim().is_empty() && text.len() <= MAX_CONTENT_FILTER_GUIDANCE_BYTES
            })
            .unwrap_or(match self.language {
                PromptLanguage::English => CONTENT_FILTER_GUIDANCE,
                PromptLanguage::Chinese => CONTENT_FILTER_GUIDANCE_ZH_CN,
            })
    }
}
