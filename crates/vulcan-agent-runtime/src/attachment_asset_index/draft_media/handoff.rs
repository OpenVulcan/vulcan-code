//! Exact draft upload handoff under the existing asset-index owner.
//! 既有资产索引所有者内的精确草稿上传交接。

use super::*;
use vulcan_workspace_store::workspace_store::ProjectResourceCatalogRegistry;

/// Original intent and exact registered project destinations retained across draft lease release.
/// 跨草稿租约释放保留的原意图及精确已登记项目目标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentDraftMediaHandoff {
    /// Original immutable selection with its optional first validated receipt.
    /// 原不可变选择及其可选首次已验证回执。
    original: AttachmentDraftMediaOrigin,
    /// Exact destinations supplied by prompt registration, never inferred from current project selection.
    /// 提示词注册提供的精确目标，绝不从当前项目选择推测。
    destinations: Vec<AttachmentDraftMediaDestination>,
}

/// One host-owned project catalog entry that accepted this exact original draft intent.
/// 接纳本精确原草稿意图的一个宿主所有项目目录条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentDraftMediaDestination {
    /// Absolute directory selected by the actual prompt's storage owner.
    /// 实际提示词存储所有者选择的绝对目录。
    project_directory: PathBuf,
    /// Exact resource id allocated by that project's existing catalog.
    /// 该项目既有目录分配的精确资源ID。
    resource_id: ResourceId,
}

/// Registers `original` for exact `attachment`, `project_directory` and `resource_id` before its draft lease can be promoted.
/// 草稿租约提升之前，为精确`attachment`、`project_directory`及`resource_id`登记`original`。
/// Uses `registry` to publish an already completed receipt; returns an error without guessing another intent or project.
/// 使用`registry`发布已经完成的回执；错误时不猜测另一意图或项目。
pub fn register_attachment_draft_upload_handoff(
    attachment: &ConversationAttachment,
    original: &AttachmentDraftMediaOrigin,
    project_directory: &Path,
    resource_id: ResourceId,
    registry: &ProjectResourceCatalogRegistry,
) -> Result<(), String> {
    if !project_directory.is_absolute() {
        return Err(String::from(
            "Draft upload handoff requires an absolute project owner.",
        ));
    }
    // The submission owner, not the upload planner, supplies the actual destination.
    // 实际目标由提交所有者而非上传规划器提供。
    let destination = AttachmentDraftMediaDestination {
        project_directory: project_directory.to_path_buf(),
        resource_id,
    };
    // Registration and completion serialize with lease promotion through the same existing asset owner.
    // 登记和完成经由同一既有资产所有者与租约提升串行化。
    let handoff = asset_media_transaction(attachment, |record, _| {
        original.validate(record)?;
        // Only the same live UUID or an earlier exact registered handoff can authorize this transfer.
        // 只有同一活动UUID或更早精确已登记交接能授权本次转移。
        let mut handoff =
            if let Some(handoff) = record.draft_media_handoffs.get(&original.intent_id) {
                handoff.clone()
            } else {
                // Live scope membership is mandatory; stale draft map contents alone are not authority.
                // 活动作用域成员关系是强制条件；陈旧草稿映射内容本身不构成权威。
                let live = record
                    .draft_media
                    .iter()
                    .find(|(scope, live)| {
                        live.intent_id == original.intent_id && record.staging_refs.contains(scope)
                    })
                    .map(|(_, live)| live.clone())
                    .ok_or_else(|| {
                        String::from("Draft upload handoff no longer owns a live original intent.")
                    })?;
                AttachmentDraftMediaHandoff {
                    original: live,
                    destinations: Vec::new(),
                }
            };
        require_same_selection(&handoff.original, original, record)?;
        if !handoff.destinations.contains(&destination) {
            handoff.destinations.push(destination);
        }
        record
            .draft_media_handoffs
            .insert(original.intent_id.clone(), handoff.clone());
        Ok(handoff)
    })?;
    publish_completed_handoff(&handoff, registry)
}

