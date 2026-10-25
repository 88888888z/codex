//! Render the configuration catalog and its child choice picker.

use super::ConfigPanel;
use super::ConfigPreference;
use super::ConfigSection;
use super::ConfigSetting;
use super::ConfigSettingValue;
use super::ConfigToggle;
use super::PickerState;
use super::ResumeCurrentAvailability;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::ColumnWidthMode;
use crate::bottom_pane::ListSelectionView;
use crate::bottom_pane::PickerSurface;
use crate::bottom_pane::SELECTION_TOGGLE_UNAVAILABLE_PREFIX;
use crate::bottom_pane::SearchMode;
use crate::bottom_pane::SelectionDescriptionLayout;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionRowDisplay;
use crate::bottom_pane::SelectionTab;
use crate::bottom_pane::SelectionToggle;
use crate::bottom_pane::SelectionToggleState;
use crate::bottom_pane::SelectionViewParams;
use crate::bottom_pane::footer_hint_items_line;
use crate::key_hint::KeyBinding;
use crate::keymap::ListAction;
use crate::keymap::ListKeymap;
use crate::render::renderable::Renderable;
use crate::wrapping::word_wrap_lines;
use codex_config::types::TuiLanguage;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

pub(crate) const EFFECTS_VIEW_ID: &str = "config-effects";

fn picker_hint_line(keymap: &ListKeymap, language: TuiLanguage) -> Line<'static> {
    let labels = [
        (ListAction::Accept, crate::i18n::text(language, "select", "选择")),
        (ListAction::Cancel, crate::i18n::text(language, "back", "返回")),
    ];
    let mut spans = Vec::new();
    for (action, label) in labels {
        if let Some(hint) = keymap.primary_hint(action) {
            if !spans.is_empty() {
                spans.push(" · ".dim());
            }
            spans.extend(hint.spans());
            spans.push(format!(" {label}").dim());
        }
    }
    Line::from(spans)
}

pub(crate) fn effects(tui: &codex_config::types::Tui, keymap: &ListKeymap) -> SelectionViewParams {
    effects_with_toggle_states(tui, keymap, &[])
}

pub(crate) fn effects_with_toggle_states(
    tui: &codex_config::types::Tui,
    keymap: &ListKeymap,
    toggle_states: &[Option<SelectionToggleState>],
) -> SelectionViewParams {
    let language = tui.language;
    let footer_hint = [
        (
            ListAction::Accept,
            crate::i18n::text(language, "toggle", "切换"),
        ),
        (ListAction::Cancel, crate::i18n::text(language, "back", "返回")),
    ]
        .into_iter()
        .filter_map(|(action, label)| {
            keymap
                .primary_hint(action)
                .map(|hint| (hint.display_label(), label.into()))
        })
        .collect::<Vec<_>>();
    let settings = [
        (
            crate::i18n::text(
                language,
                "Animations",
                "动画",
            ),
            crate::i18n::text(
                language,
                "Animate the welcome screen, working indicator, and transitions",
                "欢迎页、运行指示和界面过渡使用动画",
            ),
            ConfigToggle::Animations,
        ),
        (
            crate::i18n::text(language, "Composer stars", "输入框星光"),
            crate::i18n::text(
                language,
                "Show stars briefly for new Astra tasks",
                "开始新的 Astra 任务时短暂显示星光",
            ),
            ConfigToggle::ComposerStars,
        ),
    ];
    let animations_enabled = toggle_states
        .first()
        .copied()
        .flatten()
        .map(|state| state.is_on)
        .unwrap_or(tui.animations);
    SelectionViewParams {
        view_id: Some(EFFECTS_VIEW_ID),
        title: Some(crate::i18n::text(language, "Effects", "视觉效果").to_string()),
        subtitle: Some(
            crate::i18n::text(language, "Configure visual effects", "配置界面视觉效果")
                .to_string(),
        ),
        footer_hint: Some(footer_hint_items_line(&footer_hint)),
        allow_input_when_disconnected: true,
        items: settings
            .into_iter()
            .enumerate()
            .map(|(index, (label, description, setting))| {
                let toggle_state = toggle_states.get(index).copied().flatten();
                let enabled = toggle_state
                    .map(|state| state.is_on)
                    .unwrap_or_else(|| setting.enabled(tui));
                let available = setting != ConfigToggle::ComposerStars || animations_enabled;
                SelectionItem {
                    name: label.to_string(),
                    toggle: Some(SelectionToggle {
                        is_on: enabled,
                        action: Box::new(move |enabled, tx| {
                            tx.send(AppEvent::SaveConfigPreference(ConfigPreference::Toggle {
                                setting,
                                enabled,
                            }));
                        }),
                    }),
                    pending_toggle_actions: toggle_state
                        .map(|state| state.pending_actions)
                        .unwrap_or_default(),
                    checkbox_toggle: available,
                    toggle_placeholder: (!available).then_some(SELECTION_TOGGLE_UNAVAILABLE_PREFIX),
                    description: available.then(|| description.to_string()),
                    disabled_reason: (!available)
                        .then(|| {
                            crate::i18n::text(
                                language,
                                "Turn on Animations to show Astra task stars",
                                "开启动画后才能显示 Astra 任务星光",
                            )
                            .to_string()
                        }),
                    dismiss_on_select: false,
                    ..Default::default()
                }
            })
            .collect(),
        ..SelectionViewParams::picker()
    }
}

