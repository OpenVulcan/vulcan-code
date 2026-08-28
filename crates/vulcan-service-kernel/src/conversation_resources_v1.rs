//! Authoritative V1 project-resource metadata and content reads.
//! 权威 V1 项目资源元数据与内容读取。

use vulcan_service_api::{
    ComposerAttachmentSummary, SESSION_RESOURCE_LIST_V1_MAX_ITEMS, ServiceMediaResponse,
    SessionResourceContentV1Request, SessionResourceListV1Request, SessionResourceListV1Response,
    SessionResourceSummaryV1, build_service_project_resource_response,
    service_media_error_response,
};
use vulcan_session_core::v1::ids::ResourceId;
use vulcan_workbench_model::SessionId;
use vulcan_workspace_store::workspace_store::{
    ProjectResourceResolution, SessionV1StorageLocator, load_project_resource_catalog,
    locate_session_v1_storage, resolve_project_resource,
};

use crate::session_runtime_registry_v1::{SessionRuntimeError, SessionRuntimeRegistry};
use crate::state::SharedServiceBridgeState;

/// Resolves browser-safe metadata for ordered project resources owned by one V1 session.
/// 解析由一个 V1 会话所属项目拥有的有序项目资源浏览器安全元数据。
///
/// # Parameters
/// 参数
/// `state` owns the exact V1 registry and SQLite controller, while `request` contains only the
/// session identity and ordered project resource ids.
/// `state` 拥有精确 V1 注册表与 SQLite 控制器，`request` 仅包含会话身份和有序项目资源 ID。
///
/// # Returns
/// 返回值
/// Returns display metadata in exact request order without exposing physical locators.
/// 返回按请求精确顺序排列的展示元数据，且不暴露物理定位信息。
pub(crate) async fn service_session_resource_list_v1(
    state: SharedServiceBridgeState,
    request: SessionResourceListV1Request,
) -> Result<SessionResourceListV1Response, String> {
    if request.resource_ids.len() > SESSION_RESOURCE_LIST_V1_MAX_ITEMS {
        return Err(format!(
            "session resource request contains {} items; maximum is {SESSION_RESOURCE_LIST_V1_MAX_ITEMS}",
            request.resource_ids.len()
        ));
    }
    let locator = resolve_session_resource_locator(&state, &request.session_id).await?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory)
        .map_err(|error| format!("failed to load project resource catalog: {error}"))?;
    let mut resources = Vec::with_capacity(request.resource_ids.len());
    for raw_resource_id in request.resource_ids {
        let resource_id = checked_resource_id(raw_resource_id)?;
        resources.push(resource_summary_from_resolution(
            resolve_project_resource(&project_directory, &catalog, resource_id)
                .map_err(|error| format!("failed to resolve project resource: {error}"))?,
        ));
    }
    Ok(SessionResourceListV1Response {
        session_id: locator.session_id().to_string(),
        resources,
    })
}

/// Resolves one controlled V1 project-resource content response.
/// 解析一个受控 V1 项目资源内容响应。
///
/// # Parameters
/// 参数
/// `state` owns exact locator authority and `request` carries only logical route identity plus
/// bounded HTTP delivery metadata.
/// `state` 拥有精确定位权威，`request` 仅携带逻辑路由身份与有界 HTTP 传输元数据。
///
/// # Returns
/// 返回值
/// Returns digest-validated bytes, a fixed missing placeholder, or an explicit corruption error.
/// 返回已通过摘要校验的字节、固定缺失占位符或显式损坏错误。
pub async fn service_session_resource_content_v1(
    state: SharedServiceBridgeState,
    request: SessionResourceContentV1Request,
) -> Result<ServiceMediaResponse, String> {
    let locator = resolve_session_resource_locator(&state, &request.session_id).await?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory)
        .map_err(|error| format!("failed to load project resource catalog: {error}"))?;
    let resource_id = checked_resource_id(request.resource_id)?;
    match resolve_project_resource(&project_directory, &catalog, resource_id)
        .map_err(|error| format!("failed to resolve project resource: {error}"))?
    {
        ProjectResourceResolution::Available { entry, bytes } => {
            build_service_project_resource_response(
                &bytes,
                entry.mime.as_deref(),
                request.is_head,
                request.range_header.as_deref(),
            )
        }
        ProjectResourceResolution::Missing { placeholder, .. } => {
            Ok(service_media_error_response(404, placeholder))
        }
    }
}

/// Resolves one session locator from the active owner first and SQLite otherwise.
/// 优先从活动所有者解析会话定位器，否则从 SQLite 解析。
///
/// Embedded task sessions are never indexed in SQLite, so an active registered handle is their
/// exact no-scan authority. Indexed root sessions use the same handle when resident and their
/// SQLite row when inactive.
/// 内嵌任务会话绝不写入 SQLite，因此活动注册句柄是其无需扫描的精确权威。已索引根会话驻留时使用同一句柄，
/// 非活动时使用其 SQLite 记录。
async fn resolve_session_resource_locator<'state>(
    state: &'state SharedServiceBridgeState,
    raw_session_id: &'state str,
) -> Result<SessionV1StorageLocator, String> {
    let session_id = raw_session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{raw_session_id}': {error}"))?;
    let (controller, registry) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        (
            controller,
            std::sync::Arc::clone(&guard.session_runtime_registry),
        )
    };
    match registered_locator(&registry, session_id)? {
        Some(locator) => Ok(locator),
        None => locate_session_v1_storage(&controller, raw_session_id)
            .await
            .map_err(|error| format!("failed to locate V1 session '{raw_session_id}': {error}")),
    }
}

