use super::ContextualUserFragment;
use codex_prompts::END_INSTRUCTIONS;
use codex_prompts::END_INSTRUCTIONS_ZH_CN;
use codex_prompts::PromptLanguage;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::input_text;
use codex_protocol::protocol::REALTIME_CONVERSATION_CLOSE_TAG;
use codex_protocol::protocol::REALTIME_CONVERSATION_OPEN_TAG;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RealtimeEndInstructions {
    instructions: Option<String>,
    language: PromptLanguage,
}

impl RealtimeEndInstructions {
    pub(crate) fn new() -> Self {
        Self {
            instructions: None,
            language: PromptLanguage::English,
        }
    }

    pub(crate) fn with_language(mut self, language: PromptLanguage) -> Self {
        self.language = language;
        self
    }

    pub(crate) fn with_instructions(instructions: impl Into<String>) -> Self {
        Self {
            instructions: Some(instructions.into()),
            language: PromptLanguage::English,
        }
    }
}

impl ContextualUserFragment for RealtimeEndInstructions {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("realtime_conversation.end_instructions".to_string())
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
        let instructions = self
            .instructions
            .as_deref()
            .unwrap_or_else(|| match self.language {
                PromptLanguage::English => END_INSTRUCTIONS.trim(),
                PromptLanguage::Chinese => END_INSTRUCTIONS_ZH_CN.trim(),
            });
        input_text(format!("\n{instructions}\n"))
    }
}
