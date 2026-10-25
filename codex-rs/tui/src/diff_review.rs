//! Structured `/diff` review surface with a file tree, side-by-side hunks, and rollback.

use std::collections::BTreeSet;
use std::io::Result;
use std::path::Path;
use std::path::PathBuf;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::i18n::ui_text;
use crate::keymap::PagerKeymap;
use crate::key_hint::KeyBindingListExt;
use crate::render::highlight::highlight_code_to_styled_spans;
use crate::style::accent_color;
use crate::style::hairline_style;
use crate::terminal_palette::best_color;
use crate::terminal_palette::default_bg;
use crate::tui;
use crate::tui::TuiEvent;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::MouseEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiffFileStatus {
    Added,
    Deleted,
    Modified,
}

impl DiffFileStatus {
    fn marker(self) -> &'static str {
        match self {
            Self::Added => "+",
            Self::Deleted => "−",
            Self::Modified => "M",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Added => ui_text("Added", "新增"),
            Self::Deleted => ui_text("Deleted", "删除"),
            Self::Modified => ui_text("Modified", "修改"),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct DiffLine {
    number: usize,
    highlight_index: usize,
    text: String,
}

#[derive(Clone, Debug, Default)]
struct DiffRow {
    old: Option<DiffLine>,
    new: Option<DiffLine>,
    changed: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct DiffFile {
    pub(crate) path: PathBuf,
    pub(crate) status: DiffFileStatus,
    rows: Vec<DiffRow>,
    old_syntax: Vec<Vec<Span<'static>>>,
    new_syntax: Vec<Vec<Span<'static>>>,
}

impl DiffFile {
    fn added_removed(&self) -> (usize, usize) {
        self.rows.iter().fold((0, 0), |(added, removed), row| {
            (
                added + usize::from(row.old.is_none() && row.new.is_some()),
                removed + usize::from(row.old.is_some() && row.new.is_none()),
            )
        })
    }

    fn prepare_highlight(&mut self) {
        let Some(language) = self.path.extension().and_then(|extension| extension.to_str()) else {
            return;
        };
        let old_code = self
            .rows
            .iter()
            .filter_map(|row| row.old.as_ref().map(|line| line.text.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
        let new_code = self
            .rows
            .iter()
            .filter_map(|row| row.new.as_ref().map(|line| line.text.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
        self.old_syntax = highlight_code_to_styled_spans(&old_code, language).unwrap_or_default();
        self.new_syntax = highlight_code_to_styled_spans(&new_code, language).unwrap_or_default();
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DiffReviewData {
    cwd: PathBuf,
    files: Vec<DiffFile>,
}

#[derive(Default)]
struct FileBuilder {
    old_path: Option<PathBuf>,
    new_path: Option<PathBuf>,
    fallback_path: Option<PathBuf>,
    status: Option<DiffFileStatus>,
    rows: Vec<DiffRow>,
    old_number: usize,
    new_number: usize,
    old_highlight_index: usize,
    new_highlight_index: usize,
    in_hunk: bool,
}

impl FileBuilder {
    fn finish(self) -> Option<DiffFile> {
        let path = self
            .new_path
            .or(self.old_path)
            .or(self.fallback_path)?;
        let status = self.status.unwrap_or(DiffFileStatus::Modified);
        Some(DiffFile {
            path,
            status,
            rows: align_diff_rows(self.rows),
            old_syntax: Vec::new(),
            new_syntax: Vec::new(),
        })
    }
}

fn align_diff_rows(rows: Vec<DiffRow>) -> Vec<DiffRow> {
    let mut aligned = Vec::with_capacity(rows.len());
    let mut index = 0;
    while index < rows.len() {
        if rows[index].old.is_some() && rows[index].new.is_none() {
            let delete_start = index;
            while index < rows.len() && rows[index].old.is_some() && rows[index].new.is_none() {
                index += 1;
            }
            let insert_start = index;
            while index < rows.len() && rows[index].old.is_none() && rows[index].new.is_some() {
                index += 1;
            }
            let deletes = &rows[delete_start..insert_start];
            let inserts = &rows[insert_start..index];
            for offset in 0..deletes.len().max(inserts.len()) {
                aligned.push(DiffRow {
                    old: deletes.get(offset).and_then(|row| row.old.clone()),
                    new: inserts.get(offset).and_then(|row| row.new.clone()),
                    changed: true,
                });
            }
        } else {
            aligned.push(rows[index].clone());
            index += 1;
        }
    }
    aligned
}

pub(crate) fn parse_diff_review(cwd: PathBuf, text: &str) -> DiffReviewData {
    let mut files = Vec::new();
    let mut current: Option<FileBuilder> = None;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(builder) = current.take()
                && let Some(file) = builder.finish()
            {
                files.push(file);
            }
            let mut builder = FileBuilder::default();
            builder.fallback_path = fallback_path_from_diff_header(rest);
            current = Some(builder);
            continue;
        }

        let Some(builder) = current.as_mut() else {
            continue;
        };
        if line.starts_with("new file mode ") {
            builder.status = Some(DiffFileStatus::Added);
        } else if line.starts_with("deleted file mode ") {
            builder.status = Some(DiffFileStatus::Deleted);
        } else if let Some(path) = line.strip_prefix("--- ") {
            builder.old_path = parse_diff_path(path);
        } else if let Some(path) = line.strip_prefix("+++ ") {
            builder.new_path = parse_diff_path(path);
        } else if line.starts_with("@@ ") {
            if let Some((old_start, new_start)) = parse_hunk_starts(line) {
                builder.old_number = old_start;
                builder.new_number = new_start;
                builder.in_hunk = true;
            }
        } else if builder.in_hunk {
            if line.starts_with('\\') {
                continue;
            }
            let text = line.get(1..).unwrap_or_default().to_string();
            match line.as_bytes().first().copied() {
                Some(b'-') => {
                    builder.rows.push(DiffRow {
                        old: Some(DiffLine {
                            number: builder.old_number,
                            highlight_index: builder.old_highlight_index,
                            text,
                        }),
                        new: None,
                        changed: true,
                    });
                    builder.old_number += 1;
                    builder.old_highlight_index += 1;
                }
                Some(b'+') => {
                    builder.rows.push(DiffRow {
                        old: None,
                        new: Some(DiffLine {
                            number: builder.new_number,
                            highlight_index: builder.new_highlight_index,
                            text,
                        }),
                        changed: true,
                    });
                    builder.new_number += 1;
                    builder.new_highlight_index += 1;
                }
                Some(b' ') => {
                    builder.rows.push(DiffRow {
                        old: Some(DiffLine {
                            number: builder.old_number,
                            highlight_index: builder.old_highlight_index,
                            text: text.clone(),
                        }),
                        new: Some(DiffLine {
                            number: builder.new_number,
                            highlight_index: builder.new_highlight_index,
                            text,
                        }),
                        changed: false,
                    });
                    builder.old_number += 1;
                    builder.new_number += 1;
                    builder.old_highlight_index += 1;
                    builder.new_highlight_index += 1;
                }
                _ => {}
            }
        }
    }

    if let Some(builder) = current
        && let Some(file) = builder.finish()
    {
        files.push(file);
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    DiffReviewData { cwd, files }
}

fn parse_diff_path(value: &str) -> Option<PathBuf> {
    let value = value.split('\t').next()?.trim();
    if value == "/dev/null" {
        return None;
    }
    let value = unquote_git_path(value);
    let value = value
        .strip_prefix("a/")
        .or_else(|| value.strip_prefix("b/"))
        .unwrap_or(&value);
    (!value.is_empty()).then(|| PathBuf::from(value))
}

fn fallback_path_from_diff_header(value: &str) -> Option<PathBuf> {
    let (_, new_path) = value.rsplit_once(" b/")?;
    let path = unquote_git_path(&format!("b/{new_path}"));
    parse_diff_path(&path)
}

fn unquote_git_path(value: &str) -> String {
    let value = value.trim();
    let Some(value) = value.strip_prefix('"').and_then(|value| value.strip_suffix('"')) else {
        return value.to_string();
    };
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(character);
        }
    }
    out
}

fn parse_hunk_starts(line: &str) -> Option<(usize, usize)> {
    let mut parts = line.strip_prefix("@@ ")?.split_whitespace();
    let old = parts.find(|part| part.starts_with('-'))?;
    let new = parts.find(|part| part.starts_with('+'))?;
    Some((range_start(old), range_start(new)))
}

fn range_start(range: &str) -> usize {
    range[1..]
        .split(',')
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(/*default*/ 1)
}

pub(crate) struct DiffReviewOverlay {
    data: DiffReviewData,
    selected: usize,
    scroll: usize,
    focus: DiffFocus,
    confirm_rollback: bool,
    done: bool,
    app_event_tx: AppEventSender,
    keymap: PagerKeymap,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum DiffFocus {
    #[default]
    Tree,
    View,
}

impl DiffReviewOverlay {
    pub(crate) fn new(
        mut data: DiffReviewData,
        app_event_tx: AppEventSender,
        keymap: PagerKeymap,
    ) -> Self {
        for file in &mut data.files {
            file.prepare_highlight();
        }
        Self {
            data,
            selected: 0,
            scroll: 0,
            focus: DiffFocus::Tree,
            confirm_rollback: false,
            done: false,
            app_event_tx,
            keymap,
        }
    }

    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let [header_area, body_area, footer_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .areas(area);
        let file_count = if crate::i18n::is_chinese(crate::i18n::current_language()) {
            format!("  {} 个文件", self.data.files.len())
        } else {
            format!("  {} files", self.data.files.len())
        };
        Line::from(vec![
            ui_text("Diff review", "差异审阅").bold(),
            "  ".into(),
            match self.focus {
                DiffFocus::Tree => ui_text("Tree", "目录树"),
                DiffFocus::View => ui_text("View", "代码视图"),
            }
            .fg(accent_color())
            .bold(),
            file_count.dim(),
        ])
        .render(header_area, buf);

        let tree_width = body_area.width.saturating_mul(/*percent*/ 30).max(24).min(40);
        let [tree_area, content_area] = Layout::horizontal([
            Constraint::Length(tree_width.min(body_area.width)),
            Constraint::Min(1),
        ])
        .areas(body_area);
        self.render_tree(tree_area, buf);
        self.render_file(content_area, buf);
        self.render_footer(footer_area, buf);
    }

    fn render_tree(&self, area: Rect, buf: &mut Buffer) {
        let block = if self.focus == DiffFocus::Tree {
            focused_panel(ui_text("Changes", "变更"))
        } else {
            panel(ui_text("Changes", "变更"))
        };
        let inner = block.inner(area);
        block.render(area, buf);
        let (rows, selected_row) = self.tree_rows();
        let height = usize::from(inner.height);
        let start = selected_row
            .saturating_sub(height / 2)
            .min(rows.len().saturating_sub(height));
        Paragraph::new(rows.into_iter().skip(start).take(height).collect::<Vec<_>>())
            .render(inner, buf);
    }

    fn tree_rows(&self) -> (Vec<Line<'static>>, usize) {
        let mut rows = Vec::new();
        let mut directories = BTreeSet::new();
        let mut selected_row = 0;
        for (index, file) in self.data.files.iter().enumerate() {
            let components = file.path.components().collect::<Vec<_>>();
            for depth in 0..components.len().saturating_sub(1) {
                let directory = components[..=depth]
                    .iter()
                    .map(|component| component.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                if directories.insert(directory.clone()) {
                    rows.push(Line::from(format!("{}{}/", "  ".repeat(depth), components[depth].as_os_str().to_string_lossy())).dim());
                }
            }
            let file_name = components
                .last()
                .map(|component| component.as_os_str().to_string_lossy())
                .unwrap_or_default();
            let marker = if index == self.selected { "›" } else { " " };
            let (added, removed) = file.added_removed();
            let mut line = Line::from(vec![
                format!(
                    "{marker} {}{} {} ",
                    "  ".repeat(components.len().saturating_sub(1)),
                    file_name,
                    file.status.marker()
                )
                .into(),
                format!("+{added} -{removed}").dim(),
            ]);
            if index == self.selected {
                line = line.fg(accent_color()).bold();
                selected_row = rows.len();
            }
            rows.push(line);
        }
        (rows, selected_row)
    }

    fn render_file(&self, area: Rect, buf: &mut Buffer) {
        let Some(file) = self.data.files.get(self.selected) else {
            Paragraph::new(ui_text("No changes detected.", "没有检测到变更。").dim())
                .block(panel(ui_text("Before / After", "修改前 / 修改后")))
                .render(area, buf);
            return;
        };
        let title = format!(
            "{}  ·  {}",
            self.display_path(file),
            file.status.label()
        );
        let block = if self.focus == DiffFocus::View {
            focused_panel(title)
        } else {
            panel(title)
        };
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.width < 8 || inner.height == 0 {
            return;
        }
        let [old_area, _, new_area] = Layout::horizontal([
            Constraint::Percentage(50),
            Constraint::Length(1),
            Constraint::Percentage(50),
        ])
        .areas(inner);
        let old_block = panel(ui_text("Before", "修改前"));
        let new_block = panel(ui_text("After", "修改后"));
        let old_inner = old_block.inner(old_area);
        let new_inner = new_block.inner(new_area);
        old_block.render(old_area, buf);
        new_block.render(new_area, buf);
        let max_scroll = file
            .rows
            .len()
            .saturating_sub(usize::from(old_inner.height));
        let scroll = self.scroll.min(max_scroll) as u16;
        for offset in 0..old_inner.height {
            let row_index = usize::from(scroll + offset);
            let Some(row) = file.rows.get(row_index) else {
                break;
            };
            let old_syntax = row
                .old
                .as_ref()
                .and_then(|line| file.old_syntax.get(line.highlight_index))
                .map(Vec::as_slice);
            render_diff_row(
                Rect::new(
                    old_inner.x,
                    old_inner.y + offset,
                    old_inner.width,
                    1,
                ),
                row.old.as_ref(),
                old_syntax,
                DiffSide::Old,
                row.changed,
                buf,
            );
            let new_syntax = row
                .new
                .as_ref()
                .and_then(|line| file.new_syntax.get(line.highlight_index))
                .map(Vec::as_slice);
            render_diff_row(
                Rect::new(
                    new_inner.x,
                    new_inner.y + offset,
                    new_inner.width,
                    1,
                ),
                row.new.as_ref(),
                new_syntax,
                DiffSide::New,
                row.changed,
                buf,
            );
        }
        Span::from("│").dim().render(
            Rect::new(old_area.right(), inner.y, 1, inner.height),
            buf,
        );
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        if self.confirm_rollback {
            let file = self
                .data
                .files
                .get(self.selected)
                .map(|file| self.display_path(file))
                .unwrap_or_default();
            let prompt = if crate::i18n::is_chinese(crate::i18n::current_language()) {
                format!("回滚 {file}？y 确认 · n/Esc 取消")
            } else {
                format!("Rollback {file}? y confirm · n/Esc cancel")
            };
            Line::from(prompt)
            .fg(Color::Yellow)
            .render(Rect::new(area.x, area.y, area.width, 1), buf);
        } else {
            let focus = match self.focus {
                DiffFocus::Tree => ui_text("Tree", "目录树"),
                DiffFocus::View => ui_text("View", "代码视图"),
            };
            let hints = if crate::i18n::is_chinese(crate::i18n::current_language()) {
                format!(
                    "区域：{focus} · Tab 切换 · ↑/↓ 操作 · PgUp/PgDn 滚动 · r 回滚 · Esc 关闭"
                )
            } else {
                format!(
                    "Focus: {focus} · Tab switch · ↑/↓ act · PgUp/PgDn scroll · r rollback · Esc close"
                )
            };
            Line::from(hints)
            .dim()
            .render(Rect::new(area.x, area.y, area.width, 1), buf);
        }
    }

    pub(crate) fn handle_event(&mut self, tui: &mut tui::Tui, event: TuiEvent) -> Result<()> {
        match event {
            TuiEvent::Key(key) if key.kind == KeyEventKind::Press => {
                self.handle_key(tui, key)?;
            }
            TuiEvent::Mouse(mouse) => self.handle_mouse(tui, mouse.kind)?,
            TuiEvent::Draw | TuiEvent::Resume | TuiEvent::Resize(_) | TuiEvent::FocusGained => {
                tui.draw(u16::MAX, |frame| self.render(frame.area(), frame.buffer))?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_key(&mut self, tui: &mut tui::Tui, key: KeyEvent) -> Result<()> {
        if self.confirm_rollback {
            match key.code {
                KeyCode::Char('y') => self.rollback_selected(),
                KeyCode::Char('n') | KeyCode::Esc => self.confirm_rollback = false,
                _ => {}
            }
            tui.frame_requester().schedule_frame();
            return Ok(());
        }
        if self.keymap.close.is_pressed(key) || key.code == KeyCode::Esc {
            self.done = true;
        } else if key.code == KeyCode::Tab && key.modifiers.is_empty() {
            self.focus = match self.focus {
                DiffFocus::Tree => DiffFocus::View,
                DiffFocus::View => DiffFocus::Tree,
            };
        } else if self.keymap.scroll_up.is_pressed(key)
            || key.code == KeyCode::Up
            || key.code == KeyCode::Char('k')
        {
            self.move_focus(-1);
        } else if self.keymap.scroll_down.is_pressed(key)
            || key.code == KeyCode::Down
            || key.code == KeyCode::Char('j')
        {
            self.move_focus(1);
        } else if self.keymap.page_up.is_pressed(key) {
            self.scroll = self.scroll.saturating_sub(10);
        } else if self.keymap.page_down.is_pressed(key) {
            self.scroll = self.scroll.saturating_add(10);
        } else if key.code == KeyCode::Char('r') && key.modifiers.is_empty() {
            self.confirm_rollback = self.data.files.get(self.selected).is_some();
        }
        tui.frame_requester().schedule_frame();
        Ok(())
    }

    fn handle_mouse(&mut self, tui: &mut tui::Tui, kind: MouseEventKind) -> Result<()> {
        if self.confirm_rollback {
            return Ok(());
        }
        match kind {
            MouseEventKind::ScrollUp => self.move_focus(-3),
            MouseEventKind::ScrollDown => self.move_focus(3),
            _ => return Ok(()),
        }
        tui.frame_requester().schedule_frame();
        Ok(())
    }

    fn move_focus(&mut self, delta: isize) {
        match self.focus {
            DiffFocus::Tree => {
                self.selected = self
                    .selected
                    .saturating_add_signed(delta)
                    .min(self.data.files.len().saturating_sub(1));
                self.scroll = 0;
            }
            DiffFocus::View => {
                self.scroll = self.scroll.saturating_add_signed(delta);
            }
        }
    }

    fn rollback_selected(&mut self) {
        let Some(file) = self.data.files.get(self.selected) else {
            self.confirm_rollback = false;
            return;
        };
        self.app_event_tx.send(AppEvent::RollbackDiffFile {
            cwd: self.data.cwd.clone(),
            path: file.path.clone(),
            status: file.status,
        });
        self.done = true;
    }

    fn display_path(&self, file: &DiffFile) -> String {
        display_path(&file.path, &self.data.cwd)
    }

    pub(crate) fn is_done(&self) -> bool {
        self.done
    }
}

#[derive(Clone, Copy)]
enum DiffSide {
    Old,
    New,
}

fn render_diff_row(
    area: Rect,
    line: Option<&DiffLine>,
    syntax: Option<&[Span<'static>]>,
    side: DiffSide,
    changed: bool,
    buf: &mut Buffer,
) {
    if area.is_empty() {
        return;
    }
    let Some(line) = line else {
        return;
    };
    let background = changed.then(|| diff_background(side)).flatten();
    if let Some(background) = background {
        buf.set_style(area, Style::default().bg(background));
    }
    let line_style = background.map_or_else(Style::default, |background| {
        Style::default().bg(background)
    });
    let number_width = area.width.min(5);
    let number_color = if changed {
        match side {
            DiffSide::Old => Color::Red,
            DiffSide::New => Color::Green,
        }
    } else {
        Color::DarkGray
    };
    Span::styled(
        format!("{:>number_width$}", line.number, number_width = usize::from(number_width)),
        Style::default().fg(number_color).bg(
            background.unwrap_or(Color::Reset),
        ),
    )
    .render(Rect::new(area.x, area.y, number_width, 1), buf);
    if area.width > number_width {
        let content = syntax.map_or_else(
            || Line::from(Span::styled(line.text.clone(), line_style)),
            |spans| {
                let mut spans = spans.to_vec();
                for span in &mut spans {
                    span.style.bg = background;
                }
                Line::from(spans)
            },
        );
        Paragraph::new(content).render(
            Rect::new(
                area.x + number_width,
                area.y,
                area.width - number_width,
                1,
            ),
            buf,
        );
    }
}

fn diff_background(side: DiffSide) -> Option<Color> {
    let background = default_bg()?;
    let light = crate::color::is_light(background);
    let rgb = match (side, light) {
        (DiffSide::Old, false) => (74, 34, 29),
        (DiffSide::New, false) => (33, 58, 43),
        (DiffSide::Old, true) => (255, 235, 233),
        (DiffSide::New, true) => (218, 251, 225),
    };
    Some(best_color(rgb))
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
        .border_style(crate::style::focused_border_style())
        .title(title)
        .style(Style::reset())
}

fn display_path(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .display()
        .to_string()
}
