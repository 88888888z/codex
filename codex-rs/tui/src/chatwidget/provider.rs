//! `/provider` picker and setup entry points.

use super::*;
use crate::bottom_pane::provider_setup::ProviderSetupView;
use crate::provider_profiles::ProfileSummary;
use crate::provider_profiles::ProviderPreset;
use crate::provider_profiles::list_profiles;

impl ChatWidget {
    pub(crate) fn open_provider_menu(&mut self) {
        let profiles = list_profiles(self.local_settings.codex_home.as_path());
        let profile_count = profiles.len();
        let selected_profile_id = self.local_settings.tui.provider_profile.as_deref();
        let matched_profile = selected_profile_id
            .and_then(|id| {
                profiles.iter().find(|profile| {
                    profile.id == id
                        && profile.provider == self.config.model_provider_id
                        && profile.model == self.current_model()
                })
            })
            .or_else(|| {
                profiles.iter().find(|profile| {
                    profile.provider == self.config.model_provider_id
                        && profile.model == self.current_model()
                })
            });
        let current_label = matched_profile
            .map(|profile| format!("{} · {}", profile.display_name, profile.model))
            .unwrap_or_else(|| {
                format!(
                    "{} · {}",
                    self.config.model_provider.name,
                    self.current_model()
                )
            });
        let current_profile_id = matched_profile.map(|profile| profile.id.clone());
        let mut items = profiles
            .into_iter()
            .map(|profile| {
                let id = profile.id.clone();
                let is_current = current_profile_id.as_deref() == Some(profile.id.as_str());
                SelectionItem {
                    name: format!("{}  ·  {}", profile.display_name, profile.model),
                    description: Some(format!("配置：{id}")),
                    is_current,
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::ActivateProviderProfile(profile.clone()));
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                }
            })
            .collect::<Vec<_>>();
        items.push(SelectionItem {
            name: "删除模型配置…".to_string(),
            description: Some("永久删除已保存的第三方模型配置".to_string()),
            actions: vec![Box::new(|tx| {
                tx.send(AppEvent::OpenProviderDeletePicker);
            })],
            dismiss_on_select: true,
            ..Default::default()
        });
        items.push(SelectionItem {
            name: "新增模型配置".to_string(),
            description: Some("添加第三方 API、本地模型或自定义 endpoint".to_string()),
            actions: vec![Box::new(|tx| {
                tx.send(AppEvent::OpenProviderPresetPicker);
            })],
            dismiss_on_select: true,
            ..Default::default()
        });
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("模型服务配置".to_string()),
            subtitle: Some(format!(
                "当前：{} · {} 个已保存配置 · {}",
                current_label,
                profile_count,
                self.local_settings.codex_home.display()
            )),
            items,
            footer_hint: Some(Line::from("↑/↓ 选择 · Enter 确认 · Esc 取消")),
            ..SelectionViewParams::picker()
        });
    }

    pub(crate) fn open_provider_delete_picker(&mut self) {
        let profiles = list_profiles(self.local_settings.codex_home.as_path());
        if profiles.is_empty() {
            self.add_info_message("没有可删除的模型配置。".to_string(), /*hint*/ None);
            return;
        }
        let current_profile_id = self.local_settings.tui.provider_profile.as_deref();
        let items = profiles
            .into_iter()
            .map(|profile| {
                let is_current =
                    current_profile_id.as_deref() == Some(profile.id.as_str());
                SelectionItem {
                    name: format!("{}  ·  {}", profile.display_name, profile.model),
                    description: Some(format!("配置：{}", profile.id)),
                    is_current,
                    is_disabled: is_current,
                    disabled_reason: is_current.then(|| {
                        "当前配置不能删除，请先切换到其他配置".to_string()
                    }),
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::ConfirmProviderProfileDelete(profile.clone()));
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                }
            })
            .collect();
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("删除模型配置".to_string()),
            subtitle: Some("选择要永久删除的配置。".to_string()),
            items,
            footer_hint: Some(Line::from("↑/↓ 选择 · Enter 下一步 · Esc 取消")),
            ..SelectionViewParams::picker()
        });
    }

    pub(crate) fn confirm_provider_profile_delete(
        &mut self,
        profile: crate::provider_profiles::ProfileSummary,
    ) {
        let profile_id = profile.id.clone();
        let label = format!("{} · {}", profile.display_name, profile.model);
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("删除模型配置？".to_string()),
            subtitle: Some(format!("将永久删除 `{profile_id}` 及其已保存的 API 密钥。")),
            items: vec![
                SelectionItem {
                    name: "取消".to_string(),
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: format!("删除 {label}"),
                    description: Some(profile_id),
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::DeleteProviderProfile(profile.clone()));
                    })],
                    require_explicit_confirmation: true,
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ],
            ..SelectionViewParams::confirmation()
        });
    }

    pub(crate) fn open_provider_preset_picker(&mut self) {
        let items = ProviderPreset::ALL
            .into_iter()
            .map(|preset| SelectionItem {
                name: preset.name().to_string(),
                description: Some(preset.description().to_string()),
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::OpenProviderSetup { preset });
                })],
                dismiss_on_select: true,
                ..Default::default()
            })
            .collect();
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("添加模型服务".to_string()),
            subtitle: Some("内置预设覆盖常见 OpenAI 兼容 API。".to_string()),
            items,
            footer_hint: Some(Line::from("↑/↓ 选择 · Enter 下一步 · Esc 取消")),
            ..SelectionViewParams::picker()
        });
    }

    pub(crate) fn open_provider_setup(&mut self, preset: ProviderPreset) {
        self.bottom_pane.show_view(Box::new(ProviderSetupView::new(
            preset,
            self.app_event_tx.clone(),
        )));
        self.request_redraw();
    }

    pub(super) fn handle_provider_command_args(&mut self, args: &str) {
        let args = args.trim();
        if args.is_empty() {
            self.open_provider_menu();
            return;
        }
        if args.eq_ignore_ascii_case("add") {
            self.open_provider_preset_picker();
            return;
        }
        let (action, profile_id) = args
            .split_once(char::is_whitespace)
            .map(|(action, id)| (action, id.trim()))
            .unwrap_or((args, ""));
        let profiles = list_profiles(self.local_settings.codex_home.as_path());
        let find_profile = |id: &str| profiles.iter().find(|profile| profile.id == id).cloned();
        match action.to_ascii_lowercase().as_str() {
            "delete" | "remove" if !profile_id.is_empty() => match find_profile(profile_id) {
                Some(profile) => self
                    .app_event_tx
                    .send(AppEvent::ConfirmProviderProfileDelete(profile)),
                None => self.add_error_message(format!("找不到模型配置 `{profile_id}`。")),
            },
            "use" | "select" if !profile_id.is_empty() => {
                self.activate_provider_profile(profile_id, &profiles);
            }
            _ if profile_id.is_empty() => {
                self.activate_provider_profile(args, &profiles);
            }
            _ => {
                self.add_error_message("用法：/provider [add|use <名称>|delete <名称>]".to_string())
            }
        }
    }

    fn activate_provider_profile(&mut self, profile_id: &str, profiles: &[ProfileSummary]) {
        let Some(profile) = profiles.iter().find(|profile| profile.id == profile_id) else {
            self.add_error_message(format!("找不到模型配置 `{profile_id}`。"));
            return;
        };
        self.app_event_tx
            .send(AppEvent::ActivateProviderProfile(profile.clone()));
    }
}
