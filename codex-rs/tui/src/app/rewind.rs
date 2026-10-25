//! App-level checkpoint capture, preview loading, and restore orchestration.

use super::*;
use crate::rewind::CheckpointSummary;
use crate::rewind::RewindRestoreOutcome;
use crate::rewind::RewindRestoreTarget;

impl App {
    pub(super) async fn capture_rewind_checkpoint(
        &mut self,
        cwd: PathBuf,
        thread_id: Option<ThreadId>,
        nth_user_message: Option<usize>,
        label: Option<String>,
        safety: bool,
    ) -> Result<Option<CheckpointSummary>, String> {
        if self.app_server_target.uses_remote_workspace() {
            return Err("检查点目前仅支持本地工作区；远程工作区尚未实现。".to_string());
        }
        let runner = self
            .workspace_command_runner
            .clone()
            .ok_or_else(|| "工作区命令执行器不可用。".to_string())?;
        crate::rewind::capture_checkpoint(
            runner.as_ref(),
            self.local_settings.codex_home.as_path(),
            &cwd,
            thread_id,
            nth_user_message,
            label,
            safety,
        )
        .await
    }

    pub(super) async fn open_rewind_overlay(
        &mut self,
        tui: &mut tui::Tui,
        cwd: PathBuf,
        thread_id: Option<ThreadId>,
    ) -> Result<()> {
        if self.app_server_target.uses_remote_workspace() {
            self.chat_widget
                .add_error_message("检查点目前仅支持本地工作区；远程工作区尚未实现。".to_string());
            tui.frame_requester().schedule_frame();
            return Ok(());
        }
        let Some(runner) = self.workspace_command_runner.clone() else {
            self.chat_widget
                .add_error_message("无法打开检查点：工作区命令执行器不可用。".to_string());
            tui.frame_requester().schedule_frame();
            return Ok(());
        };
        let checkpoints = match crate::rewind::list_checkpoints(
            runner.as_ref(),
            self.local_settings.codex_home.as_path(),
            &cwd,
            thread_id,
        )
        .await
        {
            Ok(checkpoints) => checkpoints,
            Err(error) => {
                self.chat_widget
                    .add_error_message(format!("无法读取检查点：{error}"));
                tui.frame_requester().schedule_frame();
                return Ok(());
            }
        };
        if checkpoints.is_empty() {
            self.chat_widget.add_info_message(
                "当前会话还没有检查点。发送一条消息，或使用 /checkpoint 创建。".to_string(),
                Some("自动检查点会在每条用户消息发送前创建。".to_string()),
            );
            tui.frame_requester().schedule_frame();
            return Ok(());
        }
        tui.enter_alt_screen()?;
        self.overlay = Some(Overlay::new_rewind(crate::rewind::RewindOverlay::new(
            checkpoints,
            self.app_event_tx.clone(),
            self.keymap.pager.clone(),
        )));
        tui.frame_requester().schedule_frame();
        Ok(())
    }

    pub(super) fn schedule_rewind_preview(&self, request_id: u64, checkpoint: CheckpointSummary) {
        let Some(runner) = self.workspace_command_runner.clone() else {
            self.app_event_tx.send(AppEvent::RewindPreviewLoaded {
                request_id,
                checkpoint_id: checkpoint.id,
                result: Err("工作区命令执行器不可用。".to_string()),
            });
            return;
        };
        let tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = crate::rewind::load_preview(runner.as_ref(), &checkpoint).await;
            tx.send(AppEvent::RewindPreviewLoaded {
                request_id,
                checkpoint_id: checkpoint.id,
                result,
            });
        });
    }

    pub(super) fn apply_rewind_preview(
        &mut self,
        tui: &mut tui::Tui,
        request_id: u64,
        checkpoint_id: String,
        result: Result<crate::diff_review::DiffReviewData, String>,
    ) {
        if let Some(Overlay::Rewind(overlay)) = self.overlay.as_mut() {
            overlay.accept_preview(request_id, &checkpoint_id, result);
            tui.frame_requester().schedule_frame();
        }
    }

    pub(super) fn begin_rewind_restore(
        &mut self,
        tui: &mut tui::Tui,
        checkpoint: CheckpointSummary,
        target: RewindRestoreTarget,
    ) {
        if checkpoint.thread_id != self.chat_widget.thread_id() {
            self.chat_widget
                .add_error_message("检查点所属会话已切换，请重新打开 /rewind。".to_string());
            tui.frame_requester().schedule_frame();
            return;
        }
        if target == RewindRestoreTarget::Conversation {
            self.restore_rewind_conversation(checkpoint);
            tui.frame_requester().schedule_frame();
            return;
        }
        let Some(runner) = self.workspace_command_runner.clone() else {
            self.chat_widget
                .add_error_message("恢复失败：工作区命令执行器不可用。".to_string());
            tui.frame_requester().schedule_frame();
            return;
        };
        let codex_home = self.local_settings.codex_home.to_path_buf();
        let tx = self.app_event_tx.clone();
        self.chat_widget.set_rewind_in_progress(/*running*/ true);
        self.chat_widget.add_info_message(
            format!("正在恢复到检查点「{}」…", checkpoint.label),
            Some("恢复前会先保存当前工作区。".to_string()),
        );
        tokio::spawn(async move {
            let result =
                crate::rewind::restore_checkpoint_files(runner.as_ref(), &codex_home, &checkpoint)
                    .await;
            tx.send(AppEvent::RewindFilesRestored {
                checkpoint,
                target,
                result,
            });
        });
    }

    pub(super) fn finish_rewind_file_restore(
        &mut self,
        tui: &mut tui::Tui,
        checkpoint: CheckpointSummary,
        target: RewindRestoreTarget,
        result: Result<RewindRestoreOutcome, String>,
    ) {
        self.chat_widget.set_rewind_in_progress(/*running*/ false);
        match result {
            Ok(outcome) => {
                let safety_note = outcome
                    .safety_checkpoint_id
                    .map(|_| "当前状态已保存为恢复前安全点。")
                    .unwrap_or("工作区已恢复。");
                self.chat_widget.add_info_message(
                    format!("已恢复检查点「{}」的文件。", checkpoint.label),
                    Some(safety_note.to_string()),
                );
                if target == RewindRestoreTarget::Both {
                    self.restore_rewind_conversation(checkpoint);
                }
            }
            Err(error) => self
                .chat_widget
                .add_error_message(format!("恢复检查点失败：{error}")),
        }
        self.chat_widget.maybe_send_next_queued_input();
        tui.frame_requester().schedule_frame();
    }

    fn restore_rewind_conversation(&mut self, checkpoint: CheckpointSummary) {
        let Some(thread_id) = checkpoint.thread_id else {
            self.chat_widget
                .add_error_message("该检查点没有关联可回退的对话。".to_string());
            return;
        };
        let Some(nth_user_message) = checkpoint.nth_user_message else {
            self.chat_widget
                .add_error_message("该检查点没有关联可回退的用户消息。".to_string());
            return;
        };
        let Some(selection) = self.backtrack_selection_for_thread(thread_id, nth_user_message)
        else {
            self.chat_widget.add_error_message(
                "该检查点对应的用户消息已不在当前会话中，无法回退对话。".to_string(),
            );
            return;
        };
        self.apply_backtrack_selection(selection);
    }
}
