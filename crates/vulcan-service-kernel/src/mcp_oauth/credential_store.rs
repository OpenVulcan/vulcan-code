//! OS secure-store backend for MCP OAuth credentials.
//! MCP OAuth 凭据的操作系统安全存储后端。
//!
//! Token responses routinely exceed the 2560-byte blob limit of the Windows Credential Manager, so
//! each record is split into chunk entries plus one manifest that carries the chunk count and a
//! digest. A torn or partial record fails the digest and reads as absent, which asks the user to
//! authorize again instead of sending a corrupted token.
//! 令牌响应经常超过 Windows 凭据管理器 2560 字节的数据上限，因此每条记录被拆分为若干分片条目，
//! 外加一个记录分片数量与摘要的清单。撕裂或残缺的记录无法通过摘要校验并被视为不存在，从而要求
//! 用户重新授权，而不是发送损坏的令牌。

use keyring::{Entry, Error as KeyringError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use vulcan_agent_runtime::mcp::{McpOAuthCredentialBackend, McpOAuthCredentialRecord};

/// Secure-store namespace owned by Vulcan Code for MCP OAuth credentials.
/// Vulcan Code 持有的 MCP OAuth 凭据安全存储命名空间。
const MCP_OAUTH_CREDENTIAL_SERVICE: &str = "vulcan-code.mcp-oauth.v1";

/// Characters per chunk; UTF-16 encoding keeps every chunk within the smallest platform blob limit.
/// 每个分片的字符数；按 UTF-16 编码后每个分片仍低于最小平台数据上限。
const CHUNK_CHARS: usize = 1000;

/// Manifest stored under the server's manifest account.
/// 存储在服务器清单账户下的清单。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChunkManifest {
    /// Number of chunk entries holding the serialized record.
    /// 保存序列化记录的分片条目数量。
    chunks: usize,
    /// Hex SHA-256 of the concatenated chunks.
    /// 拼接后分片的十六进制 SHA-256。
    sha256: String,
}

/// Minimal secret-slot store so chunking can be verified without the OS keyring.
/// 最小化的密钥槽位存储，使分片逻辑无需操作系统钥匙串即可验证。
pub(crate) trait SecretSlots: Send + Sync {
    /// Reads one slot, returning `None` when it does not exist.
    /// 读取单个槽位；不存在时返回 `None`。
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    /// Writes one slot.
    /// 写入单个槽位。
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    /// Deletes one slot; a missing slot is not an error.
    /// 删除单个槽位；槽位不存在不视为错误。
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// Platform keyring slots in the MCP OAuth namespace.
/// MCP OAuth 命名空间中的平台钥匙串槽位。
pub(crate) struct KeyringSlots;

impl KeyringSlots {
    /// Opens the keyring entry for one account.
    /// 打开某个账户的钥匙串条目。
    fn entry(account: &str) -> Result<Entry, String> {
        Entry::new(MCP_OAUTH_CREDENTIAL_SERVICE, account)
            .map_err(|_| String::from("Failed to open the MCP OAuth secure-store entry."))
    }
}

impl SecretSlots for KeyringSlots {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match Self::entry(account)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(_) => Err(String::from(
                "Failed to read MCP OAuth credentials securely.",
            )),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        Self::entry(account)?
            .set_password(value)
            .map_err(|_| String::from("Failed to store MCP OAuth credentials securely."))
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(_) => Err(String::from(
                "Failed to delete MCP OAuth credentials securely.",
            )),
        }
    }
}

/// Chunked OAuth credential backend over a secret-slot store.
/// 基于密钥槽位存储的分片 OAuth 凭据后端。
pub(crate) struct ChunkedMcpOAuthCredentialBackend<S> {
    /// Underlying slot store.
    /// 底层槽位存储。
    slots: S,
    /// Serializes multi-entry reads and writes so concurrent refreshes cannot interleave chunks.
    /// 串行化多条目读写，使并发刷新不会交错写入分片。
    guard: Mutex<()>,
}

/// Production backend over the platform keyring.
/// 基于平台钥匙串的生产后端。
pub(crate) type OsMcpOAuthCredentialBackend = ChunkedMcpOAuthCredentialBackend<KeyringSlots>;

impl OsMcpOAuthCredentialBackend {
    /// Builds the platform keyring backend.
    /// 构建平台钥匙串后端。
    pub(crate) fn os() -> Self {
        Self::new(KeyringSlots)
    }
}

impl<S: SecretSlots> ChunkedMcpOAuthCredentialBackend<S> {
    /// Wraps one slot store.
    /// 包装一个槽位存储。
    pub(super) fn new(slots: S) -> Self {
        Self {
            slots,
            guard: Mutex::new(()),
        }
    }

