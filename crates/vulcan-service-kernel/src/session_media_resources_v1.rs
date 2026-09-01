//! Exact registered-session project resource access for host media tools.
//! 宿主媒体工具使用的精确已注册会话项目资源访问。

use crate::session_runtime_registry_v1::SessionRuntimeRegistry;
use crate::state::SharedServiceBridgeState;
use std::sync::Arc;
use vulcan_agent_runtime::media_resource_runtime::MediaResourceRuntime;
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_session_core::v1::ResourceId;
use vulcan_workbench_model::SessionId;
use vulcan_workspace_store::workspace_store::{
    load_project_resource_catalog, project_resource_locator, resolve_project_resource_source,
};

/// Read-only port backed by the same registered session owner used by physical V1 requests.
/// 由物理V1请求使用的同一已注册会话所有者提供的只读端口。
struct SessionMediaResources {
    /// Exact registry; caller-supplied workspace paths never select the project partition.
    /// 精确注册表；调用方工作区路径绝不用于选择项目分区。
    registry: Arc<SessionRuntimeRegistry>,
}

impl std::fmt::Debug for SessionMediaResources {
    /// Formats only the port kind into `formatter`, never registry contents or private project paths.
    /// 仅向`formatter`格式化端口类型，绝不输出注册表内容或私有项目路径。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionMediaResources")
            .finish_non_exhaustive()
    }
}

/// Captures the exact V1 registry from `state` without retaining the service mutex across media work.
/// 从`state`捕获精确V1注册表，不在媒体工作期间保留服务互斥锁。
pub(crate) fn session_v1_media_resource_runtime(
    state: &SharedServiceBridgeState,
) -> Result<Arc<dyn MediaResourceRuntime>, String> {
    let registry = state
        .lock()
        .map_err(|_| String::from("Media resource owner is unavailable."))?
        .session_runtime_registry
        .clone();
    Ok(Arc::new(SessionMediaResources { registry }))
}

#[async_trait::async_trait]
impl MediaResourceRuntime for SessionMediaResources {
    /// Resolves one project resource for the host-owned session; source bytes remain unread until admission.
    /// 为宿主持有的会话解析一个项目资源；准入之前不读取来源字节。
    async fn resolve_project_resource(
        &self,
        session_id: &str,
        resource_id: ResourceId,
    ) -> Result<ConversationAttachment, String> {
        // The active session's validated locator is the only project authority, including embedded child sessions.
        // 活动会话的已校验定位器是唯一项目权威，也涵盖内嵌子会话。
        let session_id = session_id
            .parse::<SessionId>()
            .map_err(|_| String::from("Invalid media resource session identity."))?;
        let handle = self
            .registry
            .get(session_id)
            .map_err(|_| String::from("Media resource session is not registered."))?;
        let project = handle
            .storage_locator()
            .map_err(|_| String::from("Media resource session has no storage locator."))?
            .project_directory();
        tokio::task::spawn_blocking(move || {
            let catalog = load_project_resource_catalog(&project)
                .map_err(|_| String::from("Project resource catalog is unavailable or invalid."))?;
            let source = resolve_project_resource_source(&project, &catalog, resource_id)
                .map_err(|_| String::from("Project resource source is unavailable or invalid."))?
                .ok_or_else(|| String::from("Project resource source is missing."))?;
            let mime = source
                .entry
                .mime
                .clone()
                .unwrap_or_else(|| String::from("application/octet-stream"));
            let normalized = mime.to_ascii_lowercase();
            let kind = if normalized.starts_with("image/") {
                ConversationAttachmentKind::Image
            } else if normalized.starts_with("audio/") {
                ConversationAttachmentKind::Audio
            } else if normalized.starts_with("video/") {
                ConversationAttachmentKind::Video
            } else if normalized == "application/pdf" {
                ConversationAttachmentKind::Pdf
            } else {
                ConversationAttachmentKind::File
            };
            Ok(ConversationAttachment {
                kind,
                file_name: source.entry.original_name,
                local_path: vulcan_runtime_assets::native_host_path_text_from_path(&source.path),
                resource_path: Some(project_resource_locator(resource_id)),
                source_path: None,
                asset_id: None,
                byte_len: Some(source.entry.size),
                source_sha256: Some(source.entry.sha256),
                delivery_mode: None,
                preview_local_path: None,
                mime_type: mime,
            })
        })
        .await
        .map_err(|_| String::from("Project media resource resolution task failed."))?
    }
}

#[cfg(test)]
mod tests;
