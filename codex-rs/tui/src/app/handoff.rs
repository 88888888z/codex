//! Deterministic task handoff generation from visible conversation and workspace state.

use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use crate::app_event::HandoffDestination;
use crate::app_server_session::AppServerSession;
use crate::history_cell::AgentMarkdownCell;
use crate::history_cell::UserHistoryCell;
use crate::thread_transcript::RawReasoningVisibility;
use crate::workspace_command::WorkspaceCommand;

use super::App;
use super::transcript_export::load_export_transcript;

impl App {
    pub(super) async fn generate_handoff(
        &mut self,
        tui: &mut crate::tui::Tui,
        app_server: &mut AppServerSession,
        destination: HandoffDestination,
    ) -> Result<(), String> {
        let thread_id = self
            .chat_widget
            .thread_id()
            .ok_or_else(|| {
                crate::i18n::ui_text(
                    "No active conversation to hand off.",
                    "没有可生成交接的活动会话。",
                )
                .to_string()
            })?;
        let visibility = if self.config.show_raw_agent_reasoning {
            RawReasoningVisibility::Visible
        } else {
            RawReasoningVisibility::Hidden
        };
        let cells = load_export_transcript(
            app_server,
            thread_id,
            visibility,
            Some(&self.config),
            self.transcript_cells.clone(),
        )
        .await?;
        let cwd = self.chat_widget.config_ref().cwd.to_path_buf();
        let git_summary = self.handoff_git_summary(&cwd).await;
        let goal_summary = app_server
            .thread_goal_get(thread_id)
            .await
            .ok()
            .and_then(|response| response.goal)
            .map(|goal| {
                format!(
                    "{}\n\nStatus: {:?} · Tokens: {}",
                    goal.objective, goal.status, goal.tokens_used
                )
            })
            .unwrap_or_else(|| {
                crate::i18n::ui_text("No goal is set.", "未设置 Goal。").to_string()
            });
        let markdown = build_handoff_markdown(
            &cells,
            thread_id,
            &cwd,
            &goal_summary,
            &self.chat_widget.pinned_context_for_handoff(),
            &self.chat_widget.verification_summary(),
            &git_summary,
        );

        match destination {
            HandoffDestination::Clipboard => {
                let result = tui.copy_transcript_selection(
                    &markdown,
                    crate::clipboard_copy::CopyFormat::PlainText,
                );
                self.chat_widget.show_copy_result("handoff", result);
            }
            HandoffDestination::File(path) => {
                let path = write_handoff(&cwd, &path, &markdown)?;
                self.chat_widget.add_info_message(
                    if crate::i18n::is_chinese(crate::i18n::current_language()) {
                        format!("任务交接已保存到 {}", path.display())
                    } else {
                        format!("Saved handoff to {}", path.display())
                    },
                    /*hint*/ None,
                );
            }
            HandoffDestination::Default => {
                let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f");
                let path = self
                    .local_settings
                    .codex_home
                    .join("handoffs")
                    .join(format!("handoff-{stamp}.md"));
                let path = write_handoff(&cwd, &path, &markdown)?;
                self.chat_widget.add_info_message(
                    if crate::i18n::is_chinese(crate::i18n::current_language()) {
                        format!("任务交接已保存到 {}", path.display())
                    } else {
                        format!("Saved handoff to {}", path.display())
                    },
                    Some(
                        crate::i18n::ui_text(
                            "Use /handoff clipboard to copy it, or /handoff <path> to choose a file.",
                            "使用 /handoff clipboard 复制，或用 /handoff <路径> 指定文件。",
                        )
                        .to_string(),
                    ),
                );
            }
        }
        Ok(())
    }

    async fn handoff_git_summary(&self, cwd: &Path) -> String {
        let Some(runner) = self.workspace_command_runner.clone() else {
            return crate::i18n::ui_text(
                "Workspace command runner unavailable.",
                "工作区命令执行器不可用。",
            )
            .to_string();
        };
        let status = runner
            .run(
                WorkspaceCommand::new(["git", "status", "--short"], cwd)
                    .timeout(Duration::from_secs(/*secs*/ 15)),
            )
            .await;
        let stat = runner
            .run(
                WorkspaceCommand::new(["git", "diff", "--stat"], cwd)
                    .timeout(Duration::from_secs(/*secs*/ 15)),
            )
            .await;
        match (status, stat) {
            (Ok(status), Ok(stat)) if status.success() && stat.success() => {
                let status = status.stdout.trim();
                let stat = stat.stdout.trim();
                if status.is_empty() && stat.is_empty() {
                    crate::i18n::ui_text("Working tree clean.", "工作区干净。").to_string()
                } else {
                    format!(
                        "### {}\n\n```text\n{}\n```\n\n### {}\n\n```text\n{}\n```",
                        crate::i18n::ui_text("Status", "状态"),
                        if status.is_empty() {
                            crate::i18n::ui_text("(clean)", "（无变更）")
                        } else {
                            status
                        },
                        crate::i18n::ui_text("Diff stat", "差异统计"),
                        if stat.is_empty() {
                            crate::i18n::ui_text("(no tracked changes)", "（无已跟踪变更）")
                        } else {
                            stat
                        }
                    )
                }
            }
            (Err(error), _) | (_, Err(error)) => format!("Could not read Git state: {error}"),
            (Ok(status), Ok(stat)) => format!(
                "Git status exit {}\n{}\n\nGit diff exit {}\n{}",
                status.exit_code,
                status.stderr.trim(),
                stat.exit_code,
                stat.stderr.trim()
            ),
        }
    }
}

