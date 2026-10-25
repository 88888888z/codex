use codex_git_utils::merge_base_with_head;
use codex_protocol::protocol::ReviewRequest;
use codex_protocol::protocol::ReviewTarget;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_template::Template;
use crate::PromptLanguage;
use std::sync::LazyLock;

/// Review thread system prompt.
pub const REVIEW_PROMPT: &str = include_str!("../templates/review/rubric.md");
pub const REVIEW_PROMPT_ZH_CN: &str = include_str!("../templates/review/rubric.zh-CN.md");

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedReviewRequest {
    pub target: ReviewTarget,
    pub prompt: String,
    pub user_facing_hint: String,
}

const UNCOMMITTED_PROMPT: &str = "Review the current code changes (staged, unstaged, and untracked files) and provide prioritized findings.";
const UNCOMMITTED_PROMPT_ZH_CN: &str = "审查当前代码变更（已暂存、未暂存和未跟踪文件），按优先级输出问题。";

const BASE_BRANCH_PROMPT_BACKUP: &str = "Review the code changes against the base branch '{{branch}}'. Start by finding the merge diff between the current branch and {{branch}}'s upstream e.g. (`git merge-base HEAD \"$(git rev-parse --abbrev-ref \"{{branch}}@{upstream}\")\"`), then run `git diff` against that SHA to see what changes we would merge into the {{branch}} branch. Provide prioritized, actionable findings.";
const BASE_BRANCH_PROMPT: &str = "Review the code changes against the base branch '{{base_branch}}'. The merge base commit for this comparison is {{merge_base_sha}}. Run `git diff {{merge_base_sha}}` to inspect the changes relative to {{base_branch}}. Provide prioritized, actionable findings.";
const BASE_BRANCH_PROMPT_BACKUP_ZH_CN: &str = "审查当前分支相对基础分支 '{{branch}}' 的代码变更。先找到当前分支与 {{branch}} 上游的 merge diff，例如运行 `git merge-base HEAD \"$(git rev-parse --abbrev-ref \"{{branch}}@{upstream}\")\"`，再对该 SHA 运行 `git diff`。按优先级输出可操作问题。";
const BASE_BRANCH_PROMPT_ZH_CN: &str = "审查当前分支相对基础分支 '{{base_branch}}' 的代码变更。本次比较的 merge base 为 {{merge_base_sha}}。运行 `git diff {{merge_base_sha}}` 查看相对 {{base_branch}} 的变更，并按优先级输出可操作问题。";
static BASE_BRANCH_PROMPT_BACKUP_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
    Template::parse(BASE_BRANCH_PROMPT_BACKUP)
        .unwrap_or_else(|err| panic!("base branch backup review prompt must parse: {err}"))
});
static BASE_BRANCH_PROMPT_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
    Template::parse(BASE_BRANCH_PROMPT)
        .unwrap_or_else(|err| panic!("base branch review prompt must parse: {err}"))
});
static BASE_BRANCH_PROMPT_BACKUP_ZH_CN_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
    Template::parse(BASE_BRANCH_PROMPT_BACKUP_ZH_CN)
        .unwrap_or_else(|err| panic!("Chinese base branch backup review prompt must parse: {err}"))
});
static BASE_BRANCH_PROMPT_ZH_CN_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
    Template::parse(BASE_BRANCH_PROMPT_ZH_CN)
        .unwrap_or_else(|err| panic!("Chinese base branch review prompt must parse: {err}"))
});

const COMMIT_PROMPT_WITH_TITLE: &str = "Review the code changes introduced by commit {{sha}} (\"{{title}}\"). Provide prioritized, actionable findings.";
const COMMIT_PROMPT: &str = "Review the code changes introduced by commit {{sha}}. Provide prioritized, actionable findings.";
const COMMIT_PROMPT_WITH_TITLE_ZH_CN: &str = "审查提交 {{sha}}（\"{{title}}\"）引入的代码变更，按优先级输出可操作问题。";
const COMMIT_PROMPT_ZH_CN: &str = "审查提交 {{sha}} 引入的代码变更，按优先级输出可操作问题。";
static COMMIT_PROMPT_WITH_TITLE_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
    Template::parse(COMMIT_PROMPT_WITH_TITLE)
        .unwrap_or_else(|err| panic!("commit review prompt with title must parse: {err}"))
});
static COMMIT_PROMPT_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
    Template::parse(COMMIT_PROMPT)
        .unwrap_or_else(|err| panic!("commit review prompt must parse: {err}"))
});
static COMMIT_PROMPT_WITH_TITLE_ZH_CN_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
    Template::parse(COMMIT_PROMPT_WITH_TITLE_ZH_CN)
        .unwrap_or_else(|err| panic!("Chinese commit review prompt with title must parse: {err}"))
});
static COMMIT_PROMPT_ZH_CN_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
    Template::parse(COMMIT_PROMPT_ZH_CN)
        .unwrap_or_else(|err| panic!("Chinese commit review prompt must parse: {err}"))
});

