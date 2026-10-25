//! Resolves Guardian policy, feedback, and classifier text from catalog values or bundled defaults.
//! Configuration overlays and prompt composition remain with consumers.

use codex_protocol::openai_models::AutoReviewMessages;
use codex_protocol::openai_models::ModelMessages;
use crate::PromptLanguage;

const POLICY: &str = include_str!("../../templates/guardian/policy.md");
const POLICY_TEMPLATE: &str = include_str!("../../templates/guardian/policy_template.md");
const NODE_REPL_POLICY: &str = include_str!("../../templates/guardian/node_repl_policy.md");
const REJECTION_INSTRUCTIONS: &str = concat!(
    "The agent must not attempt to achieve the same outcome via workaround, ",
    "indirect execution, or policy circumvention. ",
    "Proceed only with a materially safer alternative, ",
    "or if the user explicitly approves the action after being informed of the risk. ",
    "Otherwise, stop and request user input.",
);
const REJECTION_INSTRUCTIONS_ZH_CN: &str = concat!(
    "代理不得通过变通方案、间接执行或绕过策略来达到相同结果。",
    "只能采用明显更安全的替代方案，或在向用户说明风险并获得明确批准后继续。",
    "否则应停止并请求用户输入。",
);
const TIMEOUT_INSTRUCTIONS: &str = concat!(
    "The automatic permission approval review did not finish before its deadline. ",
    "Do not assume the action is unsafe based on the timeout alone. ",
    "You may retry once, or ask the user for guidance or explicit approval.",
);
const TIMEOUT_INSTRUCTIONS_ZH_CN: &str = concat!(
    "自动权限审批未在期限内完成。",
    "不要仅因超时就认定操作不安全。",
    "可以重试一次，或向用户询问指引或明确批准。",
);
const CLASSIFIER_INSTRUCTIONS: &str =
    include_str!("../../templates/guardian/classifier_instructions.md");
const CLASSIFIER_INSTRUCTIONS_ZH_CN: &str =
    include_str!("../../templates/guardian/classifier_instructions.zh-CN.md");

/// Auto-review policy and feedback text before runtime configuration overlays and rendering.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedAutoReviewMessages<'a> {
    pub policy: &'a str,
    pub policy_template: &'a str,
    pub node_repl_policy: &'a str,
    pub rejection_instructions: &'a str,
    pub timeout_instructions: &'a str,
}

impl<'a> ResolvedAutoReviewMessages<'a> {
    pub(crate) fn new(
        messages: Option<&'a AutoReviewMessages>,
        language: PromptLanguage,
    ) -> Self {
        let (rejection_instructions, timeout_instructions) = match language {
            PromptLanguage::English => (REJECTION_INSTRUCTIONS, TIMEOUT_INSTRUCTIONS),
            PromptLanguage::Chinese => (REJECTION_INSTRUCTIONS_ZH_CN, TIMEOUT_INSTRUCTIONS_ZH_CN),
        };
        Self {
            policy: messages
                .and_then(|messages| messages.policy.as_deref())
                .unwrap_or(POLICY),
            policy_template: messages
                .and_then(|messages| messages.policy_template.as_deref())
                .unwrap_or(POLICY_TEMPLATE),
            node_repl_policy: messages
                .and_then(|messages| messages.node_repl_policy.as_deref())
                .unwrap_or(NODE_REPL_POLICY),
            rejection_instructions: messages
                .and_then(|messages| messages.rejection_instructions.as_deref())
                .unwrap_or(rejection_instructions),
            timeout_instructions: messages
                .and_then(|messages| messages.timeout_instructions.as_deref())
                .unwrap_or(timeout_instructions),
        }
    }
}

pub(crate) fn classifier_instructions(
    messages: Option<&ModelMessages>,
    language: PromptLanguage,
) -> &str {
    let bundled = match language {
        PromptLanguage::English => CLASSIFIER_INSTRUCTIONS,
        PromptLanguage::Chinese => CLASSIFIER_INSTRUCTIONS_ZH_CN,
    };
    messages
        .and_then(|messages| messages.guardian_v2.as_ref())
        .and_then(|messages| messages.classifier_instructions.as_deref())
        .unwrap_or(bundled)
}
