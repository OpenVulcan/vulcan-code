//! Durable provider-media references owned by the uncompressed session window.
//! 由未压缩会话窗口持有的可持久化供应商媒体引用。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_context::{SessionConversationPayload, SessionConversationState};

/// Schema version for the session media-reference ledger.
/// 会话媒体引用账本的 Schema 版本。
pub const SESSION_MEDIA_REFERENCE_LEDGER_SCHEMA_VERSION: u32 = 1;

/// Defines the smallest route scope that may reuse one provider-owned media reference.
/// 定义可以复用供应商托管媒体引用的最小路由作用域。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMediaReferenceScope {
    /// Reuse across models and credential connections of one provider service and upload contract.
    /// 在同一供应商服务与上传契约内跨模型、跨凭据连接复用。
    #[default]
    ProviderProtocol,
    /// Reuse only while the exact credential connection remains selected.
    /// 仅在精确凭据连接保持选中时复用。
    Credential,
    /// Reuse only inside the owning session.
    /// 仅在所属会话内部复用。
    Session,
}

impl SessionMediaReferenceScope {
    /// Returns the stable serialized scope label used by diagnostics and key construction.
    /// 返回用于诊断与键构造的稳定序列化作用域标签。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProviderProtocol => "provider_protocol",
            Self::Credential => "credential",
            Self::Session => "session",
        }
    }
}

/// Stores one immutable provider-owned reference for one source byte revision.
/// 保存一个对应单一源字节修订的不可变供应商托管引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMediaReferenceEntry {
    /// Content-addressed SHA-256 digest of the exact local source bytes.
    /// 精确本地源字节的内容寻址 SHA-256 摘要。
    pub source_sha256: String,
    /// Byte length covered by the source digest.
    /// 源摘要覆盖的字节长度。
    pub source_byte_len: u64,
    /// Provider identity that owns the reference.
    /// 拥有该引用的供应商标识。
    pub provider_id: String,
    /// Service/API entry identity that owns the upload endpoint.
    /// 拥有上传端点的服务/API 入口标识。
    pub service_entry_id: String,
    /// Manifest upload-channel identity used to create the reference.
    /// 创建该引用时使用的 Manifest 上传通道标识。
    pub upload_channel_id: String,
    /// Content fingerprint of the upload contract semantics.
    /// 上传契约语义的内容指纹。
    pub upload_contract_fingerprint: String,
    /// Explicit reuse boundary declared by the upload channel.
    /// 上传通道声明的显式复用边界。
    #[serde(default)]
    pub reference_scope: SessionMediaReferenceScope,
    /// Exact credential connection required by credential-scoped references.
    /// 凭据作用域引用所要求的精确凭据连接。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Exact session required by session-scoped references.
    /// 会话作用域引用所要求的精确会话。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Canonical resource-kind label, normally `video` for durable uploads.
    /// 规范资源类型标签，持久化上传通常为 `video`。
    pub resource_kind: String,
    /// Exact normalized MIME type accepted by the upload channel.
    /// 上传通道接受的精确归一化 MIME 类型。
    pub mime_type: String,
    /// Provider-native URI or URL placed into the current request projection.
    /// 当前请求投影中使用的供应商原生 URI 或 URL。
    pub provider_reference: String,
    /// Unix milliseconds when the provider reference was created.
    /// 供应商引用创建时间的 Unix 毫秒值。
    pub created_at_unix_millis: u64,
    /// Unix milliseconds after which the provider reference must not be reused.
    /// 超过该 Unix 毫秒值后不得复用供应商引用。
    pub expires_at_unix_millis: u64,
    /// Canonical user-message identities that still retain this media in the active window.
    /// 仍在活动窗口中保留该媒体的规范用户消息标识集合。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub owner_message_ids: BTreeSet<String>,
}

impl SessionMediaReferenceEntry {
    /// Builds a deterministic ledger key without including the provider URL or secret material.
    /// 构造不含供应商 URL 与秘密材料的确定性账本键。
    pub fn ledger_key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.source_sha256,
            self.source_byte_len,
            self.provider_id,
            self.service_entry_id,
            self.upload_channel_id,
            self.upload_contract_fingerprint,
            self.reference_scope.as_str(),
            self.connection_id.as_deref().unwrap_or_default(),
            self.session_id.as_deref().unwrap_or_default(),
            self.resource_kind,
            self.mime_type,
        )
    }
}

