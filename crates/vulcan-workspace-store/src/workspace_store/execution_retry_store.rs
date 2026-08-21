//! Atomic sidecar storage for exact execution-retry snapshots.
//! 精确执行重试快照的原子 sidecar 存储。

use super::segmented_session_store::write_file_bytes_atomic;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use vulcan_workbench_model::{
    EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION, ExecutionRetryPendingAction,
    ExecutionRetryPreparationRouteSelection, ExecutionRetrySnapshot, ExecutionRetrySnapshotRef,
    ExecutionRetryToolProgress, ExecutionRetryToolState,
};

/// Directory name containing retry sidecars below one segmented session.
/// 单个分段会话下保存重试 sidecar 的目录名。
const EXECUTION_RETRY_DIRECTORY: &str = "execution_retry";

/// Builds the content-addressed file suffix from the only accepted retry Schema generation.
/// 根据唯一接受的重试 Schema 代际构建内容寻址文件后缀。
fn execution_retry_snapshot_file_suffix() -> String {
    format!(".v{EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION}.json")
}

/// Owns strict read, write, validation, and deletion of retry sidecars for one session directory.
/// 负责单个会话目录中重试 sidecar 的严格读写、校验与删除。
#[derive(Debug, Clone)]
pub struct ExecutionRetrySnapshotStore {
    /// Exact segmented main-session directory.
    /// 精确的分段主会话目录。
    session_directory: PathBuf,
}

impl ExecutionRetrySnapshotStore {
    /// Creates a retry snapshot store rooted at one exact segmented session directory.
    /// 创建一个以精确分段会话目录为根的重试快照存储。
    ///
    /// # Parameters
    /// 参数
    /// `session_directory` is the validated main-session directory selected by workspace storage.
    /// `session_directory` 是工作区存储选定的已校验主会话目录。
    ///
    /// # Returns
    /// 返回值
    /// A path-bound store that performs no I/O until an operation is invoked.
    /// 返回路径绑定的存储；在调用具体操作前不执行 I/O。
    pub fn new(session_directory: impl Into<PathBuf>) -> Self {
        Self {
            session_directory: session_directory.into(),
        }
    }

    /// Atomically publishes one complete retry snapshot and returns its lightweight reference.
    /// 原子发布一份完整重试快照并返回其轻量引用。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` is the complete exact execution payload serialized before any final path changes.
    /// `snapshot` 是在最终路径变化前完整序列化的精确执行载荷。
    ///
    /// # Returns
    /// 返回值
    /// A digest-bound reference suitable for synchronous publication in session runtime state.
    /// 返回适合同步发布到会话运行态的摘要绑定引用。
    pub fn write(
        &self,
        snapshot: &ExecutionRetrySnapshot,
    ) -> Result<ExecutionRetrySnapshotRef, String> {
        validate_snapshot_identity(snapshot)?;
        // encoded is complete before the atomic writer is allowed to mutate the filesystem.
        // encoded 在允许原子写入器修改文件系统前已经完整生成。
        let encoded = serde_json::to_vec(snapshot)
            .map_err(|error| format!("Failed to serialize execution retry snapshot: {error}"))?;
        // contentSha256 binds the lightweight main-state reference to the exact sidecar bytes.
        // contentSha256 将轻量主状态引用绑定到精确 sidecar 字节。
        let content_sha256 = sha256_hex(&encoded);
        let content_bytes = u64::try_from(encoded.len()).map_err(|_| {
            String::from("Execution retry snapshot size exceeds the supported u64 range.")
        })?;
        // replaySafety is copied into the lightweight reference so UI admission never guesses from ledger state alone.
        // replaySafety 写入轻量引用，使 UI 准入不会只根据账本状态猜测。
        let (retryable, requires_manual_confirmation, blocked_reason) =
            snapshot_safety_summary(snapshot);
        // preparationRouteSelection mirrors the exact pre-resolution selection only for preparation replay.
        // preparationRouteSelection 仅为准备阶段重放镜像精确的解析前选择。
        let preparation_route_selection = preparation_route_selection(&snapshot.pending_action);
        // canonicalDispatchSha256 mirrors the exact provider-visible state accepted by claim validation.
        // canonicalDispatchSha256 镜像领取校验接受的精确 Provider 可见状态。
        let canonical_dispatch_sha256 = snapshot_canonical_dispatch_sha256(snapshot)?;
        let path = self.snapshot_path(snapshot.snapshot_id.as_str(), content_sha256.as_str())?;
        write_file_bytes_atomic(&path, &encoded, "execution retry snapshot")?;
        Ok(ExecutionRetrySnapshotRef {
            schema_version: EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: snapshot.snapshot_id.clone(),
            generation: snapshot.generation,
            state: snapshot.state,
            content_sha256,
            content_bytes,
            logical_execution_id: snapshot.logical_execution_id.clone(),
            pending_action_label: pending_action_label(&snapshot.pending_action).to_string(),
            provider_route: snapshot.provider_route.clone(),
            preparation_route_selection,
            context_generation_id: snapshot.context_generation_id.clone(),
            canonical_dispatch_sha256,
            retryable,
            requires_manual_confirmation,
            blocked_reason,
            claimed_attempt_id: snapshot.claimed_attempt_id.clone(),
            error_summary: snapshot.error.as_ref().map(|error| error.message.clone()),
            updated_at: snapshot.updated_at.clone(),
        })
    }

