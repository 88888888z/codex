//! Folder trust disclosure with bounded paths, shared picker styling, and protected selection.

use std::path::Path;
use std::path::PathBuf;

use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;
use ratatui::widgets::Wrap;

use crate::bottom_pane::picker_option_row;
use crate::bottom_pane::render_menu_surface;
use crate::i18n::ui_text;
use crate::key_hint::KeyBindingListExt;
use crate::onboarding::keys;
use crate::onboarding::onboarding_screen::KeyboardHandler;
use crate::onboarding::onboarding_screen::StepStateProvider;
use crate::render::Insets;
use crate::render::renderable::FlexRenderable;
use crate::render::renderable::Renderable;
use crate::render::renderable::RenderableExt as _;
use crate::render::renderable::RenderableItem;
use crate::text_formatting::center_truncate_path;

use super::onboarding_screen::StepState;
pub(crate) struct TrustDirectoryWidget {
    pub restricted: bool,
    pub existing_task: bool,
    pub cancel: TrustCancelAction,
    pub cwd: PathBuf,
    pub trust_target: PathBuf,
    pub show_windows_create_sandbox_hint: bool,
    pub should_quit: bool,
    pub selection: Option<TrustDirectorySelection>,
    pub highlighted: TrustDirectorySelection,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrustCancelAction {
    Quit,
    AgentsOverview,
    CurrentTask,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustDirectorySelection {
    Trust,
    Quit,
}

impl WidgetRef for &TrustDirectoryWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        // Measure the mandatory content before giving any rows to variable-length paths.
        let mut children = vec![(
            0,
            Paragraph::new(if self.restricted && self.existing_task {
                ui_text(
                    "This existing task may retain settings \
                     and history, including project configuration or hooks loaded while it was trusted. \
                     To use restricted settings, start a new task. The folder's trust setting will not change.",
                    "此现有任务可能保留此前受信任时加载的设置和历史，包括项目配置或 hooks。\
                     如需使用受限设置，请新建任务。文件夹的信任设置不会改变。",
                )
            } else if self.restricted {
                ui_text(
                    "Config, hooks, and exec policies from untrusted folders stay disabled. \
                     Trusted project folders can still contribute settings. Skills still load, \
                     and tools follow your permission settings. Opening will not change saved trust.",
                    "未受信任文件夹中的配置、hooks 和执行策略会保持禁用。\
                     受信任的项目文件夹仍可提供设置。Skills 仍会加载，工具遵循你的权限设置。\
                     打开文件夹不会改变已保存的信任状态。",
                )
            } else {
                ui_text(
                    "Trust this folder? Codex can read, edit, and run files here, subject to \
                     your permission settings. Folder settings can run code automatically, \
                     even without a model request. Continue only if you trust these files. \
                     Your trust decision will be saved.",
                    "信任此文件夹？Codex 可以在此读取、编辑和运行文件，具体受你的权限设置限制。\
                     文件夹设置可能在没有模型请求时自动运行代码。仅在信任这些文件时继续。\
                     你的信任决定会被保存。",
                )
            })
            .wrap(Wrap { trim: true })
            .inset(Insets::vh(/*v*/ 0, /*h*/ 2)),
        )];
        children.push((1, RenderableItem::Borrowed(&"")));

        let options: Vec<(&str, TrustDirectorySelection)> = vec![
            (
                if self.restricted && self.existing_task {
                    ui_text("Open existing task", "打开现有任务")
                } else if self.restricted {
                    ui_text("Open restricted", "以受限模式打开")
                } else {
                    ui_text("Trust and continue", "信任并继续")
                },
                TrustDirectorySelection::Trust,
            ),
            (
                match self.cancel {
                    TrustCancelAction::Quit => ui_text("Quit", "退出"),
                    TrustCancelAction::AgentsOverview => {
                        ui_text("Back to Agent Command Center", "返回 Agent 指挥中心")
                    }
                    TrustCancelAction::CurrentTask => {
                        ui_text("Keep current directory", "保持当前目录")
                    }
                },
                TrustDirectorySelection::Quit,
            ),
        ];

        for (idx, (text, selection)) in options.iter().enumerate() {
            children.push((
                0,
                picker_option_row(idx, text.to_string(), self.highlighted == *selection).into(),
            ));
        }

        children.push((1, RenderableItem::Borrowed(&"")));

        if let Some(error) = &self.error {
            children.push((
                0,
                Paragraph::new(error.to_string())
                    .red()
                    .wrap(Wrap { trim: true })
                    .inset(Insets::vh(/*v*/ 0, /*h*/ 2)),
            ));
            children.push((1, RenderableItem::Borrowed(&"")));
        }

        children.push((
            0,
            Paragraph::new(Line::from(vec![
                keys::CONFIRM[0].into(),
                if self.show_windows_create_sandbox_hint && !self.restricted {
                    ui_text(" continue and create sandbox", " 继续并创建沙箱").dim()
                } else {
                    ui_text(" continue", " 继续").dim()
                },
                " · ".dim(),
                keys::CANCEL[0].into(),
                match self.cancel {
                    TrustCancelAction::Quit => ui_text(" quit", " 退出"),
                    TrustCancelAction::AgentsOverview => ui_text(" back", " 返回"),
                    TrustCancelAction::CurrentTask => ui_text(" cancel", " 取消"),
                }
                .dim(),
            ]))
            .wrap(Wrap { trim: false })
            .inset(Insets::vh(/*v*/ 0, /*h*/ 2)),
        ));
        children.push((1, RenderableItem::Borrowed(&"")));

        #[allow(clippy::disallowed_methods)]
        let git_root_warning = (!self.restricted && self.cwd != self.trust_target).then(|| {
            Paragraph::new(
                ui_text(
                    "Note: You’re in a subdirectory of a Git project. Trusting will apply to the repository root:",
                    "注意：当前目录位于 Git 项目的子目录中。信任会应用到仓库根目录：",
                ),
            )
            .yellow()
            .wrap(Wrap { trim: true })
            .inset(Insets::vh(/*v*/ 0, /*h*/ 2))
        });
        let reserved_height = children
            .iter()
            .filter(|(flex, _)| *flex == 0)
            .map(|(_, child)| child.desired_height(area.width))
            .fold(/*init*/ 1u16, u16::saturating_add)
            .saturating_add(git_root_warning.desired_height(area.width));
        let path_height = area.height.saturating_sub(reserved_height);
        // If only one path row fits, show the repository root receiving trust.
        let cwd_height = if git_root_warning.is_some() {
            path_height / 2
        } else {
            path_height
        };
        let path = |path: &Path, max_height: u16| {
            let text = path.display().to_string();
            let paragraph = Paragraph::new(text.clone()).wrap(Wrap { trim: false });
            let width = area.width.saturating_sub(/*rhs*/ 4);
            if paragraph.desired_height(width) <= max_height {
                paragraph.dim().inset(Insets::vh(/*v*/ 0, /*h*/ 2))
            } else {
                // A single line bounds even paths containing embedded newlines.
                Line::from(center_truncate_path(&text, usize::from(width)))
                    .dim()
                    .inset(Insets::vh(/*v*/ 0, /*h*/ 2))
            }
        };

        let mut column = FlexRenderable::new();
        column.push(/*flex*/ 1, RenderableItem::Borrowed(&""));
        column.push(
            /*flex*/ 0,
            RenderableItem::Owned(Box::new(Line::from(
                ui_text("  Folder access", "  文件夹访问权限").bold(),
            ))),
        );
        if cwd_height > 0 {
            column.push(/*flex*/ 0, path(&self.cwd, cwd_height));
        }
        column.push(/*flex*/ 1, RenderableItem::Borrowed(&""));
        if let Some(warning) = git_root_warning {
            column.push(/*flex*/ 0, warning);
            let root_height = path_height.saturating_sub(cwd_height);
            if root_height > 0 {
                column.push(/*flex*/ 0, path(&self.trust_target, root_height));
            }
            column.push(/*flex*/ 1, RenderableItem::Borrowed(&""));
        }
        for (flex, child) in children {
            column.push(flex, child);
        }

        let panel = Rect {
            height: column.desired_height(area.width).min(area.height),
            ..area
        };
        render_menu_surface(panel, buf);
        // The column owns flexible vertical padding; row highlights span the full panel.
        column.render(panel, buf);
    }
}

