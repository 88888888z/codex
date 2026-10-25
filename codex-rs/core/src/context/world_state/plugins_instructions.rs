use super::PreviousSectionState;
use super::SectionTransition;
use super::WorldStateSection;
use super::WorldStateUpdate;
use crate::context::AvailablePluginsInstructions;
use crate::context::ContextualUserFragment;
use codex_prompts::PromptLanguage;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct PluginsInstructionsSnapshot {
    available: bool,
    chinese: bool,
}

/// Whether generic plugin usage guidance should be visible to the model.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PluginsInstructionsState {
    available: bool,
    language: PromptLanguage,
}

impl PluginsInstructionsState {
    pub(crate) fn new(available: bool) -> Self {
        Self {
            available,
            language: PromptLanguage::English,
        }
    }

    pub(crate) fn with_language(mut self, language: PromptLanguage) -> Self {
        self.language = language;
        self
    }
}

impl WorldStateSection for PluginsInstructionsState {
    const ID: &'static str = "plugins_instructions";
    type Snapshot = PluginsInstructionsSnapshot;

    fn matches_legacy_fragment(role: &str, text: &str) -> bool {
        role == "developer" && AvailablePluginsInstructions::matches_text(text)
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
        let current = PluginsInstructionsSnapshot {
            available: self.available,
            chinese: self.language == PromptLanguage::Chinese,
        };
        if !self.available
            || matches!(previous, PreviousSectionState::Known(previous) if previous == &current)
            || matches!(previous, PreviousSectionState::Unknown)
        {
            return (Some(current), Vec::new());
        }

        (
            Some(current),
            vec![WorldStateUpdate::fragment(AvailablePluginsInstructions {
                language: self.language,
            })],
        )
    }
}

#[cfg(test)]
#[path = "plugins_instructions_tests.rs"]
mod tests;
