//! Conversation-purpose media admission shared by draft planning and explicit submission.
//! 草稿规划和显式提交共用的主会话用途媒体准入。

use super::{
    ComposerAttachmentDeliveryPlan, ConversationAttachment, ConversationAttachmentDeliveryMode,
    attachment_kind,
};
use std::collections::BTreeMap;
use vulcan_agent_runtime::attachment_asset_index::{
    AttachmentDraftMediaDelivery, AttachmentDraftMediaOrigin,
};
use vulcan_ai_contract::ResourceKind;
use vulcan_provider_adapters::{
    MediaDeliveryDecision, MediaDeliveryOrigin, MediaDeliveryPlanner, MediaDeliveryPolicy,
    MediaDeliveryPurpose, MediaDeliveryTarget, ResolvedPublicAiExecution,
};

/// One chronological explicit-attachment batch using the frozen conversation policy and exact native counts.
/// 使用冻结主会话策略及精确原生数量的一批时序显式附件。
pub(super) struct ComposerMediaDeliveryPlanner {
    /// Shared encoded budget reserved only for admitted inline resources.
    /// 仅为已准入内联资源预留的共享编码预算。
    planner: MediaDeliveryPlanner,
    /// Admitted native occurrences per kind; rejected attachments consume no native slot.
    /// 各类型已准入原生出现次数；被拒附件不消耗原生名额。
    counts: BTreeMap<ResourceKind, u64>,
}

impl ComposerMediaDeliveryPlanner {
    /// Refunds one already-selected upload when its original execution is unavailable; inline byte reservations are never affected.
    /// 原执行不可用时退还一个已选择上传的数量额度；绝不影响内联字节预留。
    /// Parameter: kind is the exact canonical modality just admitted as upload; returns an error for a missing reservation.
    /// 参数：kind 是刚按上传准入的精确规范模态；缺少预留时返回错误。
    pub(super) fn discard_upload_count(&mut self, kind: ResourceKind) -> Result<(), String> {
        let count = self
            .counts
            .get_mut(&kind)
            .ok_or_else(|| String::from("Discarded upload has no native count reservation."))?;
        *count = count
            .checked_sub(1)
            .ok_or_else(|| String::from("Discarded upload native count would underflow."))?;
        Ok(())
    }

