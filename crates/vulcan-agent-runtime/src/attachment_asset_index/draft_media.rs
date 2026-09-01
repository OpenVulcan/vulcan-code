//! Immutable original media preparation scoped to one live composer attachment lease.
//! 限定于一个活动输入区附件租约的不可变原媒体准备。

use super::*;
use vulcan_session_core::v1::{
    ResourceId, UserPromptMediaRoute, UserPromptResourceDelivery, UserPromptResourceDeliveryKind,
};
use vulcan_workspace_store::workspace_store::ProviderBinding;

mod handoff;
pub use handoff::{
    AttachmentDraftMediaHandoff, complete_attachment_draft_upload,
    register_attachment_draft_upload_handoff,
};

/// Original representation selected for a draft attachment; upload completion fills only its initially absent receipt.
/// 为草稿附件选择的原表示形式；上传完成只填充最初缺失的回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AttachmentDraftMediaDelivery {
    /// Original bytes were selected for inline delivery before any subsequent model switch.
    /// 在任何后续模型切换之前，原始字节被选择为内联投递。
    Inline,
    /// One frozen upload contract owns this intent, independently of later projection choices.
    /// 一个冻结上传契约持有本意图，独立于后续投影选择。
    Upload {
        /// Exact originally selected upload channel.
        /// 精确原选中上传通道。
        upload_channel_id: String,
        /// Exact prefixed SHA-256 contract identity.
        /// 精确带前缀SHA-256契约身份。
        upload_contract_fingerprint: String,
        /// Original completed private reference; absence means no upload has completed for this intent.
        /// 原已完成私有引用；缺失表示本意图尚无完成上传。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        receipt: Option<ProviderBinding>,
    },
}

/// One original media intention owned by an exact asset and composer lease lifetime, not by the latest model.
/// 由精确资产及输入区租约生命周期而非最新模型持有的一次原媒体意图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentDraftMediaOrigin {
    /// Unique lease-intent identity rejecting completion after remove/re-add of the same asset.
    /// 唯一租约意图身份，拒绝同一资产移除再添加后的迟到完成。
    pub intent_id: String,
    /// Exact original provider, entry, connection, model and protocol.
    /// 精确原供应商、入口、连接、模型及协议。
    pub original_route: UserPromptMediaRoute,
    /// Original managed content digest.
    /// 原托管内容摘要。
    pub source_sha256: String,
    /// Original managed byte length.
    /// 原托管字节长度。
    pub source_byte_len: u64,
    /// Original mode and an optional immutable completed upload receipt.
    /// 原方式及可选不可变已完成上传回执。
    pub delivery: AttachmentDraftMediaDelivery,
}

impl AttachmentDraftMediaOrigin {
    /// Constructs one new intent from canonical `attachment`, frozen `original_route`, and selected `delivery` without reading bytes.
    /// 根据规范`attachment`、冻结`original_route`及选中`delivery`构造一个新意图，不读取字节。
    pub fn new(
        attachment: &ConversationAttachment,
        original_route: UserPromptMediaRoute,
        delivery: AttachmentDraftMediaDelivery,
    ) -> Result<Self, String> {
        // Validate the complete intent before any caller may persist or hand it to a prompt.
        // 在任何调用方可持久化或交接到提示词之前，校验完整意图。
        let origin = Self {
            intent_id: uuid::Uuid::new_v4().to_string(),
            original_route,
            source_sha256: attachment
                .source_sha256
                .clone()
                .ok_or_else(|| String::from("Draft media source has no registered digest."))?,
            source_byte_len: attachment
                .byte_len
                .ok_or_else(|| String::from("Draft media source has no registered byte length."))?,
            delivery,
        };
        origin.validate_attachment(attachment)?;
        Ok(origin)
    }

