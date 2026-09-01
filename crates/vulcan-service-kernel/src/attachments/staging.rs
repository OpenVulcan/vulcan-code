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
        .or_else(|| attachment_descriptor_from_path_fallback(&file_name));
    let bytes = BASE64_STANDARD
        .decode(data_base64.trim())
        .map_err(|error| format!("解码附件 `{}` 失败：{}。", file_name.trim(), error))?;
    let attachment = if let Some(descriptor) = descriptor {
        stage_attachment_bytes(
            &workspace_path,
            &descriptor,
            file_name.trim(),
            None,
            bytes.as_slice(),
        )?
    } else {
        vulcan_agent_runtime::attachment_assets::stage_generic_attachment_bytes(
            &workspace_path,
            file_name.trim(),
            None,
            &mime_type,
            bytes.as_slice(),
        )?
    };
    Ok(composer_attachment_summary_from_core(&attachment))
}

/// Binds one browser-derived JPEG preview sidecar to an already staged managed image.
/// 将一份浏览器派生的 JPEG 预览旁路文件绑定到已暂存的托管图片。
///
/// # Parameters
/// 参数
/// `state` owns the workspace snapshot, `project_id` and `conversation_id` resolve the project scope, `attachment_id` is the staged asset id, and `data_base64` carries the derived JPEG bytes.
/// `state` 持有工作区快照，`project_id` 与 `conversation_id` 解析项目范围，`attachment_id` 是已暂存资产标识，`data_base64` 携带派生 JPEG 字节。
///
/// # Returns
/// 返回值
/// The native host path of the bound preview sidecar.
/// 返回已绑定预览旁路文件的宿主原生路径。
pub(crate) async fn service_stage_composer_attachment_preview_bytes(
    state: SharedServiceBridgeState,
    project_id: Option<String>,
    conversation_id: Option<String>,
    attachment_id: String,
    data_base64: String,
) -> Result<String, String> {
    let workspace_path = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let attachment_id = attachment_id.trim();
    if attachment_id.is_empty() {
        return Err(String::from(
            "Attachment image preview requires a staged asset id.",
        ));
    }
    let attachment = resolve_indexed_attachment_resource(
        &workspace_path,
        format!("attachment://{attachment_id}").as_str(),
        ConversationAttachmentKind::Image,
    )?;
    let bytes = BASE64_STANDARD
        .decode(data_base64.trim())
        .map_err(|error| format!("Failed to decode attachment image preview: {error}"))?;
    write_attachment_image_preview_sidecar(&attachment, bytes.as_slice())
}