impl KeyboardHandler for TrustDirectoryWidget {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if key_event.kind != KeyEventKind::Press {
            return;
        }

        if keys::MOVE_UP.is_pressed(key_event) {
            self.highlighted = TrustDirectorySelection::Trust;
        } else if keys::MOVE_DOWN.is_pressed(key_event) {
            self.highlighted = TrustDirectorySelection::Quit;
        } else if keys::SELECT_FIRST.is_pressed(key_event) {
            // A terminal response fragment can start with `1`; trust always requires an explicit
            // Enter confirmation after the directory prompt is visible.
            self.highlighted = TrustDirectorySelection::Trust;
        } else if keys::SELECT_SECOND.is_pressed(key_event)
            || keys::QUIT.is_pressed(key_event)
            || keys::CANCEL.is_pressed(key_event)
        {
            self.handle_quit();
        } else if keys::CONFIRM.is_pressed(key_event) {
            match self.highlighted {
                TrustDirectorySelection::Trust => self.handle_trust(),
                TrustDirectorySelection::Quit => self.handle_quit(),
            }
        }
    }
}

impl StepStateProvider for TrustDirectoryWidget {
    fn get_step_state(&self) -> StepState {
        if self.selection.is_some() || self.should_quit {
            StepState::Complete
        } else {
            StepState::InProgress
        }
    }
}

