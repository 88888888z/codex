use std::fmt;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use codex_git_utils::get_git_repo_root;
use codex_keyring_store::DefaultKeyringStore;
use codex_keyring_store::KeyringStore;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

mod local;
mod sanitizer;

pub use local::LocalSecretsBackend;
pub use local::LocalSecretsNamespace;
pub use sanitizer::redact_secrets;

const KEYRING_SERVICE: &str = "codex";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SecretName(String);

impl SecretName {
    pub fn new(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        anyhow::ensure!(!trimmed.is_empty(), "secret name must not be empty");
        anyhow::ensure!(
            trimmed
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_'),
            "secret name must contain only A-Z, 0-9, or _"
        );
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for SecretName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Stable encrypted-secret name used by model-provider profiles.
pub fn provider_secret_name(profile: &str) -> anyhow::Result<SecretName> {
    let normalized = profile
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    SecretName::new(&format!("PROVIDER_{normalized}_API_KEY"))
}

/// Persist a provider credential, falling back to a mode-0600 private file
/// when the OS keyring is unavailable (common on headless Linux).
pub fn store_provider_secret(codex_home: &Path, profile: &str, secret: &str) -> Result<()> {
    let name = provider_secret_name(profile)?;
    let manager = SecretsManager::new(codex_home.to_path_buf(), SecretsBackendKind::Local);
    match manager.set(&SecretScope::Global, &name, secret) {
        Ok(()) => {
            let fallback = provider_fallback_secret_path(codex_home, profile)?;
            if fallback.exists() {
                std::fs::remove_file(fallback)?;
            }
            Ok(())
        }
        Err(_) => write_private_secret(codex_home, profile, secret),
    }
}

pub fn load_provider_secret(codex_home: &Path, profile: &str) -> Result<Option<String>> {
    let name = provider_secret_name(profile)?;
    let manager = SecretsManager::new(codex_home.to_path_buf(), SecretsBackendKind::Local);
    if let Ok(Some(secret)) = manager.get(&SecretScope::Global, &name) {
        return Ok(Some(secret));
    }
    let path = provider_fallback_secret_path(codex_home, profile)?;
    match std::fs::read_to_string(path) {
        Ok(secret) => Ok(Some(secret)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn delete_provider_secret(codex_home: &Path, profile: &str) -> Result<()> {
    let name = provider_secret_name(profile)?;
    let manager = SecretsManager::new(codex_home.to_path_buf(), SecretsBackendKind::Local);
    let _ = manager.delete(&SecretScope::Global, &name);
    let path = provider_fallback_secret_path(codex_home, profile)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn provider_fallback_secret_path(codex_home: &Path, profile: &str) -> Result<PathBuf> {
    let name = provider_secret_name(profile)?;
    Ok(codex_home
        .join("secrets")
        .join(format!("{}.key", name.as_str().to_ascii_lowercase())))
}

fn write_private_secret(codex_home: &Path, profile: &str, secret: &str) -> Result<()> {
    let path = provider_fallback_secret_path(codex_home, profile)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(secret.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(temporary, path)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SecretScope {
    Global,
    Environment(String),
}

impl SecretScope {
    pub fn environment(environment_id: impl Into<String>) -> Result<Self> {
        let env_id = environment_id.into();
        let trimmed = env_id.trim();
        anyhow::ensure!(!trimmed.is_empty(), "environment id must not be empty");
        Ok(Self::Environment(trimmed.to_string()))
    }

    pub fn canonical_key(&self, name: &SecretName) -> String {
        // Stable, env-safe identifier used as the on-disk map key.
        match self {
            Self::Global => format!("global/{}", name.as_str()),
            Self::Environment(environment_id) => {
                format!("env/{environment_id}/{}", name.as_str())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretListEntry {
    pub scope: SecretScope,
    pub name: SecretName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum SecretsBackendKind {
    #[default]
    Local,
}

pub trait SecretsBackend: Send + Sync {
    fn set(&self, scope: &SecretScope, name: &SecretName, value: &str) -> Result<()>;
    fn get(&self, scope: &SecretScope, name: &SecretName) -> Result<Option<String>>;
    fn delete(&self, scope: &SecretScope, name: &SecretName) -> Result<bool>;
    fn list(&self, scope_filter: Option<&SecretScope>) -> Result<Vec<SecretListEntry>>;
}

#[derive(Clone)]
pub struct SecretsManager {
    backend: Arc<dyn SecretsBackend>,
}

impl SecretsManager {
    pub fn new(codex_home: PathBuf, backend_kind: SecretsBackendKind) -> Self {
        let backend: Arc<dyn SecretsBackend> = match backend_kind {
            SecretsBackendKind::Local => {
                let keyring_store: Arc<dyn KeyringStore> = Arc::new(DefaultKeyringStore);
                Arc::new(LocalSecretsBackend::new(codex_home, keyring_store))
            }
        };
        Self { backend }
    }

    pub fn new_with_keyring_store(
        codex_home: PathBuf,
        backend_kind: SecretsBackendKind,
        keyring_store: Arc<dyn KeyringStore>,
    ) -> Self {
        let backend: Arc<dyn SecretsBackend> = match backend_kind {
            SecretsBackendKind::Local => {
                Arc::new(LocalSecretsBackend::new(codex_home, keyring_store))
            }
        };
        Self { backend }
    }

    pub fn new_with_keyring_store_and_namespace(
        codex_home: PathBuf,
        backend_kind: SecretsBackendKind,
        keyring_store: Arc<dyn KeyringStore>,
        namespace: LocalSecretsNamespace,
    ) -> Self {
        let backend: Arc<dyn SecretsBackend> = match backend_kind {
            SecretsBackendKind::Local => Arc::new(LocalSecretsBackend::new_with_namespace(
                codex_home,
                keyring_store,
                namespace,
            )),
        };
        Self { backend }
    }

    pub fn set(&self, scope: &SecretScope, name: &SecretName, value: &str) -> Result<()> {
        self.backend.set(scope, name, value)
    }

    pub fn get(&self, scope: &SecretScope, name: &SecretName) -> Result<Option<String>> {
        self.backend.get(scope, name)
    }

    pub fn delete(&self, scope: &SecretScope, name: &SecretName) -> Result<bool> {
        self.backend.delete(scope, name)
    }

    pub fn list(&self, scope_filter: Option<&SecretScope>) -> Result<Vec<SecretListEntry>> {
        self.backend.list(scope_filter)
    }
}

pub fn environment_id_from_cwd(cwd: &Path) -> String {
    if let Some(repo_root) = get_git_repo_root(cwd)
        && let Some(name) = repo_root.file_name()
    {
        let name = name.to_string_lossy().trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }

    let canonical = cwd
        .canonicalize()
        .unwrap_or_else(|_| cwd.to_path_buf())
        .to_string_lossy()
        .into_owned();
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{digest:x}");
    let short = hex.get(..12).unwrap_or(hex.as_str());
    format!("cwd-{short}")
}

/// Computes the OS keyring account name used to store a local namespace's passphrase.
/// Existing namespaces retain their shared key; gateway credentials use an independent key.
pub fn compute_keyring_account(codex_home: &Path, namespace: LocalSecretsNamespace) -> String {
    let canonical = codex_home
        .canonicalize()
        .unwrap_or_else(|_| codex_home.to_path_buf())
        .to_string_lossy()
        .into_owned();
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{digest:x}");
    let short = hex.get(..16).unwrap_or(hex.as_str());
    let home_account = format!("secrets|{short}");
    // Separate keys also prevent concurrent first writes to the gateway and primary
    // stores from overwriting each other's newly generated encryption key.
    match namespace {
        LocalSecretsNamespace::GatewayOAuth => format!("{home_account}|gateway-oauth"),
        LocalSecretsNamespace::ManagedSecrets
        | LocalSecretsNamespace::CodexAuth
        | LocalSecretsNamespace::McpOAuth => home_account,
    }
}

pub(crate) fn keyring_service() -> &'static str {
    KEYRING_SERVICE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::SECRETS_CACHE_TEST_LOCK;
    use codex_keyring_store::tests::MockKeyringStore;
    use pretty_assertions::assert_eq;
    use std::sync::PoisonError;

    #[test]
    fn environment_id_fallback_has_cwd_prefix() {
        let dir = tempfile::tempdir().expect("tempdir");
        let env_id = environment_id_from_cwd(dir.path());
        let canonical = dir
            .path()
            .canonicalize()
            .expect("tempdir canonical path should exist")
            .to_string_lossy()
            .into_owned();
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        let digest = hasher.finalize();
        let hex = format!("{digest:x}");
        let short = hex.get(..12).expect("digest has at least 12 chars");
        assert_eq!(env_id, format!("cwd-{short}"));
    }

    #[test]
    fn manager_round_trips_local_backend() -> Result<()> {
        let _cache_lock = SECRETS_CACHE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let codex_home = tempfile::tempdir().expect("tempdir");
        let keyring = Arc::new(MockKeyringStore::default());
        let manager = SecretsManager::new_with_keyring_store(
            codex_home.path().to_path_buf(),
            SecretsBackendKind::Local,
            keyring,
        );
        let scope = SecretScope::Global;
        let name = SecretName::new("GITHUB_TOKEN")?;

        manager.set(&scope, &name, "token-1")?;
        assert_eq!(manager.get(&scope, &name)?, Some("token-1".to_string()));

        let listed = manager.list(/*scope_filter*/ None)?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, name);

        assert!(manager.delete(&scope, &name)?);
        assert_eq!(manager.get(&scope, &name)?, None);
        Ok(())
    }
}
