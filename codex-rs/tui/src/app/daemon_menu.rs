//! Local daemon maintenance confirms restarts and updates before acting.
//! Restarts use normal TUI reconnection; updates run after restoring the terminal.

use super::*;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::i18n::ui_text;
use crate::update_action::DaemonUpdateSource;
use crate::wrapping::word_wrap_lines;
use ratatui::buffer::Buffer;
use ratatui::widgets::Paragraph;

struct DaemonMenuHeader(Vec<Line<'static>>);

impl Renderable for DaemonMenuHeader {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        Renderable::render(
            &Paragraph::new(word_wrap_lines(&self.0, usize::from(area.width))),
            area,
            buf,
        );
    }

    fn desired_height(&self, width: u16) -> u16 {
        word_wrap_lines(&self.0, usize::from(width)).len() as u16
    }
}

impl App {
    pub(super) async fn verify_managed_daemon(&mut self) {
        // A discovered socket alone does not prove it belongs to a managed daemon.
        // Probe only when the user opens a view that offers restart, never during startup.
        if !self.reconnect.offline
            && let Some(connection) = &mut self.chat_widget.remote_connection
            && connection.is_local_daemon
            && !connection.is_managed_daemon
        {
            connection.is_managed_daemon =
                codex_app_server_daemon::run(codex_app_server_daemon::LifecycleCommand::Version)
                    .await
                    .is_ok_and(|output| output.backend.is_some());
        }
    }

    pub(super) fn open_daemon_menu(&mut self) {
        let status = self
            .chat_widget
            .remote_connection
            .as_ref()
            .filter(|_| matches!(self.app_server_target, AppServerTarget::LocalDaemon { .. }))
            .map(|connection| {
                if crate::i18n::is_chinese(crate::i18n::current_language()) {
                    format!("守护进程运行中：{}", connection.version)
                } else {
                    format!("Running daemon: {}", connection.version)
                }
            })
            .unwrap_or_else(|| {
                ui_text(
                    "Not connected to the local background server.",
                    "未连接到本地后台服务。",
                )
                .to_string()
            });
        let mut header = vec![
            Line::from(ui_text("Daemon", "守护进程").bold()),
            Line::from(status.dim()),
        ];
        let unavailable = if matches!(self.app_server_target, AppServerTarget::Remote { .. }) {
            Some(ui_text(
                "Manage this server on its host. Local daemon updates are unavailable for remote connections.",
                "请在服务器主机上管理此服务。远程连接不支持更新本地守护进程。",
            ))
        } else if self.daemon_cli_executable.is_none() {
            Some(ui_text(
                "Run the Codex CLI to manage the daemon from this menu.",
                "请通过 Codex CLI 运行，以便从此菜单管理守护进程。",
            ))
        } else {
            None
        };
        if let Some(guidance) = unavailable {
            header.push(Line::from(guidance.dim()));
        }
        let has_package = self.daemon_cli_executable.as_ref().is_some_and(|path| {
            codex_install_context::InstallContext::from_exe(
                cfg!(target_os = "macos"),
                Some(path.as_path()),
                /*method_override*/ None,
            )
            .package_layout
            .is_some()
        });
        let mut items: Vec<SelectionItem> = [
            (
                DaemonUpdateSource::PublicStable,
                ui_text("Install latest public stable", "安装最新公开稳定版"),
            ),
            (
                DaemonUpdateSource::ThisCli,
                ui_text("Use this CLI build", "使用当前 CLI 构建"),
            ),
        ]
        .into_iter()
        .map(|(source, name)| SelectionItem {
            name: name.to_string(),
            is_disabled: unavailable.is_some(),
            disabled_reason: (unavailable.is_none()
                && source == DaemonUpdateSource::ThisCli
                && !has_package)
                .then(|| {
                    ui_text(
                        "This CLI has no local package to copy",
                        "当前 CLI 没有可复制的本地包",
                    )
                    .to_string()
                }),
            actions: vec![Box::new(move |tx| {
                tx.send(AppEvent::ConfirmDaemonUpdate(source));
            })],
            dismiss_on_select: true,
            ..Default::default()
        })
        .collect();
        items.push(SelectionItem {
            name: ui_text("Restart background server", "重启后台服务").to_string(),
            is_disabled: !self
                .chat_widget
                .remote_connection
                .as_ref()
                .is_some_and(|connection| connection.is_managed_daemon)
                || self.reconnect.offline
                || self.daemon_restart.is_some(),
            disabled_reason: self
                .chat_widget
                .remote_connection
                .as_ref()
                .filter(|connection| connection.is_local_daemon && !connection.is_managed_daemon)
                .map(|_| {
                    ui_text(
                        "Codex hasn't verified it manages this server. Restart it where it was started.",
                        "Codex 无法确认是否管理此服务。请在启动它的位置重启。",
                    )
                    .to_string()
                }),
            actions: vec![Box::new(|tx| tx.send(AppEvent::ConfirmDaemonRestart))],
            dismiss_on_select: true,
            ..Default::default()
        });
        self.chat_widget.show_selection_view(SelectionViewParams {
            header: Box::new(DaemonMenuHeader(header)),
            items,
            ..SelectionViewParams::picker()
        });
    }

