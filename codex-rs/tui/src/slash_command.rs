use codex_config::types::TuiLanguage;
use strum::IntoEnumIterator;
use strum_macros::AsRefStr;
use strum_macros::EnumIter;
use strum_macros::EnumString;
use strum_macros::IntoStaticStr;

use crate::i18n;

/// Commands that can be invoked by starting a message with a leading slash.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, EnumIter, AsRefStr, IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum SlashCommand {
    // DO NOT ALPHA-SORT! Enum order is presentation order in the popup, so
    // more frequently used commands should be listed first.
    Model,
    Presets,
    Daybreak,
    Ide,
    Permissions,
    Keymap,
    Vim,
    #[strum(serialize = "setup-default-sandbox")]
    ElevateSandbox,
    Experimental,
    #[strum(to_string = "approve")]
    AutoReview,
    Memories,
    Skills,
    Tools,
    Import,
    Hooks,
    Review,
    Rename,
    New,
    Archive,
    Delete,
    Resume,
    Fork,
    Worktree,
    Projects,
    App,
    Init,
    Compact,
    Config,
    Recap,
    Plan,
    Voice,
    Goal,
    Agents,
    Side,
    Btw,
    Copy,
    Export,
    Raw,
    Tui,
    Budget,
    Pin,
    Verify,
    Handoff,
    Diff,
    Files,
    Checkpoint,
    Rewind,
    Mention,
    Status,
    Daemon,
    Warnings,
    Cd,
    #[strum(to_string = "pwd", serialize = "cwd")]
    Pwd,
    Usage,
    DebugConfig,
    Title,
    Statusline,
    Theme,
    Language,
    Provider,
    Mcp,
    Apps,
    Plugins,
    Quit,
    Exit,
    Rollout,
    Ps,
    #[strum(to_string = "stop", serialize = "clean")]
    Stop,
    Clear,
    TestApproval,
    #[strum(serialize = "subagents")]
    MultiAgents,
    // Debugging commands.
    #[strum(serialize = "debug-m-drop")]
    MemoryDrop,
    #[strum(serialize = "debug-m-update")]
    MemoryUpdate,
}

