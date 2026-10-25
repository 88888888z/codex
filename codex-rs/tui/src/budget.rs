//! Context-budget inspection and one-key compaction for the current turn.

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Widget;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::i18n::ui_text;
use crate::key_hint::KeyBindingListExt;
use crate::keymap::PagerKeymap;
use crate::status::format_tokens_compact;
use crate::style::StatusTone;
use crate::style::accent_color;
use crate::style::focused_border_style;
use crate::style::status_style;
use crate::tui;
use crate::tui::TuiEvent;

#[derive(Clone, Debug)]
pub(crate) struct BudgetSnapshot {
    pub(crate) context_used: Option<i64>,
    pub(crate) context_window: Option<i64>,
    pub(crate) draft_tokens: usize,
    pub(crate) tool_output_tokens: usize,
    pub(crate) history_tokens: usize,
    pub(crate) attachment_count: usize,
}

impl BudgetSnapshot {
    pub(crate) fn estimated_after_send(&self) -> usize {
        self.context_used
            .unwrap_or(self.history_tokens as i64)
            .max(0) as usize
            + self.draft_tokens
    }

    fn used_percent(&self) -> Option<u16> {
        let used = self.context_used?;
        let window = self.context_window?;
        (window > 0).then(|| ((used.max(0) as u128 * 100) / window as u128).min(100) as u16)
    }
}

pub(crate) struct BudgetOverlay {
    snapshot: BudgetSnapshot,
    done: bool,
    app_event_tx: AppEventSender,
    keymap: PagerKeymap,
}

impl BudgetOverlay {
    pub(crate) fn new(
        snapshot: BudgetSnapshot,
        app_event_tx: AppEventSender,
        keymap: PagerKeymap,
    ) -> Self {
        Self {
            snapshot,
            done: false,
            app_event_tx,
            keymap,
        }
    }

    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(focused_border_style())
            .title(ui_text("Context budget", "上下文预算").bold());
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.is_empty() {
            return;
        }

        let footer_height = 2;
        let [content_area, footer_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(footer_height.min(inner.height)),
        ])
        .areas(inner);
        let window = self
            .snapshot
            .context_window
            .map_or_else(|| "?".to_string(), format_tokens_compact);
        let used = self
            .snapshot
            .context_used
            .map_or_else(|| "?".to_string(), format_tokens_compact);
        let history_other = self
            .snapshot
            .context_used
            .map_or(self.snapshot.history_tokens as i64, |used| {
                used.saturating_sub(self.snapshot.tool_output_tokens as i64)
            })
            .max(0) as usize;
        let rows = [
            (
                ui_text("Current context", "当前上下文"),
                if self.snapshot.context_used.is_some() {
                    format!("{used} / {window}")
                } else {
                    ui_text("unavailable", "不可用").to_string()
                },
            ),
            (
                ui_text("Current draft", "本轮草稿"),
                format!(
                    "~{} tokens",
                    format_tokens_compact(self.snapshot.draft_tokens as i64)
                ),
            ),
            (
                ui_text("Tool outputs", "工具输出"),
                format!(
                    "~{} tokens",
                    format_tokens_compact(self.snapshot.tool_output_tokens as i64)
                ),
            ),
            (
                ui_text("History and system", "历史与系统"),
                format!("~{} tokens", format_tokens_compact(history_other as i64)),
            ),
            (
                ui_text("Attachments", "附件"),
                self.snapshot.attachment_count.to_string(),
            ),
            (
                ui_text("Estimated after send", "发送后预计"),
                format!(
                    "~{} tokens",
                    format_tokens_compact(self.snapshot.estimated_after_send() as i64)
                ),
            ),
        ];

        let label_width = rows
            .iter()
            .map(|(label, _)| label.chars().count())
            .max()
            .unwrap_or(0) as u16;
        let mut y = content_area.y;
        for (label, value) in rows {
            if y >= content_area.bottom() {
                break;
            }
            let line = Line::from(vec![
                Span::styled(
                    format!("{label:<width$}", width = usize::from(label_width)),
                    Style::default().bold(),
                ),
                "  ".into(),
                Span::styled(value, Style::default().fg(accent_color())),
            ]);
            line.render(Rect::new(content_area.x, y, content_area.width, 1), buf);
            y = y.saturating_add(1);
        }
        if y < content_area.bottom()
            && let Some(percent) = self.snapshot.used_percent()
        {
            y = y.saturating_add(1);
            let bar_width = content_area.width.saturating_sub(6).max(1);
            let filled =
                (u32::from(bar_width) * u32::from(percent) / 100).min(u32::from(bar_width)) as u16;
            let tone = if percent >= 85 {
                StatusTone::Failure
            } else if percent >= 60 {
                StatusTone::Attention
            } else {
                StatusTone::Success
            };
            Line::from(vec![
                Span::styled("█".repeat(usize::from(filled)), status_style(tone)),
                "░"
                    .repeat(usize::from(bar_width.saturating_sub(filled)))
                    .dim(),
                format!(" {percent}%").dim(),
            ])
            .render(Rect::new(content_area.x, y, content_area.width, 1), buf);
        }

        let note = ui_text(
            "Draft, tool, and history values are local estimates.",
            "草稿、工具与历史数值为本地估算。",
        );
        Line::from(note).dim().render(
            Rect::new(footer_area.x, footer_area.y, footer_area.width, 1),
            buf,
        );
        let hints = format!(
            "c {} · r {} · Esc {}",
            ui_text("compact now", "立即压缩"),
            ui_text("refresh", "刷新"),
            ui_text("close", "关闭"),
        );
        Line::from(hints).dim().render(
            Rect::new(footer_area.x, footer_area.y + 1, footer_area.width, 1),
            buf,
        );
    }

    pub(crate) fn handle_event(
        &mut self,
        tui: &mut tui::Tui,
        event: TuiEvent,
    ) -> std::io::Result<()> {
        match event {
            TuiEvent::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key),
            TuiEvent::Draw | TuiEvent::Resume | TuiEvent::Resize(_) | TuiEvent::FocusGained => {
                tui.draw(u16::MAX, |frame| self.render(frame.area(), frame.buffer))?;
            }
            _ => {}
        }
        tui.frame_requester().schedule_frame();
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if self.keymap.close.is_pressed(key) || key.code == KeyCode::Esc {
            self.done = true;
        } else if key.modifiers.is_empty() && key.code == KeyCode::Char('c') {
            self.app_event_tx.send(AppEvent::CompactBudgetContext);
            self.done = true;
        } else if key.modifiers.is_empty() && key.code == KeyCode::Char('r') {
            self.app_event_tx.send(AppEvent::OpenBudget);
            self.done = true;
        }
    }

    pub(crate) fn is_done(&self) -> bool {
        self.done
    }
}
