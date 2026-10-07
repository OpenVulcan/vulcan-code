//! Chunked MCP OAuth credential storage over an in-memory slot store.
//! 基于内存槽位存储的分片 MCP OAuth 凭据存储测试。

use super::*;
use std::collections::BTreeMap;

/// In-memory slot store standing in for the OS keyring.
/// 替代操作系统钥匙串的内存槽位存储。
#[derive(Default)]
pub(crate) struct MemorySlots {
    /// Slots keyed by account.
    /// 以账户为键的槽位。
    values: Mutex<BTreeMap<String, String>>,
}

impl MemorySlots {
    /// Returns the number of stored slots.
    /// 返回已存储槽位数量。
    fn len(&self) -> usize {
        self.values.lock().unwrap().len()
    }

    /// Overwrites one slot directly, bypassing the backend.
    /// 绕过后端直接覆盖单个槽位。
    fn overwrite(&self, account: &str, value: &str) {
        self.values
            .lock()
            .unwrap()
            .insert(account.to_string(), value.to_string());
    }

    /// Removes one slot directly, bypassing the backend.
    /// 绕过后端直接移除单个槽位。
    fn remove(&self, account: &str) {
        self.values.lock().unwrap().remove(account);
    }
}

impl SecretSlots for MemorySlots {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        Ok(self.values.lock().unwrap().get(account).cloned())
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        assert!(
            value.encode_utf16().count() * 2 <= 2560,
            "slot exceeds the Windows credential blob limit"
        );
        self.overwrite(account, value);
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        self.remove(account);
        Ok(())
    }
}

/// Builds a record whose serialized form spans `client_id_len` characters of client id.
/// 构建一条序列化后包含 `client_id_len` 个客户端标识字符的记录。
fn record(client_id_len: usize) -> McpOAuthCredentialRecord {
    McpOAuthCredentialRecord {
        server_url: String::from("https://mcp.example.test/mcp"),
        credentials: rmcp_credentials("x".repeat(client_id_len)),
    }
}

/// Builds token-less stored credentials for one client id.
/// 为单个客户端标识构建不含令牌的已存储凭据。
fn rmcp_credentials(client_id: String) -> vulcan_agent_runtime::mcp::McpOAuthStoredCredentials {
    vulcan_agent_runtime::mcp::McpOAuthStoredCredentials::new(client_id, None, Vec::new(), None)
}

#[test]
fn large_records_round_trip_across_chunks() {
    let backend = ChunkedMcpOAuthCredentialBackend::new(MemorySlots::default());
    let stored = record(CHUNK_CHARS * 3);

    backend.save("server", &stored).unwrap();

    assert!(
        backend.slots().len() > 2,
        "record should span several chunks"
    );
    let loaded = backend.load("server").unwrap().expect("record loads");
    assert_eq!(loaded.credentials.client_id, stored.credentials.client_id);
    assert_eq!(loaded.server_url, stored.server_url);
}

#[test]
fn shorter_saves_remove_leftover_chunks() {
    let backend = ChunkedMcpOAuthCredentialBackend::new(MemorySlots::default());
    backend.save("server", &record(CHUNK_CHARS * 3)).unwrap();

    backend.save("server", &record(10)).unwrap();

    // One manifest plus one chunk remain.
    // 只剩一个清单加一个分片。
    assert_eq!(backend.slots().len(), 2);
    assert!(backend.load("server").unwrap().is_some());
}

#[test]
fn clear_removes_every_slot_and_reads_absent() {
    let backend = ChunkedMcpOAuthCredentialBackend::new(MemorySlots::default());
    backend.save("server", &record(CHUNK_CHARS * 2)).unwrap();

    backend.clear("server").unwrap();
    backend.clear("server").unwrap();

    assert_eq!(backend.slots().len(), 0);
    assert!(backend.load("server").unwrap().is_none());
}

#[test]
fn tampered_or_missing_chunks_read_as_absent() {
    let backend = ChunkedMcpOAuthCredentialBackend::new(MemorySlots::default());
    backend.save("server", &record(CHUNK_CHARS * 2)).unwrap();
    backend
        .slots()
        .overwrite(&chunk_account("server", 0), "tampered");
    assert!(backend.load("server").unwrap().is_none());

    backend.save("server", &record(CHUNK_CHARS * 2)).unwrap();
    backend.slots().remove(&chunk_account("server", 1));
    assert!(backend.load("server").unwrap().is_none());
}

#[test]
fn servers_do_not_share_slots() {
    let backend = ChunkedMcpOAuthCredentialBackend::new(MemorySlots::default());
    backend.save("plugin:docs", &record(10)).unwrap();

    assert!(backend.load("plugin").unwrap().is_none());
    backend.clear("plugin").unwrap();
    assert!(backend.load("plugin:docs").unwrap().is_some());
}