impl SlashCommand {
    /// User-visible description shown in the popup.
    pub fn description(self) -> &'static str {
        self.description_for(TuiLanguage::English)
    }

    /// User-visible description shown in the popup for an interface language.
    pub fn description_for(self, language: TuiLanguage) -> &'static str {
        if i18n::is_chinese(language) {
            return match self {
                SlashCommand::Config => "配置 Codex 偏好",
                SlashCommand::New => "在当前对话中开始新对话",
                SlashCommand::Init => "创建包含 Codex 指令的 AGENTS.md",
                SlashCommand::Compact => "压缩对话，避免超出上下文限制",
                SlashCommand::Recap => "立即总结当前对话",
                SlashCommand::Review => "检查当前改动并找出问题",
                SlashCommand::Rename => "重命名当前对话",
                SlashCommand::Resume => "继续已保存的对话",
                SlashCommand::Archive => "归档当前会话",
                SlashCommand::Delete => "永久删除当前会话",
                SlashCommand::Clear => "清空终端并开始新对话",
                SlashCommand::Fork => "复制当前对话",
                SlashCommand::Worktree => "在新的 worktree 中开始或继续对话",
                SlashCommand::Projects => "查看多项目工作区并浏览项目文件",
                SlashCommand::App => "在桌面应用中继续当前会话",
                SlashCommand::Quit | SlashCommand::Exit => "退出 Codex",
                SlashCommand::Copy => "复制上一条回复或其部分内容",
                SlashCommand::Export => "将对话导出为 Markdown",
                SlashCommand::Raw => "切换原始回滚模式，便于复制终端文本",
                SlashCommand::Tui => "选择下次启动时的 TUI 模式",
                SlashCommand::Budget => "查看上下文 token 预算并压缩会话",
                SlashCommand::Pin => "固定文本、文件或行范围到后续请求",
                SlashCommand::Verify => "检测并运行项目验证命令",
                SlashCommand::Handoff => "生成包含上下文、验证和 Git 状态的任务交接",
                SlashCommand::Diff => "显示 Git 差异（包括未跟踪文件）",
                SlashCommand::Files => "浏览文件并选择附加给 AI 的行范围",
                SlashCommand::Checkpoint => "保存当前工作区检查点",
                SlashCommand::Rewind => "查看并恢复工作区检查点",
                SlashCommand::Mention => "提及文件",
                SlashCommand::Skills => "使用 Skills 改善 Codex 的特定任务表现",
                SlashCommand::Tools => "查看和管理本地自定义工具",
                SlashCommand::Import => "从 Claude Code 导入设置、项目和最近对话",
                SlashCommand::Hooks => "查看和管理生命周期钩子",
                SlashCommand::Daemon => "管理本地后台服务",
                SlashCommand::Warnings => "查看保留的警告和诊断详情",
                SlashCommand::Status => "显示当前会话配置和 token 用量",
                SlashCommand::Cd => "切换当前工作目录",
                SlashCommand::Pwd => "显示当前工作目录",
                SlashCommand::Usage => "查看账户用量或重置用量限制",
                SlashCommand::DebugConfig => "显示配置层和配置来源，用于调试",
                SlashCommand::Title => "配置终端标题显示的内容",
                SlashCommand::Statusline => "配置状态栏显示的内容",
                SlashCommand::Theme => "选择语法高亮主题",
                SlashCommand::Language => "选择界面与模型回复语言",
                SlashCommand::Provider => "管理第三方模型服务配置",
                SlashCommand::Ps => "列出后台终端",
                SlashCommand::Stop => "停止所有后台终端",
                SlashCommand::MemoryDrop | SlashCommand::MemoryUpdate => "请勿使用",
                SlashCommand::Model => "选择模型和思考强度",
                SlashCommand::Presets => "运行或保存可复用的提示词和斜杠命令",
                SlashCommand::Daybreak => "开启或关闭 Daybreak",
                SlashCommand::Ide => "引入 IDE 中的选区、打开文件和其他上下文",
                SlashCommand::Plan => "切换到计划模式",
                SlashCommand::Voice => "开始或停止语音；使用 /voice settings 选择声音",
                SlashCommand::Goal => "设置或查看长任务的 Goal",
                SlashCommand::Agents => "打开 Agent 命令中心",
                SlashCommand::MultiAgents => "在当前会话的子 Agent 之间切换",
                SlashCommand::Side | SlashCommand::Btw => "在临时分支中开始旁支对话",
                SlashCommand::Permissions => "选择 Codex 可以执行的操作",
                SlashCommand::Keymap => "重新映射 TUI 快捷键",
                SlashCommand::Vim => "切换输入区的 Vim 模式",
                SlashCommand::ElevateSandbox => "设置提权后的 Agent 沙箱",
                SlashCommand::Experimental => "切换实验功能",
                SlashCommand::AutoReview => "批准最近一次自动审查拒绝的重试",
                SlashCommand::Memories => "配置记忆的使用和生成",
                SlashCommand::Mcp => "列出 MCP 工具；使用 /mcp verbose 或 /mcp login <name>",
                SlashCommand::Apps => "管理应用",
                SlashCommand::Plugins => "浏览插件",
                SlashCommand::Rollout => "打印 rollout 文件路径",
                SlashCommand::TestApproval => "测试审批请求",
            };
        }
        match self {
            SlashCommand::Config => "configure Codex preferences",
            SlashCommand::New => "start a new chat during a conversation",
            SlashCommand::Init => "create an AGENTS.md file with instructions for Codex",
            SlashCommand::Compact => "summarize conversation to prevent hitting the context limit",
            SlashCommand::Recap => "summarize the current conversation now",
            SlashCommand::Review => "review my current changes and find issues",
            SlashCommand::Rename => "rename the current thread",
            SlashCommand::Resume => "resume a saved chat",
            SlashCommand::Archive => "archive this session",
            SlashCommand::Delete => "permanently delete this session",
            SlashCommand::Clear => "clear the terminal and start a new chat",
            SlashCommand::Fork => "fork the current chat",
            SlashCommand::Worktree => "start or continue a conversation in a new worktree",
            SlashCommand::Projects => "inspect the multi-project workspace and browse project files",
            SlashCommand::App => "continue this session in the Desktop app",
            SlashCommand::Quit | SlashCommand::Exit => "exit Codex",
            SlashCommand::Copy => "copy the last response or part of it",
            SlashCommand::Export => "export the conversation as markdown",
            SlashCommand::Raw => "toggle raw scrollback mode for copy-friendly terminal selection",
            SlashCommand::Tui => "choose the TUI mode for the next launch",
            SlashCommand::Budget => "inspect the context token budget and compact",
            SlashCommand::Pin => "pin text, files, or line ranges into later requests",
            SlashCommand::Verify => "detect and run project verification commands",
            SlashCommand::Handoff => {
                "generate a task handoff with context, verification, and git state"
            }
            SlashCommand::Diff => "show git diff (including untracked files)",
            SlashCommand::Files => "browse files and attach selected line ranges",
            SlashCommand::Checkpoint => "save the current workspace checkpoint",
            SlashCommand::Rewind => "review and restore workspace checkpoints",
            SlashCommand::Mention => "mention a file",
            SlashCommand::Skills => "use skills to improve how Codex performs specific tasks",
            SlashCommand::Tools => "inspect and manage local custom tools",
            SlashCommand::Import => "import setup, this project, and recent chats from Claude Code",
            SlashCommand::Hooks => "view and manage lifecycle hooks",
            SlashCommand::Daemon => "Manage the local background server",
            SlashCommand::Warnings => "view retained warnings and diagnostic details",
            SlashCommand::Status => "show current session configuration and token usage",
            SlashCommand::Cd => "change the current working directory",
            SlashCommand::Pwd => "show the current working directory",
            SlashCommand::Usage => "view account usage or use a usage limit reset",
            SlashCommand::DebugConfig => "show config layers and requirement sources for debugging",
            SlashCommand::Title => "configure which items appear in the terminal title",
            SlashCommand::Statusline => "configure which items appear in the status line",
            SlashCommand::Theme => "choose a syntax highlighting theme",
            SlashCommand::Language => "choose interface and model response language",
            SlashCommand::Provider => "manage third-party model provider profiles",
            SlashCommand::Ps => "list background terminals",
            SlashCommand::Stop => "stop all background terminals",
            SlashCommand::MemoryDrop => "DO NOT USE",
            SlashCommand::MemoryUpdate => "DO NOT USE",
            SlashCommand::Model => "choose what model and reasoning effort to use",
            SlashCommand::Presets => "run or save reusable prompts and slash commands",
            SlashCommand::Daybreak => "turn Daybreak on or off",
            SlashCommand::Ide => {
                "include current selection, open files, and other context from your IDE"
            }
            SlashCommand::Plan => "switch to Plan mode",
            SlashCommand::Voice => "start or stop voice; use /voice settings to choose a voice",
            SlashCommand::Goal => "set or view the goal for a long-running task",
            SlashCommand::Agents => "open the agent command center",
            SlashCommand::MultiAgents => "switch between this session's subagents",
            SlashCommand::Side | SlashCommand::Btw => {
                "start a side conversation in an ephemeral fork"
            }
            SlashCommand::Permissions => "choose what Codex is allowed to do",
            SlashCommand::Keymap => "remap TUI shortcuts",
            SlashCommand::Vim => "toggle Vim mode for the composer",
            SlashCommand::ElevateSandbox => "set up elevated agent sandbox",
            SlashCommand::Experimental => "toggle experimental features",
            SlashCommand::AutoReview => "approve one retry of a recent auto-review denial",
            SlashCommand::Memories => "configure memory use and generation",
            SlashCommand::Mcp => "list MCP tools; use /mcp verbose or /mcp login <name>",
            SlashCommand::Apps => "manage apps",
            SlashCommand::Plugins => "browse plugins",
            SlashCommand::Rollout => "print the rollout file path",
            SlashCommand::TestApproval => "test approval request",
        }
    }

    /// Command string without the leading '/'. Provided for compatibility with
    /// existing code that expects a method named `command()`.
    pub fn command(self) -> &'static str {
        self.into()
    }

    /// Whether this command supports inline args (for example `/review ...`).
    pub fn supports_inline_args(self) -> bool {
        matches!(
            self,
            SlashCommand::Review
                | SlashCommand::Model
                | SlashCommand::Permissions
                | SlashCommand::Title
                | SlashCommand::Statusline
                | SlashCommand::Rename
                | SlashCommand::New
                | SlashCommand::Clear
                | SlashCommand::Fork
                | SlashCommand::Plan
                | SlashCommand::Goal
                | SlashCommand::Voice
                | SlashCommand::Ide
                | SlashCommand::Keymap
                | SlashCommand::Mcp
                | SlashCommand::Export
                | SlashCommand::Raw
                | SlashCommand::Cd
                | SlashCommand::Pwd
                | SlashCommand::Usage
                | SlashCommand::Language
                | SlashCommand::Provider
                | SlashCommand::Side
                | SlashCommand::Btw
                | SlashCommand::Resume
                | SlashCommand::Files
                | SlashCommand::Checkpoint
                | SlashCommand::Pin
                | SlashCommand::Verify
                | SlashCommand::Handoff
                | SlashCommand::Tools
        )
    }

    /// Whether this command remains available inside an active side conversation.
    pub fn available_in_side_conversation(self) -> bool {
        matches!(
            self,
            SlashCommand::Copy
                | SlashCommand::Agents
                | SlashCommand::Export
                | SlashCommand::Raw
                | SlashCommand::Diff
                | SlashCommand::Files
                | SlashCommand::Mention
                | SlashCommand::Projects
                | SlashCommand::Status
                | SlashCommand::Daemon
                | SlashCommand::Warnings
                | SlashCommand::Pwd
                | SlashCommand::Usage
                | SlashCommand::Ide
                | SlashCommand::Language
                | SlashCommand::Provider
        )
    }

    /// Whether dispatch needs thread state to validate this command before consuming its draft.
    /// The composer must defer busy-state rejection and draft clearing for these commands.
    pub(crate) fn requires_dispatch_validation(self) -> bool {
        matches!(self, SlashCommand::Review)
    }

    /// Commands that do not require a writable current thread. The server must still be connected.
    pub(crate) fn available_when_thread_unavailable(self) -> bool {
        matches!(
            self,
            SlashCommand::New
                | SlashCommand::Clear
                | SlashCommand::Resume
                | SlashCommand::Agents
                | SlashCommand::MultiAgents
                | SlashCommand::Quit
                | SlashCommand::Exit
                | SlashCommand::Status
                | SlashCommand::Warnings
                | SlashCommand::DebugConfig
                | SlashCommand::Pwd
                | SlashCommand::Rollout
                | SlashCommand::Copy
                | SlashCommand::Raw
                | SlashCommand::Language
                | SlashCommand::Provider
                | SlashCommand::Budget
                | SlashCommand::Pin
                | SlashCommand::Verify
                | SlashCommand::Tools
                | SlashCommand::Files
                | SlashCommand::Projects
                | SlashCommand::Checkpoint
                | SlashCommand::Rewind
        )
    }

    /// Whether this command can be run while a task is in progress.
    pub fn available_during_task(self) -> bool {
        match self {
            SlashCommand::New
            | SlashCommand::Delete
            | SlashCommand::Fork
            | SlashCommand::Worktree
            | SlashCommand::Init
            | SlashCommand::Compact
            | SlashCommand::Recap
            | SlashCommand::Export
            | SlashCommand::Keymap
            | SlashCommand::Tui
            | SlashCommand::Vim
            | SlashCommand::ElevateSandbox
            | SlashCommand::Experimental
            | SlashCommand::Memories
            | SlashCommand::Import
            | SlashCommand::Review
            | SlashCommand::Plan
            | SlashCommand::Cd
            | SlashCommand::Clear
            | SlashCommand::Budget
            | SlashCommand::Verify
            | SlashCommand::Files
            | SlashCommand::Checkpoint
            | SlashCommand::Rewind
            | SlashCommand::MemoryDrop
            | SlashCommand::MemoryUpdate => false,
            SlashCommand::Diff
            | SlashCommand::Archive
            | SlashCommand::Resume
            | SlashCommand::Model
            | SlashCommand::Presets
            | SlashCommand::Daybreak
            | SlashCommand::Permissions
            | SlashCommand::Copy
            | SlashCommand::Raw
            | SlashCommand::Rename
            | SlashCommand::Mention
            | SlashCommand::Projects
            | SlashCommand::Skills
            | SlashCommand::Tools
            | SlashCommand::Hooks
            | SlashCommand::Status
            | SlashCommand::Daemon
            | SlashCommand::Warnings
            | SlashCommand::Pwd
            | SlashCommand::Usage
            | SlashCommand::DebugConfig
            | SlashCommand::Ps
            | SlashCommand::Stop
            | SlashCommand::App
            | SlashCommand::Goal
            | SlashCommand::Voice
            | SlashCommand::Mcp
            | SlashCommand::Apps
            | SlashCommand::Plugins
            | SlashCommand::Title
            | SlashCommand::Statusline
            | SlashCommand::AutoReview
            | SlashCommand::Ide
            | SlashCommand::Pin
            | SlashCommand::Language
            | SlashCommand::Provider
            | SlashCommand::Quit
            | SlashCommand::Exit
            | SlashCommand::Side
            | SlashCommand::Btw
            | SlashCommand::Handoff => true,
            SlashCommand::Rollout => true,
            SlashCommand::TestApproval => true,
            SlashCommand::Agents | SlashCommand::MultiAgents => true,
            SlashCommand::Config | SlashCommand::Theme => false,
        }
    }

    fn is_visible(self) -> bool {
        match self {
            SlashCommand::Copy => !cfg!(target_os = "android"),
            SlashCommand::App => cfg!(any(target_os = "macos", target_os = "windows")),
            SlashCommand::Voice => true,
            SlashCommand::Rollout | SlashCommand::TestApproval => cfg!(debug_assertions),
            _ => true,
        }
    }
}

