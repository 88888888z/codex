//! Adaptive workbench chrome for the fullscreen transcript.
//!
//! The workbench keeps navigation and context visible without changing the inline transcript.
//! Narrow terminals fall back to the existing single-column layout.

use super::*;
use crate::exec_cell::ExecCell;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::slash_command::SlashCommand;
use crate::status::format_tokens_compact;
use crate::style::StatusTone;
use crate::style::accent_color;
use crate::style::accent_style;
use crate::style::status_style;
use crate::usage_cost::ModelCostEstimate;
use crossterm::event::MouseEvent;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Styled;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Widget;

const WORKBENCH_MIN_WIDTH: u16 = 100;
const WORKBENCH_MIN_HEIGHT: u16 = 18;
const WORKBENCH_MIN_BODY_HEIGHT: u16 = 12;
const WORKBENCH_GAP: u16 = 1;

#[derive(Clone, Copy, Debug)]
pub(super) struct WorkbenchColumns {
    pub(super) main_width: u16,
    left_width: u16,
    right_width: u16,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WorkbenchAreas {
    pub(super) main: Rect,
    left: Rect,
    right: Rect,
}

pub(super) struct WorkbenchSnapshot {
    chinese: bool,
    model: String,
    running: bool,
    plan_lines: Vec<String>,
    context_used: Option<i64>,
    context_window: Option<i64>,
    draft_tokens: usize,
    cached_tokens: i64,
    uncached_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    estimated_cost: Option<ModelCostEstimate>,
    attachment_count: usize,
    latest_tool: Option<ToolReceipt>,
}

struct ToolReceipt {
    kind: String,
    command: String,
    detail: String,
    status: String,
    tone: StatusTone,
}

impl WorkbenchColumns {
    fn new(width: u16) -> Option<Self> {
        if width < WORKBENCH_MIN_WIDTH {
            return None;
        }
        let left_width = (width / 7).clamp(/*min*/ 18, /*max*/ 22);
        let right_width = (width / 5).clamp(/*min*/ 24, /*max*/ 32);
        let main_width = width.checked_sub(left_width + right_width + WORKBENCH_GAP * 2)?;
        if main_width < 48 {
            return None;
        }
        (left_width + main_width + right_width + WORKBENCH_GAP * 2 == width).then_some(Self {
            main_width,
            left_width,
            right_width,
        })
    }

    pub(super) fn areas(self, height: u16) -> WorkbenchAreas {
        let left = Rect::new(/*x*/ 0, /*y*/ 0, self.left_width, height);
        let main = Rect::new(
            self.left_width + WORKBENCH_GAP,
            /*y*/ 0,
            self.main_width,
            height,
        );
        let right = Rect::new(
            main.right() + WORKBENCH_GAP,
            /*y*/ 0,
            self.right_width,
            height,
        );
        WorkbenchAreas { main, left, right }
    }
}

impl App {
    pub(super) fn workbench_columns(&self, width: u16, height: u16) -> Option<WorkbenchColumns> {
        (!self.workbench_collapsed && height >= WORKBENCH_MIN_HEIGHT)
            .then(|| WorkbenchColumns::new(width))
            .flatten()
    }

    pub(super) fn workbench_snapshot(&self) -> WorkbenchSnapshot {
        let (context_used, context_window) = self.chat_widget.context_budget_usage();
        let draft_tokens =
            codex_utils_string::approx_token_count(&self.chat_widget.composer_text_with_pending());
        let token_usage = self.chat_widget.token_usage();
        let cached_tokens = token_usage.cached_input_tokens.max(0);
        let uncached_tokens = token_usage
            .input_tokens
            .saturating_sub(cached_tokens)
            .max(0);
        let output_tokens = token_usage.output_tokens.max(0);
        let estimated_cost = self.chat_widget.estimated_session_cost();
        WorkbenchSnapshot {
            chinese: crate::i18n::is_chinese(crate::i18n::current_language()),
            model: self.chat_widget.model_display_name().to_string(),
            running: self.chat_widget.is_agent_turn_running(),
            plan_lines: latest_plan_lines(&self.transcript_cells),
            context_used,
            context_window,
            draft_tokens,
            cached_tokens,
            uncached_tokens,
            output_tokens,
            total_tokens: token_usage
                .total_tokens
                .max(token_usage.input_tokens.saturating_add(output_tokens)),
            estimated_cost,
            attachment_count: self.chat_widget.composer_attachment_count(),
            latest_tool: latest_tool_receipt(
                &self.transcript_cells,
                crate::i18n::is_chinese(crate::i18n::current_language()),
            ),
        }
    }