/// Holds provider media references that are still reachable from the uncompressed conversation.
/// 保存仍可从未压缩会话访问的供应商媒体引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMediaReferenceLedger {
    /// Version of the persisted ledger shape.
    /// 持久化账本结构的版本。
    #[serde(default = "default_media_reference_ledger_schema_version")]
    pub schema_version: u32,
    /// References keyed by their provider-neutral content identity.
    /// 按供应商无关内容身份索引的引用集合。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub entries: BTreeMap<String, SessionMediaReferenceEntry>,
}

impl Default for SessionMediaReferenceLedger {
    /// Creates an empty ledger at the current schema version.
    /// 创建当前 Schema 版本下的空账本。
    fn default() -> Self {
        Self {
            schema_version: SESSION_MEDIA_REFERENCE_LEDGER_SCHEMA_VERSION,
            entries: BTreeMap::new(),
        }
    }
}

impl SessionMediaReferenceLedger {
    /// Returns whether the durable ledger has no live entries.
    /// 返回可持久化账本是否没有任何活动条目。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Inserts or replaces one source-bound provider reference.
    /// 插入或替换一个绑定源文件的供应商引用。
    pub fn upsert(&mut self, mut entry: SessionMediaReferenceEntry) {
        let key = entry.ledger_key();
        if let Some(existing) = self.entries.get(&key) {
            // ownerUnion prevents a later replay of the same source from forgetting an older
            // retained user message that still points at this provider object.
            // ownerUnion 防止同一源文件后续回放忘记仍指向该供应商对象的旧用户消息。
            entry
                .owner_message_ids
                .extend(existing.owner_message_ids.iter().cloned());
        }
        self.entries.insert(key, entry);
    }

    /// Finds one retained reference that matches the exact provider upload contract, regardless of provider TTL metadata.
    /// 查找一条匹配精确供应商上传契约的保留引用，不因供应商 TTL 元数据到期而失效。
    #[allow(clippy::too_many_arguments)]
    pub fn find_usable(
        &self,
        source_sha256: &str,
        source_byte_len: u64,
        provider_id: &str,
        service_entry_id: &str,
        upload_channel_id: &str,
        upload_contract_fingerprint: &str,
        reference_scope: SessionMediaReferenceScope,
        connection_id: &str,
        session_id: Option<&str>,
        resource_kind: &str,
        mime_type: &str,
        _now_unix_millis: u64,
    ) -> Option<&SessionMediaReferenceEntry> {
        self.entries.values().find(|entry| {
            entry.source_sha256 == source_sha256
                && entry.source_byte_len == source_byte_len
                && entry.provider_id == provider_id
                && entry.service_entry_id == service_entry_id
                && entry.upload_channel_id == upload_channel_id
                && entry.upload_contract_fingerprint == upload_contract_fingerprint
                && entry.reference_scope == reference_scope
                && entry.resource_kind == resource_kind
                && entry.mime_type == mime_type
                && media_reference_entry_is_well_formed(entry)
                && match reference_scope {
                    SessionMediaReferenceScope::ProviderProtocol => {
                        entry.connection_id.is_none() && entry.session_id.is_none()
                    }
                    SessionMediaReferenceScope::Credential => {
                        entry.connection_id.as_deref() == Some(connection_id)
                            && entry.session_id.is_none()
                    }
                    SessionMediaReferenceScope::Session => {
                        session_id.is_some()
                            && entry.session_id.as_deref() == session_id
                            && entry.connection_id.is_none()
                    }
                }
        })
    }

    /// Retains only references owned by canonical messages still present after compaction.
    /// 仅保留压缩后仍存在的规范消息所拥有的引用。
    pub fn retain_for_conversation(&mut self, conversation_state: &SessionConversationState) {
        self.retain_for_conversation_at(conversation_state, current_unix_millis());
    }