/// Return all built-in commands in a Vec paired with their command string.
pub fn built_in_slash_commands() -> Vec<(&'static str, SlashCommand)> {
    SlashCommand::iter()
        .filter(|command| command.is_visible())
        .map(|c| (c.command(), c))
        .collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use std::str::FromStr;

    use super::SlashCommand;

    #[test]
    fn stop_command_is_canonical_name() {
        assert_eq!(SlashCommand::Stop.command(), "stop");
    }

    #[test]
    fn clean_alias_parses_to_stop_command() {
        assert_eq!(SlashCommand::from_str("clean"), Ok(SlashCommand::Stop));
    }

    #[test]
    fn certain_commands_are_available_during_task() {
        assert!(SlashCommand::Goal.available_during_task());
        assert!(SlashCommand::Ide.available_during_task());
        assert!(SlashCommand::Title.available_during_task());
        assert!(SlashCommand::Statusline.available_during_task());
        assert!(SlashCommand::Raw.available_during_task());
        assert!(SlashCommand::Raw.available_in_side_conversation());
        assert!(SlashCommand::Raw.supports_inline_args());
        assert!(SlashCommand::App.available_during_task());
    }

    #[test]
    fn auto_review_command_is_approve() {
        assert_eq!(SlashCommand::AutoReview.command(), "approve");
        assert_eq!(
            SlashCommand::from_str("approve"),
            Ok(SlashCommand::AutoReview)
        );
    }
}
