//! Small, dependency-free localization layer for user-facing TUI text.

use codex_config::types::TuiLanguage;
use std::cell::Cell;

thread_local! {
    static CURRENT_LANGUAGE: Cell<TuiLanguage> = const { Cell::new(TuiLanguage::Auto) };
}

pub(crate) fn set_current_language(language: TuiLanguage) {
    CURRENT_LANGUAGE.set(language);
}

pub(crate) fn current_language() -> TuiLanguage {
    CURRENT_LANGUAGE.get()
}

/// Select user-interface text for the language active on the rendering thread.
pub(crate) fn ui_text(english: &'static str, chinese: &'static str) -> &'static str {
    text(current_language(), english, chinese)
}

/// Resolve `auto` from the conventional POSIX locale variables.
pub(crate) fn effective_language(language: TuiLanguage) -> TuiLanguage {
    language.effective()
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

/// Localize experimental-feature metadata that originates from the app server.
pub(crate) fn experimental_feature(
    language: TuiLanguage,
    key: &str,
) -> Option<(&'static str, &'static str)> {
    if !is_chinese(language) {
        return None;
    }
    match key {
        "analytics_plan_history" => Some((
            "套餐用量历史",
            "在 /analytics 中预览个人账户的 5 小时与每周额度记录。",
        )),
        "login_shell_package_path" => Some((
            "登录 Shell 的内置工具",
            "登录 Shell 重置 PATH 后，仍保留 ripgrep 等内置工具。",
        )),
        "network_proxy" => Some((
            "网络代理",
            "对已启用网络的沙箱会话应用网络代理限制。",
        )),
        "prevent_idle_sleep" => Some((
            "运行期间防止休眠",
            "Codex 运行任务时让电脑保持唤醒。",
        )),
        _ => None,
    }
}

/// Translate known experimental-feature diagnostics while retaining server detail.
pub(crate) fn experimental_diagnostic(language: TuiLanguage, message: &str) -> String {
    if !is_chinese(language) {
        return message.to_string();
    }
    let translated = match message {
        "Changes were saved, but the configured values differ from your selections. A higher-priority setting may override them." => {
            "更改已保存，但配置值与你选择的状态不一致，可能有优先级更高的设置覆盖了它。"
        }
        "Saving experimental features timed out; the write may still finish. Reopen /experimental to check." => {
            "保存实验功能超时；写入可能仍会完成。重新打开 /experimental 检查。"
        }
        "Failed to save experimental features. Reopen /experimental to check configured values before retrying." => {
            "保存实验功能失败。重新打开 /experimental 检查已配置的值后再重试。"
        }
        "Feature discovery was interrupted" => "实验功能读取被中断",
        "Features were saved, but readback was interrupted" => {
            "实验功能已保存，但重新读取配置被中断"
        }
        "Experimental feature request failed" => "实验功能请求失败",
        "Experimental feature page exceeds requested limit" => "实验功能分页数据超过请求上限",
        "Experimental feature pagination repeated a cursor" => "实验功能分页返回了重复游标",
        "Experimental feature discovery exceeded 10 pages" => "实验功能加载超过 10 页",
        "Experimental feature discovery timed out" => "加载实验功能超时",
        _ => {
            if let Some(name) =
                message.strip_prefix("The server did not advertise experimental feature `")
                && let Some(name) = name.strip_suffix('`')
            {
                return format!("服务端未提供实验功能 `{name}`");
            }
            if let Some(detail) =
                message.strip_prefix("Features were saved, but configured values could not be refreshed: ")
            {
                return format!("实验功能已保存，但无法刷新配置：{detail}");
            }
            return format!("实验功能操作失败：{message}");
        }
    };
    translated.to_string()
}
