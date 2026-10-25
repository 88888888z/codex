//! Task-level workspace checkpoints used by `/checkpoint` and `/rewind`.
//!
//! Checkpoints capture changed, non-ignored Git files before each user turn. Restoring
//! writes file contents in the worktree; it intentionally leaves the Git index untouched.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io::Write;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use chrono::DateTime;
use chrono::Local;
use chrono::Utc;
use codex_git_utils::SAFE_BARE_REPOSITORY_CONFIG;
use codex_protocol::ThreadId;
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
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::diff_review::DiffFileStatus;
use crate::diff_review::DiffReviewData;
use crate::diff_review::parse_diff_review;
use crate::diff_review::render_diff_preview;
use crate::i18n::ui_text;
use crate::key_hint::KeyBindingListExt;
use crate::keymap::PagerKeymap;
use crate::style::StatusTone;
use crate::style::accent_color;
use crate::style::focused_border_style;
use crate::style::hairline_style;
use crate::style::status_style;
use crate::tui;
use crate::tui::TuiEvent;
use crate::workspace_command::WorkspaceCommand;
use crate::workspace_command::WorkspaceCommandExecutor;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::MouseEventKind;

const CHECKPOINT_VERSION: u32 = 1;
const CHECKPOINT_LIMIT: usize = 50;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CHECKPOINT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PREVIEW_TEXT_BYTES: usize = 1024 * 1024;
const GIT_TIMEOUT: Duration = Duration::from_secs(/*secs*/ 30);
const DISABLE_HOOKS_CONFIG: &str = if cfg!(windows) {
    "core.hooksPath=NUL"
} else {
    "core.hooksPath=/dev/null"
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RewindRestoreTarget {
    Files,
    Conversation,
    Both,
}

#[derive(Clone, Debug)]
pub(crate) struct CheckpointSummary {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) file_count: usize,
    pub(crate) thread_id: Option<ThreadId>,
    pub(crate) nth_user_message: Option<usize>,
    pub(crate) safety: bool,
    checkpoint_dir: PathBuf,
}

impl CheckpointSummary {
    pub(crate) fn has_conversation(&self) -> bool {
        self.thread_id.is_some() && self.nth_user_message.is_some()
    }