    /// Plans one existing project `entry` using its exact resolved `preparation`, frozen execution and current session scope.
    /// 使用精确解析的`preparation`、冻结执行及当前会话作用域，规划一个既有项目`entry`。
    /// `attachment` is catalog-owned metadata; returns a reversible view without reading bytes or scheduling replacement uploads.
    /// `attachment`是目录所有元数据；返回可逆视图，不读取字节或调度替代上传。
    pub(super) fn plan_project(
        &mut self,
        execution: &ResolvedPublicAiExecution,
        attachment: &ConversationAttachment,
        entry: &vulcan_workspace_store::workspace_store::ResourceCatalogEntry,
        preparation: &vulcan_session_core::v1::UserPromptResourcePreparation,
        session_id: &str,
    ) -> Result<(ComposerAttachmentDeliveryPlan, &'static str), String> {
        use vulcan_session_core::v1::{
            UserPromptResourceDeliveryKind, UserPromptResourcePreparation,
        };
        // Only an explicit pending source permits a new first preparation; missing history never does.
        // 只有显式待准备来源允许新的首次准备；缺失历史绝不允许。
        let original = match preparation {
            UserPromptResourcePreparation::Unproven => {
                return Ok((
                    ComposerAttachmentDeliveryPlan::MetadataOnly,
                    "historical_origin_unknown",
                ));
            }
            UserPromptResourcePreparation::Pending => None,
            UserPromptResourcePreparation::Prepared(original) => {
                if original.resource_id != entry.resource_id {
                    return Err(String::from(
                        "Project media source occurrence names another resource.",
                    ));
                }
                if original.delivery == UserPromptResourceDeliveryKind::Attachment {
                    return Ok((
                        ComposerAttachmentDeliveryPlan::MetadataOnly,
                        "original_attachment_only",
                    ));
                }
                if matches!(
                    original.delivery,
                    UserPromptResourceDeliveryKind::Uploaded { .. }
                        | UserPromptResourceDeliveryKind::UploadPending { .. }
                ) && original.original_route != super::composer_media_original_route(execution)
                {
                    return Ok((
                        ComposerAttachmentDeliveryPlan::MetadataOnly,
                        "original_upload_belongs_to_another_model",
                    ));
                }
                if matches!(
                    original.delivery,
                    UserPromptResourceDeliveryKind::UploadPending {
                        draft_intent_id: None,
                        ..
                    }
                ) {
                    return Ok((
                        ComposerAttachmentDeliveryPlan::MetadataOnly,
                        "original_upload_reference_unavailable",
                    ));
                }
                Some(original)
            }
        };
        // Canonical MIME/kind and source size remain independent of original route and soft preferences.
        // 规范MIME、类别及来源大小独立于原路线和软偏好。
        let kind = attachment_kind(attachment.kind.clone());
        // Select only the current exact upload declaration, never another or larger channel.
        // 只选择当前精确上传声明，绝不选择另一或更大的通道。
        let upload = execution.client.conversation_media_upload_contract(
            &execution.route,
            kind,
            &attachment.mime_type,
        )?;
        // Receipt lookup is append-only and keyed by the original opaque fingerprint, never the newest catalog entry.
        // 回执查询面对仅追加集合，并以原不透明指纹定位，绝不选择最新目录条目。
        let origin = match original.map(|original| &original.delivery) {
            None => MediaDeliveryOrigin::NewIntent,
            Some(UserPromptResourceDeliveryKind::Inline) => MediaDeliveryOrigin::HistoricalInline,
            Some(UserPromptResourceDeliveryKind::UploadPending {
                draft_intent_id: None,
                ..
            }) => {
                return Ok((
                    ComposerAttachmentDeliveryPlan::MetadataOnly,
                    "original_upload_reference_unavailable",
                ));
            }
            Some(
                delivery @ (UserPromptResourceDeliveryKind::Uploaded { .. }
                | UserPromptResourceDeliveryKind::UploadPending {
                    draft_intent_id: Some(_),
                    ..
                }),
            ) => {
                // All source/route/lifetime facts must still match before restoring the exact original reference.
                // 恢复精确原引用之前，全部来源、路线及生命周期事实必须仍然匹配。
                let usable = upload.as_ref().is_some_and(|contract| {
                    entry.provider_bindings.iter().any(|binding| {
                        binding.matches_prompt_upload(delivery, &entry.sha256, entry.size)
                            && media_receipt_is_usable(
                                binding,
                                execution,
                                kind,
                                &attachment.mime_type,
                                contract,
                                Some(session_id),
                            )
                    })
                });
                MediaDeliveryOrigin::HistoricalUpload {
                    original_target_matches: true,
                    reference_usable: usable,
                }
            }
            Some(UserPromptResourceDeliveryKind::Attachment) => {
                return Ok((
                    ComposerAttachmentDeliveryPlan::MetadataOnly,
                    "original_attachment_only",
                ));
            }
        };
        // The shared batch planner retains native counts and exact cumulative Base64 reservations.
        // 共享批次规划器保留原生数量及精确累计Base64预留。
        let target = MediaDeliveryTarget {
            profile_id: &execution.route.protocol_route.protocol_profile_id,
            model_inputs: &execution.model.capabilities.input_resources,
            constraint: execution
                .route
                .protocol_route
                .input_resource_constraints
                .iter()
                .find(|constraint| constraint.resource_kind == kind),
            upload: upload.as_ref(),
        };
        self.plan_target_with_origin(
            &target,
            kind,
            &attachment.mime_type,
            entry.size,
            origin,
            false,
        )
    }

