//! `/projects` project-set picker.

use super::*;
use crate::bottom_pane::MultiSelectItem;
use crate::bottom_pane::MultiSelectPicker;
use crate::workspace_set::WorkspaceSet;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

impl ChatWidget {
    pub(crate) fn show_workspace_projects(&mut self) {
        let cwd = self
            .current_cwd
            .clone()
            .unwrap_or_else(|| self.config.cwd.to_path_buf());
        let workspace_set = match WorkspaceSet::discover(&cwd) {
            Ok(Some(workspace_set)) => workspace_set,
            Ok(None) => {
                self.show_workspace_creation(&cwd);
                return;
            }
            Err(error) => {
                self.add_error_message(error);
                return;
            }
        };
        let items = workspace_set
            .roots
            .iter()
            .map(|root| {
                let path = root.path.clone();
                SelectionItem {
                    name: format!("@{}", root.id),
                    description: Some(format!(
                        "{} · {}",
                        path.display(),
                        if root.write {
                            crate::i18n::ui_text("writable", "可写")
                        } else {
                            crate::i18n::ui_text("read-only", "只读")
                        }
                    )),
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::OpenFilePicker {
                            cwd: path.clone(),
                            path: None,
                        });
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                }
            })
            .collect();
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some(format!(
                "{} · {}",
                crate::i18n::ui_text("Project set", "项目集"),
                workspace_set.name
            )),
            subtitle: Some(workspace_set.manifest_path.display().to_string()),
            items,
            footer_note: Some(Line::from(crate::i18n::ui_text(
                "Edit this file to change project scope; restart Codex to apply.",
                "编辑此文件修改项目范围；重启 Codex 后生效。",
            ))),
            footer_hint: Some(Line::from(crate::i18n::ui_text(
                "Enter browse · Esc close",
                "Enter 浏览 · Esc 关闭",
            ))),
            ..SelectionViewParams::picker()
        });
    }

    fn show_workspace_creation(&mut self, parent: &Path) {
        let candidates = project_candidates(parent);
        if candidates.is_empty() {
            self.add_error_message(
                crate::i18n::ui_text(
                    "No child project directories were found. Run /projects from the folder that contains the projects.",
                    "没有找到子项目目录。请在包含多个项目的父目录中运行 /projects。",
                )
                .to_string(),
            );
            return;
        }
        let paths_by_id = candidates
            .iter()
            .map(|candidate| (candidate.id.clone(), candidate.path.clone()))
            .collect::<HashMap<_, _>>();
        let items = candidates
            .iter()
            .map(|candidate| MultiSelectItem {
                id: candidate.id.clone(),
                name: candidate.id.clone(),
                description: Some(candidate.description.clone()),
                enabled: false,
                orderable: true,
                section_break_after: false,
            })
            .collect();
        let parent = parent.to_path_buf();
        let picker = MultiSelectPicker::builder(
            crate::i18n::ui_text("Create project set", "创建项目集").to_string(),
            Some(format!(
                "{} · {}",
                crate::i18n::ui_text(
                    "Select the projects that Codex may inspect and modify.",
                    "选择允许 Codex 读取和修改的项目。"
                ),
                parent.display()
            )),
            self.app_event_tx.clone(),
        )
        .list_keymap(self.bottom_pane.list_keymap())
        .items(items)
        .enable_ordering()
        .require_selection()
        .on_preview(|items| {
            let selected = items
                .iter()
                .filter(|item| item.enabled)
                .map(|item| format!("@{}", item.id))
                .collect::<Vec<_>>();
            (!selected.is_empty()).then(|| Line::from(selected.join(", ")).dim())
        })
        .on_confirm(move |ids, tx| {
            let roots = ids
                .iter()
                .filter_map(|id| paths_by_id.get(id).cloned())
                .collect();
            tx.send(AppEvent::CreateWorkspaceSet {
                parent: parent.clone(),
                roots,
            });
        })
        .build();
        self.bottom_pane.show_view(Box::new(picker));
    }
}

struct ProjectCandidate {
    id: String,
    path: PathBuf,
    description: String,
}

fn project_candidates(parent: &Path) -> Vec<ProjectCandidate> {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut candidates = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || ignored_directory(&name) {
                return None;
            }
            let path = entry.path();
            path.is_dir().then(|| ProjectCandidate {
                id: name,
                description: format!(
                    "{} · {}",
                    path.display(),
                    project_markers(&path).unwrap_or_else(|| {
                        crate::i18n::ui_text("directory", "目录").to_string()
                    })
                ),
                path,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    candidates.truncate(/*rows*/ 200);
    candidates
}

fn ignored_directory(name: &str) -> bool {
    matches!(
        name,
        "target" | "node_modules" | "vendor" | "dist" | "build" | ".next"
    )
}

fn project_markers(path: &Path) -> Option<String> {
    let mut markers = Vec::new();
    if path.join(".git").exists() {
        markers.push("Git");
    }
    if path.join("Cargo.toml").is_file() {
        markers.push("Rust");
    }
    if path.join("package.json").is_file() {
        markers.push("Node");
    }
    if path.join("pyproject.toml").is_file() || path.join("setup.py").is_file() {
        markers.push("Python");
    }
    if path.join("go.mod").is_file() {
        markers.push("Go");
    }
    if path.join("Makefile").is_file() {
        markers.push("Make");
    }
    (!markers.is_empty()).then(|| markers.join(" + "))
}
