use super::ContextualUserFragment;
use codex_prompts::START_INSTRUCTIONS;
use codex_prompts::START_INSTRUCTIONS_ZH_CN;
use codex_prompts::PromptLanguage;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::input_text;
use codex_protocol::protocol::REALTIME_CONVERSATION_CLOSE_TAG;
use codex_protocol::protocol::REALTIME_CONVERSATION_OPEN_TAG;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RealtimeStartInstructions {
    pub(crate) language: PromptLanguage,
}

impl ContextualUserFragment for RealtimeStartInstructions {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("realtime_conversation.start_instructions".to_string())
    }

    fn role(&self) -> &'static str {
        "developer"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        (
            REALTIME_CONVERSATION_OPEN_TAG,
            REALTIME_CONVERSATION_CLOSE_TAG,
        )
    }

    fn body(&self) -> ContentItem {
        let instructions = match self.language {
            PromptLanguage::English => START_INSTRUCTIONS,
            PromptLanguage::Chinese => START_INSTRUCTIONS_ZH_CN,
        };
        input_text(format!("\n{}\n", instructions.trim()))
    }
}
