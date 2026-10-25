//! Five-hour and weekly period tables share grouping, with independent row expansion.
use super::AnalyticsView;
use super::data;
use super::plan;
use super::render::columns;
use super::render::join_columns;
use super::sections::Section;
use super::styles::secondary_style;
use crate::keymap::ListAction;
use codex_backend_client::PlanLimitDimension;
use ratatui::style::Styled;
use ratatui::style::Stylize;
use ratatui::text::Line;
use std::ops::Range;

impl AnalyticsView {
    pub(super) fn plan_action(&mut self, action: Option<ListAction>) {
        let window = self.plan.window;
        let periods = self
            .plan
            .report
            .ready()
            .map(|report| &report.periods[window]);
        let count = periods.map_or(/*default*/ 0, Vec::len);
        let mut scroll_offset = self.scroll_offset();
        let cursor = &mut self.plan.cursor[window];
        let previous_cursor = *cursor;
        match action {
            Some(ListAction::MoveLeft | ListAction::MoveRight) => self.plan.window ^= 1,
            Some(ListAction::MoveUp) => *cursor = cursor.saturating_sub(/*rhs*/ 1),
            Some(ListAction::MoveDown) => {
                *cursor = (*cursor + 1).min(count.saturating_sub(/*rhs*/ 1))
            }
            Some(ListAction::JumpTop) => *cursor = 0,
            Some(ListAction::JumpBottom) => *cursor = count.saturating_sub(/*rhs*/ 1),
            Some(ListAction::Accept) => {
                if let Some(period) = periods.and_then(|periods| periods.get(*cursor)) {
                    self.plan.expanded[window] =
                        if self.plan.expanded[window].as_ref() == Some(&period.id) {
                            None
                        } else {
                            Some(period.id.clone())
                        };
                }
            }
            Some(ListAction::Cancel) => {
                self.is_done |= self.plan.expanded[window].take().is_none();
            }
            Some(ListAction::PageDown) => {
                scroll_offset += self.viewport_height;
                self.follow_selection = false;
            }
            Some(ListAction::PageUp) => {
                scroll_offset = scroll_offset.saturating_sub(self.viewport_height);
                self.follow_selection = false;
            }
            None => {}
        }
        if *cursor != previous_cursor {
            self.plan.expanded[window] = None;
        }
        self.sections[Section::Plan].follow_selection_on_focus |=
            !self.zoomed && (self.plan.window != window || *cursor != previous_cursor);
        *self.scroll_offset_mut() = scroll_offset;
    }

