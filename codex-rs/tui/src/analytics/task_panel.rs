//! Consumer chat metrics retain actual balance debits, current-limit comparisons, and partial states.

use super::AnalyticsView;
use super::render::columns;
use super::sections::Section;
use super::styles::number;
use super::styles::secondary_style;
use super::tasks::Chat;
use crate::i18n::ui_text;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow as truncate;
use crate::style::accent_style;
use crate::wrapping::RtOptions;
use crate::wrapping::word_wrap_lines;
use codex_backend_client::TaskUsageAmounts;
use codex_backend_client::TaskUsageStatus;
use ratatui::style::Styled;
use ratatui::style::Stylize;
use ratatui::text::Line;
use std::cmp::Ordering;
use std::ops::Range;

pub(super) fn metric_label(metric: usize) -> &'static str {
    match metric {
        0 => ui_text("Weekly %", "每周 %"),
        1 => ui_text("5-hour %", "5 小时 %"),
        _ => ui_text("Balance credits", "余额额度"),
    }
}

fn compare(a: Option<&TaskUsageAmounts>, b: Option<&TaskUsageAmounts>, metric: usize) -> Ordering {
    if metric == 2 {
        a.and_then(|a| a.balance_usage_credits.as_ref())
            .cmp(&b.and_then(|b| b.balance_usage_credits.as_ref()))
    } else {
        let value = |amounts: &TaskUsageAmounts| {
            if metric == 0 {
                amounts.weekly_limit_percent
            } else {
                amounts.five_hour_limit_percent
            }
        };
        a.and_then(value)
            .partial_cmp(&b.and_then(value))
            .unwrap_or(Ordering::Equal)
    }
}

pub(super) fn amount(amounts: Option<&TaskUsageAmounts>, metric: usize) -> String {
    let Some(amounts) = amounts else {
        return "—".into();
    };
    if metric == 2 {
        amounts
            .balance_usage_credits
            .as_ref()
            .map(|amount| {
                let value = amount.as_str();
                let (mantissa, exponent) = value.split_once(['e', 'E']).unwrap_or((value, "0"));
                let fraction = mantissa
                    .split_once('.')
                    .map_or("", |(_, fraction)| fraction);
                let digits = mantissa.trim_start_matches(['+', '-']).replace('.', "");
                let digits = digits.trim_start_matches('0');
                if digits.is_empty() {
                    return "0.00".to_string();
                }
                let cent_digits = digits.len() as i64
                    + exponent.parse::<i32>().unwrap_or_default() as i64
                    - fraction.len() as i64
                    + 2;
                // Retain tiny adjustments and bound the integer conversion before padding.
                if !(1..=18).contains(&cent_digits) {
                    return value.to_string();
                }
                let mut cents = digits
                    .bytes()
                    .chain(std::iter::repeat(b'0'))
                    .take(cent_digits as usize)
                    .fold(
                        /*init*/ 0_i64,
                        |cents, digit| cents * 10 + i64::from(digit - b'0'),
                    );
                if digits
                    .as_bytes()
                    .get(cent_digits as usize)
                    .is_some_and(|digit| *digit >= b'5')
                {
                    cents += 1;
                }
                cents
                    .checked_mul(/*rhs*/ 10_000)
                    .map(|micros| {
                        super::data::credits(if mantissa.starts_with('-') {
                            -micros
                        } else {
                            micros
                        })
                    })
                    .unwrap_or_else(|| value.to_string())
            })
            .unwrap_or_else(|| "—".into())
    } else {
        (if metric == 0 {
            amounts.weekly_limit_percent
        } else {
            amounts.five_hour_limit_percent
        })
        .map(|value| format!("{value:.1}%"))
        .unwrap_or_else(|| "—".into())
    }
}

pub(super) fn available(chat: &Chat) -> Option<&TaskUsageAmounts> {
    chat.task
        .as_ref()
        .filter(|task| task.data_status != TaskUsageStatus::Unavailable)
        .map(|task| &task.amounts)
}

impl AnalyticsView {
    pub(super) fn task_metrics(&self) -> Vec<usize> {
        (0..3)
            .filter(|metric| {
                *metric == 2
                    || self.tasks.ready().is_some_and(|chats| {
                        chats.rows.iter().any(|chat| {
                            available(chat).is_some_and(|amounts| {
                                if *metric == 0 {
                                    amounts.weekly_limit_percent.is_some()
                                } else {
                                    amounts.five_hour_limit_percent.is_some()
                                }
                            })
                        })
                    })
            })
            .collect()
    }

    pub(super) fn task_metric(&self) -> usize {
        let metrics = self.task_metrics();
        if metrics.contains(&self.chat_metric) {
            self.chat_metric
        } else {
            metrics[0]
        }
    }

