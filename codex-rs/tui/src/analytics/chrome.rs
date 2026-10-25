//! Report navigation and bounded keyboard/mouse help for the full-screen usage view.
//! Each report retains its own reading position; help never replaces its selection or draft.

use super::AnalyticsView;
use super::controls::Control;
use super::sections::Section;
use crate::keymap::ListAction;
use ratatui::text::Line;

impl AnalyticsView {
    pub(super) fn tab_label(&self, section: Section) -> &'static str {
        match section {
            Section::Summary => crate::i18n::ui_text("Overview", "概览"),
            Section::Usage if self.business() => crate::i18n::ui_text("Tokens", "Token"),
            Section::Usage => crate::i18n::ui_text("Usage", "用量"),
            Section::Credits => crate::i18n::ui_text("Credits", "额度"),
            Section::Activity => crate::i18n::ui_text("Messages", "消息"),
            Section::Plugins => crate::i18n::ui_text("Plugins", "插件"),
            Section::Skills => crate::i18n::ui_text("Skills", "Skills"),
            Section::Chats => crate::i18n::ui_text("Chats", "会话"),
            Section::Plan => crate::i18n::ui_text("Plan", "套餐"),
        }
    }

    /// Each surface owns its reading position; switching surfaces never copies it.
    pub(super) fn scroll_offset(&self) -> usize {
        if self.show_help {
            self.help_scroll
        } else if self.zoomed {
            self.sections[self.section].scroll_offset
        } else {
            self.dashboard_scroll
        }
    }

    pub(super) fn scroll_offset_mut(&mut self) -> &mut usize {
        if self.show_help {
            &mut self.help_scroll
        } else if self.zoomed {
            &mut self.sections[self.section].scroll_offset
        } else {
            &mut self.dashboard_scroll
        }
    }

    pub(super) fn toggle_dashboard(&mut self) {
        self.zoomed = !self.zoomed;
        self.follow_selection = !self.zoomed;
    }

    pub(super) fn select_section(&mut self, section: Section) {
        if section != self.section {
            self.section = section;
            self.follow_selection = !self.zoomed;
            self.show_help = false;
        }
    }

    pub(super) fn chat_has_details(&self) -> bool {
        let cursor = self.sections[Section::Chats].cursor;
        if self.business() {
            self.chats
                .ready()
                .and_then(|chats| chats.rows.get(cursor))
                .and_then(|chat| chat.usage.as_ref())
                .is_some()
        } else {
            self.task_rows()
                .get(cursor)
                .and_then(|chat| super::task_panel::available(chat))
                .is_some()
        }
    }

    pub(super) fn hint(&self, action: ListAction) -> String {
        self.keymap
            .primary_hint(action)
            .map(crate::key_hint::ShortcutHint::display_label)
            .unwrap_or_default()
    }

    pub(super) fn help_shortcut_available(&self) -> bool {
        [
            crossterm::event::KeyModifiers::NONE,
            crossterm::event::KeyModifiers::SHIFT,
        ]
        .into_iter()
        .all(|modifiers| {
            self.keymap
                .action_for(crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Char('?'),
                    modifiers,
                ))
                .is_none()
        })
    }

    pub(super) fn help_lines(&self, width: usize) -> Vec<Line<'static>> {
        let mut controls = vec![
            format!(
                "Tab / {} · {}",
                crate::key_hint::shift(crossterm::event::KeyCode::Tab).display_label()
                ,
                crate::i18n::ui_text("next / previous report", "下一个 / 上一个报告")
            ),
            format!(
                "1–{} · {}",
                self.visible_sections().len(),
                crate::i18n::ui_text("open a report directly", "直接打开报告")
            ),
            format!(
                "{} / {} · {}",
                self.hint(ListAction::MoveLeft),
                self.hint(ListAction::MoveRight),
                crate::i18n::ui_text("select chart day or plan window", "选择图表日期或套餐周期")
            ),
            format!(
                "{} / {} · {}",
                self.hint(ListAction::MoveUp),
                self.hint(ListAction::MoveDown),
                crate::i18n::ui_text(
                    "select chat or plan period; scroll Overview",
                    "选择会话或套餐周期；滚动概览"
                )
            ),
            format!(
                "{} · {}",
                self.hint(ListAction::Accept),
                crate::i18n::ui_text("expand or collapse details", "展开或收起详情")
            ),
            format!(
                "{} / {} · {}",
                self.hint(ListAction::PageUp),
                self.hint(ListAction::PageDown),
                crate::i18n::ui_text("scroll report", "滚动报告")
            ),
            crate::i18n::ui_text(
                "In alternate screen: mouse wheel · scroll; click tabs or report controls",
                "备用屏幕中：滚轮滚动；点击标签页或报告控件"
            ).into(),
        ];
        if self.control_available(Control::Range) {
            controls.push(crate::i18n::ui_text("r · switch between 7 and 30 days", "r · 在 7 天和 30 天间切换").into());
        }
        if self.control_available(Control::Group) {
            controls.push(crate::i18n::ui_text("g · change grouping / Overview aggregation", "g · 更改分组 / 概览聚合").into());
        }
        if self.control_available(Control::Model) {
            controls.push(crate::i18n::ui_text("m · cycle model filter", "m · 循环切换模型筛选").into());
        }
        if self.control_available(Control::TaskMetric) {
            controls.push(crate::i18n::ui_text("s · change chat sort metric", "s · 更改会话排序指标").into());
        }
        if self.control_available(Control::ZeroCreditGroups) {
            controls.push(crate::i18n::ui_text("a · show zero-credit groups in expanded details", "a · 在展开详情中显示零额度分组").into());
        }
        controls.extend([
            crate::i18n::ui_text("R · refresh all reports", "R · 刷新所有报告").into(),
            format!(
                "z · {}; {} · {}",
                crate::i18n::ui_text("dashboard / focused report", "仪表盘 / 专注报告"),
                self.hint(ListAction::Accept),
                crate::i18n::ui_text("focus dashboard card", "聚焦仪表盘卡片")
            ),
            format!(
                "{} · {}; q / {} · {}",
                self.hint(ListAction::Cancel),
                crate::i18n::ui_text("back", "返回"),
                crate::key_hint::ctrl(crossterm::event::KeyCode::Char('c')).display_label(),
                crate::i18n::ui_text("close usage", "关闭用量")
            ),
        ]);
        if self.help_shortcut_available() {
            controls.insert(controls.len() - 1, crate::i18n::ui_text("? · toggle this help", "? · 切换帮助").into());
        }
        if let Some(updated) = self.sections[self.section]
            .history
            .ready()
            .and_then(|report| report.updated_at)
            .and_then(|timestamp| chrono::DateTime::from_timestamp(timestamp, /*nsecs*/ 0))
        {
            let timestamp = updated.format(self.clock_format.date_time_format());
            controls.push(if crate::i18n::is_chinese(crate::i18n::current_language()) {
                format!("报告更新于 {timestamp} UTC")
            } else {
                format!("Report updated {timestamp} UTC")
            });
        }
        controls
            .into_iter()
            .flat_map(|line| {
                textwrap::wrap(&line, width)
                    .into_iter()
                    .map(|line| Line::from(line.into_owned()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}
