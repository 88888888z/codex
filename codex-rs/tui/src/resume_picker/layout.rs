//! Session-picker chrome shares its geometry with paging and rendering.
//! Decorative gaps collapse before the list loses room for two comfortable rows.

use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::style::Styled;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Widget;

use super::PickerState;
use super::PickerPaneFocus;
use super::list_viewport_width;
use super::render_picker_footer;
use super::render_tree_list;
use super::render_transcript_loading_overlay;
use super::search_line;
use super::toolbar_for_width;

pub(super) struct PickerAreas {
    header: Rect,
    toolbar: Rect,
    search: Rect,
    pub(super) list: Rect,
    footer: Rect,
}

pub(super) fn areas(area: Rect) -> PickerAreas {
    const FIXED_CHROME_ROWS: u16 = 7;
    const MIN_LIST_ROWS: u16 = 6;
    let gaps = area
        .height
        .saturating_sub(FIXED_CHROME_ROWS + MIN_LIST_ROWS)
        .min(/*other*/ 3);
    let [header, _, toolbar, _, search, _, list, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(u16::from(gaps >= 3)),
        Constraint::Length(1),
        Constraint::Length(u16::from(gaps >= 2)),
        Constraint::Length(1),
        Constraint::Length(u16::from(gaps >= 1)),
        Constraint::Min(area.height.saturating_sub(FIXED_CHROME_ROWS + gaps)),
        Constraint::Length(4),
    ])
    .areas(area);
    PickerAreas {
        header,
        toolbar,
        search,
        list,
        footer,
    }
}

pub(super) fn render(frame: &mut crate::custom_terminal::Frame, state: &PickerState) {
    let PickerAreas {
        header,
        toolbar,
        search,
        list,
        footer,
    } = areas(frame.area());
    let chrome = |area: Rect| {
        Rect::new(
            area.x.saturating_add(/*rhs*/ 1),
            area.y,
            area.width.saturating_sub(/*rhs*/ 2),
            area.height,
        )
    };
    frame.render_widget_ref(&Line::from(state.action.title().bold()), chrome(header));
    let toolbar = chrome(toolbar);
    frame.render_widget_ref(&toolbar_for_width(state, toolbar.width), toolbar);
    let search = chrome(search);
    frame.render_widget_ref(&search_line(state, search.width), search);
    let body = Rect::new(
        list.x.saturating_add(/*rhs*/ 2),
        list.y,
        list_viewport_width(list.width),
        list.height,
    );
    let (tree_area, view_area) = body_areas(body);
    let tree_block = session_panel(
        crate::i18n::ui_text("Sessions", "会话"),
        state.focus == PickerPaneFocus::Tree,
    );
    let tree_inner = tree_block.inner(tree_area);
    tree_block.render(tree_area, frame.buffer);
    Clear.render(tree_inner, frame.buffer);
    render_tree_list(frame, tree_inner, state);

    let view_block = session_panel(
        crate::i18n::ui_text("Recent 6 turns", "最近 6 轮"),
        state.focus == PickerPaneFocus::View,
    );
    let view_inner = view_block.inner(view_area);
    view_block.render(view_area, frame.buffer);
    Clear.render(view_inner, frame.buffer);
    let view_content = Rect::new(
        view_inner.x.saturating_add(/*horizontal padding*/ 2),
        view_inner.y,
        view_inner.width.saturating_sub(/*horizontal padding*/ 4),
        view_inner.height,
    );
    if let Some(cells) = state.selected_preview_transcript_cells() {
        state
            .preview_transcript_view
            .borrow_mut()
            .render(view_content, frame.buffer, &cells);
    } else if state.selected_preview_transcript_failed() {
        crate::i18n::ui_text("Could not load transcript", "无法加载对话记录")
            .set_style(crate::style::status_style(
                crate::style::StatusTone::Failure,
            ))
            .render(view_content, frame.buffer);
    } else {
        render_transcript_loading_overlay(frame, view_content);
    }
    render_picker_footer(frame, footer, state, tree_inner.height);
}

pub(super) fn body_areas(body: Rect) -> (Rect, Rect) {
    const SIDE_BY_SIDE_MIN_WIDTH: u16 = 84;
    if body.width >= SIDE_BY_SIDE_MIN_WIDTH {
        let tree_width = body
            .width
            .saturating_mul(/*percent*/ 43)
            .max(/*min*/ 36)
            .min(/*max*/ 72)
            .min(body.width.saturating_sub(/*view*/ 32));
        let [tree_area, view_area] = Layout::horizontal([
            Constraint::Length(tree_width),
            Constraint::Min(1),
        ])
        .areas(body);
        (tree_area, view_area)
    } else {
        let tree_height = body
            .height
            .saturating_mul(/*percent*/ 42)
            .max(/*min*/ 4)
            .min(body.height.saturating_sub(/*view*/ 4));
        let [tree_area, view_area] = Layout::vertical([
            Constraint::Length(tree_height),
            Constraint::Min(1),
        ])
        .areas(body);
        (tree_area, view_area)
    }
}

fn session_panel(title: &'static str, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if focused {
            crate::style::focused_border_style()
        } else {
            crate::style::hairline_style()
        })
        .title(Line::from(title).style(Style::default().bold()))
        .style(Style::reset())
}
