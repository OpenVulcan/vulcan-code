//! Read-only delivery admission and send-scoped staging for composer drops.
//! 输入区拖入文件的只读交付准入与发送作用域暂存。

use super::*;
use crate::session_runtime_registry_v1::SessionHandle;
use vulcan_agent_runtime::attachment_assets::{
    attachment_descriptor_from_mime_type, generic_attachment_descriptor_for_mime_type,
    normalize_generic_attachment_mime_type,
};
use vulcan_provider_adapters::ResolvedPublicAiExecution;
use vulcan_service_api::{WEBUI_SERVICE_RPC_BODY_LIMIT_BYTES, WebuiServiceRpcHttpRequest};
use vulcan_session_core::v1::config::ActiveRequestConfig;

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

/// One preflight request with existing occurrences in final submission order.
/// 一次预检请求，已有附件按最终提交顺序排列。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DroppedComposerPreflight {
    /// Exact composer slot used by the existing draft lease.
    /// 既有草稿租约使用的精确输入区槽位。
    draft_slot_key: String,
    /// Selected project, when the draft has one.
    /// 草稿有所属项目时使用选中项目。
    project_id: Option<String>,
    /// Existing V1 session, or no session for a new draft.
    /// 既有 V1 会话，新建草稿则为空。
    conversation_id: Option<String>,
    /// Rendered model choice for a new draft.
    /// 新建草稿时界面选中的模型。
    model_selection: Option<SubmissionModelSelection>,
    /// Ordered attachment prefix sent before the dropped originals.
    /// 在拖入原件之前发送的有序附件前缀。
    attachments: Vec<ComposerAttachmentSummary>,
    /// Remaining attachment slots in the composer, shared with the file picker.
    /// 输入区与文件选择器共用的剩余附件槽位。
    max_new_attachments: usize,
    /// File facts in original draft order.
    /// 按原草稿顺序排列的文件信息。
    files: Vec<DroppedComposerFile>,
    /// Whether to retain the snapshot across staging and prompt admission.
    /// 是否跨暂存和提示词准入保留快照。
    for_submission: bool,
}

/// Returns eligibility per file and a Core-owned plan ID for sending.
/// 返回每个文件的交付资格及发送时由 Core 持有的规划 ID。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DroppedComposerClassification {
    /// Eligibility in input order.
    /// 按输入顺序排列的交付资格。
    eligible: Vec<bool>,
    /// Exact frozen submission plan, absent for drag-time queries.
    /// 精确冻结的提交规划，拖入时查询不返回。
    plan_id: Option<uuid::Uuid>,
}

/// Exact stage method shared by the dispatcher and the full-body size check.
/// 分派器与完整请求体大小检查共用的精确暂存方法名。
pub(crate) const STAGE_DROPPED_COMPOSER_FILE_RPC: &str = "service_stage_dropped_composer_file";

/// Retains one eligible occurrence and its staged result for transport retries.
/// 保存一次已准入资源出现及其暂存结果，以支持传输重试。
struct PlannedDroppedFile {
    /// File facts admitted by the shared budget.
    /// 由共享额度准入的文件信息。
    source: DroppedComposerFile,
    /// Managed summary returned by the completed stage request.
    /// 已完成暂存请求返回的托管摘要。
    staged: Option<ComposerAttachmentSummary>,
}

/// Core-owned planning state scoped to one draft and one short-lived send attempt.
/// Core 持有的规划状态，限定于单个草稿和短期发送尝试。
pub(crate) struct DroppedComposerPlan {
    /// Canonical workspace and draft lease used for staging and promotion.
    /// 暂存与提升使用的规范工作区和草稿租约。
    workspace: String,
    /// Lease scope derived from the exact selected owner.
    /// 从精确选中所有者派生的租约作用域。
    scope: String,
    /// Existing session whose active Provider lease may own the exact model.
    /// 当前供应商租约可能持有精确模型的已有会话。
    conversation_id: Option<String>,
    /// Immutable request configuration captured before reading any file bytes.
    /// 在读取任何文件字节之前捕获的不可变请求配置。
    request_config: ActiveRequestConfig,
    /// Effective route revision and media policy captured together.
    /// 同时捕获的生效路线修订和媒体策略。
    route_revision: String,
    /// Policy used by the shared batch budget.
    /// 共享批次额度使用的策略。
    policy: vulcan_provider_adapters::MediaDeliveryPolicy,
    /// Existing ordered attachment summaries from the same preflight.
    /// 同一预检中的有序已有附件摘要。
    attachments: Vec<ComposerAttachmentSummary>,
    /// Eligible entries aligned with their preflight file indices.
    /// 与预检文件索引对齐的准入条目。
    files: Vec<Option<PlannedDroppedFile>>,
    /// Client action bound to the plan on its first submission, including exact retries.
    /// 首次提交时绑定规划的客户端动作，精确重试也使用它。
    action_id: Option<String>,
}

