use super::segmented_session_store::{
    append_segmented_session_message_update, cached_runtime_system_updates,
    invalidate_runtime_system_update_cache, read_runtime_conversation_state,
    repair_runtime_log_lines, repair_segmented_session_display_if_needed,
    replace_segmented_session_message_records_for_repair, replay_segmented_session_context,
    sync_runtime_log_from_conversation_state,
};
use super::*;
use crate::conversation_protocol::conversation_state_to_session_messages;

/// Syncs provider-facing runtime JSONL for an already resolved segmented session directory.
/// 为已经解析出的分段会话目录同步 Provider 可见 runtime JSONL。
///
/// # Parameters
/// 参数
/// `session_dir` is the main-session or task-session directory that owns the runtime files.
/// `session_dir` 是拥有 runtime 文件的主会话或子任务会话目录。
/// `conversation_state` is the canonical provider-facing conversation state.
/// `conversation_state` 是规范的 Provider 可见会话状态。
/// `updated_at` is written to new or rebuilt runtime log lines.
/// `updated_at` 会写入新增或重建的 runtime 日志行。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` after the runtime log is synchronized.
/// runtime 日志同步完成后返回 `Ok(())`。
pub fn sync_segmented_session_runtime_log_from_conversation_state(
    session_dir: &std::path::Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
) -> Result<(), String> {
    sync_runtime_log_from_conversation_state(session_dir, conversation_state, updated_at)?;
    Ok(())
}

/// Reads provider-facing runtime JSONL for an already resolved segmented session directory.
/// 读取已经解析出的分段会话目录中的 Provider 可见 runtime JSONL。
///
/// # Parameters
/// 参数
/// `session_dir` is the main-session or task-session directory that owns the runtime files.
/// `session_dir` 是拥有 runtime 文件的主会话或子任务会话目录。
///
/// # Returns
/// 返回值
/// Returns the canonical runtime conversation state when runtime files exist.
/// 当 runtime 文件存在时返回规范运行态会话状态。
pub fn read_segmented_session_runtime_conversation_state(
    session_dir: &std::path::Path,
) -> Result<Option<SessionConversationState>, String> {
    read_runtime_conversation_state(session_dir)
}

/// Explicitly repairs runtime-log storage for one already resolved segmented session.
/// 显式修复一个已经解析的分段会话运行时日志存储。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact session directory selected by a maintenance command.
/// `session_dir` 是由维护命令选择的精确会话目录。
///
/// # Returns
/// 返回值
/// The repaired runtime-line count, zero for consistent absence, or a strict repair failure.
/// 返回修复后的运行时日志行数；一致缺失时返回零，否则返回严格修复失败。
pub fn repair_segmented_session_runtime_storage(
    session_dir: &std::path::Path,
) -> Result<usize, String> {
    Ok(repair_runtime_log_lines(session_dir)?
        .map(|lines| lines.len())
        .unwrap_or(0))
}

/// Releases the process-local runtime SystemUpdate projection for one exact inactive session.
/// 释放一个精确非活动会话的进程内运行时 SystemUpdate 投影。
///
/// # Parameters
/// 参数
/// `session_dir` is the deterministic directory of the execution generation that just ended.
/// `session_dir` 是刚刚结束的执行代次对应的确定性目录。
///
/// # Returns
/// 返回值
/// Success after exact-key removal; no filesystem path is opened or enumerated.
/// 精确键移除后返回成功；不会打开或枚举任何文件系统路径。
pub fn release_session_runtime_system_update_cache(
    session_dir: &std::path::Path,
) -> Result<(), String> {
    invalidate_runtime_system_update_cache(session_dir)
}

