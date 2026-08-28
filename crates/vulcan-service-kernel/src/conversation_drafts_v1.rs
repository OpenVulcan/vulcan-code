//! Authoritative V1 composer-draft reads and writes.
//! 权威 V1 输入区草稿读写。

use std::sync::Arc;

use vulcan_core::types::ConversationAttachment;
use vulcan_service_api::{
    ComposerAttachmentSummary, SESSION_RESOURCE_LIST_V1_MAX_ITEMS, SessionDraftReadV1Request,
    SessionDraftV1Response, SessionDraftWriteV1Request,
};
use vulcan_workbench_model::SessionId;
use vulcan_workspace_store::workspace_store::{
    ProjectResourceCatalogRegistry, SessionV1StorageLocator, load_project_resource_catalog,
    resolve_project_resource,
};

use crate::attachments::{
    composer_attachment_summary_to_core, composer_attachment_workspace_path_from_snapshot,
    resolve_composer_attachment_summary_for_workspace,
};
use crate::conversation_resources_v1::{
    composer_attachment_from_project_resource_v1, resource_summary_from_resolution,
    validate_project_resource_attachment_reference,
};
use crate::session_goal_lifecycle::current_v1_goal_unix_millis;
use crate::session_runtime_registry_v1::SessionHandle;
use crate::session_v1_lifecycle::{get_or_load_session_v1, register_project_attachments_v1};
use crate::state::SharedServiceBridgeState;

/// Reads one authoritative V1 composer draft without consulting browser-local storage.
/// 读取一份权威 V1 输入区草稿，且不查询浏览器本地存储。
pub(crate) async fn service_session_draft_v1(
    state: SharedServiceBridgeState,
    request: SessionDraftReadV1Request,
) -> Result<SessionDraftV1Response, String> {
    let authority = load_draft_authority(&state, &request.session_id).await?;
    draft_response_from_authority(authority.handle.as_ref(), &authority.locator)
}

/// Replaces one authoritative V1 composer draft after exact resource admission.
/// 在精确资源准入后替换一份权威 V1 输入区草稿。
pub(crate) async fn service_set_session_draft_v1(
    state: SharedServiceBridgeState,
    request: SessionDraftWriteV1Request,
) -> Result<SessionDraftV1Response, String> {
    if request.attachments.len() > SESSION_RESOURCE_LIST_V1_MAX_ITEMS {
        return Err(format!(
            "session draft contains {} attachments; maximum is {SESSION_RESOURCE_LIST_V1_MAX_ITEMS}",
            request.attachments.len()
        ));
    }
    let authority = load_draft_authority(&state, &request.session_id).await?;
    let attachments = resolve_draft_attachments(&state, &authority.locator, request.attachments)?;
    let resource_ids = register_project_attachments_v1(
        &authority.controller,
        authority.resource_registry.as_ref(),
        authority.locator.project_id(),
        &attachments,
    )
    .map_err(|error| error.to_string())?;
    let now_ms = current_v1_goal_unix_millis()?;
    if request.text.is_empty() && resource_ids.is_empty() {
        authority
            .handle
            .clear_draft(now_ms)
            .map_err(|error| error.to_string())?;
    } else {
        authority
            .handle
            .set_draft(request.text, resource_ids, now_ms)
            .map_err(|error| error.to_string())?;
    }
    draft_response_from_authority(authority.handle.as_ref(), &authority.locator)
}

/// Exact runtime and storage owners required by one draft operation.
/// 一次草稿操作所需的精确运行时与存储 owner。
struct SessionDraftAuthority {
    /// Loaded unique session owner.
    /// 已加载的唯一会话 owner。
    handle: Arc<SessionHandle>,
    /// Exact session storage locator.
    /// 精确会话存储定位器。
    locator: SessionV1StorageLocator,
    /// Shared controller used by project-resource publication.
    /// 项目资源发布使用的共享控制器。
    controller: vulcan_workspace_store::data_plane::ControllerRuntimeHandle,
    /// Per-project serialized resource catalog owner.
    /// 按项目串行化的资源目录 owner。
    resource_registry: Arc<ProjectResourceCatalogRegistry>,
}

