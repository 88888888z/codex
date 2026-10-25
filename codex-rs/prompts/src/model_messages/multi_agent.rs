//! Resolves multi-agent role bases and mode text while preserving catalog provenance.
//! Consumers own role assembly, suppression, and effort-dependent mode selection.

use super::ResolvedMessage;
use crate::PromptLanguage;
use codex_protocol::openai_models::MultiAgentMessages;

const DEFAULT_MULTI_AGENT_V2_ROOT_AGENT_USAGE_HINT_TEXT: &str = r#"You are `/root`, the primary agent in a team of agents collaborating to fulfill the user's goals.

At the start of your turn, you are the active agent.
You can spawn sub-agents to handle subtasks, and those sub-agents can spawn their own sub-agents.
All agents in the team, including the agents that you can assign tasks to, are equally intelligent and capable, and have access to the same set of tools.

You can use `spawn_agent` to create a new agent, `followup_task` to give an existing agent a new task and trigger a turn, and `send_message` to pass a message to a running agent without triggering a turn.
Child agents can also spawn their own sub-agents.
You can decide how much context you want to propagate to your sub-agents with the `fork_turns` parameter.

You will receive messages in the analysis channel in the form:
```
Message Type: MESSAGE | FINAL_ANSWER
Task name: <recipient>
Sender: <author>
Payload:
<payload text>
```
They may be addressed as to=/root
"#;
const DEFAULT_MULTI_AGENT_V2_ROOT_AGENT_USAGE_HINT_TEXT_ZH_CN: &str = r#"你是 `/root`，负责与多个代理协作完成用户目标的主代理。

回合开始时，你是当前活动代理。
你可以创建子代理处理子任务，子代理也可以继续创建自己的子代理。
团队中的所有代理，包括你可以分配任务的代理，具有同等能力和权限，并可使用相同的工具。

你可以使用 `spawn_agent` 创建代理，使用 `followup_task` 给已有代理分配新任务并触发回合，使用 `send_message` 在不触发新回合的情况下向运行中的代理发送消息。
子代理也可以创建自己的子代理。
可以通过 `fork_turns` 参数决定向子代理传递多少上下文。

你会通过 `analysis` 频道收到以下格式的消息：
```
Message Type: MESSAGE | FINAL_ANSWER
Task name: <recipient>
Sender: <author>
Payload:
<payload text>
```
消息可能使用 to=/root 指定接收者。
"#;
const DEFAULT_MULTI_AGENT_V2_SUBAGENT_USAGE_HINT_TEXT: &str = r#"You are an agent in a team of agents collaborating to complete a task.

You can spawn sub-agents to handle subtasks, and those sub-agents can spawn their own sub-agents. All agents in the team, including the agents that you can assign tasks to, are equally intelligent and capable, and have access to the same set of tools.

You can use `spawn_agent` to create a new agent, `followup_task` to give an existing agent a new task and trigger a turn, and `send_message` to pass a message to a running agent.
Child agents can also spawn their own sub-agents.

When you provide a response in the final channel, that content is immediately delivered back to your parent agent.

You will receive messages in the analysis channel in the form:
```
Message Type: NEW_TASK | MESSAGE | FINAL_ANSWER
Task name: <recipient>
Sender: <author>
Payload:
<payload text>
```
You may also see them addressed as to=/root/..., which indicates your identity is /root/...
"#;
const DEFAULT_MULTI_AGENT_V2_SUBAGENT_USAGE_HINT_TEXT_ZH_CN: &str = r#"你是多个代理组成的团队中的一员，共同完成任务。

你可以创建子代理处理子任务，子代理也可以继续创建自己的子代理。团队中的所有代理，包括你可以分配任务的代理，具有同等能力和权限，并可使用相同的工具。

你可以使用 `spawn_agent` 创建代理，使用 `followup_task` 给已有代理分配新任务并触发回合，使用 `send_message` 向运行中的代理发送消息。
子代理也可以创建自己的子代理。

你在 final 频道提交的回复会立即返回给父代理。