/// Explicitly repairs display transcript storage for one already resolved segmented session.
/// 显式修复一个已经解析的分段会话展示转录存储。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact session directory selected by a maintenance command.
/// `session_dir` 是由维护命令选择的精确会话目录。
///
/// # Returns
/// 返回值
/// `true` when a display rebuild was published, `false` when only validation or narrow tail repair was needed.
/// 发布展示重建时返回 `true`；仅需验证或定向尾修复时返回 `false`。
pub fn repair_segmented_session_display_storage(
    session_dir: &std::path::Path,
) -> Result<bool, String> {
    Ok(repair_segmented_session_display_if_needed(session_dir)?.is_some())
}

/// Replaces damaged display rows with one complete history during explicit maintenance.
/// 在显式维护期间使用一份完整历史替换受损展示记录。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact session directory selected by the maintenance caller.
/// `session_dir` 是维护调用方选定的精确会话目录。
/// `messages` is the complete verified display history, including every retained process row.
/// `messages` 是经过完整校验的展示历史，包括所有需要保留的过程行。
///
/// # Returns
/// 返回值
/// Returns the number of rewritten display-event lines after the exclusive repair succeeds.
/// 在独占修复成功后返回重写的展示事件行数量。
pub fn replace_segmented_session_display_storage_for_repair(
    session_dir: &std::path::Path,
    messages: &[SessionMessageEntry],
) -> Result<usize, String> {
    Ok(replace_segmented_session_message_records_for_repair(session_dir, messages)?.len())
}

/// Reads display transcript state from an already resolved segmented session directory.
/// 从已经解析出的分段会话目录读取展示转录状态。
///
/// # Parameters
/// 参数
/// `session_dir` is the main-session or task-session directory that owns display segments.
/// `session_dir` 是拥有展示分段的主会话或子任务会话目录。
///
/// # Returns
/// 返回值
/// Returns replayed display conversation state when display rows exist.
/// 当展示行存在时返回重放后的展示会话状态。
pub fn read_segmented_session_display_conversation_state(
    session_dir: &std::path::Path,
) -> Result<Option<SessionConversationState>, String> {
    Ok(replay_segmented_session_context(session_dir)?.map(|replay| replay.conversation_state))
}

/// Syncs display transcript segments from a canonical conversation state for a resolved directory.
/// 为已解析目录根据规范会话状态同步展示转录分段。
///
/// # Parameters
/// 参数
/// `session_dir` is the main-session or task-session directory that owns display segments.
/// `session_dir` 是拥有展示分段的主会话或子任务会话目录。
/// `conversation_state` is the canonical display-capable conversation state.
/// `conversation_state` 是可投影展示记录的规范会话状态。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` after display transcript rows are appended or rewritten.
/// 展示转录行追加或重写完成后返回 `Ok(())`。
pub fn sync_segmented_session_display_from_conversation_state(
    session_dir: &std::path::Path,
    conversation_state: &SessionConversationState,
) -> Result<(), String> {
    let messages = conversation_state_to_session_messages(conversation_state);
    append_segmented_session_message_update(session_dir, &messages)?;
    Ok(())
}

/// Persists the current segmented context state document for an existing session directory.
/// 为已有会话目录持久化当前分段上下文状态文档。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `project_id` identifies the owning project directory for the session.
/// `project_id` 标识会话所属的项目目录。
/// `session_id` identifies the session whose current context document should be written.
/// `session_id` 标识需要写入当前上下文文档的会话。
/// `runtime_state` is the runtime state projected into the segmented state document.
/// `runtime_state` 是将被投影为分段状态文档的运行态。
///
/// # Returns
/// 返回值
/// Returns success when the document is written or the session directory no longer exists.
/// 当文档写入成功或会话目录已不存在时返回成功。
pub fn persist_segmented_session_context_current_document(
    session_directory: &std::path::Path,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    let state_document = state_document_from_runtime_state(&session_id.to_string(), runtime_state)?;
    write_state(session_directory, &state_document)
}