    pub(crate) fn display_time(&self) -> String {
        self.created_at
            .with_timezone(&Local)
            .format("%m-%d %H:%M")
            .to_string()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CheckpointManifest {
    version: u32,
    id: String,
    created_at: DateTime<Utc>,
    repo_root: PathBuf,
    git_head: String,
    thread_id: Option<String>,
    nth_user_message: Option<usize>,
    label: String,
    safety: bool,
    files: Vec<SnapshotFile>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotFileKind {
    File,
    Deleted,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SnapshotFile {
    path: PathBuf,
    kind: SnapshotFileKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    data_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<u32>,
}

#[derive(Debug)]
pub(crate) struct RewindRestoreOutcome {
    pub(crate) safety_checkpoint_id: Option<String>,
}

pub(crate) async fn capture_checkpoint(
    runner: &dyn WorkspaceCommandExecutor,
    codex_home: &Path,
    cwd: &Path,
    thread_id: Option<ThreadId>,
    nth_user_message: Option<usize>,
    label: Option<String>,
    safety: bool,
) -> Result<Option<CheckpointSummary>, String> {
    let Some(repo_root) = git_repo_root(runner, cwd).await? else {
        return Ok(None);
    };
    let git_head = git_stdout(runner, &repo_root, &["rev-parse", "HEAD"]).await?;
    let git_head = git_head.trim().to_string();
    if git_head.is_empty() {
        return Err(ui_text(
            "Checkpoints require at least one Git commit.",
            "检查点至少需要一个 Git 提交。",
        )
        .to_string());
    }
    let changed_paths = changed_paths(runner, &repo_root, &git_head).await?;
    let checkpoint_id = Uuid::new_v4().to_string();
    let thread_key = thread_id.map_or_else(|| "shared".to_string(), |id| id.to_string());
    let checkpoint_root = checkpoints_root(codex_home, &repo_root)?.join(thread_key);
    let label = sanitize_label(
        label.unwrap_or_else(|| ui_text("Manual checkpoint", "手动检查点").to_string()),
    );
    let manifest = CheckpointManifest {
        version: CHECKPOINT_VERSION,
        id: checkpoint_id.clone(),
        created_at: Utc::now(),
        repo_root: repo_root.clone(),
        git_head,
        thread_id: thread_id.map(|id| id.to_string()),
        nth_user_message,
        label,
        safety,
        files: Vec::new(),
    };
    let created = tokio::task::spawn_blocking(move || {
        write_checkpoint(checkpoint_root, manifest, changed_paths)
    })
    .await
    .map_err(|error| format!("检查点任务失败：{error}"))??;
    if !safety {
        prune_checkpoints(&created.1);
    }
    Ok(Some(created.0))
}

pub(crate) async fn list_checkpoints(
    runner: &dyn WorkspaceCommandExecutor,
    codex_home: &Path,
    cwd: &Path,
    thread_id: Option<ThreadId>,
) -> Result<Vec<CheckpointSummary>, String> {
    let Some(repo_root) = git_repo_root(runner, cwd).await? else {
        return Ok(Vec::new());
    };
    let thread_key = thread_id.map_or_else(|| "shared".to_string(), |id| id.to_string());
    let root = checkpoints_root(codex_home, &repo_root)?.join(thread_key);
    let repo_root_for_task = repo_root.clone();
    tokio::task::spawn_blocking(move || -> Result<Vec<CheckpointSummary>, String> {
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("无法读取检查点目录：{error}")),
        };
        let mut checkpoints = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| {
                let path = entry.path();
                let manifest = read_manifest(&path).ok()?;
                Some((path, manifest))
            })
            .filter(|(_, manifest)| {
                manifest.version == CHECKPOINT_VERSION && manifest.repo_root == repo_root_for_task
            })
            .map(|(path, manifest)| summary_from_manifest(path, manifest))
            .collect::<Vec<_>>();
        checkpoints.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        checkpoints.truncate(CHECKPOINT_LIMIT);
        Ok(checkpoints)
    })
    .await
    .map_err(|error| format!("读取检查点列表失败：{error}"))?
}

pub(crate) async fn load_preview(
    runner: &dyn WorkspaceCommandExecutor,
    checkpoint: &CheckpointSummary,
) -> Result<DiffReviewData, String> {
    let manifest = read_manifest(&checkpoint.checkpoint_dir)?;
    let current_head = git_stdout(runner, &manifest.repo_root, &["rev-parse", "HEAD"]).await?;
    let current_head = current_head.trim();
    let current_paths = changed_paths(runner, &manifest.repo_root, current_head).await?;
    let checkpoint_dir = checkpoint.checkpoint_dir.clone();
    tokio::task::spawn_blocking(move || build_preview(&manifest, &checkpoint_dir, current_paths))
        .await
        .map_err(|error| format!("生成检查点预览失败：{error}"))?
}

pub(crate) async fn restore_checkpoint_files(
    runner: &dyn WorkspaceCommandExecutor,
    codex_home: &Path,
    checkpoint: &CheckpointSummary,
) -> Result<RewindRestoreOutcome, String> {
    let manifest = read_manifest(&checkpoint.checkpoint_dir)?;
    let current_root = canonicalize_repo_root(runner, &manifest.repo_root).await?;
    if current_root != manifest.repo_root {
        return Err(ui_text(
            "The checkpoint belongs to a different workspace.",
            "该检查点属于另一个工作区。",
        )
        .to_string());
    }
    let current_head = git_stdout(runner, &manifest.repo_root, &["rev-parse", "HEAD"]).await?;
    if current_head.trim() != manifest.git_head {
        return Err(ui_text(
            "Git HEAD changed since this checkpoint; create a new checkpoint before restoring.",
            "Git HEAD 已变化；请重新创建检查点后再恢复。",
        )
        .to_string());
    }

    let thread_id = manifest
        .thread_id
        .as_deref()
        .and_then(|value| ThreadId::from_string(value).ok());
    let safety_label = ui_text(
        "Automatic safety checkpoint before restore",
        "恢复前自动安全点",
    )
    .to_string();
    let safety = capture_checkpoint(
        runner,
        codex_home,
        &manifest.repo_root,
        thread_id,
        None,
        Some(safety_label),
        true,
    )
    .await?
    .ok_or_else(|| {
        ui_text(
            "Could not create a safety checkpoint; restore was cancelled.",
            "无法创建安全点，已取消恢复。",
        )
        .to_string()
    })?;

    let current_paths = changed_paths(runner, &manifest.repo_root, &manifest.git_head).await?;
    let head_paths = head_paths(runner, &manifest.repo_root).await?;
    let checkpoint_paths = manifest
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>();
    let all_paths = current_paths
        .union(&checkpoint_paths)
        .cloned()
        .collect::<BTreeSet<_>>();
    let reset_paths = all_paths
        .iter()
        .filter(|path| !checkpoint_paths.contains(*path) && head_paths.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    for chunk in reset_paths.chunks(/*chunk*/ 100) {
        let mut args = vec![
            "restore".to_string(),
            "--source=HEAD".to_string(),
            "--worktree".to_string(),
            "--".to_string(),
        ];
        args.extend(chunk.iter().map(|path| path.to_string_lossy().into_owned()));
        git_ok(runner, &manifest.repo_root, &args).await?;
    }

    let remove_paths = all_paths
        .iter()
        .filter(|path| !checkpoint_paths.contains(*path) && !head_paths.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    let checkpoint_dir = checkpoint.checkpoint_dir.clone();
    tokio::task::spawn_blocking(move || {
        restore_snapshot_entries(&manifest, &checkpoint_dir, &remove_paths)
    })
    .await
    .map_err(|error| format!("恢复检查点任务失败：{error}"))??;

    Ok(RewindRestoreOutcome {
        safety_checkpoint_id: Some(safety.id),
    })
}

fn write_checkpoint(
    root: PathBuf,
    mut manifest: CheckpointManifest,
    changed_paths: BTreeSet<PathBuf>,
) -> Result<(CheckpointSummary, PathBuf), String> {
    std::fs::create_dir_all(&root).map_err(|error| format!("无法创建检查点目录：{error}"))?;
    let checkpoint_dir = root.join(&manifest.id);
    let temp_dir = root.join(format!(".tmp-{}", manifest.id));
    let data_dir = temp_dir.join("data");
    std::fs::create_dir_all(&data_dir)
        .map_err(|error| format!("无法创建检查点数据目录：{error}"))?;

    let result = (|| {
        let mut total_bytes = 0_u64;
        for (index, relative_path) in changed_paths.iter().enumerate() {
            validate_relative_path(relative_path)?;
            let absolute_path = safe_path(&manifest.repo_root, relative_path)?;
            let Some(metadata) = metadata_if_exists(&absolute_path)? else {
                manifest.files.push(SnapshotFile {
                    path: relative_path.clone(),
                    kind: SnapshotFileKind::Deleted,
                    data_file: None,
                    mode: None,
                });
                continue;
            };
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "检查点暂不支持符号链接变更：{}",
                    relative_path.display()
                ));
            }
            if !metadata.is_file() {
                return Err(format!("检查点仅支持普通文件：{}", relative_path.display()));
            }
            if metadata.len() > MAX_FILE_BYTES {
                return Err(format!(
                    "{} 超过单个文件 64 MiB 的检查点上限",
                    relative_path.display()
                ));
            }
            total_bytes = total_bytes.saturating_add(metadata.len());
            if total_bytes > MAX_CHECKPOINT_BYTES {
                return Err("检查点总大小超过 256 MiB 上限".to_string());
            }
            let data_file = format!("{index:08}");
            std::fs::copy(&absolute_path, data_dir.join(&data_file))
                .map_err(|error| format!("无法保存 {}：{error}", relative_path.display()))?;
            manifest.files.push(SnapshotFile {
                path: relative_path.clone(),
                kind: SnapshotFileKind::File,
                data_file: Some(data_file),
                mode: file_mode(&metadata),
            });
        }
        let serialized = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| format!("无法序列化检查点：{error}"))?;
        std::fs::write(temp_dir.join("manifest.json"), serialized)
            .map_err(|error| format!("无法写入检查点清单：{error}"))?;
        std::fs::rename(&temp_dir, &checkpoint_dir)
            .map_err(|error| format!("无法提交检查点：{error}"))?;
        let summary = summary_from_manifest(checkpoint_dir.clone(), manifest);
        Ok((summary, root))
    })();

    if result.is_err() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    result
}

