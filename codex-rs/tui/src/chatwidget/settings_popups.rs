//! Settings-adjacent popup surfaces for `ChatWidget`.
//!
//! This keeps theme and experimental-feature UI out of the main
//! orchestration module without changing their event wiring.

use super::*;
use codex_config::types::TuiLanguage;

impl ChatWidget {
    pub(super) fn open_language_picker(&mut self) {
        let current = self.local_settings.tui.language;
        let effective = crate::i18n::effective_language(current);
        let current_label = match current {
            TuiLanguage::Auto => crate::i18n::text(effective, "Auto", "自动"),
            TuiLanguage::Chinese => "简体中文",
            TuiLanguage::English => "English",
        };
        let effective_label = match effective {
            TuiLanguage::Chinese => "简体中文",
            TuiLanguage::English | TuiLanguage::Auto => "English",
        };
        let title = crate::i18n::text(effective, "Select Interface Language", "选择界面语言");
        let subtitle = if crate::i18n::is_chinese(effective) {
            format!(
                "当前设置：{current_label} · 实际生效：{effective_label}。切换后立即生效并保存到后续会话。"
            )
        } else {
            format!(
                "Current setting: {current_label} · Effective: {effective_label}. Changes apply immediately and are saved for future sessions."
            )
        };
        let auto_description =
            crate::i18n::text(effective, "Follow the system locale", "跟随系统语言环境");
        let chinese_description =
            crate::i18n::text(effective, "Use Simplified Chinese", "使用简体中文");
        let english_description = crate::i18n::text(effective, "Use English", "使用英文");
        let items = vec![
            SelectionItem {
                name: crate::i18n::text(effective, "Auto", "自动").to_string(),
                description: Some(auto_description.to_string()),
                is_current: current == TuiLanguage::Auto,
                dismiss_on_select: true,
                actions: vec![Box::new(|tx| {
                    tx.send(AppEvent::LanguageSelected {
                        language: TuiLanguage::Auto,
                    });
                })],
                ..Default::default()
            },
            SelectionItem {
                name: "简体中文".to_string(),
                description: Some(chinese_description.to_string()),
                is_current: current == TuiLanguage::Chinese,
                dismiss_on_select: true,
                actions: vec![Box::new(|tx| {
                    tx.send(AppEvent::LanguageSelected {
                        language: TuiLanguage::Chinese,
                    });
                })],
                ..Default::default()
            },
            SelectionItem {
                name: "English".to_string(),
                description: Some(english_description.to_string()),
                is_current: current == TuiLanguage::English,
                dismiss_on_select: true,
                actions: vec![Box::new(|tx| {
                    tx.send(AppEvent::LanguageSelected {
                        language: TuiLanguage::English,
                    });
                })],
                ..Default::default()
            },
        ];
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some(title.to_string()),
            subtitle: Some(subtitle),
            items,
            initial_selected_idx: Some(match current {
                TuiLanguage::Auto => 0,
                TuiLanguage::Chinese => 1,
                TuiLanguage::English => 2,
            }),
            footer_hint: Some(Line::from(crate::i18n::text(
                effective,
                "↑/↓ select · Enter confirm · Esc cancel",
                "↑/↓ 选择 · Enter 确认 · Esc 取消",
            ))),
            ..SelectionViewParams::picker()
        });
    }

    pub(super) fn handle_language_command_args(&mut self, args: &str) {
        let language = match args.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(TuiLanguage::Auto),
            "zh" | "zh-cn" | "zh_cn" | "chinese" | "中文" => Some(TuiLanguage::Chinese),
            "en" | "english" => Some(TuiLanguage::English),
            _ => None,
        };
        if let Some(language) = language {
            self.app_event_tx
                .send(AppEvent::LanguageSelected { language });
        } else {
            self.add_error_message(
                crate::i18n::text(
                    self.local_settings.tui.language,
                    "Usage: /language [auto|zh-CN|en]",
                    "用法：/language [auto|zh-CN|en]",
                )
                .to_string(),
            );
        }
    }

    pub(super) fn open_theme_picker(&mut self) {
        let codex_home = codex_utils_home_dir::find_codex_home().ok();
        let params = crate::theme_picker::build_theme_picker_params_for_language(
            self.local_settings.tui.theme.as_deref(),
            codex_home.as_deref(),
            self.last_rendered_width.get(),
            self.local_settings.tui.language,
        );
        self.bottom_pane.show_selection_view(params);
    }

    pub(crate) fn open_experimental_popup(&mut self) {
        let Some(thread_id) = self.thread_id() else {
            self.add_info_message(
                crate::i18n::text(
                    self.local_settings.tui.language,
                    "Experimental features are unavailable until startup completes.",
                    "启动完成前无法使用实验功能。",
                )
                .to_string(),
                /*hint*/ None,
            );
            return;
        };
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.app_event_tx.send(AppEvent::FetchExperimentalFeatures {
            thread_id,
            response_tx,
        });
        let view = ExperimentalFeaturesView::new(
            Vec::new(),
            thread_id,
            Some(response_rx),
            self.app_event_tx.clone(),
            self.bottom_pane.list_keymap(),
            self.local_settings.tui.language,
        );
        self.bottom_pane.show_view(Box::new(view));
    }
}
