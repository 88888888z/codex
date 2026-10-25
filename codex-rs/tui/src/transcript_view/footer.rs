//! Project transcript interactions and page-loading feedback into the existing footer.
//! Loading never inserts transcript rows; the owner schedules motion only while a page is pending.
//! Surface owners supply the latest shortcut because legacy pagers reserve Escape for editing.
//! Known navigation labels style keys separately from prose; transcript and query text stay untouched.

use super::*;
use crate::bottom_pane::TranscriptFooter;
use crate::footer_hint::first_fitting_line;
use crate::key_hint::key_label_spans;
use crate::motion::MotionMode;
use crate::motion::shimmer_text;
use crate::style::accent_color;
use crossterm::event::KeyCode;
use unicode_width::UnicodeWidthStr;

impl TranscriptView {
    pub(crate) fn footer_with_navigation(
        &self,
        width: u16,
        motion: MotionMode,
        latest_navigation: &str,
    ) -> Option<TranscriptFooter> {
        if let Some(mode) = &self.copy_mode {
            return Some(TranscriptFooter {
                text: vec![
                    Line::from(if crate::i18n::is_chinese(crate::i18n::current_language()) {
                        format!("复制{}", mode.label)
                    } else {
                        format!("Copy {}", mode.label.to_lowercase())
                    })
                    .bold(),
                    navigation_line(if crate::i18n::is_chinese(crate::i18n::current_language()) {
                        "↑/↓/j/k 选择 · g/G 两端 · enter 复制 · esc 关闭"
                    } else {
                        "↑/↓/j/k select · g/G ends · enter copy · esc close"
                    }),
                ]
                .into(),
                cursor_column: None,
                is_interactive: true,
            });
        }
        // A mouse-down anchor owns the gesture, but has no text to copy yet. Compare
        // source positions so movement across wrapping or padding is not a selection.
        let has_selected_text = self.selection.as_ref().is_some_and(|selection| {
            (selection.start.key, selection.start.offset)
                != (selection.end.key, selection.end.offset)
        });
        if let Some((query, cursor)) = self.search_footer(width) {
            return Some(TranscriptFooter {
                text: vec![
                    query,
                    if has_selected_text {
                        selection_hint(width)
                    } else {
                        self.search.status_line(width, self.history)
                    },
                ]
                .into(),
                cursor_column: self.selection.is_none().then_some(cursor),
                is_interactive: true,
            });
        }
        let pending = self.is_loading_history() || self.history == TranscriptHistoryState::Failed;
        if has_selected_text {
            return Some(TranscriptFooter {
                text: if self.tail_visible && !pending {
                    selection_hint(width)
                } else {
                    first_fitting_line(
                        [
                            self.status_line_with_navigation(
                                &if crate::i18n::is_chinese(crate::i18n::current_language()) {
                                    format!(
                                        "{} 复制 · enter 复制并跟随 · esc 清除",
                                        crate::key_hint::ctrl(KeyCode::Char('c')).display_label()
                                    )
                                } else {
                                    format!(
                                        "{} copy · enter copy & follow · esc clear",
                                        crate::key_hint::ctrl(KeyCode::Char('c')).display_label()
                                    )
                                },
                                motion,
                            ),
                            selection_hint(width),
                        ],
                        width,
                    )
                }
                .into(),
                cursor_column: None,
                is_interactive: true,
            });
        }
        if self.search.is_reading() && !pending && !self.is_activity_focused() {
            return Some(TranscriptFooter {
                text: self.search.status_line(width, self.history).into(),
                cursor_column: None,
                is_interactive: false,
            });
        }
        if self.is_activity_focused() && !pending {
            return self.disclosure_footer(width);
        }
        // Selection can pause following without hiding the current final row.
        let can_return =
            self.selection.is_none() && self.can_return_to_latest() && !self.tail_visible;
        (pending || self.unseen_activity || can_return).then(|| {
            let navigation = if self.can_return_to_latest() {
                latest_navigation
            } else {
                ""
            };
            let mut status = self.status_line_with_navigation(navigation, motion);
            // Loading must not resize the composer or move its caret. Shorten secondary text
            // before clipping either shortcut; failed loads prioritize their retry action.
            if status.width() > usize::from(width) {
                status = if self.is_loading_history() {
                    let loading = if navigation.is_empty() {
                        let loading = crate::i18n::ui_text("Loading…", "加载中…");
                        if Line::from(format!("↑ {loading}")).width() <= usize::from(width) {
                            loading
                        } else {
                            "…"
                        }
                    } else if Line::from(format!(
                        "↑ {} · ",
                        crate::i18n::ui_text("Loading…", "加载中…")
                    ))
                    .width()
                        + navigation.width()
                        <= usize::from(width)
                    {
                        crate::i18n::ui_text("Loading… · ", "加载中… · ")
                    } else {
                        "… "
                    };
                    let mut spans = vec!["↑ ".fg(accent_color())];
                    spans.extend(shimmer_text(loading, motion));
                    spans.extend(navigation_line(navigation).spans);
                    Line::from(spans)
                } else if self.history == TranscriptHistoryState::Failed {
                    let mut spans =
                        vec![crate::i18n::ui_text("Retry: ", "重试：").dim()];
                    spans.extend(key_label_spans(&JumpTarget::Beginning.hint_label()));
                    Line::from(spans)
                } else if navigation.is_empty() {
                    Line::from(crate::i18n::ui_text("New activity", "新活动").dim())
                } else if self.unseen_activity {
                    let mut spans =
                        vec![crate::i18n::ui_text("New · ", "新活动 · ").dim()];
                    spans.extend(navigation_line(navigation).spans);
                    Line::from(spans)
                } else {
                    navigation_line(navigation)
                };
            }
            let retry = if self.history == TranscriptHistoryState::Failed {
                if crate::i18n::is_chinese(crate::i18n::current_language()) {
                    format!(
                        "{} 重试",
                        crate::key_hint::ctrl(KeyCode::Home).display_label()
                    )
                } else {
                    format!(
                        "{} retry",
                        crate::key_hint::ctrl(KeyCode::Home).display_label()
                    )
                }
            } else {
                navigation.to_owned()
            };
            TranscriptFooter {
                text: first_fitting_line(
                    [status, navigation_line(&retry), navigation_line(navigation)],
                    width,
                )
                .into(),
                cursor_column: None,
                is_interactive: false,
            }
        })
    }