fn build_preview(
    manifest: &CheckpointManifest,
    checkpoint_dir: &Path,
    current_paths: BTreeSet<PathBuf>,
) -> Result<DiffReviewData, String> {
    let checkpoint_files = manifest
        .files
        .iter()
        .map(|file| (file.path.clone(), file))
        .collect::<BTreeMap<_, _>>();
    let paths = current_paths
        .into_iter()
        .chain(checkpoint_files.keys().cloned())
        .collect::<BTreeSet<_>>();
    let mut patch = String::new();
    for relative_path in paths {
        validate_relative_path(&relative_path)?;
        let after = checkpoint_files
            .get(&relative_path)
            .map(|file| read_snapshot_file(checkpoint_dir, file))
            .transpose()?
            .flatten();
        let current_path = safe_path(&manifest.repo_root, &relative_path)?;
        let before = read_file_if_exists(&current_path)?;
        if before == after {
            continue;
        }
        let status = match (before.is_some(), after.is_some()) {
            (false, true) => DiffFileStatus::Added,
            (true, false) => DiffFileStatus::Deleted,
            _ => DiffFileStatus::Modified,
        };
        append_preview_file(&mut patch, &relative_path, status, before, after);
    }
    let mut data = parse_diff_review(manifest.repo_root.clone(), &patch);
    data.prepare_highlight();
    Ok(data)
}

fn append_preview_file(
    output: &mut String,
    path: &Path,
    status: DiffFileStatus,
    old: Option<Vec<u8>>,
    new: Option<Vec<u8>>,
) {
    let display_path = path.to_string_lossy().replace('\n', "\\n");
    let old_text = preview_text(old.as_deref());
    let new_text = preview_text(new.as_deref());
    let patch = diffy::create_patch(&old_text, &new_text).to_string();
    let hunks = patch
        .lines()
        .skip(/*headers*/ 2)
        .collect::<Vec<_>>()
        .join("\n");
    if hunks.is_empty() {
        return;
    }
    output.push_str(&format!("diff --git a/{display_path} b/{display_path}\n"));
    match status {
        DiffFileStatus::Added => output.push_str("new file mode 100644\n"),
        DiffFileStatus::Deleted => output.push_str("deleted file mode 100644\n"),
        DiffFileStatus::Modified => {}
    }
    if old.is_some() {
        output.push_str(&format!("--- a/{display_path}\n"));
    } else {
        output.push_str("--- /dev/null\n");
    }
    if new.is_some() {
        output.push_str(&format!("+++ b/{display_path}\n"));
    } else {
        output.push_str("+++ /dev/null\n");
    }
    output.push_str(&hunks);
    output.push('\n');
}