    /// Converts this exact intent into an ordered prompt occurrence identified by `resource_id`, without exposing its private reference.
    /// 将本精确意图转换为由`resource_id`标识的有序提示词出现，不暴露私有引用。
    /// An unfinished upload retains its selected route and channel as unavailable original media; it never fabricates a receipt or authorizes history reupload.
    /// 未完成上传作为不可用原媒体保留所选路线及通道；绝不伪造回执或授权历史重传。
    pub fn prompt_delivery(
        &self,
        resource_id: ResourceId,
    ) -> Result<UserPromptResourceDelivery, String> {
        // Only a real original receipt can produce the opaque uploaded representation.
        // 只有真实原回执才能产出不透明已上传表示。
        let delivery = match &self.delivery {
            AttachmentDraftMediaDelivery::Inline => UserPromptResourceDeliveryKind::Inline,
            AttachmentDraftMediaDelivery::Upload {
                upload_channel_id,
                upload_contract_fingerprint,
                receipt: Some(receipt),
            } => UserPromptResourceDeliveryKind::Uploaded {
                upload_channel_id: upload_channel_id.clone(),
                upload_contract_fingerprint: upload_contract_fingerprint.clone(),
                reference_fingerprint: receipt
                    .upload_receipt_fingerprint(&self.source_sha256, self.source_byte_len)
                    .map_err(|error| error.to_string())?,
            },
            AttachmentDraftMediaDelivery::Upload {
                upload_channel_id,
                upload_contract_fingerprint,
                receipt: None,
            } => UserPromptResourceDeliveryKind::UploadPending {
                draft_intent_id: Some(self.intent_id.clone().into_boxed_str()),
                upload_channel_id: upload_channel_id.clone(),
                upload_contract_fingerprint: upload_contract_fingerprint.clone(),
            },
        };
        Ok(UserPromptResourceDelivery {
            resource_id,
            original_route: self.original_route.clone(),
            delivery,
        })
    }

    /// Checks immutable source and route fields against exact indexed `record`; errors contain no private reference.
    /// 根据精确索引`record`检查不可变来源及路线字段；错误不包含私有引用。
    fn validate(&self, record: &AttachmentAssetRecord) -> Result<(), String> {
        self.validate_source(
            record.source_sha256.as_deref(),
            record.byte_len,
            &record.kind,
            &record.mime_type,
        )
    }

    /// Validates all immutable source, route and receipt facts against canonical `attachment` without reading bytes.
    /// 根据规范`attachment`校验全部不可变来源、路线及回执事实，不读取字节。
    /// Returns a safe error before project registration if any proof belongs to another source or route.
    /// 任意证明属于其他来源或路线时，在项目注册前返回安全错误。
    pub fn validate_attachment(&self, attachment: &ConversationAttachment) -> Result<(), String> {
        self.validate_source(
            attachment.source_sha256.as_deref(),
            attachment.byte_len,
            &attachment.kind,
            &attachment.mime_type,
        )
    }

