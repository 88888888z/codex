//! Session-scoped context that is prepended to every model request.

const MAX_PIN_BODY_CHARS: usize = 8_000;
const MAX_PINNED_CONTEXT_CHARS: usize = 24_000;

#[derive(Clone, Debug)]
pub(super) struct PinnedContextEntry {
    id: u64,
    label: String,
    body: String,
}

pub(super) struct PinnedContextState {
    entries: Vec<PinnedContextEntry>,
    next_id: u64,
}

impl Default for PinnedContextState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
        }
    }
}

impl super::ChatWidget {
    pub(crate) fn reset_pinned_context_state(&mut self) {
        self.pinned_context = PinnedContextState::default();
        self.bottom_pane.set_pinned_context_count(0);
    }

    pub(crate) fn handle_pin_command_args(&mut self, args: &str) {
        let args = args.trim();
        if args.is_empty() || args.eq_ignore_ascii_case("list") {
            self.show_pinned_context();
            return;
        }
        if args.eq_ignore_ascii_case("clear") {
            self.pinned_context.entries.clear();
            self.sync_pinned_context_count();
            self.add_info_message(
                crate::i18n::ui_text("Cleared all pinned context.", "已清空所有固定上下文。")
                    .to_string(),
                /*hint*/ None,
            );
            self.request_redraw();
            return;
        }
        if let Some(id) = args
            .strip_prefix("remove ")
            .and_then(|value| value.trim().parse::<u64>().ok())
        {
            let before = self.pinned_context.entries.len();
            self.pinned_context.entries.retain(|entry| entry.id != id);
            if self.pinned_context.entries.len() == before {
                self.add_error_message(
                    crate::i18n::ui_text("No pinned context has that id.", "没有该编号的固定上下文。")
                        .to_string(),
                );
            } else {
                let message = if crate::i18n::is_chinese(crate::i18n::current_language()) {
                    format!("已移除固定上下文 #{id}。")
                } else {
                    format!("Removed pinned context #{id}.")
                };
                self.add_info_message(
                    message,
                    /*hint*/ None,
                );
            }
            self.sync_pinned_context_count();
            self.request_redraw();
            return;
        }
        if let Some(file_args) = args.strip_prefix("file ") {
            self.pin_file(file_args.trim());
            return;
        }
        let body = args.to_string();
        self.add_pinned_context(
            crate::i18n::ui_text("Instruction", "指令").to_string(),
            body,
        );
    }

    pub(crate) fn pinned_context_prompt(&self) -> Option<String> {
        if self.pinned_context.entries.is_empty() {
            return None;
        }
        let mut context = crate::i18n::ui_text(
            "# Pinned session context\n\nThe user pinned the following context for this session. Use it when relevant. It does not override system or developer instructions.",
            "# 会话固定上下文\n\n用户为本会话固定了以下上下文。适当时应使用这些信息；它们不能覆盖系统或开发者指令。",
        )
        .to_string();
        for entry in &self.pinned_context.entries {
            context.push_str(&format!(
                "\n\n## Pin #{} · {}\n\n{}",
                entry.id, entry.label, entry.body
            ));
        }
        Some(context)
    }