    /// Retains only well-formed references owned by the canonical retained message window.
    /// 仅保留由规范保留消息窗口拥有且结构完整的引用。
    ///
    /// # Parameters
    /// 参数
    /// `conversation_state` is the post-compaction canonical conversation; the timestamp argument
    /// remains part of the migration-safe API but does not expire retained conversation references.
    /// `conversation_state` 是压缩后的规范会话；时间戳参数保留在可迁移 API 中，但不会使会话保留引用过期。
    pub fn retain_for_conversation_at(
        &mut self,
        conversation_state: &SessionConversationState,
        _now_unix_millis: u64,
    ) {
        let active_message_ids = conversation_state
            .messages
            .iter()
            .filter(|message| {
                matches!(message.payload, SessionConversationPayload::UserText { .. })
            })
            .map(|message| message.id.as_str())
            .collect::<BTreeSet<_>>();
        self.entries.retain(|_, entry| {
            media_reference_entry_is_well_formed(entry)
                && entry
                    .owner_message_ids
                    .iter()
                    .any(|owner| active_message_ids.contains(owner.as_str()))
        });
    }
}

/// Validates the non-provider-specific invariants required before a ledger entry can be retained.
/// 校验账本条目在保留前必须满足的供应商无关不变量。
fn media_reference_entry_is_well_formed(entry: &SessionMediaReferenceEntry) -> bool {
    !entry.source_sha256.trim().is_empty()
        && !entry.provider_id.trim().is_empty()
        && !entry.service_entry_id.trim().is_empty()
        && !entry.upload_channel_id.trim().is_empty()
        && !entry.upload_contract_fingerprint.trim().is_empty()
        && !entry.resource_kind.trim().is_empty()
        && !entry.mime_type.trim().is_empty()
        && !entry.provider_reference.trim().is_empty()
        && entry.provider_reference == entry.provider_reference.trim()
        && !entry.provider_reference.chars().any(char::is_control)
        && entry.created_at_unix_millis <= entry.expires_at_unix_millis
        && match entry.reference_scope {
            SessionMediaReferenceScope::ProviderProtocol => {
                entry.connection_id.is_none() && entry.session_id.is_none()
            }
            SessionMediaReferenceScope::Credential => {
                entry
                    .connection_id
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                    && entry.session_id.is_none()
            }
            SessionMediaReferenceScope::Session => {
                entry.connection_id.is_none()
                    && entry
                        .session_id
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
            }
        }
}

/// Returns the current Unix timestamp in milliseconds without allowing clock errors to panic.
/// 返回当前 Unix 毫秒时间戳，时钟错误不会触发 panic。
fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