/// Plans the managed prefix and dropped originals together and returns eligibility in source order.
/// 将已有托管前缀和拖入原件统一规划，按来源顺序返回交付资格。
pub(crate) async fn classify_dropped_composer_files(
    state: &SharedServiceBridgeState,
    request: DroppedComposerPreflight,
) -> Result<DroppedComposerClassification, String> {
    let scope = crate::attachments::composer_attachment_draft_scope_for_owner(
        &request.draft_slot_key,
        request.project_id.as_deref(),
        request.conversation_id.as_deref(),
    )?;
    let workspace = composer_attachment_workspace_path(
        state,
        request.project_id.as_deref(),
        request.conversation_id.as_deref(),
    )?;
    let (controller, runtime, _, _, _) = runtime_parts_from_state(state)?;
    let session_id = composer_media_session_id(request.conversation_id.as_deref())?;
    // Load the same V1 authority used by prompt admission, including cold sessions.
    // 加载提示词准入使用的同一 V1 权威，包括冷会话。
    let handle = if let Some(session_id) = session_id {
        let registry = Arc::clone(
            &state
                .lock()
                .map_err(|error| error.to_string())?
                .session_runtime_registry,
        );
        Some(
            crate::session_v1_lifecycle::get_or_load_session_v1(&controller, &registry, session_id)
                .await
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let mut request_config = handle
        .as_ref()
        .map(|handle| {
            crate::session_request_config_v1::reconcile_pending_reasoning_for_next_user_boundary_v1(
                handle.as_ref(),
                runtime.public_ai_runtime.as_ref(),
            )
        })
        .transpose()?
        .unwrap_or_default();
    let selection = if handle.is_some() {
        Some(dropped_plan_model(&request_config)?)
    } else {
        request.model_selection
    };
    let execution = resolve_drop_execution(
        &runtime,
        request.conversation_id.as_deref(),
        selection.as_ref(),
    )
    .await?;
    if handle.is_none() {
        request_config.provider = Some(execution.route.provider.id.clone());
        request_config.connection_id = Some(execution.route.connection.id.clone());
        request_config.model = Some(execution.model.id.clone());
    }
    // The same planner reserves the submitted attachment prefix before looking at the drops.
    // 同一规划器先为待提交的已有附件前缀占用额度，再处理拖入项。
    let mut planner = ComposerMediaDeliveryPlanner::new(execution.media_delivery_policy)?;
    for summary in &request.attachments {
        reserve_existing_attachment(
            &mut planner,
            &execution,
            &workspace,
            &scope,
            handle.as_deref(),
            request.conversation_id.as_deref(),
            summary,
        )?;
    }
    let plan_id = uuid::Uuid::new_v4();
    // Only accepted originals consume a composer slot; path references remain text.
    // 只有准入的原件占用输入区槽位；路径引用仍为文本。
    let mut remaining = request.max_new_attachments;
    let eligible = request
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            if remaining == 0 || !dropped_stage_request_fits(plan_id, index, file.byte_len)? {
                return Ok(false);
            }
            let (kind, mime_type) = delivery_descriptor(file)?;
            let (plan, _) = planner.plan_file(&execution, kind, &mime_type, file.byte_len)?;
            let accepted = plan != ComposerAttachmentDeliveryPlan::MetadataOnly;
            remaining -= usize::from(accepted);
            Ok(accepted)
        })
        .collect::<Result<Vec<_>, String>>()?;
    if request.for_submission {
        let files = request
            .files
            .into_iter()
            .zip(&eligible)
            .map(|(source, eligible)| {
                eligible.then_some(PlannedDroppedFile {
                    source,
                    staged: None,
                })
            })
            .collect();
        let plan = DroppedComposerPlan {
            workspace,
            scope,
            conversation_id: request.conversation_id,
            request_config,
            route_revision: execution.frozen_route_revision().to_owned(),
            policy: execution.media_delivery_policy,
            attachments: request.attachments,
            files,
            action_id: None,
        };
        let mut shared = state.lock().map_err(|error| error.to_string())?;
        shared
            .dropped_composer_plans
            .retain(|_, (created, _)| created.elapsed() < DRAFT_MEDIA_UPLOAD_PLAN_TTL);
        shared.dropped_composer_plans.insert(
            plan_id,
            (std::time::Instant::now(), Arc::new(StdMutex::new(plan))),
        );
    }
    Ok(DroppedComposerClassification {
        eligible,
        plan_id: request.for_submission.then_some(plan_id),
    })
}