    /// Plans exact draft `attachment` against its optional first `original` intention and current frozen execution/session scope.
    /// 根据可选首次`original`意图及当前冻结执行/会话作用域，规划精确草稿`attachment`。
    /// Returns a reversible view only; a different model cannot turn the original upload into a new upload or inline data.
    /// 仅返回可逆视图；不同模型不能把原上传转成新上传或内联数据。
    pub(super) fn plan_draft(
        &mut self,
        execution: &ResolvedPublicAiExecution,
        attachment: &ConversationAttachment,
        original: Option<&AttachmentDraftMediaOrigin>,
        session_id: Option<&str>,
    ) -> Result<(ComposerAttachmentDeliveryPlan, &'static str), String> {
        // Route comparison happens before inspecting the new model's upload declaration.
        // 路线比较发生在检查新模型上传声明之前。
        let current_route = super::composer_media_original_route(execution);
        if original.is_some_and(|original| {
            matches!(
                original.delivery,
                AttachmentDraftMediaDelivery::Upload { .. }
            ) && original.original_route != current_route
        }) {
            return Ok((
                ComposerAttachmentDeliveryPlan::MetadataOnly,
                "original_upload_belongs_to_another_model",
            ));
        }
        // Canonical metadata is required for every native decision.
        // 每个原生决策都要求规范元数据。
        let Some(bytes) = attachment.byte_len else {
            return Ok((
                ComposerAttachmentDeliveryPlan::MetadataOnly,
                "attachment_size_is_unknown",
            ));
        };
        // Never relabel a modality to evade a target restriction.
        // 绝不重新标记模态来绕过目标限制。
        let kind = attachment_kind(attachment.kind.clone());
        // This exact route selects the sole applicable upload contract.
        // 本精确路线选择唯一适用上传契约。
        let upload = execution.client.conversation_media_upload_contract(
            &execution.route,
            kind,
            &attachment.mime_type,
        )?;
        if upload.as_ref().is_some_and(|upload| {
            upload.reference_scope == vulcan_provider_adapters::UploadReferenceScope::Session
                && session_id.is_none()
        }) {
            return Ok((
                ComposerAttachmentDeliveryPlan::MetadataOnly,
                "upload_requires_persisted_session",
            ));
        }
        // Unfinished first uploads keep their original branch; only a successful receipt becomes restorable history.
        // 未完成首次上传保持原分支；只有成功回执成为可恢复历史。
        let (origin, force_upload) = match original.map(|original| &original.delivery) {
            None => (MediaDeliveryOrigin::NewIntent, false),
            Some(AttachmentDraftMediaDelivery::Inline) => {
                (MediaDeliveryOrigin::HistoricalInline, false)
            }
            Some(AttachmentDraftMediaDelivery::Upload {
                upload_channel_id,
                upload_contract_fingerprint,
                receipt,
            }) => {
                // A changed original contract cannot authorize replacement uploads, even after switching back to the same model.
                // 即使切回相同模型，变化的原契约也不能授权替代上传。
                let Some(contract) = upload.as_ref().filter(|contract| {
                    contract.upload_channel_id == *upload_channel_id
                        && contract.contract_fingerprint == *upload_contract_fingerprint
                }) else {
                    return Ok((
                        ComposerAttachmentDeliveryPlan::MetadataOnly,
                        "original_upload_contract_changed",
                    ));
                };
                if let Some(binding) = receipt {
                    // Receipt validation is pure: no cache lookup, credential read or provider call occurs during replay planning.
                    // 回执校验是纯操作：重放规划期间不查询缓存、读取凭据或调用供应商。
                    let usable = media_receipt_is_usable(
                        binding,
                        execution,
                        kind,
                        &attachment.mime_type,
                        contract,
                        session_id,
                    );
                    (
                        MediaDeliveryOrigin::HistoricalUpload {
                            original_target_matches: true,
                            reference_usable: usable,
                        },
                        false,
                    )
                } else {
                    (MediaDeliveryOrigin::NewIntent, true)
                }
            }
        };
        // Model and protocol facts remain independent of original intent and configurable soft limits.
        // 模型及协议事实独立于原意图和可配置软限制。
        let target = MediaDeliveryTarget {
            profile_id: &execution.route.protocol_route.protocol_profile_id,
            model_inputs: &execution.model.capabilities.input_resources,
            constraint: execution
                .route
                .protocol_route
                .input_resource_constraints
                .iter()
                .find(|constraint| constraint.resource_kind == kind),
            upload: upload.as_ref(),
        };
        self.plan_target_with_origin(
            &target,
            kind,
            &attachment.mime_type,
            bytes,
            origin,
            force_upload,
        )
    }
    /// Freezes validated host `policy` for one conversation batch; no file access or configuration writes occur.
    /// 为一批主会话附件冻结已校验宿主`policy`；不访问文件，也不写入配置。
    pub(super) fn new(policy: MediaDeliveryPolicy) -> Result<Self, String> {
        Ok(Self {
            planner: MediaDeliveryPlanner::new(policy, MediaDeliveryPurpose::Conversation)?,
            counts: BTreeMap::new(),
        })
    }