    /// Reads and strictly validates one exact digest-bound retry snapshot.
    /// 读取并严格校验一份精确摘要绑定的重试快照。
    ///
    /// # Parameters
    /// 参数
    /// `reference` is the lightweight state reference loaded from the owning session.
    /// `reference` 是从所属会话加载的轻量状态引用。
    ///
    /// # Returns
    /// 返回值
    /// The exact decoded snapshot only when schema, identity, size, digest, generation, and state all match.
    /// 仅当 Schema、身份、大小、摘要、代际与状态全部一致时返回精确解码快照。
    pub fn read(
        &self,
        reference: &ExecutionRetrySnapshotRef,
    ) -> Result<ExecutionRetrySnapshot, String> {
        if reference.schema_version != EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported execution retry reference schema version `{}`; expected `{}`.",
                reference.schema_version, EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION
            ));
        }
        let path = self.snapshot_path(
            reference.snapshot_id.as_str(),
            reference.content_sha256.as_str(),
        )?;
        let encoded = fs::read(&path).map_err(|error| {
            format!(
                "Failed to read execution retry snapshot `{}`: {error}",
                path.display()
            )
        })?;
        let actual_bytes = u64::try_from(encoded.len()).map_err(|_| {
            String::from("Execution retry snapshot size exceeds the supported u64 range.")
        })?;
        if actual_bytes != reference.content_bytes {
            return Err(format!(
                "Execution retry snapshot byte length mismatch: expected `{}`, actual `{actual_bytes}`.",
                reference.content_bytes
            ));
        }
        let actual_sha256 = sha256_hex(&encoded);
        if actual_sha256 != reference.content_sha256 {
            return Err(String::from(
                "Execution retry snapshot SHA-256 digest mismatch.",
            ));
        }
        let snapshot = serde_json::from_slice::<ExecutionRetrySnapshot>(&encoded)
            .map_err(|error| format!("Failed to decode execution retry snapshot: {error}"))?;
        validate_snapshot_identity(&snapshot)?;
        let (retryable, requires_manual_confirmation, blocked_reason) =
            snapshot_safety_summary(&snapshot);
        let pending_action_label = pending_action_label(&snapshot.pending_action);
        let preparation_route_selection = preparation_route_selection(&snapshot.pending_action);
        let canonical_dispatch_sha256 = snapshot_canonical_dispatch_sha256(&snapshot)?;
        let error_summary = snapshot.error.as_ref().map(|error| error.message.clone());
        if snapshot.snapshot_id != reference.snapshot_id
            || snapshot.generation != reference.generation
            || snapshot.state != reference.state
            || snapshot.logical_execution_id != reference.logical_execution_id
            || snapshot.claimed_attempt_id != reference.claimed_attempt_id
            || pending_action_label != reference.pending_action_label
            || snapshot.provider_route != reference.provider_route
            || preparation_route_selection != reference.preparation_route_selection
            || snapshot.context_generation_id != reference.context_generation_id
            || canonical_dispatch_sha256 != reference.canonical_dispatch_sha256
            || retryable != reference.retryable
            || requires_manual_confirmation != reference.requires_manual_confirmation
            || blocked_reason != reference.blocked_reason
            || error_summary != reference.error_summary
        {
            return Err(String::from(
                "Execution retry snapshot identity, safety summary, or lifecycle state does not match its reference.",
            ));
        }
        Ok(snapshot)
    }

    /// Deletes every immutable body belonging to one snapshot identity after successful consumption.
    /// 在成功消费后删除属于一个快照身份的全部不可变正文。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot_id` is the validated content-independent identity selected for deletion.
    /// `snapshot_id` 是为删除选定的已校验内容无关身份。
    ///
    /// # Returns
    /// 返回值
    /// Success when all matching bodies are absent or removed, otherwise an exact filesystem error.
    /// 所有匹配正文均不存在或已删除时返回成功，否则返回精确文件系统错误。
    pub fn delete(&self, snapshot_id: &str) -> Result<(), String> {
        validate_snapshot_id(snapshot_id)?;
        // retryDirectory is the only directory scanned for immutable bodies of this exact snapshot.
        // retryDirectory 是扫描该精确快照不可变正文的唯一目录。
        let retry_directory = self.session_directory.join(EXECUTION_RETRY_DIRECTORY);
        // entries treats an absent directory as an already-completed cleanup.
        // entries 将目录不存在视为清理已经完成。
        let entries = match fs::read_dir(&retry_directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "Failed to inspect execution retry directory `{}`: {error}",
                    retry_directory.display()
                ));
            }
        };
        // filePrefix and fileSuffix restrict cleanup to current-Schema bodies owned by this exact UUID.
        // filePrefix 与 fileSuffix 将清理限制为该精确 UUID 所属的当前 Schema 正文。
        let file_prefix = format!("{snapshot_id}.");
        let file_suffix = execution_retry_snapshot_file_suffix();
        for entry in entries {
            // entry is one direct child of the validated retry directory.
            // entry 是已校验重试目录的一个直接子项。
            let entry = entry.map_err(|error| {
                format!(
                    "Failed to enumerate execution retry directory `{}`: {error}",
                    retry_directory.display()
                )
            })?;
            // fileName remains opaque unless it matches the exact content-addressed body pattern.
            // fileName 在匹配精确内容寻址正文模式前保持不透明。
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };
            if !file_name.starts_with(file_prefix.as_str())
                || !file_name.ends_with(file_suffix.as_str())
            {
                continue;
            }
            // path comes directly from this directory entry and never from string concatenation.
            // path 直接来自当前目录项，不通过字符串拼接生成。
            let path = entry.path();
            fs::remove_file(&path).map_err(|error| {
                format!(
                    "Failed to delete execution retry snapshot `{}`: {error}",
                    path.display()
                )
            })?;
        }
        Ok(())
    }

    /// Resolves one validated content-addressed body below the session-owned retry directory.
    /// 在会话所属重试目录下解析一个已校验的内容寻址正文。
    fn snapshot_path(&self, snapshot_id: &str, content_sha256: &str) -> Result<PathBuf, String> {
        validate_snapshot_id(snapshot_id)?;
        validate_sha256_hex(content_sha256)?;
        let file_suffix = execution_retry_snapshot_file_suffix();
        Ok(self
            .session_directory
            .join(EXECUTION_RETRY_DIRECTORY)
            .join(format!("{snapshot_id}.{content_sha256}{file_suffix}")))
    }
}