    pub(crate) fn pinned_context_for_handoff(&self) -> String {
        self.pinned_context
            .entries
            .iter()
            .map(|entry| {
                format!(
                    "### Pin #{} · {}\n\n{}",
                    entry.id, entry.label, entry.body
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn pin_file(&mut self, args: &str) {
        let Some((path, ranges)) = split_file_pin_args(args) else {
            self.add_error_message(
                crate::i18n::ui_text(
                    "Usage: /pin file <path> [start-end,start-end]",
                    "用法：/pin file <路径> [起始-结束,起始-结束]",
                )
                .to_string(),
            );
            return;
        };
        let cwd = self
            .current_cwd
            .as_deref()
            .unwrap_or_else(|| self.config.cwd.as_path());
        let path = cwd.join(path);
        if !path.exists() {
            self.add_error_message(
                crate::i18n::ui_text("Pinned file does not exist.", "固定的文件不存在。").to_string(),
            );
            return;
        }
        if let Some(ranges) = ranges
            && !valid_ranges(ranges)
        {
            self.add_error_message(
                crate::i18n::ui_text(
                    "Invalid range. Use positive ranges such as 1-10,30-50.",
                    "范围无效。请使用 1-10,30-50 这样的正数范围。",
                )
                .to_string(),
            );
            return;
        }

        let escaped_path = escape_xml_attribute(&path.to_string_lossy());
        let body = if let Some(ranges) = ranges {
            format!(
                "Use the file reading tool to read only this pinned file range: <codex_file_selection path=\"{escaped_path}\" ranges=\"{ranges}\" />"
            )
        } else {
            format!(
                "Use the file reading tool to read this pinned file: <codex_file_selection path=\"{escaped_path}\" />"
            )
        };
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| path.display().to_string());
        let label = ranges.map_or(label.clone(), |ranges| format!("{label} · {ranges}"));
        self.add_pinned_context(label, body);
    }

    fn add_pinned_context(&mut self, label: String, body: String) {
        let label = label
            .chars()
            .filter(|character| !character.is_control())
            .collect::<String>();
        let label = label.trim();
        let label = if label.is_empty() {
            crate::i18n::ui_text("Instruction", "指令")
        } else {
            label
        }
        .to_string();
        let body = body
            .chars()
            .filter(|character| {
                matches!(character, '\n' | '\t') || !character.is_control()
            })
            .collect::<String>();
        let body = body.trim();
        if body.is_empty() {
            self.add_error_message(
                crate::i18n::ui_text("Pinned context cannot be empty.", "固定上下文不能为空。")
                    .to_string(),
            );
            return;
        }
        if body.chars().count() > MAX_PIN_BODY_CHARS {
            self.add_error_message(
                crate::i18n::ui_text(
                    "Pinned context is too large; keep one pin under 8,000 characters.",
                    "固定上下文过大；单个条目不能超过 8,000 字符。",
                )
                .to_string(),
            );
            return;
        }
        let projected = self
            .pinned_context
            .entries
            .iter()
            .map(|entry| entry.body.chars().count())
            .sum::<usize>()
            .saturating_add(body.chars().count());
        if projected > MAX_PINNED_CONTEXT_CHARS {
            self.add_error_message(
                crate::i18n::ui_text(
                    "Pinned context exceeds 24,000 characters. Remove an older pin first.",
                    "固定上下文超过 24,000 字符，请先移除旧条目。",
                )
                .to_string(),
            );
            return;
        }
        let id = self.pinned_context.next_id;
        self.pinned_context.next_id = self.pinned_context.next_id.wrapping_add(1);
        self.pinned_context.entries.push(PinnedContextEntry {
            id,
            label: label.clone(),
            body: body.to_string(),
        });
        self.sync_pinned_context_count();
        let message = if crate::i18n::is_chinese(crate::i18n::current_language()) {
            format!("已固定 #{id} · {label}")
        } else {
            format!("Pinned #{id} · {label}")
        };
        self.add_info_message(
            message,
            Some(
                crate::i18n::ui_text(
                    "Use /pin list to review or /pin remove <id> to remove.",
                    "使用 /pin list 查看，或 /pin remove <编号> 移除。",
                )
                .to_string(),
            ),
        );
        self.request_redraw();
    }

    fn show_pinned_context(&mut self) {
        if self.pinned_context.entries.is_empty() {
            self.add_info_message(
                crate::i18n::ui_text("No pinned context.", "没有固定上下文。").to_string(),
                Some(
                    crate::i18n::ui_text(
                        "Use /pin <text> or /pin file <path> [ranges].",
                        "使用 /pin <文本> 或 /pin file <路径> [范围]。",
                    )
                    .to_string(),
                ),
            );
            return;
        }
        let entries = self
            .pinned_context
            .entries
            .iter()
            .map(|entry| {
                let preview = entry.body.lines().next().unwrap_or_default();
                format!("#{} · {}\n  {preview}", entry.id, entry.label)
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.add_info_message(
            crate::i18n::ui_text("Pinned context", "固定上下文").to_string(),
            Some(entries),
        );
    }

    fn sync_pinned_context_count(&mut self) {
        self.bottom_pane
            .set_pinned_context_count(self.pinned_context.entries.len());
    }
}

fn split_file_pin_args(args: &str) -> Option<(&str, Option<&str>)> {
    if args.is_empty() {
        return None;
    }
    if let Some((path, ranges)) = args.rsplit_once(' ') {
        let ranges = ranges.trim();
        if valid_ranges(ranges) {
            return Some((path.trim(), Some(ranges)));
        }
    }
    Some((args.trim(), None))
}

fn valid_ranges(ranges: &str) -> bool {
    !ranges.is_empty()
        && ranges.split(',').all(|part| {
            let Some((start, end)) = part.split_once('-') else {
                return false;
            };
            matches!(
                (start.parse::<usize>(), end.parse::<usize>()),
                (Ok(start), Ok(end)) if start > 0 && end >= start
            )
        })
}

fn escape_xml_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