/// Returns one registered session locator while treating only absence as an inactive session.
/// 返回一个已注册会话定位器，且仅把缺席视为非活动会话。
fn registered_locator(
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
) -> Result<Option<SessionV1StorageLocator>, String> {
    match registry.get(session_id) {
        Ok(handle) => handle
            .storage_locator()
            .map(Some)
            .map_err(|error| format!("failed to read registered V1 locator: {error}")),
        Err(SessionRuntimeError::SessionNotRegistered(_)) => Ok(None),
        Err(error) => Err(format!("failed to read V1 session registry: {error}")),
    }
}

/// Validates one JSON-safe project resource identity.
/// 校验一个 JSON 安全的项目资源身份。
pub(crate) fn checked_resource_id(raw_resource_id: u64) -> Result<ResourceId, String> {
    ResourceId::try_new(raw_resource_id)
        .map_err(|error| format!("invalid project resource id {raw_resource_id}: {error}"))
}

/// Projects one storage resolution into the browser-safe discriminated response.
/// 将一个存储解析结果投影为浏览器安全的判别响应。
pub(crate) fn resource_summary_from_resolution(
    resolution: ProjectResourceResolution,
) -> SessionResourceSummaryV1 {
    match resolution {
        ProjectResourceResolution::Available { entry, .. } => SessionResourceSummaryV1::Available {
            resource_id: entry.resource_id.value(),
            original_name: entry.original_name,
            mime: entry.mime,
            size: entry.size,
        },
        ProjectResourceResolution::Missing {
            resource_id,
            placeholder,
        } => SessionResourceSummaryV1::Missing {
            resource_id: resource_id.value(),
            placeholder: placeholder.to_string(),
        },
    }
}

/// Rebuilds one canonical composer attachment from an exact V1 project resource.
/// 从一个精确 V1 项目资源重建一条规范输入区附件。
///
/// # Parameters
/// 参数
/// `locator` owns the exact project partition and `raw_resource_id` is the project-scoped logical id.
/// `locator` 持有精确项目分区，`raw_resource_id` 是项目级逻辑 ID。
///
/// # Returns
/// 返回值
/// Returns an integrity-validated host attachment carrying the logical ResourceId, or an explicit missing/corrupt failure.
/// 返回一条携带逻辑 ResourceId 且通过完整性校验的宿主附件；资源缺失或损坏时显式失败。
pub(crate) fn composer_attachment_from_project_resource_v1(
    locator: &SessionV1StorageLocator,
    raw_resource_id: u64,
) -> Result<ComposerAttachmentSummary, String> {
    let resource_id = checked_resource_id(raw_resource_id)?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory)
        .map_err(|error| format!("failed to load project resource catalog: {error}"))?;
    let ProjectResourceResolution::Available { entry, bytes } =
        resolve_project_resource(&project_directory, &catalog, resource_id)
            .map_err(|error| format!("failed to resolve project resource: {error}"))?
    else {
        return Err(format!(
            "project resource {} is missing and cannot be submitted",
            resource_id.value()
        ));
    };
    let mime_type = entry
        .mime
        .clone()
        .unwrap_or_else(|| String::from("application/octet-stream"));
    let local_path = project_directory
        .join(&entry.entity_relative_path)
        .to_string_lossy()
        .into_owned();
    Ok(ComposerAttachmentSummary {
        kind: composer_attachment_kind_from_mime(&mime_type).to_string(),
        file_name: entry.original_name,
        local_path,
        project_resource_id: Some(resource_id.value()),
        resource_path: None,
        source_path: None,
        asset_id: None,
        byte_len: Some(bytes.len() as u64),
        source_sha256: Some(entry.sha256),
        delivery_plan: None,
        preview_local_path: None,
        mime_type,
    })
}

/// Rejects contradictory staged-asset fields on one project ResourceId reference.
/// 拒绝一条项目 ResourceId 引用上互相矛盾的暂存资产字段。
pub(crate) fn validate_project_resource_attachment_reference(
    summary: &ComposerAttachmentSummary,
) -> Result<(), String> {
    if summary.asset_id.is_some()
        || summary.resource_path.is_some()
        || summary.source_path.is_some()
        || summary.source_sha256.is_some()
        || summary.delivery_plan.is_some()
        || summary.preview_local_path.is_some()
    {
        return Err(String::from(
            "Project ResourceId attachment must not carry staged-asset or request-local fields.",
        ));
    }
    Ok(())
}

/// Classifies one authoritative MIME type into the stable composer attachment kind.
/// 把一个权威 MIME 类型分类为稳定输入区附件类型。
fn composer_attachment_kind_from_mime(mime_type: &str) -> &'static str {
    if mime_type == "application/pdf" {
        "pdf"
    } else if mime_type.starts_with("image/") {
        "image"
    } else if mime_type.starts_with("audio/") {
        "audio"
    } else if mime_type.starts_with("video/") {
        "video"
    } else {
        "file"
    }
}
