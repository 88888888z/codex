//! Shortcut picker construction for `/keymap`.
//!
//! Keep the shared picker panel and reserved result viewport on the production
//! factory so tabs and search stay anchored in the live picker.

use codex_config::types::TuiKeymap;
use ratatui::style::Styled;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

use crate::app_event::AppEvent;
use crate::bottom_pane::ColumnWidthMode;
use crate::bottom_pane::PickerSurface;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionRowDisplay;
use crate::bottom_pane::SelectionTab;
use crate::bottom_pane::SelectionViewParams;
use crate::keymap::RuntimeKeymap;
use crate::render::renderable::ColumnRenderable;
use crate::render::renderable::Renderable;
use crate::style::accent_style;

use super::actions::KEYMAP_ACTIONS;
use super::actions::KeymapActionFilter;
use super::actions::action_label;
use super::actions::format_action_binding_summary;
use super::has_custom_binding;

pub(crate) const KEYMAP_PICKER_VIEW_ID: &str = "keymap-picker";
pub(super) const KEYMAP_ALL_TAB_ID: &str = "all-shortcuts";
pub(super) const KEYMAP_COMMON_TAB_ID: &str = "common-shortcuts";
pub(super) const KEYMAP_CUSTOM_TAB_ID: &str = "custom-shortcuts";
pub(super) const KEYMAP_UNBOUND_TAB_ID: &str = "unbound-shortcuts";
pub(super) const KEYMAP_DEBUG_TAB_ID: &str = "debug-shortcuts";
const KEYMAP_CONTEXT_LABEL_WIDTH: usize = 12;
const KEYMAP_ROW_PREFIX_WIDTH: usize = KEYMAP_CONTEXT_LABEL_WIDTH + 3;

#[derive(Clone, Debug)]
struct KeymapActionRow {
    context: &'static str,
    context_label: &'static str,
    action: &'static str,
    label: String,
    description: &'static str,
    binding_summary: String,
    custom_binding: bool,
}

impl KeymapActionRow {
    fn is_unbound(&self) -> bool {
        self.binding_summary == "unbound"
    }
}

struct KeymapContextTab {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    contexts: &'static [&'static str],
}

const KEYMAP_COMMON_ACTIONS: &[(&str, &str)] = &[
    ("composer", "submit"),
    ("chat", "interrupt_turn"),
    ("editor", "insert_newline"),
    ("composer", "queue"),
    ("global", "toggle_fast_mode"),
    ("global", "open_external_editor"),
    ("global", "copy"),
    ("global", "toggle_vim_mode"),
    ("editor", "delete_backward_word"),
    ("editor", "delete_forward_word"),
    ("editor", "move_word_left"),
    ("editor", "move_word_right"),
    ("global", "open_transcript"),
    ("pager", "close"),
    ("pager", "page_up"),
    ("pager", "page_down"),
    ("approval", "open_fullscreen"),
    ("approval", "approve"),
    ("approval", "approve_for_session"),
    ("approval", "decline"),
    ("approval", "cancel"),
];

const KEYMAP_CONTEXT_TABS: &[KeymapContextTab] = &[
    KeymapContextTab {
        id: "app-shortcuts",
        label: "App",
        description: "Global and chat-level shortcuts.",
        contexts: &["global", "chat"],
    },
    KeymapContextTab {
        id: "composer-shortcuts",
        label: "Composer",
        description: "Composer submission and queue shortcuts.",
        contexts: &["composer"],
    },
    KeymapContextTab {
        id: "editor-shortcuts",
        label: "Editor",
        description: "Inline editor movement and editing shortcuts.",
        contexts: &["editor"],
    },
    KeymapContextTab {
        id: "vim-shortcuts",
        label: "Vim",
        description: "Vim normal-mode and operator shortcuts.",
        contexts: &[
            "vim_normal",
            "vim_operator",
            "vim_search",
            "vim_text_object",
        ],
    },
    KeymapContextTab {
        id: "navigation-shortcuts",
        label: "Navigation",
        description: "Pager and selection-list navigation shortcuts.",
        contexts: &["pager", "list"],
    },
    KeymapContextTab {
        id: "agents-shortcuts",
        label: "Agents",
        description: "Shared agents dashboard shortcuts.",
        contexts: &["agents"],
    },
    KeymapContextTab {
        id: "approval-shortcuts",
        label: "Approval",
        description: "Approval prompt shortcuts.",
        contexts: &["approval"],
    },
];

#[cfg(test)]
pub(crate) fn build_keymap_picker_params(
    runtime_keymap: &RuntimeKeymap,
    keymap_config: &TuiKeymap,
) -> SelectionViewParams {
    build_keymap_picker_params_with_filter(
        runtime_keymap,
        keymap_config,
        KeymapActionFilter::default(),
    )
}