    /// Returns the slot store, for inspection by tests.
    /// 返回槽位存储，供测试检查。
    #[cfg(test)]
    pub(super) fn slots(&self) -> &S {
        &self.slots
    }

    /// Takes the serialization guard; a poisoned guard protects no data of its own.
    /// 获取串行化守卫；中毒的守卫本身不保护任何数据。
    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.guard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Reads the manifest, treating an unreadable one as absent.
    /// 读取清单；无法解析的清单视为不存在。
    fn manifest(&self, server_id: &str) -> Result<Option<ChunkManifest>, String> {
        Ok(self
            .slots
            .get(&manifest_account(server_id))?
            .and_then(|value| serde_json::from_str(&value).ok()))
    }

    /// Deletes chunk entries in `range` for one server.
    /// 删除某服务器在 `range` 范围内的分片条目。
    fn delete_chunks(&self, server_id: &str, range: std::ops::Range<usize>) -> Result<(), String> {
        for index in range {
            self.slots.delete(&chunk_account(server_id, index))?;
        }
        Ok(())
    }
}

impl<S: SecretSlots + 'static> McpOAuthCredentialBackend for ChunkedMcpOAuthCredentialBackend<S> {
    fn load(&self, server_id: &str) -> Result<Option<McpOAuthCredentialRecord>, String> {
        let _guard = self.lock();
        let Some(manifest) = self.manifest(server_id)? else {
            return Ok(None);
        };
        let mut serialized = String::new();
        for index in 0..manifest.chunks {
            let Some(chunk) = self.slots.get(&chunk_account(server_id, index))? else {
                return Ok(None);
            };
            serialized.push_str(&chunk);
        }
        if digest(&serialized) != manifest.sha256 {
            return Ok(None);
        }
        Ok(serde_json::from_str(&serialized).ok())
    }

    fn save(&self, server_id: &str, record: &McpOAuthCredentialRecord) -> Result<(), String> {
        let serialized = serde_json::to_string(record)
            .map_err(|_| String::from("Failed to serialize MCP OAuth credentials."))?;
        let chunks = split_chunks(&serialized);
        let _guard = self.lock();
        let previous = self
            .manifest(server_id)?
            .map_or(0, |manifest| manifest.chunks);
        for (index, chunk) in chunks.iter().enumerate() {
            self.slots.set(&chunk_account(server_id, index), chunk)?;
        }
        let manifest = ChunkManifest {
            chunks: chunks.len(),
            sha256: digest(&serialized),
        };
        let manifest = serde_json::to_string(&manifest)
            .map_err(|_| String::from("Failed to serialize the MCP OAuth manifest."))?;
        self.slots.set(&manifest_account(server_id), &manifest)?;
        // Leftover chunks from a longer previous record are unreachable once the manifest moved on.
        // 新清单写入后，较长旧记录遗留的分片已不可达。
        self.delete_chunks(server_id, chunks.len()..previous)
    }

    fn clear(&self, server_id: &str) -> Result<(), String> {
        let _guard = self.lock();
        let previous = self
            .manifest(server_id)?
            .map_or(0, |manifest| manifest.chunks);
        // Removing the manifest first makes the record absent even if a chunk deletion fails.
        // 先删除清单，即使某个分片删除失败，记录也已不存在。
        self.slots.delete(&manifest_account(server_id))?;
        self.delete_chunks(server_id, 0..previous)
    }
}

/// Account name for one server's manifest.
/// 单个服务器清单的账户名。
fn manifest_account(server_id: &str) -> String {
    format!("manifest:{server_id}")
}

/// Account name for one chunk; the numeric suffix after the last `#` keeps names unique for any server id.
/// 单个分片的账户名；最后一个 `#` 之后的数字后缀使任意服务器标识的账户名保持唯一。
fn chunk_account(server_id: &str, index: usize) -> String {
    format!("chunk:{server_id}#{index}")
}

/// Splits on character boundaries into chunks of at most [`CHUNK_CHARS`] characters.
/// 按字符边界拆分为每片最多 [`CHUNK_CHARS`] 个字符。
fn split_chunks(serialized: &str) -> Vec<String> {
    let characters = serialized.chars().collect::<Vec<_>>();
    characters
        .chunks(CHUNK_CHARS)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

/// Hex SHA-256 of the serialized record.
/// 序列化记录的十六进制 SHA-256。
fn digest(serialized: &str) -> String {
    hex::encode(Sha256::digest(serialized.as_bytes()))
}

#[cfg(test)]
pub(super) mod tests;
