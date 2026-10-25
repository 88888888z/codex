//! `/verify` command discovery, execution, and retained results.

use std::time::Duration;
use std::time::Instant;

use crate::verification::VerificationCommand;
use crate::verification::VerificationOutcome;
use crate::verification::VerificationReport;
use crate::workspace_command::WorkspaceCommand;
use crate::workspace_set::WorkspaceSet;

struct ScopedVerificationCommand {
    project: Option<String>,
    cwd: std::path::PathBuf,
    command: VerificationCommand,
}

impl ScopedVerificationCommand {
    fn label(&self) -> String {
        self.project.as_ref().map_or_else(
            || self.command.label.clone(),
            |project| format!("@{project} · {}", self.command.label),
        )
    }
}

#[derive(Default)]
pub(super) struct VerificationState {
    report: Option<VerificationReport>,
    running: bool,
    stale: bool,
}

impl super::ChatWidget {
    pub(crate) fn reset_verification_state(&mut self) {
        self.verification = VerificationState::default();
        self.bottom_pane.set_verification_indicator(None);
    }

    pub(crate) fn handle_verify_command_args(&mut self, args: &str) {
        let args = args.trim();
        let cwd = self
            .current_cwd
            .clone()
            .unwrap_or_else(|| self.config.cwd.to_path_buf());
        let workspace_set = match WorkspaceSet::discover(&cwd) {
            Ok(workspace_set) => workspace_set,
            Err(error) => {
                self.add_error_message(error);
                return;
            }
        };
        let commands = workspace_set.as_ref().map_or_else(
            || {
                crate::verification::detect_commands(&cwd)
                    .into_iter()
                    .map(|command| ScopedVerificationCommand {
                        project: None,
                        cwd: cwd.clone(),
                        command,
                    })
                    .collect::<Vec<_>>()
            },
            |workspace_set| {
                workspace_set
                    .roots
                    .iter()
                    .flat_map(|root| {
                        crate::verification::detect_commands(&root.path)
                            .into_iter()
                            .map(|command| ScopedVerificationCommand {
                                project: Some(root.id.clone()),
                                cwd: root.path.clone(),
                                command,
                            })
                    })
                    .collect::<Vec<_>>()
            },
        );
        if commands.is_empty() {
            self.add_info_message(
                crate::i18n::ui_text(
                    "No supported verification commands were detected.",
                    "没有检测到支持的验证命令。",
                )
                .to_string(),
                Some(
                    crate::i18n::ui_text(
                        "Supported projects: Cargo, npm/pnpm/yarn/bun, Go, pytest/ruff, and Makefile.",
                        "支持 Cargo、npm/pnpm/yarn/bun、Go、pytest/ruff 和 Makefile。",
                    )
                    .to_string(),
                ),
            );
            return;
        }
        if args.is_empty() || args.eq_ignore_ascii_case("list") {
            self.show_verification_commands(&commands);
            return;
        }
        if self.verification.running {
            self.add_error_message(
                crate::i18n::ui_text(
                    "A verification run is already in progress.",
                    "已有验证正在运行。",
                )
                .to_string(),
            );
            return;
        }
        let selected = if args.eq_ignore_ascii_case("all") {
            commands
        } else {
            let selected = commands
                .into_iter()
                .filter(|command| command.command.name.eq_ignore_ascii_case(args))
                .collect::<Vec<_>>();
            if selected.is_empty() {
                self.add_error_message(
                    crate::i18n::ui_text(
                        "Unknown verification command. Run /verify to list available checks.",
                        "未知验证命令。运行 /verify 查看可用检查。",
                    )
                    .to_string(),
                );
                return;
            }
            selected
        };
        let Some(runner) = self.workspace_command_runner.clone() else {
            self.add_error_message(
                crate::i18n::ui_text(
                    "Workspace command runner is unavailable.",
                    "工作区命令执行器不可用。",
                )
                .to_string(),
            );
            return;
        };

        self.verification.running = true;
        self.bottom_pane.set_verification_indicator(None);
        let labels = selected
            .iter()
            .map(ScopedVerificationCommand::label)
            .collect::<Vec<_>>()
            .join(", ");
        self.add_info_message(
            if crate::i18n::is_chinese(crate::i18n::current_language()) {
                format!("正在验证：{labels}")
            } else {
                format!("Verifying: {labels}")
            },
            Some(
                crate::i18n::ui_text(
                    "You can continue typing; results will be added when complete.",
                    "可以继续输入；完成后会显示结果。",
                )
                .to_string(),
            ),
        );
        let tx = self.app_event_tx.clone();
        let thread_id = self.thread_id;
        tokio::spawn(async move {
            let mut outcomes = Vec::with_capacity(selected.len());
            for scoped in selected {
                let started = Instant::now();
                let label = scoped.label();
                let ScopedVerificationCommand {
                    project: _,
                    cwd,
                    command,
                } = scoped;
                let command_text = command.argv.join(" ");
                let result = runner
                    .run(
                        WorkspaceCommand::new(command.argv, cwd.as_path())
                            .timeout(Duration::from_secs(/*secs*/ 900)),
                    )
                    .await;
                let (success, exit_code, output) = match result {
                    Ok(output) => {
                        let combined = [output.stderr.trim(), output.stdout.trim()]
                            .into_iter()
                            .filter(|part| !part.is_empty())
                            .collect::<Vec<_>>()
                            .join("\n");
                        (output.success(), Some(output.exit_code), combined)
                    }
                    Err(error) => (
                        false,
                        None,
                        if crate::i18n::is_chinese(crate::i18n::current_language()) {
                            format!("命令执行失败：{error}")
                        } else {
                            format!("Command failed: {error}")
                        },
                    ),
                };
                outcomes.push(VerificationOutcome {
                    label,
                    command: command_text,
                    success,
                    exit_code,
                    output,
                    duration: started.elapsed(),
                });
            }
            tx.send(crate::app_event::AppEvent::VerificationFinished {
                thread_id,
                report: VerificationReport { outcomes },
            });
        });
        self.request_redraw();
    }