pub(crate) fn build_keymap_picker_params_with_filter(
    runtime_keymap: &RuntimeKeymap,
    keymap_config: &TuiKeymap,
    action_filter: KeymapActionFilter,
) -> SelectionViewParams {
    build_keymap_picker_params_for_action(
        runtime_keymap,
        keymap_config,
        action_filter,
        /*selected_action*/ None,
    )
}

#[cfg(test)]
pub(crate) fn build_keymap_picker_params_for_selected_action(
    runtime_keymap: &RuntimeKeymap,
    keymap_config: &TuiKeymap,
    context: &str,
    action: &str,
) -> SelectionViewParams {
    build_keymap_picker_params_for_selected_action_with_filter(
        runtime_keymap,
        keymap_config,
        KeymapActionFilter::default(),
        context,
        action,
    )
}

pub(crate) fn build_keymap_picker_params_for_selected_action_with_filter(
    runtime_keymap: &RuntimeKeymap,
    keymap_config: &TuiKeymap,
    action_filter: KeymapActionFilter,
    context: &str,
    action: &str,
) -> SelectionViewParams {
    build_keymap_picker_params_for_action(
        runtime_keymap,
        keymap_config,
        action_filter,
        Some((context, action)),
    )
}

fn build_keymap_picker_params_for_action(
    runtime_keymap: &RuntimeKeymap,
    keymap_config: &TuiKeymap,
    action_filter: KeymapActionFilter,
    selected_action: Option<(&str, &str)>,
) -> SelectionViewParams {
    let rows = build_keymap_rows(runtime_keymap, keymap_config, action_filter);
    let total = rows.len();
    let custom_count = rows.iter().filter(|row| row.custom_binding).count();
    let unbound_count = rows.iter().filter(|row| row.is_unbound()).count();
    let initial_selected_idx = selected_action.and_then(|(context, action)| {
        rows.iter()
            .position(|row| row.context == context && row.action == action)
    });
    let name_column_width = rows
        .iter()
        .map(|row| KEYMAP_ROW_PREFIX_WIDTH + UnicodeWidthStr::width(row.label.as_str()))
        .max();

    let mut tabs = Vec::new();
    tabs.push(SelectionTab {
        id: KEYMAP_ALL_TAB_ID.to_string(),
        label: crate::i18n::ui_text("All", "全部").to_string(),
        header: keymap_header(
            crate::i18n::ui_text("All configurable shortcuts.", "全部可配置快捷键。")
                .to_string(),
            action_count_line(total, Some((custom_count, unbound_count))),
        ),
        items: keymap_selection_items(
            rows.iter(),
            crate::i18n::ui_text("No shortcuts available", "没有可用快捷键"),
            crate::i18n::ui_text(
                "No configurable shortcuts are available",
                "没有可配置的快捷键",
            ),
        ),
    });

    let common_rows = keymap_common_rows(&rows);
    let common_count = common_rows.len();
    tabs.push(SelectionTab {
        id: KEYMAP_COMMON_TAB_ID.to_string(),
        label: crate::i18n::ui_text("Common", "常用").to_string(),
        header: keymap_header(
            crate::i18n::ui_text("Frequently customized shortcuts.", "经常自定义的快捷键。")
                .to_string(),
            action_count_line(common_count, None),
        ),
        items: keymap_selection_items(
            common_rows,
            crate::i18n::ui_text("No common shortcuts", "没有常用快捷键"),
            crate::i18n::ui_text(
                "No common shortcut actions are available",
                "没有可用的常用快捷键操作",
            ),
        ),
    });

    let custom_rows = rows
        .iter()
        .filter(|row| row.custom_binding)
        .collect::<Vec<_>>();
    tabs.push(SelectionTab {
        id: KEYMAP_CUSTOM_TAB_ID.to_string(),
        label: if crate::i18n::is_chinese(crate::i18n::current_language()) {
            format!("已自定义（{custom_count}）")
        } else {
            format!("Customized ({custom_count})")
        },
        header: keymap_header(
            crate::i18n::ui_text("Root-level shortcut overrides.", "根级快捷键覆盖。")
                .to_string(),
            action_count_line(custom_count, None),
        ),
        items: keymap_selection_items(
            custom_rows,
            crate::i18n::ui_text("No customized shortcuts", "没有已自定义的快捷键"),
            crate::i18n::ui_text(
                "No root-level keymap overrides have been configured",
                "尚未配置根级快捷键覆盖",
            ),
        ),
    });

    let unbound_rows = rows
        .iter()
        .filter(|row| row.is_unbound())
        .collect::<Vec<_>>();
    tabs.push(SelectionTab {
        id: KEYMAP_UNBOUND_TAB_ID.to_string(),
        label: if crate::i18n::is_chinese(crate::i18n::current_language()) {
            format!("未绑定（{unbound_count}）")
        } else {
            format!("Unbound ({unbound_count})")
        },
        header: keymap_header(
            crate::i18n::ui_text(
                "Actions without an active shortcut.",
                "没有活动快捷键的操作。",
            )
            .to_string(),
            action_count_line(unbound_count, None),
        ),
        items: keymap_selection_items(
            unbound_rows,
            crate::i18n::ui_text("No unbound shortcuts", "没有未绑定的快捷键"),
            crate::i18n::ui_text(
                "Every configurable action currently has a shortcut",
                "每个可配置操作当前都已绑定快捷键",
            ),
        ),
    });

    for tab in KEYMAP_CONTEXT_TABS {
        let tab_rows = rows
            .iter()
            .filter(|row| tab.contexts.contains(&row.context))
            .collect::<Vec<_>>();
        let count = tab_rows.len();
        tabs.push(SelectionTab {
            id: tab.id.to_string(),
            label: tab.label.to_string(),
            header: keymap_header(
                tab.description.to_string(),
                action_count_line(count, None),
            ),
            items: keymap_selection_items(
                tab_rows,
                crate::i18n::ui_text("No shortcuts in this group", "此分组没有快捷键"),
                crate::i18n::ui_text(
                    "No configurable actions are available in this group",
                    "此分组没有可配置操作",
                ),
            ),
        });
    }
    tabs.push(keymap_debug_tab());

    SelectionViewParams {
        allow_input_when_disconnected: true,
        view_id: Some(KEYMAP_PICKER_VIEW_ID),
        picker_surface: PickerSurface::Panel,
        max_visible_rows: 24,
        reserve_result_rows: true,
        header: Box::new(()),
        footer_hint: Some(keymap_picker_hint_line()),
        tab_footer_hints: vec![(KEYMAP_DEBUG_TAB_ID.to_string(), keymap_debug_hint_line())],
        tabs,
        initial_tab_id: Some(KEYMAP_ALL_TAB_ID.to_string()),
        is_searchable: true,
        search_placeholder: Some(
            crate::i18n::ui_text("Type to search shortcuts", "输入以搜索快捷键")
                .to_string(),
        ),
        col_width_mode: ColumnWidthMode::AutoAllRows,
        row_display: SelectionRowDisplay::SingleLine,
        name_column_width,
        initial_selected_idx,
        ..Default::default()
    }
}