/// Backs up provider-facing runtime JSONL files before context compaction changes a session.
/// 在上下文压缩修改会话前备份 Provider 可见 runtime JSONL 文件。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the session whose runtime log should be copied.
/// `session_id` 标识需要复制运行时日志的会话。
/// `compaction_id` is the logical compaction identifier used in the backup directory name.
/// `compaction_id` 是用于备份目录命名的逻辑压缩标识。
/// `timestamp` is the compaction creation timestamp used in the backup directory name.
/// `timestamp` 是用于备份目录命名的压缩创建时间戳。
///
/// # Returns
/// 返回值
/// Relative paths copied into the runtime backup directory; missing sessions return an empty list.
/// 已复制到运行时备份目录的相对路径；会话缺失时返回空列表。
pub fn backup_session_runtime_log_before_compaction(
    session_directory: &std::path::Path,
    session_id: SessionId,
    compaction_id: &str,
    timestamp: &str,
) -> Result<Vec<String>, String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    backup_runtime_log_before_compaction(session_directory, compaction_id, timestamp)
}

/// Request for recording the provider-facing injection view applied to one session turn.
/// 记录单个会话回合已应用 Provider 可见注入视图的请求。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the session whose runtime log should receive the audit event.
/// `session_id` 标识需要写入审计事件的会话。
/// `turn_id` is the logical prompt turn identifier when available.
/// `turn_id` 是可用时的逻辑提示词回合标识。
/// `combined_fingerprint` identifies the complete provider-facing injection overlay.
/// `combined_fingerprint` 标识完整的 Provider 可见注入覆盖层。
/// `persistent_blocks` are the persistent system-layer injection blocks.
/// `persistent_blocks` 是长期系统层注入块。
/// `transient_prefix` is the transient user-overlay injection text.
/// `transient_prefix` 是动态用户覆盖注入文本。
/// `merged_baseline_fingerprint` identifies the folded baseline after compaction when known.
/// `merged_baseline_fingerprint` 标识已知的压缩后折叠基线。
/// `timestamp` is the event timestamp stored in runtime JSONL.
/// `timestamp` 是写入 runtime JSONL 的事件时间戳。
///
pub struct SessionRuntimeInjectionAppliedRecord<'a> {
    /// Vulcan Code data root containing segmented session directories.
    /// 包含分段会话目录的 Vulcan Code 数据根目录。
    pub session_directory: &'a std::path::Path,
    /// Session whose runtime log should receive the audit event.
    /// 需要写入审计事件的会话。
    pub session_id: SessionId,
    /// Logical prompt turn identifier when available.
    /// 可用时的逻辑提示词回合标识。
    pub turn_id: Option<String>,
    /// Complete provider-facing injection overlay fingerprint.
    /// 完整 Provider 可见注入覆盖层的指纹。
    pub combined_fingerprint: Option<String>,
    /// Persistent system-layer injection blocks.
    /// 长期系统层注入块。
    pub persistent_blocks: &'a [String],
    /// Transient user-overlay injection text.
    /// 动态用户覆盖注入文本。
    pub transient_prefix: Option<&'a str>,
    /// Folded baseline fingerprint after compaction when known.
    /// 已知的压缩后折叠基线指纹。
    pub merged_baseline_fingerprint: Option<String>,
    /// Active VMM prompt-cache epoch when immutable anchors are present.
    /// 存在不可变锚点时生效的 VMM 提示词缓存纪元。
    pub cache_epoch_id: Option<String>,
    /// Monotonic immutable-anchor revision inside the active VMM cache epoch.
    /// 当前 VMM 缓存纪元内的单调不可变锚点修订号。
    pub anchor_revision: Option<u64>,
    /// Provider-context mutation classification used for continuation decisions.
    /// continuation 决策使用的 Provider 上下文变更分类。
    pub mutation_kind: Option<String>,
    /// Stable VMM anchor identifiers included in the provider-facing request.
    /// Provider 可见请求中包含的稳定 VMM 锚点标识。
    pub vmm_anchor_ids: Vec<String>,
    /// Bytes occupied by currently active VMM structured memory bodies.
    /// 当前活动 VMM 结构化记忆正文占用的字节数。
    pub active_memory_body_bytes: usize,
    /// Estimated tokens occupied by active VMM structured memory bodies.
    /// 活动 VMM 结构化记忆正文占用的估算 Token 数。
    pub active_memory_estimated_tokens: usize,
    /// Bytes held by immutable VMM anchors reclaimable at compaction.
    /// 可在压缩时回收的不可变 VMM 锚点字节数。
    pub reclaimable_anchor_bytes: usize,
    /// Estimated tokens held by reclaimable immutable VMM anchors.
    /// 可回收不可变 VMM 锚点占用的估算 Token 数。
    pub reclaimable_anchor_estimated_tokens: usize,
    /// Event timestamp stored in runtime JSONL.
    /// 写入 runtime JSONL 的事件时间戳。
    pub timestamp: &'a str,
}