fn preview_text(bytes: Option<&[u8]>) -> String {
    let Some(bytes) = bytes else {
        return String::new();
    };
    if bytes.len() > MAX_PREVIEW_TEXT_BYTES {
        let digest = preview_digest(bytes);
        return if crate::i18n::is_chinese(crate::i18n::current_language()) {
            format!("[文件预览已省略：{} 字节，sha256 {digest}]\n", bytes.len())
        } else {
            format!(
                "[file preview omitted: {} bytes, sha256 {digest}]\n",
                bytes.len()
            )
        };
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) if crate::i18n::is_chinese(crate::i18n::current_language()) => {
            format!(
                "[二进制文件：{} 字节，sha256 {}]\n",
                bytes.len(),
                preview_digest(bytes)
            )
        }
        Err(_) => format!(
            "[binary file: {} bytes, sha256 {}]\n",
            bytes.len(),
            preview_digest(bytes)
        ),
    }
}

fn preview_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")[..12].to_string()
}

fn restore_snapshot_entries(
    manifest: &CheckpointManifest,
    checkpoint_dir: &Path,
    remove_paths: &[PathBuf],
) -> Result<(), String> {
    let checkpoint_files = manifest
        .files
        .iter()
        .map(|file| (file.path.clone(), file))
        .collect::<BTreeMap<_, _>>();
    for relative_path in remove_paths {
        let path = safe_path(&manifest.repo_root, relative_path)?;
        remove_file_if_exists(&path)?;
        prune_empty_parents(&manifest.repo_root, &path);
    }
    for file in checkpoint_files.values() {
        let path = safe_path(&manifest.repo_root, &file.path)?;
        match file.kind {
            SnapshotFileKind::Deleted => {
                remove_file_if_exists(&path)?;
                prune_empty_parents(&manifest.repo_root, &path);
            }
            SnapshotFileKind::File => {
                let bytes = read_snapshot_file(checkpoint_dir, file)?
                    .ok_or_else(|| format!("{} 的检查点数据缺失", file.path.display()))?;
                write_file_atomically(&manifest.repo_root, &path, &bytes, file.mode)?;
            }
        }
    }
    Ok(())
}

fn read_snapshot_file(
    checkpoint_dir: &Path,
    file: &SnapshotFile,
) -> Result<Option<Vec<u8>>, String> {
    if file.kind == SnapshotFileKind::Deleted {
        return Ok(None);
    }
    let data_file = file
        .data_file
        .as_ref()
        .ok_or_else(|| format!("{} 的检查点数据引用缺失", file.path.display()))?;
    if data_file.contains('/') || data_file.contains('\\') {
        return Err("检查点数据路径无效".to_string());
    }
    std::fs::read(checkpoint_dir.join("data").join(data_file))
        .map(Some)
        .map_err(|error| format!("无法读取 {} 的检查点数据：{error}", file.path.display()))
}

fn read_manifest(checkpoint_dir: &Path) -> Result<CheckpointManifest, String> {
    let bytes = std::fs::read(checkpoint_dir.join("manifest.json"))
        .map_err(|error| format!("无法读取检查点清单：{error}"))?;
    let manifest = serde_json::from_slice::<CheckpointManifest>(&bytes)
        .map_err(|error| format!("无法解析检查点清单：{error}"))?;
    if manifest.version != CHECKPOINT_VERSION {
        return Err(format!("检查点格式版本不受支持：{}", manifest.version));
    }
    Ok(manifest)
}

fn summary_from_manifest(
    checkpoint_dir: PathBuf,
    manifest: CheckpointManifest,
) -> CheckpointSummary {
    CheckpointSummary {
        id: manifest.id,
        label: manifest.label,
        created_at: manifest.created_at,
        file_count: manifest.files.len(),
        thread_id: manifest
            .thread_id
            .as_deref()
            .and_then(|value| ThreadId::from_string(value).ok()),
        nth_user_message: manifest.nth_user_message,
        safety: manifest.safety,
        checkpoint_dir,
    }
}

fn prune_checkpoints(checkpoint_root: &Path) {
    let Ok(entries) = std::fs::read_dir(checkpoint_root) else {
        return;
    };
    let mut checkpoints = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let manifest = read_manifest(&entry.path()).ok()?;
            Some((manifest.created_at, entry.path()))
        })
        .collect::<Vec<_>>();
    checkpoints.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, path) in checkpoints.into_iter().skip(CHECKPOINT_LIMIT) {
        let _ = std::fs::remove_dir_all(path);
    }
}