    /// Consume workbench-only shortcuts before the composer handles them.
    pub(super) fn handle_workbench_key(&mut self, key: KeyEvent) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if key.code == KeyCode::F(2) && key.modifiers == KeyModifiers::ALT {
            self.workbench_collapsed = !self.workbench_collapsed;
            return true;
        }
        if key.modifiers != KeyModifiers::ALT {
            return false;
        }
        let view = match key.code {
            KeyCode::Char('1') => WorkbenchView::Chat,
            KeyCode::Char('2') => WorkbenchView::Diff,
            KeyCode::Char('3') => WorkbenchView::Files,
            KeyCode::Char('4') => WorkbenchView::Sessions,
            KeyCode::Char('5') => WorkbenchView::Verify,
            _ => return false,
        };
        self.activate_workbench_view(view);
        true
    }

    pub(super) fn handle_workbench_mouse(&mut self, mouse: MouseEvent, screen_size: Size) -> bool {
        if !matches!(
            mouse.kind,
            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
        ) || self.chat_widget.has_active_modal()
        {
            return false;
        }
        let Some(columns) = self.workbench_columns(screen_size.width, screen_size.height) else {
            return false;
        };
        let left = columns.areas(screen_size.height).left;
        if mouse.column <= left.x
            || mouse.column >= left.right().saturating_sub(1)
            || mouse.row <= left.y
        {
            return false;
        }
        let Some(view) = WorkbenchView::ALL.get(usize::from(mouse.row.saturating_sub(left.y + 1)))
        else {
            return false;
        };
        self.activate_workbench_view(*view);
        true
    }

    fn activate_workbench_view(&mut self, view: WorkbenchView) {
        match view.command() {
            Some(command) => self.chat_widget.dispatch_workbench_command(command),
            None => {
                self.chat_widget.dismiss_workbench_view();
            }
        }
    }
}

pub(super) fn render_workbench(
    areas: WorkbenchAreas,
    snapshot: &WorkbenchSnapshot,
    buf: &mut Buffer,
) {
    if areas.left.height < WORKBENCH_MIN_BODY_HEIGHT {
        return;
    }
    Clear.render(areas.left, buf);
    Clear.render(areas.right, buf);
    render_left_panel(areas.left, snapshot, buf);
    render_right_panel(areas.right, snapshot, buf);
}