/// Loads one exact V1 owner and verifies its storage locator identity.
/// 加载一个精确 V1 owner，并核验其存储定位器身份。
async fn load_draft_authority(
    state: &SharedServiceBridgeState,
    raw_session_id: &str,
) -> Result<SessionDraftAuthority, String> {
    let session_id = raw_session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{raw_session_id}': {error}"))?;
    let (controller, registry, resource_registry) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        (
            guard.controller_runtime.clone().ok_or_else(|| {
                String::from("service is still starting; controller runtime is not ready")
            })?,
            Arc::clone(&guard.session_runtime_registry),
            Arc::clone(&guard.project_resource_catalog_registry),
        )
    };
    let handle = get_or_load_session_v1(&controller, registry.as_ref(), session_id)
        .await
        .map_err(|error| error.to_string())?;
    let locator = handle
        .storage_locator()
        .map_err(|error| error.to_string())?;
    let locator_session_id = locator
        .session_id()
        .parse::<SessionId>()
        .map_err(|error| format!("invalid V1 draft locator session id: {error}"))?;
    if locator_session_id != session_id {
        return Err(format!(
            "V1 draft locator identity mismatch: expected '{raw_session_id}', actual '{}'",
            locator.session_id()
        ));
    }
    Ok(SessionDraftAuthority {
        handle,
        locator,
        controller,
        resource_registry,
    })
}

/// Resolves ordered staged and project-resource attachments into canonical host facts.
/// 把有序暂存附件与项目资源附件解析为规范宿主事实。
fn resolve_draft_attachments(
    state: &SharedServiceBridgeState,
    locator: &SessionV1StorageLocator,
    summaries: Vec<ComposerAttachmentSummary>,
) -> Result<Vec<ConversationAttachment>, String> {
    let needs_staged_workspace = summaries
        .iter()
        .any(|summary| summary.project_resource_id.is_none());
    let staged_workspace_path = if needs_staged_workspace {
        let workspace_snapshot = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .workspace_snapshot
            .clone()
            .ok_or_else(|| {
                String::from("workspace snapshot is unavailable for staged draft attachments")
            })?;
        Some(composer_attachment_workspace_path_from_snapshot(
            &workspace_snapshot,
            Some(locator.project_id()),
            Some(locator.session_id()),
        )?)
    } else {
        None
    };
    summaries
        .into_iter()
        .map(|summary| {
            let canonical = match summary.project_resource_id {
                Some(resource_id) => {
                    validate_project_resource_attachment_reference(&summary)?;
                    composer_attachment_from_project_resource_v1(locator, resource_id)?
                }
                None => resolve_composer_attachment_summary_for_workspace(
                    staged_workspace_path.as_deref().ok_or_else(|| {
                        String::from("staged attachment workspace was not resolved")
                    })?,
                    summary,
                )?,
            };
            composer_attachment_summary_to_core(canonical)
        })
        .collect()
}

/// Projects the current metadata draft and ordered project-resource states into one response.
/// 把当前 metadata 草稿与有序项目资源状态投影为一份响应。
fn draft_response_from_authority(
    handle: &SessionHandle,
    locator: &SessionV1StorageLocator,
) -> Result<SessionDraftV1Response, String> {
    let draft = handle
        .read(|document| document.metadata.input_state.draft.clone())
        .map_err(|error| error.to_string())?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory)
        .map_err(|error| format!("failed to load project resource catalog: {error}"))?;
    let resources = draft
        .resource_ids
        .iter()
        .map(|resource_id| {
            resolve_project_resource(&project_directory, &catalog, *resource_id)
                .map(resource_summary_from_resolution)
                .map_err(|error| format!("failed to resolve project resource: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SessionDraftV1Response {
        session_id: locator.session_id().to_string(),
        text: draft.text,
        resources,
    })
}