fn keymap_debug_tab() -> SelectionTab {
    SelectionTab {
        id: KEYMAP_DEBUG_TAB_ID.to_string(),
        label: crate::i18n::ui_text("Debug", "调试").to_string(),
        header: keymap_header(
            crate::i18n::ui_text(
                "Inspect keypresses from your terminal.",
                "检查终端发送的按键。",
            )
            .to_string(),
            crate::i18n::ui_text(
                "See the key Codex detects and any shortcuts assigned to it.",
                "查看 Codex 识别到的按键及其绑定的快捷键。",
            )
            .to_string(),
        ),
        items: vec![SelectionItem {
            name: crate::i18n::ui_text("Inspect keypresses", "检查按键").to_string(),
            description: Some(
                crate::i18n::ui_text(
                    "Press Enter, then any key to inspect it (Ctrl+C exits)",
                    "按 Enter，然后按任意键进行检查（Ctrl+C 退出）",
                )
                .to_string(),
            ),
            selected_description: Some(
                crate::i18n::ui_text(
                    "Open a live inspector that shows the detected key, config key, and matching actions",
                    "打开实时检查器，显示识别的按键、配置键和匹配操作",
                )
                .to_string(),
            ),
            actions: vec![Box::new(|tx| {
                tx.send(AppEvent::OpenKeymapDebug);
            })],
            search_value: Some("debug inspect keypress key terminal detected actions".to_string()),
            ..Default::default()
        }],
    }
}

fn build_keymap_rows(
    runtime_keymap: &RuntimeKeymap,
    keymap_config: &TuiKeymap,
    action_filter: KeymapActionFilter,
) -> Vec<KeymapActionRow> {
    KEYMAP_ACTIONS
        .iter()
        .copied()
        .filter(|descriptor| descriptor.is_visible(action_filter))
        .map(|descriptor| KeymapActionRow {
            context: descriptor.context,
            context_label: descriptor.context_label,
            action: descriptor.action,
            label: action_label(descriptor.action),
            description: descriptor.description,
            binding_summary: format_action_binding_summary(
                runtime_keymap,
                descriptor.context,
                descriptor.action,
            ),
            custom_binding: has_custom_binding(
                keymap_config,
                descriptor.context,
                descriptor.action,
            )
            .unwrap_or(/*default*/ false),
        })
        .collect()
}