fn render_left_panel(area: Rect, snapshot: &WorkbenchSnapshot, buf: &mut Buffer) {
    let title = crate::i18n::ui_text("Workbench", "工作台");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(
            Style::default()
                .fg(accent_color())
                .add_modifier(Modifier::DIM),
        )
        .style(Style::reset())
        .title(Line::from(format!(" {title} ").set_style(accent_style())));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.is_empty() {
        return;
    }

    let mut lines = Vec::new();
    for view in WorkbenchView::ALL {
        let selected = view == WorkbenchView::Chat;
        let marker = if selected { "▸ " } else { "  " };
        let label = view.label(snapshot.chinese);
        let shortcut = view.shortcut();
        let style = if selected {
            accent_style()
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            marker.set_style(style),
            label.set_style(style),
            format!("  {shortcut}").dim(),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(crate::i18n::ui_text("Model", "模型").bold()));
    lines.push(Line::from(snapshot.model.clone()).dim());
    lines.push(Line::from(vec![
        crate::i18n::ui_text("Status", "状态").bold(),
        "  ".into(),
        if snapshot.running {
            crate::i18n::ui_text("running", "运行中").set_style(status_style(StatusTone::Attention))
        } else {
            crate::i18n::ui_text("idle", "空闲").set_style(status_style(StatusTone::Success))
        },
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(crate::i18n::ui_text("Plan", "计划").bold()));
    if snapshot.plan_lines.is_empty() {
        lines.push(Line::from(crate::i18n::ui_text("No plan yet", "等待计划")).dim());
    } else {
        lines.extend(
            snapshot
                .plan_lines
                .iter()
                .map(|line| Line::from(line.clone()).dim()),
        );
    }
    Paragraph::new(lines).render(inner, buf);
}

fn render_right_panel(area: Rect, snapshot: &WorkbenchSnapshot, buf: &mut Buffer) {
    let title = crate::i18n::ui_text("Inspector", "检查器");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(
            Style::default()
                .fg(accent_color())
                .add_modifier(Modifier::DIM),
        )
        .style(Style::reset())
        .title(Line::from(format!(" {title} ").set_style(accent_style())));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.is_empty() {
        return;
    }

    let used = snapshot
        .context_used
        .map(format_tokens_compact)
        .unwrap_or_else(|| "?".to_string());
    let window = snapshot
        .context_window
        .map(format_tokens_compact)
        .unwrap_or_else(|| "?".to_string());
    let percent = match (snapshot.context_used, snapshot.context_window) {
        (Some(used), Some(window)) if window > 0 => {
            Some(((used.max(0) as u128 * 100) / window as u128).min(100) as u16)
        }
        _ => None,
    };
    let mut lines = vec![
        Line::from(crate::i18n::ui_text("Context", "上下文").bold()),
        Line::from(vec![
            format!("{used}/{window}").set_style(accent_style()),
            percent
                .map(|percent| format!("  {percent}%").dim())
                .unwrap_or_else(|| "".into()),
        ]),
        Line::from(vec![
            crate::i18n::ui_text("Draft ", "草稿   ").dim(),
            format_tokens_compact(snapshot.draft_tokens as i64).into(),
        ]),
        Line::from(vec![
            crate::i18n::ui_text("Total ", "总 Token ").dim(),
            format_tokens_compact(snapshot.total_tokens).into(),
        ]),
        Line::from(vec![
            crate::i18n::ui_text("Cache hit ", "缓存命中 ").dim(),
            format_tokens_compact(snapshot.cached_tokens).into(),
        ]),
        Line::from(vec![
            crate::i18n::ui_text("Cache miss ", "缓存未命中 ").dim(),
            format_tokens_compact(snapshot.uncached_tokens).into(),
        ]),
        Line::from(vec![
            crate::i18n::ui_text("Output ", "输出   ").dim(),
            format_tokens_compact(snapshot.output_tokens).into(),
        ]),
    ];
    if let Some(cost) = snapshot.estimated_cost.as_ref() {
        lines.push(Line::from(vec![
            crate::i18n::ui_text("Est. cost ", "预计费用 ").dim(),
            format_cost(cost).set_style(accent_style()),
        ]));
    }
    lines.push(Line::from(vec![
        crate::i18n::ui_text("Files ", "附件   ").dim(),
        snapshot.attachment_count.to_string().into(),
    ]));
    if let Some(percent) = percent {
        let bar_width = inner.width.saturating_sub(/*percent label*/ 5).max(1);
        let filled =
            (u32::from(bar_width) * u32::from(percent) / 100).min(u32::from(bar_width)) as u16;
        let tone = if percent >= 85 {
            StatusTone::Failure
        } else if percent >= 60 {
            StatusTone::Attention
        } else {
            StatusTone::Success
        };
        lines.push(Line::from(vec![
            "█"
                .repeat(usize::from(filled))
                .set_style(status_style(tone)),
            "░"
                .repeat(usize::from(bar_width.saturating_sub(filled)))
                .dim(),
            format!(" {percent}%").dim(),
        ]));
        if percent >= 80 {
            lines.push(
                Line::from(crate::i18n::ui_text(
                    "Compact recommended",
                    "建议压缩上下文",
                ))
                .set_style(status_style(StatusTone::Attention)),
            );
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(
        crate::i18n::ui_text("Latest tool", "最近工具").bold(),
    ));
    if let Some(tool) = &snapshot.latest_tool {
        lines.push(Line::from(vec![
            tool.kind.clone().bold(),
            "  ".into(),
            tool.status.clone().set_style(status_style(tool.tone)),
        ]));
        lines.push(Line::from(tool.command.clone()).dim());
        lines.push(Line::from(tool.detail.clone()).dim());
    } else {
        lines.push(Line::from(crate::i18n::ui_text("No tool call yet", "暂无工具调用")).dim());
    }

    let lines: Vec<_> = lines
        .into_iter()
        .map(|line| truncate_line_with_ellipsis_if_overflow(line, usize::from(inner.width)))
        .collect();
    Paragraph::new(lines).render(inner, buf);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkbenchView {
    Chat,
    Diff,
    Files,
    Sessions,
    Verify,
}

impl WorkbenchView {
    const ALL: [Self; 5] = [
        Self::Chat,
        Self::Diff,
        Self::Files,
        Self::Sessions,
        Self::Verify,
    ];

    fn label(self, chinese: bool) -> &'static str {
        if chinese {
            match self {
                Self::Chat => "对话",
                Self::Diff => "差异",
                Self::Files => "文件",
                Self::Sessions => "会话",
                Self::Verify => "验证",
            }
        } else {
            match self {
                Self::Chat => "Chat",
                Self::Diff => "Diff",
                Self::Files => "Files",
                Self::Sessions => "Sessions",
                Self::Verify => "Verify",
            }
        }
    }

    fn shortcut(self) -> &'static str {
        match self {
            Self::Chat => "Alt+1",
            Self::Diff => "Alt+2",
            Self::Files => "Alt+3",
            Self::Sessions => "Alt+4",
            Self::Verify => "Alt+5",
        }
    }

    fn command(self) -> Option<SlashCommand> {
        match self {
            Self::Chat => None,
            Self::Diff => Some(SlashCommand::Diff),
            Self::Files => Some(SlashCommand::Files),
            Self::Sessions => Some(SlashCommand::Resume),
            Self::Verify => Some(SlashCommand::Verify),
        }
    }
}

fn latest_plan_lines(cells: &[Arc<dyn HistoryCell>]) -> Vec<String> {
    let Some(cell) = cells
        .iter()
        .rev()
        .find(|cell| cell.as_any().is::<history_cell::PlanUpdateCell>())
    else {
        return Vec::new();
    };
    cell.raw_lines()
        .into_iter()
        .skip(/*header*/ 1)
        .map(|line| normalize_plan_line(line.to_string()))
        .filter(|line| !line.is_empty())
        .take(/*lines*/ 5)
        .collect()
}

fn normalize_plan_line(line: String) -> String {
    if let Some(step) = line.strip_prefix("Completed: ") {
        format!("✓ {step}")
    } else if let Some(step) = line.strip_prefix("InProgress: ") {
        format!("▸ {step}")
    } else if let Some(step) = line.strip_prefix("Pending: ") {
        format!("○ {step}")
    } else {
        line
    }
}

fn latest_tool_receipt(cells: &[Arc<dyn HistoryCell>], chinese: bool) -> Option<ToolReceipt> {
    for cell in cells.iter().rev() {
        if let Some(exec) = cell.as_any().downcast_ref::<ExecCell>() {
            let Some(call) = exec.iter_calls().last() else {
                continue;
            };
            let (status, tone) = match call.output.as_ref() {
                Some(output) if output.exit_code == 0 => (
                    crate::i18n::ui_text("ok", "成功").to_string(),
                    StatusTone::Success,
                ),
                Some(output) => (
                    if chinese {
                        format!("失败 {}", output.exit_code)
                    } else {
                        format!("failed {}", output.exit_code)
                    },
                    StatusTone::Failure,
                ),
                None => (
                    crate::i18n::ui_text("running", "运行中").to_string(),
                    StatusTone::Attention,
                ),
            };
            let duration = call
                .duration
                .map(format_duration)
                .unwrap_or_else(|| crate::i18n::ui_text("pending", "等待完成").to_string());
            let output_lines = call.output.as_ref().map_or(0, |output| output.line_count());
            let detail = if chinese {
                format!("输出 {output_lines} 行 · {duration}")
            } else {
                format!("{output_lines} lines · {duration}")
            };
            return Some(ToolReceipt {
                kind: crate::i18n::ui_text("Shell", "命令").to_string(),
                command: call.command.join(" "),
                detail,
                status,
                tone,
            });
        }
        if cell.as_any().is::<history_cell::McpToolCallCell>()
            || cell.as_any().is::<history_cell::DynamicToolCallCell>()
        {
            return Some(ToolReceipt {
                kind: if cell.as_any().is::<history_cell::McpToolCallCell>() {
                    "MCP".to_string()
                } else {
                    crate::i18n::ui_text("Tool", "工具").to_string()
                },
                command: crate::i18n::ui_text("Tool call", "工具调用").to_string(),
                detail: crate::i18n::ui_text("Full output in transcript", "完整输出见对话记录")
                    .to_string(),
                status: crate::i18n::ui_text("recorded", "已记录").to_string(),
                tone: StatusTone::Success,
            });
        }
    }
    None
}

fn format_duration(duration: Duration) -> String {
    if duration.as_secs() >= 1 {
        format!("{:.1}s", duration.as_secs_f32())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

fn format_cost(cost: &ModelCostEstimate) -> String {
    if cost.amount >= 1.0 {
        format!("{}{:.4}", cost.currency, cost.amount)
    } else {
        format!("{}{:.6}", cost.currency, cost.amount)
    }
}
