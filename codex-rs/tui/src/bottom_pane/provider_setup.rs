//! Focused form for adding a third-party model provider profile.

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::BottomPaneView;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::ViewCompletion;
use crate::provider_profiles::ProviderPreset;
use crate::provider_profiles::ProviderProfile;
use crate::provider_profiles::validate_profile_id;
use crate::render::renderable::Renderable;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::Wrap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Id,
    BaseUrl,
    Model,
    ApiKey,
}

impl Field {
    const ALL: [Self; 4] = [Self::Id, Self::BaseUrl, Self::Model, Self::ApiKey];

    fn label(self) -> &'static str {
        match self {
            Self::Id => "配置名称",
            Self::BaseUrl => "API 地址",
            Self::Model => "模型",
            Self::ApiKey => "API Key",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Id => "例如 deepseek-work，只能使用字母、数字、- 和 _",
            Self::BaseUrl => "例如 https://api.deepseek.com/v1",
            Self::Model => "例如 deepseek-chat",
            Self::ApiKey => "输入后加密保存；本地服务可留空",
        }
    }

    fn value_mut<'a>(self, form: &'a mut ProviderSetupForm) -> &'a mut String {
        match self {
            Self::Id => &mut form.id,
            Self::BaseUrl => &mut form.base_url,
            Self::Model => &mut form.model,
            Self::ApiKey => &mut form.api_key,
        }
    }
}

pub(crate) enum ProviderSetupAction {
    None,
    Save(ProviderProfile),
    Cancelled,
}

#[derive(Clone)]
pub(crate) struct ProviderSetupForm {
    preset: ProviderPreset,
    id: String,
    base_url: String,
    model: String,
    api_key: String,
    active: Field,
    error: Option<String>,
}

pub(crate) struct ProviderSetupView {
    form: ProviderSetupForm,
    app_event_tx: AppEventSender,
    complete: Option<ViewCompletion>,
}

impl ProviderSetupView {
    pub(crate) fn new(preset: ProviderPreset, app_event_tx: AppEventSender) -> Self {
        Self {
            form: ProviderSetupForm::new(preset),
            app_event_tx,
            complete: None,
        }
    }
}

impl ProviderSetupForm {
    pub(crate) fn new(preset: ProviderPreset) -> Self {
        Self {
            preset,
            id: preset.default_id().to_string(),
            base_url: preset.default_base_url().to_string(),
            model: preset.default_model().to_string(),
            api_key: String::new(),
            active: Field::Id,
            error: None,
        }
    }

    fn validate_active(&self) -> Result<(), String> {
        match self.active {
            Field::Id => validate_profile_id(&self.id),
            Field::BaseUrl => {
                let url = url::Url::parse(self.base_url.trim())
                    .map_err(|_| "API 地址格式无效".to_string())?;
                if !matches!(url.scheme(), "http" | "https") {
                    return Err("API 地址必须使用 http 或 https".to_string());
                }
                Ok(())
            }
            Field::Model if self.model.trim().is_empty() => Err("模型名称不能为空".to_string()),
            Field::Model => Ok(()),
            Field::ApiKey if self.preset.needs_api_key() && self.api_key.trim().is_empty() => {
                Err("该服务需要 API Key".to_string())
            }
            Field::ApiKey => Ok(()),
        }
    }

    fn submit(&mut self) -> ProviderSetupAction {
        if let Err(error) = self.validate_active() {
            self.error = Some(error);
            return ProviderSetupAction::None;
        }
        let id = self.id.trim().to_string();
        ProviderSetupAction::Save(ProviderProfile {
            id: id.clone(),
            provider_id: self.preset.provider_id(&id),
            display_name: self.preset.name().to_string(),
            base_url: self.base_url.trim().to_string(),
            model: self.model.trim().to_string(),
            wire_api: self.preset.wire_api(),
            api_key: (!self.api_key.trim().is_empty()).then(|| self.api_key.trim().to_string()),
        })
    }