    pub(super) fn confirm_daemon_restart(&mut self) {
        if !self
            .chat_widget
            .remote_connection
            .as_ref()
            .is_some_and(|connection| connection.is_managed_daemon)
            || self.daemon_restart.is_some()
        {
            return;
        }
        self.chat_widget.show_selection_view(SelectionViewParams {
            header: Box::new(DaemonMenuHeader(vec![
                Line::from(ui_text("Restart background server?", "重启后台服务？").bold()),
                Line::from(ui_text(
                    "Active or queued work in other chats may be interrupted.",
                    "其他会话中正在运行或排队的任务可能会中断。",
                )),
                Line::from(ui_text(
                    "Codex will reconnect automatically.",
                    "Codex 会自动重新连接。",
                )),
            ])),
            items: vec![
                SelectionItem {
                    name: ui_text("Cancel", "取消").to_string(),
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: ui_text("Restart", "重启").to_string(),
                    actions: vec![Box::new(|tx| tx.send(AppEvent::RunDaemonRestart))],
                    require_explicit_confirmation: true,
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ],
            ..SelectionViewParams::confirmation()
        });
    }

    pub(super) fn confirm_daemon_update(&mut self, source: DaemonUpdateSource) {
        let Some(executable) = &self.daemon_cli_executable else {
            return;
        };
        if matches!(self.app_server_target, AppServerTarget::Remote { .. }) {
            return;
        }
        let mut explanation = match source {
            DaemonUpdateSource::PublicStable => ui_text(
                "Install the latest public stable release. Restore production updates; keep your automatic-update setting.",
                "安装最新公开稳定版。恢复生产更新，并保留自动更新设置。",
            )
            .to_string(),
            DaemonUpdateSource::ThisCli => {
                let version = codex_install_context::InstallContext::current()
                    .package_manifest()
                    .map_or_else(|| CODEX_CLI_VERSION.to_string(), |manifest| manifest.version.to_string());
                if crate::i18n::is_chinese(crate::i18n::current_language()) {
                    format!(
                        "使用来自 {} 的当前 CLI 包 v{version}。复制完整包并固定版本，避免自动更新。",
                        executable.display()
                    )
                } else {
                    format!("Use this CLI package v{version} from {}. Copy the complete package and pin it against automatic updates.", executable.display())
                }
            }
        };
        explanation.push_str(ui_text(
            "\nThis may restart the daemon and interrupt active or queued work.\nCodex exits to update in this terminal. Relaunch it afterward.",
            "\n此操作可能重启守护进程，并中断正在运行或排队的任务。\nCodex 将退出并在此终端中更新。完成后请重新启动。",
        ));
        let mut header = vec![Line::from(ui_text("Update daemon and exit Codex?", "更新守护进程并退出 Codex？").bold())];
        header.extend(explanation.lines().map(|line| Line::from(line.to_owned())));
        self.chat_widget.show_selection_view(SelectionViewParams {
            header: Box::new(DaemonMenuHeader(header)),
            items: vec![
                SelectionItem {
                    name: ui_text("Cancel", "取消").to_string(),
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: ui_text("Update and exit", "更新并退出").to_string(),
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::RunDaemonUpdate(source))
                    })],
                    require_explicit_confirmation: true,
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ],
            ..SelectionViewParams::confirmation()
        });
    }
}

#[cfg(test)]
#[path = "daemon_menu_tests.rs"]
mod tests;
