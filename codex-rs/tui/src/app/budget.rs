//! Context-budget snapshot assembly for the `/budget` overlay.

use super::*;
use crate::budget::BudgetOverlay;
use crate::budget::BudgetSnapshot;

impl App {
    pub(super) fn open_budget_overlay(&mut self, tui: &mut tui::Tui) -> Result<()> {
        let (context_used, context_window) = self.chat_widget.context_budget_usage();
        let draft_tokens =
            codex_utils_string::approx_token_count(&self.chat_widget.composer_text_with_pending());
        let attachment_count = self.chat_widget.composer_attachment_count();
        let (history_tokens, tool_output_tokens) =
            estimate_transcript_tokens(&self.transcript_cells);
        tui.enter_alt_screen()?;
        self.overlay = Some(Overlay::new_budget(BudgetOverlay::new(
            BudgetSnapshot {
                context_used,
                context_window,
                draft_tokens,
                tool_output_tokens,
                history_tokens,
                attachment_count,
            },
            self.app_event_tx.clone(),
            self.keymap.pager.clone(),
        )));
        tui.frame_requester().schedule_frame();
        Ok(())
    }

    pub(super) fn compact_from_budget(&mut self) {
        self.chat_widget.compact_context();
    }
}

fn estimate_transcript_tokens(
    cells: &[Arc<dyn crate::history_cell::HistoryCell>],
) -> (usize, usize) {
    let mut history_tokens: usize = 0;
    let mut tool_output_tokens: usize = 0;
    for cell in cells {
        let text = cell
            .raw_lines()
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let tokens = codex_utils_string::approx_token_count(&text);
        history_tokens = history_tokens.saturating_add(tokens);
        if cell.as_any().is::<crate::exec_cell::ExecCell>()
            || cell.as_any().is::<crate::history_cell::McpToolCallCell>()
            || cell
                .as_any()
                .is::<crate::history_cell::DynamicToolCallCell>()
        {
            tool_output_tokens = tool_output_tokens.saturating_add(tokens);
        }
    }
    (history_tokens, tool_output_tokens)
}