/// Completes exact `original` for `attachment`/`scope`, publishing `binding` only to live original ownership or registered prompt destinations.
/// 完成`attachment`/`scope`的精确`original`，仅向活动原所有权或已登记提示词目标发布`binding`。
/// `registry` remains the sole project catalog owner; removed/re-added drafts cannot acquire an earlier intent's result.
/// `registry`仍是唯一项目目录所有者；移除重加草稿不能取得更早意图的结果。
pub fn complete_attachment_draft_upload(
    attachment: &ConversationAttachment,
    scope: &str,
    original: &AttachmentDraftMediaOrigin,
    mut binding: ProviderBinding,
    registry: &ProjectResourceCatalogRegistry,
) -> Result<(), String> {
    // Stamp correlation before calculating any durable fingerprint; an already foreign intent is an explicit error.
    // 计算任何可靠指纹之前写入关联；已有其他意图时明确报错。
    let receipt = &mut binding.upload_receipt;
    if receipt
        .draft_intent_id
        .as_ref()
        .is_some_and(|intent| intent != &original.intent_id)
    {
        return Err(String::from(
            "Draft upload receipt belongs to another intent.",
        ));
    }
    receipt.draft_intent_id = Some(original.intent_id.clone());
    // The transaction returns a durable snapshot; no project lock is held under the asset lock.
    // 事务返回可靠快照；资产锁内不持有项目锁。
    let handoff = asset_media_transaction(attachment, |record, _| {
        original.validate(record)?;
        // A new intent in the same scope is deliberately excluded instead of being overwritten.
        // 同一作用域中的新意图被明确排除，而非被覆盖。
        let live = record
            .draft_media
            .get(scope)
            .filter(|live| {
                live.intent_id == original.intent_id
                    && record.staging_refs.iter().any(|lease| lease == scope)
            })
            .cloned();
        // A previously registered target is the only authority after lease release.
        // 租约释放后，更早已登记目标是唯一权威。
        let mut handoff = record
            .draft_media_handoffs
            .get(&original.intent_id)
            .cloned();
        if live.is_none() && handoff.is_none() {
            return Err(String::from(
                "Draft upload intention has no live lease or registered prompt handoff.",
            ));
        }
        if let Some(mut live) = live {
            require_same_selection(&live, original, record)?;
            fill_receipt(&mut live, &binding, record)?;
            record.draft_media.insert(scope.to_string(), live);
        }
        if let Some(handoff) = &mut handoff {
            require_same_selection(&handoff.original, original, record)?;
            fill_receipt(&mut handoff.original, &binding, record)?;
            record
                .draft_media_handoffs
                .insert(original.intent_id.clone(), handoff.clone());
        }
        Ok(handoff)
    })?;
    if let Some(handoff) = handoff {
        publish_completed_handoff(&handoff, registry)?;
    }
    Ok(())
}

/// Verifies that `actual` and `expected` name the same selected upload in exact asset `record`, ignoring only absent completion.
/// 校验`actual`及`expected`指向精确资产`record`的同一已选上传，仅忽略缺失的完成结果。
/// Returns an explicit error for different UUIDs, routes, sources or contracts.
/// UUID、路线、来源或契约不同时返回明确错误。
fn require_same_selection(
    actual: &AttachmentDraftMediaOrigin,
    expected: &AttachmentDraftMediaOrigin,
    record: &AttachmentAssetRecord,
) -> Result<(), String> {
    actual.validate(record)?;
    // Remove only the optional completion to compare every immutable selection field.
    // 仅移除可选完成结果以比较每个不可变选择字段。
    let mut selected = actual.clone();
    let mut expected = expected.clone();
    for origin in [&mut selected, &mut expected] {
        let AttachmentDraftMediaDelivery::Upload { receipt, .. } = &mut origin.delivery else {
            return Err(String::from(
                "Draft upload handoff requires an original upload selection.",
            ));
        };
        *receipt = None;
    }
    if selected != expected {
        return Err(String::from(
            "Draft upload handoff differs from the original selection.",
        ));
    }
    Ok(())
}

/// Fills the first `binding` of `original` and validates it against `record`; replacements fail before persistence.
/// 填充`original`的首次`binding`并按`record`校验；替换在持久化前失败。
fn fill_receipt(
    original: &mut AttachmentDraftMediaOrigin,
    binding: &ProviderBinding,
    record: &AttachmentAssetRecord,
) -> Result<(), String> {
    let AttachmentDraftMediaDelivery::Upload { receipt, .. } = &mut original.delivery else {
        return Err(String::from(
            "An original inline draft cannot acquire an upload receipt.",
        ));
    };
    if receipt.as_ref().is_some_and(|existing| existing != binding) {
        return Err(String::from(
            "Original draft upload receipt cannot be replaced.",
        ));
    }
    *receipt = Some(binding.clone());
    original.validate(record)
}

/// Publishes the optional known first receipt from `handoff` through exact existing `registry` targets, without session history mutation.
/// 经由精确既有`registry`目标发布`handoff`中可选的已知首次回执，不修改会话历史。
/// Pending handoffs perform no publication; failures remain explicit and never schedule another upload.
/// 待完成交接不执行发布；失败保持明确且绝不调度另一上传。
fn publish_completed_handoff(
    handoff: &AttachmentDraftMediaHandoff,
    registry: &ProjectResourceCatalogRegistry,
) -> Result<(), String> {
    let AttachmentDraftMediaDelivery::Upload {
        receipt: Some(receipt),
        ..
    } = &handoff.original.delivery
    else {
        return Ok(());
    };
    for destination in &handoff.destinations {
        if !destination.project_directory.is_absolute() {
            return Err(String::from(
                "Draft upload handoff requires an absolute project owner.",
            ));
        }
        registry
            .record_provider_upload_receipt(
                &destination.project_directory,
                destination.resource_id,
                &handoff.original.source_sha256,
                handoff.original.source_byte_len,
                receipt.clone(),
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