    /// Plans one explicit `attachment` against exact frozen `execution`, preserving intentional metadata-only reduction.
    /// 根据精确冻结`execution`规划一个显式`attachment`，保留主动仅元数据降级。
    /// Returns the exact composer mode and non-secret reason without preview substitution, source reads or uploads.
    /// 返回精确输入区模式及非秘密原因，不替换预览图、不读取源、不上传。
    pub(super) fn plan(
        &mut self,
        execution: &ResolvedPublicAiExecution,
        attachment: &ConversationAttachment,
    ) -> Result<(ComposerAttachmentDeliveryPlan, &'static str), String> {
        if attachment.delivery_mode == Some(ConversationAttachmentDeliveryMode::MetadataOnly) {
            return Ok((
                ComposerAttachmentDeliveryPlan::MetadataOnly,
                "explicit_metadata_only",
            ));
        }
        // Missing metadata is not an empty file and cannot select an inline branch.
        // 缺失元数据不等于空文件，不能选择内联分支。
        let Some(bytes) = attachment.byte_len else {
            return Ok((
                ComposerAttachmentDeliveryPlan::MetadataOnly,
                "attachment_size_is_unknown",
            ));
        };
        // The canonical attachment kind remains unchanged even when its target cannot consume it.
        // 即使目标不能消费，规范附件类型也保持不变。
        let kind = attachment_kind(attachment.kind.clone());
        // The existing resolver distinguishes genuine upload interfaces from inline-only primitives.
        // 既有解析器区分真正上传接口和仅内联原语。
        let upload = execution.client.conversation_media_upload_contract(
            &execution.route,
            kind,
            &attachment.mime_type,
        )?;
        // Model and protocol capabilities are frozen together; no global settings lookup is allowed here.
        // 模型与协议能力一起冻结；这里不允许查询全局设置。
        let target = MediaDeliveryTarget {
            profile_id: &execution.route.protocol_route.protocol_profile_id,
            model_inputs: &execution.model.capabilities.input_resources,
            constraint: execution
                .route
                .protocol_route
                .input_resource_constraints
                .iter()
                .find(|constraint| constraint.resource_kind == kind),
            upload: upload.as_ref(),
        };
        self.plan_target(&target, kind, &attachment.mime_type, bytes)
    }

    /// Admits one verified `kind`/`mime`/`bytes` source against `target` and returns its batch-aware composer decision.
    /// 根据`target`准入一个已验证`kind`、`mime`、`bytes`来源，返回考虑整批预算的输入区决策。
    fn plan_target(
        &mut self,
        target: &MediaDeliveryTarget<'_>,
        kind: ResourceKind,
        mime: &str,
        bytes: u64,
    ) -> Result<(ComposerAttachmentDeliveryPlan, &'static str), String> {
        self.plan_target_with_origin(
            target,
            kind,
            mime,
            bytes,
            MediaDeliveryOrigin::NewIntent,
            false,
        )
    }

