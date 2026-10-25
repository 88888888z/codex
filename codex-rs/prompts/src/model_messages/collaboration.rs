//! Resolves collaboration catalog text and bundled presets while retaining their source.
//! Runtime consumers use catalog overrides first, then the selected mode's settings.

use super::ResolvedMessage;
use codex_collaboration_mode_templates::DEFAULT;
use codex_collaboration_mode_templates::DEFAULT_ZH_CN;
use codex_collaboration_mode_templates::PLAN;
use codex_collaboration_mode_templates::PLAN_ZH_CN;
use codex_protocol::openai_models::CollaborationModeMessages;
use crate::PromptLanguage;

#[derive(Debug, Clone, Copy)]
pub struct ResolvedCollaborationModeMessages<'a> {
    pub default: ResolvedMessage<'a>,
    pub plan: ResolvedMessage<'a>,
}

impl<'a> ResolvedCollaborationModeMessages<'a> {
    pub(crate) fn new(
        messages: Option<&'a CollaborationModeMessages>,
        language: PromptLanguage,
    ) -> Self {
        let (default, plan) = match language {
            PromptLanguage::English => (DEFAULT, PLAN),
            PromptLanguage::Chinese => (DEFAULT_ZH_CN, PLAN_ZH_CN),
        };
        Self {
            default: ResolvedMessage::new(
                messages.and_then(|m| m.default.as_deref()),
                default,
            ),
            plan: ResolvedMessage::new(messages.and_then(|m| m.plan.as_deref()), plan),
        }
    }
}
