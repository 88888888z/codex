//! A compact footer advertises primary actions; help lists the configured task shortcuts.

use super::*;
use crate::i18n::ui_text;
use crate::key_hint;
use crate::shortcut_help::Group;
use crate::shortcut_help::Shortcut;
use crate::style::footer_hint_label_style;

pub(super) fn hint_line(items: &[(String, String)]) -> Line<'static> {
    let mut line = Line::default();
    for (key, label) in items.iter().filter(|(key, _)| !key.is_empty()) {
        if !line.spans.is_empty() {
            line.spans.push("  ".dim());
        }
        line.spans.extend(key_hint::key_label_spans(key));
        line.spans.push(Span::styled(
            format!(" {label}"),
            footer_hint_label_style().not_bold(),
        ));
    }
    line
}

impl AgentsOverviewView {
    fn center_list_hint(&self, action: ListAction) -> Option<ShortcutHint> {
        self.keymap.primary_hint(action).filter(|hint| {
            !matches!(hint, ShortcutHint::Single(binding) if [
                &self.agents_keymap.resume,
                &self.agents_keymap.search,
                &self.agents_keymap.new_task,
                &self.agents_keymap.new_worktree,
                &self.agents_keymap.fork,
                &self.agents_keymap.rename,
                &self.agents_keymap.stop,
                &self.agents_keymap.archive,
                &self.agents_keymap.delete,
                &self.agents_keymap.hide,
                &self.agents_keymap.toggle_pin,
                &self.agents_keymap.toggle_grouping,
            ].into_iter().any(|bindings| bindings.contains(binding)))
        })
    }

    pub(super) fn center_filter_hint(&self) -> String {
        [key_hint::plain(KeyCode::Tab), key_hint::shift(KeyCode::Tab)]
            .into_iter()
            .filter(|hint| {
                let (code, modifiers) = hint.parts();
                !self
                    .center_shortcut_keys
                    .is_pressed(KeyEvent::new(code, modifiers))
            })
            .map(|hint| hint.display_label())
            .collect::<Vec<_>>()
            .join("/")
    }

    pub(super) fn center_help_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut navigate = Group {
            title: ui_text("Navigate", "导航"),
            entries: Vec::new(),
        };
        for (action, label) in [
            (ListAction::MoveUp, ui_text("Up", "上移")),
            (ListAction::MoveDown, ui_text("Down", "下移")),
            (ListAction::Accept, ui_text("Open", "打开")),
            (ListAction::PageUp, ui_text("Page up", "上一页")),
            (ListAction::PageDown, ui_text("Page down", "下一页")),
        ] {
            navigate.push(self.center_list_hint(action), label);
        }
        navigate.entries.push(Shortcut::new(
            key_hint::ctrl(KeyCode::Char('c')),
            ui_text("Quit", "退出"),
        ));
        let mut tasks = Group {
            title: ui_text("Tasks", "任务"),
            entries: Vec::new(),
        };
        for (action, bindings, label) in [
            ("new_task", &self.agents_keymap.new_task, ui_text("New", "新建")),
            (
                "new_worktree",
                &self.agents_keymap.new_worktree,
                ui_text("New worktree", "新建工作树"),
            ),
            ("resume", &self.agents_keymap.resume, ui_text("Resume", "继续")),
            ("fork", &self.agents_keymap.fork, ui_text("Fork", "分叉")),
            ("rename", &self.agents_keymap.rename, ui_text("Rename", "重命名")),
            ("stop", &self.agents_keymap.stop, ui_text("Stop", "停止")),
            ("archive", &self.agents_keymap.archive, ui_text("Archive", "归档")),
            ("hide", &self.agents_keymap.hide, ui_text("Hide", "隐藏")),
            ("delete", &self.agents_keymap.delete, ui_text("Delete", "删除")),
            (
                "toggle_pin",
                &self.agents_keymap.toggle_pin,
                ui_text("Pin/unpin", "固定/取消固定"),
            ),
        ] {
            if (action != "new_worktree" || self.worktrees_enabled)
                && (action != "toggle_pin" || self.can_toggle_selected_pin())
            {
                tasks.push(self.agents_keymap.primary_hint(action, bindings), label);
            }
        }
        let mut view = Group {
            title: ui_text("View", "视图"),
            entries: Vec::new(),
        };
        let filter = self.center_filter_hint();
        if !filter.is_empty() {
            view.entries.push(Shortcut {
                key: filter,
                action: ui_text("Filter", "筛选"),
            });
        }
        for (action, bindings, label) in [
            ("search", &self.agents_keymap.search, ui_text("Search", "搜索")),
            (
                "toggle_grouping",
                &self.agents_keymap.toggle_grouping,
                ui_text("Group", "分组"),
            ),
        ] {
            view.push(self.agents_keymap.primary_hint(action, bindings), label);
        }
        let mut lines = vec![
            ui_text("Task shortcuts", "任务快捷键").bold().into(),
            Line::default(),
        ];
        lines.extend(crate::shortcut_help::group_lines(
            [navigate, tasks, view],
            width,
        ));
        crate::wrapping::word_wrap_lines(lines, usize::from(width.max(/*other*/ 1)))
    }

    pub(super) fn center_footer_hints(&self) -> Vec<(String, String)> {
        let state = self.state();
        if let Some(items) = &state.key_chord_hint {
            return items.clone();
        }
        let mut hints = Vec::new();
        if !state.help
            && !state.editing_metadata()
            && ![KeyModifiers::NONE, KeyModifiers::SHIFT]
                .into_iter()
                .any(|modifiers| {
                    self.center_shortcut_keys
                        .is_pressed(KeyEvent::new(KeyCode::Char('?'), modifiers))
                })
        {
            hints.push(("?".into(), ui_text("help", "帮助").into()));
        }
        let cancel = if state.help {
            self.keymap.primary_hint(ListAction::Cancel)
        } else {
            self.center_list_hint(ListAction::Cancel)
        };
        if let Some(hint) = cancel {
            hints.push((hint.display_label(), ui_text("back", "返回").into()));
        }
        if state.help {
            return hints;
        }
        if !state.editing_metadata() {
            let navigation = [ListAction::MoveUp, ListAction::MoveDown]
                .into_iter()
                .filter_map(|action| self.center_list_hint(action))
                .map(ShortcutHint::display_label)
                .collect::<Vec<_>>()
                .join("/");
            if !navigation.is_empty() {
                hints.push((navigation, ui_text("move", "移动").into()));
            }
        }
        if let Some(hint) = self.center_list_hint(ListAction::Accept) {
            hints.push((
                hint.display_label(),
                (if state.rename_target.is_some() {
                    ui_text("rename", "重命名")
                } else {
                    ui_text("open", "打开")
                })
                .into(),
            ));
        }
        if !state.editing_metadata()
            && let Some(hint) = self
                .agents_keymap
                .primary_hint("new_task", &self.agents_keymap.new_task)
        {
            hints.push((hint.display_label(), ui_text("new", "新建").into()));
        }
        hints
    }
}
