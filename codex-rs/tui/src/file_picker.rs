//! Local file browser for attaching selected line ranges as bounded prompt context.

use std::cell::Cell;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::MouseEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::i18n::ui_text;
use crate::key_hint::KeyBindingListExt;
use crate::keymap::PagerKeymap;
use crate::render::highlight::highlight_code_to_styled_spans;
use crate::style::accent_color;
use crate::style::focused_border_style;
use crate::style::hairline_style;
use crate::style::selection_style;
use crate::tui;
use crate::tui::TuiEvent;

const MAX_PREVIEW_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TREE_ROWS: usize = 2_000;
const MAX_ATTACHMENT_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FilePickerFocus {
    Tree,
    View,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TreeRowKind {
    Parent,
    Directory,
    File,
}

#[derive(Clone, Debug)]
struct TreeRow {
    path: PathBuf,
    name: String,
    depth: usize,
    kind: TreeRowKind,
}

#[derive(Debug)]
struct FilePreview {
    path: PathBuf,
    lines: Vec<String>,
    highlighted: Option<Vec<Vec<Span<'static>>>>,
}

pub(crate) struct FilePickerOverlay {
    root: PathBuf,
    rows: Vec<TreeRow>,
    selected: usize,
    expanded: HashSet<PathBuf>,
    show_hidden: bool,
    tree_truncated: bool,
    focus: FilePickerFocus,
    preview: Option<FilePreview>,
    view_cursor: usize,
    view_scroll: usize,
    view_height: Cell<u16>,
    selection_anchor: Option<usize>,
    selected_ranges: Vec<(usize, usize)>,
    status: Option<String>,
    done: bool,
    app_event_tx: AppEventSender,
    keymap: PagerKeymap,
}

impl FilePickerOverlay {
    pub(crate) fn new(
        cwd: &Path,
        initial_path: Option<&str>,
        app_event_tx: AppEventSender,
        keymap: PagerKeymap,
    ) -> Result<Self, String> {
        let requested = resolve_requested_path(cwd, initial_path)?;
        let requested = dunce::canonicalize(&requested)
            .map_err(|error| format!("无法打开 {}：{error}", requested.display()))?;
        let metadata = std::fs::metadata(&requested)
            .map_err(|error| format!("无法打开 {}：{error}", requested.display()))?;
        let (root, initial_file) = if metadata.is_dir() {
            (requested, None)
        } else if metadata.is_file() {
            let parent = requested
                .parent()
                .ok_or_else(|| format!("无法确定 {} 的父目录", requested.display()))?
                .to_path_buf();
            (parent, Some(requested))
        } else {
            return Err(format!("不支持的文件类型：{}", requested.display()));
        };
        let root = dunce::canonicalize(&root)
            .map_err(|error| format!("无法打开 {}：{error}", root.display()))?;
        let mut overlay = Self {
            root,
            rows: Vec::new(),
            selected: 0,
            expanded: HashSet::new(),
            show_hidden: false,
            tree_truncated: false,
            focus: FilePickerFocus::Tree,
            preview: None,
            view_cursor: 0,
            view_scroll: 0,
            view_height: Cell::new(0),
            selection_anchor: None,
            selected_ranges: Vec::new(),
            status: None,
            done: false,
            app_event_tx,
            keymap,
        };
        overlay.rebuild_tree(None)?;
        if let Some(path) = initial_file {
            let selected = overlay
                .rows
                .iter()
                .position(|row| row.path == path)
                .unwrap_or(0);
            overlay.selected = selected;
            overlay.open_file(path);
        }
        Ok(overlay)
    }

    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let [header_area, body_area, footer_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .areas(area);
        let range = self.selection_summary();
        let focus = match self.focus {
            FilePickerFocus::Tree => ui_text("Tree", "目录树"),
            FilePickerFocus::View => ui_text("View", "代码视图"),
        };
        Line::from(vec![
            ui_text("Files", "文件选择").bold(),
            format!("  {}", self.root.display()).dim(),
            format!("  {focus}").fg(accent_color()).bold(),
            range.map_or_else(|| "".into(), |range| format!("  {range}").dim()),
        ])
        .render(header_area, buf);

        let tree_width = body_area
            .width
            .saturating_mul(/*percent*/ 32)
            .max(/*min*/ 24)
            .min(/*max*/ 46);
        let [tree_area, view_area] = Layout::horizontal([
            Constraint::Length(tree_width.min(body_area.width)),
            Constraint::Min(1),
        ])
        .areas(body_area);
        self.render_tree(tree_area, buf);
        self.render_view(view_area, buf);
        self.render_footer(footer_area, buf);
    }

    fn render_tree(&self, area: Rect, buf: &mut Buffer) {
        let block = if self.focus == FilePickerFocus::Tree {
            focused_panel(ui_text("Directory", "目录"))
        } else {
            panel(ui_text("Directory", "目录"))
        };
        let inner = block.inner(area);
        block.render(area, buf);
        let height = usize::from(inner.height);
        if height == 0 || self.rows.is_empty() {
            return;
        }
        let start = self
            .selected
            .saturating_sub(height / 2)
            .min(self.rows.len().saturating_sub(height));
        for (offset, (index, row)) in self
            .rows
            .iter()
            .enumerate()
            .skip(start)
            .take(height)
            .enumerate()
        {
            let selected = index == self.selected;
            let marker = if selected { "›" } else { " " };
            let icon = match row.kind {
                TreeRowKind::Parent => "↰",
                TreeRowKind::Directory if self.expanded.contains(&row.path) => "▾",
                TreeRowKind::Directory => "▸",
                TreeRowKind::File => "·",
            };
            let label_style = if selected {
                Style::default().fg(accent_color()).bold()
            } else if row.kind == TreeRowKind::Directory {
                Style::default().bold()
            } else {
                Style::default()
            };
            Line::from(vec![
                Span::styled(marker, label_style),
                Span::raw(" "),
                Span::raw("  ".repeat(row.depth)),
                Span::styled(format!("{icon} {}", row.name), label_style),
            ])
            .render(
                Rect::new(inner.x, inner.y + offset as u16, inner.width, 1),
                buf,
            );
        }
        if let Some(background) = selection_style().bg {
            let selected_row = self.selected.saturating_sub(start);
            if selected_row < height {
                buf.set_style(
                    Rect::new(inner.x, inner.y + selected_row as u16, inner.width, 1),
                    Style::default().bg(background),
                );
            }
        }
    }

    fn render_view(&self, area: Rect, buf: &mut Buffer) {
        let title = self.preview.as_ref().map_or_else(
            || ui_text("Preview", "预览").to_string(),
            |preview| {
                let path = display_path(&preview.path, &self.root);
                format!(
                    "{path}  ·  {} {}",
                    preview.lines.len(),
                    ui_text("lines", "行")
                )
            },
        );
        let block = if self.focus == FilePickerFocus::View {
            focused_panel(title)
        } else {
            panel(title)
        };
        let inner = block.inner(area);
        block.render(area, buf);
        let Some(preview) = &self.preview else {
            let message = self.status.as_deref().unwrap_or_else(|| {
                ui_text(
                    "Select a file and press Enter to preview.",
                    "选择文件后按 Enter 预览。",
                )
            });
            Paragraph::new(message.dim()).render(inner, buf);
            return;
        };
        if inner.width < 8 || inner.height == 0 {
            return;
        }
        let height = usize::from(inner.height);
        self.view_height.set(inner.height);
        let max_scroll = preview.lines.len().saturating_sub(height);
        let mut scroll = self.view_scroll.min(max_scroll);
        if self.view_cursor < scroll {
            scroll = self.view_cursor;
        } else if height > 0 && self.view_cursor >= scroll.saturating_add(height) {
            scroll = self.view_cursor.saturating_add(1).saturating_sub(height);
        }
        let number_width = preview.lines.len().to_string().len().max(/*min*/ 3) as u16 + 1;
        for offset in 0..inner.height {
            let line_index = scroll + usize::from(offset);
            let Some(source) = preview.lines.get(line_index) else {
                break;
            };
            let row = Rect::new(inner.x, inner.y + offset, inner.width, 1);
            let selected_line = self.line_is_selected(line_index);
            let background = selected_line.then(|| selection_style().bg).flatten();
            if let Some(background) = background {
                buf.set_style(row, Style::default().bg(background));
            }
            let number_style =
                if self.focus == FilePickerFocus::View && line_index == self.view_cursor {
                    Style::default().fg(accent_color()).bold()
                } else {
                    Style::default().dim()
                };
            Span::styled(
                format!(
                    "{:>width$} ",
                    line_index + 1,
                    width = usize::from(number_width - 1)
                ),
                number_style.bg(background.unwrap_or(ratatui::style::Color::Reset)),
            )
            .render(Rect::new(inner.x, row.y, number_width, 1), buf);
            let spans = preview
                .highlighted
                .as_ref()
                .and_then(|lines| lines.get(line_index))
                .cloned()
                .unwrap_or_else(|| vec![Span::raw(source.clone())]);
            let mut spans = spans;
            if let Some(background) = background {
                for span in &mut spans {
                    span.style.bg = Some(background);
                }
            }
            Paragraph::new(Line::from(spans)).render(
                Rect::new(
                    inner.x + number_width,
                    row.y,
                    inner.width.saturating_sub(number_width),
                    1,
                ),
                buf,
            );
        }
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        let status = if let Some(status) = &self.status {
            status.clone()
        } else if let Some(ranges) = self.selection_summary() {
            format!(
                "{ranges} · Enter {} · x {} · c {}",
                ui_text("attach all ranges", "附加全部范围"),
                ui_text("remove last range", "撤销最后一段"),
                ui_text("clear ranges", "清空范围"),
            )
        } else if let Some(preview) = &self.preview {
            format!(
                "{}:{} · v {} · a {} · Enter {}",
                display_path(&preview.path, &self.root),
                self.view_cursor + 1,
                ui_text("select lines", "选择行"),
                ui_text("attach whole file", "附加整个文件"),
                ui_text("attach current line", "附加当前行"),
            )
        } else {
            ui_text("Choose a file, then press Enter.", "选择文件后按 Enter。").to_string()
        };
        Line::from(status).render(Rect::new(area.x, area.y, area.width, 1), buf);
        let hints = format!(
            "Tab {} · ↑/↓ {} · v {} · Enter {} · h {} · r {} · Esc {}",
            ui_text("switch pane", "切换区域"),
            ui_text("move", "移动"),
            ui_text("start / finish range", "开始 / 结束一段"),
            ui_text("open / attach", "打开 / 附加"),
            ui_text("hidden files", "隐藏文件"),
            ui_text("refresh", "刷新"),
            ui_text("close", "关闭"),
        );
        Line::from(hints)
            .dim()
            .render(Rect::new(area.x, area.y + 1, area.width, 1), buf);
    }

    pub(crate) fn handle_event(
        &mut self,
        tui: &mut tui::Tui,
        event: TuiEvent,
    ) -> std::io::Result<()> {
        match event {
            TuiEvent::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key),
            TuiEvent::Mouse(mouse) => self.handle_mouse(mouse.kind),
            TuiEvent::Draw | TuiEvent::Resume | TuiEvent::Resize(_) | TuiEvent::FocusGained => {
                tui.draw(u16::MAX, |frame| self.render(frame.area(), frame.buffer))?;
            }
            _ => {}
        }
        tui.frame_requester().schedule_frame();
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc && self.selection_anchor.is_some() {
            self.selection_anchor = None;
            self.status = None;
            return;
        }
        if self.keymap.close.is_pressed(key) || key.code == KeyCode::Esc {
            self.done = true;
            return;
        }
        if key.code == KeyCode::Tab && key.modifiers.is_empty() {
            self.focus = match self.focus {
                FilePickerFocus::Tree if self.preview.is_some() => FilePickerFocus::View,
                FilePickerFocus::Tree => {
                    self.status = Some(
                        ui_text(
                            "Select a file before opening the view.",
                            "请先选择一个文件再进入代码视图。",
                        )
                        .to_string(),
                    );
                    FilePickerFocus::Tree
                }
                FilePickerFocus::View => FilePickerFocus::Tree,
            };
            return;
        }
        if key.modifiers.is_empty() && key.code == KeyCode::Char('h') {
            self.show_hidden = !self.show_hidden;
            let selected_path = self.selected_row().map(|row| row.path.clone());
            if let Err(error) = self.rebuild_tree(selected_path) {
                self.status = Some(error);
            }
            return;
        }
        if key.modifiers.is_empty() && key.code == KeyCode::Char('r') {
            let selected_path = self.selected_row().map(|row| row.path.clone());
            if let Err(error) = self.rebuild_tree(selected_path) {
                self.status = Some(error);
            }
            return;
        }
        match self.focus {
            FilePickerFocus::Tree => self.handle_tree_key(key),
            FilePickerFocus::View => self.handle_view_key(key),
        }
    }

    fn handle_tree_key(&mut self, key: KeyEvent) {
        if self.keymap.scroll_up.is_pressed(key)
            || key.code == KeyCode::Up
            || key.code == KeyCode::Char('k')
        {
            self.selected = self.selected.saturating_sub(1);
        } else if self.keymap.scroll_down.is_pressed(key)
            || key.code == KeyCode::Down
            || key.code == KeyCode::Char('j')
        {
            self.selected = self
                .selected
                .saturating_add(1)
                .min(self.rows.len().saturating_sub(1));
        } else if key.code == KeyCode::Home || key.code == KeyCode::Char('g') {
            self.selected = 0;
        } else if key.code == KeyCode::End || key.code == KeyCode::Char('G') {
            self.selected = self.rows.len().saturating_sub(1);
        } else if key.code == KeyCode::Enter {
            self.activate_tree_row();
        }
    }

    fn handle_view_key(&mut self, key: KeyEvent) {
        let Some(preview) = &self.preview else {
            self.focus = FilePickerFocus::Tree;
            return;
        };
        let line_count = preview.lines.len();
        let page = usize::from(self.view_height.get().max(/*min*/ 10));
        if self.keymap.scroll_up.is_pressed(key)
            || key.code == KeyCode::Up
            || key.code == KeyCode::Char('k')
        {
            self.view_cursor = self.view_cursor.saturating_sub(1);
        } else if self.keymap.scroll_down.is_pressed(key)
            || key.code == KeyCode::Down
            || key.code == KeyCode::Char('j')
        {
            self.view_cursor = self
                .view_cursor
                .saturating_add(1)
                .min(line_count.saturating_sub(1));
        } else if self.keymap.page_up.is_pressed(key) {
            self.view_cursor = self.view_cursor.saturating_sub(page);
            self.view_scroll = self.view_scroll.saturating_sub(page);
        } else if self.keymap.page_down.is_pressed(key) {
            self.view_cursor = self
                .view_cursor
                .saturating_add(page)
                .min(line_count.saturating_sub(1));
            self.view_scroll = self
                .view_scroll
                .saturating_add(page)
                .min(line_count.saturating_sub(1));
        } else if key.code == KeyCode::Home || key.code == KeyCode::Char('g') {
            self.view_cursor = 0;
            self.view_scroll = 0;
        } else if key.code == KeyCode::End || key.code == KeyCode::Char('G') {
            self.view_cursor = line_count.saturating_sub(1);
        } else if key.modifiers.is_empty() && key.code == KeyCode::Char('v') {
            if self.selection_anchor.is_some() {
                self.commit_active_range();
            } else {
                self.selection_anchor = Some(self.view_cursor);
            }
            self.status = None;
        } else if key.modifiers.is_empty() && key.code == KeyCode::Char('x') {
            self.selected_ranges.pop();
        } else if key.modifiers.is_empty() && key.code == KeyCode::Char('c') {
            self.selection_anchor = None;
            self.selected_ranges.clear();
            self.status = None;
        } else if key.modifiers.is_empty() && key.code == KeyCode::Char('a') {
            self.attach_ranges(vec![(0, line_count.saturating_sub(1))]);
        } else if key.code == KeyCode::Enter {
            self.commit_active_range();
            let ranges = if self.selected_ranges.is_empty() {
                vec![(self.view_cursor, self.view_cursor)]
            } else {
                self.selected_ranges.clone()
            };
            self.attach_ranges(ranges);
        }
    }

    fn handle_mouse(&mut self, kind: MouseEventKind) {
        let delta = match kind {
            MouseEventKind::ScrollUp => -3,
            MouseEventKind::ScrollDown => 3,
            _ => return,
        };
        match self.focus {
            FilePickerFocus::Tree => {
                self.selected = self
                    .selected
                    .saturating_add_signed(delta)
                    .min(self.rows.len().saturating_sub(1));
            }
            FilePickerFocus::View => {
                let line_count = self.preview.as_ref().map_or(0, |file| file.lines.len());
                self.view_cursor = self
                    .view_cursor
                    .saturating_add_signed(delta)
                    .min(line_count.saturating_sub(1));
            }
        }
    }

    fn activate_tree_row(&mut self) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        match row.kind {
            TreeRowKind::Parent => {
                let Some(parent) = self.root.parent() else {
                    return;
                };
                self.root = parent.to_path_buf();
                self.expanded.clear();
                self.preview = None;
                self.selection_anchor = None;
                self.selected_ranges.clear();
                self.status = None;
                if let Err(error) = self.rebuild_tree(None) {
                    self.status = Some(error);
                }
            }
            TreeRowKind::Directory => {
                if !self.expanded.insert(row.path.clone()) {
                    self.expanded.remove(&row.path);
                }
                if let Err(error) = self.rebuild_tree(Some(row.path)) {
                    self.status = Some(error);
                }
            }
            TreeRowKind::File => self.open_file(row.path),
        }
    }

    fn open_file(&mut self, path: PathBuf) {
        match load_preview(&path) {
            Ok(preview) => {
                self.preview = Some(preview);
                self.view_cursor = 0;
                self.view_scroll = 0;
                self.selection_anchor = None;
                self.selected_ranges.clear();
                self.status = None;
                self.focus = FilePickerFocus::View;
            }
            Err(error) => {
                self.preview = None;
                self.selection_anchor = None;
                self.selected_ranges.clear();
                self.status = Some(error);
            }
        }
    }

    fn attach_ranges(&mut self, mut ranges: Vec<(usize, usize)>) {
        let Some(preview) = &self.preview else {
            return;
        };
        let last_line = preview.lines.len().saturating_sub(1);
        for (start, end) in &mut ranges {
            *start = (*start).min(last_line);
            *end = (*end).min(last_line).max(*start);
        }
        normalize_ranges(&mut ranges);
        let total_bytes = ranges
            .iter()
            .map(|(start, end)| {
                preview.lines[*start..=*end]
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
            })
            .sum::<usize>();
        if total_bytes > MAX_ATTACHMENT_BYTES {
            self.status = Some(format!(
                "{}：{}",
                ui_text("Selected range is too large", "所选范围过大"),
                ui_text("512 KiB maximum", "最多 512 KiB")
            ));
            return;
        }
        let path = escape_xml_attribute(&preview.path.to_string_lossy());
        let instruction = ui_text(
            "Use only the following selected file range for this request. Do not read the rest of this file unless I explicitly ask.",
            "以下是我为本轮选择的文件内容，请只使用这些行；除非我明确要求，不要读取该文件的其他部分。",
        );
        let mut payload = format!("\n{instruction}\n<codex_file_context path=\"{path}\">\n");
        for (start, end) in &ranges {
            let body = preview.lines[*start..=*end].join("\n");
            payload.push_str(&format!(
                "<range start=\"{}\" end=\"{}\">\n{body}\n</range>\n",
                start + 1,
                end + 1
            ));
        }
        payload.push_str("</codex_file_context>\n");
        let display_path = display_path(&preview.path, &self.root);
        let range_label = ranges
            .iter()
            .map(|(start, end)| format!("{}-{}", start + 1, end + 1))
            .collect::<Vec<_>>()
            .join(",");
        let placeholder = format!("[{} {display_path}:{range_label}]", ui_text("File", "文件"),);
        self.app_event_tx.send(AppEvent::AttachFileContext {
            placeholder,
            payload,
        });
        self.done = true;
    }

    fn commit_active_range(&mut self) {
        let Some(anchor) = self.selection_anchor.take() else {
            return;
        };
        let start = anchor.min(self.view_cursor);
        let end = anchor.max(self.view_cursor);
        self.selected_ranges.push((start, end));
        normalize_ranges(&mut self.selected_ranges);
    }

    fn line_is_selected(&self, line: usize) -> bool {
        self.selected_ranges
            .iter()
            .any(|(start, end)| line >= *start && line <= *end)
            || self.selection_anchor.is_some_and(|anchor| {
                line >= anchor.min(self.view_cursor) && line <= anchor.max(self.view_cursor)
            })
    }

    fn selection_summary(&self) -> Option<String> {
        let mut ranges = self.selected_ranges.clone();
        if let Some(anchor) = self.selection_anchor {
            ranges.push((anchor.min(self.view_cursor), anchor.max(self.view_cursor)));
        }
        normalize_ranges(&mut ranges);
        (!ranges.is_empty()).then(|| {
            format!(
                "{} {}",
                ui_text("Lines", "行"),
                ranges
                    .iter()
                    .map(|(start, end)| format!("{}-{}", start + 1, end + 1))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
    }

    fn selected_row(&self) -> Option<&TreeRow> {
        self.rows.get(self.selected)
    }

    fn rebuild_tree(&mut self, preferred_path: Option<PathBuf>) -> Result<(), String> {
        self.status = None;
        let mut rows = Vec::new();
        if let Some(parent) = self.root.parent() {
            rows.push(TreeRow {
                path: parent.to_path_buf(),
                name: "..".to_string(),
                depth: 0,
                kind: TreeRowKind::Parent,
            });
        }
        self.tree_truncated = false;
        append_directory_rows(
            &self.root,
            0,
            self.show_hidden,
            &self.expanded,
            &mut rows,
            &mut self.tree_truncated,
        )?;
        self.rows = rows;
        self.selected = preferred_path
            .and_then(|path| self.rows.iter().position(|row| row.path == path))
            .unwrap_or(0)
            .min(self.rows.len().saturating_sub(1));
        if self.tree_truncated {
            self.status = Some(
                ui_text(
                    "Directory listing was truncated at 2000 rows.",
                    "目录内容超过 2000 行，列表已截断。",
                )
                .to_string(),
            );
        }
        Ok(())
    }

    pub(crate) fn is_done(&self) -> bool {
        self.done
    }
}

fn resolve_requested_path(cwd: &Path, initial_path: Option<&str>) -> Result<PathBuf, String> {
    let Some(initial_path) = initial_path.map(str::trim).filter(|path| !path.is_empty()) else {
        return Ok(cwd.to_path_buf());
    };
    let path = if initial_path == "~" {
        dirs::home_dir().ok_or_else(|| "无法确定用户主目录".to_string())?
    } else if let Some(rest) = initial_path.strip_prefix("~/") {
        dirs::home_dir()
            .ok_or_else(|| "无法确定用户主目录".to_string())?
            .join(rest)
    } else {
        let path = PathBuf::from(initial_path);
        if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        }
    };
    Ok(path)
}

fn append_directory_rows(
    directory: &Path,
    depth: usize,
    show_hidden: bool,
    expanded: &HashSet<PathBuf>,
    rows: &mut Vec<TreeRow>,
    truncated: &mut bool,
) -> Result<(), String> {
    if rows.len() >= MAX_TREE_ROWS {
        *truncated = true;
        return Ok(());
    }
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("无法读取 {}：{error}", directory.display()))?;
    let mut entries = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !show_hidden && name.starts_with('.') {
                return None;
            }
            let file_type = entry.file_type().ok()?;
            if file_type.is_symlink() {
                return None;
            }
            let kind = if file_type.is_dir() {
                TreeRowKind::Directory
            } else if file_type.is_file() {
                TreeRowKind::File
            } else {
                return None;
            };
            Some(TreeRow {
                path: entry.path(),
                name,
                depth,
                kind,
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        let directory_order =
            (left.kind != TreeRowKind::Directory).cmp(&(right.kind != TreeRowKind::Directory));
        directory_order.then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    for entry in entries {
        if rows.len() >= MAX_TREE_ROWS {
            *truncated = true;
            break;
        }
        let is_expanded = entry.kind == TreeRowKind::Directory && expanded.contains(&entry.path);
        let path = entry.path.clone();
        rows.push(entry);
        if is_expanded {
            append_directory_rows(&path, depth + 1, show_hidden, expanded, rows, truncated)?;
        }
    }
    Ok(())
}

fn load_preview(path: &Path) -> Result<FilePreview, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("无法读取 {}：{error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("不是普通文件：{}", path.display()));
    }
    if metadata.len() > MAX_PREVIEW_BYTES {
        return Err(ui_text(
            "File is larger than the 8 MiB preview limit.",
            "文件超过 8 MiB 预览上限。",
        )
        .to_string());
    }
    let bytes =
        std::fs::read(path).map_err(|error| format!("无法读取 {}：{error}", path.display()))?;
    if bytes.iter().take(8 * 1024).any(|byte| *byte == 0) {
        return Err(ui_text(
            "Binary files cannot be attached as text.",
            "二进制文件不能作为文本附加。",
        )
        .to_string());
    }
    let text =
        String::from_utf8(bytes).map_err(|_| format!("文件不是有效 UTF-8：{}", path.display()))?;
    let mut lines = text
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect::<Vec<_>>();
    if text.ends_with('\n') {
        lines.pop();
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    let highlighted = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(|language| highlight_code_to_styled_spans(&text, language));
    Ok(FilePreview {
        path: path.to_path_buf(),
        lines,
        highlighted,
    })
}

fn display_path(path: &Path, root: &Path) -> String {
    pathdiff::diff_paths(path, root)
        .unwrap_or_else(|| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn escape_xml_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn normalize_ranges(ranges: &mut Vec<(usize, usize)>) {
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges.drain(..) {
        if let Some((_, previous_end)) = merged.last_mut()
            && start <= previous_end.saturating_add(1)
        {
            *previous_end = (*previous_end).max(end);
        } else {
            merged.push((start, end));
        }
    }
    *ranges = merged;
}

fn panel(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(hairline_style())
        .title(title)
        .style(Style::reset())
}

fn focused_panel(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(focused_border_style())
        .title(title)
        .style(Style::reset())
}
