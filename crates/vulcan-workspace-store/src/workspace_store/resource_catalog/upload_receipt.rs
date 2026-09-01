//! Immutable private upload receipts attached to existing project resources.
//! 附着于既有项目资源的不可变私有上传回执。

use super::*;
use vulcan_ai_contract::{ResourceKind, ResourceSourceKind};

/// Exact upload facts, while the original model and delivery mode remain owned by the user prompt.
/// 精确上传事实，原模型和投递方式仍由用户提示词持有。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectResourceUploadReceipt {
    /// Original draft intent, when a receipt crosses composer submission; never a file-global cache identity.
    /// 回执跨输入区提交时的原草稿意图；绝不是文件全局缓存身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_intent_id: Option<String>,
    /// Supported receipt schema version.
    /// 受支持的回执结构版本。
    pub version: u32,
    /// Exact provider service entry selected when uploading.
    /// 上传时选择的精确供应商服务入口。
    pub service_entry_id: String,
    /// Exact executable manifest upload channel.
    /// 精确可执行清单上传通道。
    pub upload_channel_id: String,
    /// Adapter-produced `sha256:` contract identity, without normalization.
    /// 适配器产出的`sha256:`契约身份，不进行规范化改写。
    pub upload_contract_fingerprint: String,
    /// Exact canonical media kind uploaded under this contract.
    /// 在本契约下上传的精确规范媒体类型。
    pub resource_kind: ResourceKind,
    /// Exact source MIME, also bound into the receipt fingerprint.
    /// 精确来源MIME，同样绑定进回执指纹。
    pub mime_type: String,
    /// Wire source form produced by the channel, never inferred from the returned string.
    /// 通道产出的线路来源形态，绝不从返回字符串猜测。
    pub source_kind: ResourceSourceKind,
    /// Exact session owner only when the manifest requires session-scoped reuse.
    /// 仅当清单要求会话级复用时保存精确会话所有者。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_scope_id: Option<String>,
    /// Original receipt creation time, never refreshed by reading history.
    /// 原回执创建时间，读取历史绝不刷新。
    pub created_at_unix_millis: u64,
    /// Original expiry bound, never extended by replay.
    /// 原过期边界，重放绝不延长。
    pub expires_at_unix_millis: u64,
}

impl ProviderBinding {
    /// Validates typed receipt fields without leaking private references in errors; legacy bindings remain readable.
    /// 校验强类型回执字段且错误不泄漏私有引用；旧绑定仍可读取。
    /// Returns a catalog error for malformed new receipts and succeeds for valid or legacy bindings.
    /// 新回执损坏时返回目录错误，有效或旧绑定则成功。
    pub(super) fn validate_upload_receipt(&self) -> Result<(), ProjectResourceCatalogError> {
        // Old records remain readable, but fingerprint generation separately requires typed proof.
        // 旧记录仍可读取，但生成指纹另外要求强类型证明。
        let Some(receipt) = &self.upload_receipt else {
            return Ok(());
        };
        // Exact field validation prevents a URL or malformed version from becoming an opaque receipt identity.
        // 精确字段校验防止URL或错误版本成为不透明回执身份。
        if receipt.version != 1
            || receipt
                .draft_intent_id
                .as_deref()
                .is_some_and(|intent| uuid::Uuid::parse_str(intent).is_err())
            || !receipt_mime_matches_kind(receipt.resource_kind, &receipt.mime_type)
            || !matches!(
                receipt.source_kind,
                ResourceSourceKind::ProviderFileId | ResourceSourceKind::RemoteUrl
            )
            || receipt.created_at_unix_millis >= receipt.expires_at_unix_millis
            || !receipt
                .upload_contract_fingerprint
                .strip_prefix("sha256:")
                .is_some_and(is_lowercase_sha256)
            || !valid_identity(&self.provider)
            || !self.connection_id.as_deref().is_some_and(valid_identity)
            || !valid_identity(&receipt.service_entry_id)
            || !valid_identity(&receipt.upload_channel_id)
            || receipt
                .session_scope_id
                .as_deref()
                .is_some_and(|scope| !valid_identity(scope))
            || self.media_id.is_empty()
            || self.media_id.len() > 16_384
            || self.media_id.trim() != self.media_id
            || self.media_id.chars().any(char::is_control)
            || self.media_id.to_ascii_lowercase().starts_with("data:")
        {
            return Err(invalid_receipt("invalid project resource upload receipt"));
        }
        Ok(())
    }

