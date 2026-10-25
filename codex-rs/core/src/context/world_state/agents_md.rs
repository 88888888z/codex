use super::PreviousSectionState;
use super::SectionTransition;
use super::WorldStateSection;
use super::WorldStateUpdate;
use crate::agents_md::LoadedAgentsMd;
use crate::context::ContextualUserFragment;
use crate::context::UserInstructions;
use codex_prompts::PromptLanguage;
use serde::Deserialize;
use serde::Serialize;

const REPLACEMENT_NOTICE: &str =
    "These AGENTS.md instructions replace all previously provided AGENTS.md instructions.";
const REMOVAL_NOTICE: &str = "The previously provided AGENTS.md instructions no longer apply.";
const REPLACEMENT_NOTICE_ZH_CN: &str = "以下 AGENTS.md 指令将替换之前提供的全部 AGENTS.md 指令。";
const REMOVAL_NOTICE_ZH_CN: &str = "之前提供的 AGENTS.md 指令不再生效。";

/// The AGENTS.md instructions currently visible to the model.
#[derive(Clone, Debug, Default)]
pub(crate) struct AgentsMdState {
    instructions: Option<UserInstructions>,
    language: PromptLanguage,
}

/// Persisted model-visible AGENTS.md state, without filesystem provenance.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct AgentsMdSnapshot {
    directory: Option<String>,
    text: Option<String>,
}

impl AgentsMdState {
    pub(crate) fn new(loaded: Option<&LoadedAgentsMd>) -> Self {
        Self {
            instructions: loaded.map(LoadedAgentsMd::contextual_user_fragment),
            language: PromptLanguage::English,
        }
    }

    pub(crate) fn with_language(mut self, language: PromptLanguage) -> Self {
        self.language = language;
        self
    }
}

impl WorldStateSection for AgentsMdState {
    const ID: &'static str = "agents_md";
    type Snapshot = AgentsMdSnapshot;

    fn matches_legacy_fragment(role: &str, text: &str) -> bool {
        role == "user" && UserInstructions::matches_text(text)
    }

    fn render_diff(
        &self,
        previous: PreviousSectionState<'_, Self::Snapshot>,
    ) -> SectionTransition<Self::Snapshot> {
        let current = match &self.instructions {
            Some(instructions) => AgentsMdSnapshot {
                directory: instructions.directory.clone(),
                text: Some(instructions.text.clone()),
            },
            None => AgentsMdSnapshot::default(),
        };
        if matches!(previous, PreviousSectionState::Known(previous) if previous == &current) {
            return (None, Vec::new());
        }

        let previous_may_contain_instructions = match previous {
            PreviousSectionState::Known(previous) => previous.text.is_some(),
            PreviousSectionState::Unknown => true,
            PreviousSectionState::Absent => false,
        };
        let instructions = match (&self.instructions, previous_may_contain_instructions) {
            (Some(instructions), true) => UserInstructions {
                directory: instructions.directory.clone(),
                text: format!(
                    "{}\n\n{}",
                    match self.language {
                        PromptLanguage::English => REPLACEMENT_NOTICE,
                        PromptLanguage::Chinese => REPLACEMENT_NOTICE_ZH_CN,
                    },
                    instructions.text
                ),
            },
            (Some(instructions), false) => instructions.clone(),
            (None, true) => UserInstructions {
                directory: None,
                text: match self.language {
                    PromptLanguage::English => REMOVAL_NOTICE,
                    PromptLanguage::Chinese => REMOVAL_NOTICE_ZH_CN,
                }
                .to_string(),
            },
            (None, false) => return (Some(current), Vec::new()),
        };
        (
            Some(current),
            vec![WorldStateUpdate::fragment(instructions)],
        )
    }
}

#[cfg(test)]
#[path = "agents_md_tests.rs"]
mod tests;
