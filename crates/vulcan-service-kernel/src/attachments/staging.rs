//! Composer attachment staging helpers.
//! 输入区附件暂存辅助逻辑。

use super::*;
use crate::{
    session_summary_project_id, workspace_session_summary_for_session,
    workspace_task_project_for_project_id,
};

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
        return Ok(default_composer_attachment_workspace_path());
    };
    let owner_project_id = conversation_id
        .and_then(parse_session_id)
        .and_then(|session_id| {
            workspace_session_summary_for_session(workspace_snapshot, session_id)
                .and_then(session_summary_project_id)
        })
        .or(project_id)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(owner_project_id) = owner_project_id else {
        return Ok(default_composer_attachment_workspace_path());
    };
    Ok(
        workspace_task_project_for_project_id(workspace_snapshot, owner_project_id)
            .map(|project| project.path.clone())
            .unwrap_or_else(default_composer_attachment_workspace_path),
    )
}

/// Returns the fallback workspace path used by composer attachment staging.
/// 返回输入区附件暂存使用的回退工作区路径。
///
/// # Returns
/// 返回值
/// The normalized process workspace root path used when no project path resolves.
/// 当无法解析项目路径时使用的规范化进程工作区根路径。
fn default_composer_attachment_workspace_path() -> String {
    normalized_workspace_path_text_from_path(&workspace_root())
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use super::*;
    use crate::{ServiceShellHost, SharedServiceBridgeState, create_service_bridge_state};
    use vulcan_workbench_model::{SessionId, SessionState, SessionSummary, TaskProjectSummary};
    use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

    /// Test shell host used by attachment staging path-resolution tests.
    /// 附件暂存路径解析测试使用的测试宿主壳。
    struct AttachmentStagingTestShellHost;

    impl ServiceShellHost for AttachmentStagingTestShellHost {
        /// Rejects host file-manager use in attachment staging tests.
        /// 在附件暂存测试中拒绝使用宿主文件管理器。
        fn open_path(&self, _path: &str) -> Result<(), String> {
            Err(String::from(
                "attachment staging tests do not open host paths",
            ))
        }

        /// Rejects host browser use in attachment staging tests.
        /// 在附件暂存测试中拒绝使用宿主浏览器。
        fn open_url(&self, _url: &str) -> Result<(), String> {
            Err(String::from(
                "attachment staging tests do not open host URLs",
            ))
        }
    }

    /// Builds service state with one retained workspace snapshot.
    /// 构建带有单个保留工作区快照的服务状态。
    ///
    /// # Parameters
    /// 参数
    /// `workspace_snapshot` is the retained workspace snapshot under test.
    /// `workspace_snapshot` 是测试中的保留工作区快照。
    ///
    /// # Returns
    /// 返回值
    /// Shared service state containing the supplied workspace snapshot.
    /// 返回包含给定工作区快照的共享服务状态。
    fn service_state_with_workspace_snapshot(
        workspace_snapshot: WorkspaceSnapshot,
    ) -> SharedServiceBridgeState {
        // state stores the minimal bridge state required by attachment staging.
        // state 保存附件暂存所需的最小 bridge 状态。
        let state = create_service_bridge_state(Arc::new(AttachmentStagingTestShellHost));
        // guard mutates the retained workspace snapshot before the test reads it.
        // guard 在测试读取前写入保留工作区快照。
        let mut guard = state.lock().expect("test service state lock must be valid");
        guard.workspace_snapshot = Some(workspace_snapshot);
        drop(guard);
        state
    }

    /// Builds a workspace snapshot with one project and one session.
    /// 构建包含一个项目与一个会话的工作区快照。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the retained conversation id.
    /// `session_id` 是保留的会话标识。
    /// `project_id` is the project id stored on that session.
    /// `project_id` 是写入该会话的项目标识。
    ///
    /// # Returns
    /// 返回值
    /// A workspace snapshot suitable for attachment staging path tests.
    /// 返回适用于附件暂存路径测试的工作区快照。
    fn workspace_snapshot_for_attachment_staging(
        session_id: SessionId,
        project_id: Option<String>,
    ) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects: vec![TaskProjectSummary::new(
                "project-a",
                "Project A",
                r"D:\projects\project-a",
                false,
                0,
                None,
                None,
                Vec::new(),
            )],
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                project_id,
                "附件暂存测试会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Verifies conversation-owned attachment staging resolves the owning project path.
    /// 验证会话所属附件暂存会解析到所属项目路径。
    #[test]
    fn composer_attachment_workspace_path_resolves_conversation_project() {
        // session_id identifies the conversation supplied to the staging path resolver.
        // session_id 标识传给暂存路径解析器的会话。
        let session_id = SessionId::new_v4();
        // conversation_id stores the WebUI conversation id text.
        // conversation_id 保存 WebUI 会话标识文本。
        let conversation_id = session_id.to_string();
        // state contains a session project id with boundary whitespace to preserve trim behavior.
        // state 包含带边界空白的会话项目标识，用于保持 trim 行为。
        let state =
            service_state_with_workspace_snapshot(workspace_snapshot_for_attachment_staging(
                session_id,
                Some(String::from(" project-a ")),
            ));

        assert_eq!(
            composer_attachment_workspace_path(&state, None, Some(&conversation_id))
                .expect("conversation project should resolve to a workspace path"),
            r"D:\projects\project-a"
        );
    }

    /// Verifies explicit project ids resolve through the backend project snapshot.
    /// 验证明示项目标识会通过后端项目快照解析。
    #[test]
    fn composer_attachment_workspace_path_resolves_explicit_project_id() {
        // session_id identifies the retained session in the workspace fixture.
        // session_id 标识工作区夹具中的保留会话。
        let session_id = SessionId::new_v4();
        // state owns the project snapshot used by the staging resolver.
        // state 持有暂存解析器使用的项目快照。
        let state = service_state_with_workspace_snapshot(
            workspace_snapshot_for_attachment_staging(session_id, Some(String::from("project-a"))),
        );

        assert_eq!(
            composer_attachment_workspace_path(&state, Some(" project-a "), None)
                .expect("explicit project should resolve to a workspace path"),
            r"D:\projects\project-a"
        );
    }

    /// Verifies blank conversation project ids fall back to the workspace root.
    /// 验证空白会话项目标识会回退到工作区根目录。
    #[test]
    fn composer_attachment_workspace_path_falls_back_for_blank_conversation_project() {
        // session_id identifies the conversation carrying a blank project id.
        // session_id 标识携带空白项目标识的会话。
        let session_id = SessionId::new_v4();
        // conversation_id stores the WebUI conversation id text.
        // conversation_id 保存 WebUI 会话标识文本。
        let conversation_id = session_id.to_string();
        // state keeps the blank project id so the fallback branch is observable.
        // state 保留空白项目标识，以便观察回退分支。
        let state = service_state_with_workspace_snapshot(
            workspace_snapshot_for_attachment_staging(session_id, Some(String::from("   "))),
        );

        assert_eq!(
            composer_attachment_workspace_path(&state, None, Some(&conversation_id))
                .expect("blank conversation project should resolve through fallback"),
            default_composer_attachment_workspace_path()
        );
    }
}
