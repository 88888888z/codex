//! Third-party model provider profiles and their encrypted API credentials.
//!
//! Each profile is a normal `<name>.config.toml` selected by `--profile`.
//! The active selection is mirrored into the base config so `/provider` can
//! switch the running session and the last choice becomes the default.

use crate::legacy_core::config::CONFIG_PROFILE_V2_SUFFIX;
use crate::legacy_core::config::edit::ConfigEditsBuilder;
use codex_model_provider_info::WireApi;
use codex_utils_cli::ProfileV2Name;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

const PROVIDER_TOKEN_COMMAND: &str = "provider";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProviderPreset {
    OpenAi,
    OpenRouter,
    DeepSeek,
    Moonshot,
    Groq,
    SiliconFlow,
    Ollama,
    LmStudio,
    CustomChat,
    CustomResponses,
}

impl ProviderPreset {
    pub(crate) const ALL: [Self; 10] = [
        Self::OpenRouter,
        Self::DeepSeek,
        Self::Moonshot,
        Self::Groq,
        Self::SiliconFlow,
        Self::Ollama,
        Self::LmStudio,
        Self::OpenAi,
        Self::CustomChat,
        Self::CustomResponses,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI",
            Self::OpenRouter => "OpenRouter",
            Self::DeepSeek => "DeepSeek",
            Self::Moonshot => "Moonshot",
            Self::Groq => "Groq",
            Self::SiliconFlow => "SiliconFlow",
            Self::Ollama => "Ollama",
            Self::LmStudio => "LM Studio",
            Self::CustomChat => "自定义 Chat Completions API",
            Self::CustomResponses => "自定义 Responses API",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::OpenAi => "ChatGPT 账号或 OpenAI API",
            Self::OpenRouter => "一个密钥访问多家模型",
            Self::DeepSeek => "DeepSeek Chat Completions",
            Self::Moonshot => "Moonshot / Kimi API",
            Self::Groq => "低延迟推理 API",
            Self::SiliconFlow => "硅基流动模型 API",
            Self::Ollama => "本地 Ollama",
            Self::LmStudio => "本地 LM Studio",
            Self::CustomChat => "兼容 /v1/chat/completions 的服务",
            Self::CustomResponses => "兼容 /v1/responses 的服务",
        }
    }

    pub(crate) fn default_id(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::OpenRouter => "openrouter",
            Self::DeepSeek => "deepseek",
            Self::Moonshot => "moonshot",
            Self::Groq => "groq",
            Self::SiliconFlow => "siliconflow",
            Self::Ollama => "ollama",
            Self::LmStudio => "lmstudio",
            Self::CustomChat => "custom-chat",
            Self::CustomResponses => "custom-responses",
        }
    }

    pub(crate) fn default_base_url(self) -> &'static str {
        match self {
            Self::OpenAi => "https://api.openai.com/v1",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::DeepSeek => "https://api.deepseek.com/v1",
            Self::Moonshot => "https://api.moonshot.cn/v1",
            Self::Groq => "https://api.groq.com/openai/v1",
            Self::SiliconFlow => "https://api.siliconflow.cn/v1",
            Self::Ollama => "http://localhost:11434/v1",
            Self::LmStudio => "http://localhost:1234/v1",
            Self::CustomChat | Self::CustomResponses => "",
        }
    }

    pub(crate) fn default_model(self) -> &'static str {
        match self {
            Self::OpenAi => "gpt-5.2",
            Self::OpenRouter
            | Self::DeepSeek
            | Self::Moonshot
            | Self::Groq
            | Self::SiliconFlow
            | Self::Ollama
            | Self::LmStudio
            | Self::CustomChat
            | Self::CustomResponses => "",
        }
    }

    pub(crate) fn wire_api(self) -> WireApi {
        match self {
            Self::OpenAi | Self::Ollama | Self::LmStudio | Self::CustomResponses => {
                WireApi::Responses
            }
            Self::OpenRouter
            | Self::DeepSeek
            | Self::Moonshot
            | Self::Groq
            | Self::SiliconFlow
            | Self::CustomChat => WireApi::Chat,
        }
    }

    pub(crate) fn needs_api_key(self) -> bool {
        !matches!(
            self,
            Self::Ollama | Self::LmStudio | Self::CustomChat | Self::CustomResponses
        )
    }

    pub(crate) fn provider_id(self, profile_id: &str) -> String {
        match self {
            Self::Ollama => "ollama".to_string(),
            Self::LmStudio => "lmstudio".to_string(),
            _ => format!("codex-{profile_id}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProviderProfile {
    pub(crate) id: String,
    pub(crate) provider_id: String,
    pub(crate) display_name: String,
    pub(crate) base_url: String,
    pub(crate) model: String,
    pub(crate) wire_api: WireApi,
    pub(crate) api_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProfileSummary {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) model: String,
    pub(crate) provider: String,
    pub(crate) path: PathBuf,
}

#[derive(Serialize)]
struct ProfileDocument {
    model: String,
    model_provider: String,
    model_providers: BTreeMap<String, ProfileProviderDocument>,
    tui: ProfileTuiDocument,
}

#[derive(Serialize)]
struct ProfileTuiDocument {
    provider_profile: String,
}

#[derive(Serialize)]
struct ProfileProviderDocument {
    name: String,
    base_url: String,
    wire_api: WireApi,
    requires_openai_auth: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth: Option<ProfileAuthDocument>,
}

#[derive(Serialize)]
struct ProfileAuthDocument {
    command: String,
    args: Vec<String>,
    timeout_ms: u64,
    refresh_interval_ms: u64,
    cwd: String,
}

pub(crate) fn validate_profile_id(id: &str) -> Result<(), String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err("配置名称不能为空".to_string());
    }
    if !trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("配置名称只能包含英文字母、数字、连字符和下划线".to_string());
    }
    Ok(())
}

