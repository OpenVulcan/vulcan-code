//! Read-only delivery admission for files dropped on the composer.
//! 输入区拖入文件的只读交付准入。

use super::*;
use vulcan_agent_runtime::attachment_assets::{
    attachment_descriptor_from_mime_type, generic_attachment_descriptor_for_mime_type,
    normalize_generic_attachment_mime_type,
};

/// Browser file facts used for preflight without reading or persisting file bytes.
/// 用于预检的浏览器文件信息，不读取或持久化文件字节。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DroppedComposerFile {
    /// Display filename used by the same descriptor rules as attachment staging.
    /// 使用与附件暂存相同描述规则的展示文件名。
    file_name: String,
    /// MIME supplied by the browser, which may be empty.
    /// 浏览器提供的 MIME，可以为空。
    mime_type: String,
    /// Exact raw file length.
    /// 原始文件的精确字节数。
    byte_len: u64,
}

/// Returns delivery eligibility in input order using the current route and one batch budget.
/// 根据当前路线与同一批次预算，按输入顺序返回交付准入。
pub(crate) async fn classify_dropped_composer_files(
    state: &SharedServiceBridgeState,
    conversation_id: Option<&str>,
    model_selection: Option<&SubmissionModelSelection>,
    files: &[DroppedComposerFile],
) -> Result<Vec<bool>, String> {
    let (_, agent_runtime, _, _, _) = runtime_parts_from_state(state)?;
    // Existing sessions bind new input to the same next-request configuration as submission.
    // 既有会话使用与正式提交一致的下一请求配置来绑定新输入。
    let session_id = composer_media_session_id(conversation_id)?;
    let next = if let Some(session_id) = session_id {
        let handle = crate::session_goal_lifecycle::registered_session_v1_handle_from_state(
            state, session_id,
        )?;
        let config = crate::session_request_config_v1::capture_next_request_config_v1(&handle)?;
        Some(SubmissionModelSelection {
            provider_id: config
                .provider
                .ok_or("Dropped file admission requires an active provider.")?,
            connection_id: config
                .connection_id
                .ok_or("Dropped file admission requires an active connection.")?,
            model_id: config
                .model
                .ok_or("Dropped file admission requires an active model.")?,
        })
    } else {
        model_selection.cloned()
    };
    let execution =
        resolve_submission_media_execution(&agent_runtime, None, None, next.as_ref()).await?;
    // One planner accounts for cumulative limits across all candidates in source order.
    // 使用一个规划器按来源顺序计算全部候选项的累积限制。
    let mut planner = ComposerMediaDeliveryPlanner::new(execution.media_delivery_policy)?;
    files
        .iter()
        .map(|file| {
            // Reject a file if its Base64 alone exhausts the transport before JSON metadata can fit.
            // 如果仅 Base64 就已耗尽传输上限，为 JSON 元数据留不出空间，则拒绝文件传输。
            if file.byte_len.div_ceil(3)
                >= (vulcan_service_api::WEBUI_SERVICE_RPC_BODY_LIMIT_BYTES / 4) as u64
            {
                return Ok(false);
            }
            let (kind, mime_type) = delivery_descriptor(file)?;
            let (plan, _) = planner.plan_file(&execution, kind, &mime_type, file.byte_len)?;
            Ok(plan != ComposerAttachmentDeliveryPlan::MetadataOnly)
        })
        .collect()
}

/// Resolves the kind and MIME that the exact same browser file would receive during staging.
/// 解析同一浏览器文件在暂存时会得到的类别与 MIME。
fn delivery_descriptor(
    file: &DroppedComposerFile,
) -> Result<(ConversationAttachmentKind, String), String> {
    if let Some(descriptor) = attachment_descriptor_from_mime_type(&file.mime_type)
        .or_else(|| crate::attachments::attachment_descriptor_from_path_fallback(&file.file_name))
    {
        return Ok((descriptor.kind, descriptor.mime_type.to_owned()));
    }
    let mime_type = normalize_generic_attachment_mime_type(&file.mime_type)?;
    Ok((
        generic_attachment_descriptor_for_mime_type(&mime_type).kind,
        mime_type,
    ))
}

/// Builds exact browser file metadata for the route-constraint regression test.
/// 为路线约束回归测试构建精确的浏览器文件元数据。
#[cfg(test)]
pub(super) fn test_file_descriptor(
    name: &str,
    mime: &str,
    byte_len: u64,
) -> Result<(ConversationAttachmentKind, String), String> {
    delivery_descriptor(&DroppedComposerFile {
        file_name: name.into(),
        mime_type: mime.into(),
        byte_len,
    })
}

/// Rechecks one dropped file against its current execution route before storing any bytes.
/// 写入任何字节之前，按实际执行路线复查一个拖入文件。
/// Returns an attachment with the current draft lease, or no attachment when the route requires a reference.
/// 返回持有当前草稿租约的附件，或在路线要求引用时返回空。
#[expect(
    clippy::too_many_arguments,
    reason = "The exact draft owner, route and file must cross the admission boundary together."
)]
pub(crate) async fn stage_dropped_composer_file(
    state: SharedServiceBridgeState,
    project_id: Option<String>,
    conversation_id: Option<String>,
    draft_slot_key: String,
    model_selection: Option<SubmissionModelSelection>,
    file: DroppedComposerFile,
    data_base64: String,
) -> Result<Option<ComposerAttachmentSummary>, String> {
    let scope = crate::attachments::composer_attachment_draft_scope_for_owner(
        &draft_slot_key,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let workspace = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    // Check the actual decoded length before route admission or creating an asset.
    // 在路线准入或创建资产之前确认实际解码长度。
    let padding = data_base64
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'=')
        .count();
    if !data_base64.len().is_multiple_of(4)
        || padding > 2
        || (data_base64.len() / 4 * 3)
            .checked_sub(padding)
            .map(|len| len as u64)
            != Some(file.byte_len)
    {
        return Err("Dropped file bytes do not match their declared length.".into());
    }
    if classify_dropped_composer_files(
        &state,
        conversation_id.as_deref(),
        model_selection.as_ref(),
        std::slice::from_ref(&file),
    )
    .await?
        != [true]
    {
        return Ok(None);
    }
    let attachment = crate::attachments::service_stage_composer_attachment_bytes(
        state,
        project_id,
        conversation_id,
        file.file_name,
        file.mime_type,
        data_base64,
    )
    .await?;
    vulcan_agent_runtime::attachment_asset_index::acquire_staged_attachment_ref(
        &workspace,
        &scope,
        &[composer_attachment_summary_to_core(attachment.clone())?],
    )?;
    Ok(Some(attachment))
}