    /// Checks the exact `sha256`, `bytes`, `kind` and `mime` supplied by either canonical source owner.
    /// 检查任一规范来源所有者提供的精确`sha256`、`bytes`、`kind`及`mime`。
    /// Both owners share the same validation so prompt handoff cannot bypass index validation.
    /// 两种所有者共享相同校验，使提示词交接无法绕过索引校验。
    fn validate_source(
        &self,
        sha256: Option<&str>,
        bytes: Option<u64>,
        kind: &ConversationAttachmentKind,
        mime: &str,
    ) -> Result<(), String> {
        self.original_route
            .validate()
            .map_err(|error| error.to_string())?;
        if uuid::Uuid::parse_str(&self.intent_id).is_err()
            || self.source_sha256.len() != 64
            || !self
                .source_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || sha256 != Some(self.source_sha256.as_str())
            || bytes != Some(self.source_byte_len)
        {
            return Err(String::from(
                "Draft media intent does not match its immutable asset identity.",
            ));
        }
        if let AttachmentDraftMediaDelivery::Upload {
            upload_channel_id,
            upload_contract_fingerprint,
            receipt,
        } = &self.delivery
        {
            if upload_channel_id.trim().is_empty()
                || upload_channel_id.trim() != upload_channel_id
                || upload_channel_id.chars().any(char::is_control)
                || upload_channel_id.len() > 1024
            {
                return Err(String::from("Draft upload channel identity is invalid."));
            }
            // The declaration fingerprint is exact, not a URL or a guessed alternate contract.
            // 声明指纹是精确身份，而非URL或猜测替代契约。
            let digest = upload_contract_fingerprint
                .strip_prefix("sha256:")
                .ok_or_else(|| String::from("Draft upload contract identity is invalid."))?;
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(String::from("Draft upload contract identity is invalid."));
            }
            if let Some(binding) = receipt {
                binding
                    .upload_receipt_fingerprint(&self.source_sha256, self.source_byte_len)
                    .map_err(|error| error.to_string())?;
                // The private receipt must belong to the exact original route and source type.
                // 私有回执必须属于精确原路线及来源类型。
                let receipt = binding
                    .upload_receipt
                    .as_ref()
                    .ok_or_else(|| String::from("Draft upload has no typed receipt."))?;
                // PDF is a file capability; all other kinds preserve their exact canonical modality.
                // PDF属于文件能力；其他类型保留精确规范模态。
                let resource_kind = match kind {
                    ConversationAttachmentKind::Image => vulcan_ai_contract::ResourceKind::Image,
                    ConversationAttachmentKind::Audio => vulcan_ai_contract::ResourceKind::Audio,
                    ConversationAttachmentKind::Video => vulcan_ai_contract::ResourceKind::Video,
                    ConversationAttachmentKind::Pdf | ConversationAttachmentKind::File => {
                        vulcan_ai_contract::ResourceKind::File
                    }
                };
                if binding.provider != self.original_route.provider_id
                    || receipt
                        .draft_intent_id
                        .as_ref()
                        .is_some_and(|intent| intent != &self.intent_id)
                    || binding.connection_id.as_deref()
                        != Some(self.original_route.connection_id.as_str())
                    || receipt.service_entry_id != self.original_route.service_entry_id
                    || receipt.upload_channel_id != *upload_channel_id
                    || receipt.upload_contract_fingerprint != *upload_contract_fingerprint
                    || receipt.mime_type != mime
                    || receipt.resource_kind != resource_kind
                {
                    return Err(String::from(
                        "Draft upload receipt differs from its original route or source.",
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Loads this exact `attachment`/`scope` original intention from its live lease, returning absence only for unplanned input.
/// 从活动租约加载本精确`attachment`/`scope`原意图，仅对未规划输入返回空。
pub fn load_attachment_draft_media(
    attachment: &ConversationAttachment,
    scope: &str,
) -> Result<Option<AttachmentDraftMediaOrigin>, String> {
    draft_transaction(attachment, scope, |record| {
        Ok(record.draft_media.get(scope).cloned())
    })
}

/// Records `proposed` only if this exact live lease has no original intention; returns the existing immutable original otherwise.
/// 仅当本精确活动租约尚无原意图时记录`proposed`；否则返回既有不可变原意图。
pub fn record_attachment_draft_media(
    attachment: &ConversationAttachment,
    scope: &str,
    proposed: AttachmentDraftMediaOrigin,
) -> Result<AttachmentDraftMediaOrigin, String> {
    draft_transaction(attachment, scope, |record| {
        proposed.validate(record)?;
        if let Some(existing) = record.draft_media.get(scope) {
            return Ok(existing.clone());
        }
        record
            .draft_media
            .insert(scope.to_string(), proposed.clone());
        Ok(proposed)
    })
}

/// Fills the original upload receipt for exact `intent_id`; a released lease, changed intention or replacement receipt fails atomically.
/// 为精确`intent_id`填充原上传回执；租约已释放、意图变化或回执替换均原子失败。
pub fn record_attachment_draft_upload(
    attachment: &ConversationAttachment,
    scope: &str,
    intent_id: &str,
    receipt: ProviderBinding,
) -> Result<AttachmentDraftMediaOrigin, String> {
    draft_transaction(attachment, scope, |record| {
        // Clone the exact original record so failed receipt validation does not mutate the transaction's before-image.
        // 克隆精确原记录，使回执校验失败不会修改事务前镜像。
        let mut original = record
            .draft_media
            .get(scope)
            .cloned()
            .ok_or_else(|| String::from("Draft upload intention is no longer active."))?;
        if original.intent_id != intent_id {
            return Err(String::from(
                "Draft upload belongs to an earlier attachment intention.",
            ));
        }
        match &mut original.delivery {
            AttachmentDraftMediaDelivery::Upload {
                receipt: existing, ..
            } => {
                if existing
                    .as_ref()
                    .is_some_and(|existing| existing != &receipt)
                {
                    return Err(String::from(
                        "Original draft upload receipt cannot be replaced.",
                    ));
                }
                *existing = Some(receipt);
            }
            AttachmentDraftMediaDelivery::Inline => {
                return Err(String::from(
                    "An original inline draft cannot acquire an upload receipt.",
                ));
            }
        }
        original.validate(record)?;
        record
            .draft_media
            .insert(scope.to_string(), original.clone());
        Ok(original)
    })
}

/// Runs `operation` under the existing asset-index transaction owner for exact `attachment` and live draft `scope`.
/// 在既有资产索引事务所有者内，为精确`attachment`及活动草稿`scope`执行`operation`。
/// Persists only changed draft provenance; no bytes, cleanup, or network work is performed here.
/// 只持久化变化的草稿来源；此处不执行字节读取、清理或网络工作。
fn draft_transaction<T>(
    attachment: &ConversationAttachment,
    scope: &str,
    operation: impl FnOnce(&mut AttachmentAssetRecord) -> Result<T, String>,
) -> Result<T, String> {
    asset_media_transaction(attachment, |record, _| {
        validate_live_draft_scope(record, scope)?;
        operation(record)
    })
}

/// Validates the exact live draft scope and its stored provenance before reading or retaining a physical source.
/// 在读取或持有实体来源之前校验精确活动草稿作用域及其存储来源。
fn validate_live_draft_scope(record: &AttachmentAssetRecord, scope: &str) -> Result<(), String> {
    if !scope.starts_with("composer-draft:v1:")
        || scope.chars().any(char::is_control)
        || scope.len() > 1024
    {
        return Err(String::from("Draft media scope is invalid."));
    }
    if !record.staging_refs.iter().any(|lease| lease == scope) {
        return Err(String::from(
            "Draft media source or live lease does not match its authoritative asset.",
        ));
    }
    if let Some(original) = record.draft_media.get(scope) {
        original.validate(record)?;
    }
    Ok(())
}

/// Atomically validates `attachment`, live `scope` and original `intent_id`, then retains the bytes for one physical execution.
/// 原子校验`attachment`、活动`scope`及原`intent_id`，随后为一次实体执行保留字节。
/// Returns the authoritative original intent and a non-cloneable hold; no persistent references or prompt fields change.
/// 返回权威原意图及不可克隆持有；不改变持久引用或提示词字段。
pub fn retain_draft_attachment_source(
    attachment: &ConversationAttachment,
    scope: &str,
    intent_id: &str,
) -> Result<
    (
        AttachmentDraftMediaOrigin,
        super::source_hold::AttachmentSourceHold,
    ),
    String,
> {
    // Keep the shared transaction owner alive before the validated operation increments its physical count.
    // 在已校验操作增加实体计数之前保持共享事务所有者存活。
    let asset_root = resolve_asset_root_from_attachment(attachment)
        .ok_or_else(|| String::from("Draft source has no managed asset root."))?;
    let transaction = attachment_index_transaction_lock(&asset_root)?;
    let (original, asset_id) = asset_media_transaction(attachment, |record, owner| {
        validate_live_draft_scope(record, scope)?;
        let original = record
            .draft_media
            .get(scope)
            .filter(|original| original.intent_id == intent_id)
            .cloned()
            .ok_or_else(|| String::from("Draft source intent or lease is no longer active."))?;
        if !matches!(
            original.delivery,
            AttachmentDraftMediaDelivery::Upload { .. }
        ) {
            return Err(String::from(
                "A physical draft upload requires its original upload intention.",
            ));
        }
        // This changes only process-local counts; unchanged draft metadata cannot fail a later persistent save.
        // 此处只改变进程内计数；未改变的草稿元数据不会在后续持久保存失败。
        owner.retain_source(&record.asset_id)?;
        Ok((original, record.asset_id.clone()))
    })?;
    Ok((
        original,
        super::source_hold::AttachmentSourceHold::new(asset_root, asset_id, transaction),
    ))
}

/// Runs `operation` on the exact canonical `attachment` under the sole asset-index lock; callers enforce live lease or registered handoff authority.
/// 在唯一资产索引锁内对精确规范`attachment`执行`operation`；调用方强制活动租约或已登记交接权威。
/// Returns the operation result only after changed draft/handoff metadata is durably saved, with no source byte or network I/O.
/// 变化的草稿及交接元数据可靠保存后才返回操作结果，不执行源字节或网络I/O。
fn asset_media_transaction<T>(
    attachment: &ConversationAttachment,
    operation: impl FnOnce(
        &mut AttachmentAssetRecord,
        &mut super::source_hold::AttachmentIndexTransaction,
    ) -> Result<T, String>,
) -> Result<T, String> {
    // Managed source identity selects one record, never a filename or a content-only cache key.
    // 托管来源身份选择一个记录，绝不使用文件名或仅内容缓存键。
    let asset_id = attachment
        .asset_id
        .as_deref()
        .ok_or_else(|| String::from("Draft media requires a managed asset identity."))?;
    // The canonical attachment has already been resolved by its project owner before reaching this internal boundary.
    // 规范附件在抵达本内部边界之前已经由其项目所有者解析。
    let asset_root = resolve_asset_root_from_attachment(attachment)
        .ok_or_else(|| String::from("Draft media has no managed asset root."))?;
    // This is the same lock used by lease reconciliation, promotion and cleanup.
    // 此锁与租约协调、提升及清理使用的锁相同。
    let transaction = attachment_index_transaction_lock(&asset_root)?;
    // Keep the guard through complete validation and durable publication.
    // 在完整校验及可靠发布期间保持守卫。
    let mut guard = transaction
        .lock()
        .map_err(|_| String::from("Draft media index owner is unavailable."))?;
    // No alternate index or fallback path is consulted.
    // 不查询其他索引或回退路径。
    let mut document = load_attachment_asset_index_document(&asset_root)?;
    // Exact asset and live scope jointly authorize reading or changing draft provenance.
    // 精确资产及活动作用域共同授权读取或修改草稿来源。
    let record = document
        .assets
        .iter_mut()
        .find(|record| record.asset_id == asset_id)
        .ok_or_else(|| String::from("Draft media asset is not indexed."))?;
    if record.deletion_pending
        || record.purged_at_unix_millis.is_some()
        || record.source_sha256 != attachment.source_sha256
        || record.byte_len != attachment.byte_len
        || record.mime_type != attachment.mime_type
        || record.kind != attachment.kind
        || vulcan_runtime_assets::native_host_path_text(&record.managed_path)
            != vulcan_runtime_assets::native_host_path_text(&attachment.local_path)
    {
        return Err(String::from(
            "Draft media source or live lease does not match its authoritative asset.",
        ));
    }
    // Compare only task-owned draft provenance, leaving all other asset metadata unchanged.
    // 只比较本任务持有的草稿来源，保持全部其他资产元数据不变。
    let before = (
        record.draft_media.clone(),
        record.draft_media_handoffs.clone(),
    );
    // Failed operations return before any durable write.
    // 失败操作在任何可靠写入之前返回。
    let result = operation(record, &mut guard)?;
    if before
        != (
            record.draft_media.clone(),
            record.draft_media_handoffs.clone(),
        )
    {
        save_attachment_asset_index_document(&asset_root, &document)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
