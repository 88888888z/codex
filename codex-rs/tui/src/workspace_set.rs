//! Named multi-project workspace roots loaded from `.codex/workspace.toml`.

use codex_config::types::TuiLanguage;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub(crate) const WORKSPACE_DIR: &str = ".codex";
pub(crate) const WORKSPACE_MANIFEST_FILE: &str = "workspace.toml";
pub(crate) const LEGACY_WORKSPACE_MANIFEST_FILE: &str = ".codex-workspace.toml";

#[derive(Clone, Debug)]
pub(crate) struct WorkspaceSet {
    pub(crate) name: String,
    pub(crate) manifest_path: PathBuf,
    pub(crate) roots: Vec<WorkspaceRoot>,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkspaceRoot {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
    pub(crate) write: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceManifest {
    name: String,
    #[serde(default)]
    roots: Vec<WorkspaceManifestRoot>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceManifestRoot {
    id: String,
    path: PathBuf,
    #[serde(default)]
    write: bool,
}

#[derive(Debug, Serialize)]
struct WorkspaceManifestWrite {
    name: String,
    roots: Vec<WorkspaceManifestRootWrite>,
}

#[derive(Debug, Serialize)]
struct WorkspaceManifestRootWrite {
    id: String,
    path: PathBuf,
    write: bool,
}

impl WorkspaceSet {
    /// Discover the nearest workspace manifest from `start`.
    pub(crate) fn discover(start: &Path) -> Result<Option<Self>, String> {
        let start = if start.is_file() {
            start.parent().unwrap_or(start)
        } else {
            start
        };
        let start = fs::canonicalize(start)
            .map_err(|error| format!("无法解析项目集目录 {}：{error}", start.display()))?;
        for ancestor in start.ancestors() {
            let manifest_path = ancestor.join(WORKSPACE_DIR).join(WORKSPACE_MANIFEST_FILE);
            if manifest_path.is_file() {
                return Self::load(&manifest_path).map(Some);
            }
            let legacy_path = ancestor.join(LEGACY_WORKSPACE_MANIFEST_FILE);
            if legacy_path.is_file() {
                return Self::load(&legacy_path).map(Some);
            }
        }
        Ok(None)
    }

    pub(crate) fn writable_roots(&self) -> impl Iterator<Item = &Path> {
        self.roots
            .iter()
            .filter(|root| root.write)
            .map(|root| root.path.as_path())
    }

    pub(crate) fn create(parent: &Path, selected: &[PathBuf]) -> Result<Self, String> {
        let parent = fs::canonicalize(parent)
            .map_err(|error| format!("无法解析项目集目录 {}：{error}", parent.display()))?;
        let manifest_path = parent.join(WORKSPACE_DIR).join(WORKSPACE_MANIFEST_FILE);
        let legacy_path = parent.join(LEGACY_WORKSPACE_MANIFEST_FILE);
        if manifest_path.exists() {
            return Err(format!("{} 已存在，不会覆盖", manifest_path.display()));
        }
        if legacy_path.exists() {
            return Err(format!(
                "{} 已存在；请继续使用该文件或迁移到 {}",
                legacy_path.display(),
                manifest_path.display()
            ));
        }
        if selected.is_empty() {
            return Err("至少选择一个项目".to_string());
        }
        let mut used_ids = HashSet::new();
        let mut roots = Vec::with_capacity(selected.len());
        for path in selected {
            let path = fs::canonicalize(path)
                .map_err(|error| format!("无法解析项目目录 {}：{error}", path.display()))?;
            if !path.is_dir() || path.parent() != Some(parent.as_path()) {
                return Err(format!(
                    "项目必须是当前目录的直接子目录：{}",
                    path.display()
                ));
            }
            let folder_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("project");
            roots.push(WorkspaceManifestRootWrite {
                id: unique_project_id(folder_name, &mut used_ids),
                path: PathBuf::from("./").join(
                    path.file_name()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| PathBuf::from("project")),
                ),
                write: true,
            });
        }
        let workspace_name = parent
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("workspace")
            .to_string();
        let manifest = WorkspaceManifestWrite {
            name: workspace_name,
            roots,
        };
        let contents = toml::to_string_pretty(&manifest)
            .map_err(|error| format!("无法生成项目集配置：{error}"))?;
        let workspace_dir = parent.join(WORKSPACE_DIR);
        fs::create_dir_all(&workspace_dir)
            .map_err(|error| format!("无法创建 {}：{error}", workspace_dir.display()))?;
        fs::write(&manifest_path, contents)
            .map_err(|error| format!("无法写入 {}：{error}", manifest_path.display()))?;
        Self::load(&manifest_path)
    }

    pub(crate) fn prompt_instructions(&self, language: TuiLanguage) -> String {
        let roots = self
            .roots
            .iter()
            .map(|root| {
                if language.effective() == TuiLanguage::Chinese {
                    format!(
                        "- @{}: {} ({})",
                        root.id,
                        root.path.display(),
                        if root.write { "可写" } else { "只读" }
                    )
                } else {
                    format!(
                        "- @{}: {} ({})",
                        root.id,
                        root.path.display(),
                        if root.write { "writable" } else { "read-only" }
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        if language.effective() == TuiLanguage::Chinese {
            format!(
                "<workspace_projects name=\"{}\" manifest=\"{}\">\n当前会话包含多个独立项目。把每个 @项目名 当作独立根目录。\n{}\n只检查和修改上面列出的项目，不要扫描父目录中的其他同级项目。工具调用必须使用上面给出的真实文件系统路径，不要把 @web 当作文件系统路径；回复中引用文件时保留项目标签，例如 @web/src/main.ts。跨项目修改时明确说明每个改动属于哪个项目。\n</workspace_projects>",
                self.name,
                self.manifest_path.display(),
                roots
            )
        } else {
            format!(
                "<workspace_projects name=\"{}\" manifest=\"{}\">\nThis session spans multiple independent project roots. Treat each @project id as a separate root.\n{}\nOnly inspect and modify the projects listed above. Do not scan unrelated sibling directories under the parent folder. Tool calls must use the real filesystem paths shown above; do not pass @web as a filesystem path. When citing files in prose, keep the project label, for example @web/src/main.ts. For cross-project changes, state which project each change belongs to.\n</workspace_projects>",
                self.name,
                self.manifest_path.display(),
                roots
            )
        }
    }

    fn load(manifest_path: &Path) -> Result<Self, String> {
        let contents = fs::read_to_string(manifest_path)
            .map_err(|error| format!("无法读取项目集配置 {}：{error}", manifest_path.display()))?;
        let manifest: WorkspaceManifest = toml::from_str(&contents)
            .map_err(|error| format!("无法解析项目集配置 {}：{error}", manifest_path.display()))?;
        let name = manifest.name.trim();
        if name.is_empty() {
            return Err(format!(
                "项目集配置 {} 的 name 不能为空",
                manifest_path.display()
            ));
        }
        if manifest.roots.is_empty() {
            return Err(format!(
                "项目集配置 {} 至少需要一个 [[roots]] 条目",
                manifest_path.display()
            ));
        }
        let base = manifest_path.parent().unwrap_or_else(|| Path::new("."));
        let mut ids = HashSet::new();
        let mut paths = HashSet::new();
        let mut roots = Vec::with_capacity(manifest.roots.len());
        for root in manifest.roots {
            let id = root.id.trim();
            if !valid_id(id) {
                return Err(format!(
                    "项目 id `{id}` 无效；只允许 ASCII 字母、数字、点、下划线和短横线"
                ));
            }
            if !ids.insert(id.to_string()) {
                return Err(format!("项目 id `{id}` 重复"));
            }
            let requested = if root.path.is_absolute() {
                root.path
            } else {
                base.join(root.path)
            };
            let path = fs::canonicalize(&requested).map_err(|error| {
                format!(
                    "无法解析项目 `{id}` 的目录 {}：{error}",
                    requested.display()
                )
            })?;
            if !path.is_dir() {
                return Err(format!("项目 `{id}` 的路径不是目录：{}", path.display()));
            }
            if !paths.insert(path.clone()) {
                return Err(format!("项目目录重复：{}", path.display()));
            }
            roots.push(WorkspaceRoot {
                id: id.to_string(),
                path,
                write: root.write,
            });
        }
        Ok(Self {
            name: name.to_string(),
            manifest_path: manifest_path.to_path_buf(),
            roots,
        })
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn unique_project_id(folder_name: &str, used: &mut HashSet<String>) -> String {
    let base = folder_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_ascii_lowercase();
    let base = if base.is_empty() {
        "project".to_string()
    } else {
        base.chars().take(28).collect()
    };
    if used.insert(base.clone()) {
        return base;
    }
    for suffix in 2.. {
        let candidate = format!("{base}-{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("unbounded project id suffix")
}
