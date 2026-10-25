use codex_protocol::protocol::ENVIRONMENTS_INSTRUCTIONS_CLOSE_TAG;
use codex_protocol::protocol::ENVIRONMENTS_INSTRUCTIONS_OPEN_TAG;

use super::ContextualUserFragment;
use codex_prompts::PromptLanguage;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::input_text;

pub(crate) struct EnvironmentsInstructions {
    pub(crate) language: PromptLanguage,
}

impl ContextualUserFragment for EnvironmentsInstructions {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("environments.instructions".to_string())
    }

    fn role(&self) -> &'static str {
        "developer"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        (
            ENVIRONMENTS_INSTRUCTIONS_OPEN_TAG,
            ENVIRONMENTS_INSTRUCTIONS_CLOSE_TAG,
        )
    }

    fn body(&self) -> ContentItem {
        if self.language == PromptLanguage::Chinese {
            return input_text(
                "\n## 执行环境\n\
执行环境是拥有独立文件、shell 和已安装能力的机器或工作区。`<environment_context>` 会列出当前任务选择的环境。\n\
\n\
标记为 `starting` 的环境尚不可用。启动完成后，其文件、命令、AGENTS.md 指令、技能、插件和 MCP 工具才可能可用。\n\
\n\
只有当前任务确实需要该环境时才等待。与它无关的工作应继续使用已经可用的工具。\n",
            );
        }
        input_text("\n## Execution environments\n\
Execution environments are separate machines or workspaces with their own files, shell, and installed capabilities. `<environment_context>` lists the environments selected for this task.\n\
\n\
An environment marked `starting` is not yet usable. Its files, commands, AGENTS.md instructions, skills, plugins, and MCP tools may become available when startup completes.\n\
\n\
Wait only when the current task needs that environment. Continue using tools that are already available for unrelated work.\n"
            .to_string())
    }
}
