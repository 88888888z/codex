//! Show one complete diagnostic with bounded scrolling and the viewer's own footer.

use super::*;
use crate::bottom_pane::footer_hint_items_line;
use crate::render::renderable::Renderable;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Margin;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Widget;

impl Renderable for WarningsView {
    fn desired_height(&self, _width: u16) -> u16 {
        12
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let area = area.inner(Margin::new(/*horizontal*/ 2, /*vertical*/ 0));
        let [header, _, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(area);
        let entry = self.entries.get(self.current);
        let chinese = crate::i18n::is_chinese(crate::i18n::current_language());
        let title = match entry {
            Some(entry) if chinese => format!(
                "警告 · 第 {} 个，共 {} 个 · {}",
                self.current + 1,
                self.entries.len(),
                localized_warning_source(&entry.source, chinese)
            ),
            Some(entry) => format!(
                "Warnings · {} of {} · {}",
                self.current + 1,
                self.entries.len(),
                localized_warning_source(&entry.source, chinese)
            ),
            None => crate::i18n::ui_text("Warnings", "警告").to_string(),
        };
        Line::from(title.bold()).render(header, buf);
        let details = entry
            .map(|entry| localized_warning_details(&entry.details, chinese))
            .unwrap_or_else(|| crate::i18n::ui_text("No warnings", "没有警告").to_string());
        let restart_hint = (self.daemon_restart_hint_at == Some(self.current)).then_some(
            if chinese {
                "\n这些警告可能来自后台服务启动时的设置。如果问题已经修复，请使用 /daemon 重启服务。"
            } else {
                "\nThese warnings may reflect settings from when the background server started. If you’ve already fixed them, use /daemon to restart it."
            },
        );
        if body.width > 0
            && body.height > 0
            && let Some(visited) = self.visited.get(self.current)
        {
            visited.set(/*val*/ true);
        }
        let lines: Vec<_> = details
            .lines()
            .chain(restart_hint.into_iter().flat_map(str::lines))
            .flat_map(|text| {
                let source =
                    crate::terminal_hyperlinks::annotate_web_urls_in_line(text.to_owned().into());
                let wrapped =
                    crate::wrapping::wrap_ranges_trim(text, usize::from(body.width.max(1)))
                        .into_iter()
                        .map(|range| crate::wrapping::WrappedLine {
                            line: (&text[range.clone()]).into(),
                            range,
                            prefix_bytes: 0,
                        })
                        .collect();
                crate::terminal_hyperlinks::remap_source_wrapped_line(&source, wrapped)
            })
            .collect();
        let page_size = usize::from(body.height.max(/*other*/ 1));
        self.page_size.set(page_size);
        self.max_offset.set(lines.len().saturating_sub(page_size));
        let offset = self.offset.get().min(self.max_offset.get());
        self.offset.set(offset);
        crate::terminal_hyperlinks::HyperlinkParagraph::new(
            &lines[offset..],
            ratatui::style::Style::default(),
        )
        .render(body, buf);
        // Plain k belongs to this viewer, including when configured as a chord prefix.
        let hint = |context, action| {
            self.keymap.primary_hint(context, action).filter(|hint| {
                let binding = match hint {
                    crate::key_hint::ShortcutHint::Single(binding)
                    | crate::key_hint::ShortcutHint::Chord {
                        prefix: binding, ..
                    } => binding,
                };
                *binding != key_hint::plain(KeyCode::Char('k'))
            })
        };
        let navigation = ["move_left", "move_right"]
            .into_iter()
            .filter_map(|action| hint(KeymapContext::List, action))
            .map(crate::key_hint::ShortcutHint::display_label)
            .collect::<Vec<_>>()
            .join("/");
        let mut items = Vec::new();
        for (hint, label) in [
            (
                Some("k".to_string()),
                crate::i18n::ui_text("keep & next", "保留下一条"),
            ),
            (
                Some(
                    hint(KeymapContext::List, "cancel")
                        .unwrap_or_else(|| key_hint::ctrl(KeyCode::Char('c')).into())
                        .display_label(),
                ),
                crate::i18n::ui_text("dismiss & close", "关闭并忽略"),
            ),
            (
                hint(KeymapContext::Global, "copy")
                    .map(crate::key_hint::ShortcutHint::display_label),
                crate::i18n::ui_text("copy", "复制"),
            ),
            (
                (!navigation.is_empty()).then_some(navigation),
                crate::i18n::ui_text("warning", "切换警告"),
            ),
            (
                hint(KeymapContext::List, "move_down")
                    .map(crate::key_hint::ShortcutHint::display_label),
                crate::i18n::ui_text("scroll", "滚动"),
            ),
        ] {
            if let Some(hint) = hint {
                items.push((hint, label.to_string()));
                if footer_hint_items_line(&items).width() > usize::from(footer.width) {
                    items.pop();
                }
            }
        }
        if let Some((line, expires_at)) = &self.flash
            && std::time::Instant::now() < *expires_at
        {
            line.clone().render(footer, buf);
        } else {
            footer_hint_items_line(self.pending_hint.as_ref().unwrap_or(&items))
                .render(footer, buf);
        }
    }
}

fn localized_warning_source(source: &str, chinese: bool) -> String {
    if !chinese {
        return source.to_string();
    }
    match source {
        "Warning" => "警告".to_string(),
        "Startup" => "启动检查".to_string(),
        "Deprecation" => "弃用提醒".to_string(),
        _ => source.strip_prefix("MCP · ").map_or_else(
            || source.to_string(),
            |server| format!("MCP 服务 · {server}"),
        ),
    }
}

fn localized_warning_details(details: &str, chinese: bool) -> String {
    if !chinese {
        return details.to_string();
    }
    details
        .split('\n')
        .map(|line| {
            if let Some(message) = line.strip_prefix("Warning: ") {
                return format!("警告：{message}");
            }
            if line == "Sign-in required." {
                return "需要重新登录。".to_string();
            }
            if let Some(server) = line.strip_prefix("MCP startup incomplete: ") {
                return format!("MCP 启动未完成：{server}");
            }
            if let Some(failed) = line
                .strip_prefix("MCP startup incomplete (failed: ")
                .and_then(|value| value.strip_suffix(')'))
            {
                return format!("MCP 启动未完成（失败：{failed}）");
            }
            if line == "MCP startup incomplete" {
                return "MCP 启动未完成".to_string();
            }
            if let Some(remainder) = line.strip_prefix("MCP client for `")
                && let Some((server, error)) = remainder.split_once("` failed to start: ")
            {
                return format!("MCP 客户端 `{server}` 启动失败：{error}");
            }
            if let Some(server) = line
                .strip_prefix("MCP client for `")
                .and_then(|value| value.strip_suffix("` failed to start"))
            {
                return format!("MCP 客户端 `{server}` 启动失败");
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