    /// Computes a stable private-receipt digest bound to exact source `sha256` and `size`.
    /// 计算绑定精确源`sha256`和`size`的稳定私有回执摘要。
    /// Returns a lowercase SHA-256 for a verified receipt; old unproven bindings are rejected.
    /// 为已验证回执返回小写SHA-256；拒绝未经证明的旧绑定。
    pub fn upload_receipt_fingerprint(
        &self,
        sha256: &str,
        size: u64,
    ) -> Result<String, ProjectResourceCatalogError> {
        self.validate_upload_receipt()?;
        if self.upload_receipt.is_none() || !is_lowercase_sha256(sha256) {
            return Err(invalid_receipt(
                "upload receipt fingerprint requires verified source and receipt",
            ));
        }
        // A versioned tuple covers all immutable receipt fields and source facts, not just the latest URL.
        // 版本化元组覆盖所有不可变回执字段及源事实，而不只是最近一次URL。
        let bytes = serde_json::to_vec(&(1_u32, sha256, size, self))
            .map_err(|_| invalid_receipt("upload receipt fingerprint serialization failed"))?;
        Ok(sha256_hex(&bytes))
    }

    /// Matches this private binding to exact prompt `original`, source `sha256` and `size`; route and lifetime remain caller checks.
    /// 将本私有绑定匹配到精确提示词`original`及来源`sha256`、`size`；路线和生命周期仍由调用方检查。
    /// Returns false for unknown pending origins and never searches for a newer receipt.
    /// 未知待回执来源返回假，绝不查询更新回执。
    pub fn matches_prompt_upload(
        &self,
        original: &vulcan_session_core::v1::UserPromptResourceDeliveryKind,
        sha256: &str,
        size: u64,
    ) -> bool {
        use vulcan_session_core::v1::UserPromptResourceDeliveryKind;
        // Validation binds the complete receipt and source before an opaque identity can be compared.
        // 在比较不透明身份前，校验绑定完整回执及来源。
        let Ok(fingerprint) = self.upload_receipt_fingerprint(sha256, size) else {
            return false;
        };
        // Fingerprint validation above requires a typed receipt, without any legacy field fallback.
        // 上述指纹校验要求强类型回执，不进行任何旧字段回退。
        let Some(receipt) = &self.upload_receipt else {
            return false;
        };
        match original {
            UserPromptResourceDeliveryKind::Uploaded {
                upload_channel_id,
                upload_contract_fingerprint,
                reference_fingerprint,
            } => {
                receipt.upload_channel_id == *upload_channel_id
                    && receipt.upload_contract_fingerprint == *upload_contract_fingerprint
                    && fingerprint == *reference_fingerprint
            }
            UserPromptResourceDeliveryKind::UploadPending {
                draft_intent_id: Some(intent_id),
                upload_channel_id,
                upload_contract_fingerprint,
            } => {
                receipt.draft_intent_id.as_deref() == Some(intent_id.as_ref())
                    && receipt.upload_channel_id == *upload_channel_id
                    && receipt.upload_contract_fingerprint == *upload_contract_fingerprint
            }
            _ => false,
        }
    }
}