fn build_handoff_markdown(
    cells: &[std::sync::Arc<dyn crate::history_cell::HistoryCell>],
    thread_id: codex_protocol::ThreadId,
    cwd: &Path,
    goal: &str,
    pinned_context: &str,
    verification: &str,
    git_summary: &str,
) -> String {
    let mut user_messages = Vec::new();
    let mut agent_messages = Vec::new();
    for cell in cells {
        if let Some(user) = cell.as_any().downcast_ref::<UserHistoryCell>() {
            let (request, _) = crate::user_prompt::extract_prompt_request_with_offset(&user.message);
            if !request.trim().is_empty() {
                user_messages.push(truncate(request.trim(), 2_000));
            }
        } else if cell.as_any().is::<AgentMarkdownCell>() {
            let text = cell.copy_source().map_or_else(
                || {
                    cell.raw_lines()
                        .into_iter()
                        .map(|line| line.to_string())
                        .collect::<Vec<_>>()
                        .join("\n")
                },
                str::to_string,
            );
            if !text.trim().is_empty() {
                agent_messages.push(truncate(text.trim(), 4_000));
            }
        }
    }
    let user_messages = last_items(&user_messages, 6);
    let agent_messages = last_items(&agent_messages, 4);
    let mut markdown = format!(
        "{}\n\n- {}: `{thread_id}`\n- {}: `{}`\n- {}: {}\n",
        crate::i18n::ui_text("# Codex handoff", "# Codex 交接"),
        crate::i18n::ui_text("Thread", "会话"),
        crate::i18n::ui_text("Workspace", "工作区"),
        cwd.display(),
        crate::i18n::ui_text("Generated", "生成时间"),
        chrono::Utc::now().to_rfc3339()
    );
    markdown.push_str(&format!(
        "\n## {}\n\n",
        crate::i18n::ui_text("Goal", "目标")
    ));
    markdown.push_str(goal.trim());
    markdown.push_str(&format!(
        "\n\n## {}\n\n",
        crate::i18n::ui_text("Pinned context", "固定上下文")
    ));
    markdown.push_str(if pinned_context.trim().is_empty() {
        crate::i18n::ui_text("None.", "无。")
    } else {
        pinned_context.trim()
    });
    markdown.push_str(&format!(
        "\n\n## {}\n\n",
        crate::i18n::ui_text("Verification", "验证结果")
    ));
    markdown.push_str(verification.trim());
    markdown.push_str(&format!(
        "\n\n## {}\n\n",
        crate::i18n::ui_text("Workspace changes", "工作区变更")
    ));
    markdown.push_str(git_summary.trim());
    markdown.push_str(&format!(
        "\n\n## {}\n\n",
        crate::i18n::ui_text("Recent requests", "最近请求")
    ));
    for (index, message) in user_messages.iter().enumerate() {
        markdown.push_str(&format!("{}. {}\n", index + 1, message.replace('\n', "\n   ")));
    }
    markdown.push_str(&format!(
        "\n## {}\n\n",
        crate::i18n::ui_text("Recent agent output", "最近回复")
    ));
    for message in &agent_messages {
        markdown.push_str(message);
        markdown.push_str("\n\n");
    }
    markdown.push_str(&format!(
        "## {}\n\n",
        crate::i18n::ui_text("Suggested next-session prompt", "建议的新会话提示词")
    ));
    markdown.push_str(crate::i18n::ui_text(
        "Continue this task from the handoff above. Read the pinned context and workspace changes first, then inspect the relevant files and finish the remaining work. Re-run affected verification before reporting completion.",
        "基于以上交接继续任务。先阅读固定上下文和工作区变更，再检查相关文件并完成剩余工作；报告完成前重新运行受影响的验证。",
    ));
    markdown.push('\n');
    markdown
}

fn last_items(items: &[String], count: usize) -> Vec<String> {
    let start = items.len().saturating_sub(count);
    items[start..].to_vec()
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut truncated = text.chars().take(max_chars).collect::<String>();
    truncated.push_str("\n…");
    truncated
}

fn write_handoff(cwd: &Path, requested: &Path, markdown: &str) -> Result<PathBuf, String> {
    let path = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        cwd.join(requested)
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    std::fs::write(&path, markdown)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    Ok(path)
}
