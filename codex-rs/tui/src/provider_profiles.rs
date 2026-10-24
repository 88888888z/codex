//! Third-party model provider profiles and their encrypted API credentials.
//!
//! Each profile is a normal `<name>.config.toml` selected by `--profile`.
//! The active selection is mirrored into the base config so `/provider` can
//! switch the running session and the last choice becomes the default.

use crate::legacy_core::config::CONFIG_PROFILE_V2_SUFFIX;
use crate::legacy_core::config::edit::ConfigEdit;
use crate::legacy_core::config::edit::ConfigEditsBuilder;
use codex_model_provider_info::WireApi;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use toml_edit::Item;
use toml_edit::value;

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

    let edits = base_config_edits(profile, executable, codex_home, has_api_key);
    ConfigEditsBuilder::for_config_path(base_config_path)
        .with_edits(edits)
        .apply()
        .await?;
    Ok(path)
}

pub(crate) async fn activate_profile(
    profile: &ProfileSummary,
    executable: &Path,
    codex_home: &Path,
    base_config_path: &Path,
) -> anyhow::Result<()> {
    let value = toml::from_str::<toml::Value>(&std::fs::read_to_string(&profile.path)?)?;
    let provider = value
        .get("model_providers")
        .and_then(|providers| providers.get(&profile.provider));
    let default_wire = if matches!(profile.provider.as_str(), "ollama" | "lmstudio") {
        "responses"
    } else {
        "chat"
    };
    let wire_api = provider
        .and_then(|provider| provider.get("wire_api"))
        .and_then(toml::Value::as_str)
        .unwrap_or(default_wire);
    let data = ProviderProfile {
        id: profile.id.clone(),
        provider_id: profile.provider.clone(),
        display_name: provider
            .and_then(|provider| provider.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or(&profile.provider)
            .to_string(),
        base_url: provider
            .and_then(|provider| provider.get("base_url"))
            .and_then(toml::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        model: profile.model.clone(),
        wire_api: if wire_api == "chat" {
            WireApi::Chat
        } else {
            WireApi::Responses
        },
        api_key: None,
    };
    let has_auth = provider.is_some_and(|provider| provider.get("auth").is_some());
    ConfigEditsBuilder::for_config_path(base_config_path)
        .with_edits(base_config_edits(&data, executable, codex_home, has_auth))
        .apply()
        .await?;
    Ok(())
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

fn base_config_edits(
    profile: &ProviderProfile,
    executable: &Path,
    codex_home: &Path,
    has_api_key: bool,
) -> Vec<ConfigEdit> {
    let provider_path = |field: &str| {
        vec![
            "model_providers".to_string(),
            profile.provider_id.clone(),
            field.to_string(),
        ]
    };
    let auth_path = |field: &str| {
        vec![
            "model_providers".to_string(),
            profile.provider_id.clone(),
            "auth".to_string(),
            field.to_string(),
        ]
    };
    let mut edits = vec![
        ConfigEdit::SetPath {
            segments: vec!["model".to_string()],
            value: value(profile.model.clone()),
        },
        ConfigEdit::SetPath {
            segments: vec!["model_provider".to_string()],
            value: value(profile.provider_id.clone()),
        },
    ];

    if profile.provider_id.starts_with("codex-") {
        edits.extend([
            ConfigEdit::SetPath {
                segments: provider_path("name"),
                value: value(profile.display_name.clone()),
            },
            ConfigEdit::SetPath {
                segments: provider_path("base_url"),
                value: value(profile.base_url.trim_end_matches('/').to_string()),
            },
            ConfigEdit::SetPath {
                segments: provider_path("wire_api"),
                value: value(profile.wire_api.to_string()),
            },
            ConfigEdit::SetPath {
                segments: provider_path("requires_openai_auth"),
                value: value(false),
            },
            ConfigEdit::ClearPath {
                segments: provider_path("env_key"),
            },
            ConfigEdit::ClearPath {
                segments: provider_path("experimental_bearer_token"),
            },
        ]);
    }

    if has_api_key {
        let mut args = toml_edit::Array::new();
        for arg in token_args(&profile.id) {
            args.push(arg);
        }
        edits.extend([
            ConfigEdit::SetPath {
                segments: auth_path("command"),
                value: value(executable.to_string_lossy().into_owned()),
            },
            ConfigEdit::SetPath {
                segments: auth_path("args"),
                value: Item::Value(args.into()),
            },
            ConfigEdit::SetPath {
                segments: auth_path("timeout_ms"),
                value: value(5_000_i64),
            },
            ConfigEdit::SetPath {
                segments: auth_path("refresh_interval_ms"),
                value: value(0_i64),
            },
            ConfigEdit::SetPath {
                segments: auth_path("cwd"),
                value: value(codex_home.to_string_lossy().into_owned()),
            },
        ]);
    } else {
        edits.push(ConfigEdit::ClearPath {
            segments: vec![
                "model_providers".to_string(),
                profile.provider_id.clone(),
                "auth".to_string(),
            ],
        });
    }
    edits
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