    pub(crate) fn status_line_with_navigation(
        &self,
        navigation: &str,
        motion: MotionMode,
    ) -> Line<'static> {
        if self.is_loading_history() {
            let mut spans = vec!["↑ ".fg(accent_color())];
            spans.extend(shimmer_text(
                crate::i18n::ui_text("Loading earlier messages…", "正在加载更早的消息…"),
                motion,
            ));
            if self.unseen_activity {
                spans.push(
                    crate::i18n::ui_text(" · New activity", " · 新活动").dim(),
                );
            }
            if !navigation.is_empty() {
                spans.push(" · ".dim());
                spans.extend(navigation_line(navigation).spans);
            }
            return Line::from(spans);
        }
        if self.selection.is_some() && self.history == TranscriptHistoryState::Failed {
            let mut spans = navigation_line(navigation).spans;
            spans.push(crate::i18n::ui_text(" · Retry: ", " · 重试：").dim());
            spans.extend(key_label_spans(&JumpTarget::Beginning.hint_label()));
            return Line::from(spans);
        }
        let history = match self.history {
            TranscriptHistoryState::Failed => {
                let mut spans =
                    vec![crate::i18n::ui_text("Retry history: ", "重试历史记录：").dim()];
                spans.extend(key_label_spans(&JumpTarget::Beginning.hint_label()));
                spans.push(".  ".dim());
                spans
            }
            TranscriptHistoryState::Partial => vec![crate::i18n::ui_text(
                "Earlier messages available.  ",
                "有更早的消息可用。  ",
            )
            .dim()],
            TranscriptHistoryState::LoadingOlder
            | TranscriptHistoryState::LoadingBeginning
            | TranscriptHistoryState::Complete
            | TranscriptHistoryState::Idle => Vec::new(),
        };
        let activity = if self.unseen_activity {
            if history.is_empty() && navigation.is_empty() {
                crate::i18n::ui_text("New activity", "新活动")
            } else {
                crate::i18n::ui_text("New activity · ", "新活动 · ")
            }
        } else {
            ""
        };
        let mut spans = vec![activity.dim()];
        spans.extend(history);
        spans.extend(navigation_line(navigation).spans);
        Line::from(spans)
    }

    pub(crate) fn is_loading_history(&self) -> bool {
        matches!(
            self.history,
            TranscriptHistoryState::LoadingOlder | TranscriptHistoryState::LoadingBeginning
        )
    }
}

fn selection_hint(width: u16) -> Line<'static> {
    let chinese = crate::i18n::is_chinese(crate::i18n::current_language());
    let candidates = if chinese {
        [
            format!(
                "{} 复制 · enter 复制并跟随 · esc 清除",
                crate::key_hint::ctrl(KeyCode::Char('c')).display_label()
            ),
            "enter 复制并跟随 · esc 清除".to_string(),
            "enter 复制+↓ · esc".to_string(),
            "esc 清除".to_string(),
        ]
    } else {
        [
            format!(
                "{} copy · enter copy & follow · esc clear",
                crate::key_hint::ctrl(KeyCode::Char('c')).display_label()
            ),
            "enter copy & follow · esc clear".to_string(),
            "enter copy+↓ · esc".to_string(),
            "esc clear".to_string(),
        ]
    };
    first_fitting_line(
        candidates
            .into_iter()
            .map(|hint| navigation_line(&hint)),
        width,
    )
}

/// Style only the explicit key-label prefixes of trusted navigation hints.
///
/// Callers supply UI-authored hints, including runtime key chords followed by a known action.
/// Unknown segments remain dim prose; this must never be applied to transcript or query content.
pub(super) fn navigation_line(navigation: &str) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, hint) in navigation.split(" · ").enumerate() {
        if index > 0 {
            spans.push(" · ".dim());
        }
        let actions = [
            (" copy & follow", " 复制并跟随"),
            (" clear selection", " 清除选择"),
            (" copy+↓", " 复制+↓"),
            (" previous", " 上一个"),
            (" accept", " 接受"),
            (" cancel", " 取消"),
            (" older", " 更早"),
            (" newer", " 更新"),
            (" latest", " 最新"),
            (" retry", " 重试"),
            (" select", " 选择"),
            (" copy", " 复制"),
            (" clear", " 清除"),
            (" next", " 下一个"),
            (" close", " 关闭"),
        ];
        let action = actions
            .into_iter()
            .find_map(|(english, chinese)| {
                if hint.ends_with(english) {
                    Some((english, crate::i18n::ui_text(english, chinese)))
                } else if hint.ends_with(chinese) {
                    Some((chinese, chinese))
                } else {
                    None
                }
            });
        if let Some((suffix, label)) = action {
            spans.extend(key_label_spans(&hint[..hint.len() - suffix.len()]));
            spans.push(label.dim());
        } else if hint == "esc" {
            spans.extend(key_label_spans(hint));
        } else {
            spans.push(hint.to_owned().dim());
        }
    }
    Line::from(spans)
}

#[cfg(test)]
#[path = "footer_tests.rs"]
mod tests;