pub fn resolve_review_request(
    request: ReviewRequest,
    cwd: &AbsolutePathBuf,
) -> anyhow::Result<ResolvedReviewRequest> {
    resolve_review_request_with_language(request, cwd, PromptLanguage::English)
}

pub fn resolve_review_request_with_language(
    request: ReviewRequest,
    cwd: &AbsolutePathBuf,
    language: PromptLanguage,
) -> anyhow::Result<ResolvedReviewRequest> {
    let target = request.target;
    let prompt = review_prompt_with_language(&target, cwd, language)?;
    let user_facing_hint = request
        .user_facing_hint
        .unwrap_or_else(|| user_facing_hint(&target));

    Ok(ResolvedReviewRequest {
        target,
        prompt,
        user_facing_hint,
    })
}

pub fn review_prompt(target: &ReviewTarget, cwd: &AbsolutePathBuf) -> anyhow::Result<String> {
    review_prompt_with_language(target, cwd, PromptLanguage::English)
}

pub fn review_prompt_with_language(
    target: &ReviewTarget,
    cwd: &AbsolutePathBuf,
    language: PromptLanguage,
) -> anyhow::Result<String> {
    match target {
        ReviewTarget::UncommittedChanges => Ok(match language {
            PromptLanguage::English => UNCOMMITTED_PROMPT,
            PromptLanguage::Chinese => UNCOMMITTED_PROMPT_ZH_CN,
        }
        .to_string()),
        ReviewTarget::BaseBranch { branch } => {
            if let Some(commit) = merge_base_with_head(cwd, branch)? {
                Ok(render_review_prompt(
                    match language {
                        PromptLanguage::English => &BASE_BRANCH_PROMPT_TEMPLATE,
                        PromptLanguage::Chinese => &BASE_BRANCH_PROMPT_ZH_CN_TEMPLATE,
                    },
                    [
                        ("base_branch", branch.as_str()),
                        ("merge_base_sha", commit.as_str()),
                    ],
                ))
            } else {
                Ok(render_review_prompt(
                    match language {
                        PromptLanguage::English => &BASE_BRANCH_PROMPT_BACKUP_TEMPLATE,
                        PromptLanguage::Chinese => &BASE_BRANCH_PROMPT_BACKUP_ZH_CN_TEMPLATE,
                    },
                    [("branch", branch.as_str())],
                ))
            }
        }
        ReviewTarget::Commit { sha, title } => {
            if let Some(title) = title {
                Ok(render_review_prompt(
                    match language {
                        PromptLanguage::English => &COMMIT_PROMPT_WITH_TITLE_TEMPLATE,
                        PromptLanguage::Chinese => &COMMIT_PROMPT_WITH_TITLE_ZH_CN_TEMPLATE,
                    },
                    [("sha", sha.as_str()), ("title", title.as_str())],
                ))
            } else {
                Ok(render_review_prompt(
                    match language {
                        PromptLanguage::English => &COMMIT_PROMPT_TEMPLATE,
                        PromptLanguage::Chinese => &COMMIT_PROMPT_ZH_CN_TEMPLATE,
                    },
                    [("sha", sha.as_str())],
                ))
            }
        }
        ReviewTarget::Custom { instructions } => {
            let prompt = instructions.trim();
            if prompt.is_empty() {
                anyhow::bail!("Review prompt cannot be empty");
            }
            Ok(prompt.to_string())
        }
    }
}

fn render_review_prompt<'a, const N: usize>(
    template: &Template,
    variables: [(&'a str, &'a str); N],
) -> String {
    template
        .render(variables)
        .unwrap_or_else(|err| panic!("review prompt template must render: {err}"))
}

pub fn user_facing_hint(target: &ReviewTarget) -> String {
    match target {
        ReviewTarget::UncommittedChanges => "current changes".to_string(),
        ReviewTarget::BaseBranch { branch } => format!("changes against '{branch}'"),
        ReviewTarget::Commit { sha, title } => {
            let short_sha: String = sha.chars().take(7).collect();
            if let Some(title) = title {
                format!("commit {short_sha}: {title}")
            } else {
                format!("commit {short_sha}")
            }
        }
        ReviewTarget::Custom { instructions } => instructions.trim().to_string(),
    }
}

impl From<ResolvedReviewRequest> for ReviewRequest {
    fn from(resolved: ResolvedReviewRequest) -> Self {
        ReviewRequest {
            target: resolved.target,
            user_facing_hint: Some(resolved.user_facing_hint),
        }
    }
}

#[cfg(test)]
#[path = "review_request_tests.rs"]
mod review_request_tests;
