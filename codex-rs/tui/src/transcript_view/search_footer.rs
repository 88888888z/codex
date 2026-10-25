//! Render Find query input and navigation hints without owning search transitions.

use super::*;
use crate::bottom_pane::TextAreaState;
use ratatui::text::Span;
use ratatui::widgets::StatefulWidgetRef;
use unicode_width::UnicodeWidthStr;

impl TranscriptView {
    /// Project the editor and its caret from the same one-row viewport into the existing footer.
    pub(crate) fn search_footer(&self, width: u16) -> Option<(Line<'static>, u16)> {
        if !self.is_search_editing() || width == 0 {
            return None;
        }
        let area = Rect::new(/*x*/ 0, /*y*/ 0, width, /*height*/ 1);
        let mut buffer = Buffer::empty(area);
        let prefix_width = width.saturating_sub(/*rhs*/ 1).min(/*other*/ 6);
        Line::from("Find: ").dim().render(
            Rect::new(/*x*/ 0, /*y*/ 0, prefix_width, /*height*/ 1),
            &mut buffer,
        );
        let query_area = Rect::new(
            prefix_width,
            /*y*/ 0,
            width - prefix_width,
            /*height*/ 1,
        );
        let mut state = TextAreaState::default();
        StatefulWidgetRef::render_ref(&&self.search.editor, query_area, &mut buffer, &mut state);
        let (cursor_column, _) = self
            .search
            .editor
            .cursor_pos_with_state(query_area, state)?;
        let mut spans = Vec::new();
        let mut column = 0;
        while column < width {
            let cell = &buffer[(column, 0)];
            spans.push(Span::styled(cell.symbol().to_string(), cell.style()));
            column += cell.symbol().width().max(/*other*/ 1) as u16;
        }
        Some((Line::from(spans), cursor_column))
    }
}

impl Search {
    pub(in crate::transcript_view) fn status_line(
        &self,
        width: u16,
        history: TranscriptHistoryState,
    ) -> Line<'static> {
        let previous = crate::key_hint::ctrl(crossterm::event::KeyCode::Char('p')).display_label();
        let next = crate::key_hint::ctrl(crossterm::event::KeyCode::Char('n')).display_label();
        let retry_hint = format!(
            "{previous} {}",
            crate::i18n::ui_text("retry", "重试")
        );
        let unavailable_hint = format!(
            "{} · {retry_hint}",
            crate::i18n::ui_text("History unavailable", "历史记录不可用")
        );
        let next_hint = format!(
            "{} · {previous} {} · {next} {}",
            crate::i18n::ui_text("enter accept", "Enter 接受"),
            crate::i18n::ui_text("older", "上一条"),
            crate::i18n::ui_text("newer", "下一条"),
        );
        let exhausted_hint = format!(
            "{} · {next_hint}",
            crate::i18n::ui_text("No more matches", "没有更多匹配")
        );
        let (status, compact) = match self.progress {
            Progress::Idle | Progress::Found if self.is_reading() => {
                (
                    crate::i18n::ui_text("ctrl+p older · ctrl+n newer", "Ctrl+P 上一条 · Ctrl+N 下一条"),
                    crate::i18n::ui_text("ctrl+p older", "Ctrl+P 上一条"),
                )
            }
            Progress::Idle if !self.editor.is_empty() => (
                next_hint.as_str(),
                crate::i18n::ui_text("enter accept", "Enter 接受"),
            ),
            Progress::Idle => (
                crate::i18n::ui_text("Type to find", "输入以查找"),
                crate::i18n::ui_text("Type to find", "输入以查找"),
            ),
            Progress::Restart | Progress::Scanning(_) => (
                crate::i18n::ui_text("Searching…", "正在搜索…"),
                crate::i18n::ui_text("Searching…", "正在搜索…"),
            ),
            Progress::AwaitingHistory if history == TranscriptHistoryState::Failed => {
                (unavailable_hint.as_str(), retry_hint.as_str())
            }
            Progress::AwaitingHistory => (
                crate::i18n::ui_text("Searching earlier history…", "正在搜索更早的记录…"),
                crate::i18n::ui_text("Loading…", "正在加载…"),
            ),
            Progress::Found => (
                next_hint.as_str(),
                crate::i18n::ui_text("enter accept", "Enter 接受"),
            ),
            Progress::Exhausted if self.is_reading() && self.current.is_some() => (
                crate::i18n::ui_text(
                    "No more matches · ctrl+p older · ctrl+n newer",
                    "没有更多匹配 · Ctrl+P 上一条 · Ctrl+N 下一条",
                ),
                crate::i18n::ui_text("ctrl+p older", "Ctrl+P 上一条"),
            ),
            Progress::Exhausted if self.current.is_some() => {
                (
                    exhausted_hint.as_str(),
                    crate::i18n::ui_text("enter accept", "Enter 接受"),
                )
            }
            Progress::Exhausted => (
                crate::i18n::ui_text("No matches", "没有匹配"),
                crate::i18n::ui_text("No matches", "没有匹配"),
            ),
        };
        let limit = if self.query_truncated {
            crate::i18n::ui_text(
                " · query limited to 4 KiB",
                " · 查询最多 4 KiB",
            )
        } else {
            ""
        };
        if self.is_reading() {
            return crate::footer_hint::first_fitting_line(
                [
                    format!("Find · {status} · esc latest"),
                    format!("Find · {compact} · esc latest"),
                    "esc latest".to_owned(),
                ]
                .map(|hint| crate::transcript_view::footer::navigation_line(&hint)),
                width,
            );
        }
        crate::footer_hint::first_fitting_line(
            [
                format!("{status} · full transcript · esc cancel{limit}"),
                format!("{status} · esc cancel{limit}"),
                format!("{compact} · esc cancel"),
                format!("{compact} · esc"),
                "esc cancel".to_owned(),
            ]
            .map(|hint| crate::transcript_view::footer::navigation_line(&hint)),
            width,
        )
    }
}
