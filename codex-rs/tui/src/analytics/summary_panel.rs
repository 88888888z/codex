//! Responsive profile summary using the Analytics container's typography and spacing.

use super::AnalyticsView;
use super::activity_chart;
use super::render::columns;
use super::render::join_columns;
use super::sections::Section;
use super::styles::secondary_style;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow as truncate;
use crate::wrapping::RtOptions;
use crate::wrapping::word_wrap_lines;
use codex_backend_client::ProfileInvocationKind;
use ratatui::style::Styled;
use ratatui::style::Stylize;
use ratatui::text::Line;

impl AnalyticsView {
    pub(super) fn summary_lines(&self, width: usize) -> Vec<Line<'static>> {
        let width = width.max(/*other*/ 1);
        let inner_width = width.min(/*other*/ 110);
        let mut lines = Vec::new();
        let Some(profile) = self.profile.ready() else {
            lines.push(
                self.profile
                    .message()
                    .unwrap_or(crate::i18n::ui_text(
                        "Profile statistics unavailable.",
                        "账户统计不可用。",
                    ))
                    .to_owned()
                    .into(),
            );
            return lines;
        };
        if let Some(identity) = &profile.profile {
            if let Some(name) = identity
                .display_name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
            {
                lines.push(center(
                    truncate(name.trim().to_owned().bold().into(), inner_width),
                    inner_width,
                ));
            }
            if let Some(username) = identity
                .username
                .as_deref()
                .filter(|name| !name.trim().is_empty())
            {
                lines.push(center(
                    truncate(
                        format!("@{}", username.trim().trim_start_matches('@'))
                            .set_style(secondary_style())
                            .into(),
                        inner_width,
                    ),
                    inner_width,
                ));
            }
        }
        let plan = self
            .account
            .ready()
            .map(|plan| crate::subscription::SubscriptionDisplay::Analytics.label(*plan));
        if let Some(plan) = plan {
            lines.push(center(
                plan.set_style(secondary_style()).into(),
                inner_width,
            ));
        }
        lines.push(Line::default());
        let tokens = &profile.stats.tokens;
        let fields = [
            (
                crate::i18n::ui_text("Lifetime tokens", "累计 Token"),
                tokens
                    .lifetime_tokens
                    .map(|value| super::data::compact_amount(value as f64)),
            ),
            (
                crate::i18n::ui_text("Peak tokens", "最高 Token"),
                tokens
                    .peak_daily_tokens
                    .map(|value| super::data::compact_amount(value as f64)),
            ),
            (
                crate::i18n::ui_text("Longest chat", "最长会话"),
                tokens.longest_running_turn_sec.map(duration),
            ),
            (
                crate::i18n::ui_text("Current streak", "当前连续"),
                tokens.current_streak_days.map(days),
            ),
            (
                crate::i18n::ui_text("Longest streak", "最长连续"),
                tokens.longest_streak_days.map(days),
            ),
        ];
        if !self.zoomed {
            lines.extend(fields.into_iter().map(|(label, value)| {
                columns(
                    label.set_style(secondary_style()).into(),
                    value.unwrap_or_else(|| "—".into()).bold().into(),
                    inner_width,
                )
            }));
            return lines;
        }
        let count = if inner_width >= 90 {
            5
        } else if inner_width >= 54 {
            3
        } else if inner_width >= 36 {
            2
        } else {
            1
        };
        for chunk in fields.chunks(count) {
            let cell_width = inner_width / chunk.len();
            let mut values = Line::default();
            let mut labels = Line::default();
            for (index, (label, value)) in chunk.iter().enumerate() {
                for (row, cell) in [
                    (
                        &mut values,
                        value.clone().unwrap_or_else(|| "—".into()).bold(),
                    ),
                    (&mut labels, label.to_string().set_style(secondary_style())),
                ] {
                    let cell = center(truncate(cell.into(), cell_width), cell_width);
                    let padding = cell_width.saturating_sub(cell.width());
                    row.spans.extend(cell.spans);
                    if index + 1 < chunk.len() {
                        row.spans.push(" ".repeat(padding).into());
                    }
                }
            }
            lines.extend([values, labels, Line::default()]);
        }
        let selected = self.sections[Section::Summary].group;
        lines.push(crate::i18n::ui_text("Token activity", "Token 活动").bold().into());
        lines.push(
            crate::i18n::ui_text("Last 12 months", "最近 12 个月")
                .set_style(secondary_style())
                .into(),
        );
        lines.push(Line::default());
        if let Some(buckets) = &tokens.daily_usage_buckets {
            lines.extend(activity_chart::chart_lines(
                super::summary::VIEWS[selected],
                buckets,
                self.end_date,
                inner_width as u16,
            ));
        } else {
            lines.push(
                crate::i18n::ui_text(
                    "Token activity history unavailable",
                    "Token 活动历史不可用",
                )
                    .set_style(secondary_style())
                    .into(),
            );
        }
        lines.push(Line::default());
        let stats = &profile.stats;
        let reasoning = stats
            .most_used_reasoning_effort
            .as_deref()
            .map(|effort| {
                let percentage = percent(stats.most_used_reasoning_effort_percentage.as_ref());
                format!("{effort} · {percentage}")
            })
            .unwrap_or_else(|| "—".into());
        let metrics = [
            (
                crate::i18n::ui_text("Fast Mode", "Fast 模式"),
                percent(stats.fast_mode_usage_percentage.as_ref()),
            ),
            (crate::i18n::ui_text("Most used reasoning", "最常用思考强度"), reasoning),
            (
                crate::i18n::ui_text("Skills explored", "使用过的 Skills"),
                count_value(stats.unique_skills_used),
            ),
            (
                crate::i18n::ui_text("Total skills used", "Skills 总使用次数"),
                count_value(stats.total_skills_used),
            ),
            (
                crate::i18n::ui_text("Total chats", "会话总数"),
                count_value(stats.total_threads),
            ),
        ];
        let two_columns = inner_width >= 76;
        let column_width = if two_columns {
            (inner_width - 4) / 2
        } else {
            inner_width
        };
        let mut insights = vec![
            crate::i18n::ui_text("Activity insights", "活动洞察").bold().into(),
            Line::default(),
        ];
        for (label, value) in metrics {
            insights.extend(word_wrap_lines(
                [columns(
                    label.set_style(secondary_style()).into(),
                    value.into(),
                    column_width,
                )],
                RtOptions::new(column_width),
            ));
        }
        let mut plugins = vec![
            crate::i18n::ui_text("Most used plugins and skills", "最常用的插件和 Skills")
                .bold()
                .into(),
            Line::default(),
        ];
        if let Some(invocations) = &stats.top_invocations {
            let mut displayed = 0;
            for invocation in invocations {
                let (prefix, name) = match invocation.kind {
                    ProfileInvocationKind::Plugin => ("@", invocation.plugin_name.as_deref()),
                    ProfileInvocationKind::Skill => ("$", invocation.skill_name.as_deref()),
                    ProfileInvocationKind::Unknown => continue,
                };
                let Some(name) = name.filter(|name| !name.trim().is_empty()) else {
                    continue;
                };
                let value = invocation
                    .usage_count
                    .map(|count| {
                        format!(
                            "{} {}",
                            count_value(Some(count)),
                            if crate::i18n::is_chinese(crate::i18n::current_language()) {
                                "次"
                            } else if count == 1 {
                                "run"
                            } else {
                                "runs"
                            }
                        )
                    })
                    .unwrap_or_else(|| "—".into());
                plugins.extend(word_wrap_lines(
                    [columns(
                        truncate(
                            format!("{prefix}{}", name.trim()).into(),
                            column_width.saturating_sub(value.len() + 1),
                        ),
                        value.set_style(secondary_style()).into(),
                        column_width,
                    )],
                    RtOptions::new(column_width),
                ));
                displayed += 1;
                if displayed == 5 {
                    break;
                }
            }
            if displayed == 0 {
                plugins.push(
                    crate::i18n::ui_text(
                        "No reported plugins or skills.",
                        "没有插件或 Skills 使用记录。",
                    )
                        .set_style(secondary_style())
                        .into(),
                );
            }
        } else {
            plugins.push(
                crate::i18n::ui_text(
                    "Plugin and skill usage unavailable.",
                    "插件和 Skills 使用情况不可用。",
                )
                    .set_style(secondary_style())
                    .into(),
            );
        }
        if two_columns {
            lines.extend(join_columns(&insights, &plugins, column_width));
        } else {
            lines.extend(insights);
            lines.push(Line::default());
            lines.extend(plugins);
        }
        if let Some(metadata) = &profile.metadata {
            lines.push(Line::default());
            if let Some(date) = metadata
                .stats_as_of
                .as_deref()
                .and_then(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
            {
                lines.push(
                    if crate::i18n::is_chinese(crate::i18n::current_language()) {
                        format!("统计截至 {}", date.format("%Y-%m-%d"))
                    } else {
                        format!("Statistics as of {}", date.format("%b %-d, %Y"))
                    }
                        .set_style(secondary_style())
                        .into(),
                );
            }
            if metadata
                .stats_error
                .as_deref()
                .is_some_and(|error| !error.trim().is_empty())
            {
                lines.push(
                    crate::i18n::ui_text(
                        "Some profile statistics are unavailable.",
                        "部分账户统计不可用。",
                    )
                        .set_style(secondary_style())
                        .into(),
                );
            }
        }
        let padding = " ".repeat(width.saturating_sub(inner_width) / 2);
        word_wrap_lines(lines, RtOptions::new(inner_width))
            .into_iter()
            .map(|mut line| {
                line.spans.insert(/*index*/ 0, padding.clone().into());
                line
            })
            .collect()
    }
}

