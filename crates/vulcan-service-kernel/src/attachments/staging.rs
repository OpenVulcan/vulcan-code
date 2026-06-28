//! Composer attachment staging helpers.
//! 输入区附件暂存辅助逻辑。

use super::*;

/// Rejects the retired native attachment picker RPC.
/// 拒绝已退役的原生附件选择器 RPC。
///
/// # Parameters
/// 参数
/// `_state` is retained for the retired RPC signature.
/// `_state` 为已退役 RPC 签名保留。
/// `_project_id` is retained for the retired RPC signature.
/// `_project_id` 为已退役 RPC 签名保留。
/// `_conversation_id` is retained for the retired RPC signature.
/// `_conversation_id` 为已退役 RPC 签名保留。
/// `_kind` is retained for the retired RPC signature.
/// `_kind` 为已退役 RPC 签名保留。
///
/// # Returns
/// 返回值
/// Always returns an error directing callers to WebUI upload and path staging APIs.
/// 始终返回错误，引导调用方使用 WebUI 上传与路径暂存接口。
pub(crate) async fn service_pick_composer_attachments(
    _state: SharedServiceBridgeState,
    _project_id: Option<String>,
    _conversation_id: Option<String>,
    _kind: Option<String>,
) -> Result<Vec<ComposerAttachmentSummary>, String> {
    Err(String::from(
        "原生附件选择器已从服务壳层移除，请使用 WebUI 文件上传或路径暂存接口。",
    ))
}

/// Stages existing local paths as composer attachments.
/// 将已有本地路径暂存为输入区附件。
///
/// # Parameters
/// 参数
/// `state` owns the workspace snapshot used to resolve the staging root.
/// `state` 持有用于解析暂存根目录的工作区快照。
/// `project_id` optionally selects the owning project.
/// `project_id` 可选指定所属项目。
/// `conversation_id` optionally selects a conversation whose project owns the attachment.
/// `conversation_id` 可选指定拥有附件项目的会话。
/// `paths` contains local filesystem paths to stage.
/// `paths` 包含要暂存的本地文件系统路径。
///
/// # Returns
/// 返回值
/// Composer attachment summaries for the staged local files.
/// 已暂存本地文件的输入区附件摘要。
pub(crate) async fn service_stage_composer_attachment_paths(
    state: SharedServiceBridgeState,
    project_id: Option<String>,
    conversation_id: Option<String>,
    paths: Vec<String>,
) -> Result<Vec<ComposerAttachmentSummary>, String> {
    let workspace_path = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    stage_composer_attachment_paths_for_workspace(&workspace_path, paths)
}

/// Stages one browser-provided in-memory file as a composer attachment.
/// 将浏览器提供的一份内存文件暂存为输入区附件。
///
/// # Parameters
/// 参数
/// `state` owns the workspace snapshot used to resolve the staging root.
/// `state` 持有用于解析暂存根目录的工作区快照。
/// `project_id` optionally selects the owning project.
/// `project_id` 可选指定所属项目。
/// `conversation_id` optionally selects a conversation whose project owns the attachment.
/// `conversation_id` 可选指定拥有附件项目的会话。
/// `file_name` is the browser-provided display file name.
/// `file_name` 是浏览器提供的展示文件名。
/// `mime_type` is the browser-provided MIME type.
/// `mime_type` 是浏览器提供的 MIME 类型。
/// `data_base64` is the base64-encoded file content.
/// `data_base64` 是 base64 编码的文件内容。
///
/// # Returns
/// 返回值
/// A composer attachment summary for the staged in-memory file.
/// 已暂存内存文件的输入区附件摘要。
pub(crate) async fn service_stage_composer_attachment_bytes(
    state: SharedServiceBridgeState,
    project_id: Option<String>,
    conversation_id: Option<String>,
    file_name: String,
    mime_type: String,
    data_base64: String,
) -> Result<ComposerAttachmentSummary, String> {
    let workspace_path = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let descriptor = attachment_descriptor_from_mime_type(&mime_type)
        .or_else(|| attachment_descriptor_from_path_fallback(&file_name))
        .ok_or_else(|| format!("文件 `{}` 不是受支持的附件格式。", file_name.trim()))?;
    let bytes = BASE64_STANDARD
        .decode(data_base64.trim())
        .map_err(|error| format!("解码附件 `{}` 失败：{}。", file_name.trim(), error))?;
    let attachment = stage_attachment_bytes(
        &workspace_path,
        &descriptor,
        file_name.trim(),
        None,
        bytes.as_slice(),
    )?;
    Ok(composer_attachment_summary_from_core(&attachment))
}

/// Resolves the project workspace path used to stage one composer attachment.
/// 解析暂存输入区附件时使用的项目工作区路径。
///
/// # Parameters
/// 参数
/// `state` owns the workspace snapshot used for project lookup.
/// `state` 持有用于项目查找的工作区快照。
/// `project_id` optionally provides an explicit project id.
/// `project_id` 可选提供显式项目标识。
/// `conversation_id` optionally provides a conversation id whose project should be used.
/// `conversation_id` 可选提供需要使用其项目的会话标识。
///
/// # Returns
/// 返回值
/// The resolved workspace path text used by attachment staging.
/// 附件暂存使用的已解析工作区路径文本。
fn composer_attachment_workspace_path(
    state: &SharedServiceBridgeState,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
) -> Result<String, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() else {
        return Ok(normalized_workspace_path_text_from_path(&workspace_root()));
    };
    let owner_project_id = conversation_id
        .and_then(parse_session_id)
        .and_then(|session_id| {
            workspace_snapshot
                .sessions
                .iter()
                .find(|session| session.id == session_id)
                .and_then(|session| session.project_id.as_deref())
        })
        .or(project_id)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(owner_project_id) = owner_project_id else {
        return Ok(normalized_workspace_path_text_from_path(&workspace_root()));
    };
    Ok(workspace_snapshot
        .task_projects
        .iter()
        .find(|project| project.id == owner_project_id)
        .map(|project| project.path.clone())
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root())))
}

/// Stages local paths for a resolved workspace path and converts them to service DTOs.
/// 为已解析的工作区路径暂存本地路径，并转换为 service DTO。
///
/// # Parameters
/// 参数
/// `workspace_path` is the resolved project workspace path.
/// `workspace_path` 是已解析的项目工作区路径。
/// `paths` contains local filesystem paths to stage.
/// `paths` 包含要暂存的本地文件系统路径。
///
/// # Returns
/// 返回值
/// Composer attachment summaries for every staged path.
/// 每个已暂存路径对应的输入区附件摘要。
fn stage_composer_attachment_paths_for_workspace(
    workspace_path: &str,
    paths: Vec<PathBuf>,
) -> Result<Vec<ComposerAttachmentSummary>, String> {
    paths
        .iter()
        .map(|path| stage_attachment_file(workspace_path, path))
        .map(|result| result.map(|attachment| composer_attachment_summary_from_core(&attachment)))
        .collect()
}
