//! Status tabs, task rows, the rename editor and details share a wide/narrow layout.

use super::hints::hint_line;
use super::*;
use crate::bottom_pane::render_filled_tab_bar;
use crate::i18n::ui_text;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crossterm::cursor::SetCursorStyle;
use ratatui::widgets::WidgetRef;
use unicode_segmentation::UnicodeSegmentation;

pub(super) fn row(area: Rect, offset: u16, height: u16) -> Rect {
    let offset = offset.min(area.height);
    Rect::new(
        area.x,
        area.y + offset,
        area.width,
        height.min(area.height - offset),
    )
}

pub(super) fn line(text: impl Into<Line<'static>>, area: Rect, buf: &mut Buffer) {
    if !area.is_empty() {
        truncate_line_with_ellipsis_if_overflow(text.into(), usize::from(area.width))
            .render(row(area, /*offset*/ 0, /*height*/ 1), buf);
    }
}

fn search_prefix() -> &'static str {
    ui_text("Search › ", "搜索 › ")
}

fn rename_prefix() -> &'static str {
    ui_text("Rename › ", "重命名 › ")
}

fn visible_suffix(input: &str, available: usize) -> &str {
    let mut start = input.len();
    let mut visible_width = 0;
    for (offset, grapheme) in input.grapheme_indices(/*is_extended*/ true).rev() {
        let width = grapheme.width();
        if visible_width + width > available {
            break;
        }
        visible_width += width;
        start = offset;
    }
    &input[start..]
}

struct CenterLayout {
    header: Rect,
    list: Rect,
    gap: Rect,
    details: Rect,
    search: Rect,
    footer: Rect,
}

impl AgentsOverviewView {
    fn center_layout(&self, area: Rect) -> CenterLayout {
        let state = self.state();
        let footer_height = u16::from(area.height >= 2);
        let header_height = if area.height >= 6 { 3 } else { 0 };
        let footer = row(area, area.height - footer_height, footer_height);
        let header = row(area, /*offset*/ 0, header_height);
        let body = row(area, header_height, footer.y - header.bottom())
            .inner(Margin::new(/*horizontal*/ 2, /*vertical*/ 0));
        let [list, gap, details] = if body.width >= 90 {
            Layout::horizontal([
                Constraint::Min(46),
                Constraint::Length(3),
                Constraint::Length(38),
            ])
            .areas(body)
        } else {
            [body, Rect::default(), Rect::default()]
        };
        let search = row(list, /*offset*/ 0, u16::from(state.editing_metadata()));
        let list = row(
            list,
            search.height,
            list.height.saturating_sub(search.height),
        );
        CenterLayout {
            header,
            list,
            gap,
            details,
            search,
            footer,
        }
    }
}

impl Renderable for AgentsOverviewView {
    fn desired_height(&self, _width: u16) -> u16 {
        24
    }