/// Resolves the next tuple using the same active lease that physical dispatch would retain.
/// 用真实分派会保留的同一活动租约解析下一请求三元组。
/// A different tuple resolves from the current catalog without altering the live lease.
/// 不同的三元组按当前目录解析，保持活动租约不变。
async fn resolve_drop_execution(
    runtime: &crate::state::AgentRuntimeBridge,
    conversation_id: Option<&str>,
    selection: Option<&SubmissionModelSelection>,
) -> Result<ResolvedPublicAiExecution, String> {
    if let (Some(session_id), Some(target)) = (conversation_id, selection)
        && let Some(lease) = runtime
            .public_ai_runtime
            .session_selection_lease(session_id)?
        && lease.route.provider.id == target.provider_id
        && lease.route.connection.id == target.connection_id
        && lease.model.id == target.model_id
    {
        return runtime
            .public_ai_runtime
            .resolve_execution_from_frozen_selection(lease)
            .await;
    }
    resolve_submission_media_execution(runtime, None, None, selection).await
}

/// Adds one existing attachment using the same origin and rules as composer media preparation.
/// 用输入区媒体准备的同一来源及规则计入一个已有附件。
/// Returns an error if an exact asset or project occurrence cannot be resolved.
/// 若无法解析精确资产或项目资源出现，则返回错误。
fn reserve_existing_attachment(
    planner: &mut ComposerMediaDeliveryPlanner,
    execution: &ResolvedPublicAiExecution,
    workspace: &str,
    scope: &str,
    handle: Option<&SessionHandle>,
    conversation_id: Option<&str>,
    summary: &ComposerAttachmentSummary,
) -> Result<(), String> {
    if summary.delivery_plan == Some(ComposerAttachmentDeliveryPlan::MetadataOnly) {
        return Ok(());
    }
    if let Some(raw_id) = summary.project_resource_id {
        crate::conversation_resources_v1::validate_project_resource_attachment_reference(summary)?;
        let handle =
            handle.ok_or("Project composer attachments require a materialized V1 session.")?;
        let locator = handle
            .storage_locator()
            .map_err(|error| error.to_string())?;
        let id = crate::conversation_resources_v1::checked_resource_id(raw_id)?;
        let catalog = vulcan_workspace_store::workspace_store::load_project_resource_catalog(
            &locator.project_directory(),
        )
        .map_err(|error| error.to_string())?;
        let entry = catalog
            .entries
            .iter()
            .find(|entry| entry.resource_id == id)
            .ok_or("Project attachment is missing from its resource catalog.")?;
        if vulcan_workspace_store::workspace_store::resolve_project_resource_source(
            &locator.project_directory(),
            &catalog,
            id,
        )
        .is_ok_and(|source| source.is_some())
        {
            let canonical =
                crate::conversation_resources_v1::composer_attachment_from_project_resource_v1(
                    &locator, raw_id,
                )?;
            let attachment = composer_attachment_summary_to_core(canonical)?;
            let preparation = if let Some(source) = &summary.prompt_source {
                handle
                    .read(|document| document.user_prompt_resource_preparation(source, id))
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?
            } else {
                vulcan_session_core::v1::UserPromptResourcePreparation::Unproven
            };
            planner.plan_project(
                execution,
                &attachment,
                entry,
                &preparation,
                locator.session_id(),
            )?;
        }
    } else {
        let declared = composer_attachment_summary_to_core(summary.clone())?;
        let asset_id = declared
            .asset_id
            .as_deref()
            .ok_or("Composer attachment is missing its stable asset id.")?;
        let attachment =
            vulcan_agent_runtime::attachment_asset_index::resolve_indexed_attachment_metadata(
                workspace,
                asset_id,
                declared.kind,
            )?;
        let original = vulcan_agent_runtime::attachment_asset_index::load_attachment_draft_media(
            &attachment,
            scope,
        )?;
        planner.plan_draft(execution, &attachment, original.as_ref(), conversation_id)?;
    }
    Ok(())
}