/// Reconciles one durable composer-draft lease with its complete canonical attachment set.
/// 使用完整规范附件集合协调一条持久输入区草稿租约。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative project/session lookup snapshot.
/// `state` 持有权威项目与会话查找快照。
/// `project_id` and `conversation_id` identify the exact attachment workspace.
/// `project_id` 与 `conversation_id` 标识精确附件工作区。
/// `draft_slot_key` is the stable browser draft owner selected by the composer.
/// `draft_slot_key` 是输入区选定的稳定浏览器草稿所有者。
/// `attachments` is the complete service-issued draft attachment set.
/// `attachments` 是完整的服务签发草稿附件集合。
///
/// # Returns
/// 返回值
/// Whether the durable owner set changed, or an explicit ownership/integrity error.
/// 返回持久所有者集合是否变化，或明确的所有权/完整性错误。
pub(crate) async fn service_reconcile_composer_attachment_draft(
    state: SharedServiceBridgeState,
    project_id: Option<String>,
    conversation_id: Option<String>,
    draft_slot_key: String,
    attachments: Vec<ComposerAttachmentSummary>,
) -> Result<bool, String> {
    let workspace_path = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    // Validate the exact slot owner before changing any managed asset lease.
    // 改变任何托管资产租约之前验证精确槽位所有者。
    let scope = composer_attachment_draft_scope_for_owner(
        &draft_slot_key,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let attachments = attachments
        .into_iter()
        .filter_map(|summary| {
            if let Some(resource_id) = summary.project_resource_id {
                // Project resources are retained by the V1 catalog, never by an asset-index staging lease.
                // 项目资源由V1目录保留，绝不由资产索引暂存租约保留。
                let validation = if conversation_id.is_none() { Err(String::from("Project draft references require a materialized session.")) } else {
                    crate::conversation_resources_v1::checked_resource_id(resource_id).and_then(|_| crate::conversation_resources_v1::validate_project_resource_attachment_reference(&summary))
                };
                return validation.err().map(Err);
            }
            Some(resolve_composer_attachment_summary_for_workspace(&workspace_path, summary)
                .and_then(composer_attachment_summary_to_core))
        })
        .collect::<Result<Vec<_>, _>>()?;
    reconcile_staged_attachment_ref_scope(&workspace_path, &scope, attachments.as_slice())
}

/// Builds the internal staging-reference scope for one exact composer draft slot.
/// 为一个精确输入区草稿槽构建内部暂存引用作用域。
///
/// # Parameters
/// 参数
/// `draft_slot_key` is the canonical `conversation:` or `new:` slot identity from WebUI.
/// `draft_slot_key` 是来自 WebUI 的规范 `conversation:` 或 `new:` 草稿槽身份。
///
/// # Returns
/// 返回值
/// The namespaced internal scope, or an explicit invalid-slot error.
/// 返回带命名空间的内部作用域，或明确的无效草稿槽错误。
pub(crate) fn composer_attachment_draft_scope(draft_slot_key: &str) -> Result<String, String> {
    if draft_slot_key.chars().any(char::is_control) {
        return Err(String::from(
            "Composer attachment draft slot must be one bounded conversation: or new: identity.",
        ));
    }
    let draft_slot_key = draft_slot_key.trim();
    let has_valid_prefix = draft_slot_key
        .strip_prefix("conversation:")
        .or_else(|| draft_slot_key.strip_prefix("new:"))
        .is_some_and(|owner| !owner.trim().is_empty());
    if !has_valid_prefix || draft_slot_key.len() > 512 {
        return Err(String::from(
            "Composer attachment draft slot must be one bounded conversation: or new: identity.",
        ));
    }
    Ok(format!("composer-draft:v1:{draft_slot_key}"))
}

/// Validates `draft_slot_key` against the selected `conversation_id` or new-draft `project_id`.
/// 对照所选`conversation_id`或新草稿`project_id`验证`draft_slot_key`。
/// Returns the canonical lease scope only for that exact owner; workspace is the declared ownerless draft.
/// 仅对该精确所有者返回规范租约作用域；工作区是声明的无主草稿。
pub(crate) fn composer_attachment_draft_scope_for_owner(
    draft_slot_key: &str,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
) -> Result<String, String> {
    // Syntax alone cannot prevent a different draft in the same project from acquiring this lease.
    // 仅语法校验无法阻止同项目另一草稿取得本租约。
    let scope = composer_attachment_draft_scope(draft_slot_key)?;
    // Session identity is canonicalized by the existing parser, matching prompt submission admission.
    // 会话身份由既有解析器规范化，与提示词提交准入一致。
    let expected = match conversation_id {
        Some(conversation_id) => {
            // Only a valid exact session may own a conversation slot.
            // 只有有效精确会话可以持有会话槽位。
            let session_id = parse_session_id(conversation_id.trim())
                .ok_or_else(|| String::from("Attachment conversation id is invalid."))?;
            format!("conversation:{session_id}")
        }
        None => format!(
            "new:{}",
            project_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("workspace")
        ),
    };
    if draft_slot_key.trim() != expected {
        return Err(String::from(
            "Composer attachment draft slot does not own the selected conversation target.",
        ));
    }
    Ok(scope)
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
pub(crate) fn composer_attachment_workspace_path(
    state: &SharedServiceBridgeState,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
) -> Result<String, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() else {
        if project_id.is_some_and(|value| !value.trim().is_empty())
            || conversation_id.is_some_and(|value| !value.trim().is_empty())
        {
            return Err(String::from(
                "Attachment project scope is unavailable while the workspace is loading.",
            ));
        }
        return Ok(default_composer_attachment_workspace_path());
    };
    composer_attachment_workspace_path_from_snapshot(
        workspace_snapshot,
        project_id,
        conversation_id,
    )
}

/// Resolves one authoritative attachment workspace path from an already loaded workspace snapshot.
/// 从已经加载的工作区快照解析一条权威附件工作区路径。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns project/session identities, while `project_id` and `conversation_id` select the exact attachment owner.
/// `workspace_snapshot` 持有项目/会话身份，`project_id` 与 `conversation_id` 选择精确附件所有者。
///
/// # Returns
/// 返回值
/// The exact project path, the process workspace only when no owner is supplied, or one explicit unknown-owner error.
/// 返回精确项目路径；仅当未提供所有者时返回进程工作区；未知所有者则返回明确错误。
pub(crate) fn composer_attachment_workspace_path_from_snapshot(
    workspace_snapshot: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
) -> Result<String, String> {
    if project_id.is_some_and(|value| value.trim().is_empty())
        || conversation_id.is_some_and(|value| value.trim().is_empty())
    {
        return Err(String::from(
            "Attachment project and conversation ids cannot be blank.",
        ));
    }
    let conversation_project_id = if let Some(conversation_id) = conversation_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let session_id = parse_session_id(conversation_id)
            .ok_or_else(|| String::from("Attachment conversation id is invalid."))?;
        let session = workspace_session_summary_for_session(workspace_snapshot, session_id)
            .ok_or_else(|| String::from("Attachment conversation is not available."))?;
        Some(
            session_summary_project_id(session)
                .ok_or_else(|| String::from("Attachment conversation has no owning project."))?
                .trim(),
        )
    } else {
        None
    };
    let requested_project_id = project_id.map(str::trim).filter(|value| !value.is_empty());
    if let (Some(conversation_project_id), Some(requested_project_id)) =
        (conversation_project_id, requested_project_id)
        && conversation_project_id != requested_project_id
    {
        return Err(String::from(
            "Attachment project does not own the supplied conversation.",
        ));
    }
    // The conversation owner is authoritative when present; the explicit project selects only ownerless draft staging.
    // 存在会话所有者时以其为权威；显式项目仅用于选择尚无会话所有者的草稿暂存位置。
    let owner_project_id = conversation_project_id.or(requested_project_id);
    let Some(owner_project_id) = owner_project_id else {
        return Ok(default_composer_attachment_workspace_path());
    };
    workspace_task_project_for_project_id(workspace_snapshot, owner_project_id)
        .map(|project| project.path.clone())
        .ok_or_else(|| format!("Attachment project `{owner_project_id}` is not available."))
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
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Verifies supported draft-slot identities map to stable internal asset scopes.
    /// 验证受支持的草稿槽身份会映射为稳定的内部资产作用域。
    #[test]
    fn composer_attachment_draft_scope_accepts_exact_owner_kinds() {
        assert_eq!(
            composer_attachment_draft_scope(" conversation:session-a ")
                .expect("conversation draft owner should be valid"),
            "composer-draft:v1:conversation:session-a"
        );
        assert_eq!(
            composer_attachment_draft_scope("new:project-a")
                .expect("new-conversation draft owner should be valid"),
            "composer-draft:v1:new:project-a"
        );
    }

    /// Verifies malformed or ownerless draft slots fail closed before entering the asset index.
    /// 验证异常或缺少所有者的草稿槽会在进入资产索引前关闭失败。
    #[test]
    fn composer_attachment_draft_scope_rejects_invalid_owner_identity() {
        for invalid in [
            "",
            "conversation:",
            "new:   ",
            "workspace:project-a",
            "new:a\n",
        ] {
            assert!(
                composer_attachment_draft_scope(invalid).is_err(),
                "invalid draft slot should fail: {invalid:?}"
            );
        }
    }

    /// Verifies planning, reconciliation and submission use the same exact draft owner.
    /// 验证规划、协调及提交使用相同精确草稿所有者。
    #[test]
    fn composer_attachment_draft_scope_requires_selected_owner() {
        // The selected session is canonical and cannot acquire another conversation's lease.
        // 所选会话具有规范身份，不能取得另一会话的租约。
        let conversation_id = SessionId::new_v4().to_string();
        // The canonical selected slot is accepted even with boundary whitespace.
        // 即使有边界空白，也接受规范所选槽位。
        let slot = format!("conversation:{conversation_id}");
        assert_eq!(
            composer_attachment_draft_scope_for_owner(
                &format!(" {slot} "),
                Some("project-a"),
                Some(&conversation_id)
            )
            .unwrap(),
            format!("composer-draft:v1:{slot}")
        );
        for invalid in [
            format!("conversation:{}", SessionId::new_v4()),
            String::from("new:project-a"),
        ] {
            assert!(
                composer_attachment_draft_scope_for_owner(
                    &invalid,
                    Some("project-a"),
                    Some(&conversation_id)
                )
                .is_err()
            );
        }
        assert!(composer_attachment_draft_scope_for_owner(&slot, Some("project-a"), None).is_err());
        assert!(
            composer_attachment_draft_scope_for_owner(
                "conversation:invalid",
                None,
                Some("invalid")
            )
            .is_err()
        );
        assert!(
            composer_attachment_draft_scope_for_owner("new:project-a", Some(" project-a "), None)
                .is_ok()
        );
        assert!(
            composer_attachment_draft_scope_for_owner("new:project-b", Some("project-a"), None)
                .is_err()
        );
        assert!(composer_attachment_draft_scope_for_owner("new:workspace", None, None).is_ok());
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

    /// Verifies an explicit project cannot be paired with a conversation owned by another project.
    /// 验证明示项目不能与属于另一项目的会话配对。
    #[test]
    fn composer_attachment_workspace_path_rejects_project_conversation_mismatch() {
        // session_id identifies the conversation owned by project-a.
        // session_id 标识属于 project-a 的会话。
        let session_id = SessionId::new_v4();
        // conversation_id stores the WebUI conversation identity.
        // conversation_id 保存 WebUI 会话身份。
        let conversation_id = session_id.to_string();
        // state retains the authoritative project-a ownership relation.
        // state 保留权威的 project-a 所有关系。
        let state = service_state_with_workspace_snapshot(
            workspace_snapshot_for_attachment_staging(session_id, Some(String::from("project-a"))),
        );

        let error =
            composer_attachment_workspace_path(&state, Some("project-b"), Some(&conversation_id))
                .expect_err("mismatched project and conversation must fail closed");

        assert_eq!(
            error,
            "Attachment project does not own the supplied conversation."
        );
    }

    /// Verifies an existing conversation without an owning project fails closed instead of escaping to the process workspace.
    /// 验证缺少所属项目的既有会话会封闭失败，而不会逃逸到进程工作区。
    #[test]
    fn composer_attachment_workspace_path_rejects_blank_conversation_project() {
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

        assert!(composer_attachment_workspace_path(&state, None, Some(&conversation_id)).is_err());
    }
}