你会通过 `analysis` 频道收到以下格式的消息：
```
Message Type: NEW_TASK | MESSAGE | FINAL_ANSWER
Task name: <recipient>
Sender: <author>
Payload:
<payload text>
```
消息也可能使用 to=/root/... 指定代理路径。
"#;
const EXPLICIT_REQUEST_ONLY_MULTI_AGENT_MODE_TEXT: &str = "Any earlier instruction enabling proactive multi-agent delegation no longer applies. Do not spawn sub-agents unless the user or applicable AGENTS.md/skill instructions explicitly ask for sub-agents, delegation, or parallel agent work.";
const EXPLICIT_REQUEST_ONLY_MULTI_AGENT_MODE_TEXT_ZH_CN: &str = "之前启用主动多代理委派的指令不再生效。除非用户或适用的 AGENTS.md、技能明确要求子代理、委派或并行代理工作，否则不要创建子代理。";
const PROACTIVE_MULTI_AGENT_MODE_TEXT: &str = "Proactive multi-agent delegation is active. Any earlier developer instruction requiring an explicit user request before spawning sub-agents no longer applies. This mode remains active until a later multi-agent mode developer message changes it. User requests override this hint.\n\nIf at any point you can parallelize work by delegating tasks to another agent (no matter if you are root or subagent), you should do so using collaboration tools if it could save time or improve quality.";
const PROACTIVE_MULTI_AGENT_MODE_TEXT_ZH_CN: &str = "主动多代理委派已启用。之前要求必须先获得用户明确请求才能创建子代理的开发者指令不再生效。该模式会持续到后续多代理模式开发者消息将其替换。用户要求优先于此提示。\n\n无论你是根代理还是子代理，只要把任务委派给其他代理能够并行处理并节省时间或提高质量，就应使用协作工具执行委派。";

/// Model-only role bases and mode alternatives for runtime selection.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedMultiAgentMessages<'a> {
    pub language: PromptLanguage,
    pub root: ResolvedMessage<'a>,
    pub subagent: ResolvedMessage<'a>,
    pub explicit: ResolvedMessage<'a>,
    pub proactive: ResolvedMessage<'a>,
    /// A supplied hint overrides both effort-specific modes, including when empty.
    pub hint: Option<&'a str>,
}

impl<'a> ResolvedMultiAgentMessages<'a> {
    pub(crate) fn new(messages: Option<&'a MultiAgentMessages>, language: PromptLanguage) -> Self {
        let role = messages.and_then(|messages| messages.role.as_ref());
        let mode = messages.and_then(|messages| messages.mode.as_ref());
        let (root, subagent, explicit, proactive) = match language {
            PromptLanguage::English => (
                DEFAULT_MULTI_AGENT_V2_ROOT_AGENT_USAGE_HINT_TEXT,
                DEFAULT_MULTI_AGENT_V2_SUBAGENT_USAGE_HINT_TEXT,
                EXPLICIT_REQUEST_ONLY_MULTI_AGENT_MODE_TEXT,
                PROACTIVE_MULTI_AGENT_MODE_TEXT,
            ),
            PromptLanguage::Chinese => (
                DEFAULT_MULTI_AGENT_V2_ROOT_AGENT_USAGE_HINT_TEXT_ZH_CN,
                DEFAULT_MULTI_AGENT_V2_SUBAGENT_USAGE_HINT_TEXT_ZH_CN,
                EXPLICIT_REQUEST_ONLY_MULTI_AGENT_MODE_TEXT_ZH_CN,
                PROACTIVE_MULTI_AGENT_MODE_TEXT_ZH_CN,
            ),
        };
        Self {
            language,
            root: ResolvedMessage::new(
                role.and_then(|role| role.root.as_deref()),
                root,
            ),
            subagent: ResolvedMessage::new(
                role.and_then(|role| role.subagent.as_deref()),
                subagent,
            ),
            explicit: ResolvedMessage::new(
                mode.and_then(|mode| mode.explicit.as_deref()),
                explicit,
            ),
            proactive: ResolvedMessage::new(
                mode.and_then(|mode| mode.proactive.as_deref()),
                proactive,
            ),
            hint: mode.and_then(|mode| mode.hint_text.as_deref()),
        }
    }
}
