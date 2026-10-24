//! Small, dependency-free localization layer for user-facing TUI text.

use codex_config::types::TuiLanguage;

/// Resolve `auto` from the conventional POSIX locale variables.
pub(crate) fn effective_language(language: TuiLanguage) -> TuiLanguage {
    match language {
        TuiLanguage::Auto => {
            let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
                .into_iter()
                .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()));
            match locale {
                Some(locale) if locale.to_ascii_lowercase().starts_with("zh") => {
                    TuiLanguage::Chinese
                }
                _ => TuiLanguage::English,
            }
        }
        explicit => explicit,
    }
}

pub(crate) fn is_chinese(language: TuiLanguage) -> bool {
    effective_language(language) == TuiLanguage::Chinese
}

/// Select one of two static strings using the effective interface language.
pub(crate) fn text(
    language: TuiLanguage,
    english: &'static str,
    chinese: &'static str,
) -> &'static str {
    if is_chinese(language) {
        chinese
    } else {
        english
    }
}

pub(crate) fn placeholder(language: TuiLanguage) -> &'static str {
    text(
        language,
        "Ask Codex to do anything",
        "让 Codex 处理任何任务",
    )
}

pub(crate) fn side_placeholder(language: TuiLanguage) -> &'static str {
    text(language, "Ask a follow-up question", "继续提问")
}
