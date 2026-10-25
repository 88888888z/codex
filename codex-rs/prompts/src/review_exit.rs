use codex_utils_template::Template;
use crate::PromptLanguage;
use std::borrow::Cow;
use std::sync::LazyLock;

const REVIEW_EXIT_SUCCESS_TEMPLATE_TEXT: &str =
    include_str!("../templates/review/exit_success.xml");
const REVIEW_EXIT_INTERRUPTED_TEMPLATE_TEXT: &str =
    include_str!("../templates/review/exit_interrupted.xml");
const REVIEW_EXIT_SUCCESS_TEMPLATE_TEXT_ZH_CN: &str =
    include_str!("../templates/review/exit_success.zh-CN.xml");
const REVIEW_EXIT_INTERRUPTED_TEMPLATE_TEXT_ZH_CN: &str =
    include_str!("../templates/review/exit_interrupted.zh-CN.xml");

static REVIEW_EXIT_SUCCESS_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
    let normalized = normalize_review_template_line_endings(REVIEW_EXIT_SUCCESS_TEMPLATE_TEXT);
    Template::parse(normalized.as_ref())
        .unwrap_or_else(|err| panic!("review exit success template must parse: {err}"))
});
static REVIEW_EXIT_SUCCESS_TEMPLATE_ZH_CN: LazyLock<Template> = LazyLock::new(|| {
    let normalized = normalize_review_template_line_endings(
        REVIEW_EXIT_SUCCESS_TEMPLATE_TEXT_ZH_CN,
    );
    Template::parse(normalized.as_ref())
        .unwrap_or_else(|err| panic!("Chinese review exit success template must parse: {err}"))
});

pub fn render_review_exit_success(results: &str) -> String {
    render_review_exit_success_with_language(results, PromptLanguage::English)
}

pub fn render_review_exit_success_with_language(
    results: &str,
    language: PromptLanguage,
) -> String {
    let template = match language {
        PromptLanguage::English => &REVIEW_EXIT_SUCCESS_TEMPLATE,
        PromptLanguage::Chinese => &REVIEW_EXIT_SUCCESS_TEMPLATE_ZH_CN,
    };
    template
        .render([("results", results)])
        .unwrap_or_else(|err| panic!("review exit success template must render: {err}"))
}

pub fn render_review_exit_interrupted() -> String {
    render_review_exit_interrupted_with_language(PromptLanguage::English)
}

pub fn render_review_exit_interrupted_with_language(language: PromptLanguage) -> String {
    normalize_review_template_line_endings(match language {
        PromptLanguage::English => REVIEW_EXIT_INTERRUPTED_TEMPLATE_TEXT,
        PromptLanguage::Chinese => REVIEW_EXIT_INTERRUPTED_TEMPLATE_TEXT_ZH_CN,
    })
    .into_owned()
}

fn normalize_review_template_line_endings(template: &str) -> Cow<'_, str> {
    if template.contains('\r') {
        Cow::Owned(template.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(template)
    }
}

#[cfg(test)]
#[path = "review_exit_tests.rs"]
mod review_exit_tests;
