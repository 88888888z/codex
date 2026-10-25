use super::PreviousSectionState;
use super::SectionTransition;
use super::WorldStateSection;
use super::WorldStateUpdate;
use crate::context::AppsInstructions;
use crate::context::ContextualUserFragment;
use codex_prompts::PromptLanguage;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct AppsInstructionsSnapshot {
    available: bool,
    chinese: bool,
}

/// Whether generic Apps usage guidance should be visible to the model.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AppsInstructionsState {
    available: bool,
    language: PromptLanguage,
}

impl AppsInstructionsState {
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

impl WorldStateSection for AppsInstructionsState {
    const ID: &'static str = "apps_instructions";
    type Snapshot = AppsInstructionsSnapshot;

    fn matches_legacy_fragment(role: &str, text: &str) -> bool {
        role == "developer" && AppsInstructions::matches_text(text)
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
        let current = AppsInstructionsSnapshot {
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
            vec![WorldStateUpdate::fragment(AppsInstructions {
                language: self.language,
            })],
        )
    }
}

#[cfg(test)]
#[path = "apps_instructions_tests.rs"]
mod tests;
