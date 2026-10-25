//! Catalog of TUI configuration editors and directly editable preferences shown by `/config`.
//!
//! This module defines the stable grouping, copy, search terms, and displayed values. Rendering
//! and interaction live in later modules so they can consume one ordered catalog.

mod input;
mod render;
mod toggle;
pub(crate) use render::EFFECTS_VIEW_ID;
pub(crate) use render::copy_on_select;
pub(crate) use render::effects;
pub(crate) use render::effects_with_toggle_states;
pub(crate) use render::notification_condition;
pub(crate) use render::resume_directory;
pub(crate) use render::right_click_paste;
pub(crate) use render::session_picker_layout;
pub(crate) use toggle::ConfigPreference;
pub(crate) use toggle::ConfigToggle;

#[cfg(test)]
#[path = "config_panel_tests.rs"]
mod tests;

use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::ListSelectionView;
use crate::key_hint::KeyBinding;
use crate::keymap::ListKeymap;
use codex_config::types::CopyOnSelect;
use codex_config::types::NotificationCondition;
use codex_config::types::Notifications;
use codex_config::types::RightClickPaste;
use codex_config::types::Tui;
use codex_config::types::TuiLanguage;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

fn right_click_paste_description(language: TuiLanguage) -> &'static str {
    crate::i18n::text(
        language,
        "Paste when nothing is selected, with Auto following platform defaults and On also enabling macOS",
        "未选中内容时粘贴；自动会跟随平台默认，开启还会在 macOS 上启用",
    )
}