    /// Applies one occurrence's `origin` to the same batch planner; `force_upload` retains an unfinished originally selected upload branch.
    /// 将一次出现的`origin`应用于同一批次规划器；`force_upload`保留未完成的原选中上传分支。
    /// The verified target, kind, MIME and bytes still enforce every current hard limit and cumulative budget.
    /// 已验证目标、类别、MIME及字节数仍执行全部当前硬限制及累计预算。
    fn plan_target_with_origin(
        &mut self,
        target: &MediaDeliveryTarget<'_>,
        kind: ResourceKind,
        mime: &str,
        bytes: u64,
        origin: MediaDeliveryOrigin,
        force_upload: bool,
    ) -> Result<(ComposerAttachmentDeliveryPlan, &'static str), String> {
        // Only already admitted native occurrences contribute to the next count.
        // 仅已准入原生出现次数参与下一数量计算。
        let count = self
            .counts
            .get(&kind)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| {
                String::from("Conversation media count exceeds the host integer range.")
            })?;
        // Original intent may remove inline preference but can never weaken a hard target/interface boundary.
        // 原意图可以移除内联偏好，但绝不能削弱目标或接口硬边界。
        let mut facts = target.facts(kind, mime, bytes, origin, count);
        if force_upload {
            facts.inline_supported = false;
        }
        // This owner always uses conversation purpose; original history changes only representation selection.
        // 本所有者始终使用主会话用途；原历史只改变表示选择。
        let decision = self.planner.plan(facts)?;
        // Only fresh conversation actions can become composer states; historical restoration belongs to the prompt replay owner.
        // 只有新的主会话动作可成为输入区状态；历史恢复属于提示词重放所有者。
        let result = match decision {
            MediaDeliveryDecision::Inline { .. } => (
                ComposerAttachmentDeliveryPlan::InlineOriginal,
                "original_bytes_fit_frozen_inline_policy",
            ),
            MediaDeliveryDecision::Upload => (
                ComposerAttachmentDeliveryPlan::UploadReference,
                "provider_upload_reference_required",
            ),
            MediaDeliveryDecision::RestoreOriginalUpload => (
                ComposerAttachmentDeliveryPlan::UploadReference,
                "original_upload_reference_restored",
            ),
            MediaDeliveryDecision::Attachment { reason } => (
                ComposerAttachmentDeliveryPlan::MetadataOnly,
                reason.explanation(),
            ),
            MediaDeliveryDecision::Reject { .. } => {
                return Err(String::from(
                    "Explicit conversation attachment returned a non-conversation or historical action.",
                ));
            }
        };
        if result.0 != ComposerAttachmentDeliveryPlan::MetadataOnly {
            self.counts.insert(kind, count);
        }
        Ok(result)
    }
}

/// Checks one private `binding` against frozen `execution`, canonical `kind`/`mime`, selected `contract` and optional `session_id` without I/O.
/// 根据冻结`execution`、规范`kind`及`mime`、选中`contract`和可选`session_id`检查私有`binding`，不执行I/O。
/// Returns true only for the exact typed route/reference and original unextended lifetime; no cache or credential lookup occurs.
/// 仅对精确强类型路线/引用及未经延长的原生命周期返回真；不查询缓存或凭据。
pub(crate) fn media_receipt_is_usable(
    binding: &vulcan_workspace_store::workspace_store::ProviderBinding,
    execution: &ResolvedPublicAiExecution,
    kind: ResourceKind,
    mime: &str,
    contract: &vulcan_provider_adapters::ConversationMediaUploadContract,
    session_id: Option<&str>,
) -> bool {
    // Legacy untyped bindings cannot prove source representation or scope.
    // 旧无类型绑定不能证明来源表示或作用域。
    let Some(receipt) = &binding.upload_receipt else {
        return false;
    };
    // The original expiry is compared directly and never refreshed on read.
    // 直接比较原过期时间，读取时绝不刷新。
    let now = super::current_unix_millis_u64();
    binding.provider == execution.route.provider.id
        && binding.connection_id.as_deref() == Some(execution.route.connection.id.as_str())
        && receipt.service_entry_id == execution.route.entry.id
        && receipt.upload_channel_id == contract.upload_channel_id
        && receipt.upload_contract_fingerprint == contract.contract_fingerprint
        && receipt.resource_kind == kind
        && receipt.mime_type == mime
        && receipt.source_kind
            == vulcan_provider_adapters::media_upload_source_kind(contract.reference_encoding)
        && if contract.reference_scope == vulcan_provider_adapters::UploadReferenceScope::Session {
            session_id.is_some() && receipt.session_scope_id.as_deref() == session_id
        } else {
            receipt.session_scope_id.is_none()
        }
        && receipt.created_at_unix_millis <= now
        && now < receipt.expires_at_unix_millis
        && execution
            .client
            .validate_conversation_media_reference(&execution.route, kind, mime, &binding.media_id)
            .is_ok()
}

#[cfg(test)]
mod tests;