/// Records the provider-facing injection view applied to one session turn.
/// 记录单个会话回合已应用的 Provider 可见注入视图。
///
/// # Parameters
/// 参数
/// `request` contains the target session, injection fingerprints, source blocks, and timestamp.
/// `request` 包含目标会话、注入指纹、源块与时间戳。
///
/// # Returns
/// 返回值
/// `true` when written; missing sessions or consistently absent/empty runtime logs return `false`.
/// 写入成功时返回 `true`；会话缺失或运行时日志一致缺失/为空时返回 `false`。
/// Persisted runtime damage and I/O failures remain observable as errors.
/// 已持久化的运行时损坏与 I/O 失败会保持为可观察错误。
pub fn record_session_runtime_injection_applied(
    request: SessionRuntimeInjectionAppliedRecord<'_>,
) -> Result<bool, String> {
    let SessionRuntimeInjectionAppliedRecord {
        session_directory,
        session_id,
        turn_id,
        combined_fingerprint,
        persistent_blocks,
        transient_prefix,
        merged_baseline_fingerprint,
        cache_epoch_id,
        anchor_revision,
        mutation_kind,
        vmm_anchor_ids,
        active_memory_body_bytes,
        active_memory_estimated_tokens,
        reclaimable_anchor_bytes,
        reclaimable_anchor_estimated_tokens,
        timestamp,
    } = request;
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    let persistent_fingerprint = injection_text_parts_fingerprint(persistent_blocks);
    let transient_fingerprint = transient_prefix.and_then(injection_text_fingerprint);
    let source_node_ids = injection_applied_source_node_ids(persistent_blocks, transient_prefix);
    record_runtime_injection_applied(
        session_directory,
        timestamp,
        vulcan_context::RuntimeInjectionAppliedEvent {
            turn_id,
            combined_fingerprint,
            persistent_fingerprint,
            transient_fingerprint,
            has_transient: transient_prefix.is_some_and(|prefix| !prefix.trim().is_empty()),
            merged_baseline_fingerprint,
            source_node_ids,
            cache_epoch_id,
            anchor_revision,
            mutation_kind,
            vmm_anchor_ids,
            active_memory_body_bytes,
            active_memory_estimated_tokens,
            reclaimable_anchor_bytes,
            reclaimable_anchor_estimated_tokens,
        },
    )
}