fn right_click_paste_unavailable(language: TuiLanguage, reason: &'static str) -> &'static str {
    if !crate::i18n::is_chinese(language) {
        return reason;
    }
    match reason {
        "Unavailable in inline mode; use your terminal's paste command" => {
            "内联模式不可用，请使用终端自身的粘贴命令"
        }
        "Unavailable over SSH; use your terminal's paste command" => {
            "SSH 连接中不可用，请使用终端自身的粘贴命令"
        }
        "VS Code handles pasting; use its terminal paste command" => {
            "VS Code 会处理粘贴，请使用其终端粘贴命令"
        }
        "Auto cannot identify this WSL terminal; use On or your terminal's paste command" => {
            "自动模式无法识别此 WSL 终端，请选择开启或使用终端粘贴命令"
        }
        "Unavailable on Android" => "Android 上不可用",
        _ => "此终端不支持右键粘贴",
    }
}

pub(crate) struct ConfigPanelUpdate {
    pub(crate) tui: Tui,
    pub(crate) keymap: ListKeymap,
    pub(crate) status: ConfigPanelStatus,
    pub(crate) notice: Option<String>,
}

#[derive(Default)]
struct PickerState {
    tab: Option<String>,
    selected: Option<usize>,
    query: Option<String>,
    notice: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigEditor {
    Theme,
    Effects,
    StatusLine,
    TerminalTitle,
    Keyboard,
    SessionPickerLayout,
    ResumeDirectory,
    CopyOnSelect,
    RightClickPaste,
    NotificationCondition,
}

#[derive(Clone, Copy)]
pub(crate) enum ResumeCurrentAvailability {
    Available,
    RequiresCwdOverride,
}

/// Effective state the panel cannot derive from the `Tui` preferences alone.
#[derive(Clone, Copy)]
pub(crate) struct ConfigPanelStatus {
    pub(crate) copy_on_select: bool,
    pub(crate) right_click_paste: Result<bool, &'static str>,
    pub(crate) custom_notification_filters: bool,
}

/// State shared by the panel that queued an editor and the event that will open it.
#[derive(Clone, Debug)]
pub(crate) struct ConfigEditorHandoff {
    pub(crate) editor: ConfigEditor,
    pub(crate) activation_key: KeyBinding,
    activation: Arc<Mutex<ConfigEditorActivation>>,
}

#[derive(Debug)]
struct ConfigEditorActivation {
    active: bool,
    last_seen_at: Instant,
}

impl ConfigEditorHandoff {
    pub(crate) fn new(editor: ConfigEditor, activation_key: KeyBinding) -> Self {
        Self {
            editor,
            activation_key,
            activation: Arc::new(Mutex::new(ConfigEditorActivation {
                active: true,
                last_seen_at: Instant::now(),
            })),
        }
    }

    pub(crate) fn mark_activation_seen(&self) {
        self.mark_activation_seen_at(Instant::now());
    }

    fn mark_activation_seen_at(&self, seen_at: Instant) {
        let mut activation = self
            .activation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if activation.active {
            activation.last_seen_at = activation.last_seen_at.max(seen_at);
        }
    }

    pub(crate) fn mark_activation_inactive(&self) {
        self.activation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .active = false;
    }

    pub(crate) fn active_at(&self) -> Option<Instant> {
        let activation = self
            .activation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        activation.active.then_some(activation.last_seen_at)
    }
}

impl ConfigEditor {
    fn value(self, tui: &Tui, status: ConfigPanelStatus) -> ConfigSettingValue {
        let language = tui.language;
        let label = match self {
            Self::Theme => tui
                .theme
                .clone()
                .unwrap_or_else(|| crate::i18n::text(language, "Automatic", "自动").to_string()),
            Self::SessionPickerLayout => match tui.session_picker_view.unwrap_or_default() {
                codex_config::types::SessionPickerViewMode::Comfortable => {
                    crate::i18n::text(language, "Comfortable", "舒适")
                }
                codex_config::types::SessionPickerViewMode::Dense => {
                    crate::i18n::text(language, "Dense", "紧凑")
                }
            }
            .to_string(),
            Self::ResumeDirectory => match tui.resume_cwd {
                None => crate::i18n::text(language, "Ask", "询问"),
                Some(codex_config::types::ResumeCwdMode::Current) => {
                    crate::i18n::text(language, "Current directory", "当前目录")
                }
                Some(codex_config::types::ResumeCwdMode::Session) => {
                    crate::i18n::text(language, "Session directory", "会话目录")
                }
            }
            .to_string(),
            Self::CopyOnSelect => match tui.copy_on_select {
                CopyOnSelect::Auto => format!(
                    "{} · {}",
                    crate::i18n::text(language, "Auto", "自动"),
                    if status.copy_on_select {
                        crate::i18n::text(language, "On", "开")
                    } else {
                        crate::i18n::text(language, "Off", "关")
                    }
                ),
                CopyOnSelect::Always => {
                    crate::i18n::text(language, "On", "开").to_string()
                }
                CopyOnSelect::Never => {
                    crate::i18n::text(language, "Off", "关").to_string()
                }
            },
            Self::RightClickPaste => {
                let preference = match tui.right_click_paste {
                    RightClickPaste::Auto => crate::i18n::text(language, "Auto", "自动"),
                    RightClickPaste::On => crate::i18n::text(language, "On", "开"),
                    RightClickPaste::Off => crate::i18n::text(language, "Off", "关"),
                };
                match status.right_click_paste {
                    Ok(enabled) if tui.right_click_paste == RightClickPaste::Auto => {
                        format!(
                            "{} · {}",
                            preference,
                            if enabled {
                                crate::i18n::text(language, "On", "开")
                            } else {
                                crate::i18n::text(language, "Off", "关")
                            }
                        )
                    }
                    Ok(_) => preference.to_string(),
                    Err(_) => format!(
                        "{preference} · {}",
                        crate::i18n::text(language, "Unavailable", "不可用")
                    ),
                }
            }
            Self::NotificationCondition => match tui.notification_settings.condition {
                NotificationCondition::Unfocused => {
                    crate::i18n::text(language, "Unfocused", "失焦时")
                }
                NotificationCondition::Always => {
                    crate::i18n::text(language, "Always", "始终")
                }
            }
            .to_string(),
            Self::Effects | Self::StatusLine | Self::TerminalTitle | Self::Keyboard => {
                crate::i18n::text(language, "Configure", "配置").to_string()
            }
        };
        ConfigSettingValue::Editor {
            label,
            editor: self,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfigSection {
    Appearance,
    Notifications,
    Input,
    Sessions,
    TextSelection,
}

impl ConfigSection {
    const ALL: [Self; 5] = [
        Self::Appearance,
        Self::Notifications,
        Self::Input,
        Self::Sessions,
        Self::TextSelection,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Notifications => "Notifications",
            Self::Input => "Input",
            Self::Sessions => "Sessions",
            Self::TextSelection => "Text selection",
        }
    }

    fn title_for(self, language: TuiLanguage) -> &'static str {
        match self {
            Self::Appearance => crate::i18n::text(language, "Appearance", "外观"),
            Self::Notifications => crate::i18n::text(language, "Notifications", "通知"),
            Self::Input => crate::i18n::text(language, "Input", "输入"),
            Self::Sessions => crate::i18n::text(language, "Sessions", "会话"),
            Self::TextSelection => {
                crate::i18n::text(language, "Text selection", "文本选择")
            }
        }
    }
}

struct ConfigSetting {
    section: ConfigSection,
    label: &'static str,
    description: &'static str,
    keywords: &'static str,
    value: ConfigSettingValue,
}

enum ConfigSettingValue {
    Toggle {
        setting: ConfigToggle,
        enabled: bool,
    },
    Editor {
        label: String,
        editor: ConfigEditor,
    },
    DependentToggle {
        setting: ConfigToggle,
        enabled: bool,
        dependency_met: bool,
        unavailable_reason: &'static str,
    },
    NotificationTest,
    Unavailable {
        reason: &'static str,
    },
}

impl ConfigSettingValue {
    fn label(&self, language: TuiLanguage) -> String {
        match self {
            Self::Toggle { enabled, .. }
            | Self::DependentToggle {
                enabled,
                dependency_met: true,
                ..
            } => if *enabled {
                crate::i18n::text(language, "[x] On", "[x] 开")
            } else {
                crate::i18n::text(language, "[ ] Off", "[ ] 关")
            }
            .to_string(),
            Self::Editor { label, .. } => format!("{label} ›"),
            Self::NotificationTest => {
                crate::i18n::text(language, "Send in 5s", "5 秒后发送").to_string()
            }
            Self::DependentToggle {
                dependency_met: false,
                ..
            }
            | Self::Unavailable { .. } => {
                crate::i18n::text(language, "Unavailable", "不可用").to_string()
            }
        }
    }
}

fn settings(tui: &Tui, status: ConfigPanelStatus) -> Vec<ConfigSetting> {
    let language = tui.language;
    let notifications_enabled = !matches!(
        tui.notification_settings.notifications,
        Notifications::Enabled(false)
    );
    vec![
        ConfigSetting {
            section: ConfigSection::Appearance,
            label: crate::i18n::text(language, "Theme", "主题"),
            description: crate::i18n::text(
                language,
                "Choose a syntax color palette with a live preview",
                "选择语法配色方案并实时预览",
            ),
            keywords: "colors dark light palette 颜色 深色 浅色 配色",
            value: ConfigEditor::Theme.value(tui, status),
        },
        ConfigSetting {
            section: ConfigSection::Appearance,
            label: crate::i18n::text(language, "Effects", "视觉效果"),
            description: crate::i18n::text(
                language,
                "Configure animations and composer effects",
                "配置动画和输入框特效",
            ),
            keywords: "animations motion accessibility spinner starfield stars composer stars 动画 动态 无障碍 星空",
            value: ConfigEditor::Effects.value(tui, status),
        },
        ConfigSetting {
            section: ConfigSection::Appearance,
            label: crate::i18n::text(language, "Tips", "提示"),
            description: crate::i18n::text(
                language,
                "Show helpful shortcuts and suggestions",
                "显示实用快捷键和建议",
            ),
            keywords: "welcome hints tooltips 欢迎 提示 快捷键",
            value: ConfigSettingValue::Toggle {
                setting: ConfigToggle::Tips,
                enabled: ConfigToggle::Tips.enabled(tui),
            },
        },
        ConfigSetting {
            section: ConfigSection::Appearance,
            label: crate::i18n::text(language, "Status line colors", "状态栏颜色"),
            description: crate::i18n::text(
                language,
                "Use theme colors for the model, branch, and other status line details",
                "为模型、分支和其他状态栏信息使用主题色",
            ),
            keywords: "statusline footer colour 状态栏 颜色 模型 分支",
            value: ConfigSettingValue::Toggle {
                setting: ConfigToggle::StatusLineColors,
                enabled: ConfigToggle::StatusLineColors.enabled(tui),
            },
        },
        ConfigSetting {
            section: ConfigSection::Appearance,
            label: crate::i18n::text(language, "Status line display", "状态栏内容"),
            description: crate::i18n::text(
                language,
                "Choose and reorder the information below the composer",
                "选择并排序输入框下方显示的信息",
            ),
            keywords: "statusline footer model branch context 状态栏 模型 分支 上下文",
            value: ConfigEditor::StatusLine.value(tui, status),
        },
        ConfigSetting {
            section: ConfigSection::Appearance,
            label: crate::i18n::text(language, "Terminal title display", "终端标题内容"),
            description: crate::i18n::text(
                language,
                "Choose and reorder the information in your terminal tab or window title",
                "选择并排序终端标签页或窗口标题中的信息",
            ),
            keywords: "title tab window name 标题 标签页 窗口 名称",
            value: ConfigEditor::TerminalTitle.value(tui, status),
        },
        ConfigSetting {
            section: ConfigSection::Notifications,
            label: crate::i18n::text(language, "Notifications", "通知"),
            description: crate::i18n::text(
                language,
                "Let your terminal notify you when a turn finishes or approval is needed",
                "任务完成或需要审批时由终端提醒你",
            ),
            keywords: "alerts bell completion approval 提醒 铃声 完成 审批",
            value: if status.custom_notification_filters {
                ConfigSettingValue::Unavailable {
                    reason: crate::i18n::text(
                        language,
                        "Event-specific notifications are configured in config.toml",
                        "按事件区分的通知已在 config.toml 中配置",
                    ),
                }
            } else {
                ConfigSettingValue::Toggle {
                    setting: ConfigToggle::Notifications,
                    enabled: notifications_enabled,
                }
            },
        },
        ConfigSetting {
            section: ConfigSection::Notifications,
            label: crate::i18n::text(language, "Notify when", "通知时机"),
            description: crate::i18n::text(
                language,
                "Notify only while you are away or while this terminal is focused too",
                "仅在离开时通知，或终端处于焦点时也通知",
            ),
            keywords: "focus alerts background always unfocused 焦点 后台 始终 失焦",
            value: if notifications_enabled {
                ConfigEditor::NotificationCondition.value(tui, status)
            } else {
                ConfigSettingValue::Unavailable {
                    reason: crate::i18n::text(
                        language,
                        "Turn on notifications to choose when they appear",
                        "开启通知后才能选择通知时机",
                    ),
                }
            },
        },
        ConfigSetting {
            section: ConfigSection::Notifications,
            label: crate::i18n::text(language, "Test notification", "测试通知"),
            description: crate::i18n::text(
                language,
                "Send in 5 seconds so you can switch apps before the notification appears",
                "5 秒后发送，方便你在通知出现前切换应用",
            ),
            keywords: "alerts bell sound test 提醒 铃声 声音 测试",
            value: ConfigSettingValue::NotificationTest,
        },
        ConfigSetting {
            section: ConfigSection::Input,
            label: crate::i18n::text(language, "Keyboard shortcuts", "键盘快捷键"),
            description: crate::i18n::text(
                language,
                "Browse actions and remap their keys",
                "浏览操作并重新绑定按键",
            ),
            keywords: "keymap bindings hotkeys 快捷键 按键 绑定",
            value: ConfigEditor::Keyboard.value(tui, status),
        },
        ConfigSetting {
            section: ConfigSection::Input,
            label: crate::i18n::text(language, "Start in Vim mode", "启动时使用 Vim 模式"),
            description: crate::i18n::text(
                language,
                "Start new composers in Vim mode and use /vim for the current composer",
                "新输入框默认使用 Vim 模式；当前输入框可用 /vim 切换",
            ),
            keywords: "editor modal normal vim 编辑器 模式",
            value: ConfigSettingValue::Toggle {
                setting: ConfigToggle::StartInVimMode,
                enabled: ConfigToggle::StartInVimMode.enabled(tui),
            },
        },
        ConfigSetting {
            section: ConfigSection::Input,
            label: crate::i18n::text(language, "Paste detection", "粘贴检测"),
            description: crate::i18n::text(
                language,
                "Recognize pasted-text bursts and keep multiline input together",
                "识别连续粘贴，并将多行内容保持为一个整体",
            ),
            keywords: "burst clipboard multiline 剪贴板 多行 粘贴",
            value: ConfigSettingValue::Toggle {
                setting: ConfigToggle::PasteDetection,
                enabled: ConfigToggle::PasteDetection.enabled(tui),
            },
        },
        ConfigSetting {
            section: ConfigSection::Sessions,
            label: crate::i18n::text(language, "Automatic recaps", "自动回顾"),
            description: crate::i18n::text(
                language,
                "Prepare recaps while the terminal is unfocused",
                "终端处于后台时准备对话回顾",
            ),
            keywords: "summary background 总结 后台 回顾",
            value: ConfigSettingValue::Toggle {
                setting: ConfigToggle::AutomaticRecaps,
                enabled: ConfigToggle::AutomaticRecaps.enabled(tui),
            },
        },
        ConfigSetting {
            section: ConfigSection::Sessions,
            label: crate::i18n::text(language, "Session picker layout", "会话列表布局"),
            description: crate::i18n::text(
                language,
                "Choose tighter rows or a roomier list when resuming and forking conversations",
                "恢复或分叉会话时选择紧凑或宽松的列表",
            ),
            keywords: "dense comfortable spacing resume fork 紧凑 舒适 间距 恢复 分叉",
            value: ConfigEditor::SessionPickerLayout.value(tui, status),
        },
        ConfigSetting {
            section: ConfigSection::Sessions,
            label: crate::i18n::text(language, "Resume directory", "恢复目录"),
            description: crate::i18n::text(
                language,
                "Choose which working directory to use when the current directory and saved session differ",
                "当前目录与已保存会话不同时，选择恢复使用的工作目录",
            ),
            keywords: "cwd folder path fork ask 目录 路径 分叉 询问",
            value: ConfigEditor::ResumeDirectory.value(tui, status),
        },
        ConfigSetting {
            section: ConfigSection::TextSelection,
            label: crate::i18n::text(language, "Copy on select", "选中即复制"),
            description: crate::i18n::text(
                language,
                "Copy transcript, footer, and composer selections on release; Auto follows terminal defaults",
                "松开鼠标时复制对话、底栏和输入框中的选区；自动会跟随终端默认行为",
            ),
            keywords: "clipboard mouse selection automatic drag 剪贴板 鼠标 选择 拖拽",
            value: ConfigEditor::CopyOnSelect.value(tui, status),
        },
        ConfigSetting {
            section: ConfigSection::TextSelection,
            label: crate::i18n::text(language, "Deselect after copy", "复制后取消选择"),
            description: crate::i18n::text(
                language,
                "Remove read-only transcript and footer highlights after automatic copy succeeds",
                "自动复制成功后清除对话和底栏中的只读高亮",
            ),
            keywords: "clipboard deselect clear highlight automatic mouse selection 剪贴板 取消选择 高亮 自动复制",
            value: ConfigSettingValue::DependentToggle {
                setting: ConfigToggle::CopyOnSelectClearSelection,
                enabled: ConfigToggle::CopyOnSelectClearSelection.enabled(tui),
                dependency_met: status.copy_on_select,
                unavailable_reason: crate::i18n::text(
                    language,
                    "Turn on Copy on select to deselect after copying",
                    "开启“选中即复制”后才能自动取消选择",
                ),
            },
        },
        ConfigSetting {
            section: ConfigSection::TextSelection,
            label: crate::i18n::text(language, "Right-click paste", "右键粘贴"),
            description: status
                .right_click_paste
                .err()
                .map(|reason| right_click_paste_unavailable(language, reason))
                .unwrap_or_else(|| right_click_paste_description(language)),
            keywords: "clipboard mouse paste automatic 剪贴板 鼠标 粘贴 自动",
            value: ConfigEditor::RightClickPaste.value(tui, status),
        },
    ]
}

pub(crate) struct ConfigPanel {
    settings: Vec<ConfigSetting>,
    picker: ListSelectionView,
    saving: bool,
    editor_opening: Option<ConfigEditorHandoff>,
    app_event_tx: AppEventSender,
    language: TuiLanguage,
}

impl ConfigPanel {
    pub(crate) fn new(
        tui: &Tui,
        app_event_tx: AppEventSender,
        keymap: ListKeymap,
        status: ConfigPanelStatus,
    ) -> Self {
        let settings = settings(tui, status);
        let picker = render::picker(
            &settings,
            app_event_tx.clone(),
            keymap,
            PickerState::default(),
            tui.language,
        );
        Self {
            settings,
            picker,
            saving: false,
            editor_opening: None,
            app_event_tx,
            language: tui.language,
        }
    }
}