    pub(super) fn plan_lines(&self, width: usize) -> (Vec<Line<'static>>, Range<usize>) {
        let side_by_side = width >= 100;
        let table_width = if side_by_side { (width - 4) / 2 } else { width };
        let group = self.sections[Section::Plan].group;
        let dimension = match group {
            1 => PlanLimitDimension::ThreadSource,
            2 => PlanLimitDimension::Model,
            3 => PlanLimitDimension::TurnTrigger,
            _ => PlanLimitDimension::Surface,
        };
        let mut tables = Vec::new();
        let mut selections = Vec::new();
        for (window, title) in [
            crate::i18n::ui_text("5-hour limits", "5 小时额度"),
            crate::i18n::ui_text("Weekly limits", "每周额度"),
        ]
        .into_iter()
        .enumerate()
        {
            let heading = format!(
                "{} {title}",
                if self.plan.window == window {
                    "›"
                } else {
                    " "
                }
            );
            let mut lines = vec![heading.bold().into(), Line::default()];
            let mut selection = 0..2;
            if let Some(report) = self.plan.report.ready() {
                if report.periods[window].is_empty() {
                    lines.push(
                        if report.coverage_complete {
                            crate::i18n::ui_text(
                                "No limit periods in this date range",
                                "此时间范围没有额度周期",
                            )
                        } else {
                            crate::i18n::ui_text(
                                "Limit history isn't available yet",
                                "额度历史尚不可用",
                            )
                        }
                        .set_style(secondary_style())
                        .into(),
                    );
                }
                for (index, period) in report.periods[window]
                    .iter()
                    .take(if self.zoomed { usize::MAX } else { 1 })
                    .enumerate()
                {
                    let start = lines.len();
                    let selected = self.plan.cursor[window] == index;
                    let label = format!(
                        "{} {} – {}{}",
                        if selected { "›" } else { " " },
                        period.start.format(self.clock_format.date_time_format()),
                        period.end.format(self.clock_format.date_time_format()),
                        if period.start <= report.as_of && period.end > report.as_of {
                            " *"
                        } else {
                            ""
                        }
                    );
                    let amount = period
                        .used
                        .map(|value| format!("{}%", data::amount(value / 100.0)))
                        .unwrap_or_else(|| {
                            crate::i18n::ui_text("Not available", "不可用").into()
                        });
                    let mut row = columns(label.into(), amount.into(), table_width);
                    if selected && self.plan.window == window {
                        super::styles::select_row(&mut row, table_width);
                    }
                    lines.push(row);
                    if self.zoomed && self.plan.expanded[window].as_ref() == Some(&period.id) {
                        if !period.complete {
                            lines.push(
                                "  Some usage is unavailable; this period may be incomplete"
                                    .dim()
                                    .into(),
                            );
                        }
                        if let Some(breakdown) = period.breakdowns.as_ref().and_then(|groups| {
                            groups.iter().find(|group| group.dimension == dimension)
                        }) {
                            for row in &breakdown.rows {
                                let key = if dimension == PlanLimitDimension::TurnTrigger {
                                    format!("start-{}", row.key)
                                } else {
                                    row.key.clone()
                                };
                                let label = if dimension == PlanLimitDimension::Model {
                                    self.model_name(&key)
                                } else {
                                    super::normalize::label(&key)
                                };
                                lines.push(columns(
                                    format!("  {label}").into(),
                                    format!("{}%", data::amount(row.basis_points / 100.0)).into(),
                                    table_width,
                                ));
                            }
                        } else {
                            lines.push("  Breakdown isn't available for this period".dim().into());
                        }
                    }
                    if selected {
                        selection = start..lines.len();
                    }
                }
            } else {
                lines.push(
                    match &self.plan.report {
                        data::Load::Unavailable => crate::i18n::ui_text(
                            "Limit history isn't available yet",
                            "额度历史尚不可用",
                        ),
                        state => state
                            .message()
                            .unwrap_or(crate::i18n::ui_text(
                                "Limit history isn't available yet",
                                "额度历史尚不可用",
                            )),
                    }
                    .to_string()
                    .set_style(secondary_style())
                    .into(),
                );
            }
            // Wrap each column before joining so long timestamps never collide with its neighbor.
            let mut wrapped = Vec::new();
            let mut wrapped_selection = 0..0;
            for (index, line) in lines.into_iter().enumerate() {
                if index == selection.start {
                    wrapped_selection.start = wrapped.len();
                }
                wrapped.extend(crate::wrapping::word_wrap_lines(
                    [line],
                    crate::wrapping::RtOptions::new(table_width.max(/*other*/ 1)),
                ));
                if index + 1 == selection.end {
                    wrapped_selection.end = wrapped.len();
                }
            }
            tables.push(wrapped);
            selections.push(wrapped_selection);
        }
        let mut selection = selections[self.plan.window].clone();
        let mut lines = if side_by_side {
            join_columns(&tables[0], &tables[1], table_width)
        } else {
            if self.plan.window == 1 {
                selection =
                    selection.start + tables[0].len() + 1..selection.end + tables[0].len() + 1;
            }
            let mut lines = std::mem::take(&mut tables[0]);
            lines.push(Line::default());
            lines.append(&mut tables[1]);
            lines
        };
        if group == 3 {
            lines.push(
                crate::i18n::ui_text(
                    "By turn start includes Tasks only; percentages use the full period limit",
                    "按轮次开始时间仅统计任务；百分比使用完整周期额度",
                )
                    .dim()
                    .into(),
            );
        }
        if let Some(plan::Report {
            as_of,
            coverage_start,
            coverage_complete,
            approximate,
            ..
        }) = self.plan.report.ready()
        {
            if !*coverage_complete {
                lines.push(
                    crate::i18n::ui_text(
                        "Some periods aren't available yet",
                        "部分周期尚不可用",
                    )
                    .dim()
                    .into(),
                );
            }
            let timestamp = as_of.format(self.clock_format.date_time_format());
            lines.push(
                if crate::i18n::is_chinese(crate::i18n::current_language()) {
                    format!("用量截至 {timestamp} UTC · * 表示上次更新时进行中")
                } else {
                    format!("Usage as of {timestamp} UTC · * current at last update")
                }
                .dim()
                .into(),
            );
            if let Some(start) = coverage_start {
                let start = start.format(self.clock_format.date_time_format());
                lines.push(
                    if crate::i18n::is_chinese(crate::i18n::current_language()) {
                        format!("自 {start} UTC 起可用")
                    } else {
                        format!("Available since {start} UTC")
                    }
                    .dim()
                    .into(),
                );
            }
            if *approximate {
                lines.push(
                    crate::i18n::ui_text(
                        "Amounts and period boundaries are approximate; recent activity may be delayed",
                        "金额和周期边界为近似值；最近活动可能延迟",
                    )
                        .dim()
                        .into(),
                );
            }
        }
        (lines, selection)
    }
}