/// Builds a stable fingerprint for one non-empty injection text block.
/// 为单个非空注入文本块构建稳定指纹。
fn injection_text_fingerprint(text: &str) -> Option<String> {
    let normalized = text.trim();
    if normalized.is_empty() {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

/// Builds a stable fingerprint for an ordered list of provider-facing injection blocks.
/// 为一组有序 Provider 可见注入块构建稳定指纹。
fn injection_text_parts_fingerprint(parts: &[String]) -> Option<String> {
    let normalized_parts = parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if normalized_parts.is_empty() {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    normalized_parts.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

/// Builds stable source identifiers for applied injection blocks without storing injection text.
/// 为已应用注入块构建稳定来源标识，同时不存储注入正文。
fn injection_applied_source_node_ids(
    persistent_blocks: &[String],
    transient_prefix: Option<&str>,
) -> Vec<String> {
    let mut source_node_ids = persistent_blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            injection_text_fingerprint(block)
                .map(|fingerprint| format!("persistent:{index}:{fingerprint}"))
        })
        .collect::<Vec<_>>();
    if let Some(fingerprint) = transient_prefix.and_then(injection_text_fingerprint) {
        source_node_ids.push(format!("transient:0:{fingerprint}"));
    }
    source_node_ids
}

/// Records that an injection source was cancelled and must not enter future provider requests.
/// 记录某个注入来源已取消，后续不得进入 Provider 请求。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the session whose runtime log should receive the cancellation event.
/// `session_id` 标识需要写入取消事件的会话。
/// `event` describes the cancelled source nodes and previous fingerprints.
/// `event` 描述被取消的来源节点和旧指纹。
/// `timestamp` is the event timestamp stored in runtime JSONL.
/// `timestamp` 是写入 runtime JSONL 的事件时间戳。
///
/// # Returns
/// 返回值
/// `true` when written; missing sessions or consistently absent/empty runtime logs return `false`.
/// 写入成功时返回 `true`；会话缺失或运行时日志一致缺失/为空时返回 `false`。
/// Persisted runtime damage and I/O failures remain observable as errors.
/// 已持久化的运行时损坏与 I/O 失败会保持为可观察错误。
pub fn record_session_runtime_injection_cancelled(
    session_directory: &std::path::Path,
    session_id: SessionId,
    event: vulcan_context::RuntimeInjectionCancelledEvent,
    timestamp: &str,
) -> Result<bool, String> {
    record_session_runtime_audit_event(
        session_directory,
        session_id,
        vulcan_context::RuntimeLogEvent::InjectionCancelled(event),
        timestamp,
    )
}

/// Records that persistent injection content was merged into a stable baseline.
/// 记录长期注入内容已合并进稳定基线。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the session whose runtime log should receive the merge event.
/// `session_id` 标识需要写入合并事件的会话。
/// `event` describes the merged baseline and included source nodes.
/// `event` 描述合并后的基线和纳入的来源节点。
/// `timestamp` is the event timestamp stored in runtime JSONL.
/// `timestamp` 是写入 runtime JSONL 的事件时间戳。
///
/// # Returns
/// 返回值
/// `true` when written; missing sessions or consistently absent/empty runtime logs return `false`.
/// 写入成功时返回 `true`；会话缺失或运行时日志一致缺失/为空时返回 `false`。
/// Persisted runtime damage and I/O failures remain observable as errors.
/// 已持久化的运行时损坏与 I/O 失败会保持为可观察错误。
pub fn record_session_runtime_injection_merged(
    session_directory: &std::path::Path,
    session_id: SessionId,
    event: vulcan_context::RuntimeInjectionMergedEvent,
    timestamp: &str,
) -> Result<bool, String> {
    record_session_runtime_audit_event(
        session_directory,
        session_id,
        vulcan_context::RuntimeLogEvent::InjectionMerged(event),
        timestamp,
    )
}

/// Records one runtime audit event for a segmented session when the session can be found.
/// 当能够找到分段会话时，为该会话记录单个运行时审计事件。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the session whose runtime log should receive the audit event.
/// `session_id` 标识需要写入审计事件的会话。
/// `event` is the runtime-only event to append to `runtime.jsonl`.
/// `event` 是需要追加到 `runtime.jsonl` 的仅运行时事件。
/// `timestamp` is the event timestamp stored in runtime JSONL.
/// `timestamp` 是写入 runtime JSONL 的事件时间戳。
///
/// # Returns
/// 返回值
/// `true` when written; missing sessions or consistently absent/empty runtime logs return `false`.
/// 写入成功时返回 `true`；会话缺失或运行时日志一致缺失/为空时返回 `false`。
/// Persisted runtime damage and I/O failures remain observable as errors.
/// 已持久化的运行时损坏与 I/O 失败会保持为可观察错误。
pub fn record_session_runtime_audit_event(
    session_directory: &std::path::Path,
    session_id: SessionId,
    event: vulcan_context::RuntimeLogEvent,
    timestamp: &str,
) -> Result<bool, String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    record_runtime_audit_event(session_directory, timestamp, event)
}

/// Loads chronological runtime system updates for one segmented session.
/// 加载单个分段会话的时间顺序 runtime system update。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the session whose runtime log should be scanned.
/// `session_id` 标识需要扫描 runtime 日志的会话。
///
/// # Returns
/// 返回值
/// Runtime system updates in log order; missing sessions or consistently absent logs return an empty list.
/// 按日志顺序返回运行时系统更新；会话缺失或日志一致缺失时返回空列表。
/// Persisted runtime damage and I/O failures remain observable as errors.
/// 已持久化的运行时损坏与 I/O 失败会保持为可观察错误。
pub fn load_session_runtime_system_updates(
    session_directory: &std::path::Path,
    session_id: SessionId,
) -> Result<Vec<vulcan_context::RuntimeSystemUpdateEvent>, String> {
    validate_exact_segmented_session_directory_owner(session_directory, session_id)?;
    // cached_updates keeps retained execution turns on the zero-runtime-log-read path.
    // cached_updates 使保留执行上下文的回合保持运行时日志零读取路径。
    if let Some(cached_updates) = cached_runtime_system_updates(session_directory)? {
        return Ok(cached_updates);
    }
    // lines performs one strict cold recovery only before a resident projection exists.
    // lines 仅在常驻投影尚不存在时执行一次严格冷恢复。
    let Some(lines) = read_runtime_log_lines(session_directory)? else {
        return Ok(Vec::new());
    };
    Ok(lines
        .into_iter()
        .filter_map(|line| match line.event {
            vulcan_context::RuntimeLogEvent::SystemUpdate(event) => Some(event),
            _ => None,
        })
        .collect())
}

/// Loads the display-only fork source marker from one resolved segmented session directory.
/// 从一个已解析的分段会话目录加载纯展示分叉来源标记。
///
/// # Parameters
/// 参数
/// `session_dir` points at the resolved segmented session directory.
/// `session_dir` 指向已解析的分段会话目录。
///
/// # Returns
/// 返回值
/// A display-only fork source marker when present.
/// 存在时返回纯展示分叉来源标记。
pub fn load_segmented_session_fork_source_from_directory(
    session_dir: &std::path::Path,
) -> Result<Option<SessionForkSourceSnapshot>, String> {
    let metadata = segmented_session_store::read_session_metadata(session_dir)?;
    Ok(metadata
        .fork_source
        .map(|source| SessionForkSourceSnapshot {
            source_project_tag: source.source_project_tag,
            source_session_id: source.source_session_id,
            source_title: source.source_title,
            anchor_message_id: source.anchor_message_id,
            created_at: source.created_at,
        }))
}

/// Loads lightweight child-task mirrors from one already resolved session directory.
/// 从一个已解析的会话目录加载轻量子任务镜像。
///
/// # Parameters
/// 参数
/// `session_dir` points at the resolved segmented parent session directory.
/// `session_dir` 指向已解析的分段父会话目录。
///
/// # Returns
/// 返回值
/// Lightweight task mirror rows from the parent `metadata.json`.
/// 来自父级 `metadata.json` 的轻量任务镜像行。
pub fn load_segmented_session_task_mirrors_from_directory(
    session_dir: &std::path::Path,
) -> Result<Vec<SessionTaskMirrorSnapshot>, String> {
    let metadata = segmented_session_store::read_session_metadata(session_dir)?;
    Ok(metadata
        .tasks
        .into_iter()
        .filter_map(|task| {
            Some(SessionTaskMirrorSnapshot {
                task_session_id: SessionId::parse_str(&task.session_id).ok()?,
                status: task.status,
                line_count: task.line_count,
                updated_at: task.updated_at,
            })
        })
        .collect())
}