    pub(super) fn task_rows(&self) -> Vec<&Chat> {
        let mut rows = self
            .tasks
            .ready()
            .map(|chats| chats.rows.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        rows.sort_by(|a, b| compare(available(b), available(a), self.task_metric()));
        rows
    }

    pub(super) fn task_lines(&self, width: usize) -> (Vec<Line<'static>>, Range<usize>) {
        let row_width = width.clamp(/*min*/ 1, /*max*/ 111);
        let width = row_width.saturating_sub(/*rhs*/ 1).max(/*other*/ 1);
        let wrap = |lines| word_wrap_lines(lines, RtOptions::new(width));
        let Some(chats) = self.tasks.ready() else {
            return (
                wrap(vec![
                    self.tasks
                        .message()
                        .unwrap_or(ui_text("No task usage reported.", "没有任务用量记录。"))
                        .to_string()
                        .into(),
                ]),
                0..1,
            );
        };
        let metric = self.task_metric();
        let mut lines = wrap(vec![
            (if crate::i18n::is_chinese(crate::i18n::current_language()) {
                format!(
                    "用量估算 · 最近 30 天内活跃 · 按{}排序",
                    metric_label(metric)
                )
            } else {
                format!(
                    "Usage estimates · active in past 30 days · sorted by {}",
                    metric_label(metric)
                )
            })
            .set_style(secondary_style())
            .into(),
            if metric == 2 {
                ui_text(
                    "Credits debited from balance · includes adjustments",
                    "从余额扣除的额度 · 含调整",
                )
            } else {
                ui_text(
                    "Recorded usage / current full limit · may exceed 100%",
                    "已记录用量 / 当前完整上限 · 可能超过 100%",
                )
            }
            .set_style(secondary_style())
            .into(),
        ]);
        if chats.rows.is_empty() {
            lines.push(ui_text("No recent local chats.", "没有最近的本地会话。").into());
            return (lines, 0..1);
        }
        let missing = chats
            .rows
            .iter()
            .filter(|chat| amount(available(chat), metric) == "—")
            .count();
        let partial = chats
            .rows
            .iter()
            .filter(|chat| {
                chat.task
                    .as_ref()
                    .is_some_and(|task| task.data_status == TaskUsageStatus::Partial)
            })
            .count();
        if missing + partial > 0 {
            lines.extend(wrap(vec![
                (if crate::i18n::is_chinese(crate::i18n::current_language()) {
                    format!("部分排名 · {partial} 项部分可用 · {missing} 项不可用")
                } else {
                    format!("Partial ranking · {partial} partial · {missing} unavailable")
                })
                    .set_style(secondary_style())
                    .into(),
            ]));
        }
        let metrics = self.task_metrics();
        let widths = metrics
            .iter()
            .map(|index| {
                chats
                    .rows
                    .iter()
                    .map(|chat| amount(available(chat), *index).len())
                    .max()
                    .unwrap_or_default()
                    .max(metric_label(*index).len())
            })
            .collect::<Vec<_>>();
        let show_all = widths.iter().sum::<usize>() + widths.len() * 2 + 24 <= width;
        let metric_text = |chat: Option<&Chat>| {
            if !show_all {
                return chat.map_or_else(
                    || metric_label(metric).to_string(),
                    |chat| amount(available(chat), metric),
                );
            }
            metrics
                .iter()
                .zip(&widths)
                .map(|(index, column_width)| {
                    let value = chat.map_or_else(
                        || metric_label(*index).to_string(),
                        |chat| amount(available(chat), *index),
                    );
                    format!("{value:>column_width$}")
                })
                .collect::<Vec<_>>()
                .join("  ")
        };
        lines.push(Line::default());
        lines.push(columns(
            ui_text("  Chat", "  会话").bold().into(),
            metric_text(/*chat*/ None).bold().into(),
            width,
        ));
        let rows = self
            .task_rows()
            .into_iter()
            .enumerate()
            .map(|(index, chat)| {
                let selected =
                    self.section == Section::Chats && index == self.sections[Section::Chats].cursor;
                let value = metric_text(Some(chat));
                let status = if chat
                    .task
                    .as_ref()
                    .is_some_and(|task| task.data_status == TaskUsageStatus::Partial)
                {
                    ui_text(" · partial", " · 部分可用")
                } else {
                    ""
                };
                let label = format!(
                    "{} {}{status}",
                    if selected { "›" } else { " " },
                    chat.display_title()
                );
                let label: Line<'static> = if selected {
                    label.set_style(accent_style()).into()
                } else {
                    label.into()
                };
                let mut row = vec![columns(
                    truncate(label, width.saturating_sub(value.len() + 1)),
                    if value == "—" {
                        value.set_style(secondary_style()).into()
                    } else if selected {
                        number(value).into()
                    } else {
                        value.into()
                    },
                    width,
                )];
                let header_len = row.len();
                if self.zoomed && self.sections[Section::Chats].detail == Some(index) {
                    row.push("─".repeat(width).dim().into());
                    row.extend(word_wrap_lines(
                        [chat.display_title().to_owned().bold()],
                        RtOptions::new(width)
                            .initial_indent("  ".into())
                            .subsequent_indent("  ".into()),
                    ));
                    for index in self.task_metrics() {
                        let label = metric_label(index);
                        row.push(columns(
                            format!("  {label}").into(),
                            amount(available(chat), index).into(),
                            width,
                        ));
                    }
                    if let Some(task) = &chat.task {
                        if task.data_status != TaskUsageStatus::Unavailable
                            && !task.groups.is_empty()
                        {
                            let mut groups = task.groups.iter().collect::<Vec<_>>();
                            groups.sort_by(|a, b| {
                                compare(Some(&b.amounts), Some(&a.amounts), metric)
                            });
                            row.push(Line::default());
                            row.push(columns(
                                ui_text(
                                    "  Model / effort / speed",
                                    "  模型 / 思考强度 / 速度",
                                )
                                .bold()
                                .into(),
                                metric_label(metric).bold().into(),
                                width,
                            ));
                            for group in groups {
                                let model = self
                                    .model_name(group.model.as_deref().unwrap_or(ui_text(
                                        "Not reported",
                                        "未上报",
                                    )));
                                let value = amount(Some(&group.amounts), metric);
                                row.push(columns(
                                    truncate(
                                        format!("  {model}").bold().into(),
                                        width.saturating_sub(value.len() + 1),
                                    ),
                                    value.into(),
                                    width,
                                ));
                                row.push(
                                    format!(
                                        "    {} · {} · {}",
                                        group.reasoning_effort.as_deref().unwrap_or(ui_text(
                                            "Not reported",
                                            "未上报"
                                        )),
                                        group.speed.as_deref().unwrap_or(ui_text(
                                            "Not reported",
                                            "未上报"
                                        )),
                                        group
                                            .product_experience
                                            .as_deref()
                                            .unwrap_or(ui_text("Not reported", "未上报"))
                                    )
                                    .set_style(secondary_style())
                                    .into(),
                                );
                            }
                        }
                    } else {
                        row.push(
                            ui_text("  Task usage unavailable.", "  任务用量不可用。")
                                .set_style(secondary_style())
                                .into(),
                        );
                    }
                    row.push(Line::default());
                }
                let details = row.split_off(header_len);
                let mut row = wrap(row);
                // Wrap first: wrapping trims the highlighted trailing blank otherwise.
                if selected {
                    for line in &mut row {
                        super::styles::select_row(line, row_width);
                    }
                }
                row.extend(wrap(details));
                row
            })
            .collect::<Vec<_>>();
        let mut coverage = vec![
            ui_text(
                "Local chats · includes discovered descendants · excludes archived roots",
                "本地会话 · 含发现的后代会话 · 不含已归档根会话",
            )
                .set_style(secondary_style())
                .into(),
        ];
        if chats.truncated {
            coverage.push(
                ui_text(
                    "Partial ranking · 100 most recently active chats",
                    "部分排名 · 最近活跃的 100 个会话",
                )
                    .set_style(secondary_style())
                    .into(),
            );
        }
        if let Some(time) = chats.updated_at {
            coverage.push(
                (if crate::i18n::is_chinese(crate::i18n::current_language()) {
                    format!(
                        "更新于 {} UTC · 最近活动可能有延迟",
                        time.format(self.clock_format.date_time_format())
                    )
                } else {
                    format!(
                        "Updated {} UTC · recent activity may be delayed",
                        time.format(self.clock_format.date_time_format())
                    )
                })
                .set_style(secondary_style())
                .into(),
            );
        }
        if !self.zoomed {
            let count = rows.len();
            lines.extend(rows.into_iter().take(/*n*/ 5).flatten());
            lines.push(Line::default());
            lines.push(
                (if crate::i18n::is_chinese(crate::i18n::current_language()) {
                    format!(
                        "显示前 {} 个会话，共 {count} 个",
                        count.min(/*other*/ 5)
                    )
                } else {
                    format!("Showing top {} of {count} chats", count.min(/*other*/ 5))
                })
                .into(),
            );
            lines.extend(wrap(coverage));
            (lines, 0..1)
        } else {
            self.chat_window(lines, rows, wrap(coverage), width)
        }
    }
}