async fn changed_paths(
    runner: &dyn WorkspaceCommandExecutor,
    repo_root: &Path,
    head: &str,
) -> Result<BTreeSet<PathBuf>, String> {
    let tracked = git_stdout(
        runner,
        repo_root,
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--submodule=short",
            "--ignore-submodules=dirty",
            head,
            "--",
        ],
    )
    .await?;
    let untracked = git_stdout(
        runner,
        repo_root,
        &["ls-files", "--others", "--exclude-standard", "-z", "--"],
    )
    .await?;
    parse_nul_paths(&format!("{tracked}{untracked}"))
}

async fn head_paths(
    runner: &dyn WorkspaceCommandExecutor,
    repo_root: &Path,
) -> Result<BTreeSet<PathBuf>, String> {
    let paths = git_stdout(
        runner,
        repo_root,
        &["ls-tree", "-r", "-z", "--name-only", "HEAD", "--"],
    )
    .await?;
    parse_nul_paths(&paths)
}

fn parse_nul_paths(output: &str) -> Result<BTreeSet<PathBuf>, String> {
    let mut paths = BTreeSet::new();
    for value in output.split('\0').filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        validate_relative_path(&path)?;
        paths.insert(path);
    }
    Ok(paths)
}

fn sanitize_label(label: String) -> String {
    let label = label
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .chars()
        .filter(|character| !character.is_control())
        .take(/*max_chars*/ 80)
        .collect::<String>();
    if label.is_empty() {
        ui_text("Checkpoint", "检查点").to_string()
    } else {
        label
    }
}

async fn git_repo_root(
    runner: &dyn WorkspaceCommandExecutor,
    cwd: &Path,
) -> Result<Option<PathBuf>, String> {
    let output = run_git(
        runner,
        cwd,
        &["rev-parse", "--is-inside-work-tree", "--show-toplevel"],
    )
    .await?;
    if !output.success()
        || !output
            .stdout
            .lines()
            .next()
            .is_some_and(|line| line == "true")
    {
        return Ok(None);
    }
    let Some(root) = output.stdout.lines().nth(/*index*/ 1) else {
        return Ok(None);
    };
    let root = PathBuf::from(root);
    let root =
        dunce::canonicalize(&root).map_err(|error| format!("无法解析 Git 根目录：{error}"))?;
    Ok(Some(root))
}

async fn canonicalize_repo_root(
    runner: &dyn WorkspaceCommandExecutor,
    cwd: &Path,
) -> Result<PathBuf, String> {
    git_repo_root(runner, cwd)
        .await?
        .ok_or_else(|| "检查点所在工作区已不是 Git 仓库".to_string())
}

async fn git_stdout(
    runner: &dyn WorkspaceCommandExecutor,
    cwd: &Path,
    args: &[&str],
) -> Result<String, String> {
    let output = run_git(runner, cwd, args).await?;
    if output.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "Git 命令 {:?} 退出码 {}：{}",
            args,
            output.exit_code,
            output.stderr.trim()
        ))
    }
}

async fn git_ok(
    runner: &dyn WorkspaceCommandExecutor,
    cwd: &Path,
    args: &[String],
) -> Result<(), String> {
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = run_git(runner, cwd, &refs).await?;
    if output.success() {
        Ok(())
    } else {
        Err(format!(
            "Git 命令 {:?} 退出码 {}：{}",
            args,
            output.exit_code,
            output.stderr.trim()
        ))
    }
}

async fn run_git(
    runner: &dyn WorkspaceCommandExecutor,
    cwd: &Path,
    args: &[&str],
) -> Result<crate::workspace_command::WorkspaceCommandOutput, String> {
    let argv = [
        "git",
        "-c",
        SAFE_BARE_REPOSITORY_CONFIG,
        "-c",
        DISABLE_HOOKS_CONFIG,
    ]
    .into_iter()
    .chain(args.iter().copied());
    let output = runner
        .run(
            WorkspaceCommand::new(argv, cwd.to_path_buf())
                .timeout(GIT_TIMEOUT)
                .disable_output_cap(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(output)
}

fn checkpoints_root(codex_home: &Path, repo_root: &Path) -> Result<PathBuf, String> {
    let mut hasher = Sha256::new();
    hasher.update(repo_root.to_string_lossy().as_bytes());
    let repo_key = format!("{:x}", hasher.finalize());
    Ok(codex_home.join("rewind").join(&repo_key[..16]))
}

fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("检查点路径无效：{}", path.display()));
    }
    Ok(())
}

fn safe_path(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    validate_relative_path(relative)?;
    let mut current = root.to_path_buf();
    let components = relative.components().collect::<Vec<_>>();
    for component in &components {
        let Component::Normal(part) = component else {
            return Err("检查点路径无效".to_string());
        };
        current.push(part);
        if current == root.join(relative) {
            break;
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&current)
            && metadata.file_type().is_symlink()
        {
            return Err(format!("拒绝通过符号链接恢复：{}", current.display()));
        }
    }
    Ok(root.join(relative))
}