pub(crate) fn notification_condition(
    tui: &codex_config::types::Tui,
    keymap: &ListKeymap,
) -> SelectionViewParams {
    use codex_config::types::NotificationCondition;

    let language = tui.language;
    let current = tui.notification_settings.condition;
    let conditions = [
        (
            crate::i18n::text(language, "Unfocused", "失焦时"),
            NotificationCondition::Unfocused,
        ),
        (
            crate::i18n::text(language, "Always", "始终"),
            NotificationCondition::Always,
        ),
    ];
    SelectionViewParams {
        title: Some(crate::i18n::text(language, "Notify when", "通知时机").to_string()),
        subtitle: Some(
            crate::i18n::text(
                language,
                "Notify only while you are away or while this terminal is focused too",
                "仅在离开时通知，或终端处于焦点时也通知",
            )
            .to_string(),
        ),
        footer_hint: Some(picker_hint_line(keymap, language)),
        initial_selected_idx: conditions
            .iter()
            .position(|(_, condition)| *condition == current),
        allow_input_when_disconnected: true,
        items: conditions
            .into_iter()
            .map(|(label, condition)| SelectionItem {
                name: label.to_string(),
                is_current: condition == current,
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::SaveConfigPreference(
                        ConfigPreference::NotificationCondition(condition),
                    ));
                })],
                dismiss_on_select: true,
                ..Default::default()
            })
            .collect(),
        ..SelectionViewParams::picker()
    }
}

pub(crate) fn copy_on_select(
    tui: &codex_config::types::Tui,
    keymap: &ListKeymap,
) -> SelectionViewParams {
    use codex_config::types::CopyOnSelect;

    let language = tui.language;
    let current = tui.copy_on_select;
    let modes = [
        (
            crate::i18n::text(language, "Auto (default)", "自动（默认）"),
            CopyOnSelect::Auto,
        ),
        (
            crate::i18n::text(language, "On", "开"),
            CopyOnSelect::Always,
        ),
        (
            crate::i18n::text(language, "Off", "关"),
            CopyOnSelect::Never,
        ),
    ];
    SelectionViewParams {
        title: Some(crate::i18n::text(language, "Copy on select", "选中即复制").to_string()),
        subtitle: Some(
            crate::i18n::text(
                language,
                "Copy transcript, footer, and composer selections on release; Auto follows terminal defaults",
                "松开鼠标时复制对话、底栏和输入框中的选区；自动会跟随终端默认行为",
            )
            .to_string(),
        ),
        footer_hint: Some(picker_hint_line(keymap, language)),
        initial_selected_idx: modes.iter().position(|(_, mode)| *mode == current),
        allow_input_when_disconnected: true,
        items: modes
            .into_iter()
            .map(|(label, mode)| SelectionItem {
                name: label.to_string(),
                is_current: mode == current,
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::SaveConfigPreference(
                        ConfigPreference::CopyOnSelect(mode),
                    ));
                })],
                dismiss_on_select: true,
                ..Default::default()
            })
            .collect(),
        ..SelectionViewParams::picker()
    }
}