/// Validates strict snapshot schema and content-independent identity fields.
/// 校验严格快照 Schema 与内容无关身份字段。
fn validate_snapshot_identity(snapshot: &ExecutionRetrySnapshot) -> Result<(), String> {
    if snapshot.schema_version != EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported execution retry snapshot schema version `{}`; expected `{}`.",
            snapshot.schema_version, EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION
        ));
    }
    validate_snapshot_id(snapshot.snapshot_id.as_str())?;
    if snapshot.session_id.trim().is_empty()
        || snapshot.logical_execution_id.trim().is_empty()
        || snapshot.physical_attempt_id.trim().is_empty()
        || snapshot.canonical_digest.trim().is_empty()
    {
        return Err(String::from(
            "Execution retry snapshot is missing a required stable identity or digest.",
        ));
    }
    if snapshot
        .user_followup
        .as_ref()
        .is_some_and(|followup| !followup.is_structurally_valid())
    {
        return Err(String::from(
            "Execution retry snapshot contains an invalid user follow-up boundary.",
        ));
    }
    Ok(())
}

/// Derives the safety summary copied into the lightweight reference.
/// 推导写入轻量引用的安全摘要。
fn snapshot_safety_summary(snapshot: &ExecutionRetrySnapshot) -> (bool, bool, Option<String>) {
    let pending_action_supported = matches!(
        &snapshot.pending_action,
        ExecutionRetryPendingAction::ResumePreparation { .. }
            | ExecutionRetryPendingAction::DispatchProviderRequest { .. }
            | ExecutionRetryPendingAction::ExecuteToolBatch { .. }
            | ExecutionRetryPendingAction::PrepareNextProviderRequest
            | ExecutionRetryPendingAction::FinalizeAssistant
    );
    let action_boundary_blocked_reason = pending_action_boundary_blocked_reason(snapshot);
    let error_blocked_reason = snapshot.error.as_ref().and_then(|error| {
        if error.side_effect_observed
            && matches!(
                snapshot.pending_action,
                ExecutionRetryPendingAction::DispatchProviderRequest { .. }
            )
        {
            return Some(String::from("tool_side_effect_in_flight"));
        }
        None
    });
    let blocked_reason = if !pending_action_supported {
        Some(String::from("pending_action_executor_unavailable"))
    } else {
        error_blocked_reason.or(action_boundary_blocked_reason)
    };
    let retryable = blocked_reason.is_none();
    let requires_manual_confirmation = blocked_reason.is_some();
    (retryable, requires_manual_confirmation, blocked_reason)
}

/// Returns the exact pre-resolution route selection mirrored by one preparation action.
/// 返回一个准备动作镜像的精确解析前路由选择。
///
/// # Parameters
/// 参数
/// `action` is the complete frozen pending action stored in the sidecar.
/// `action` 是 sidecar 中保存的完整冻结待执行动作。
///
/// # Returns
/// 返回值
/// A present selection for `ResumePreparation`, otherwise `None`.
/// 对 `ResumePreparation` 返回存在的选择，其他动作返回 `None`。
fn preparation_route_selection(
    action: &ExecutionRetryPendingAction,
) -> Option<ExecutionRetryPreparationRouteSelection> {
    match action {
        ExecutionRetryPendingAction::ResumePreparation { input } => {
            Some(ExecutionRetryPreparationRouteSelection {
                active_provider_id: input.active_provider_id.clone(),
                active_connection_id: input.active_connection_id.clone(),
                active_model_id: input.active_model_id.clone(),
            })
        }
        _ => None,
    }
}

/// Returns a stable structural blocker when the frozen action lacks its required route boundary.
/// 当冻结动作缺少所需路由边界时返回稳定的结构性阻断原因。
///
/// # Parameters
/// 参数
/// `snapshot` is the complete sidecar whose action and controller boundary are inspected together.
/// `snapshot` 是同时检查动作与控制器边界的完整 sidecar。
///
/// # Returns
/// 返回值
/// A stable WebUI-localized reason, or `None` when the action boundary is complete.
/// 返回稳定的 WebUI 本地化原因；动作边界完整时返回 `None`。
fn pending_action_boundary_blocked_reason(snapshot: &ExecutionRetrySnapshot) -> Option<String> {
    match &snapshot.pending_action {
        ExecutionRetryPendingAction::DispatchProviderRequest { .. }
            if snapshot.provider_route.is_none() =>
        {
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING",
            ))
        }
        ExecutionRetryPendingAction::PrepareNextProviderRequest
            if snapshot.provider_route.is_none() =>
        {
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING",
            ))
        }
        ExecutionRetryPendingAction::ExecuteToolBatch { .. }
            if snapshot.provider_route.is_none() =>
        {
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING",
            ))
        }
        ExecutionRetryPendingAction::FinalizeAssistant if snapshot.provider_route.is_none() => {
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING",
            ))
        }
        ExecutionRetryPendingAction::ExecuteToolBatch { tools } => {
            execute_tool_batch_boundary_blocked_reason(snapshot, tools)
        }
        ExecutionRetryPendingAction::PrepareNextProviderRequest
            if !vulcan_core::controller::conversation_snapshot_has_seeded_provider_input(
                &snapshot.controller_snapshot,
            ) =>
        {
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_NEXT_PROVIDER_BOUNDARY_MISSING",
            ))
        }
        ExecutionRetryPendingAction::FinalizeAssistant
            if !matches!(
                snapshot
                    .controller_snapshot
                    .messages
                    .last()
                    .map(|message| &message.payload),
                Some(vulcan_core::types::ConversationPayload::Assistant { tool_calls, .. })
                    if tool_calls.is_empty()
            ) =>
        {
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_FINAL_ASSISTANT_BOUNDARY_MISSING",
            ))
        }
        _ => None,
    }
}