fn metadata_if_exists(path: &Path) -> Result<Option<std::fs::Metadata>, String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("无法检查 {}：{error}", path.display())),
    }
}

fn read_file_if_exists(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let Some(metadata) = metadata_if_exists(path)? else {
        return Ok(None);
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("无法预览非普通文件：{}", path.display()));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Ok(Some(
            format!("[当前文件过大，未读取预览：{} 字节]\n", metadata.len()).into_bytes(),
        ));
    }
    std::fs::read(path)
        .map(Some)
        .map_err(|error| format!("无法读取 {}：{error}", path.display()))
}

fn write_file_atomically(
    root: &Path,
    path: &Path,
    bytes: &[u8],
    mode: Option<u32>,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("恢复路径无效：{}", path.display()))?;
    ensure_safe_parent(root, parent)?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("无法创建 {}：{error}", parent.display()))?;
    if path.exists() {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|error| format!("无法检查 {}：{error}", path.display()))?;
        if metadata.is_dir() {
            return Err(format!("拒绝用文件覆盖目录：{}", path.display()));
        }
        std::fs::remove_file(path)
            .map_err(|error| format!("无法替换 {}：{error}", path.display()))?;
    }
    let mut temp =
        NamedTempFile::new_in(parent).map_err(|error| format!("无法创建临时文件：{error}"))?;
    temp.write_all(bytes)
        .map_err(|error| format!("无法写入 {}：{error}", path.display()))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| format!("无法刷新 {}：{error}", path.display()))?;
    temp.persist(path)
        .map_err(|error| format!("无法安装 {}：{}", path.display(), error.error))?;
    set_file_mode(path, mode)?;
    Ok(())
}

fn ensure_safe_parent(root: &Path, parent: &Path) -> Result<(), String> {
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| "恢复路径超出工作区".to_string())?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err("恢复路径超出工作区".to_string());
        };
        current.push(part);
        if let Ok(metadata) = std::fs::symlink_metadata(&current)
            && metadata.file_type().is_symlink()
        {
            return Err(format!("拒绝通过符号链接恢复：{}", current.display()));
        }
    }
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            Err(format!("恢复文件时拒绝删除目录：{}", path.display()))
        }
        Ok(_) => std::fs::remove_file(path)
            .map_err(|error| format!("无法删除 {}：{error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法检查 {}：{error}", path.display())),
    }
}

fn prune_empty_parents(root: &Path, path: &Path) {
    let mut current = path.parent();
    while let Some(parent) = current {
        if parent == root || std::fs::remove_dir(parent).is_err() {
            break;
        }
        current = parent.parent();
    }
}