pub(crate) fn right_click_paste(
    tui: &codex_config::types::Tui,
    keymap: &ListKeymap,
) -> SelectionViewParams {
    use codex_config::types::RightClickPaste;

    let language = tui.language;
    let current = tui.right_click_paste;
    let modes = [
        (
            crate::i18n::text(language, "Auto (default)", "自动（默认）"),
            RightClickPaste::Auto,
        ),
        (
            crate::i18n::text(language, "On", "开"),
            RightClickPaste::On,
        ),
        (
            crate::i18n::text(language, "Off", "关"),
            RightClickPaste::Off,
        ),
    ];
    SelectionViewParams {
        title: Some(crate::i18n::text(language, "Right-click paste", "右键粘贴").to_string()),
        subtitle: Some(super::right_click_paste_description(language).to_string()),
        footer_hint: Some(picker_hint_line(keymap, language)),
        initial_selected_idx: modes.iter().position(|(_, mode)| *mode == current),
        allow_input_when_disconnected: true,
        items: modes
            .into_iter()
            .map(|(label, mode)| SelectionItem {
                name: label.to_string(),
                is_current: mode == current,
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::SaveConfigPreference(
                        ConfigPreference::RightClickPaste(mode),
                    ));
                })],
                dismiss_on_select: true,
                ..Default::default()
            })
            .collect(),
        ..SelectionViewParams::picker()
    }
}

pub(crate) fn session_picker_layout(
    tui: &codex_config::types::Tui,
    keymap: &ListKeymap,
) -> SelectionViewParams {
    use codex_config::types::SessionPickerViewMode;

    let language = tui.language;
    let current = tui.session_picker_view.unwrap_or_default();
    let modes = [
        (
            crate::i18n::text(language, "Dense", "紧凑"),
            SessionPickerViewMode::Dense,
        ),
        (
            crate::i18n::text(language, "Comfortable", "舒适"),
            SessionPickerViewMode::Comfortable,
        ),
    ];
    SelectionViewParams {
        title: Some(crate::i18n::text(language, "Session picker layout", "会话列表布局").to_string()),
        subtitle: Some(
            crate::i18n::text(
                language,
                "Choose tighter rows or a roomier list when resuming and forking conversations",
                "恢复或分叉会话时选择紧凑或宽松的列表",
            )
            .to_string(),
        ),
        footer_hint: Some(picker_hint_line(keymap, language)),
        initial_selected_idx: modes.iter().position(|(_, mode)| *mode == current),
        allow_input_when_disconnected: true,
        items: modes
            .into_iter()
            .map(|(label, mode)| SelectionItem {
                name: label.to_string(),
                is_current: mode == current,
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::SaveConfigPreference(
                        ConfigPreference::SessionPickerLayout(mode),
                    ));
                })],
                dismiss_on_select: true,
                ..Default::default()
            })
            .collect(),
        ..SelectionViewParams::picker()
    }
}