/// Validates an exact retained assistant tool batch and every per-tool recovery slot.
/// 校验精确保留的助手工具批次及每个逐工具恢复槽位。
fn execute_tool_batch_boundary_blocked_reason(
    snapshot: &ExecutionRetrySnapshot,
    tools: &[ExecutionRetryToolProgress],
) -> Option<String> {
    let Some(vulcan_core::types::ConversationPayload::Assistant {
        tool_calls: assistant_calls,
        ..
    }) = snapshot
        .controller_snapshot
        .messages
        .last()
        .map(|message| &message.payload)
    else {
        return Some(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BOUNDARY_MISSING",
        ));
    };
    if assistant_calls.is_empty() || assistant_calls.len() != tools.len() {
        return Some(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BOUNDARY_MISSING",
        ));
    }

    let mut observed_tool_call_ids = std::collections::HashSet::new();
    for (assistant_call, progress) in assistant_calls.iter().zip(tools) {
        // expectedToolCallId uses the same canonical resolver as normal execution and crash-gap reconstruction.
        // expectedToolCallId 使用与正常执行及崩溃窗口重建相同的规范解析器。
        let expected_tool_call_id = vulcan_core::controller::resolved_tool_call_id(assistant_call);
        // An unresolved binding is valid only for a proven-unstarted slot. The mandatory
        // BatchPrepared durability barrier must replace it before any Started transition.
        // 未解析绑定仅对已证明未启动的槽位有效。强制 BatchPrepared 可靠屏障必须在任何
        // Started 迁移前填充该绑定。
        let binding_fingerprint_valid =
            match (&progress.state, progress.binding_fingerprint.as_deref()) {
                (ExecutionRetryToolState::NotStarted, None) => true,
                (_, Some(binding_fingerprint)) => {
                    binding_fingerprint.len() == 64
                        && binding_fingerprint
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                }
                (_, None) => false,
            };
        if !observed_tool_call_ids.insert(progress.request.tool_call_id.as_str())
            || progress.request.tool_call_id != expected_tool_call_id
            || progress.request.tool_name != assistant_call.tool_name
            || progress.request.arguments != assistant_call.arguments
            || !binding_fingerprint_valid
        {
            return Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BINDING_MISMATCH",
            ));
        }

        let result_matches_request = progress.result.as_ref().is_none_or(|result| {
            result.tool_call_id == progress.request.tool_call_id
                && result.tool_name == progress.request.tool_name
        });
        let state_matches_result = match (&progress.state, &progress.result) {
            (
                ExecutionRetryToolState::NotStarted
                | ExecutionRetryToolState::Running
                | ExecutionRetryToolState::OutcomeUnknown,
                None,
            ) => true,
            (ExecutionRetryToolState::Completed, Some(result)) => !result.is_error,
            (ExecutionRetryToolState::Failed, Some(result)) => result.is_error,
            _ => false,
        };
        if !result_matches_request || !state_matches_result {
            return Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_RESULT_MISMATCH",
            ));
        }
    }
    None
}

/// Computes the claim-equivalent digest of provider-visible messages and continuation state.
/// 计算 Provider 可见消息与 continuation 状态的领取等价摘要。
///
/// # Parameters
/// 参数
/// `snapshot` owns the frozen canonical controller state.
/// `snapshot` 持有冻结的规范控制器状态。
///
/// # Returns
/// 返回值
/// A lowercase SHA-256 digest, or an exact serialization error.
/// 返回小写 SHA-256 摘要，或精确的序列化错误。
fn snapshot_canonical_dispatch_sha256(snapshot: &ExecutionRetrySnapshot) -> Result<String, String> {
    let encoded = serde_json::to_vec(&(
        &snapshot.controller_snapshot.messages,
        &snapshot.controller_snapshot.continuation,
    ))
    .map_err(|error| format!("Failed to serialize execution retry dispatch state: {error}"))?;
    Ok(sha256_hex(&encoded))
}

/// Rejects path separators and non-UUID snapshot identities before filesystem resolution.
/// 在文件系统解析前拒绝路径分隔符与非 UUID 快照身份。
fn validate_snapshot_id(snapshot_id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(snapshot_id)
        .map(|_| ())
        .map_err(|_| String::from("Execution retry snapshot id must be an exact UUID."))
}

/// Rejects malformed or path-like digest text before resolving a content-addressed body.
/// 在解析内容寻址正文前拒绝格式错误或路径式摘要文本。
fn validate_sha256_hex(content_sha256: &str) -> Result<(), String> {
    if content_sha256.len() == 64
        && content_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(());
    }
    Err(String::from(
        "Execution retry snapshot digest must be exactly 64 lowercase hexadecimal characters.",
    ))
}

/// Returns the stable lightweight label for one exact pending action.
/// 返回一个精确待执行动作的稳定轻量标签。
fn pending_action_label(action: &ExecutionRetryPendingAction) -> &'static str {
    match action {
        ExecutionRetryPendingAction::ResumePreparation { .. } => "resume_preparation",
        ExecutionRetryPendingAction::DispatchProviderRequest { .. } => "dispatch_provider_request",
        ExecutionRetryPendingAction::ExecuteToolBatch { .. } => "execute_tool_batch",
        ExecutionRetryPendingAction::PrepareNextProviderRequest => "prepare_next_provider_request",
        ExecutionRetryPendingAction::FinalizeAssistant => "finalize_assistant",
    }
}

