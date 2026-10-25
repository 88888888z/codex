//! Project verification discovery and bounded result reporting.

use std::path::Path;
use std::time::Duration;
use std::collections::HashSet;

const MAX_REPORT_CHARS: usize = 16_000;

#[derive(Clone, Debug)]
pub(crate) struct VerificationCommand {
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) argv: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct VerificationOutcome {
    pub(crate) label: String,
    pub(crate) command: String,
    pub(crate) success: bool,
    pub(crate) exit_code: Option<i32>,
    pub(crate) output: String,
    pub(crate) duration: Duration,
}

#[derive(Clone, Debug)]
pub(crate) struct VerificationReport {
    pub(crate) outcomes: Vec<VerificationOutcome>,
}

impl VerificationReport {
    pub(crate) fn success(&self) -> bool {
        !self.outcomes.is_empty() && self.outcomes.iter().all(|outcome| outcome.success)
    }

    pub(crate) fn render(&self) -> String {
        if self.outcomes.is_empty() {
            return crate::i18n::ui_text(
                "No verification commands were run.",
                "没有运行验证命令。",
            )
            .to_string();
        }
        let mut sections = Vec::new();
        for outcome in &self.outcomes {
            let mark = if outcome.success { "✓" } else { "✗" };
            let exit = outcome
                .exit_code
                .map_or_else(String::new, |code| {
                    if crate::i18n::is_chinese(crate::i18n::current_language()) {
                        format!(" · 退出码 {code}")
                    } else {
                        format!(" · exit {code}")
                    }
                });
            let mut section = format!(
                "{mark} {} · {} · {:.1}s{exit}",
                outcome.label,
                outcome.command,
                outcome.duration.as_secs_f64()
            );
            if !outcome.output.trim().is_empty() {
                section.push('\n');
                section.push_str(&bounded_output(&outcome.output));
            }
            sections.push(section);
        }
        let mut report = sections.join("\n\n");
        if report.chars().count() > MAX_REPORT_CHARS {
            report = report.chars().take(MAX_REPORT_CHARS).collect();
            report.push_str(crate::i18n::ui_text(
                "\n… output truncated",
                "\n… 输出已截断",
            ));
        }
        report
    }

    pub(crate) fn handoff_summary(&self) -> String {
        if self.outcomes.is_empty() {
            return crate::i18n::ui_text("No verification run yet.", "尚未运行验证。")
                .to_string();
        }
        self.outcomes
            .iter()
            .map(|outcome| {
                let mark = if outcome.success { "✓" } else { "✗" };
                format!("{mark} {} · {}", outcome.label, outcome.command)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub(crate) fn detect_commands(cwd: &Path) -> Vec<VerificationCommand> {
    let mut commands = Vec::new();
    if cwd.join("Cargo.toml").is_file() {
        commands.extend([
            command(
                "check",
                crate::i18n::ui_text("Rust check", "Rust 检查"),
                ["cargo", "check"],
            ),
            command(
                "test",
                crate::i18n::ui_text("Rust tests", "Rust 测试"),
                ["cargo", "test"],
            ),
            command(
                "fmt",
                crate::i18n::ui_text("Rust format", "Rust 格式"),
                ["cargo", "fmt", "--", "--check"],
            ),
            command(
                "clippy",
                crate::i18n::ui_text("Rust clippy", "Rust Clippy"),
                [
                    "cargo",
                    "clippy",
                    "--all-targets",
                    "--all-features",
                    "--",
                    "-D",
                    "warnings",
                ],
            ),
            command(
                "build",
                crate::i18n::ui_text("Rust build", "Rust 构建"),
                ["cargo", "build"],
            ),
        ]);
    }

    if let Some(scripts) = package_scripts(cwd) {
        let runner = if cwd.join("pnpm-lock.yaml").is_file() {
            "pnpm"
        } else if cwd.join("yarn.lock").is_file() {
            "yarn"
        } else if cwd.join("bun.lock").is_file() || cwd.join("bun.lockb").is_file() {
            "bun"
        } else {
            "npm"
        };
        for (script, label) in [
            (
                "test",
                crate::i18n::ui_text("JavaScript tests", "JavaScript 测试"),
            ),
            (
                "lint",
                crate::i18n::ui_text("JavaScript lint", "JavaScript 检查"),
            ),
            (
                "typecheck",
                crate::i18n::ui_text("JavaScript types", "JavaScript 类型"),
            ),
            (
                "build",
                crate::i18n::ui_text("JavaScript build", "JavaScript 构建"),
            ),
        ] {
            if scripts.iter().any(|candidate| candidate == script) {
                commands.push(command(script, label, [runner, "run", script]));
            }
        }
    }

    if cwd.join("go.mod").is_file() {
        commands.extend([
            command(
                "test",
                crate::i18n::ui_text("Go tests", "Go 测试"),
                ["go", "test", "./..."],
            ),
            command(
                "vet",
                crate::i18n::ui_text("Go vet", "Go 检查"),
                ["go", "vet", "./..."],
            ),
            command(
                "build",
                crate::i18n::ui_text("Go build", "Go 构建"),
                ["go", "build", "./..."],
            ),
        ]);
    }

    if let Some(contents) = read_text(&cwd.join("pyproject.toml")) {
        if contents.contains("pytest") || cwd.join("tests").is_dir() {
            commands.push(command(
                "test",
                crate::i18n::ui_text("Python tests", "Python 测试"),
                ["python", "-m", "pytest"],
            ));
        }
        if contents.contains("ruff") {
            commands.push(command(
                "lint",
                crate::i18n::ui_text("Python lint", "Python 检查"),
                ["ruff", "check", "."],
            ));
        }
    }

    if let Some(contents) = read_text(&cwd.join("Makefile")) {
        for (name, label) in [
            ("check", crate::i18n::ui_text("Make check", "Make 检查")),
            ("test", crate::i18n::ui_text("Make tests", "Make 测试")),
            ("lint", crate::i18n::ui_text("Make lint", "Make 检查")),
        ] {
            if contents.lines().any(|line| {
                line.strip_prefix(name)
                    .is_some_and(|rest| rest.starts_with(':'))
            }) {
                commands.push(command(name, label, ["make", name]));
            }
        }
    }

    let mut seen = HashSet::new();
    commands.retain(|command| seen.insert(command.name.clone()));
    commands
}

fn command<const N: usize>(
    name: &str,
    label: &str,
    argv: [&str; N],
) -> VerificationCommand {
    VerificationCommand {
        name: name.to_string(),
        label: label.to_string(),
        argv: argv.into_iter().map(str::to_string).collect(),
    }
}

fn package_scripts(cwd: &Path) -> Option<Vec<String>> {
    let contents = read_text(&cwd.join("package.json"))?;
    let package: serde_json::Value = serde_json::from_str(&contents).ok()?;
    Some(
        package
            .get("scripts")?
            .as_object()?
            .keys()
            .cloned()
            .collect(),
    )
}

fn read_text(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > 1_000_000 {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn bounded_output(output: &str) -> String {
    let lines = output.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(/*recent lines*/ 40);
    let mut bounded = lines[start..].join("\n");
    if bounded.chars().count() > 12_000 {
        bounded = bounded.chars().take(12_000).collect();
        bounded.push_str(crate::i18n::ui_text(
            "\n… output truncated",
            "\n… 输出已截断",
        ));
    }
    bounded
}
