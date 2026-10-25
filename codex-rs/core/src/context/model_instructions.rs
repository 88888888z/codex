//! A single model-context fragment for independent model and instruction changes.

use super::ContextualUserFragment;
use codex_prompts::PromptLanguage;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::input_text;

const REPLACEMENT_NOTICE: &str =
    "The following instructions replace the previous base instructions:";
const REMOVAL_NOTICE: &str =
    "The previous base instructions no longer apply. No replacement base instructions are active.";
const REPLACEMENT_NOTICE_ZH_CN: &str = "以下指令将替换之前的基础指令：";
const REMOVAL_NOTICE_ZH_CN: &str = "之前的基础指令不再生效，当前没有新的基础指令。";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ModelInstructions {
    pub(crate) model: Option<String>,
    /// None leaves instructions unchanged; Some replaces them, including an empty value.
    pub(crate) instructions: Option<String>,
    pub(crate) language: PromptLanguage,
}

impl ContextualUserFragment for ModelInstructions {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("model.instructions".to_string())
    }

    fn role(&self) -> &'static str {
        "developer"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        ("<model_instructions>", "</model_instructions>")
    }

    fn body(&self) -> ContentItem {
        let mut body = String::from("\n");
        if let Some(model) = &self.model {
            match self.language {
                PromptLanguage::English => {
                    body.push_str(&format!("The current model is {model}.\n"));
                }
                PromptLanguage::Chinese => {
                    body.push_str(&format!("当前模型为 {model}。\n"));
                }
            }
        }
        match &self.instructions {
            Some(instructions) if instructions.is_empty() => {
                body.push_str(match self.language {
                    PromptLanguage::English => REMOVAL_NOTICE,
                    PromptLanguage::Chinese => REMOVAL_NOTICE_ZH_CN,
                });
                body.push('\n');
            }
            Some(instructions) => {
                let notice = match self.language {
                    PromptLanguage::English => REPLACEMENT_NOTICE,
                    PromptLanguage::Chinese => REPLACEMENT_NOTICE_ZH_CN,
                };
                body.push_str(&format!("{notice}\n\n{instructions}\n"));
            }
            None => {}
        }
        input_text(body)
    }
}