/// Computes a lowercase SHA-256 digest for exact serialized bytes.
/// 为精确序列化字节计算小写 SHA-256 摘要。
fn sha256_hex(encoded: &[u8]) -> String {
    let digest = Sha256::digest(encoded);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    //! Tests for strict atomic execution-retry sidecar storage.
    //! 严格原子执行重试 sidecar 存储测试。

    use super::*;
    use vulcan_ai_contract::{AiErrorCategory, AiExecutionError, DispatchOutcome};
    use vulcan_workbench_model::{ExecutionRetrySnapshotState, SessionProviderRouteLeaseIdentity};

    /// Builds one complete minimal retry snapshot for storage contract tests.
    /// 为存储契约测试构建一份完整的最小重试快照。
    fn test_snapshot(snapshot_id: String) -> ExecutionRetrySnapshot {
        // controllerSnapshot ends at the exact completed tool boundary required by the pending action.
        // controllerSnapshot 结束于待执行动作要求的精确已完成工具边界。
        let controller_snapshot = vulcan_core::types::ConversationSnapshot {
            messages: vec![vulcan_core::types::ConversationMessage {
                id: String::from("tool-result-1"),
                payload: vulcan_core::types::ConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("tool-call-1")),
                    is_error: false,
                    text: String::from("completed tool result"),
                },
            }],
            ..vulcan_core::types::ConversationSnapshot::default()
        };
        ExecutionRetrySnapshot {
            schema_version: EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id,
            session_id: uuid::Uuid::from_u128(42).to_string(),
            generation: 3,
            state: ExecutionRetrySnapshotState::Frozen,
            logical_execution_id: String::from("logical-execution-1"),
            logical_turn_id: Some(String::from("turn-1")),
            next_provider_attempt_index: 0,
            physical_attempt_id: String::from("attempt-1"),
            claimed_attempt_id: None,
            provider_route: Some(SessionProviderRouteLeaseIdentity {
                schema_version: 1,
                provider_id: String::from("provider-a"),
                connection_id: String::from("connection-a"),
                model_id: String::from("model-a"),
                protocol_profile_id: String::from("openai-responses.v1"),
                manifest_revision: 1,
                manifest_generation: 1,
                catalog_revision: String::from("catalog-a"),
                route_revision: String::from("route-a"),
            }),
            provider_service_entry_id: Some(String::from("entry-a")),
            credential_profile_id: Some(String::from("credential-a")),
            controller_config: Some(vulcan_core::types::ControllerConfig::default()),
            controller_snapshot,
            pending_action: ExecutionRetryPendingAction::PrepareNextProviderRequest,
            user_followup: None,
            error: None,
            canonical_digest: String::from("canonical-digest-a"),
            context_generation_id: Some(String::from("context-generation-a")),
            created_at: String::from("1000"),
            updated_at: String::from("1000"),
        }
    }

    /// Builds one valid interrupted assistant tool batch with an exact not-started recovery slot.
    /// 构建一个有效的中断助手工具批次，并包含精确的未启动恢复槽位。
    fn test_execute_tool_batch_snapshot(snapshot_id: String) -> ExecutionRetrySnapshot {
        // snapshot reuses the complete route and identity fixture before replacing its canonical tail.
        // snapshot 在替换规范尾部前复用完整的路由与身份测试夹具。
        let mut snapshot = test_snapshot(snapshot_id);
        // request is the exact model-issued invocation retained by the recovery slot.
        // request 是恢复槽位保留的模型原始精确调用。
        let request = vulcan_core::types::ToolInvocationRequest {
            tool_call_id: String::from("tool-item-1"),
            tool_name: String::from("read_file"),
            arguments: serde_json::json!({"path": "README.md"}),
        };
        snapshot.controller_snapshot.messages = vec![vulcan_core::types::ConversationMessage {
            id: String::from("assistant-tool-batch-1"),
            payload: vulcan_core::types::ConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: Some(String::from("provider-message-1")),
                provider_message_state: None,
                usage: None,
                tool_calls: vec![vulcan_core::types::ConversationToolCall {
                    id: String::from("tool-item-1"),
                    // The fixture proves storage resolves a blank Provider id exactly like normal Core execution.
                    // 该测试夹具证明存储层与 Core 正常执行使用同一空白 Provider 标识解析规则。
                    tool_call_id: Some(String::from("   ")),
                    tool_name: request.tool_name.clone(),
                    arguments: request.arguments.clone(),
                }],
            },
        }];
        snapshot.pending_action = ExecutionRetryPendingAction::ExecuteToolBatch {
            tools: vec![ExecutionRetryToolProgress {
                request,
                result: None,
                binding_fingerprint: Some("a".repeat(64)),
                state: ExecutionRetryToolState::NotStarted,
            }],
        };
        snapshot
    }

    /// Verifies atomic publication round-trips through a digest-bound lightweight reference.
    /// 验证原子发布能够通过摘要绑定轻量引用完整往返。
    #[test]
    fn retry_snapshot_round_trips_through_digest_reference() {
        // tempRoot owns the exact isolated session directory used by this test.
        // tempRoot 持有当前测试使用的精确隔离会话目录。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store binds every sidecar operation to the isolated session directory.
        // store 将每个 sidecar 操作绑定到隔离会话目录。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());

        let reference = store.write(&snapshot).expect("snapshot should persist");
        let restored = store.read(&reference).expect("snapshot should restore");

        // sidecarPath proves the storage filename generation cannot drift from the accepted Schema version.
        // sidecarPath 证明存储文件名代际不会偏离当前接受的 Schema 版本。
        let sidecar_path = store
            .snapshot_path(
                snapshot.snapshot_id.as_str(),
                reference.content_sha256.as_str(),
            )
            .expect("snapshot path should remain valid");
        let expected_suffix = format!(".v{EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION}.json");
        assert!(
            sidecar_path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|file_name| file_name.ends_with(expected_suffix.as_str()))
        );
        assert!(sidecar_path.is_file());

        assert_eq!(restored.snapshot_id, snapshot.snapshot_id);
        assert_eq!(restored.generation, snapshot.generation);
        assert_eq!(restored.canonical_digest, snapshot.canonical_digest);
        assert!(reference.retryable);
        assert!(!reference.requires_manual_confirmation);
        assert_eq!(reference.blocked_reason, None);
        assert_eq!(reference.provider_route, snapshot.provider_route);
        assert_eq!(reference.preparation_route_selection, None);
        assert_eq!(
            reference.context_generation_id,
            snapshot.context_generation_id
        );
        assert_eq!(reference.canonical_dispatch_sha256.len(), 64);
        assert_eq!(reference.content_sha256.len(), 64);
        assert!(reference.content_bytes > 0);
    }

    /// Verifies every Core-authorized Provider seed produces a replay-safe PrepareNext reference.
    /// 验证 Core 授权的每种 Provider 播种输入都会生成可安全重放的 PrepareNext 引用。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails if storage projection narrows the shared Core boundary contract.
    /// 此测试无返回值；若存储投影缩窄共享 Core 边界契约，则测试失败。
    #[test]
    fn prepare_next_reference_accepts_every_core_provider_seed() {
        // providerSeeds enumerate the complete shared Core contract used by runtime interventions.
        // providerSeeds 枚举运行时干预使用的完整共享 Core 契约。
        let provider_seeds = vec![
            vulcan_core::types::ConversationPayload::UserText {
                text: String::from("continue with the user supplement"),
                attachments: Vec::new(),
            },
            vulcan_core::types::ConversationPayload::AgentMessage {
                message_type: String::from("message"),
                task_name: String::from("/root"),
                sender: String::from("/root/reviewer"),
                recipient: String::from("/root"),
                payload: String::from("continue after mailbox delivery"),
                trigger_turn: true,
                metadata: None,
            },
            vulcan_core::types::ConversationPayload::ToolResult {
                tool_call_id: Some(String::from("tool-call-1")),
                is_error: false,
                text: String::from("completed tool result"),
            },
        ];

        for (seed_index, payload) in provider_seeds.into_iter().enumerate() {
            // tempRoot isolates the immutable sidecar body for this exact seed kind.
            // tempRoot 隔离当前精确播种输入类型的不可变 sidecar 正文。
            let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
            // store derives the lightweight replay proof from the complete body.
            // store 从完整正文派生轻量重放证明。
            let store = ExecutionRetrySnapshotStore::new(temp_root.path());
            // snapshot replaces only the canonical tail while preserving the valid route and action fixture.
            // snapshot 仅替换规范尾部，同时保留有效路由与动作夹具。
            let mut snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
            snapshot.controller_snapshot.messages = vec![vulcan_core::types::ConversationMessage {
                id: format!("provider-seed-{seed_index}"),
                payload,
            }];

            let reference = store
                .write(&snapshot)
                .expect("authorized Provider seed should persist");

            assert!(reference.retryable);
            assert!(!reference.requires_manual_confirmation);
            assert_eq!(reference.blocked_reason, None);
            assert!(reference.replay_safety_proven());
        }
    }

    /// Verifies a successful lifecycle cleanup removes the sidecar instead of retaining a consumed snapshot.
    /// 验证成功生命周期清理会删除 sidecar，而不是保留已消费快照。
    #[test]
    fn retry_snapshot_delete_removes_persisted_sidecar() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        let reference = store.write(&snapshot).expect("snapshot should persist");
        let path = store
            .snapshot_path(
                reference.snapshot_id.as_str(),
                reference.content_sha256.as_str(),
            )
            .expect("snapshot path should resolve");

        store
            .delete(reference.snapshot_id.as_str())
            .expect("successful cleanup should delete the sidecar");

        assert!(!path.exists());
    }

    /// Verifies changed sidecar bytes fail closed instead of being decoded as a retry candidate.
    /// 验证被修改的 sidecar 字节会失败关闭，而不是被解码为重试候选。
    #[test]
    fn retry_snapshot_rejects_digest_mismatch() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        let reference = store.write(&snapshot).expect("snapshot should persist");
        // path is resolved by the same validated UUID rule used by normal reads.
        // path 使用与正常读取相同的已校验 UUID 规则解析。
        let path = store
            .snapshot_path(
                reference.snapshot_id.as_str(),
                reference.content_sha256.as_str(),
            )
            .expect("snapshot path should resolve");
        let mut bytes = fs::read(&path).expect("snapshot bytes should exist");
        bytes.push(b' ');
        fs::write(&path, bytes).expect("test corruption should be written");

        let error = store
            .read(&reference)
            .expect_err("changed bytes must fail closed");

        assert!(error.contains("byte length mismatch") || error.contains("digest mismatch"));
    }

    /// Verifies publishing a new body never destroys the body named by the prior durable reference.
    /// 验证发布新正文绝不会破坏旧持久引用所命名的正文。
    #[test]
    fn retry_snapshot_replacement_preserves_prior_digest_body() {
        // tempRoot owns both immutable generations under one exact snapshot identity.
        // tempRoot 在同一精确快照身份下持有两个不可变代际。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store publishes each serialized body at its own digest-addressed path.
        // store 将每份序列化正文发布到各自的摘要寻址路径。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // snapshot changes lifecycle and generation while preserving the logical snapshot identity.
        // snapshot 在保留逻辑快照身份的同时改变生命周期与代际。
        let mut snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        // priorReference models the reference already durable in state.json.
        // priorReference 模拟已经持久化到 state.json 的引用。
        let prior_reference = store.write(&snapshot).expect("prior body should persist");
        snapshot.generation += 1;
        snapshot.state = ExecutionRetrySnapshotState::Claimed;
        snapshot.claimed_attempt_id = Some(String::from("attempt-2"));
        snapshot.updated_at = String::from("2000");
        // replacementReference models the new in-memory reference before state.json commits.
        // replacementReference 模拟 state.json 提交前的新内存引用。
        let replacement_reference = store.write(&snapshot).expect("replacement should persist");

        assert_ne!(
            prior_reference.content_sha256,
            replacement_reference.content_sha256
        );
        assert_eq!(
            store
                .read(&prior_reference)
                .expect("prior durable reference must remain readable")
                .generation,
            3
        );
        assert_eq!(
            store
                .read(&replacement_reference)
                .expect("replacement reference must be readable")
                .generation,
            4
        );
        store
            .delete(snapshot.snapshot_id.as_str())
            .expect("consumption should delete every immutable body");
        assert!(store.read(&prior_reference).is_err());
        assert!(store.read(&replacement_reference).is_err());
    }

    /// Verifies a lightweight reference cannot remain readable after its exact sidecar disappears.
    /// 验证精确 sidecar 消失后，轻量引用不能继续被读取。
    #[test]
    fn retry_snapshot_rejects_missing_sidecar_body() {
        // tempRoot owns the isolated segmented-session directory for the missing-body case.
        // tempRoot 持有缺失正文场景使用的隔离分段会话目录。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store publishes and reads the same digest-bound sidecar identity.
        // store 发布并读取同一个摘要绑定 sidecar 身份。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // snapshot is the complete body removed after its reference is published.
        // snapshot 是引用发布后被移除的完整正文。
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        // reference remains in main state after the simulated external body loss.
        // reference 在模拟外部正文丢失后仍保留在主状态中。
        let reference = store.write(&snapshot).expect("snapshot should persist");
        store
            .delete(reference.snapshot_id.as_str())
            .expect("test setup should remove the exact sidecar");

        let error = store
            .read(&reference)
            .expect_err("a missing sidecar body must fail closed");

        assert!(error.contains("Failed to read execution retry snapshot"));
    }

    /// Verifies a lightweight safety summary cannot diverge from the sidecar body.
    /// 验证轻量安全摘要不能与 sidecar 正文发生分歧。
    #[test]
    fn retry_snapshot_rejects_reference_safety_drift() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        let mut reference = store.write(&snapshot).expect("snapshot should persist");
        reference.pending_action_label = String::from("finalize_assistant");

        let error = store
            .read(&reference)
            .expect_err("reference safety drift must fail closed");

        assert!(error.contains("safety summary"));
    }

    /// Verifies an omitted lightweight route cannot bypass the exact sidecar route identity.
    /// 验证轻量引用缺失路由时不能绕过 sidecar 的精确路由身份。
    #[test]
    fn retry_snapshot_rejects_reference_route_omission() {
        // tempRoot isolates the exact digest-bound body used by this route-drift case.
        // tempRoot 隔离当前路由漂移场景使用的精确摘要绑定正文。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store derives the authoritative lightweight route from the persisted body.
        // store 从持久化正文推导权威轻量路由。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // snapshot contains the frozen provider route that Continue must preserve.
        // snapshot 包含 Continue 必须保留的冻结供应商路由。
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        // reference simulates a truncated main-state pointer with its route removed.
        // reference 模拟路由字段被移除的截断主状态指针。
        let mut reference = store.write(&snapshot).expect("snapshot should persist");
        reference.provider_route = None;

        let error = store
            .read(&reference)
            .expect_err("route omission must fail closed");

        assert!(error.contains("safety summary"));
    }

    /// Verifies side-effect uncertainty is copied into the lightweight blocked reference.
    /// 验证副作用结果未知会被复制为轻量阻断引用。
    #[test]
    fn retry_snapshot_marks_non_retryable_side_effect_blocked() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let mut snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        // request represents the still-dispatched Provider request whose side effect lacks any per-tool boundary.
        // request 表示已分派但其副作用缺少逐工具边界的 Provider 请求。
        let request =
            serde_json::from_value::<vulcan_ai_contract::AiExecutionRequest>(serde_json::json!({
                "operation": "conversation_respond",
                "model_id": "model-a"
            }))
            .expect("minimal provider request should decode through defaulted fields");
        snapshot.pending_action = ExecutionRetryPendingAction::DispatchProviderRequest {
            request: Box::new(request),
        };
        snapshot.error = Some(AiExecutionError {
            category: AiErrorCategory::Unclassified,
            subcategory: Some(String::from("provider_specific_tool_state")),
            stage: String::from("tool_execution"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: String::from("tool outcome is unknown"),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: true,
            side_effect_observed: true,
            terminal: false,
            retryable: false,
            retry_after: None,
        });

        let reference = store
            .write(&snapshot)
            .expect("unsafe snapshot should persist for audit");

        assert!(!reference.retryable);
        assert!(reference.requires_manual_confirmation);
        assert_eq!(
            reference.blocked_reason.as_deref(),
            Some("tool_side_effect_in_flight")
        );
    }

    /// Verifies an unknown Provider dispatch remains retryable when no tool side effect was observed.
    /// 验证未观察到工具副作用时，分派结果未知的 Provider 请求仍可重试。
    #[test]
    fn retry_snapshot_allows_unknown_provider_dispatch_without_tool_side_effect() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let mut snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        // request is the exact logical Provider request that Continue may reissue as a new physical attempt.
        // request 是 Continue 可作为新物理尝试重新发出的精确逻辑 Provider 请求。
        let request =
            serde_json::from_value::<vulcan_ai_contract::AiExecutionRequest>(serde_json::json!({
                "operation": "conversation_respond",
                "model_id": "model-a"
            }))
            .expect("minimal provider request should decode through defaulted fields");
        snapshot.pending_action = ExecutionRetryPendingAction::DispatchProviderRequest {
            request: Box::new(request),
        };
        snapshot.error = Some(AiExecutionError {
            category: AiErrorCategory::ProviderUnavailable,
            subcategory: Some(String::from("transport_interrupted")),
            stage: String::from("provider_request"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: String::from("dispatch outcome is unknown"),
            dispatch_outcome: DispatchOutcome::Unknown,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: true,
            retry_after: None,
        });

        let reference = store
            .write(&snapshot)
            .expect("unknown dispatch snapshot should persist for audit");

        assert!(reference.retryable);
        assert!(!reference.requires_manual_confirmation);
        assert_eq!(reference.blocked_reason, None);
        assert!(reference.replay_safety_proven());
    }

    /// Verifies retained tool-batch and final-assistant actions are both claimable when exact.
    /// 验证保留工具批次与最终助手动作在边界精确时均可领取。
    #[test]
    fn retry_snapshot_supports_tool_batch_and_final_assistant_actions() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // toolSnapshot represents a Provider-completed batch whose tool body has not started.
        // toolSnapshot 表示 Provider 已完成但工具主体尚未启动的批次。
        let tool_snapshot = test_execute_tool_batch_snapshot(uuid::Uuid::new_v4().to_string());
        let tool_reference = store
            .write(&tool_snapshot)
            .expect("exact tool batch should persist");
        assert!(tool_reference.replay_safety_proven());
        assert_eq!(tool_reference.blocked_reason, None);

        // finalSnapshot represents a tool-free assistant tail already proven by the Provider.
        // finalSnapshot 表示已由 Provider 证明的无工具助手尾部。
        let mut final_snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        final_snapshot.controller_snapshot.messages =
            vec![vulcan_core::types::ConversationMessage {
                id: String::from("assistant-final-1"),
                payload: vulcan_core::types::ConversationPayload::Assistant {
                    text: String::from("completed answer"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("provider-message-final-1")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            }];
        final_snapshot.pending_action = ExecutionRetryPendingAction::FinalizeAssistant;
        let final_reference = store
            .write(&final_snapshot)
            .expect("exact final assistant boundary should persist");
        assert!(final_reference.replay_safety_proven());
        assert_eq!(final_reference.blocked_reason, None);
    }

    /// Verifies a changed tool binding is retained for audit but exposed as a precise blocker.
    /// 验证变化后的工具绑定会保留用于审计，但投影为精确阻断原因。
    #[test]
    fn retry_snapshot_blocks_changed_tool_binding_with_exact_reason() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let mut snapshot = test_execute_tool_batch_snapshot(uuid::Uuid::new_v4().to_string());
        let ExecutionRetryPendingAction::ExecuteToolBatch { tools } = &mut snapshot.pending_action
        else {
            panic!("tool batch fixture must retain tool progress");
        };
        tools[0].binding_fingerprint = Some(String::from("changed"));

        let reference = store
            .write(&snapshot)
            .expect("corrupt tool binding should persist for audit");

        assert!(!reference.retryable);
        assert!(reference.requires_manual_confirmation);
        assert_eq!(
            reference.blocked_reason.as_deref(),
            Some("ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BINDING_MISMATCH")
        );
        assert!(!reference.replay_safety_proven());
    }

    /// Verifies an unresolved binding is claimable only while its exact tool slot is proven unstarted.
    /// 验证未解析绑定仅在精确工具槽位已证明未启动时才可领取。
    #[test]
    fn retry_snapshot_allows_unprepared_binding_only_for_not_started_tool() {
        // tempRoot owns the isolated immutable sidecars for both state classifications.
        // tempRoot 持有两种状态分类使用的隔离不可变 sidecar。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store applies the production structural blocker calculation.
        // store 应用生产环境的结构性阻断计算。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // notStarted retains the exact assistant call while deferring binding freeze to BatchPrepared.
        // notStarted 保留精确助手调用，并把绑定冻结延迟到 BatchPrepared。
        let mut not_started = test_execute_tool_batch_snapshot(uuid::Uuid::new_v4().to_string());
        let ExecutionRetryPendingAction::ExecuteToolBatch { tools } =
            &mut not_started.pending_action
        else {
            panic!("tool batch fixture must retain tool progress");
        };
        tools[0].binding_fingerprint = None;

        let not_started_reference = store
            .write(&not_started)
            .expect("unprepared not-started slot should persist");
        assert!(not_started_reference.replay_safety_proven());
        assert_eq!(not_started_reference.blocked_reason, None);

        // running proves a body cannot cross Started without one exact frozen binding.
        // running 证明工具主体不能在缺少精确冻结绑定时跨过 Started。
        let mut running = not_started;
        running.snapshot_id = uuid::Uuid::new_v4().to_string();
        let ExecutionRetryPendingAction::ExecuteToolBatch { tools } = &mut running.pending_action
        else {
            panic!("tool batch fixture must retain tool progress");
        };
        tools[0].state = ExecutionRetryToolState::Running;

        let running_reference = store
            .write(&running)
            .expect("invalid running slot should persist for audit");
        assert!(!running_reference.replay_safety_proven());
        assert_eq!(
            running_reference.blocked_reason.as_deref(),
            Some("ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BINDING_MISMATCH")
        );
    }

    /// Verifies unknown schemas and path-like identities are rejected before publication.
    /// 验证未知 Schema 与路径式身份会在发布前被拒绝。
    #[test]
    fn retry_snapshot_rejects_unknown_schema_and_invalid_identity() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let mut snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        snapshot.schema_version = EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION + 1;
        assert!(store.write(&snapshot).is_err());

        snapshot.schema_version = EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION;
        snapshot.snapshot_id = String::from("../escape");
        assert!(store.write(&snapshot).is_err());
    }

    /// Verifies a malformed durable user follow-up is rejected before any sidecar is published.
    /// 验证格式错误的持久用户补充消息会在发布任何 sidecar 前被拒绝。
    #[test]
    fn retry_snapshot_rejects_invalid_user_followup() {
        // tempRoot owns the isolated directory that must remain free of an invalid sidecar.
        // tempRoot 持有不得写入无效 sidecar 的隔离目录。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store applies the same structural validation to every write and subsequent read.
        // store 对每次写入及后续读取应用相同的结构校验。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // snapshot carries an empty visible prompt that cannot form a real transcript record.
        // snapshot 携带无法形成真实转录记录的空白可见提示词。
        let mut snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        snapshot.user_followup = Some(vulcan_workbench_model::ExecutionRetryUserFollowup {
            visible_prompt: String::from("   "),
            provider_prompt: String::from("provider prompt"),
            client_submission_id: String::from("submission-1"),
        });

        let error = store
            .write(&snapshot)
            .expect_err("invalid user follow-up must fail closed");

        assert!(error.contains("invalid user follow-up boundary"));
    }
}
