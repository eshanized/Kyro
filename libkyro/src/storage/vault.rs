use super::Database;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const SERVICE_NAME: &str = "com.kyro.agent";
pub const VAULT_SECRET_SENTINEL: &str = "[VAULT_SECRET]";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiKeys {
    pub nvidia: Option<String>,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub openai: Option<String>,
    pub anthropic: Option<String>,
    pub google: Option<String>,
    pub groq: Option<String>,
    pub mistral: Option<String>,
    pub deepseek: Option<String>,
    pub openrouter: Option<String>,
    pub together: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiKeysMetadata {
    pub nvidia_configured: bool,
    pub telegram_configured: bool,
    pub telegram_chat_id: Option<String>,
    pub openai_configured: bool,
    pub anthropic_configured: bool,
    pub google_configured: bool,
    pub groq_configured: bool,
    pub mistral_configured: bool,
    pub deepseek_configured: bool,
    pub openrouter_configured: bool,
    pub together_configured: bool,
}

#[derive(Clone)]
pub struct Vault {
    db: Database,
    force_db_fallback: bool,
}

static MASTER_KEY_CACHE: std::sync::Mutex<Option<[u8; 32]>> = std::sync::Mutex::new(None);

impl Vault {
    pub fn new(db: Database) -> Self {
        Self::with_fallback_option(db, false)
    }

    pub fn new_encrypted_fallback(db: Database) -> Self {
        Self::with_fallback_option(db, true)
    }

    pub fn with_fallback_option(db: Database, force_fallback: bool) -> Self {
        let vault = Self {
            db,
            force_db_fallback: force_fallback,
        };
        if let Err(e) = vault.migrate_plaintext_secrets() {
            log::warn!("Vault migration warning: {}", e);
        }
        if let Err(e) = vault.migrate_mcp_server_secrets() {
            log::warn!("Vault MCP secret migration warning: {}", e);
        }
        vault
    }

    /// Clears any cached in-memory master key (primarily for test isolation)
    pub fn clear_master_key_cache() {
        if let Ok(mut lock) = MASTER_KEY_CACHE.lock() {
            *lock = None;
        }
    }

    /// Verifies whether the OS credential service is genuinely functional
    /// by performing a live write, read, verification, and deletion of a probe key.
    pub fn is_keyring_available() -> bool {
        let probe_key = "__kyro_keyring_health_probe__";
        let probe_val = "probe_token_kyro_health_check_live";

        match keyring::Entry::new(SERVICE_NAME, probe_key) {
            Ok(entry) => {
                if entry.set_password(probe_val).is_ok() {
                    let matches = entry.get_password().ok().as_deref() == Some(probe_val);
                    let _ = entry.delete_credential();
                    matches
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    /// Obtains a genuinely secret 256-bit encryption key for database-backed fallback:
    ///
    /// 1. KYRO_VAULT_KEY or KYRO_MASTER_KEY environment variable if present.
    /// 2. Stored key in OS Keyring under SERVICE_NAME / "vault_master_key".
    /// 3. Securely configured, user-owned key file (~/.config/kyro/.vault_master.key) with 0600 permissions.
    ///
    /// If no secure key protection is available, refuses the insecure fallback.
    fn get_or_create_master_key() -> Result<[u8; 32], String> {
        let mut cache = MASTER_KEY_CACHE
            .lock()
            .map_err(|e| format!("Master key cache lock poisoned: {}", e))?;
        if let Some(key) = *cache {
            return Ok(key);
        }

        let resolved_key = Self::resolve_master_key()?;
        *cache = Some(resolved_key);
        Ok(resolved_key)
    }

    fn resolve_master_key() -> Result<[u8; 32], String> {
        // 1. Explicit user/admin environment key
        if let Ok(env_key) =
            std::env::var("KYRO_VAULT_KEY").or_else(|_| std::env::var("KYRO_MASTER_KEY"))
        {
            let trimmed = env_key.trim();
            if !trimmed.is_empty() {
                let mut hasher = Sha256::new();
                hasher.update(b"kyro-env-master-key-seed:");
                hasher.update(trimmed.as_bytes());
                let result = hasher.finalize();
                let mut key = [0u8; 32];
                key.copy_from_slice(&result);
                return Ok(key);
            }
        }

        // 2. OS Keyring master key entry
        if let Ok(entry) = keyring::Entry::new(SERVICE_NAME, "vault_master_key") {
            if let Ok(pwd) = entry.get_password() {
                if let Ok(bytes) = BASE64.decode(pwd.trim()) {
                    if bytes.len() == 32 {
                        let mut key = [0u8; 32];
                        key.copy_from_slice(&bytes);
                        return Ok(key);
                    }
                }
            }
            // Generate and save to keyring if keyring is functional
            let mut rand_bytes = [0u8; 32];
            rand::fill(&mut rand_bytes);
            let b64_key = BASE64.encode(rand_bytes);
            if entry.set_password(&b64_key).is_ok() {
                return Ok(rand_bytes);
            }
        }

        // 3. User-restricted file storage with strict mode 0600
        let key_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kyro");
        std::fs::create_dir_all(&key_dir)
            .map_err(|e| format!("Failed to create key directory: {}", e))?;

        let key_file = key_dir.join(".vault_master.key");

        if key_file.exists() {
            #[cfg(unix)]
            {
                let meta = std::fs::metadata(&key_file)
                    .map_err(|e| format!("Failed to inspect keyfile metadata: {}", e))?;
                let mode = meta.permissions().mode();
                if mode & 0o077 != 0 {
                    return Err(format!(
                        "Insecure permissions on master key file '{}' (mode {:o}). Must be 0600.",
                        key_file.display(),
                        mode
                    ));
                }
            }

            let content = std::fs::read_to_string(&key_file)
                .map_err(|e| format!("Failed to read master key file: {}", e))?;
            let bytes = BASE64
                .decode(content.trim())
                .map_err(|e| format!("Invalid base64 key in master key file: {}", e))?;
            if bytes.len() == 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                return Ok(key);
            } else {
                return Err("Master key file does not contain a 256-bit key".into());
            }
        }

        // Generate new key and save with 0600 permissions
        let mut rand_bytes = [0u8; 32];
        rand::fill(&mut rand_bytes);
        let b64_key = BASE64.encode(rand_bytes);

        #[cfg(unix)]
        {
            use std::io::Write;
            let file_res = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&key_file);

            match file_res {
                Ok(mut file) => {
                    file.write_all(b64_key.as_bytes())
                        .map_err(|e| format!("Failed to write master key: {}", e))?;
                    return Ok(rand_bytes);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let content = std::fs::read_to_string(&key_file)
                        .map_err(|e| format!("Failed to read existing master key: {}", e))?;
                    let bytes = BASE64
                        .decode(content.trim())
                        .map_err(|e| format!("Invalid base64 in master key: {}", e))?;
                    if bytes.len() == 32 {
                        let mut key = [0u8; 32];
                        key.copy_from_slice(&bytes);
                        return Ok(key);
                    }
                }
                Err(e) => return Err(format!("Failed to create secure keyfile: {}", e)),
            }
        }

        #[cfg(not(unix))]
        {
            if std::fs::write(&key_file, b64_key.as_bytes()).is_ok() {
                return Ok(rand_bytes);
            }
        }

        Err("Secure master encryption key could not be established. Keyring, env, and 0600 file are unavailable.".into())
    }

    /// Encrypt a secret using AES-256-GCM for the authenticated database fallback
    fn encrypt_value(secret: &str) -> Result<(String, String), String> {
        let key = Self::get_or_create_master_key()?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| format!("Cipher initialization failed: {}", e))?;

        let mut nonce_bytes = [0u8; 12];
        rand::fill(&mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);

        let ciphertext = cipher
            .encrypt(&nonce, secret.as_bytes())
            .map_err(|e| format!("Encryption failure: {}", e))?;

        Ok((BASE64.encode(ciphertext), BASE64.encode(nonce_bytes)))
    }

    /// Decrypt an AES-256-GCM encrypted record
    fn decrypt_value(encrypted_b64: &str, nonce_b64: &str) -> Result<String, String> {
        let key = Self::get_or_create_master_key()?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| format!("Cipher initialization failed: {}", e))?;

        let ciphertext = BASE64
            .decode(encrypted_b64)
            .map_err(|e| format!("Base64 decode ciphertext error: {}", e))?;
        let nonce_bytes = BASE64
            .decode(nonce_b64)
            .map_err(|e| format!("Base64 decode nonce error: {}", e))?;

        let nonce_array: [u8; 12] = nonce_bytes
            .try_into()
            .map_err(|_| "Invalid nonce array length".to_string())?;
        let nonce = Nonce::from(nonce_array);
        let plaintext_bytes = cipher
            .decrypt(&nonce, ciphertext.as_ref())
            .map_err(|e| format!("Decryption failure / authentication tag mismatch: {}", e))?;

        String::from_utf8(plaintext_bytes).map_err(|e| format!("Utf8 decode error: {}", e))
    }

    pub fn set_secret(&self, key: &str, secret: &str) -> Result<(), String> {
        if secret.trim().is_empty() {
            return self.delete_secret(key);
        }

        if !self.force_db_fallback {
            match keyring::Entry::new(SERVICE_NAME, key) {
                Ok(entry) => {
                    if let Ok(()) = entry.set_password(secret) {
                        let _ = self.db.delete_vault_record(key);
                        return Ok(());
                    }
                }
                Err(e) => {
                    log::debug!(
                        "OS Keyring unavailable ({}), using authenticated fallback",
                        e
                    );
                }
            }
        }

        // Secure fallback: Authenticated AES-256-GCM encryption in SQLite
        let (encrypted_data, nonce) = Self::encrypt_value(secret)?;
        let record = super::db::VaultRecord {
            key: key.to_string(),
            encrypted_data,
            nonce,
            updated_at: chrono::Utc::now().timestamp_millis(),
        };

        self.db
            .set_vault_record(&record)
            .map_err(|e| format!("Database vault record error: {}", e))?;

        Ok(())
    }

    pub fn get_secret(&self, key: &str) -> Result<Option<String>, String> {
        if !self.force_db_fallback {
            if let Ok(entry) = keyring::Entry::new(SERVICE_NAME, key) {
                if let Ok(password) = entry.get_password() {
                    return Ok(Some(password));
                }
            }
        }

        // Check authenticated fallback
        match self.db.get_vault_record(key) {
            Ok(Some(record)) => {
                let decrypted = Self::decrypt_value(&record.encrypted_data, &record.nonce)?;
                Ok(Some(decrypted))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(format!("Failed to retrieve vault record: {}", e)),
        }
    }

    pub fn delete_secret(&self, key: &str) -> Result<(), String> {
        if !self.force_db_fallback {
            if let Ok(entry) = keyring::Entry::new(SERVICE_NAME, key) {
                let _ = entry.delete_credential();
            }
        }
        let _ = self.db.delete_vault_record(key);
        Ok(())
    }

    /// Securely store sensitive credentials for a connected account
    pub fn store_account_secret(&self, account_id: &str, secret_json: &str) -> Result<(), String> {
        let key = format!("connector:account:{}", account_id);
        self.set_secret(&key, secret_json)
    }

    /// Retrieve sensitive credentials for a connected account
    pub fn get_account_secret(&self, account_id: &str) -> Result<Option<String>, String> {
        let key = format!("connector:account:{}", account_id);
        self.get_secret(&key)
    }

    /// Delete all credentials associated with a connected account
    pub fn delete_account_secret(&self, account_id: &str) -> Result<(), String> {
        let key = format!("connector:account:{}", account_id);
        self.delete_secret(&key)
    }

    /// Return metadata to UI without exposing raw credentials
    pub fn get_metadata(&self) -> Result<ApiKeysMetadata, String> {
        let nvidia = self.get_secret("nvidia")?;
        let telegram = self.get_secret("telegram_bot_token")?;
        let chat_id = self
            .db
            .get_setting("telegram_chat_id")
            .map_err(|e| e.to_string())?;
        let openai = self.get_secret("openai")?;
        let anthropic = self.get_secret("anthropic")?;
        let google = self.get_secret("google")?;
        let groq = self.get_secret("groq")?;
        let mistral = self.get_secret("mistral")?;
        let deepseek = self.get_secret("deepseek")?;
        let openrouter = self.get_secret("openrouter")?;
        let together = self.get_secret("together")?;

        Ok(ApiKeysMetadata {
            nvidia_configured: nvidia.map(|s| !s.is_empty()).unwrap_or(false),
            telegram_configured: telegram.map(|s| !s.is_empty()).unwrap_or(false),
            telegram_chat_id: chat_id,
            openai_configured: openai.map(|s| !s.is_empty()).unwrap_or(false),
            anthropic_configured: anthropic.map(|s| !s.is_empty()).unwrap_or(false),
            google_configured: google.map(|s| !s.is_empty()).unwrap_or(false),
            groq_configured: groq.map(|s| !s.is_empty()).unwrap_or(false),
            mistral_configured: mistral.map(|s| !s.is_empty()).unwrap_or(false),
            deepseek_configured: deepseek.map(|s| !s.is_empty()).unwrap_or(false),
            openrouter_configured: openrouter.map(|s| !s.is_empty()).unwrap_or(false),
            together_configured: together.map(|s| !s.is_empty()).unwrap_or(false),
        })
    }

    pub fn get_all_keys(&self) -> Result<ApiKeys, String> {
        let nvidia = self.get_secret("nvidia")?;
        let telegram_bot_token = self.get_secret("telegram_bot_token")?;
        let telegram_chat_id = self
            .db
            .get_setting("telegram_chat_id")
            .map_err(|e| e.to_string())?;
        let openai = self.get_secret("openai")?;
        let anthropic = self.get_secret("anthropic")?;
        let google = self.get_secret("google")?;
        let groq = self.get_secret("groq")?;
        let mistral = self.get_secret("mistral")?;
        let deepseek = self.get_secret("deepseek")?;
        let openrouter = self.get_secret("openrouter")?;
        let together = self.get_secret("together")?;

        Ok(ApiKeys {
            nvidia,
            telegram_bot_token,
            telegram_chat_id,
            openai,
            anthropic,
            google,
            groq,
            mistral,
            deepseek,
            openrouter,
            together,
        })
    }

    pub fn save_all_keys(&self, keys: &ApiKeys) -> Result<(), String> {
        if let Some(ref k) = keys.nvidia {
            self.set_secret("nvidia", k)?;
        }
        if let Some(ref k) = keys.telegram_bot_token {
            self.set_secret("telegram_bot_token", k)?;
        }
        if let Some(ref c) = keys.telegram_chat_id {
            self.db
                .set_setting("telegram_chat_id", c)
                .map_err(|e| e.to_string())?;
        }
        if let Some(ref k) = keys.openai {
            self.set_secret("openai", k)?;
        }
        if let Some(ref k) = keys.anthropic {
            self.set_secret("anthropic", k)?;
        }
        if let Some(ref k) = keys.google {
            self.set_secret("google", k)?;
        }
        if let Some(ref k) = keys.groq {
            self.set_secret("groq", k)?;
        }
        if let Some(ref k) = keys.mistral {
            self.set_secret("mistral", k)?;
        }
        if let Some(ref k) = keys.deepseek {
            self.set_secret("deepseek", k)?;
        }
        if let Some(ref k) = keys.openrouter {
            self.set_secret("openrouter", k)?;
        }
        if let Some(ref k) = keys.together {
            self.set_secret("together", k)?;
        }
        Ok(())
    }

    /// Redacts known secret strings from diagnostics, outputs, and errors
    pub fn redact_string(input: &str, secrets: &[&str]) -> String {
        let mut sanitized = input.to_string();
        for s in secrets {
            if !s.is_empty() && s.len() > 3 {
                sanitized = sanitized.replace(s, "[REDACTED_SECRET]");
            }
        }
        sanitized
    }

    /// Check if an environment variable key indicates sensitive data
    pub fn is_secret_env_key(key: &str) -> bool {
        let lower = key.to_lowercase();
        lower.contains("key")
            || lower.contains("token")
            || lower.contains("secret")
            || lower.contains("password")
            || lower.contains("pass")
            || lower.contains("auth")
            || lower.contains("credential")
            || lower.contains("private")
            || lower.contains("signing")
            || lower.contains("access")
            || lower.contains("cert")
            || lower.contains("webhook")
            || lower.contains("database")
            || lower.contains("db_url")
            || lower.contains("dsn")
            || lower.contains("conn_str")
            || lower.contains("connection_string")
    }

    /// Check if an environment variable value indicates credentials or sensitive tokens
    pub fn is_sensitive_env_value(val: &str) -> bool {
        let trimmed = val.trim();
        if trimmed.is_empty() || trimmed == VAULT_SECRET_SENTINEL {
            return false;
        }
        // Check for URI with userinfo e.g. postgres://user:password@host/db or redis://:password@host
        if let Ok(parsed) = reqwest::Url::parse(trimmed) {
            if parsed.password().is_some() || !parsed.username().is_empty() {
                return true;
            }
        }
        if (trimmed.contains("://") || trimmed.starts_with("//")) && trimmed.contains('@') {
            return true;
        }
        if trimmed.starts_with("ghp_")
            || trimmed.starts_with("github_pat_")
            || trimmed.starts_with("sk-")
            || (trimmed.starts_with("ey") && trimmed.contains('.'))
            || trimmed.contains("-----BEGIN")
        {
            return true;
        }
        false
    }

    /// Stable namespaced key for an MCP server's environment variable secret
    pub fn mcp_env_secret_key(server_id: &str, env_key: &str) -> String {
        format!("mcp:env:{}:{}", server_id, env_key)
    }

    /// Securely store an MCP server environment secret in Vault
    pub fn store_mcp_env_secret(
        &self,
        server_id: &str,
        env_key: &str,
        secret_val: &str,
    ) -> Result<(), String> {
        let key = Self::mcp_env_secret_key(server_id, env_key);
        self.set_secret(&key, secret_val)
    }

    /// Retrieve an MCP server environment secret from Vault
    pub fn get_mcp_env_secret(
        &self,
        server_id: &str,
        env_key: &str,
    ) -> Result<Option<String>, String> {
        let key = Self::mcp_env_secret_key(server_id, env_key);
        self.get_secret(&key)
    }

    /// Delete an MCP server environment secret from Vault
    pub fn delete_mcp_env_secret(&self, server_id: &str, env_key: &str) -> Result<(), String> {
        let key = Self::mcp_env_secret_key(server_id, env_key);
        self.delete_secret(&key)
    }

    /// Verifies that an MCP environment secret exists in Vault and is non-empty
    pub fn verify_mcp_env_secret(&self, server_id: &str, env_key: &str) -> bool {
        matches!(self.get_mcp_env_secret(server_id, env_key), Ok(Some(s)) if !s.is_empty())
    }

    /// Migrates any legacy plaintext secrets stored in `mcp_servers.env_json` into the Vault.
    /// Replaces migrated values in `env_json` with `[VAULT_SECRET]` only after verifying readback.
    pub fn migrate_mcp_server_secrets(&self) -> Result<usize, String> {
        let servers = match self.db.list_mcp_servers() {
            Ok(s) => s,
            Err(_) => return Ok(0),
        };

        let mut migrated_count = 0;
        for mut srv in servers {
            let env_map: HashMap<String, String> =
                serde_json::from_str(&srv.env_json).unwrap_or_default();
            let mut modified = false;
            let mut new_env_map = env_map.clone();

            for (k, v) in &env_map {
                if v != VAULT_SECRET_SENTINEL
                    && !v.is_empty()
                    && (Self::is_secret_env_key(k) || Self::is_sensitive_env_value(v))
                {
                    // 1. Store in secure vault
                    self.store_mcp_env_secret(&srv.id, k, v)?;

                    // 2. Verify readback
                    let verified = self.get_mcp_env_secret(&srv.id, k)?;
                    if verified.as_deref() == Some(v.as_str()) {
                        new_env_map.insert(k.clone(), VAULT_SECRET_SENTINEL.to_string());
                        modified = true;
                        migrated_count += 1;
                        log::info!(
                            "Migrated MCP server '{}' env secret '{}' to vault",
                            srv.id,
                            k
                        );
                    } else {
                        return Err(format!(
                            "Verification failed for migrated MCP secret '{}:{}'. Preserving plaintext for recoverability.",
                            srv.id, k
                        ));
                    }
                }
            }

            if modified {
                srv.env_json =
                    serde_json::to_string(&new_env_map).unwrap_or_else(|_| "{}".to_string());
                self.db.create_mcp_server(&srv).map_err(|e| e.to_string())?;
            }
        }

        Ok(migrated_count)
    }

    /// Migrates any legacy plaintext `secret:*` records in `settings` table into secure vault.
    /// Only purges plaintext entries after confirming safe readback.
    pub fn migrate_plaintext_secrets(&self) -> Result<usize, String> {
        let legacy_entries = self
            .db
            .list_settings_keys_with_prefix("secret:")
            .map_err(|e| e.to_string())?;

        let mut migrated_count = 0;
        for (legacy_key, secret_val) in legacy_entries {
            if let Some(clean_key) = legacy_key.strip_prefix("secret:") {
                // 1. Store in secure vault
                self.set_secret(clean_key, &secret_val)?;

                // 2. Verify stored secret can be retrieved and matches
                let verified = self.get_secret(clean_key)?;
                if verified.as_deref() == Some(&secret_val) {
                    // 3. Verified safe -> delete plaintext record from settings
                    self.db
                        .delete_setting(&legacy_key)
                        .map_err(|e| e.to_string())?;
                    migrated_count += 1;
                    log::info!("Migrated and purged legacy plaintext secret: {}", clean_key);
                } else {
                    return Err(format!(
                        "Verification failed for migrated secret '{}'. Plaintext value preserved for safety.",
                        clean_key
                    ));
                }
            }
        }

        Ok(migrated_count)
    }
}