/// Projects the exact provider/model/account from a Core-owned request configuration.
/// 从 Core 持有的请求配置投影精确供应商、模型和账号。
pub(crate) fn dropped_plan_model(
    config: &ActiveRequestConfig,
) -> Result<SubmissionModelSelection, String> {
    Ok(SubmissionModelSelection {
        provider_id: config
            .provider
            .clone()
            .ok_or("Dropped file admission requires an active provider.")?,
        connection_id: config
            .connection_id
            .clone()
            .ok_or("Dropped file admission requires an active connection.")?,
        model_id: config
            .model
            .clone()
            .ok_or("Dropped file admission requires an active model.")?,
    })
}

/// Checks capacity for raw file bytes using the exact stage envelope and Base64 expansion.
/// 使用精确暂存请求体和 Base64 膨胀量检查原文件字节的容量。
fn dropped_stage_request_fits(
    plan_id: uuid::Uuid,
    index: usize,
    byte_len: u64,
) -> Result<bool, String> {
    let envelope = WebuiServiceRpcHttpRequest {
        method: STAGE_DROPPED_COMPOSER_FILE_RPC.into(),
        params: Some(serde_json::json!({ "planId": plan_id, "index": index, "dataBase64": "" })),
    };
    let overhead = serde_json::to_vec(&envelope)
        .map_err(|error| error.to_string())?
        .len();
    Ok(byte_len.div_ceil(3) <= ((WEBUI_SERVICE_RPC_BODY_LIMIT_BYTES - overhead) / 4) as u64)
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

/// Retrieves the unexpired plan without retaining the service-state lock during I/O.
/// 获取未过期规划，I/O 期间不持有服务状态锁。
fn dropped_composer_plan(
    state: &SharedServiceBridgeState,
    plan_id: uuid::Uuid,
) -> Result<Arc<StdMutex<DroppedComposerPlan>>, String> {
    let shared = state.lock().map_err(|error| error.to_string())?;
    let (_, plan) = shared
        .dropped_composer_plans
        .get(&plan_id)
        .filter(|(created, _)| created.elapsed() < DRAFT_MEDIA_UPLOAD_PLAN_TTL)
        .ok_or("Dropped file send plan expired. Send the draft again.")?;
    Ok(Arc::clone(plan))
}

/// Stages one position admitted by the immutable plan and returns its leased managed summary.
/// 暂存不可变规划准入的一个位置，并返回带租约的托管摘要。
pub(crate) fn stage_dropped_composer_file(
    state: &SharedServiceBridgeState,
    plan_id: uuid::Uuid,
    index: usize,
    data_base64: String,
) -> Result<ComposerAttachmentSummary, String> {
    let plan = dropped_composer_plan(state, plan_id)?;
    let mut plan = plan.lock().map_err(|error| error.to_string())?;
    let DroppedComposerPlan {
        workspace,
        scope,
        files,
        ..
    } = &mut *plan;
    let planned = files
        .get_mut(index)
        .and_then(Option::as_mut)
        .ok_or("Dropped file was not admitted by this send plan.")?;
    let file = &planned.source;
    // The payload position carries the preflight facts, so staging never consults mutable selection.
    // 有效载荷位置绑定预检事实，因此暂存阶段不会读取可变选择。
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
    if let Some(staged) = &planned.staged {
        return Ok(staged.clone());
    }
    let attachment = crate::attachments::stage_composer_attachment_base64(
        workspace,
        &file.file_name,
        &file.mime_type,
        &data_base64,
    )?;
    vulcan_agent_runtime::attachment_asset_index::acquire_staged_attachment_ref(
        workspace,
        scope,
        &[composer_attachment_summary_to_core(attachment.clone())?],
    )?;
    planned.staged = Some(attachment.clone());
    Ok(attachment)
}

/// Claims the exact planned attachment list and returns the configuration to bind to this input.
/// 领取规划中的精确附件列表，并返回绑定到此次输入的配置。
/// Any newer pending model remains for subsequent inputs.
/// 更新的待生效模型保留给后续输入。
pub(crate) async fn claim_dropped_composer_plan(
    state: &SharedServiceBridgeState,
    plan_id: uuid::Uuid,
    workspace: &str,
    scope: Option<&str>,
    attachments: &[ComposerAttachmentSummary],
    action_id: &str,
    runtime: &crate::state::AgentRuntimeBridge,
) -> Result<ActiveRequestConfig, String> {
    let plan = dropped_composer_plan(state, plan_id)?;
    let (config, route_revision, policy, conversation_id) = {
        let plan = plan.lock().map_err(|error| error.to_string())?;
        let tail = attachments
            .strip_prefix(plan.attachments.as_slice())
            .ok_or("Existing attachments changed after dropped file preflight.")?;
        if plan.workspace != workspace
            || Some(plan.scope.as_str()) != scope
            || !plan
                .files
                .iter()
                .flatten()
                .map(|file| file.staged.as_ref())
                .eq(tail.iter().map(Some))
        {
            return Err(
                "Dropped file send does not match its preflight owner or attachments.".into(),
            );
        }
        (
            plan.request_config.clone(),
            plan.route_revision.clone(),
            plan.policy,
            plan.conversation_id.clone(),
        )
    };
    // Revalidate the exact tuple to detect policy changes unrelated to a session model picker.
    // 重新验证精确三元组，以检测与会话模型选择器无关的策略变动。
    let selection = dropped_plan_model(&config)?;
    let execution =
        resolve_drop_execution(runtime, conversation_id.as_deref(), Some(&selection)).await?;
    if execution.frozen_route_revision() != route_revision
        || execution.media_delivery_policy != policy
    {
        return Err("Dropped file route policy changed. Send the draft again.".into());
    }
    let mut plan = plan.lock().map_err(|error| error.to_string())?;
    match plan.action_id.as_deref() {
        Some(claimed) if claimed != action_id => {
            return Err("Dropped file send plan belongs to a different client action.".into());
        }
        Some(_) => {}
        None => plan.action_id = Some(action_id.to_owned()),
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the full HTTP envelope boundary, including the plan ID and variable-width index.
    /// 验证完整 HTTP 请求体边界，包括规划 ID 和可变宽度索引。
    #[test]
    fn dropped_stage_counts_the_complete_json_body() -> Result<(), String> {
        let plan_id = uuid::Uuid::nil();
        for index in [0, 10, usize::MAX] {
            let empty_body = serde_json::to_vec(&serde_json::json!({
                "method": STAGE_DROPPED_COMPOSER_FILE_RPC,
                "params": { "planId": plan_id, "index": index, "dataBase64": "" },
            }))
            .map_err(|error| error.to_string())?;
            let max_raw = ((WEBUI_SERVICE_RPC_BODY_LIMIT_BYTES - empty_body.len()) / 4 * 3) as u64;
            assert!(dropped_stage_request_fits(plan_id, index, max_raw)?);
            assert!(!dropped_stage_request_fits(plan_id, index, max_raw + 1)?);
            assert!(!dropped_stage_request_fits(
                plan_id,
                index,
                WEBUI_SERVICE_RPC_BODY_LIMIT_BYTES as u64 / 4 * 3 - 3
            )?);
        }
        assert!(!dropped_stage_request_fits(plan_id, 0, u64::MAX)?);
        Ok(())
    }
}