pub(crate) fn profile_path(codex_home: &Path, id: &str) -> PathBuf {
    codex_home.join(format!("{id}{CONFIG_PROFILE_V2_SUFFIX}"))
}

pub(crate) fn active_profile(codex_home: &Path) -> Option<ProfileV2Name> {
    let text = std::fs::read_to_string(codex_home.join(codex_config::CONFIG_TOML_FILE)).ok()?;
    let config = toml::from_str::<toml::Value>(&text).ok()?;
    let profile: ProfileV2Name = config
        .get("tui")
        .and_then(|tui| tui.get("provider_profile"))
        .and_then(toml::Value::as_str)?
        .parse()
        .ok()?;
    profile_path(codex_home, &profile.to_string())
        .is_file()
        .then_some(profile)
}

pub(crate) fn list_profiles(codex_home: &Path) -> Vec<ProfileSummary> {
    let Ok(entries) = std::fs::read_dir(codex_home) else {
        return Vec::new();
    };
    let mut profiles = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let id = path
                .file_name()?
                .to_str()?
                .strip_suffix(CONFIG_PROFILE_V2_SUFFIX)?;
            if id == "config" || validate_profile_id(id).is_err() {
                return None;
            }
            let (value, parse_error) = match std::fs::read_to_string(&path) {
                Ok(text) => match toml::from_str::<toml::Value>(&text) {
                    Ok(value) => (Some(value), None),
                    Err(error) => (None, Some(error.to_string())),
                },
                Err(error) => (None, Some(error.to_string())),
            };
            let provider = value
                .as_ref()
                .and_then(|value| value.get("model_provider"))
                .and_then(toml::Value::as_str)
                .unwrap_or(id)
                .to_string();
            let model = value
                .as_ref()
                .and_then(|value| value.get("model"))
                .and_then(toml::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    parse_error
                        .clone()
                        .unwrap_or_else(|| "配置读取失败".to_string())
                });
            let display_name = value
                .as_ref()
                .and_then(|value| value.get("model_providers"))
                .and_then(|providers| providers.get(&provider))
                .and_then(|provider| provider.get("name"))
                .and_then(toml::Value::as_str)
                .unwrap_or(&provider)
                .to_string();
            Some(ProfileSummary {
                id: id.to_string(),
                display_name,
                model,
                provider,
                path,
            })
        })
        .collect::<Vec<_>>();
    profiles.sort_by(|left, right| left.id.cmp(&right.id));
    profiles
}

pub(crate) async fn save_profile(
    codex_home: &Path,
    profile: &ProviderProfile,
    executable: &Path,
    base_config_path: &Path,
) -> anyhow::Result<PathBuf> {
    validate_profile_id(&profile.id).map_err(anyhow::Error::msg)?;
    anyhow::ensure!(!profile.base_url.trim().is_empty(), "API 地址不能为空");
    anyhow::ensure!(!profile.model.trim().is_empty(), "模型名称不能为空");

    let has_api_key = profile
        .api_key
        .as_deref()
        .is_some_and(|key| !key.trim().is_empty());
    if has_api_key {
        codex_secrets::store_provider_secret(
            codex_home,
            &profile.id,
            profile.api_key.as_deref().unwrap_or_default().trim(),
        )?;
    }

    let document = ProfileDocument {
        model: profile.model.clone(),
        model_provider: profile.provider_id.clone(),
        tui: ProfileTuiDocument {
            provider_profile: profile.id.clone(),
        },
        model_providers: if profile.provider_id.starts_with("codex-") {
            BTreeMap::from([(
                profile.provider_id.clone(),
                ProfileProviderDocument {
                    name: profile.display_name.clone(),
                    base_url: profile.base_url.trim_end_matches('/').to_string(),
                    wire_api: profile.wire_api,
                    requires_openai_auth: false,
                    auth: has_api_key.then(|| ProfileAuthDocument {
                        command: executable.to_string_lossy().into_owned(),
                        args: token_args(&profile.id),
                        timeout_ms: 5_000,
                        refresh_interval_ms: 0,
                        cwd: codex_home.to_string_lossy().into_owned(),
                    }),
                },
            )])
        } else {
            BTreeMap::new()
        },
    };
    let path = profile_path(codex_home, &profile.id);
    write_private_file(&path, toml::to_string_pretty(&document)?.as_bytes())?;

    write_active_profile(base_config_path, &profile.id).await?;
    Ok(path)
}

pub(crate) async fn activate_profile(
    profile: &ProfileSummary,
    base_config_path: &Path,
) -> anyhow::Result<()> {
    write_active_profile(base_config_path, &profile.id).await
}

pub(crate) fn delete_profile(codex_home: &Path, profile: &ProfileSummary) -> anyhow::Result<()> {
    std::fs::remove_file(&profile.path)?;
    codex_secrets::delete_provider_secret(codex_home, &profile.id)?;
    Ok(())
}

fn token_args(profile_id: &str) -> Vec<String> {
    vec![
        PROVIDER_TOKEN_COMMAND.to_string(),
        "token".to_string(),
        "--profile".to_string(),
        profile_id.to_string(),
    ]
}

async fn write_active_profile(base_config_path: &Path, profile_id: &str) -> anyhow::Result<()> {
    ConfigEditsBuilder::for_config_path(base_config_path)
        .with_edits([crate::legacy_core::config::edit::ConfigEdit::SetPath {
            segments: vec!["tui".to_string(), "provider_profile".to_string()],
            value: toml_edit::value(profile_id),
        }])
        .apply()
        .await?;
    Ok(())
}

fn write_private_file(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(temporary, path)?;
    Ok(())
}