fn center(mut line: Line<'static>, width: usize) -> Line<'static> {
    line.spans.insert(
        /*index*/ 0,
        " ".repeat(width.saturating_sub(line.width()) / 2).into(),
    );
    line
}

fn percent(value: Option<&serde_json::Number>) -> String {
    value
        .and_then(serde_json::Number::as_f64)
        .filter(|value| value.is_finite() && (0.0..=100.0).contains(value))
        .map(|value| format!("{value:.0}%"))
        .unwrap_or_else(|| "—".into())
}

fn count_value(value: Option<u64>) -> String {
    let Some(value) = value else {
        return "—".into();
    };
    let digits = value.to_string();
    digits
        .chars()
        .enumerate()
        .fold(String::new(), |mut result, (index, ch)| {
            if index > 0 && (digits.len() - index).is_multiple_of(/*rhs*/ 3) {
                result.push(',');
            }
            result.push(ch);
            result
        })
}

fn days(days: i64) -> String {
    if crate::i18n::is_chinese(crate::i18n::current_language()) {
        format!("{days} 天")
    } else {
        format!("{days} days")
    }
}

fn duration(seconds: i64) -> String {
    let seconds = seconds.max(/*other*/ 0);
    let (hours, minutes) = (seconds / 3600, seconds % 3600 / 60);
    if crate::i18n::is_chinese(crate::i18n::current_language()) {
        return match (hours, minutes) {
            (0, 0) => format!("{seconds} 秒"),
            (0, minutes) => format!("{minutes} 分钟"),
            (hours, 0) => format!("{hours} 小时"),
            (hours, minutes) => format!("{hours} 小时 {minutes} 分"),
        };
    }
    match (hours, minutes) {
        (0, 0) => format!("{seconds}s"),
        (0, minutes) => format!("{minutes}m"),
        (hours, 0) => format!("{hours}h"),
        (hours, minutes) => format!("{hours}h {minutes}m"),
    }
}