impl TrustDirectoryWidget {
    fn handle_trust(&mut self) {
        self.highlighted = TrustDirectorySelection::Trust;
        self.error = None;
        self.selection = Some(TrustDirectorySelection::Trust);
    }

    fn handle_quit(&mut self) {
        self.highlighted = TrustDirectorySelection::Quit;
        self.should_quit = true;
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }
}

#[cfg(test)]
mod tests {
    use crate::test_backend::VT100Backend;

    use super::*;
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyEventKind;
    use crossterm::event::KeyModifiers;
    use pretty_assertions::assert_eq;
    use ratatui::Terminal;
    use std::path::PathBuf;

    fn widget(error: Option<String>) -> TrustDirectoryWidget {
        TrustDirectoryWidget {
            restricted: false,
            existing_task: false,
            cancel: TrustCancelAction::Quit,
            cwd: PathBuf::from("/workspace/project"),
            trust_target: PathBuf::from("/workspace/project"),
            show_windows_create_sandbox_hint: false,
            should_quit: false,
            selection: None,
            highlighted: TrustDirectorySelection::Trust,
            error,
        }
    }

    #[test]
    fn release_event_does_not_change_selection() {
        let mut widget = TrustDirectoryWidget {
            restricted: false,
            existing_task: false,
            cancel: TrustCancelAction::Quit,
            cwd: PathBuf::from("."),
            trust_target: PathBuf::from("."),
            show_windows_create_sandbox_hint: false,
            should_quit: false,
            selection: None,
            highlighted: TrustDirectorySelection::Quit,
            error: None,
        };

        let release = KeyEvent {
            kind: KeyEventKind::Release,
            ..KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        };
        widget.handle_key_event(release);
        assert_eq!(widget.selection, None);

        let repeat =
            KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Repeat);
        widget.handle_key_event(repeat);
        assert_eq!(widget.selection, None);