    fn advance(&mut self) -> ProviderSetupAction {
        if let Err(error) = self.validate_active() {
            self.error = Some(error);
            return ProviderSetupAction::None;
        }
        self.error = None;
        let Some(index) = Field::ALL.iter().position(|field| *field == self.active) else {
            return ProviderSetupAction::None;
        };
        if let Some(next) = Field::ALL.get(index + 1) {
            self.active = *next;
            ProviderSetupAction::None
        } else {
            self.submit()
        }
    }

    fn go_back(&mut self) -> ProviderSetupAction {
        self.error = None;
        let Some(index) = Field::ALL.iter().position(|field| *field == self.active) else {
            return ProviderSetupAction::None;
        };
        if let Some(previous) = index.checked_sub(1).and_then(|index| Field::ALL.get(index)) {
            self.active = *previous;
            ProviderSetupAction::None
        } else {
            ProviderSetupAction::Cancelled
        }
    }

    fn active_value_mut(&mut self) -> &mut String {
        self.active.value_mut(self)
    }

    pub(crate) fn handle_key_event(&mut self, key_event: KeyEvent) -> ProviderSetupAction {
        if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return ProviderSetupAction::None;
        }
        match key_event.code {
            KeyCode::Esc => self.go_back(),
            KeyCode::Enter => self.advance(),
            KeyCode::Backspace => {
                self.active_value_mut().pop();
                self.error = None;
                ProviderSetupAction::None
            }
            KeyCode::Char('u') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.active_value_mut().clear();
                self.error = None;
                ProviderSetupAction::None
            }
            KeyCode::Char(character)
                if !key_event.modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) =>
            {
                self.active_value_mut().push(character);
                self.error = None;
                ProviderSetupAction::None
            }
            _ => ProviderSetupAction::None,
        }
    }

    pub(crate) fn handle_paste(&mut self, pasted: String) {
        self.active_value_mut().push_str(pasted.trim());
        self.error = None;
    }

    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        let mut lines = vec![
            Line::from(format!("  {}", self.preset.name()).bold()),
            Line::from(format!("  {}", self.preset.description()).dim()),
            Line::from(""),
        ];
        for field in Field::ALL {
            let selected = field == self.active;
            let value = match field {
                Field::Id => self.id.as_str(),
                Field::BaseUrl => self.base_url.as_str(),
                Field::Model => self.model.as_str(),
                Field::ApiKey if !self.api_key.is_empty() => "••••••••",
                Field::ApiKey => "",
            };
            if selected {
                lines.push(Line::from(vec![
                    "> ".cyan(),
                    field.label().cyan(),
                    ": ".into(),
                    value.to_string().cyan().add_modifier(Modifier::BOLD),
                    "▌".cyan(),
                ]));
                lines.push(Line::from(format!("    {}", field.hint())).dim());
            } else {
                lines.push(Line::from(format!("  {}: {value}", field.label())));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from("  Enter 下一步   Esc 返回   Ctrl+C 退出").dim());
        if let Some(error) = self.error.as_deref() {
            lines.push(Line::from(format!("  {error}")).red());
        }
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL))
            .render(area, buf);
    }
}

impl Renderable for ProviderSetupView {
    fn desired_height(&self, _width: u16) -> u16 {
        15
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.form.render(area, buf);
    }
}

impl BottomPaneView for ProviderSetupView {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match self.form.handle_key_event(key_event) {
            ProviderSetupAction::None => {}
            ProviderSetupAction::Save(profile) => {
                self.app_event_tx
                    .send(AppEvent::SaveProviderProfile(profile));
                self.complete = Some(ViewCompletion::Accepted);
            }
            ProviderSetupAction::Cancelled => {
                self.complete = Some(ViewCompletion::Cancelled);
            }
        }
    }

    fn handle_paste(&mut self, pasted: String) -> bool {
        self.form.handle_paste(pasted);
        true
    }

    fn prefer_esc_to_handle_key_event(&self) -> bool {
        true
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.complete = Some(ViewCompletion::Cancelled);
        CancellationEvent::Handled
    }

    fn is_complete(&self) -> bool {
        self.complete.is_some()
    }

    fn completion(&self) -> Option<ViewCompletion> {
        self.complete
    }
}