impl ProjectResourceCatalogRegistry {
    /// Atomically appends `binding` to `resource_id` only if the catalog still matches `source_sha256` and `source_size`.
    /// 仅当目录仍匹配`source_sha256`及`source_size`时，把`binding`原子追加至`resource_id`。
    /// `project_dir` is the existing exact project owner; network and source reads must already have completed.
    /// `project_dir`是既有精确项目所有者；网络与源读取必须已完成。
    /// Returns the immutable receipt fingerprint without replacing earlier receipts or allocating a resource id.
    /// 返回不可变回执指纹，不替换更早回执，也不分配资源ID。
    pub fn record_provider_upload_receipt(
        &self,
        project_dir: &Path,
        resource_id: ResourceId,
        source_sha256: &str,
        source_size: u64,
        binding: ProviderBinding,
    ) -> Result<String, ProjectResourceCatalogError> {
        // Fingerprint validation happens before taking the project publication lock.
        // 指纹校验先于项目发布锁获取。
        let fingerprint = binding.upload_receipt_fingerprint(source_sha256, source_size)?;
        // Canonical project identity selects the same existing registry owner used by resource registration.
        // 规范项目身份选择资源注册使用的同一既有注册表所有者。
        let canonical_project_dir = prepare_exact_project_directory(project_dir)?;
        // Registry lookup does not hold any upload or session writer lock.
        // 注册表查询不持有上传锁或会话写入锁。
        let owner = self.project_owner(&canonical_project_dir)?;
        // Serialize only the atomic metadata append for this exact project.
        // 仅串行化此精确项目的原子元数据追加。
        let mut owner_guard =
            owner
                .lock()
                .map_err(|_| ProjectResourceCatalogError::OwnerPoisoned {
                    path: canonical_project_dir.clone(),
                })?;
        if owner_guard.catalog.is_none() {
            owner_guard.catalog = Some(load_project_resource_catalog(&canonical_project_dir)?);
        }
        // The owner-held catalog is the sole mutable authority, not a stale caller snapshot.
        // 所有者持有的目录是唯一可变权威，而非陈旧调用方快照。
        let catalog = owner_guard
            .catalog
            .as_mut()
            .ok_or_else(|| invalid_receipt("project resource owner lost its loaded catalog"))?;
        // A receipt can only bind to an existing exact resource occurrence identity.
        // 回执只能绑定既有精确资源出现身份。
        let entry_index = catalog
            .entries
            .iter()
            .position(|entry| entry.resource_id == resource_id)
            .ok_or_else(|| {
                invalid_receipt("upload receipt refers to an unknown project resource")
            })?;
        // Compare immutable source facts before publication, without loading media under the owner lock.
        // 发布前比较不可变源事实，不在所有者锁内加载媒体。
        let entry = &catalog.entries[entry_index];
        if entry.sha256 != source_sha256 || entry.size != source_size {
            return Err(invalid_receipt(
                "upload receipt source does not match project resource",
            ));
        }
        // Existing append-only merge preserves every first-observed receipt, including repeated uploads of one source.
        // 既有仅追加合并保留每份首次观察到的回执，包括同一来源的多次上传。
        merge_provider_bindings(&canonical_project_dir, catalog, entry_index, &[binding])?;
        Ok(fingerprint)
    }
}

/// Checks a non-secret exact identity `value` without accepting empty or control-containing strings.
/// 检查非秘密精确身份`value`，不接受空串或包含控制字符的字符串。
fn valid_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

/// Rejects conflicting first receipts for one draft intent in `bindings`, including catalog hydration.
/// 拒绝`bindings`中同一草稿意图的冲突首次回执，包含目录补水路径。
/// Exact duplicate values remain idempotent; unrelated intents remain independent.
/// 完全重复值保持幂等；不同意图保持独立。
pub(super) fn validate_draft_intent_receipts(
    bindings: &[ProviderBinding],
) -> Result<(), ProjectResourceCatalogError> {
    // The first binding, not insertion recency, owns each exact intent.
    // 首次绑定而非插入新旧顺序持有每个精确意图。
    let mut intents = std::collections::BTreeMap::new();
    for binding in bindings {
        if let Some(intent) = binding
            .upload_receipt
            .as_ref()
            .and_then(|receipt| receipt.draft_intent_id.as_deref())
            && intents
                .insert(intent, binding)
                .is_some_and(|first| first != binding)
        {
            return Err(invalid_receipt(
                "original draft upload receipt cannot be replaced",
            ));
        }
    }
    Ok(())
}

/// Validates exact `mime` against canonical `kind`, keeping image/audio/video distinct from generic files.
/// 按规范`kind`校验精确`mime`，保持图片、音频、视频与通用文件分离。
fn receipt_mime_matches_kind(kind: ResourceKind, mime: &str) -> bool {
    if mime.len() > 255 || mime.trim() != mime || mime.chars().any(char::is_control) {
        return false;
    }
    // The catalog owns the MIME; validation checks its type/subtype shape without renaming it.
    // 目录持有MIME；校验检查其类型及子类型形态，不对其改名。
    let Some((top_level, subtype)) = mime.split_once('/') else {
        return false;
    };
    if top_level.is_empty() || subtype.is_empty() || subtype.contains('/') {
        return false;
    }
    match kind {
        ResourceKind::Image => top_level.eq_ignore_ascii_case("image"),
        ResourceKind::Audio => top_level.eq_ignore_ascii_case("audio"),
        ResourceKind::Video => top_level.eq_ignore_ascii_case("video"),
        ResourceKind::File => !["image", "audio", "video"]
            .iter()
            .any(|media| top_level.eq_ignore_ascii_case(media)),
    }
}

/// Produces one stable catalog diagnostic `detail` without serializing a receipt or its private reference.
/// 生成稳定目录诊断`detail`，不序列化回执或其私有引用。
fn invalid_receipt(detail: &str) -> ProjectResourceCatalogError {
    ProjectResourceCatalogError::InvalidCatalog {
        detail: detail.to_string(),
    }
}

#[cfg(test)]
mod tests;
