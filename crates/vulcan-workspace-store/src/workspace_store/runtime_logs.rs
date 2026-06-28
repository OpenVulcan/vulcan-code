use super::*;

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
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    let Some(session_dir) =
        find_existing_segmented_context_session_dir(data_root, project_id, session_id)?
    else {
        return Ok(());
    };
    let state_document = state_document_from_runtime_state(&session_id.to_string(), runtime_state)?;
    write_state(&session_dir, &state_document)
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
    data_root: &std::path::Path,
    session_id: SessionId,
    compaction_id: &str,
    timestamp: &str,
) -> Result<Vec<String>, String> {
    let Some(session_dir) =
        find_segmented_session_directory_by_id(data_root, &session_id.to_string())?
    else {
        return Ok(Vec::new());
    };
    backup_runtime_log_before_compaction(&session_dir, compaction_id, timestamp)
}

/// Records the provider-facing injection view applied to one session turn.
/// 记录单个会话回合已应用的 Provider 可见注入视图。
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
/// # Returns
/// 返回值
/// `true` when the runtime event was written; missing sessions or empty runtime logs return `false`.
/// 写入运行时事件时返回 `true`；会话缺失或 runtime 日志为空时返回 `false`。
pub fn record_session_runtime_injection_applied(
    data_root: &std::path::Path,
    session_id: SessionId,
    turn_id: Option<String>,
    combined_fingerprint: Option<String>,
    persistent_blocks: &[String],
    transient_prefix: Option<&str>,
    merged_baseline_fingerprint: Option<String>,
    timestamp: &str,
) -> Result<bool, String> {
    let Some(session_dir) =
        find_segmented_session_directory_by_id(data_root, &session_id.to_string())?
    else {
        return Ok(false);
    };
    let persistent_fingerprint = injection_text_parts_fingerprint(persistent_blocks);
    let transient_fingerprint =
        transient_prefix.and_then(|prefix| injection_text_fingerprint(prefix));
    let source_node_ids = injection_applied_source_node_ids(persistent_blocks, transient_prefix);
    record_runtime_injection_applied(
        &session_dir,
        timestamp,
        vulcan_context::RuntimeInjectionAppliedEvent {
            turn_id,
            combined_fingerprint,
            persistent_fingerprint,
            transient_fingerprint,
            has_transient: transient_prefix.is_some_and(|prefix| !prefix.trim().is_empty()),
            merged_baseline_fingerprint,
            source_node_ids,
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
/// `true` when the runtime event was written; missing sessions or empty runtime logs return `false`.
/// 写入运行时事件时返回 `true`；会话缺失或 runtime 日志为空时返回 `false`。
pub fn record_session_runtime_injection_cancelled(
    data_root: &std::path::Path,
    session_id: SessionId,
    event: vulcan_context::RuntimeInjectionCancelledEvent,
    timestamp: &str,
) -> Result<bool, String> {
    record_session_runtime_audit_event(
        data_root,
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
/// `true` when the runtime event was written; missing sessions or empty runtime logs return `false`.
/// 写入运行时事件时返回 `true`；会话缺失或 runtime 日志为空时返回 `false`。
pub fn record_session_runtime_injection_merged(
    data_root: &std::path::Path,
    session_id: SessionId,
    event: vulcan_context::RuntimeInjectionMergedEvent,
    timestamp: &str,
) -> Result<bool, String> {
    record_session_runtime_audit_event(
        data_root,
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
/// `true` when the runtime event was written; missing sessions or unavailable runtime logs return `false`.
/// 写入运行时事件时返回 `true`；会话缺失或 runtime 日志不可用时返回 `false`。
pub fn record_session_runtime_audit_event(
    data_root: &std::path::Path,
    session_id: SessionId,
    event: vulcan_context::RuntimeLogEvent,
    timestamp: &str,
) -> Result<bool, String> {
    let Some(session_dir) =
        find_segmented_session_directory_by_id(data_root, &session_id.to_string())?
    else {
        return Ok(false);
    };
    record_runtime_audit_event(&session_dir, timestamp, event)
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
/// Runtime system updates in log order; missing sessions or missing runtime logs return an empty list.
/// 按日志顺序返回 runtime system update；会话或 runtime 日志缺失时返回空列表。
pub fn load_session_runtime_system_updates(
    data_root: &std::path::Path,
    session_id: SessionId,
) -> Result<Vec<vulcan_context::RuntimeSystemUpdateEvent>, String> {
    let Some(session_dir) =
        find_segmented_session_directory_by_id(data_root, &session_id.to_string())?
    else {
        return Ok(Vec::new());
    };
    let Some(lines) = read_runtime_log_lines(&session_dir)? else {
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

/// Loads lightweight child-task mirrors from the parent session metadata without reading child transcripts.
/// 从父会话元数据加载轻量子任务镜像，不读取子任务转录正文。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `session_id` identifies the parent session whose child task mirrors should be read.
/// `session_id` 标识需要读取子任务镜像的父会话。
///
/// # Returns
/// 返回值
/// Lightweight task mirror rows from the parent `metadata.json`; missing sessions return an empty list.
/// 来自父级 `metadata.json` 的轻量任务镜像；会话缺失时返回空列表。
pub fn load_segmented_session_task_mirrors(
    data_root: &std::path::Path,
    session_id: SessionId,
) -> Result<Vec<SessionTaskMirrorSnapshot>, String> {
    let Some(session_dir) =
        find_segmented_session_directory_by_id(data_root, &session_id.to_string())?
    else {
        return Ok(Vec::new());
    };
    load_segmented_session_task_mirrors_from_dir(&session_dir)
}

/// Loads lightweight child-task mirrors by checking only one known owning project.
/// 仅检查一个已知所属项目来加载轻量子任务镜像。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `project_id` is the known segmented project tag for the parent session.
/// `project_id` 是父会话已知的分段项目标签。
/// `session_id` identifies the parent session whose child task mirrors should be read.
/// `session_id` 标识需要读取子任务镜像的父会话。
///
/// # Returns
/// 返回值
/// Lightweight task mirror rows from the parent `metadata.json`; missing sessions return an empty list.
/// 来自父级 `metadata.json` 的轻量任务镜像；会话缺失时返回空列表。
pub fn load_segmented_session_task_mirrors_for_project(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
) -> Result<Vec<SessionTaskMirrorSnapshot>, String> {
    let session_id_text = session_id.to_string();
    let active_dir =
        find_segmented_session_directory(data_root, false, project_id, &session_id_text)?;
    let session_dir = match active_dir {
        Some(session_dir) => Some(session_dir),
        None => find_segmented_session_directory(data_root, true, project_id, &session_id_text)?,
    };
    let Some(session_dir) = session_dir else {
        return Ok(Vec::new());
    };
    load_segmented_session_task_mirrors_from_dir(&session_dir)
}

/// Loads the display-only fork source marker from one known owning project.
/// 从一个已知所属项目加载纯展示分叉来源标记。
///
/// # Parameters
/// 参数
/// `data_root` is the Vulcan Code data root containing segmented session directories.
/// `data_root` 是包含分段会话目录的 Vulcan Code 数据根目录。
/// `project_id` is the known segmented project tag for the session.
/// `project_id` 是会话已知的分段项目标签。
/// `session_id` identifies the session whose fork source should be read.
/// `session_id` 标识需要读取分叉来源的会话。
///
/// # Returns
/// 返回值
/// A display-only fork source marker when present.
/// 存在时返回纯展示分叉来源标记。
pub fn load_segmented_session_fork_source_for_project(
    data_root: &std::path::Path,
    project_id: &str,
    session_id: SessionId,
) -> Result<Option<SessionForkSourceSnapshot>, String> {
    let session_id_text = session_id.to_string();
    let active_dir =
        find_segmented_session_directory(data_root, false, project_id, &session_id_text)?;
    let session_dir = match active_dir {
        Some(session_dir) => Some(session_dir),
        None => find_segmented_session_directory(data_root, true, project_id, &session_id_text)?,
    };
    let Some(session_dir) = session_dir else {
        return Ok(None);
    };
    let metadata = segmented_session_store::read_session_metadata(&session_dir)?;
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
fn load_segmented_session_task_mirrors_from_dir(
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
