//! Report loading and grouping metadata for the account dashboard.
//! Replacing a pending load aborts it; its response cannot populate a newer range or account.

use super::models::AccountAnalyticsGrouping as Grouping;
use crate::tui::FrameRequester;
use futures::FutureExt;
use std::panic::AssertUnwindSafe;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

pub(super) const GROUPINGS: [Grouping; 7] = [
    Grouping::Surface,
    Grouping::Feature,
    Grouping::Model,
    Grouping::TaskStart,
    Grouping::Speed,
    Grouping::Reasoning,
    Grouping::TokenType,
];

pub(super) fn group_label(index: usize) -> &'static str {
    match GROUPINGS[index] {
        Grouping::Surface => crate::i18n::ui_text("Surface", "入口"),
        Grouping::Feature => crate::i18n::ui_text("Feature", "功能"),
        Grouping::Model => crate::i18n::ui_text("Model", "模型"),
        Grouping::TaskStart => crate::i18n::ui_text("Turn start", "轮次开始方式"),
        Grouping::Speed => crate::i18n::ui_text("Speed", "速度"),
        Grouping::Reasoning => crate::i18n::ui_text("Reasoning", "思考强度"),
        Grouping::TokenType => crate::i18n::ui_text("Token type", "Token 类型"),
    }
}

pub(super) struct Pending<T> {
    receiver: oneshot::Receiver<Result<Option<T>, String>>,
    task: JoinHandle<()>,
}

impl<T> Drop for Pending<T> {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(super) enum Load<T> {
    Loading(Pending<T>),
    Ready(T),
    Unavailable,
    Error(String),
}

impl<T: Send + 'static> Load<T> {
    pub(super) fn start(
        future: impl std::future::Future<Output = Result<Option<T>, String>> + Send + 'static,
        frame: FrameRequester,
    ) -> Self {
        Self::start_with_timeout(future, frame, std::time::Duration::from_secs(/*secs*/ 30))
    }

    pub(super) fn start_with_timeout(
        future: impl std::future::Future<Output = Result<Option<T>, String>> + Send + 'static,
        frame: FrameRequester,
        timeout: std::time::Duration,
    ) -> Self {
        let (sender, receiver) = oneshot::channel();
        let task =
            tokio::spawn(async move {
                let result =
                    match tokio::time::timeout(timeout, AssertUnwindSafe(future).catch_unwind())
                        .await
                    {
                        Ok(Ok(result)) => result,
                        Ok(Err(_)) => Err(crate::i18n::ui_text(
                            "Request interrupted. Press R to retry.",
                            "请求已中断。按 R 重试。",
                        )
                        .into()),
                        Err(_) => Err(crate::i18n::ui_text(
                            "Request timed out. Press R to retry.",
                            "请求超时。按 R 重试。",
                        )
                        .into()),
                    };
                let _ = sender.send(result);
                frame.schedule_frame();
            });
        Self::Loading(Pending { receiver, task })
    }

    pub(super) fn poll(&mut self) {
        if let Self::Loading(pending) = self {
            *self = match pending.receiver.try_recv() {
                Ok(Ok(Some(data))) => Self::Ready(data),
                Ok(Ok(None)) => Self::Unavailable,
                Ok(Err(message)) => Self::Error(message),
                Err(oneshot::error::TryRecvError::Closed) => {
                    Self::Error(
                        crate::i18n::ui_text(
                            "Request interrupted. Press R to retry.",
                            "请求已中断。按 R 重试。",
                        )
                        .into(),
                    )
                }
                Err(oneshot::error::TryRecvError::Empty) => return,
            };
        }
    }

    pub(super) fn ready(&self) -> Option<&T> {
        if let Self::Ready(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub(super) fn message(&self) -> Option<&str> {
        match self {
            Self::Ready(_) => None,
            Self::Loading(_) => Some(crate::i18n::ui_text("Loading…", "正在加载…")),
            Self::Unavailable => Some(crate::i18n::ui_text(
                "No history has been reported.",
                "没有历史记录。",
            )),
            Self::Error(message) => Some(message),
        }
    }
}

/// Keep untyped server and transport diagnostics out of account reports.
pub(super) fn error(_error: codex_app_server_client::TypedRequestError) -> String {
    crate::i18n::ui_text(
        "Couldn't load analytics. Press R to retry.",
        "无法加载分析数据。按 R 重试。",
    )
    .into()
}

/// Keep tiny refunds visible while avoiding noise on ordinary credit amounts.
pub(super) fn amount(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    if value.abs() < 0.000001 {
        return format!("{value:.2e}");
    }
    let rounded = if value.abs() < 0.01 {
        format!("{value:.6}")
    } else {
        format!("{value:.2}")
    };
    let (whole, fraction) = rounded.split_once('.').unwrap_or((&rounded, ""));
    let mut grouped = String::new();
    for (index, ch) in whole.chars().enumerate() {
        if index > 0
            && ch.is_ascii_digit()
            && (whole.len() - index).is_multiple_of(/*rhs*/ 3)
            && !grouped.ends_with('-')
        {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let fraction = fraction.trim_end_matches('0');
    if !fraction.is_empty() {
        grouped.push('.');
        grouped.push_str(fraction);
    }
    grouped
}

/// Compact headline and axis labels to one decimal, preserving small signed adjustments.
pub(super) fn compact_amount(value: f64) -> String {
    if value.abs() < 1_000.0 || !value.is_finite() {
        return amount(value);
    }
    let units = ["K", "M", "B", "T"];
    let mut scaled = value / 1_000.0;
    let mut unit = 0;
    // Promote before rounding would produce a label such as 1000K.
    while scaled.abs() >= 999.95 && unit + 1 < units.len() {
        scaled /= 1_000.0;
        unit += 1;
    }
    let rounded = format!("{:.1}", (scaled * 10.0).round() / 10.0);
    format!(
        "{}{}",
        rounded.trim_end_matches('0').trim_end_matches('.'),
        units[unit]
    )
}

/// Align ordinary credit amounts to two decimals without hiding tiny adjustments.
pub(super) fn credit_amount(value: f64) -> String {
    let formatted = amount(value);
    if value != 0.0 && value.abs() < 0.01 {
        return formatted;
    }
    match formatted.split_once('.') {
        None => format!("{formatted}.00"),
        Some((_, fraction)) if fraction.len() == 1 => format!("{formatted}0"),
        Some(_) => formatted,
    }
}

pub(super) fn credits(micros: i64) -> String {
    let magnitude = micros.unsigned_abs();
    if magnitude > 0 && magnitude < 10_000 {
        let fractional = format!("{magnitude:06}");
        return format!(
            "{}0.{}",
            if micros < 0 { "-" } else { "" },
            fractional.trim_end_matches('0')
        );
    }
    let cents = (magnitude + 5_000) / 10_000;
    let mut whole = (cents / 100).to_string();
    let digits = whole.len();
    for index in (1..digits).rev() {
        if (digits - index).is_multiple_of(/*rhs*/ 3) {
            whole.insert(index, ',');
        }
    }
    format!(
        "{}{whole}.{:02}",
        if micros < 0 { "-" } else { "" },
        cents % 100
    )
}

#[cfg(test)]
#[path = "data_tests.rs"]
mod tests;