fn keymap_common_rows(rows: &[KeymapActionRow]) -> Vec<&KeymapActionRow> {
    KEYMAP_COMMON_ACTIONS
        .iter()
        .filter_map(|(context, action)| {
            rows.iter()
                .find(|row| row.context == *context && row.action == *action)
        })
        .collect()
}

fn keymap_selection_items<'a>(
    rows: impl IntoIterator<Item = &'a KeymapActionRow>,
    empty_name: &str,
    empty_description: &str,
) -> Vec<SelectionItem> {
    let items = rows
        .into_iter()
        .map(keymap_selection_item)
        .collect::<Vec<_>>();
    if items.is_empty() {
        return vec![SelectionItem {
            name: empty_name.to_string(),
            description: Some(empty_description.to_string()),
            is_disabled: true,
            ..Default::default()
        }];
    }

    items
}

fn keymap_selection_item(row: &KeymapActionRow) -> SelectionItem {
    let context = row.context.to_string();
    let action = row.action.to_string();
    let source = if row.custom_binding {
        "Custom"
    } else {
        "Default"
    };
    let search_value = format!(
        "{} {} {} {} {} {}",
        row.context_label, row.action, row.label, row.description, row.binding_summary, source
    );

    SelectionItem {
        name: row.label.clone(),
        name_prefix_spans: keymap_row_prefix(row),
        description: Some(row.binding_summary.clone()),
        actions: vec![Box::new(move |tx| {
            tx.send(AppEvent::OpenKeymapActionMenu {
                context: context.clone(),
                action: action.clone(),
            });
        })],
        search_value: Some(search_value),
        ..Default::default()
    }
}

fn keymap_row_prefix(row: &KeymapActionRow) -> Vec<Span<'static>> {
    let indicator = if row.custom_binding {
        "*".set_style(accent_style())
    } else if row.is_unbound() {
        "-".dim()
    } else {
        " ".into()
    };

    vec![
        format!(
            "{:<width$} ",
            row.context_label,
            width = KEYMAP_CONTEXT_LABEL_WIDTH
        )
        .dim(),
        indicator,
        " ".dim(),
    ]
}

fn keymap_header(description: String, summary: String) -> Box<dyn Renderable> {
    let mut header = ColumnRenderable::new();
    header.push(Line::from(crate::i18n::ui_text("Keymap", "快捷键").bold()));
    header.push(Line::from(description.dim()));
    header.push(Line::from(summary.dim()));
    Box::new(header)
}

fn action_count_line(count: usize, details: Option<(usize, usize)>) -> String {
    if crate::i18n::is_chinese(crate::i18n::current_language()) {
        if let Some((customized, unbound)) = details {
            format!("{count} 个操作，{customized} 个已自定义，{unbound} 个未绑定。")
        } else {
            format!("{count} 个操作。")
        }
    } else if let Some((customized, unbound)) = details {
        format!("{count} actions, {customized} customized, {unbound} unbound.")
    } else {
        match count {
            1 => "1 action.".to_string(),
            _ => format!("{count} actions."),
        }
    }
}

fn keymap_picker_hint_line() -> Line<'static> {
    [
        (
            "left/right",
            crate::i18n::ui_text(" group · ", " 分组 · "),
        ),
        (
            "enter",
            crate::i18n::ui_text(" edit shortcut · ", " 编辑快捷键 · "),
        ),
        ("*", crate::i18n::ui_text(" custom · ", " 已自定义 · ")),
        ("-", crate::i18n::ui_text(" unbound · ", " 未绑定 · ")),
        ("esc", crate::i18n::ui_text(" close", " 关闭")),
    ]
    .into_iter()
    .flat_map(|(key, label)| {
        crate::key_hint::key_label_spans(key)
            .into_iter()
            .chain([label.set_style(crate::style::footer_hint_label_style())])
    })
    .collect::<Line>()
}

fn keymap_debug_hint_line() -> Line<'static> {
    let mut spans = crate::key_hint::key_label_spans("enter");
    spans.push(
        crate::i18n::ui_text(" start inspector · ", " 开始检查 · ")
            .set_style(crate::style::footer_hint_label_style()),
    );
    spans.extend(crate::key_hint::key_label_spans("esc"));
    spans.push(
        crate::i18n::ui_text(" close", " 关闭")
            .set_style(crate::style::footer_hint_label_style()),
    );
    spans.into()
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;