    pub(crate) fn finish_verification(
        &mut self,
        thread_id: Option<codex_protocol::ThreadId>,
        report: VerificationReport,
    ) {
        if self.thread_id != thread_id {
            return;
        }
        self.verification.running = false;
        self.verification.stale = false;
        let success = report.success();
        let rendered = report.render();
        self.verification.report = Some(report);
        self.bottom_pane.set_verification_indicator(Some(success));
        if success {
            self.add_info_message(
                crate::i18n::ui_text("Verification passed", "验证通过").to_string(),
                Some(rendered),
            );
        } else {
            self.add_error_message(format!(
                "{}\n{rendered}",
                crate::i18n::ui_text("Verification failed", "验证失败")
            ));
        }
        self.request_redraw();
    }

    pub(crate) fn mark_verification_stale(&mut self) {
        if self.verification.report.is_some() {
            self.verification.stale = true;
            self.bottom_pane.set_verification_indicator(None);
        }
    }

    pub(crate) fn verification_summary(&self) -> String {
        let summary = self.verification.report.as_ref().map_or_else(
            || crate::i18n::ui_text("No verification run yet.", "尚未运行验证。").to_string(),
            VerificationReport::handoff_summary,
        );
        if self.verification.stale {
            format!(
                "{}\n{}",
                crate::i18n::ui_text(
                    "This result is stale after later input.",
                    "后续输入已使该结果过期。"
                ),
                summary
            )
        } else {
            summary
        }
    }

    fn show_verification_commands(&mut self, commands: &[ScopedVerificationCommand]) {
        let hint = commands
            .iter()
            .map(|command| format!("/verify {} — {}", command.command.name, command.label()))
            .collect::<Vec<_>>()
            .join("\n");
        self.add_info_message(
            crate::i18n::ui_text("Verification commands", "验证命令").to_string(),
            Some(format!(
                "{hint}\n\n{}",
                crate::i18n::ui_text(
                    "Use /verify all to run every detected check.",
                    "使用 /verify all 运行所有检测项。"
                )
            )),
        );
    }
}