#[cfg(unix)]
fn file_mode(metadata: &std::fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(metadata.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn file_mode(_metadata: &std::fs::Metadata) -> Option<u32> {
    None
}

#[cfg(unix)]
fn set_file_mode(path: &Path, mode: Option<u32>) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(mode) = mode {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|error| format!("无法设置 {} 的权限：{error}", path.display()))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_file_mode(_path: &Path, _mode: Option<u32>) -> Result<(), String> {
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RewindFocus {
    Timeline,
    Files,
    View,
}

pub(crate) struct RewindOverlay {
    checkpoints: Vec<CheckpointSummary>,
    selected: usize,
    file_selected: usize,
    scroll: usize,
    focus: RewindFocus,
    preview: Option<DiffReviewData>,
    preview_error: Option<String>,
    loading_checkpoint: Option<String>,
    request_id: u64,
    pending_restore: Option<RewindRestoreTarget>,
    done: bool,
    app_event_tx: AppEventSender,
    keymap: PagerKeymap,
}

impl RewindOverlay {
    pub(crate) fn new(
        checkpoints: Vec<CheckpointSummary>,
        app_event_tx: AppEventSender,
        keymap: PagerKeymap,
    ) -> Self {
        let mut overlay = Self {
            checkpoints,
            selected: 0,
            file_selected: 0,
            scroll: 0,
            focus: RewindFocus::Timeline,
            preview: None,
            preview_error: None,
            loading_checkpoint: None,
            request_id: 0,
            pending_restore: None,
            done: false,
            app_event_tx,
            keymap,
        };
        overlay.request_preview();
        overlay
    }

    pub(crate) fn accept_preview(
        &mut self,
        request_id: u64,
        checkpoint_id: &str,
        result: Result<DiffReviewData, String>,
    ) {
        if request_id != self.request_id
            || self
                .selected_checkpoint()
                .map(|checkpoint| checkpoint.id.as_str())
                != Some(checkpoint_id)
        {
            return;
        }
        self.loading_checkpoint = None;
        match result {
            Ok(preview) => {
                self.preview = Some(preview);
                self.preview_error = None;
                self.file_selected = 0;
                self.scroll = 0;
            }
            Err(error) => {
                self.preview = None;
                self.preview_error = Some(error);
            }
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
        let (added, removed) = self
            .preview
            .as_ref()
            .map(DiffReviewData::added_removed)
            .unwrap_or((0, 0));
        Line::from(vec![
            ui_text("Rewind", "回到检查点").bold(),
            format!(
                "  {} {}",
                self.checkpoints.len(),
                ui_text("checkpoints", "个检查点")
            )
            .dim(),
            format!("  +{added} -{removed}").dim(),
        ])
        .render(header_area, buf);

        let timeline_width = body_area
            .width
            .saturating_mul(/*percent*/ 30)
            .max(/*min*/ 26)
            .min(/*max*/ 42);
        let [timeline_area, preview_area] = Layout::horizontal([
            Constraint::Length(timeline_width.min(body_area.width)),
            Constraint::Min(1),
        ])
        .areas(body_area);
        self.render_timeline(timeline_area, buf);
        if let Some(preview) = &self.preview {
            render_diff_preview(
                preview,
                preview_area,
                buf,
                self.file_selected,
                self.scroll,
                self.focus == RewindFocus::Files,
                self.focus == RewindFocus::View,
            );
        } else {
            let message = if let Some(error) = &self.preview_error {
                error.clone()
            } else if self.loading_checkpoint.is_some() {
                ui_text("Loading checkpoint preview...", "正在加载检查点预览...").to_string()
            } else {
                ui_text("Select a checkpoint to preview.", "选择检查点以预览。").to_string()
            };
            Paragraph::new(message.dim())
                .block(panel(ui_text("Preview", "预览")))
                .render(preview_area, buf);
        }
        self.render_footer(footer_area, buf);
    }

    fn render_timeline(&self, area: Rect, buf: &mut Buffer) {
        let block = if self.focus == RewindFocus::Timeline {
            focused_panel(ui_text("Timeline", "时间线"))
        } else {
            panel(ui_text("Timeline", "时间线"))
        };
        let inner = block.inner(area);
        block.render(area, buf);
        let height = usize::from(inner.height);
        if height == 0 {
            return;
        }
        let total_rows = self.checkpoints.len().saturating_mul(/*rows per item*/ 2);
        let selected_row = self.selected.saturating_mul(/*rows per item*/ 2);
        let start = selected_row
            .saturating_sub(height / 2)
            .min(total_rows.saturating_sub(height));
        let start_index = start / 2;
        for (index, checkpoint) in self.checkpoints.iter().enumerate().skip(start_index) {
            let row = index
                .saturating_mul(/*rows per item*/ 2)
                .saturating_sub(start);
            if row >= height {
                break;
            }
            let selected = index == self.selected;
            let marker = if selected { "●" } else { "○" };
            let marker_style = if selected {
                Style::default().fg(accent_color()).bold()
            } else {
                Style::default().dim()
            };
            let label_style = if checkpoint.safety {
                status_style(StatusTone::Attention)
            } else if selected {
                Style::default().bold()
            } else {
                Style::default()
            };
            let line = Line::from(vec![
                Span::styled(marker, marker_style),
                Span::raw(" "),
                Span::styled(checkpoint.label.clone(), label_style),
            ]);
            line.render(
                Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                buf,
            );
            if row + 1 >= height {
                continue;
            }
            let meta = format!(
                "  {}  ·  {} {}",
                checkpoint.display_time(),
                checkpoint.file_count,
                ui_text("files", "个文件")
            );
            Line::from(meta).dim().render(
                Rect::new(inner.x, inner.y + row as u16 + 1, inner.width, 1),
                buf,
            );
        }
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        let first = if let Some(target) = self.pending_restore {
            let action = match target {
                RewindRestoreTarget::Files => ui_text("restore files", "恢复文件"),
                RewindRestoreTarget::Conversation => ui_text("restore conversation", "回退对话"),
                RewindRestoreTarget::Both => ui_text("restore both", "恢复文件和对话"),
            };
            format!(
                "{} · y {} · n/Esc {}",
                action,
                ui_text("confirm", "确认"),
                ui_text("cancel", "取消"),
            )
        } else {
            ui_text(
                "A safety checkpoint is created before every restore.",
                "每次恢复前都会自动创建安全点。",
            )
            .to_string()
        };
        Line::from(first).render(Rect::new(area.x, area.y, area.width, 1), buf);
        let hints = format!(
            "Tab {} · ↑/↓ {} · PgUp/PgDn {} · f {} · c {} · b {} · u {} · Esc {}",
            ui_text("switch pane", "切换区域"),
            ui_text("select", "选择"),
            ui_text("scroll", "滚动"),
            ui_text("files", "文件"),
            ui_text("conversation", "对话"),
            ui_text("both", "两者"),
            ui_text("undo restore", "撤销恢复"),
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
            TuiEvent::Key(key) if key.kind == KeyEventKind::Press => {
                self.handle_key(key);
            }
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
        if let Some(target) = self.pending_restore {
            match key.code {
                KeyCode::Char('y') => self.request_restore(target),
                KeyCode::Char('n') | KeyCode::Esc => self.pending_restore = None,
                _ => {}
            }
            return;
        }
        if self.keymap.close.is_pressed(key) || key.code == KeyCode::Esc {
            self.done = true;
        } else if key.code == KeyCode::Tab && key.modifiers.is_empty() {
            self.cycle_focus(false);
        } else if key.code == KeyCode::BackTab {
            self.cycle_focus(true);
        } else if self.keymap.scroll_up.is_pressed(key) || key.code == KeyCode::Up {
            self.move_focus(-1);
        } else if self.keymap.scroll_down.is_pressed(key) || key.code == KeyCode::Down {
            self.move_focus(1);
        } else if self.keymap.page_up.is_pressed(key) {
            self.scroll_by(-10);
        } else if self.keymap.page_down.is_pressed(key) {
            self.scroll_by(10);
        } else if key.modifiers.is_empty() && key.code == KeyCode::Char('f') {
            self.pending_restore = Some(RewindRestoreTarget::Files);
        } else if key.modifiers.is_empty() && key.code == KeyCode::Char('c') {
            if self
                .selected_checkpoint()
                .is_some_and(CheckpointSummary::has_conversation)
            {
                self.pending_restore = Some(RewindRestoreTarget::Conversation);
            }
        } else if key.modifiers.is_empty() && key.code == KeyCode::Char('b') {
            if self
                .selected_checkpoint()
                .is_some_and(CheckpointSummary::has_conversation)
            {
                self.pending_restore = Some(RewindRestoreTarget::Both);
            }
        } else if key.modifiers.is_empty() && key.code == KeyCode::Char('u') {
            if let Some(index) = self
                .checkpoints
                .iter()
                .position(|checkpoint| checkpoint.safety)
            {
                self.selected = index;
                self.preview = None;
                self.preview_error = None;
                self.file_selected = 0;
                self.scroll = 0;
                self.pending_restore = Some(RewindRestoreTarget::Files);
                self.request_preview();
            }
        }
    }

    fn handle_mouse(&mut self, kind: MouseEventKind) {
        match kind {
            MouseEventKind::ScrollUp => self.move_focus(-3),
            MouseEventKind::ScrollDown => self.move_focus(3),
            _ => {}
        }
    }

    fn cycle_focus(&mut self, reverse: bool) {
        let has_files = self
            .preview
            .as_ref()
            .is_some_and(|preview| !preview.is_empty());
        self.focus = match (self.focus, reverse, has_files) {
            (RewindFocus::Timeline, false, true) => RewindFocus::Files,
            (RewindFocus::Timeline, false, false) => RewindFocus::Timeline,
            (RewindFocus::Files, false, _) => RewindFocus::View,
            (RewindFocus::View, false, _) => RewindFocus::Timeline,
            (RewindFocus::Timeline, true, true) => RewindFocus::View,
            (RewindFocus::Timeline, true, false) => RewindFocus::Timeline,
            (RewindFocus::Files, true, _) => RewindFocus::Timeline,
            (RewindFocus::View, true, _) => RewindFocus::Files,
        };
    }

    fn move_focus(&mut self, delta: isize) {
        match self.focus {
            RewindFocus::Timeline => {
                if self.checkpoints.is_empty() {
                    return;
                }
                let next = self
                    .selected
                    .saturating_add_signed(delta)
                    .min(self.checkpoints.len().saturating_sub(1));
                if next != self.selected {
                    self.selected = next;
                    self.preview = None;
                    self.preview_error = None;
                    self.file_selected = 0;
                    self.scroll = 0;
                    self.request_preview();
                }
            }
            RewindFocus::Files => {
                let file_count = self.preview.as_ref().map_or(0, DiffReviewData::file_count);
                self.file_selected = self
                    .file_selected
                    .saturating_add_signed(delta)
                    .min(file_count.saturating_sub(1));
                self.scroll = 0;
            }
            RewindFocus::View => self.scroll_by(delta),
        }
    }

    fn scroll_by(&mut self, delta: isize) {
        self.scroll = self.scroll.saturating_add_signed(delta);
    }

    fn request_preview(&mut self) {
        let Some(checkpoint) = self.selected_checkpoint().cloned() else {
            self.loading_checkpoint = None;
            return;
        };
        self.request_id = self.request_id.wrapping_add(1);
        self.loading_checkpoint = Some(checkpoint.id.clone());
        self.app_event_tx.send(AppEvent::LoadRewindPreview {
            request_id: self.request_id,
            checkpoint,
        });
    }

    fn request_restore(&mut self, target: RewindRestoreTarget) {
        let Some(checkpoint) = self.selected_checkpoint().cloned() else {
            self.pending_restore = None;
            return;
        };
        self.app_event_tx
            .send(AppEvent::RestoreRewindCheckpoint { checkpoint, target });
        self.done = true;
    }

    fn selected_checkpoint(&self) -> Option<&CheckpointSummary> {
        self.checkpoints.get(self.selected)
    }

    pub(crate) fn is_done(&self) -> bool {
        self.done
    }
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