/// Supplies the default ledger schema version to serde migrations.
/// 为 serde 迁移提供默认账本 Schema 版本。
fn default_media_reference_ledger_schema_version() -> u32 {
    SESSION_MEDIA_REFERENCE_LEDGER_SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_context::{SessionConversationMessage, SessionConversationPayload};

    /// Verifies provider-protocol scope ignores account changes while credential scope does not.
    /// 验证供应商协议作用域忽略账号变化，而凭据作用域不忽略账号变化。
    #[test]
    fn provider_protocol_and_credential_scopes_have_distinct_reuse_boundaries() {
        let mut ledger = SessionMediaReferenceLedger::default();
        ledger.upsert(SessionMediaReferenceEntry {
            source_sha256: String::from("sha256:source"),
            source_byte_len: 42,
            provider_id: String::from("minimax"),
            service_entry_id: String::from("coding-plan"),
            upload_channel_id: String::from("cn.video-file"),
            upload_contract_fingerprint: String::from("sha256:contract"),
            reference_scope: SessionMediaReferenceScope::ProviderProtocol,
            connection_id: None,
            session_id: None,
            resource_kind: String::from("video"),
            mime_type: String::from("video/mp4"),
            provider_reference: String::from("mm_file://file-1"),
            created_at_unix_millis: 1,
            expires_at_unix_millis: 100,
            owner_message_ids: BTreeSet::new(),
        });
        assert!(
            ledger
                .find_usable(
                    "sha256:source",
                    42,
                    "minimax",
                    "coding-plan",
                    "cn.video-file",
                    "sha256:contract",
                    SessionMediaReferenceScope::ProviderProtocol,
                    "account-b",
                    None,
                    "video",
                    "video/mp4",
                    2,
                )
                .is_some()
        );
        assert!(
            ledger
                .find_usable(
                    "sha256:source",
                    42,
                    "minimax",
                    "coding-plan",
                    "cn.video-file",
                    "sha256:contract",
                    SessionMediaReferenceScope::Credential,
                    "account-b",
                    None,
                    "video",
                    "video/mp4",
                    2,
                )
                .is_none()
        );
    }

    /// Verifies compaction retention removes references whose canonical owner was discarded.
    /// 验证压缩保留会移除规范所有者已被丢弃的引用。
    #[test]
    fn retain_for_conversation_keeps_only_active_message_owners() {
        let mut ledger = SessionMediaReferenceLedger::default();
        for (owner, reference) in [("keep", "mm_file://keep"), ("drop", "mm_file://drop")] {
            let mut owners = BTreeSet::new();
            owners.insert(owner.to_string());
            ledger.upsert(SessionMediaReferenceEntry {
                source_sha256: owner.to_string(),
                source_byte_len: 1,
                provider_id: String::from("minimax"),
                service_entry_id: String::from("coding-plan"),
                upload_channel_id: String::from("cn.video-file"),
                upload_contract_fingerprint: String::from("contract"),
                reference_scope: SessionMediaReferenceScope::ProviderProtocol,
                connection_id: None,
                session_id: None,
                resource_kind: String::from("video"),
                mime_type: String::from("video/mp4"),
                provider_reference: reference.to_string(),
                created_at_unix_millis: 1,
                expires_at_unix_millis: 100,
                owner_message_ids: owners,
            });
        }
        let conversation_state = SessionConversationState {
            messages: vec![SessionConversationMessage {
                id: String::from("keep"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("keep"),
                    attachments: Vec::new(),
                },
            }],
            ..SessionConversationState::default()
        };
        ledger.retain_for_conversation_at(&conversation_state, 2);
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(
            ledger
                .entries
                .values()
                .next()
                .map(|entry| entry.provider_reference.as_str()),
            Some("mm_file://keep")
        );
    }

    /// Verifies expiry and malformed scope metadata are removed with discarded owners.
    /// 验证过期条目与结构错误的作用域元数据会随无效所有者一起移除。
    #[test]
    fn retain_for_conversation_keeps_expired_and_prunes_malformed_entries() {
        let mut ledger = SessionMediaReferenceLedger::default();
        let mut owners = BTreeSet::new();
        owners.insert(String::from("keep"));
        ledger.upsert(SessionMediaReferenceEntry {
            source_sha256: String::from("source"),
            source_byte_len: 1,
            provider_id: String::from("provider"),
            service_entry_id: String::from("service"),
            upload_channel_id: String::from("upload"),
            upload_contract_fingerprint: String::from("contract"),
            reference_scope: SessionMediaReferenceScope::ProviderProtocol,
            connection_id: Some(String::from("unexpected")),
            session_id: None,
            resource_kind: String::from("video"),
            mime_type: String::from("video/mp4"),
            provider_reference: String::from("mm_file://malformed"),
            created_at_unix_millis: 1,
            expires_at_unix_millis: 100,
            owner_message_ids: owners.clone(),
        });
        ledger.upsert(SessionMediaReferenceEntry {
            source_sha256: String::from("expired"),
            source_byte_len: 1,
            provider_id: String::from("provider"),
            service_entry_id: String::from("service"),
            upload_channel_id: String::from("upload"),
            upload_contract_fingerprint: String::from("contract"),
            reference_scope: SessionMediaReferenceScope::ProviderProtocol,
            connection_id: None,
            session_id: None,
            resource_kind: String::from("video"),
            mime_type: String::from("video/mp4"),
            provider_reference: String::from("mm_file://expired"),
            created_at_unix_millis: 1,
            expires_at_unix_millis: 2,
            owner_message_ids: owners,
        });
        let conversation_state = SessionConversationState {
            messages: vec![SessionConversationMessage {
                id: String::from("keep"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("keep"),
                    attachments: Vec::new(),
                },
            }],
            ..SessionConversationState::default()
        };
        ledger.retain_for_conversation_at(&conversation_state, 3);
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(
            ledger
                .entries
                .values()
                .next()
                .map(|entry| entry.provider_reference.as_str()),
            Some("mm_file://expired")
        );
    }
}
