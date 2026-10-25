use super::PreviousSectionState;
use super::SectionTransition;
use super::WorldStateHash;
use super::WorldStateSection;
use super::WorldStateUpdate;
use crate::context::ContextualUserFragment;
use codex_config::Sourced;
use codex_prompts::PromptLanguage;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::ContentItemMetadata;
use codex_protocol::models::input_text;
use codex_utils_string::approx_bytes_for_tokens;
use codex_utils_string::approx_tokens_from_byte_count;
use serde::Deserialize;
use serde::Serialize;
use std::io;

const MAX_MANAGED_DEVELOPER_INSTRUCTIONS_TOKENS: usize = 10_000;
const REPLACEMENT_NOTICE: &str = "These managed developer instructions replace all previously provided managed developer instructions.";
const REMOVAL_NOTICE: &str =
    "The previously provided managed developer instructions no longer apply.";
const REPLACEMENT_NOTICE_ZH_CN: &str = "以下受管开发者指令将替换之前提供的全部受管开发者指令。";
const REMOVAL_NOTICE_ZH_CN: &str = "之前提供的受管开发者指令不再生效。";

/// A requirements-owned developer message, kept separate from client instructions.
#[derive(Clone, Debug)]
pub(crate) struct ManagedDeveloperInstructions {
    instructions: String,
}

impl ContextualUserFragment for ManagedDeveloperInstructions {
    fn content_metadata(&self) -> ContentItemMetadata {
        ContentItemMetadata::developer_instructions(/*from_additional_requirements*/ true)
    }

    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("managed_config.developer_instructions".to_string())
    }

    fn role(&self) -> &'static str {
        "developer"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        (
            "<managed_developer_instructions>",
            "</managed_developer_instructions>",
        )
    }

    fn body(&self) -> ContentItem {
        input_text(format!("\n{}\n", self.instructions))
    }
}

/// Reject oversized policy rather than silently dropping part of its instructions.
pub(crate) fn validate_managed_developer_instructions(
    requirement: Option<&Sourced<String>>,
) -> io::Result<()> {
    let Some(requirement) = requirement else {
        return Ok(());
    };
    // Reserve the notice used when an existing conversation receives a new policy.
    let overhead = ManagedDeveloperInstructions {
        instructions: format!("{REPLACEMENT_NOTICE}\n\n"),
    }
    .render()
    .into_text()
    .unwrap_or_default()
    .len();
    let rendered_bytes = requirement.value.len().saturating_add(overhead);
    if rendered_bytes > approx_bytes_for_tokens(MAX_MANAGED_DEVELOPER_INSTRUCTIONS_TOKENS) {
        let estimated_tokens = approx_tokens_from_byte_count(rendered_bytes);
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "`additional_developer_instructions` from {} exceeds the model-context limit of {MAX_MANAGED_DEVELOPER_INSTRUCTIONS_TOKENS} estimated tokens ({estimated_tokens} including context markers)",
                requirement.source,
            ),
        ));
    }
    Ok(())
}

/// The managed policy currently visible in retained model history.
#[derive(Clone, Debug)]
pub(crate) struct ManagedDeveloperInstructionsState {
    instructions: Option<ManagedDeveloperInstructions>,
    language: PromptLanguage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ManagedDeveloperInstructionsSnapshot {
    instructions: Option<WorldStateHash>,
}

impl ManagedDeveloperInstructionsState {
    pub(crate) fn new(requirement: Option<&Sourced<String>>) -> Self {
        Self {
            instructions: requirement
                .filter(|requirement| !requirement.value.is_empty())
                .map(|requirement| ManagedDeveloperInstructions {
                    instructions: requirement.value.clone(),
                }),
            language: PromptLanguage::English,
        }
    }

    pub(crate) fn with_language(mut self, language: PromptLanguage) -> Self {
        self.language = language;
        self
    }
}

impl WorldStateSection for ManagedDeveloperInstructionsState {
    const ID: &'static str = "managed_developer_instructions";
    type Snapshot = ManagedDeveloperInstructionsSnapshot;

    fn matches_legacy_fragment(role: &str, text: &str) -> bool {
        role == "developer" && ManagedDeveloperInstructions::matches_text(text)
    }

    fn has_retained_fragment_matcher() -> bool {
        true
    }

    fn matches_retained_fragment(role: &str, text: &str) -> bool {
        Self::matches_legacy_fragment(role, text)
    }

    fn render_diff(
        &self,
        previous: PreviousSectionState<'_, Self::Snapshot>,
    ) -> SectionTransition<Self::Snapshot> {
        let current = ManagedDeveloperInstructionsSnapshot {
            instructions: self
                .instructions
                .as_ref()
                .map(WorldStateHash::from_fragment),
        };
        if matches!(previous, PreviousSectionState::Known(previous) if previous == &current) {
            return (None, Vec::new());
        }
        let previous_had_instructions = match previous {
            PreviousSectionState::Absent => false,
            PreviousSectionState::Unknown => true,
            PreviousSectionState::Known(previous) => previous.instructions.is_some(),
        };
        let fragment = match (&self.instructions, previous_had_instructions) {
            (Some(instructions), false) => instructions.clone(),
            (Some(instructions), true) => ManagedDeveloperInstructions {
                instructions: format!(
                    "{}\n\n{}",
                    match self.language {
                        PromptLanguage::English => REPLACEMENT_NOTICE,
                        PromptLanguage::Chinese => REPLACEMENT_NOTICE_ZH_CN,
                    },
                    instructions.instructions
                ),
            },
            (None, true) => ManagedDeveloperInstructions {
                instructions: match self.language {
                    PromptLanguage::English => REMOVAL_NOTICE,
                    PromptLanguage::Chinese => REMOVAL_NOTICE_ZH_CN,
                }
                .to_string(),
            },
            (None, false) => return (Some(current), Vec::new()),
        };
        (
            Some(current),
            vec![WorldStateUpdate::fragment(fragment).standalone()],
        )
    }
}

#[cfg(test)]
#[path = "managed_developer_instructions_tests.rs"]
mod tests;