pub(crate) fn resume_directory(
    tui: &codex_config::types::Tui,
    keymap: &ListKeymap,
    current_availability: ResumeCurrentAvailability,
) -> SelectionViewParams {
    use codex_config::types::ResumeCwdMode;

    let language = tui.language;
    let current = tui.resume_cwd;
    let modes = [
        (crate::i18n::text(language, "Ask", "询问"), None),
        (
            crate::i18n::text(language, "Current directory", "当前目录"),
            Some(ResumeCwdMode::Current),
        ),
        (
            crate::i18n::text(language, "Session directory", "会话目录"),
            Some(ResumeCwdMode::Session),
        ),
    ];
    SelectionViewParams {
        title: Some(crate::i18n::text(language, "Resume directory", "恢复目录").to_string()),
        subtitle: Some(
            crate::i18n::text(
                language,
                "Choose the working directory when resuming, with --cd taking precedence for this session",
                "选择恢复时使用的工作目录；当前会话的 --cd 参数优先",
            )
            .to_string(),
        ),
        footer_hint: Some(picker_hint_line(keymap, language)),
        initial_selected_idx: modes.iter().position(|(_, mode)| *mode == current),
        allow_input_when_disconnected: true,
        items: modes
            .into_iter()
            .map(|(label, mode)| SelectionItem {
                name: label.to_string(),
                is_current: mode == current,
                disabled_reason: if mode == Some(ResumeCwdMode::Current)
                    && matches!(
                        current_availability,
                        ResumeCurrentAvailability::RequiresCwdOverride
                    )
                {
                    Some(
                        crate::i18n::text(
                            language,
                            "Requires --cd for remote sessions",
                            "远程会话需要指定 --cd",
                        )
                        .to_string(),
                    )
                } else {
                    None
                },
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::SaveConfigPreference(
                        ConfigPreference::ResumeDirectory(mode),
                    ));
                })],
                dismiss_on_select: true,
                ..Default::default()
            })
            .collect(),
        ..SelectionViewParams::picker()
    }
}

struct ConfigPanelHeader(Vec<Line<'static>>);

impl Renderable for ConfigPanelHeader {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        Renderable::render(
            &Paragraph::new(word_wrap_lines(&self.0, usize::from(area.width))),
            area,
            buf,
        );
    }

    fn desired_height(&self, width: u16) -> u16 {
        word_wrap_lines(&self.0, usize::from(width)).len() as u16
    }
}

fn header(notice: Option<String>, language: TuiLanguage) -> Box<dyn Renderable> {
    let subtitle = notice.unwrap_or_else(|| {
        crate::i18n::text(language, "Changes save automatically", "更改会自动保存").to_string()
    });
    Box::new(ConfigPanelHeader(vec![
        Line::from(crate::i18n::text(language, "Configuration", "配置").bold()),
        Line::from(subtitle.dim()),
    ]))
}

fn hint(
    keymap: &ListKeymap,
    search: Option<KeyBinding>,
    accept_label: Option<&str>,
    language: TuiLanguage,
) -> Line<'static> {
    let key_label = |action| {
        keymap.primary_hint(action).map(|hint| {
            hint.display_label()
                .replace('←', crate::i18n::text(language, "left", "左"))
                .replace('→', crate::i18n::text(language, "right", "右"))
        })
    };
    let mut items = Vec::new();
    let keys = [ListAction::MoveLeft, ListAction::MoveRight]
        .into_iter()
        .filter_map(key_label)
        .collect::<Vec<_>>()
        .join("/");
    if !keys.is_empty() {
        items.push((keys, crate::i18n::text(language, "group", "分组").into()));
    }
    if let Some(search) = search {
        items.push((
            search.display_label(),
            crate::i18n::text(language, "search", "搜索").into(),
        ));
    }
    items.extend(
        accept_label
            .map(|label| (ListAction::Accept, label))
            .into_iter()
            .chain([(
                ListAction::Cancel,
                crate::i18n::text(language, "clear search/close", "清除搜索/关闭"),
            )])
            .filter_map(|(action, label)| key_label(action).map(|key| (key, label.into()))),
    );
    footer_hint_items_line(&items)
}

#[derive(Clone, Copy)]
enum SettingItemContext {
    Group,
    Search,
}

