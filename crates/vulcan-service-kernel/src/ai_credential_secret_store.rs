//! OS credential-store integration for public AI connections.
//! 公共 AI 连接的操作系统凭据库存储集成。

use keyring::{Entry, Error as KeyringError};
use sha2::{Digest, Sha256};
use std::sync::{Mutex, OnceLock};
use vulcan_provider_adapters::{
    CredentialSecretEnvelope, CredentialSecretResolver, CredentialSecretStore,
};

/// Stable operating-system credential service name for AI connections.
/// AI 连接使用的稳定操作系统凭据服务名。
const VULCAN_AI_CREDENTIAL_SERVICE: &str = "vulcan-code.ai-provider";

/// Stateless secure-store adapter for AI credential references.
/// AI 凭据引用的无状态安全存储适配器。
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OsAiCredentialSecretStore;

impl OsAiCredentialSecretStore {
    /// Restores one exact deleted secret only for transaction rollback.
    /// 仅为事务回滚恢复一个精确的已删除秘密。
    ///
    /// # Parameters
    /// 参数
    /// `secret_reference` must be absent and `secret` is the previously validated envelope.
    /// `secret_reference` 必须不存在，`secret` 是此前已验证的封装。
    ///
    /// # Returns
    /// 返回值
    /// Success after restoring the exact prior secure-store value.
    /// 恢复精确的先前安全存储值后返回成功。
    pub(crate) fn restore_deleted_secret(
        &self,
        secret_reference: &str,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        secret.validate()?;
        let _guard = credential_store_lock()
            .lock()
            .map_err(|_| String::from("The AI credential-store lock is poisoned."))?;
        let entry = ai_secret_entry(secret_reference)?;
        match entry.get_password() {
            Err(KeyringError::NoEntry) => write_secret_unlocked(&entry, secret),
            Ok(_) => Err(format!(
                "AI credential `{secret_reference}` already exists during rollback."
            )),
            Err(_) => Err(format!(
                "AI credential `{secret_reference}` could not be read securely."
            )),
        }
    }
}

impl CredentialSecretResolver for OsAiCredentialSecretStore {
    /// Reads one exact AI connection secret.
    /// 读取一个精确 AI 连接秘密。
    fn read_secret(&self, secret_reference: &str) -> Result<CredentialSecretEnvelope, String> {
        let _guard = credential_store_lock()
            .lock()
            .map_err(|_| String::from("The AI credential-store lock is poisoned."))?;
        read_secret_unlocked(secret_reference)
    }
}

impl CredentialSecretStore for OsAiCredentialSecretStore {
    /// Creates one exact versioned secret without overwriting an existing entry.
    /// 创建一个精确版本化秘密且不覆盖已有条目。
    fn create_secret(
        &self,
        secret_reference: &str,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        secret.validate()?;
        if secret.revision != 1 {
            return Err(String::from(
                "A new AI credential secret must start at revision one.",
            ));
        }
        let _guard = credential_store_lock()
            .lock()
            .map_err(|_| String::from("The AI credential-store lock is poisoned."))?;
        let entry = ai_secret_entry(secret_reference)?;
        match entry.get_password() {
            Ok(_) => {
                return Err(format!(
                    "AI credential `{secret_reference}` is already configured."
                ));
            }
            Err(KeyringError::NoEntry) => {}
            Err(_) => {
                return Err(format!(
                    "AI credential `{secret_reference}` could not be read securely."
                ));
            }
        }
        write_secret_unlocked(&entry, secret)
    }

    /// Replaces one exact secret through revision compare-and-replace.
    /// 通过修订号比较替换一个精确秘密。
    fn replace_secret(
        &self,
        secret_reference: &str,
        expected_revision: u64,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        secret.validate()?;
        let expected_next = expected_revision
            .checked_add(1)
            .ok_or_else(|| String::from("AI credential secret revision is exhausted."))?;
        if secret.revision != expected_next {
            return Err(String::from(
                "Replacement AI credential secret must advance exactly one revision.",
            ));
        }
        let _guard = credential_store_lock()
            .lock()
            .map_err(|_| String::from("The AI credential-store lock is poisoned."))?;
        let current = read_secret_unlocked(secret_reference)?;
        if current.revision != expected_revision {
            return Err(format!(
                "AI credential `{secret_reference}` changed while it was being updated."
            ));
        }
        let entry = ai_secret_entry(secret_reference)?;
        write_secret_unlocked(&entry, secret)
    }

    /// Deletes one exact secret while holding the process-wide store lock.
    /// 在持有进程级存储锁时删除一个精确秘密。
    fn delete_secret(&self, secret_reference: &str) -> Result<(), String> {
        let _guard = credential_store_lock()
            .lock()
            .map_err(|_| String::from("The AI credential-store lock is poisoned."))?;
        match ai_secret_entry(secret_reference)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(_) => Err(String::from("Failed to delete the AI credential securely.")),
        }
    }
}

/// Returns the process-wide compare-and-replace lock for OS credential mutations.
/// 返回操作系统凭据变更使用的进程级比较替换锁。
fn credential_store_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Reads and validates one secret while the caller owns the store lock.
/// 在调用方持有存储锁时读取并校验一个秘密。
fn read_secret_unlocked(secret_reference: &str) -> Result<CredentialSecretEnvelope, String> {
    match ai_secret_entry(secret_reference)?.get_password() {
        Ok(document) => CredentialSecretEnvelope::from_json(document.as_str()),
        Err(KeyringError::NoEntry) => Err(format!(
            "AI credential `{secret_reference}` is not configured."
        )),
        Err(_) => Err(format!(
            "AI credential `{secret_reference}` could not be read securely."
        )),
    }
}

/// Writes one serialized secret while the caller owns the store lock.
/// 在调用方持有存储锁时写入一个序列化秘密。
fn write_secret_unlocked(entry: &Entry, secret: &CredentialSecretEnvelope) -> Result<(), String> {
    let document = secret.to_json()?;
    entry
        .set_password(document.as_str())
        .map_err(|_| String::from("Failed to store the AI credential securely."))
}

/// Builds one collision-resistant OS credential entry from an opaque reference.
/// 根据不透明引用构建一个抗碰撞的操作系统凭据条目。
fn ai_secret_entry(secret_reference: &str) -> Result<Entry, String> {
    if secret_reference.trim().is_empty() {
        return Err(String::from(
            "AI credential secret reference must not be empty.",
        ));
    }
    let mut digest = Sha256::new();
    digest.update(secret_reference.len().to_be_bytes());
    digest.update(secret_reference.as_bytes());
    let account = format!("sha256:{:x}", digest.finalize());
    Entry::new(VULCAN_AI_CREDENTIAL_SERVICE, &account)
        .map_err(|_| String::from("Failed to open the operating-system credential store."))
}
