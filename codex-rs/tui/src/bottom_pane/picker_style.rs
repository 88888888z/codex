//! Shared visual tokens and overflow hints for the picker component.
//!
//! Resolve RGB colors when rendering so terminal palette fallbacks stay effective.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Widget;

use super::selection_popup_common::RenderedRows;
pub(crate) use crate::style::selection_style;

/// The shared picker can blend into the transcript or form a shaded browser panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PickerSurface {
    #[default]
    Terminal,
    Panel,
}

pub(crate) fn active_tab_style() -> Style {
    selection_style()
}

/// Draw overflow hints against the background already painted beneath each cell.
pub(super) fn render_scroll_indicators(area: Rect, buf: &mut Buffer, rendered: RenderedRows) {
    if area.height < 3 || area.width == 0 {
        return;
    }
    for (visible, symbol, y) in [
        (rendered.has_above, "↑", area.y),
        (rendered.has_below, "↓", area.bottom() - 1),
    ] {
        if visible {
            Line::from(symbol)
                .fg(crate::style::accent_color_on(Some(buf[(area.x, y)].bg)))
                .render(Rect::new(area.x, y, /*width*/ 1, /*height*/ 1), buf);
        }
    }
}