fn setting_item(
    setting: &ConfigSetting,
    keymap: &ListKeymap,
    search_hint: Option<KeyBinding>,
    context: SettingItemContext,
    language: TuiLanguage,
) -> SelectionItem {
    let (unavailable, accept_label) = match &setting.value {
        ConfigSettingValue::DependentToggle {
            dependency_met: false,
            unavailable_reason,
            ..
        } => (Some(*unavailable_reason), None),
        ConfigSettingValue::Unavailable { reason } => (Some(*reason), None),
        ConfigSettingValue::Toggle { .. } | ConfigSettingValue::DependentToggle { .. } => {
            (None, Some(crate::i18n::text(language, "toggle", "切换")))
        }
        ConfigSettingValue::Editor { .. } => (None, Some(crate::i18n::text(language, "open", "打开"))),
        ConfigSettingValue::NotificationTest => {
            (None, Some(crate::i18n::text(language, "test", "测试")))
        }
    };
    let value_label = setting.value.label(language);
    let description = unavailable.unwrap_or(setting.description);
    let search_footer_hint = footer_hint_items_line(
        &accept_label
            .map(|label| ("enter".to_string(), label.to_string()))
            .into_iter()
            .chain([(
                "esc".to_string(),
                crate::i18n::text(language, "clear search", "清除搜索").to_string(),
            )])
            .collect::<Vec<_>>(),
    );
    let description = match context {
        SettingItemContext::Group => description.to_string(),
        SettingItemContext::Search => {
            format!("{} · {description}", setting.section.title_for(language))
        }
    };
    SelectionItem {
        name: setting.label.to_string(),
        category_tag: Some(format!("{value_label:<20}")),
        description: Some(description),
        is_disabled: unavailable.is_some(),
        footer_hint: Some(hint(keymap, search_hint, accept_label, language)),
        search_footer_hint: Some(search_footer_hint),
        search_value: Some(format!(
            "{} {} {} {} {}",
            setting.section.title_for(language),
            setting.label,
            value_label,
            setting.keywords,
            setting.description
        )),
        ..Default::default()
    }
}

pub(super) fn picker(
    settings: &[ConfigSetting],
    app_event_tx: AppEventSender,
    keymap: ListKeymap,
    state: PickerState,
    language: TuiLanguage,
) -> ListSelectionView {
    let search = crate::key_hint::plain(KeyCode::Char('/'));
    let (search_code, search_modifiers) = search.parts();
    let search_event = KeyEvent::new(search_code, search_modifiers);
    let search_hint = (keymap.action_for(search_event).is_none()
        && !keymap.has_chord_prefix(search_event))
    .then_some(search);
    let mut params = SelectionViewParams {
        picker_surface: PickerSurface::Panel,
        max_visible_rows: ConfigSection::ALL
            .iter()
            .map(|section| {
                settings
                    .iter()
                    .filter(|setting| setting.section == *section)
                    .count()
            })
            .max()
            .unwrap_or_default(),
        reserve_result_rows: true,
        row_display: SelectionRowDisplay::SingleLine,
        col_width_mode: ColumnWidthMode::AutoAllRows,
        description_layout: SelectionDescriptionLayout::SelectedBelowWhenNarrow {
            min_description_width: 45,
        },
        footer_hint: Some(hint(
            &keymap,
            search_hint,
            /*accept_label*/ None,
            language,
        )),
        is_searchable: true,
        search_mode: SearchMode::OnDemand(search),
        search_placeholder: Some(
            crate::i18n::text(language, "Type to search settings", "输入以搜索设置").to_string(),
        ),
        search_items: settings
            .iter()
            .map(|setting| {
                setting_item(
                    setting,
                    &keymap,
                    search_hint,
                    SettingItemContext::Search,
                    language,
                )
            })
            .collect(),
        name_column_width: settings
            .iter()
            .map(|setting| UnicodeWidthStr::width(setting.label))
            .max(),
        tabs: ConfigSection::ALL
            .iter()
            .map(|section| {
                let title = section.title_for(language);
                SelectionTab {
                    id: title.to_string(),
                    label: title.to_string(),
                    header: header(state.notice.clone(), language),
                    items: settings
                        .iter()
                        .filter(|setting| setting.section == *section)
                        .map(|setting| {
                            setting_item(
                                setting,
                                &keymap,
                                search_hint,
                                SettingItemContext::Group,
                                language,
                            )
                        })
                        .collect(),
                }
            })
            .collect(),
        ..Default::default()
    };
    params.initial_selected_idx = state.selected;
    params.initial_search_query = state.query;
    params.initial_tab_id = state.tab;
    ListSelectionView::new(params, app_event_tx, keymap)
}

impl Renderable for ConfigPanel {
    fn desired_height(&self, width: u16) -> u16 {
        self.picker.desired_height(width)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.picker.render(area, buf);
    }

    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.picker.cursor_pos(area)
    }
}
