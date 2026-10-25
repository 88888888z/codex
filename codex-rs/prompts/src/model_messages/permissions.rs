//! Resolves permission and approval text from sparse catalog messages and bundled defaults.
//! Preserves explicit empty strings and source identity; composition and rendering live in
//! `permissions_instructions` and consume caller-resolved runtime facts.

use super::ResolvedMessage;
use crate::PromptLanguage;
use codex_protocol::openai_models::ApprovalMessages;
use codex_protocol::openai_models::PermissionMessages;

const APPROVAL_POLICY_NEVER: &str =
    include_str!("../../templates/permissions/approval_policy/never.md");
const APPROVAL_POLICY_UNLESS_TRUSTED: &str =
    include_str!("../../templates/permissions/approval_policy/unless_trusted.md");
const APPROVAL_POLICY_ON_REQUEST: &str =
    include_str!("../../templates/permissions/approval_policy/on_request.md");
const APPROVAL_POLICY_NEVER_ZH_CN: &str =
    include_str!("../../templates/permissions/approval_policy/never.zh-CN.md");
const APPROVAL_POLICY_UNLESS_TRUSTED_ZH_CN: &str =
    include_str!("../../templates/permissions/approval_policy/unless_trusted.zh-CN.md");
const APPROVAL_POLICY_ON_REQUEST_ZH_CN: &str =
    include_str!("../../templates/permissions/approval_policy/on_request.zh-CN.md");

pub(crate) const DANGER_FULL_ACCESS_TEMPLATE: &str =
    include_str!("../../templates/permissions/sandbox_mode/danger_full_access.md");
pub(crate) const WORKSPACE_WRITE_TEMPLATE: &str =
    include_str!("../../templates/permissions/sandbox_mode/workspace_write.md");
pub(crate) const READ_ONLY_TEMPLATE: &str =
    include_str!("../../templates/permissions/sandbox_mode/read_only.md");
pub(crate) const DANGER_FULL_ACCESS_TEMPLATE_ZH_CN: &str =
    include_str!("../../templates/permissions/sandbox_mode/danger_full_access.zh-CN.md");
pub(crate) const WORKSPACE_WRITE_TEMPLATE_ZH_CN: &str =
    include_str!("../../templates/permissions/sandbox_mode/workspace_write.zh-CN.md");
pub(crate) const READ_ONLY_TEMPLATE_ZH_CN: &str =
    include_str!("../../templates/permissions/sandbox_mode/read_only.zh-CN.md");
pub(crate) const WRITABLE_ROOT_TEMPLATE: &str = " The writable root is {{ roots }}.";
pub(crate) const WRITABLE_ROOTS_TEMPLATE: &str = " The writable roots are {{ roots }}.";
pub(crate) const WRITABLE_ROOT_TEMPLATE_ZH_CN: &str = " 可写根目录为 {{ roots }}。";
pub(crate) const WRITABLE_ROOTS_TEMPLATE_ZH_CN: &str = " 可写根目录为 {{ roots }}。";

/// Resolved approval-policy text; policy and tool selection remain with the consumer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolvedApprovalMessages<'a> {
    pub(crate) language: PromptLanguage,
    pub(crate) never: ResolvedMessage<'a>,
    pub(crate) on_request: ResolvedMessage<'a>,
    pub(crate) on_request_auto_review: ResolvedMessage<'a>,
    pub(crate) unless_trusted: ResolvedMessage<'a>,
}

impl<'a> ResolvedApprovalMessages<'a> {
    pub(crate) fn new(messages: Option<&'a ApprovalMessages>) -> Self {
        Self::new_with_language(messages, PromptLanguage::English)
    }

