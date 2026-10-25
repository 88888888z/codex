//! A compact shortcut reference with shared width-aware measurement and rendering.
//!
//! Groups flow into three, two, or one column without changing key routing. Runtime hints
//! remain authoritative for remapped, chorded, and disabled bindings. A clipped reference
//! keeps customization visible; the composer reserves a separate bottom row for the close hint.

use super::footer::FooterProps;
use crate::i18n::ui_text;
use crate::key_hint;
use crate::multi_agents::next_agent_shortcut;
use crate::multi_agents::previous_agent_shortcut;
use crate::shortcut_help::Group;
use crate::shortcut_help::Shortcut;
use crate::style::accent_color;
use crate::style::secondary_text_style;
use crate::wrapping::word_wrap_lines;
use crossterm::event::KeyCode;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Styled;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

pub(super) fn lines(props: &FooterProps, width: u16) -> Vec<Line<'static>> {
    let hints = props.key_hints;
    let mut compose = Group {
        title: ui_text("Compose", "编辑"),
        entries: vec![
            Shortcut::new(
                key_hint::plain(KeyCode::Char('/')),
                ui_text("Commands", "命令"),
            ),
            Shortcut::new(
                key_hint::plain(KeyCode::Char('@')),
                ui_text("Mention files", "引用文件"),
            ),
            Shortcut::new(
                key_hint::plain(KeyCode::Char('!')),
                ui_text("Shell command", "Shell 命令"),
            ),
        ],
    };
    compose.push(hints.insert_newline, ui_text("New line", "换行"));
    compose.entries.push(Shortcut::new(
        if props.is_wsl {
            key_hint::ctrl_alt(KeyCode::Char('v'))
        } else {
            key_hint::ctrl(KeyCode::Char('v'))
        },
        ui_text("Paste image", "粘贴图片"),
    ));
    compose.push(
        hints.external_editor,
        ui_text("External editor", "外部编辑器"),
    );
    compose.push(
        hints.history_search,
        ui_text("Search history", "搜索历史"),
    );
    if let Some(key) = hints.edit_previous {
        let label = key.display_label();
        compose.entries.push(Shortcut {
            key: if props.esc_backtrack_hint {
                label
            } else {
                format!("{label} {label}")
            },
            action: ui_text("Edit last message", "编辑上一条消息"),
        });
    }

    let mut session = Group {
        title: ui_text("Session", "会话"),
        entries: Vec::new(),
    };
    session.push(
        hints.queue,
        if props.is_task_running || props.queue_submissions {
            ui_text("Queue message", "排队消息")
        } else {
            ui_text("Send message", "发送消息")
        },
    );
    if props.collaboration_modes_enabled {
        session.entries.push(Shortcut::new(
            key_hint::shift(KeyCode::Tab),
            ui_text("Change mode", "切换模式"),
        ));
    }
    session.push(
        hints.reasoning_down,
        ui_text("Less reasoning", "降低思考强度"),
    );
    session.push(
        hints.reasoning_up,
        ui_text("More reasoning", "提高思考强度"),
    );
    session.push(hints.toggle_voice, ui_text("Voice", "语音"));
    session.push(
        hints.agents,
        ui_text("Agents (empty prompt)", "Agent（输入框为空时）"),
    );
    session.entries.push(Shortcut {
        key: format!(
            "{} / {}",
            previous_agent_shortcut().display_label(),
            next_agent_shortcut().display_label()
        ),
        action: ui_text("Subagents (empty prompt)", "子 Agent（输入框为空时）"),
    });
    session.push(hints.focus_activity, ui_text("Inspect activity", "查看活动"));
    session.entries.push(Shortcut::new(
        key_hint::ctrl(KeyCode::Char('c')),
        if props.is_task_running {
            ui_text("Interrupt", "中断")
        } else {
            ui_text("Quit", "退出")
        },
    ));

    let mut transcript = Group {
        title: ui_text("Transcript (open first)", "对话记录（先打开）"),
        entries: Vec::new(),
    };
    transcript.push(
        hints.show_transcript,
        ui_text("Open transcript", "打开对话记录"),
    );
    transcript.push(hints.find_transcript, ui_text("Find text", "查找文本"));
    transcript.entries.extend([
        Shortcut {
            key: "pgup / pgdn".into(),
            action: ui_text("Scroll", "滚动"),
        },
        Shortcut::new(
            key_hint::ctrl(KeyCode::Char(' ')),
            ui_text("Start selection", "开始选择"),
        ),
        Shortcut {
            key: format!(
                "{} / {}",
                key_hint::ctrl(KeyCode::Home).display_label(),
                key_hint::ctrl(KeyCode::End).display_label()
            ),
            action: ui_text("Top / latest", "顶部 / 最新"),
        },
        // Keep both jump alternatives: the terminal may be on another OS over SSH.
        Shortcut {
            key: format!(
                "{} / {}",
                key_hint::KeyBinding::new(
                    KeyCode::Char(','),
                    KeyModifiers::ALT | KeyModifiers::SHIFT
                )
                .display_label(),
                key_hint::KeyBinding::new(
                    KeyCode::Char('.'),
                    KeyModifiers::ALT | KeyModifiers::SHIFT
                )
                .display_label()
            ),
            action: ui_text("Top / latest", "顶部 / 最新"),
        },
    ]);

    let mut result = vec![
        Line::from(ui_text("Keyboard shortcuts", "键盘快捷键")).bold(),
        Line::default(),
    ];
    result.extend(crate::shortcut_help::group_lines(
        [compose, session, transcript],
        width,
    ));
    let width = usize::from(width.max(/*other*/ 1));
    result.push(Line::default());
    result.extend(footer_lines(width));
    word_wrap_lines(&result, width)
}

pub(super) fn close_hint(props: &FooterProps, width: u16) -> Line<'static> {
    let mut line = Line::default();
    if let Some(key) = props.key_hints.toggle_shortcuts {
        line.extend(key.spans());
        line.push_span(" / ".set_style(secondary_text_style()));
    }
    line.extend(key_hint::plain(KeyCode::Esc).spans());
    line.push_span(
        ui_text(" close", " 关闭").set_style(secondary_text_style()),
    );
    if line.width() > usize::from(width) {
        line = Line::from(key_hint::plain(KeyCode::Esc).spans());
        line.push_span(
            ui_text(" close", " 关闭").set_style(secondary_text_style()),
        );
    }
    line
}

fn footer_lines(width: usize) -> Vec<Line<'static>> {
    word_wrap_lines(
        [Line::from(vec![
            "/keymap".fg(accent_color()),
            ui_text(" customize", " 自定义").set_style(secondary_text_style()),
        ])],
        width,
    )
}

pub(super) fn render(props: &FooterProps, area: Rect, buf: &mut Buffer) {
    let mut lines = lines(props, area.width);
    let height = usize::from(area.height);
    if lines.len() > height && height > 0 {
        let mut footer = footer_lines(usize::from(area.width.max(/*other*/ 1)));
        footer.truncate(height);
        let body_height = height.saturating_sub(footer.len());
        lines.truncate(body_height.saturating_sub(/*rhs*/ 1));
        if body_height > 0 {
            lines.push(Line::from(ui_text("… resize to see all", "… 调整窗口大小以查看全部")).dim());
        }
        lines.extend(footer);
    }
    Paragraph::new(lines).render(area, buf);
}

#[cfg(test)]
#[path = "shortcut_overlay_tests.rs"]
mod tests;