    fn cursor_style(&self, _area: Rect) -> SetCursorStyle {
        let state = self.state();
        if state.rename_target.is_some() && state.input.uses_vim_insert_cursor() {
            SetCursorStyle::SteadyBar
        } else {
            SetCursorStyle::DefaultUserShape
        }
    }

    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        let layout = self.center_layout(area);
        let state = self.state();
        if state.editing_metadata() && !layout.search.is_empty() {
            if state.rename_target.is_some() {
                let mut area = layout.search;
                let prefix_width = rename_prefix().width().min(usize::from(area.width)) as u16;
                area.x += prefix_width;
                area.width -= prefix_width;
                return state.input.cursor_pos(area);
            }
            let input = &state.search;
            let prefix = search_prefix();
            let available = usize::from(layout.search.width).saturating_sub(prefix.width() + 1);
            return Some((
                layout.search.x
                    + (prefix.width() + visible_suffix(input, available).width())
                        .min(usize::from(layout.search.width.saturating_sub(/*rhs*/ 1)))
                        as u16,
                layout.search.y,
            ));
        }
        None
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let layout = self.center_layout(area);
        let inset =
            |area: Rect| area.inner(Margin::new(/*horizontal*/ 2, /*vertical*/ 0));
        let footer_hints = self.center_footer_hints();
        let state = self.state();
        let filter_keys = if state.editing_metadata() {
            String::new()
        } else {
            self.center_filter_hint()
        };
        let grouping = match state.grouping {
            AgentsOverviewGrouping::Project => ui_text("Project", "项目"),
            AgentsOverviewGrouping::Directory => ui_text("Directory", "目录"),
            AgentsOverviewGrouping::Status => ui_text("Status", "状态"),
            AgentsOverviewGrouping::Model => ui_text("Model", "模型"),
        };
        let group_key = self
            .agents_keymap
            .primary_hint("toggle_grouping", &self.agents_keymap.toggle_grouping)
            .map(ShortcutHint::display_label)
            .unwrap_or_default();
        line(
            vec![
                ui_text("Agent command center", "Agent 指挥中心").bold(),
                if crate::i18n::is_chinese(crate::i18n::current_language()) {
                    format!("  分组：{grouping}  {group_key}")
                } else {
                    format!("  Group: {grouping}  {group_key}")
                }
                .dim(),
            ],
            inset(layout.header),
            buf,
        );
        let labels = TASK_FILTERS
            .iter()
            .map(|(_, group)| {
                let count = self
                    .rows
                    .iter()
                    .filter(|row| group.is_none_or(|group| group == row.group))
                    .count();
                let label = match group {
                    None => ui_text("All", "全部"),
                    Some(AgentsOverviewGroup::NeedsYou) => {
                        ui_text("Needs you", "需要你")
                    }
                    Some(AgentsOverviewGroup::Working) => ui_text("Working", "运行中"),
                    Some(AgentsOverviewGroup::Ready) => ui_text("Ready", "就绪"),
                    Some(AgentsOverviewGroup::Finished) => ui_text("Inactive", "未活动"),
                };
                format!("{label} {count}")
            })
            .collect::<Vec<_>>();
        let mut tabs = inset(row(layout.header, /*offset*/ 1, /*height*/ 1));
        let filter_hint = hint_line(&[(filter_keys, ui_text("filter", "筛选").into())]);
        if tabs.width >= 80 && filter_hint.width() > 0 {
            let hint_width = filter_hint.width() as u16;
            filter_hint.render(
                Rect::new(tabs.right() - hint_width, tabs.y, hint_width, tabs.height),
                buf,
            );
            tabs.width = tabs.width.saturating_sub(hint_width + 2);
        }
        render_filled_tab_bar(
            &labels.iter().map(String::as_str).collect::<Vec<_>>(),
            state.status_filter,
            tabs,
            buf,
        );
        line(
            "─"
                .repeat(usize::from(area.width.saturating_sub(/*rhs*/ 4)))
                .dim(),
            inset(row(layout.header, /*offset*/ 2, /*height*/ 1)),
            buf,
        );
        if state.editing_metadata() {
            buf.set_style(layout.search, crate::bottom_pane::active_tab_style());
            if state.rename_target.is_some() {
                line(rename_prefix().cyan().bold(), layout.search, buf);
                let mut area = layout.search;
                let prefix_width = rename_prefix().width().min(usize::from(area.width)) as u16;
                area.x += prefix_width;
                area.width -= prefix_width;
                WidgetRef::render_ref(&&state.input, area, buf);
            } else {
                let label = search_prefix();
                let available = usize::from(layout.search.width).saturating_sub(label.width() + 1);
                line(
                    vec![
                        label.cyan().bold(),
                        visible_suffix(&state.search, available).to_owned().into(),
                    ],
                    layout.search,
                    buf,
                );
            }
        }
        let notice = state
            .connection_notice
            .map(str::to_owned)
            .or_else(|| {
                state
                    .creating_worktree
                    .then(|| ui_text("Creating worktree…", "正在创建工作树…").to_owned())
            })
            .or_else(|| {
                state.removal.map(|action| match action {
                    AgentsOverviewAction::Archive => ui_text(
                        "Archiving task…  You can still browse.",
                        "正在归档任务…  仍可继续浏览。",
                    )
                    .to_owned(),
                    AgentsOverviewAction::Delete => ui_text(
                        "Deleting task…  You can still browse.",
                        "正在删除任务…  仍可继续浏览。",
                    )
                    .to_owned(),
                })
            })
            .or_else(|| {
                (state.refresh_failed && !state.loading)
                    .then(|| ui_text("Error loading tasks", "加载任务失败").to_owned())
            })
            .or_else(|| state.server_version_notice.clone());
        if let Some(notice) = notice.filter(|_| !state.help && state.key_chord_hint.is_none()) {
            line(notice.dim(), inset(layout.footer), buf);
        } else {
            line(hint_line(&footer_hints), inset(layout.footer), buf);
        }
        let help = state.help;
        drop(state);
        if help {
            let body = row(
                area,
                layout.header.height,
                layout.footer.y - layout.header.bottom(),
            );
            Clear.render(inset(body), buf);
            let body = inset(body);
            let mut lines = self.center_help_lines(body.width);
            if lines.len() > usize::from(body.height) {
                lines.truncate(usize::from(body.height.saturating_sub(/*rhs*/ 1)));
                lines.push(ui_text("… resize to see all", "… 调整窗口大小以查看全部").dim().into());
            }
            Paragraph::new(lines).render(body, buf);
            return;
        }
        for y in layout.gap.y..layout.gap.bottom() {
            if layout.gap.width > 1 {
                buf[(layout.gap.x + 1, y)]
                    .set_symbol("│")
                    .set_style(Style::default().dim());
            }
        }
        self.render_center_rows(layout.list, buf);
        if !layout.details.is_empty() {
            self.render_details(layout.details, buf);
        }
    }
}