    pub(crate) fn new_with_language(
        messages: Option<&'a ApprovalMessages>,
        language: PromptLanguage,
    ) -> Self {
        let (never, on_request, unless_trusted) = match language {
            PromptLanguage::English => (
                APPROVAL_POLICY_NEVER,
                APPROVAL_POLICY_ON_REQUEST,
                APPROVAL_POLICY_UNLESS_TRUSTED,
            ),
            PromptLanguage::Chinese => (
                APPROVAL_POLICY_NEVER_ZH_CN,
                APPROVAL_POLICY_ON_REQUEST_ZH_CN,
                APPROVAL_POLICY_UNLESS_TRUSTED_ZH_CN,
            ),
        };
        Self {
            language,
            never: ResolvedMessage::new(
                messages.and_then(|m| m.never.as_deref()),
                never,
            ),
            on_request: ResolvedMessage::new(
                messages.and_then(|m| m.on_request.as_deref()),
                on_request,
            ),
            on_request_auto_review: ResolvedMessage::new(
                messages.and_then(|m| m.on_request_auto_review.as_deref()),
                on_request,
            ),
            unless_trusted: ResolvedMessage::new(
                messages.and_then(|m| m.unless_trusted.as_deref()),
                unless_trusted,
            ),
        }
    }
}

/// Permission text retains its source so renderers can distinguish catalog text from templates.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolvedPermissionMessages<'a> {
    pub(crate) language: PromptLanguage,
    pub(crate) approved_prefixes: ResolvedMessage<'a>,
    pub(crate) danger_full_access: ResolvedMessage<'a>,
    pub(crate) workspace_write: ResolvedMessage<'a>,
    pub(crate) read_only: ResolvedMessage<'a>,
    pub(crate) writable_root: ResolvedMessage<'a>,
    pub(crate) writable_roots: ResolvedMessage<'a>,
}

impl<'a> ResolvedPermissionMessages<'a> {
    pub(crate) fn new(messages: Option<&'a PermissionMessages>) -> Self {
        Self::new_with_language(messages, PromptLanguage::English)
    }

    pub(crate) fn new_with_language(
        messages: Option<&'a PermissionMessages>,
        language: PromptLanguage,
    ) -> Self {
        let (
            approved_prefixes,
            danger_full_access,
            workspace_write,
            read_only,
            writable_root,
            writable_roots,
        ) = match language {
            PromptLanguage::English => (
                include_str!("../../templates/permissions/approved_prefixes.md").trim_end(),
                DANGER_FULL_ACCESS_TEMPLATE,
                WORKSPACE_WRITE_TEMPLATE,
                READ_ONLY_TEMPLATE,
                WRITABLE_ROOT_TEMPLATE,
                WRITABLE_ROOTS_TEMPLATE,
            ),
            PromptLanguage::Chinese => (
                include_str!("../../templates/permissions/approved_prefixes.zh-CN.md").trim_end(),
                DANGER_FULL_ACCESS_TEMPLATE_ZH_CN,
                WORKSPACE_WRITE_TEMPLATE_ZH_CN,
                READ_ONLY_TEMPLATE_ZH_CN,
                WRITABLE_ROOT_TEMPLATE_ZH_CN,
                WRITABLE_ROOTS_TEMPLATE_ZH_CN,
            ),
        };
        Self {
            language,
            approved_prefixes: ResolvedMessage::new(
                messages.and_then(|m| m.approved_prefixes.as_deref()),
                approved_prefixes,
            ),
            danger_full_access: ResolvedMessage::new(
                messages.and_then(|m| m.danger_full_access.as_deref()),
                danger_full_access,
            ),
            workspace_write: ResolvedMessage::new(
                messages.and_then(|m| m.workspace_write.as_deref()),
                workspace_write,
            ),
            read_only: ResolvedMessage::new(
                messages.and_then(|m| m.read_only.as_deref()),
                read_only,
            ),
            writable_root: ResolvedMessage::new(
                messages.and_then(|m| m.writable_root.as_deref()),
                writable_root,
            ),
            writable_roots: ResolvedMessage::new(
                messages.and_then(|m| m.writable_roots.as_deref()),
                writable_roots,
            ),
        }
    }
}
