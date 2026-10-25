use super::ContextualUserFragment;
use codex_prompts::PromptLanguage;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::input_text;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ModelSwitchInstructions {
    model_instructions: String,
    language: PromptLanguage,
}

impl ModelSwitchInstructions {
    pub(crate) fn new(model_instructions: impl Into<String>) -> Self {
        Self {
            model_instructions: model_instructions.into(),
            language: PromptLanguage::English,
        }
    }

    pub(crate) fn new_with_language(
        model_instructions: impl Into<String>,
        language: PromptLanguage,
    ) -> Self {
        Self {
            model_instructions: model_instructions.into(),
            language,
        }
    }
}

impl ContextualUserFragment for ModelSwitchInstructions {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("model_switch.instructions".to_string())
    }

    fn role(&self) -> &'static str {
        "developer"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        ("<model_switch>", "</model_switch>")
    }

    fn body(&self) -> ContentItem {
        let notice = match self.language {
            PromptLanguage::English => {
                "The user was previously using a different model. Please continue the conversation according to the following instructions:"
            }
            PromptLanguage::Chinese => {
                "用户之前使用了其他模型。请按照以下指令继续对话："
            }
        };
        input_text(format!("\n{notice}\n\n{}\n", self.model_instructions))
    }
}