        let press = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        widget.handle_key_event(press);
        assert!(widget.should_quit);
    }

    #[test]
    fn fragmented_terminal_response_cannot_grant_directory_trust() {
        let mut widget = widget(/*error*/ None);
        widget.highlighted = TrustDirectorySelection::Quit;

        // The prefix may have been consumed by the protected-screen input drain, leaving the
        // numeric OSC slot as the first key delivered after the trust prompt becomes active.
        for character in "10;rgb:ffff/ffff/ffff".chars() {
            widget.handle_key_event(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }

        assert_eq!(widget.selection, None);
        assert_eq!(widget.highlighted, TrustDirectorySelection::Trust);

        widget.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(widget.selection, Some(TrustDirectorySelection::Trust));
    }

    #[test]
    fn renders_snapshot_for_git_repo() {
        let mut widget = widget(/*error*/ None);
        widget.show_windows_create_sandbox_hint = true;

        let mut terminal =
            Terminal::new(VT100Backend::new(/*width*/ 70, /*height*/ 14)).expect("terminal");
        terminal
            .draw(|f| (&widget).render_ref(f.area(), f.buffer_mut()))
            .expect("draw");

        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn directory_change_consent_can_keep_current_directory() {
        let mut widget = widget(/*error*/ None);
        widget.cancel = TrustCancelAction::CurrentTask;
        let mut terminal =
            Terminal::new(VT100Backend::new(/*width*/ 70, /*height*/ 18)).expect("terminal");
        terminal
            .draw(|f| (&widget).render_ref(f.area(), f.buffer_mut()))
            .expect("draw");
        insta::assert_snapshot!(terminal.backend());
        widget.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(widget.should_quit());
    }

    #[test]
    fn renders_snapshot_for_remote_git_subdirectory() {
        let widget = TrustDirectoryWidget {
            restricted: false,
            existing_task: false,
            cancel: TrustCancelAction::AgentsOverview,
            cwd: PathBuf::from("/srv/remote/project/nested"),
            trust_target: PathBuf::from("/srv/remote/project"),
            ..widget(/*error*/ None)
        };

        let mut terminal =
            Terminal::new(VT100Backend::new(/*width*/ 70, /*height*/ 18)).expect("terminal");
        terminal
            .draw(|f| (&widget).render_ref(f.area(), f.buffer_mut()))
            .expect("draw");

        insta::assert_snapshot!(
            terminal
                .backend()
                .to_string()
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn renders_restricted_folder() {
        for existing_task in [false, true] {
            let widget = TrustDirectoryWidget {
                restricted: true,
                existing_task,
                cancel: TrustCancelAction::AgentsOverview,
                ..widget(/*error*/ None)
            };
            let mut terminal =
                Terminal::new(VT100Backend::new(/*width*/ 70, /*height*/ 18)).expect("terminal");
            terminal
                .draw(|f| (&widget).render_ref(f.area(), f.buffer_mut()))
                .expect("draw");
            if existing_task {
                insta::assert_snapshot!("existing_untrusted_task", terminal.backend());
            } else {
                insta::assert_snapshot!(terminal.backend());
            }
        }
    }

    #[test]
    fn renders_snapshot_for_trust_error() {
        let widget = widget(Some(
            "Failed to set trust for /workspace/project: config/batchWrite failed in TUI: Invalid configuration: features.fast_mode=true is not supported; allowed set [fast_mode=false]"
                .to_string(),
        ));

        let mut terminal =
            Terminal::new(VT100Backend::new(/*width*/ 70, /*height*/ 22)).expect("terminal");
        terminal
            .draw(|f| (&widget).render_ref(f.area(), f.buffer_mut()))
            .expect("draw");

        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn picker_wraps_paths_and_restricted_actions() {
        let widget = TrustDirectoryWidget {
            restricted: true,
            cancel: TrustCancelAction::AgentsOverview,
            cwd: PathBuf::from("/workspace/project/long-nested-folder"),
            ..widget(/*error*/ None)
        };
        let mut terminal =
            Terminal::new(VT100Backend::new(/*width*/ 40, /*height*/ 24)).expect("terminal");
        terminal
            .draw(|frame| (&widget).render_ref(frame.area(), frame.buffer_mut()))
            .expect("draw");
        insta::assert_snapshot!("folder_picker_restricted_40x24", terminal.backend());
    }

    #[test]
    fn long_paths_keep_disclosure_and_choices_visible() {
        let trust_target: PathBuf = std::iter::once("workspace")
            .chain(std::iter::repeat_n("nested-checkout", /*count*/ 60))
            .chain(["repository"])
            .collect();
        for (subdirectory, height, snapshot) in [
            (false, 13, "long_checkout_40x13"),
            (true, 17, "long_repository_root_40x17"),
            (true, 16, "only_repository_root_fits_40x16"),
        ] {
            let widget = TrustDirectoryWidget {
                cwd: if subdirectory {
                    trust_target.join("checkout")
                } else {
                    trust_target.clone()
                },
                trust_target: trust_target.clone(),
                ..widget(/*error*/ None)
            };
            let mut terminal =
                Terminal::new(VT100Backend::new(/*width*/ 40, height)).expect("terminal");
            terminal
                .draw(|frame| (&widget).render_ref(frame.area(), frame.buffer_mut()))
                .expect("draw");
            insta::assert_snapshot!(snapshot, terminal.backend().to_string().replace('\\', "/"));
        }
    }
}
