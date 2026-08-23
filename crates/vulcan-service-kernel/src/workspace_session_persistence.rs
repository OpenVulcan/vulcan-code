//! Workspace session persistence tasks.
//! 工作区会话持久化任务。

use super::*;
use std::collections::VecDeque;
use std::future::Future;
use std::sync::{
    Condvar, Mutex as StdMutex, MutexGuard as StdMutexGuard, OnceLock,
    atomic::{AtomicBool, Ordering},
};

/// Process-local registry of active root-session transcript writer cursors.
/// 活动根会话转录写入游标的进程内注册表。
static SESSION_TRANSCRIPT_WRITERS: OnceLock<
    StdMutex<BTreeMap<SessionId, Arc<StdMutex<SessionTranscriptWriterCursor>>>>,
> = OnceLock::new();

/// Global pending-command counter used only by the application shutdown drain.
/// 仅供应用关闭排空使用的全局待处理命令计数器。
static SESSION_TRANSCRIPT_WRITER_PENDING: OnceLock<(StdMutex<usize>, Condvar)> = OnceLock::new();

/// Process-local mutex that makes command admission and shutdown sealing one atomic decision.
/// 使命令准入与关闭封印成为单次原子决策的进程内互斥锁。
static SESSION_TRANSCRIPT_WRITER_ADMISSION: OnceLock<StdMutex<()>> = OnceLock::new();

/// Process-local admission seal set immediately before the final shutdown drain.
/// 在最终关闭排空前立即设置的进程内接纳封印。
static SESSION_TRANSCRIPT_WRITER_SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

/// In-memory committed and durable boundaries for one exact root-session writer.
/// 单个精确根会话写入器的内存已提交与可靠边界。
struct SessionTranscriptWriterCursor {
    /// Exact session directory resolved from the retained locator.
    /// 从保留定位信息解析出的精确会话目录。
    session_directory: PathBuf,
    /// Opaque workspace-store cursor that keeps the exact metadata frontier resident.
    /// 将精确元数据前沿常驻内存的不透明 workspace-store 游标。
    append_cursor: Option<
        Arc<StdMutex<vulcan_workspace_store::workspace_store::SessionTranscriptAppendCursor>>,
    >,
    /// Frontier reserved by all commands accepted into the ordered queue.
    /// 已被有序队列接纳的全部命令所预留的前沿。
    committed_checkpoint:
        Option<vulcan_workspace_store::workspace_store::PersistedSessionTranscriptCheckpoint>,
    /// Frontier confirmed durable by completed append operations.
    /// 已由完成追加操作确认可靠的前沿。
    durable_checkpoint:
        Option<vulcan_workspace_store::workspace_store::PersistedSessionTranscriptCheckpoint>,
    /// Number of accepted commands not yet completed by the writer.
    /// 已接纳但尚未由写入器完成的命令数量。
    queue_depth: usize,
    /// Ordered immutable commands retained until their exact committed boundaries become durable.
    /// 按顺序保留的不可变命令，直到其精确已提交边界变为可靠状态。
    retry_commands: VecDeque<Arc<SessionTranscriptWriterRetryCommand>>,
    /// First persistence failure that blocks later dependent commands.
    /// 阻止后续依赖命令的首个持久化失败。
    blocked_error: Option<String>,
    /// Waiter notification emitted whenever durable state or blocking state changes.
    /// 可靠状态或阻塞状态变化时发送的等待者通知。
    notify: Arc<tokio::sync::Notify>,
}

/// Immutable physical append payload retained until its exact boundary becomes durable.
/// 保留到精确边界变为可靠状态为止的不可变物理追加载荷。
struct SessionTranscriptWriterRetryCommand {
    /// Exact deterministic target directory.
    /// 精确确定性的目标目录。
    session_directory: PathBuf,
    /// Durable boundary required immediately before this command.
    /// 当前命令之前必须存在的可靠边界。
    expected_checkpoint:
        vulcan_workspace_store::workspace_store::PersistedSessionTranscriptCheckpoint,
    /// Boundary committed by this command after its immutable suffix.
    /// 当前命令不可变后缀之后提交的边界。
    committed_checkpoint:
        vulcan_workspace_store::workspace_store::PersistedSessionTranscriptCheckpoint,
    /// Exact immutable transcript suffix owned by this command.
    /// 当前命令拥有的精确不可变转录后缀。
    tail_messages: Arc<Vec<SessionMessageEntry>>,
}

/// Immutable command accepted by one session writer in canonical order.
/// 由单个会话写入器按规范顺序接纳的不可变命令。
struct SessionTranscriptWriterCommand {
    /// Shared exact-session cursor owning ordering and acknowledgements.
    /// 拥有顺序与确认信息的共享精确会话游标。
    cursor: Arc<StdMutex<SessionTranscriptWriterCursor>>,
    /// Retry payload shared with the cursor until durable completion.
    /// 与游标共享且保留到可靠完成为止的重试载荷。
    retry: Arc<SessionTranscriptWriterRetryCommand>,
}

/// One admitted command paired with shutdown-visible RAII accounting before it leaves admission.
/// 一条已准入命令，在离开准入区前即配对关闭流程可见的 RAII 记账。
struct PreparedSessionTranscriptWriterCommand {
    /// Immutable exact-row writer command.
    /// 不可变的精确记录写入命令。
    command: SessionTranscriptWriterCommand,
    /// Pending token retained until the spawned writer task terminates.
    /// 保留到生成的写入任务终止为止的待处理令牌。
    pending_command: PendingTranscriptWriterCommand,
}

/// RAII accounting token for one accepted transcript writer command.
/// 单个已接纳转录写入命令的 RAII 计数令牌。
struct PendingTranscriptWriterCommand;

impl PendingTranscriptWriterCommand {
    /// Increments the shutdown-visible pending command count.
    /// 增加关闭流程可见的待处理命令计数。
    ///
    /// # Returns
    /// 返回值
    /// An accounting token that decrements the count when dropped.
    /// 返回一个在释放时减少计数的记账令牌。
    fn begin() -> Self {
        // lifecycle owns the counter mutex and its drain condition variable.
        // lifecycle 持有计数互斥锁及其排空条件变量。
        let lifecycle = transcript_writer_pending_lifecycle();
        // pending_count is modified before the task can be spawned or observed by shutdown.
        // pending_count 在任务生成或关闭流程观察之前完成修改。
        let mut pending_count = lifecycle
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *pending_count = pending_count.saturating_add(1);
        Self
    }
}

impl Drop for PendingTranscriptWriterCommand {
    /// Decrements the pending count and wakes shutdown after every command completion.
    /// 每条命令完成后减少待处理计数并唤醒关闭流程。
    fn drop(&mut self) {
        let lifecycle = transcript_writer_pending_lifecycle();
        let mut pending_count = lifecycle
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *pending_count = pending_count.saturating_sub(1);
        lifecycle.1.notify_all();
    }
}

/// Detached data required to persist exactly one regular workspace session.
/// 持久化一个精确普通工作区会话所需的分离数据。
struct WorkspaceSessionPersistencePayload {
    /// Exact project owner carried by the retained session locator.
    /// 保留会话定位信息携带的精确项目归属。
    project_id: String,
    /// Exact session summary being persisted.
    /// 正在持久化的精确会话摘要。
    session: SessionSummary,
    /// Whether the exact session currently belongs to archived storage.
    /// 精确会话当前是否属于归档存储。
    archived: bool,
    /// Stored display transcript supplied to the workspace-store canonical merge.
    /// 提供给 workspace-store 规范合并的已存展示转录。
    messages: Vec<SessionMessageEntry>,
    /// Optional canonical runtime state for the exact session.
    /// 精确会话的可选规范运行态。
    runtime_state: Option<SessionRuntimeStateEntry>,
}

/// Detached history-free payload for one authoritative retained-session write.
/// 单个权威保留会话写入使用的分离无历史载荷。
struct RetainedWorkspaceSessionPersistencePayload {
    /// Exact deterministic directory resolved before releasing retained state.
    /// 释放保留状态前解析出的精确确定性目录。
    session_directory: PathBuf,
    /// Exact project owner carried by the retained session locator.
    /// 保留会话定位信息携带的精确项目归属。
    project_id: String,
    /// Exact session summary being persisted.
    /// 正在持久化的精确会话摘要。
    session: SessionSummary,
    /// Whether the exact session currently belongs to archived storage.
    /// 精确会话当前是否属于归档存储。
    archived: bool,
    /// Explicit transcript authority available to this exact retained write.
    /// 本次精确保留写入可使用的显式转录权威范围。
    transcript: RetainedTranscriptPersistence,
    /// Optional history-free `state.json` projection produced while borrowing runtime state.
    /// 借用运行态期间生成的可选无历史 `state.json` 投影。
    runtime_projection:
        Option<vulcan_workspace_store::workspace_store::SessionRuntimeStatePersistenceProjection>,
}

/// Transcript authority available to one retained-session persistence operation.
/// 单次保留会话持久化操作可用的转录权威范围。
enum RetainedTranscriptPersistence {
    /// The retained window starts at absolute index zero and contains the complete transcript.
    /// 保留窗口从绝对下标零开始并包含完整转录。
    Complete(Vec<SessionMessageEntry>),
    /// The retained partial window can be reconciled after the global state lock is released.
    /// 保留的局部窗口可在释放全局状态锁后进行协调。
    PartialWindow {
        /// Optional absolute persisted index represented by the first retained message.
        /// 第一条保留消息所表示的可选绝对持久化下标。
        message_start_index: Option<usize>,
        /// Bounded retained rows used to select a checkpointed immutable suffix later.
        /// 稍后用于选择带检查点不可变后缀的有界保留行。
        messages: Vec<SessionMessageEntry>,
    },
    /// Only the existing index row and runtime projection may be updated.
    /// 仅允许更新既有索引行与运行时投影。
    ExistingIndexOnly,
}

/// Classifies retained transcript authority without inferring completeness from vector length.
/// 在不根据向量长度推断完整性的前提下分类保留转录权威。
///
/// # Parameters
/// 参数
/// `messages` is the retained display cache for the exact session.
/// `messages` 是精确会话的保留展示缓存。
/// `window` is the explicit hydration boundary retained for that cache.
/// `window` 是为该缓存保留的显式水合边界。
/// # Returns
/// 返回值
/// Complete authority for an index-zero window, deferred partial authority for a bounded window,
/// or index-only authority when the cache was never hydrated.
/// 对零起点窗口返回完整权威，对已水合窗口返回延迟局部权威；缓存从未水合时仅返回索引权威。
fn classify_retained_transcript_persistence(
    messages: &[SessionMessageEntry],
    window: Option<&HydratedConversationWindowState>,
) -> RetainedTranscriptPersistence {
    match window {
        Some(window) if !window.transcript_window_loaded => {
            RetainedTranscriptPersistence::ExistingIndexOnly
        }
        Some(window) if window.message_start_index == Some(0) && !window.has_more_history => {
            RetainedTranscriptPersistence::Complete(messages.to_vec())
        }
        Some(window) => RetainedTranscriptPersistence::PartialWindow {
            message_start_index: window.message_start_index,
            messages: messages.to_vec(),
        },
        None => RetainedTranscriptPersistence::ExistingIndexOnly,
    }
}

/// Selects one immutable transcript suffix from absolute persisted and retained-window coordinates.
/// 根据绝对持久化坐标与保留窗口坐标选择一段不可变转录后缀。
///
/// # Parameters
/// 参数
/// `messages` is the retained window in ascending display order.
/// `messages` 是按展示正序排列的保留窗口。
/// `message_start_index` is the absolute index represented by `messages[0]`.
/// `message_start_index` 是 `messages[0]` 所表示的绝对下标。
/// `persisted_message_count` is the exact durable prefix length read from metadata.
/// `persisted_message_count` 是从元数据读取的精确可靠前缀长度。
///
/// # Returns
/// 返回值
/// No work when the window end is already durable, otherwise the exact immutable suffix.
/// 当窗口末端已可靠持久化时无工作，否则返回精确的不可变后缀。
fn retained_transcript_tail_for_persisted_boundary(
    messages: &[SessionMessageEntry],
    message_start_index: usize,
    persisted_message_count: usize,
) -> Result<Option<Vec<SessionMessageEntry>>, String> {
    let retained_message_end = message_start_index
        .checked_add(messages.len())
        .ok_or_else(|| {
            format!(
                "Retained transcript window boundary overflowed: start={message_start_index} retained={}",
                messages.len()
            )
        })?;
    if persisted_message_count < message_start_index
        || persisted_message_count > retained_message_end
    {
        return Err(format!(
            "Persisted transcript checkpoint {persisted_message_count} is outside retained absolute window {message_start_index}..={retained_message_end}."
        ));
    }
    if persisted_message_count == retained_message_end {
        return Ok(None);
    }
    let local_boundary = persisted_message_count - message_start_index;
    Ok(Some(messages[local_boundary..].to_vec()))
}

/// Selects an immutable suffix using a stable checkpoint identity before falling back to an absolute coordinate.
/// 优先使用稳定检查点身份选择不可变后缀，之后才回退到绝对坐标。
///
/// # Parameters
/// 参数
/// `messages` is the bounded hydrated display window in ascending order.
/// `messages` 是按正序排列的有界已水合展示窗口。
/// `message_start_index` is the optional absolute coordinate retained by raw hydration.
/// `message_start_index` 是原始水合保留的可选绝对坐标。
/// `checkpoint` is the exact durable transcript frontier read from metadata or a resident cursor.
/// `checkpoint` 是从元数据或常驻游标读取的精确可靠转录前沿。
///
/// # Returns
/// 返回值
/// The exact suffix after a matching durable message, an absolute-coordinate suffix, or no work when neither proof is available.
/// 返回匹配可靠消息之后的精确后缀、绝对坐标后缀；两种证据均不可用时不执行写入。
fn retained_transcript_tail_for_checkpoint(
    messages: &[SessionMessageEntry],
    message_start_index: Option<usize>,
    checkpoint: &vulcan_workspace_store::workspace_store::PersistedSessionTranscriptCheckpoint,
) -> Result<Option<Vec<SessionMessageEntry>>, String> {
    if let (Some(last_message_id), Some(last_message_fingerprint)) = (
        checkpoint.last_message_id.as_deref(),
        checkpoint.last_message_fingerprint.as_deref(),
    ) && let Some((boundary_index, boundary_message)) = messages
        .iter()
        .enumerate()
        .find(|(_, message)| message.id == last_message_id)
    {
        // derived_checkpoint uses the workspace-store fingerprint contract without reading storage.
        // derived_checkpoint 使用 workspace-store 指纹契约且不读取存储。
        let derived_checkpoint =
            vulcan_workspace_store::workspace_store::advance_single_session_transcript_checkpoint(
                &vulcan_workspace_store::workspace_store::PersistedSessionTranscriptCheckpoint {
                    message_count: 0,
                    last_message_id: None,
                    last_message_fingerprint: None,
                },
                std::slice::from_ref(boundary_message),
            )?;
        if derived_checkpoint.last_message_fingerprint.as_deref() != Some(last_message_fingerprint)
        {
            return Err(format!(
                "Retained transcript message `{last_message_id}` conflicts with its durable checkpoint fingerprint."
            ));
        }
        let local_boundary = boundary_index.saturating_add(1);
        if local_boundary == messages.len() {
            return Ok(None);
        }
        return Ok(Some(messages[local_boundary..].to_vec()));
    }
    if let Some(message_start_index) = message_start_index {
        return retained_transcript_tail_for_persisted_boundary(
            messages,
            message_start_index,
            checkpoint.message_count,
        );
    }
    // A boundary projection can preserve complete visible history without a one-to-one raw row coordinate.
    // 边界投影可以保留完整可见历史，但不具备与原始行一一对应的坐标。
    Ok(None)
}

/// Persists only the changed session after a WebUI runtime mutation.
/// 在 WebUI 运行时变更后仅持久化被修改的单个会话。
///
/// # Parameters
/// 参数
/// `state` optionally supplies the authoritative retained workspace and invalidates older background writers.
/// `state` 可选提供权威保留工作区，并使更早的后台写入者失效。
/// `controller_runtime` owns the workspace-store persistence route.
/// `controller_runtime` 持有工作区存储持久化路由。
/// `workspace_snapshot` is the caller-owned fallback used only when no retained service state exists.
/// `workspace_snapshot` 是仅在不存在保留服务状态时使用的调用方候选回退。
/// `session_id` selects the exact regular session to persist.
/// `session_id` 选择需要持久化的精确普通会话。
///
/// # Returns
/// 返回值
/// Success after the newest authoritative session revision has crossed its session-keyed write channel.
/// 最新权威会话修订通过其会话键写入通道后返回成功。
pub(crate) async fn persist_workspace_session_update(
    state: Option<&SharedServiceBridgeState>,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    if let Some(state) = state {
        return persist_latest_retained_workspace_session_update(
            state,
            controller_runtime,
            session_id,
        )
        .await;
    }
    let persist_lock = webui_session_persist_lock(session_id);
    let _persist_guard = persist_lock.lock().await;
    persist_workspace_session_update_unlocked(controller_runtime, workspace_snapshot, session_id)
        .await
}

/// Persists the newest authoritative revision of one retained session without cloning its workspace.
/// 在不克隆工作区的情况下持久化一个保留会话的最新权威修订。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative retained workspace and invalidates older background writers.
/// `state` 持有权威保留工作区，并使较早的后台写入者失效。
/// `controller_runtime` owns the exact workspace-store persistence route.
/// `controller_runtime` 持有精确的工作区存储持久化路由。
/// `session_id` selects the only retained session captured after write admission.
/// `session_id` 选择取得写入许可后唯一捕获的保留会话。
///
/// # Returns
/// 返回值
/// Success after the newest exact session payload crosses its session-keyed write channel.
/// 最新精确会话载荷通过其会话键写入通道后返回成功。
pub(crate) async fn persist_latest_retained_workspace_session_update(
    state: &SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    invalidate_webui_session_persist_version(state, session_id)?;
    let _persist_guard = webui_session_persist_lock(session_id).lock_owned().await;
    persist_latest_retained_workspace_session_update_unlocked(state, controller_runtime, session_id)
        .await
}

/// Persists one retained session after the caller has acquired its session-keyed write channel.
/// 在调用方已经取得会话键写入通道后持久化一个保留会话。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative retained session borrowed for bounded runtime synchronization.
/// `state` 持有为有界运行态同步而借用的权威保留会话。
/// `controller_runtime` owns the exact data root and locator index.
/// `controller_runtime` 持有精确数据根与定位索引。
/// `session_id` selects the only retained session written by this transaction.
/// `session_id` 选择该事务唯一写入的保留会话。
///
/// # Returns
/// 返回值
/// Success after transcript, runtime metadata, and locator persistence complete.
/// 转录、运行态元数据与定位信息持久化完成后返回成功。
pub(crate) async fn persist_latest_retained_workspace_session_update_unlocked(
    state: &SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    // payload is captured after write admission and never clones canonical runtime history.
    // payload 在取得写入许可后捕获，且绝不克隆规范运行态历史。
    let payload = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        retained_workspace_session_persistence_payload(&guard, &controller_runtime, session_id)?
    };
    persist_retained_workspace_session_payload(controller_runtime, payload).await
}

/// Captures one retained session after synchronizing its borrowed bounded runtime tail.
/// 在同步借用的有界运行态尾部后捕获一个保留会话。
///
/// # Parameters
/// 参数
/// `guard` owns authoritative retained state, `controller_runtime` owns exact storage,
/// and `session_id` selects one regular session.
/// `guard` 持有权威保留状态，`controller_runtime` 持有精确存储，`session_id`
/// 选择一个普通会话。
///
/// # Returns
/// 返回值
/// A detached payload without canonical conversation history, or an exact locator or persistence error.
/// 返回不含规范会话历史的分离载荷，或精确定位与持久化错误。
fn retained_workspace_session_persistence_payload(
    guard: &ServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<RetainedWorkspaceSessionPersistencePayload, String> {
    let session_directory = require_segmented_session_directory_for_retained_session(
        guard,
        &controller_runtime.data_root,
        session_id,
    )?;
    let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
        String::from(
            "cannot persist a retained session without an authoritative workspace snapshot",
        )
    })?;
    let (session, archived) = workspace_session_with_archive_status(workspace_snapshot, session_id)
        .ok_or_else(|| format!("单会话保存失败：未找到会话：{session_id}"))?;
    let project_id = session_summary_project_id(session)
        .map(str::to_string)
        .ok_or_else(|| format!("单会话保存失败：会话未携带所属项目：{session_id}"))?;
    let session_messages = if archived {
        &workspace_snapshot.archived_session_messages
    } else {
        &workspace_snapshot.session_messages
    };
    let session_runtime_state = if archived {
        &workspace_snapshot.archived_session_runtime_state
    } else {
        &workspace_snapshot.session_runtime_state
    };
    let runtime_projection = match session_runtime_state.get(&session_id) {
        Some(runtime_state) => {
            // The append synchronizer reads constant metadata plus only the open active tail.
            // 追加同步器仅读取常量元数据以及尚未闭合的活动尾部。
            vulcan_workspace_store::workspace_store::sync_single_session_runtime_log_update(
                &session_directory,
                session_id,
                runtime_state,
            )?;
            Some(
                vulcan_workspace_store::workspace_store::
                    project_single_session_runtime_state_update(session_id, runtime_state)?,
            )
        }
        None => None,
    };
    let retained_messages = session_messages
        .get(&session_id)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let retained_window = guard.hydrated_conversation_windows.get(&session_id);
    let transcript = classify_retained_transcript_persistence(retained_messages, retained_window);
    Ok(RetainedWorkspaceSessionPersistencePayload {
        session_directory,
        project_id,
        session: session.clone(),
        archived,
        transcript,
        runtime_projection,
    })
}

/// Persists one detached retained-session payload without revisiting retained history.
/// 在不重新访问保留历史的情况下持久化一个分离的保留会话载荷。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the locator database and `payload` owns one exact history-free revision.
/// `controller_runtime` 持有定位数据库，`payload` 持有一个精确无历史修订。
///
/// # Returns
/// 返回值
/// Success after transcript, locator, and optional runtime metadata writes complete.
/// 转录、定位信息与可选运行态元数据写入完成后返回成功。
async fn persist_retained_workspace_session_payload(
    controller_runtime: ControllerRuntimeHandle,
    payload: RetainedWorkspaceSessionPersistencePayload,
) -> Result<(), String> {
    let RetainedWorkspaceSessionPersistencePayload {
        session_directory,
        project_id,
        session,
        archived,
        transcript,
        runtime_projection,
    } = payload;
    match transcript {
        RetainedTranscriptPersistence::Complete(messages) => {
            vulcan_workspace_store::workspace_store::persist_single_session_update(
                controller_runtime,
                &project_id,
                &session,
                archived,
                &messages,
                None,
            )
            .await?;
        }
        RetainedTranscriptPersistence::PartialWindow {
            message_start_index,
            messages,
        } => {
            persist_partial_retained_transcript_and_index(
                controller_runtime,
                &session_directory,
                &project_id,
                &session,
                archived,
                message_start_index,
                &messages,
            )
            .await?;
        }
        RetainedTranscriptPersistence::ExistingIndexOnly => {
            vulcan_workspace_store::workspace_store::persist_existing_session_index_update(
                controller_runtime,
                &project_id,
                &session,
                archived,
            )
            .await?;
        }
    }
    if let Some(runtime_projection) = runtime_projection.as_ref() {
        vulcan_workspace_store::workspace_store::
            persist_projected_single_session_runtime_state_update(
                session_directory,
                session.id,
                runtime_projection,
            )
            .await?;
    }
    Ok(())
}

/// Persists one partial retained transcript window after leaving the global service-state lock.
/// 在离开全局服务状态锁后持久化一个局部保留转录窗口。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the exact locator index update route.
/// `controller_runtime` 持有精确定位索引更新路由。
/// `session_directory` is the deterministic storage target for the exact session.
/// `session_directory` 是精确会话的确定性存储目标。
/// `project_id` identifies the existing locator row owner.
/// `project_id` 标识既有定位行的所属项目。
/// `session` is the exact retained session summary being persisted.
/// `session` 是正在持久化的精确保留会话摘要。
/// `archived` records which deterministic storage branch owns the session.
/// `archived` 记录该会话属于哪个确定性存储分支。
/// `message_start_index` is the absolute transcript coordinate represented by `messages[0]`.
/// `message_start_index` 是 `messages[0]` 所表示的绝对转录坐标。
/// `messages` is the bounded retained display window in ascending order.
/// `messages` 是按正序排列的有界保留展示窗口。
///
/// # Returns
/// 返回值
/// Success after any exact immutable suffix and the existing locator row are durable.
/// 精确不可变后缀（若存在）与既有定位行可靠落盘后返回成功。
async fn persist_partial_retained_transcript_and_index(
    controller_runtime: ControllerRuntimeHandle,
    session_directory: &Path,
    project_id: &str,
    session: &SessionSummary,
    archived: bool,
    message_start_index: Option<usize>,
    messages: &[SessionMessageEntry],
) -> Result<(), String> {
    // Metadata is read only after the global service-state lock has been released.
    // 元数据仅在全局服务状态锁释放后读取。
    let checkpoint = vulcan_workspace_store::workspace_store::
        load_single_session_transcript_checkpoint_in_directory(
            session_directory,
            session.id,
        )?;
    if let Some(tail) =
        retained_transcript_tail_for_checkpoint(messages, message_start_index, &checkpoint)?
    {
        vulcan_workspace_store::workspace_store::
            persist_single_session_transcript_tail_from_checkpoint_in_directory(
                session_directory,
                session.id,
                &checkpoint,
                &tail,
            )?;
    }
    vulcan_workspace_store::workspace_store::persist_existing_session_index_update(
        controller_runtime,
        project_id,
        session,
        archived,
    )
    .await
}

/// Persists one session snapshot after the caller has acquired the shared WebUI write gate.
/// 在调用方已取得共享 WebUI 写入闸门后持久化单个会话快照。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the workspace snapshot that contains the changed session.
/// `workspace_snapshot` 是包含变更会话的工作区快照。
/// `session_id` selects the session to persist.
/// `session_id` 选择要持久化的会话。
///
/// # Returns
/// 返回值
/// `Ok(())` when the session has been saved.
/// 会话保存完成时返回 `Ok(())`。
pub(crate) async fn persist_workspace_session_update_unlocked(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    let payload = workspace_session_persistence_payload(workspace_snapshot, session_id)?;
    persist_workspace_session_payload(controller_runtime, payload).await
}

/// Captures only the exact session fields required by the workspace-store persistence API.
/// 仅捕获 workspace-store 持久化 API 所需的精确会话字段。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the authoritative retained or transaction-local workspace revision.
/// `workspace_snapshot` 是权威保留或事务本地工作区修订。
/// `session_id` selects the exact regular session to detach.
/// `session_id` 选择需要分离的精确普通会话。
///
/// # Returns
/// 返回值
/// A single-session payload without unrelated session maps, or an explicit missing-session error.
/// 返回不包含无关会话映射的单会话载荷，或明确的会话缺失错误。
fn workspace_session_persistence_payload(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<WorkspaceSessionPersistencePayload, String> {
    let (session, archived) = workspace_session_with_archive_status(workspace_snapshot, session_id)
        .ok_or_else(|| format!("单会话保存失败：未找到会话：{session_id}"))?;
    let session_messages = if archived {
        &workspace_snapshot.archived_session_messages
    } else {
        &workspace_snapshot.session_messages
    };
    let session_runtime_state = if archived {
        &workspace_snapshot.archived_session_runtime_state
    } else {
        &workspace_snapshot.session_runtime_state
    };
    let runtime_state = session_runtime_state.get(&session_id).cloned();
    // messages remains the stored display slice because workspace-store owns the single canonical merge.
    // messages 保持为已存展示切片，因为 workspace-store 唯一负责规范合并。
    let messages = session_messages
        .get(&session_id)
        .cloned()
        .unwrap_or_default();
    let project_id = session_summary_project_id(session)
        .map(str::to_string)
        .ok_or_else(|| format!("单会话保存失败：会话未携带所属项目：{session_id}"))?;
    Ok(WorkspaceSessionPersistencePayload {
        project_id,
        session: session.clone(),
        archived,
        messages,
        runtime_state,
    })
}

/// Persists one already-scoped session payload after the caller has acquired its session write channel.
/// 在调用方取得会话写入通道后持久化一个已限定范围的会话载荷。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace-store route and `payload` owns one exact session revision.
/// `controller_runtime` 持有 workspace-store 路由，`payload` 持有一个精确会话修订。
///
/// # Returns
/// 返回值
/// Success after the single-session segmented persistence transaction completes.
/// 单会话分段持久化事务完成后返回成功。
async fn persist_workspace_session_payload(
    controller_runtime: ControllerRuntimeHandle,
    payload: WorkspaceSessionPersistencePayload,
) -> Result<(), String> {
    vulcan_workspace_store::workspace_store::persist_single_session_update(
        controller_runtime,
        &payload.project_id,
        &payload.session,
        payload.archived,
        &payload.messages,
        payload.runtime_state.as_ref(),
    )
    .await
}

/// Returns whether a WebUI snapshot owns the id as a regular workspace-store session.
/// 返回 WebUI 快照是否将该标识拥有为普通 workspace-store 会话。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained WebUI workspace snapshot.
/// `workspace_snapshot` 是当前保留的 WebUI 工作区快照。
/// `session_id` is the candidate session identifier.
/// `session_id` 是候选会话标识。
///
/// # Returns
/// 返回值
/// `true` when the id belongs to a normal active or archived session, not a persisted TaskSession.
/// 当该标识属于普通活动或归档会话、而不是持久化 TaskSession 时返回 `true`。
pub(crate) fn workspace_snapshot_contains_regular_session(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    workspace_session_with_archive_status(workspace_snapshot, session_id).is_some()
}

/// Returns one session summary and whether it comes from the archived list.
/// 返回单个会话摘要以及它是否来自归档列表。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained WebUI workspace snapshot.
/// `workspace_snapshot` 是保留的 WebUI 工作区快照。
/// `session_id` identifies the session to find in active or archived rows.
/// `session_id` 标识需要在活动或归档行中查找的会话。
///
/// # Returns
/// 返回值
/// The matched session summary and archived flag, or `None` when absent.
/// 返回匹配的会话摘要和归档标记；不存在时返回 `None`。
pub(crate) fn workspace_session_with_archive_status(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<(&SessionSummary, bool)> {
    workspace_snapshot
        .sessions
        .iter()
        .find(|candidate| candidate.id == session_id)
        .map(|session| (session, false))
        .or_else(|| {
            workspace_snapshot
                .archived_sessions
                .iter()
                .find(|candidate| candidate.id == session_id)
                .map(|session| (session, true))
        })
}

/// Returns whether the retained workspace snapshot contains one active project.
/// 返回保留工作区快照是否包含一个活动项目。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the current retained WebUI workspace snapshot.
/// `workspace_snapshot` 是当前保留的 WebUI 工作区快照。
/// `project_id` is the project id required for a new draft submission.
/// `project_id` 是新建草稿提交所需的项目标识。
///
/// # Returns
/// 返回值
/// `true` when the active project list contains the requested project id.
/// 当活动项目列表包含请求的项目标识时返回 `true`。
pub(crate) fn workspace_snapshot_contains_active_project(
    workspace_snapshot: &WorkspaceSnapshot,
    project_id: &str,
) -> bool {
    workspace_snapshot
        .task_projects
        .iter()
        .any(|project| project.id == project_id)
}

/// Spawns one session-versioned persistence task behind its deterministic session write channel.
/// 在确定性会话写入通道后启动一个按会话版本保护的持久化任务。
///
/// # Parameters
/// 参数
/// `state` owns the session persistence version table.
/// `state` 持有会话持久化版本表。
/// `session_id` selects the session whose write version is being checked.
/// `session_id` 选择需要检查写入版本的会话。
/// `version_error_label` is the diagnostic text used when version allocation fails.
/// `version_error_label` 是分配版本失败时使用的诊断文本。
/// `persist` performs the concrete persistence write after the version is still current.
/// `persist` 在版本仍为当前版本后执行具体持久化写入。
fn spawn_session_versioned_workspace_persist_task<F, Fut>(
    state: SharedServiceBridgeState,
    session_id: SessionId,
    version_error_label: &'static str,
    persist: F,
) where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    // session_persist_version fences only the ordinary session snapshot.
    // session_persist_version 仅隔离普通会话快照。
    let session_persist_version =
        match next_webui_session_runtime_persist_version(&state, session_id) {
            Ok(version) => version,
            Err(error) => {
                eprintln!("[webui-service] {version_error_label}: {error}");
                return;
            }
        };
    spawn_service_task(async move {
        if !webui_session_runtime_persist_version_is_current(
            &state,
            session_id,
            session_persist_version,
        ) {
            return;
        }
        let persist_lock = webui_session_persist_lock(session_id);
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_runtime_persist_version_is_current(
            &state,
            session_id,
            session_persist_version,
        ) {
            return;
        }
        persist().await;
    });
}

/// Persists one WebUI session snapshot in the background without blocking visible UI updates.
/// 在后台持久化一个 WebUI 会话快照，不阻塞可见 UI 更新。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `session_id` selects the session that changed.
/// `session_id` 选择发生变更的会话。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
pub(crate) fn spawn_workspace_session_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    reason: Option<String>,
) {
    // persistence_state is read only after version admission so the caller never clones a workspace.
    // persistence_state 仅在版本准入后读取，因此调用方绝不克隆工作区。
    let persistence_state = state.clone();
    spawn_session_versioned_workspace_persist_task(
        state,
        session_id,
        "background persist version failed",
        move || async move {
            if let Err(error) = persist_latest_retained_workspace_session_update_unlocked(
                &persistence_state,
                controller_runtime,
                session_id,
            )
            .await
            {
                let reason = reason.unwrap_or_else(|| String::from("session_update"));
                eprintln!("[webui-service] background persist failed ({reason}): {error}");
            }
        },
    );
}

/// Persists one canonical conversation checkpoint without cloning runtime or workspace history.
/// 在不克隆运行时历史或工作区历史的情况下持久化一个规范会话检查点。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative retained checkpoint, `controller_runtime` owns exact storage,
/// `session_id` selects the main session, and `reason` labels diagnostic failures.
/// `state` 持有权威保留检查点，`controller_runtime` 持有精确存储，
/// `session_id` 选择主会话，`reason` 标记诊断失败来源。
pub(crate) fn spawn_latest_conversation_checkpoint_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    reason: Option<String>,
) {
    let persistence_state = state.clone();
    spawn_session_versioned_workspace_persist_task(
        state,
        session_id,
        "conversation checkpoint persist version failed",
        move || async move {
            // persistence_snapshot contains only history-free state.json fields after the bounded log sync.
            // persistence_snapshot 在有界日志同步后仅包含无历史的 state.json 字段。
            let persistence_snapshot = match persistence_state.lock() {
                Ok(guard) => {
                    let session_directory =
                        match require_segmented_session_directory_for_retained_session(
                            &guard,
                            &controller_runtime.data_root,
                            session_id,
                        ) {
                            Ok(session_directory) => session_directory,
                            Err(error) => {
                                eprintln!(
                                    "[webui-service] checkpoint exact locator failed: session={session_id} error={error}"
                                );
                                return;
                            }
                        };
                    let Some(workspace) = guard.workspace_snapshot.as_ref() else {
                        return;
                    };
                    let Some(runtime_state) = workspace.session_runtime_state.get(&session_id)
                    else {
                        return;
                    };
                    let projection = match vulcan_workspace_store::workspace_store::
                        project_single_session_runtime_state_update(session_id, runtime_state)
                    {
                        Ok(projection) => projection,
                        Err(error) => {
                            eprintln!(
                                "[webui-service] checkpoint state projection failed: session={session_id} error={error}"
                            );
                            return;
                        }
                    };
                    // Runtime synchronization borrows the authoritative state and projects only its bounded active tail.
                    // 运行时同步借用权威状态，并且只投影其有界活动尾部。
                    let runtime_log_result = vulcan_workspace_store::workspace_store::
                        sync_single_session_runtime_log_update(
                            &session_directory,
                            session_id,
                            runtime_state,
                        );
                    Some((session_directory, projection, runtime_log_result))
                }
                Err(error) => {
                    eprintln!(
                        "[webui-service] checkpoint retained-state lock failed: session={session_id} error={error}"
                    );
                    return;
                }
            };
            let Some((session_directory, projection, runtime_log_result)) = persistence_snapshot
            else {
                return;
            };
            let reason_label = reason.as_deref().unwrap_or("conversation_checkpoint");
            if let Err(error) = runtime_log_result {
                eprintln!(
                    "[webui-service] checkpoint bounded runtime-log persist failed ({reason_label}): {error}"
                );
            }
            if let Err(error) = vulcan_workspace_store::workspace_store::
                persist_projected_single_session_runtime_state_update(
                    session_directory,
                    session_id,
                    &projection,
                )
                .await
            {
                eprintln!(
                    "[webui-service] checkpoint runtime-state persist failed ({reason_label}): {error}"
                );
            }
        },
    );
}

/// Persists only one WebUI session runtime-state document in the background.
/// 在后台仅持久化一个 WebUI 会话运行态文档。
///
/// # Parameters
/// 参数
/// `state` owns the retained service state and per-session write versions.
/// `state` 持有保留服务状态与按会话划分的写入版本。
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `session_id` selects the session runtime-state document to persist.
/// `session_id` 选择需要持久化的会话运行态文档。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
pub(crate) fn spawn_workspace_session_runtime_state_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    reason: Option<String>,
) {
    spawn_latest_session_runtime_state_persist_task(state, controller_runtime, session_id, reason);
}

/// Persists the newest history-free runtime metadata for one retained session synchronously.
/// 同步持久化一个保留会话的最新无历史运行态元数据。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative runtime entry and invalidates older runtime-only writers.
/// `state` 持有权威运行态条目，并使较早的仅运行态写入者失效。
/// `controller_runtime` owns exact session storage.
/// `controller_runtime` 持有精确会话存储。
/// `session_id` selects the only runtime metadata document to replace.
/// `session_id` 选择唯一需要替换的运行态元数据文档。
///
/// # Returns
/// 返回值
/// Success after a projection produced under the retained-state lock is durably written.
/// 在保留状态锁下生成的投影可靠写入后返回成功。
pub(crate) async fn persist_latest_retained_session_runtime_state_update(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    invalidate_webui_session_runtime_only_persist_version(state, session_id)?;
    let persist_lock = webui_session_persist_lock(session_id);
    let _persist_guard = persist_lock.lock().await;
    let (session_directory, projection) =
        {
            let guard = state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))?;
            let session_directory = require_segmented_session_directory_for_retained_session(
                &guard,
                &controller_runtime.data_root,
                session_id,
            )?;
            let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
                String::from("cannot persist runtime state before workspace hydration completes")
            })?;
            let runtime_state = workspace
                .session_runtime_state
                .get(&session_id)
                .or_else(|| workspace.archived_session_runtime_state.get(&session_id))
                .ok_or_else(|| format!("Session `{session_id}` has no retained runtime state."))?;
            let projection = vulcan_workspace_store::workspace_store::
            project_single_session_runtime_state_update(session_id, runtime_state)?;
            (session_directory, projection)
        };
    vulcan_workspace_store::workspace_store::persist_projected_single_session_runtime_state_update(
        session_directory,
        session_id,
        &projection,
    )
    .await
}

/// Persists the latest retained runtime state without cloning a complete workspace snapshot.
/// 在不克隆完整工作区快照的情况下持久化最新保留运行态。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative retained runtime entry, `controller_runtime` owns storage,
/// `session_id` selects the exact owner, and `reason` labels diagnostic failures.
/// `state` 持有权威保留运行态，`controller_runtime` 持有存储，`session_id` 选择精确归属，
/// `reason` 标记诊断失败来源。
pub(crate) fn spawn_latest_session_runtime_state_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    reason: Option<String>,
) {
    if let Err(error) = invalidate_webui_session_runtime_only_persist_version(&state, session_id) {
        eprintln!(
            "[webui-service] latest runtime-state persist fence failed: session={session_id} error={error}"
        );
        return;
    }
    spawn_service_task(async move {
        let persist_lock = webui_session_persist_lock(session_id);
        let _persist_guard = persist_lock.lock().await;
        let persistence_snapshot = match state.lock() {
            Ok(guard) => {
                // session_directory is captured from the same retained snapshot as runtime_state.
                // session_directory 与 runtime_state 从同一份保留快照中捕获。
                let session_directory =
                    match require_segmented_session_directory_for_retained_session(
                        &guard,
                        &controller_runtime.data_root,
                        session_id,
                    ) {
                        Ok(session_directory) => session_directory,
                        Err(error) => {
                            eprintln!(
                                "[webui-service] latest runtime-state exact locator failed: session={session_id} error={error}"
                            );
                            return;
                        }
                    };
                let Some(workspace) = guard.workspace_snapshot.as_ref() else {
                    return;
                };
                let Some(runtime_state) = workspace
                    .session_runtime_state
                    .get(&session_id)
                    .or_else(|| workspace.archived_session_runtime_state.get(&session_id))
                else {
                    return;
                };
                // projection serializes only state.json fields while the authoritative entry is borrowed.
                // projection 在借用权威条目期间只序列化 state.json 字段。
                match vulcan_workspace_store::workspace_store::
                    project_single_session_runtime_state_update(session_id, runtime_state)
                {
                    Ok(projection) => Some((session_directory, projection)),
                    Err(error) => {
                        eprintln!(
                            "[webui-service] latest runtime-state projection failed: session={session_id} error={error}"
                        );
                        return;
                    }
                }
            }
            Err(error) => {
                eprintln!(
                    "[webui-service] latest runtime-state snapshot lock failed: session={session_id} error={error}"
                );
                return;
            }
        };
        let Some((session_directory, projection)) = persistence_snapshot else {
            return;
        };
        if let Err(error) = vulcan_workspace_store::workspace_store::
            persist_projected_single_session_runtime_state_update(
                session_directory,
                session_id,
                &projection,
            )
            .await
        {
            let reason_label = reason.as_deref().unwrap_or("latest_runtime_state");
            eprintln!(
                "[webui-service] background latest runtime-state persist failed ({reason_label}): {error}"
            );
        }
    });
}

/// Returns the active transcript writer registry guard for one exact-key operation.
/// 返回用于一次精确键操作的活动转录写入器注册表守卫。
///
/// # Returns
/// 返回值
/// The poison-recovered process-local writer registry guard.
/// 返回从中毒状态恢复的进程内写入器注册表守卫。
fn session_transcript_writer_registry()
-> StdMutexGuard<'static, BTreeMap<SessionId, Arc<StdMutex<SessionTranscriptWriterCursor>>>> {
    // registry stores active cursors only; callers always perform exact session-id lookup.
    // registry 仅保存活动游标；调用方始终执行精确会话标识查询。
    let registry = SESSION_TRANSCRIPT_WRITERS.get_or_init(|| StdMutex::new(BTreeMap::new()));
    registry
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Returns the shutdown lifecycle counter and condition variable.
/// 返回关闭生命周期计数器与条件变量。
///
/// # Returns
/// 返回值
/// The stable process-local pending-command lifecycle tuple.
/// 返回稳定的进程内待处理命令生命周期元组。
fn transcript_writer_pending_lifecycle() -> &'static (StdMutex<usize>, Condvar) {
    SESSION_TRANSCRIPT_WRITER_PENDING.get_or_init(|| (StdMutex::new(0), Condvar::new()))
}

/// Returns the process-local writer admission and shutdown-seal mutex.
/// 返回进程内写入器准入与关闭封印互斥锁。
///
/// # Returns
/// 返回值
/// The stable mutex serializing only admission publication and shutdown sealing.
/// 返回仅串行化准入发布与关闭封印的稳定互斥锁。
fn transcript_writer_admission_lock() -> &'static StdMutex<()> {
    SESSION_TRANSCRIPT_WRITER_ADMISSION.get_or_init(|| StdMutex::new(()))
}

/// Gets or creates the exact root-session cursor without touching unrelated entries.
/// 获取或创建精确根会话游标，且不接触无关表项。
///
/// # Parameters
/// 参数
/// `session_id` is the exact registry key.
/// `session_id` 是精确注册表键。
/// `session_directory` is the deterministic retained locator target.
/// `session_directory` 是保留定位信息指向的确定性目标。
///
/// # Returns
/// 返回值
/// The shared cursor for this active session, or an ownership mismatch error.
/// 返回当前活动会话的共享游标，或归属不匹配错误。
fn session_transcript_writer_cursor(
    session_id: SessionId,
    session_directory: &Path,
) -> Result<Arc<StdMutex<SessionTranscriptWriterCursor>>, String> {
    // registry_guard protects only this exact lookup or insertion.
    // registry_guard 仅保护当前精确查询或插入。
    let mut registry_guard = session_transcript_writer_registry();
    if let Some(cursor) = registry_guard.get(&session_id) {
        let cursor_guard = cursor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cursor_guard.session_directory != session_directory {
            return Err(format!(
                "Transcript writer session `{session_id}` is already bound to `{}`, not `{}`.",
                cursor_guard.session_directory.display(),
                session_directory.display()
            ));
        }
        drop(cursor_guard);
        return Ok(Arc::clone(cursor));
    }
    // cursor is cold until the first command initializes both boundaries from exact metadata.
    // cursor 在首条命令从精确元数据初始化两个边界之前保持冷态。
    let cursor = Arc::new(StdMutex::new(SessionTranscriptWriterCursor {
        session_directory: session_directory.to_path_buf(),
        append_cursor: None,
        committed_checkpoint: None,
        durable_checkpoint: None,
        queue_depth: 0,
        retry_commands: VecDeque::new(),
        blocked_error: None,
        notify: Arc::new(tokio::sync::Notify::new()),
    }));
    registry_guard.insert(session_id, Arc::clone(&cursor));
    Ok(cursor)
}

/// Prepares one immutable suffix command and advances only the in-memory committed cursor.
/// 准备一条不可变后缀命令，并且只推进内存已提交游标。
///
/// # Parameters
/// 参数
/// `state` owns the exact retained display window used to select the suffix.
/// `state` 持有用于选择后缀的精确保留展示窗口。
/// `session_id` selects the root session writer.
/// `session_id` 选择根会话写入器。
/// `session_directory` is the exact deterministic storage target.
/// `session_directory` 是精确确定性的存储目标。
///
/// # Returns
/// 返回值
/// One ordered command per stable semantic row, an empty vector for no suffix, or an explicit error.
/// 每个稳定语义行返回一条有序命令；没有后缀时返回空向量；否则返回明确错误。
fn prepare_session_transcript_writer_commands(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    session_directory: &Path,
    exact_tail_messages: Option<Vec<SessionMessageEntry>>,
) -> Result<Vec<PreparedSessionTranscriptWriterCommand>, String> {
    // admission_guard prevents shutdown from sealing between the flag check and pending publication.
    // admission_guard 防止关闭流程在标志检查与待处理发布之间设置封印。
    let _admission_guard = transcript_writer_admission_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if SESSION_TRANSCRIPT_WRITER_SHUTTING_DOWN.load(Ordering::Acquire) {
        return Err(String::from(
            "Transcript writer admission is closed because application shutdown is draining durable records.",
        ));
    }
    if exact_tail_messages.as_ref().is_some_and(Vec::is_empty) {
        return Ok(Vec::new());
    }
    let cursor = session_transcript_writer_cursor(session_id, session_directory)?;
    // cursor_guard serializes command admission and preserves canonical event order.
    // cursor_guard 串行化命令接纳并保持规范事件顺序。
    let mut cursor_guard = cursor
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // A physical failure blocks dependent writes, but later stable rows must still reserve their
    // immutable retry positions. Rejecting them here would let shutdown replay the first failure
    // successfully while silently losing rows that became visible after that failure.
    // 物理失败会阻止依赖写入继续执行，但后续稳定记录仍必须预留其不可变重试位置。若在此拒绝，
    // 关闭流程可能成功重放首条失败记录，却静默丢失该失败之后已经对用户可见的记录。
    if cursor_guard.committed_checkpoint.is_none() {
        // append_cursor performs the only metadata read for this active writer generation.
        // append_cursor 执行当前活动写入器代次唯一一次元数据读取。
        let (append_cursor, checkpoint) = vulcan_workspace_store::workspace_store::
            open_single_session_transcript_append_cursor_in_directory(
                session_directory,
                session_id,
            )?;
        cursor_guard.append_cursor = Some(Arc::new(StdMutex::new(append_cursor)));
        cursor_guard.committed_checkpoint = Some(checkpoint.clone());
        cursor_guard.durable_checkpoint = Some(checkpoint);
    }
    let expected_checkpoint = cursor_guard
        .committed_checkpoint
        .clone()
        .expect("writer initialization installs the committed checkpoint");
    // tail_messages clones only rows after the writer's in-memory committed boundary.
    // tail_messages 仅克隆写入器内存已提交边界之后的行。
    let tail_messages = if let Some(exact_tail_messages) = exact_tail_messages {
        // Event-local rows already own their exact mutation boundary and never depend on UI pagination coordinates.
        // 事件局部记录已经持有精确变更边界，绝不依赖界面分页坐标。
        Some(exact_tail_messages)
    } else {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("Workspace snapshot is unavailable for transcript write.")
        })?;
        let messages = workspace.session_messages.get(&session_id).ok_or_else(|| {
            format!("Session `{session_id}` has no retained transcript for writer admission.")
        })?;
        let Some(retained_window) = guard
            .hydrated_conversation_windows
            .get(&session_id)
            .filter(|window| window.transcript_window_loaded)
        else {
            return Ok(Vec::new());
        };
        retained_transcript_tail_for_checkpoint(
            messages,
            retained_window.message_start_index,
            &expected_checkpoint,
        )?
    };
    let Some(tail_messages) = tail_messages else {
        return Ok(Vec::new());
    };
    // initial_message_count anchors diagnostics before per-event command admission advances the cursor.
    // initial_message_count 在逐事件命令准入推进游标前锚定诊断起点。
    let initial_message_count = expected_checkpoint.message_count;
    // next_expected_checkpoint advances exactly once per immutable semantic row.
    // next_expected_checkpoint 对每个不可变稳定语义行精确推进一次。
    let mut next_expected_checkpoint = expected_checkpoint;
    // commands preserve one durable boundary per message and avoid partial multi-row append ambiguity.
    // commands 为每条消息保留一个可靠边界，避免多行部分追加的歧义。
    let mut commands = Vec::with_capacity(tail_messages.len());
    for tail_message in tail_messages {
        // single_message_tail transfers ownership without cloning the message body.
        // single_message_tail 在不复制消息正文的情况下转移所有权。
        let single_message_tail = vec![tail_message];
        // committed_checkpoint is the deterministic boundary after exactly this semantic row.
        // committed_checkpoint 是恰好完成当前稳定语义行后的确定性边界。
        let committed_checkpoint =
            vulcan_workspace_store::workspace_store::advance_single_session_transcript_checkpoint(
                &next_expected_checkpoint,
                &single_message_tail,
            )?;
        // retry retains this single immutable row until durable acknowledgement.
        // retry 在可靠确认前保留当前单条不可变记录。
        let retry = Arc::new(SessionTranscriptWriterRetryCommand {
            session_directory: session_directory.to_path_buf(),
            expected_checkpoint: next_expected_checkpoint.clone(),
            committed_checkpoint: committed_checkpoint.clone(),
            tail_messages: Arc::new(single_message_tail),
        });
        commands.push(SessionTranscriptWriterCommand {
            cursor: Arc::clone(&cursor),
            retry,
        });
        next_expected_checkpoint = committed_checkpoint;
    }
    // retry_commands become visible atomically only after every command has been derived successfully.
    // retry_commands 仅在全部命令成功派生后一次性变为可见。
    cursor_guard
        .retry_commands
        .extend(commands.iter().map(|command| Arc::clone(&command.retry)));
    cursor_guard.committed_checkpoint = Some(next_expected_checkpoint.clone());
    cursor_guard.queue_depth = cursor_guard.queue_depth.saturating_add(commands.len());
    let queue_depth = cursor_guard.queue_depth;
    drop(cursor_guard);
    webui_trace(
        "session_transcript_writer_enqueue",
        format!(
            "session={session_id} writer_queue_depth={queue_depth} expected_message_count={} committed_message_count={} tail_messages={}",
            initial_message_count,
            next_expected_checkpoint.message_count,
            commands.len()
        ),
    );
    // prepared_commands publish every pending token before the admission mutex is released.
    // prepared_commands 在释放准入互斥锁前发布每一个待处理令牌。
    let prepared_commands = commands
        .into_iter()
        .map(|command| PreparedSessionTranscriptWriterCommand {
            command,
            pending_command: PendingTranscriptWriterCommand::begin(),
        })
        .collect();
    Ok(prepared_commands)
}

/// Completes one writer command and wakes exact-session durability waiters.
/// 完成一条写入器命令并唤醒精确会话可靠性等待者。
///
/// # Parameters
/// 参数
/// `command` owns the expected and committed boundaries of one append.
/// `command` 持有一次追加的预期边界与已提交边界。
/// `result` is the physical append outcome.
/// `result` 是物理追加结果。
///
/// # Returns
/// 返回值
/// The same append outcome after the in-memory durable or blocked state is published.
/// 返回在发布内存可靠或阻塞状态后的同一个追加结果。
fn complete_session_transcript_writer_command(
    command: &SessionTranscriptWriterCommand,
    result: Result<(), String>,
) -> Result<(), String> {
    let mut cursor_guard = command
        .cursor
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cursor_guard.queue_depth = cursor_guard.queue_depth.saturating_sub(1);
    match result {
        Ok(()) => {
            if cursor_guard.durable_checkpoint.as_ref() != Some(&command.retry.expected_checkpoint)
            {
                let error = format!(
                    "Transcript writer durable boundary changed before ordered command completion for `{}`.",
                    command.retry.session_directory.display()
                );
                cursor_guard.blocked_error = Some(error.clone());
                cursor_guard.notify.notify_waiters();
                return Err(error);
            }
            let owns_retry_front = cursor_guard
                .retry_commands
                .front()
                .is_some_and(|retry| Arc::ptr_eq(retry, &command.retry));
            if !owns_retry_front {
                let error = format!(
                    "Transcript writer completed a command outside its retained retry order for `{}`.",
                    command.retry.session_directory.display()
                );
                cursor_guard.blocked_error = Some(error.clone());
                cursor_guard.notify.notify_waiters();
                return Err(error);
            }
            cursor_guard.retry_commands.pop_front();
            cursor_guard.durable_checkpoint = Some(command.retry.committed_checkpoint.clone());
            cursor_guard.notify.notify_waiters();
            Ok(())
        }
        Err(error) => {
            cursor_guard.blocked_error = Some(error.clone());
            cursor_guard.notify.notify_waiters();
            Err(error)
        }
    }
}

/// Waits until the durable cursor reaches the exact predecessor required by one command.
/// 等待可靠游标到达一条命令所要求的精确前驱边界。
///
/// # Parameters
/// 参数
/// `command` owns the exact predecessor checkpoint reserved during admission.
/// `command` 持有接纳期间预留的精确前驱检查点。
///
/// # Returns
/// 返回值
/// Success when this command owns the next write turn, or the first blocking error.
/// 当前命令获得下一写入轮次时返回成功，否则返回首个阻塞错误。
async fn await_session_transcript_writer_command_turn(
    command: &SessionTranscriptWriterCommand,
) -> Result<(), String> {
    loop {
        let notify = {
            let cursor_guard = command
                .cursor
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(error) = cursor_guard.blocked_error.as_ref() {
                return Err(error.clone());
            }
            if cursor_guard.durable_checkpoint.as_ref() == Some(&command.retry.expected_checkpoint)
            {
                return Ok(());
            }
            Arc::clone(&cursor_guard.notify)
        };
        let notified = notify.notified();
        {
            let cursor_guard = command
                .cursor
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(error) = cursor_guard.blocked_error.as_ref() {
                return Err(error.clone());
            }
            if cursor_guard.durable_checkpoint.as_ref() == Some(&command.retry.expected_checkpoint)
            {
                return Ok(());
            }
        }
        notified.await;
    }
}

/// Waits until one exact session writer reaches its committed frontier or becomes blocked.
/// 等待一个精确会话写入器追上其已提交前沿或进入阻塞状态。
///
/// # Parameters
/// 参数
/// `session_id` selects the active writer without scanning the registry.
/// `session_id` 通过精确查询选择活动写入器，不扫描注册表。
///
/// # Returns
/// 返回值
/// Success after durable equals committed, or the exact persistence-blocked error.
/// 在可靠边界等于已提交边界后返回成功，否则返回精确持久化阻塞错误。
pub(crate) async fn await_session_transcript_writer_durable(
    session_id: SessionId,
) -> Result<(), String> {
    let cursor = session_transcript_writer_registry()
        .get(&session_id)
        .cloned();
    let Some(cursor) = cursor else {
        return Ok(());
    };
    loop {
        let notify = {
            let cursor_guard = cursor
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(error) = cursor_guard.blocked_error.as_ref() {
                return Err(error.clone());
            }
            if cursor_guard.queue_depth == 0
                && cursor_guard.durable_checkpoint == cursor_guard.committed_checkpoint
            {
                return Ok(());
            }
            Arc::clone(&cursor_guard.notify)
        };
        // notified is created before the second state observation to avoid a lost wakeup.
        // notified 在第二次状态观察前创建，以避免丢失唤醒。
        let notified = notify.notified();
        {
            let cursor_guard = cursor
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(error) = cursor_guard.blocked_error.as_ref() {
                return Err(error.clone());
            }
            if cursor_guard.queue_depth == 0
                && cursor_guard.durable_checkpoint == cursor_guard.committed_checkpoint
            {
                return Ok(());
            }
        }
        notified.await;
    }
}

/// Waits synchronously for every accepted writer command before application shutdown continues.
/// 在应用关闭继续之前，同步等待全部已接纳写入命令完成。
///
/// # Returns
/// 返回值
/// Success when the pending count reaches zero, or a bounded drain timeout error.
/// 待处理计数归零时返回成功，否则返回有界排空超时错误。
pub(crate) fn drain_all_session_transcript_writers_for_shutdown() -> Result<(), String> {
    {
        // admission_guard seals new commands atomically after every prior command published its pending token.
        // admission_guard 在全部既有命令发布待处理令牌后原子封印新命令。
        let _admission_guard = transcript_writer_admission_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        SESSION_TRANSCRIPT_WRITER_SHUTTING_DOWN.store(true, Ordering::Release);
    }
    let lifecycle = transcript_writer_pending_lifecycle();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut pending_count = lifecycle
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    while *pending_count > 0 {
        let now = std::time::Instant::now();
        if now >= deadline {
            return Err(format!(
                "Timed out draining {pending_count} transcript writer command(s) before shutdown."
            ));
        }
        let remaining = deadline.saturating_duration_since(now);
        let wait_result = lifecycle
            .1
            .wait_timeout(pending_count, remaining)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pending_count = wait_result.0;
    }
    drop(pending_count);
    // retry_targets are the finite resident writers already known in memory; no filesystem path is scanned.
    // retry_targets 是内存中已经明确存在的有限常驻写入器；不会扫描任何文件系统路径。
    let retry_targets = session_transcript_writer_registry()
        .iter()
        .map(|(session_id, cursor)| (*session_id, Arc::clone(cursor)))
        .collect::<Vec<_>>();
    for (session_id, cursor) in retry_targets {
        retry_retained_session_transcript_writer_commands(session_id, &cursor)?;
    }
    // registry_guard captures the final exact set only after every accepted command has completed.
    // registry_guard 仅在所有已接纳命令完成后捕获最终精确集合。
    let registry_guard = session_transcript_writer_registry();
    validate_drained_session_transcript_writer_registry(&registry_guard)
}

/// Replays retained failed commands in their exact canonical order during a shutdown retry.
/// 在关闭重试期间按精确规范顺序重放保留的失败命令。
///
/// # Parameters
/// 参数
/// `session_id` identifies the exact resident writer for diagnostics.
/// `session_id` 标识用于诊断的精确常驻写入器。
/// `cursor` owns the durable frontier and retained immutable suffixes.
/// `cursor` 持有可靠前沿与保留的不可变后缀。
///
/// # Returns
/// 返回值
/// Success when every retained suffix becomes durable, or the first exact retry failure.
/// 当每个保留后缀均变为可靠状态时返回成功，否则返回首个精确重试失败。
fn retry_retained_session_transcript_writer_commands(
    session_id: SessionId,
    cursor: &Arc<StdMutex<SessionTranscriptWriterCursor>>,
) -> Result<(), String> {
    // append_cursor is resolved exactly once from the resident writer; no metadata is rehydrated.
    // append_cursor 从常驻写入器精确解析一次；不会重新水合元数据。
    let append_cursor = {
        let cursor_guard = cursor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cursor_guard.retry_commands.is_empty() {
            return Ok(());
        }
        cursor_guard.append_cursor.as_ref().cloned().ok_or_else(|| {
            format!(
                "Transcript writer shutdown retry for session `{session_id}` has no resident append cursor."
            )
        })?
    };
    retry_retained_session_transcript_writer_commands_with(session_id, cursor, move |retry| {
        let mut append_cursor_guard = append_cursor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        vulcan_workspace_store::workspace_store::retry_single_session_transcript_tail_with_cursor(
            &mut append_cursor_guard,
            &retry.expected_checkpoint,
            retry.tail_messages.as_slice(),
        )
    })
}

/// Replays retained commands using one exact physical persistence operation.
/// 使用一个精确物理持久化操作重放保留命令。
///
/// # Parameters
/// 参数
/// `session_id` and `cursor` identify the exact ordered writer state.
/// `session_id` 与 `cursor` 标识精确的有序写入器状态。
/// `persist` durably appends one immutable retry payload and returns its observed checkpoint.
/// `persist` 可靠追加一个不可变重试载荷并返回其观测检查点。
///
/// # Returns
/// 返回值
/// Success after the retained queue is empty, or the first ordering or persistence failure.
/// 保留队列清空后返回成功，否则返回首个顺序或持久化失败。
fn retry_retained_session_transcript_writer_commands_with<Persist>(
    session_id: SessionId,
    cursor: &Arc<StdMutex<SessionTranscriptWriterCursor>>,
    mut persist: Persist,
) -> Result<(), String>
where
    Persist: FnMut(
        &SessionTranscriptWriterRetryCommand,
    ) -> Result<
        vulcan_workspace_store::workspace_store::PersistedSessionTranscriptCheckpoint,
        String,
    >,
{
    loop {
        // retry is cloned by Arc only; potentially large message bodies remain single-copy.
        // retry 仅复制 Arc；可能很大的消息正文仍保持单份。
        let retry = {
            let mut cursor_guard = cursor
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(retry) = cursor_guard.retry_commands.front().cloned() else {
                cursor_guard.blocked_error = None;
                cursor_guard.notify.notify_waiters();
                return Ok(());
            };
            if cursor_guard.durable_checkpoint.as_ref() != Some(&retry.expected_checkpoint) {
                let error = format!(
                    "Transcript writer shutdown retry for session `{session_id}` expected durable checkpoint {:?} but found {:?}.",
                    retry.expected_checkpoint, cursor_guard.durable_checkpoint
                );
                cursor_guard.blocked_error = Some(error.clone());
                cursor_guard.notify.notify_waiters();
                return Err(error);
            }
            retry
        };
        // physical_result retries only the exact retained suffix against its resident append cursor.
        // physical_result 仅针对其常驻追加游标重试精确保留后缀。
        let physical_result = persist(&retry);
        let mut cursor_guard = cursor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match physical_result {
            Ok(durable_checkpoint) if durable_checkpoint == retry.committed_checkpoint => {
                let owns_retry_front = cursor_guard
                    .retry_commands
                    .front()
                    .is_some_and(|front| Arc::ptr_eq(front, &retry));
                if !owns_retry_front {
                    let error = format!(
                        "Transcript writer shutdown retry order changed for session `{session_id}`."
                    );
                    cursor_guard.blocked_error = Some(error.clone());
                    cursor_guard.notify.notify_waiters();
                    return Err(error);
                }
                cursor_guard.retry_commands.pop_front();
                cursor_guard.durable_checkpoint = Some(durable_checkpoint);
                cursor_guard.blocked_error = None;
                cursor_guard.notify.notify_waiters();
            }
            Ok(durable_checkpoint) => {
                let error = format!(
                    "Transcript writer shutdown retry checkpoint mismatch for session `{session_id}`: expected={:?} actual={durable_checkpoint:?}.",
                    retry.committed_checkpoint
                );
                cursor_guard.blocked_error = Some(error.clone());
                cursor_guard.notify.notify_waiters();
                return Err(error);
            }
            Err(error) => {
                let error = format!(
                    "Transcript writer shutdown retry failed for session `{session_id}`: {error}"
                );
                cursor_guard.blocked_error = Some(error.clone());
                cursor_guard.notify.notify_waiters();
                return Err(error);
            }
        }
    }
}

/// Reopens writer admission after the desktop user cancels a failed shutdown attempt.
/// 在桌面用户取消失败的关闭尝试后重新开放写入器准入。
///
/// # Returns
/// 返回值
/// This operation has no failure mode and changes only the process-local admission seal.
/// 此操作不会失败，并且只修改进程内准入封印。
pub(crate) fn resume_session_transcript_writer_admission_after_failed_shutdown() {
    // admission_guard prevents reopening from racing a concurrent second shutdown seal.
    // admission_guard 防止重新开放与并发第二次关闭封印发生竞争。
    let _admission_guard = transcript_writer_admission_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    SESSION_TRANSCRIPT_WRITER_SHUTTING_DOWN.store(false, Ordering::Release);
}

/// Validates that every retained writer reached its durable frontier without a hidden failure.
/// 验证每个保留写入器均已到达耐久前沿且不存在隐藏失败。
///
/// # Parameters
/// 参数
/// `registry` is the shutdown-frozen in-memory writer registry after pending commands reach zero.
/// `registry` 是待处理命令归零后由关闭流程冻结的内存写入器注册表。
///
/// # Returns
/// 返回值
/// Success only when all writers are idle, durable, and unblocked; otherwise the first exact failure.
/// 仅当全部写入器空闲、耐久且未阻塞时返回成功，否则返回首个精确失败。
fn validate_drained_session_transcript_writer_registry(
    registry: &BTreeMap<SessionId, Arc<StdMutex<SessionTranscriptWriterCursor>>>,
) -> Result<(), String> {
    for (session_id, cursor) in registry {
        // cursor_guard reads one exact writer's final frontier under its own sharded lock.
        // cursor_guard 在其独立分片锁下读取一个精确写入器的最终前沿。
        let cursor_guard = cursor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(error) = cursor_guard.blocked_error.as_ref() {
            return Err(format!(
                "Transcript writer for session `{session_id}` could not drain durably: {error}"
            ));
        }
        if cursor_guard.queue_depth != 0 {
            return Err(format!(
                "Transcript writer for session `{session_id}` retained {} queued command(s) after shutdown drain.",
                cursor_guard.queue_depth
            ));
        }
        if cursor_guard.committed_checkpoint != cursor_guard.durable_checkpoint {
            return Err(format!(
                "Transcript writer for session `{session_id}` stopped before its committed frontier became durable."
            ));
        }
        if !cursor_guard.retry_commands.is_empty() {
            return Err(format!(
                "Transcript writer for session `{session_id}` retained {} retry command(s) after shutdown drain.",
                cursor_guard.retry_commands.len()
            ));
        }
    }
    Ok(())
}

/// Releases one drained writer cursor after authoritative terminal persistence succeeds.
/// 在权威终态持久化成功后释放一个已经排空的写入器游标。
///
/// # Parameters
/// 参数
/// `session_id` is the exact registry key to consider for removal.
/// `session_id` 是待考虑移除的精确注册表键。
///
/// # Returns
/// 返回值
/// `true` when the exact drained cursor was removed. A prior blocked state is safe to discard
/// because the caller proves that the authoritative terminal snapshot has superseded it.
/// 精确且已经排空的游标被移除时返回 `true`。由于调用方证明权威终态快照已经取代先前状态，
/// 因此可以安全丢弃此前的阻塞状态。
pub(crate) fn release_session_transcript_writer_after_terminal_persist(
    session_id: SessionId,
) -> bool {
    let mut registry_guard = session_transcript_writer_registry();
    let Some(cursor) = registry_guard.get(&session_id).cloned() else {
        return false;
    };
    let cursor_guard = cursor
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let removable = cursor_guard.queue_depth == 0;
    drop(cursor_guard);
    if removable
        && registry_guard
            .get(&session_id)
            .is_some_and(|registered| Arc::ptr_eq(registered, &cursor))
    {
        registry_guard.remove(&session_id);
        return true;
    }
    false
}

/// Spawns a background transcript write from an already-scoped session message snapshot.
/// 从已经限定到单个会话的消息快照启动后台转录写入。
///
/// # Parameters
/// 参数
/// `state` owns the live transcript persistence version table.
/// `state` 持有运行中转录持久化版本表。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `session_id` selects the session transcript to persist.
/// `session_id` 选择要持久化的会话转录。
/// `reason` is an optional diagnostic label for logs.
/// `reason` 是用于日志的可选诊断标签。
pub(crate) fn spawn_session_transcript_messages_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    reason: Option<String>,
) {
    // reasonLabel remains available if synchronous writer admission rejects the legacy retained-window request.
    // reasonLabel 会在同步写入器准入拒绝旧版保留窗口请求时继续用于诊断。
    let reason_label = reason
        .clone()
        .unwrap_or_else(|| String::from("live_transcript"));
    if let Err(error) =
        spawn_session_transcript_persist_task(state, controller_runtime, session_id, None, reason)
    {
        eprintln!(
            "[webui-service] transcript writer admission failed ({reason_label}): session={session_id} error={error}"
        );
    }
}

/// Spawns ordered persistence for exact event-local transcript rows.
/// 为精确的事件局部转录记录启动有序持久化。
///
/// # Parameters
/// 参数
/// `state` owns the exact retained session locator and writer registry context.
/// `state` 持有精确保留会话定位信息与写入器注册上下文。
/// `controller_runtime` provides the deterministic session data root.
/// `controller_runtime` 提供确定性的会话数据根目录。
/// `session_id` identifies the sole transcript writer receiving these rows.
/// `session_id` 标识接收这些记录的唯一转录写入器。
/// `exact_tail_messages` contains only rows changed by the current semantic event.
/// `exact_tail_messages` 仅包含当前语义事件修改的记录。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
///
/// # Returns
/// 返回值
/// Success after every exact row has reserved its ordered writer command, or an explicit admission error.
/// 每条精确记录完成顺序写入命令预留后返回成功，否则返回明确的准入错误。
pub(crate) fn spawn_exact_session_transcript_entries_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    exact_tail_messages: Vec<SessionMessageEntry>,
    reason: Option<String>,
) -> Result<(), String> {
    spawn_session_transcript_persist_task(
        state,
        controller_runtime,
        session_id,
        Some(exact_tail_messages),
        reason,
    )
}

/// Spawns the shared ordered transcript writer with retained-window or exact-entry admission.
/// 使用保留窗口或精确记录准入启动共享的有序转录写入器。
///
/// # Parameters
/// 参数
/// `state` owns the exact retained session locator and optional retained display window.
/// `state` 持有精确保留会话定位信息与可选的保留展示窗口。
/// `controller_runtime` provides the deterministic session data root.
/// `controller_runtime` 提供确定性的会话数据根目录。
/// `session_id` identifies the sole transcript writer receiving the commands.
/// `session_id` 标识接收命令的唯一转录写入器。
/// `exact_tail_messages` bypasses display-window inference when the caller owns exact changed rows.
/// `exact_tail_messages` 在调用方持有精确变更记录时绕过展示窗口推断。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
///
/// # Returns
/// 返回值
/// Success after commands are synchronously admitted and asynchronously scheduled, or an exact locator/cursor error.
/// 命令同步完成准入并异步调度后返回成功，否则返回精确定位器或游标错误。
fn spawn_session_transcript_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    exact_tail_messages: Option<Vec<SessionMessageEntry>>,
    reason: Option<String>,
) -> Result<(), String> {
    // session_directory comes only from the retained exact main-session or TaskSession locator.
    // session_directory 仅来自保留态中的精确主会话或 TaskSession 定位信息。
    let session_directory = match state.lock() {
        Ok(guard) => match segmented_session_directory_for_retained_session(
            &guard,
            &controller_runtime.data_root,
            session_id,
        ) {
            Ok(Some(session_directory)) => session_directory,
            Ok(None) => {
                return Err(format!(
                    "Live transcript exact session directory is unavailable for session `{session_id}`."
                ));
            }
            Err(error) => {
                return Err(format!(
                    "Live transcript exact session directory resolution failed for session `{session_id}`: {error}"
                ));
            }
        },
        Err(error) => {
            return Err(format!(
                "Live transcript locator lock failed for session `{session_id}`: {error}"
            ));
        }
    };
    // commands reserve one exact immutable row each before later runtime events advance the cursor.
    // commands 在后续运行事件推进游标前分别预留一条精确不可变记录。
    let prepared_commands = match prepare_session_transcript_writer_commands(
        &state,
        session_id,
        &session_directory,
        exact_tail_messages,
    ) {
        Ok(prepared_commands) if prepared_commands.is_empty() => return Ok(()),
        Ok(prepared_commands) => prepared_commands,
        Err(error) => return Err(error),
    };
    for prepared_command in prepared_commands {
        // command and pending_command were atomically published before shutdown could seal admission.
        // command 与 pending_command 已在关闭流程能够封印准入前完成原子发布。
        let PreparedSessionTranscriptWriterCommand {
            command,
            pending_command,
        } = prepared_command;
        // command_reason gives each independent stable-row task an owned diagnostic label.
        // command_reason 为每个独立稳定记录任务提供自有诊断标签。
        let command_reason = reason.clone();
        spawn_service_task(async move {
            let _pending_command = pending_command;
            let command = command;
            if let Err(error) = await_session_transcript_writer_command_turn(&command).await {
                let completion_result =
                    complete_session_transcript_writer_command(&command, Err(error.clone()));
                let reason_label = command_reason.as_deref().unwrap_or("live_transcript");
                eprintln!(
                    "[webui-service] transcript writer ordering failed ({reason_label}): session={session_id} error={error} completion={completion_result:?}"
                );
                return;
            }
            let persist_lock = webui_session_persist_lock(session_id);
            let _persist_guard = persist_lock.lock().await;
            // retry shares the immutable payload with shutdown recovery without cloning message bodies.
            // retry 与关闭恢复共享不可变载荷，不复制消息正文。
            let retry = Arc::clone(&command.retry);
            // append_cursor is shared once per resident session rather than duplicated in every retry payload.
            // append_cursor 按常驻会话共享一次，而不是在每条重试载荷中重复保存。
            let append_cursor = command
                .cursor
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .append_cursor
                .as_ref()
                .cloned()
                .expect("writer command retains its initialized append cursor");
            let physical_result = tokio::task::spawn_blocking(move || {
                let mut append_cursor_guard = append_cursor
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let durable_checkpoint = vulcan_workspace_store::workspace_store::
                    persist_single_session_transcript_tail_with_cursor(
                        &mut append_cursor_guard,
                        &retry.expected_checkpoint,
                        retry.tail_messages.as_slice(),
                    )?;
                if durable_checkpoint != retry.committed_checkpoint {
                    return Err(format!(
                        "Transcript writer durable checkpoint mismatch: expected={:?} actual={durable_checkpoint:?}.",
                        retry.committed_checkpoint
                    ));
                }
                Ok(())
            })
            .await
            .map_err(|error| format!("Transcript writer blocking task failed: {error}"))
            .and_then(|result| result);
            let completion_result =
                complete_session_transcript_writer_command(&command, physical_result);
            if let Err(error) = completion_result {
                let reason_label = command_reason.as_deref().unwrap_or("live_transcript");
                eprintln!(
                    "[webui-service] background live transcript persist failed ({reason_label}): session={session_id} error={error}"
                );
            }
        });
    }
    Ok(())
}

/// Spawns a background write for one exact session addon-state document.
/// 为单个精确会话附属状态文档启动后台写入任务。
///
/// # Parameters
/// 参数
/// `app` is the event emitter used after the file write succeeds.
/// `app` 是文件写入成功后使用的事件发送器。
/// `state` owns the addon persistence version table used to reject stale writes.
/// `state` 持有用于拒绝过期写入的附属状态持久化版本表。
/// `session_directory` is the deterministic target resolved before this task starts.
/// `session_directory` 是任务启动前已经解析完成的确定性目标。
/// `session_id` selects the addon document to persist.
/// `session_id` 选择要持久化的附属状态文档。
/// `addon_state` is the already-derived addon document to persist.
/// `addon_state` 是已经派生完成且需要持久化的附属状态文档。
/// `changed_namespaces` are the namespace keys represented by this write.
/// `changed_namespaces` 是本次写入代表的命名空间键。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
pub(crate) fn spawn_exact_session_addon_state_persist_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    session_directory: PathBuf,
    session_id: SessionId,
    addon_state: SessionAddonState,
    changed_namespaces: Vec<SessionAddonNamespace>,
    reason: Option<String>,
) {
    let mut persist_versions = Vec::new();
    for namespace in &changed_namespaces {
        match next_webui_session_addon_persist_version(&state, session_id, *namespace) {
            Ok(version) => persist_versions.push((*namespace, version)),
            Err(error) => {
                eprintln!(
                    "[webui-service] background session-addon persist version failed: {error}"
                );
                return;
            }
        }
    }
    spawn_service_task(async move {
        let persist_lock = webui_session_persist_lock(session_id);
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_addon_persist_versions_are_current(&state, session_id, &persist_versions)
        {
            return;
        }
        match vulcan_workspace_store::workspace_store::persist_single_session_addon_state_update_in_directory(
            session_directory,
            session_id,
            &addon_state,
        )
        .await
        {
            Ok(merged_state) => {
                emit_session_addon_changed_event(
                    &app,
                    session_id,
                    &changed_namespaces,
                    merged_state.revision,
                    latest_session_addon_updated_at(&merged_state),
                );
            }
            Err(error) => {
                let reason_label = reason.as_deref().unwrap_or("session_addon");
                eprintln!(
                    "[webui-service] background session-addon persist failed ({reason_label}): {error}"
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a workspace snapshot with one active and one archived session/project row.
    /// 构建包含一条活动与一条归档会话/项目行的工作区快照。
    fn test_workspace_snapshot_with_archived_rows(
        active_session_id: SessionId,
        archived_session_id: SessionId,
    ) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects: vec![TaskProjectSummary::new(
                "active-project",
                "Active Project",
                r"D:\active-project",
                false,
                0,
                None,
                None,
                vec![active_session_id],
            )],
            archived_task_projects: vec![TaskProjectSummary::new(
                "archived-project",
                "Archived Project",
                r"D:\archived-project",
                false,
                1,
                None,
                None,
                vec![archived_session_id],
            )],
            sessions: vec![SessionSummary::new(
                active_session_id,
                Some(String::from("active-project")),
                "Active Session",
                "",
                SessionState::Running,
                0,
                0,
                "now",
            )],
            archived_sessions: vec![SessionSummary::new(
                archived_session_id,
                Some(String::from("archived-project")),
                "Archived Session",
                "",
                SessionState::Completed,
                0,
                0,
                "earlier",
            )],
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: active_session_id,
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Verifies session lookup reports active and archived rows through one helper.
    /// 验证会话查找通过同一个 helper 区分活动与归档行。
    #[test]
    fn workspace_session_with_archive_status_reports_active_and_archived_rows() {
        let active_session_id = SessionId::new_v4();
        let archived_session_id = SessionId::new_v4();
        let missing_session_id = SessionId::new_v4();
        let workspace_snapshot =
            test_workspace_snapshot_with_archived_rows(active_session_id, archived_session_id);

        let (active_session, active_archived) =
            workspace_session_with_archive_status(&workspace_snapshot, active_session_id)
                .expect("active session should be found");
        let (archived_session, archived_archived) =
            workspace_session_with_archive_status(&workspace_snapshot, archived_session_id)
                .expect("archived session should be found");

        assert_eq!(active_session.id, active_session_id);
        assert!(!active_archived);
        assert_eq!(archived_session.id, archived_session_id);
        assert!(archived_archived);
        assert!(workspace_snapshot_contains_regular_session(
            &workspace_snapshot,
            active_session_id
        ));
        assert!(workspace_snapshot_contains_regular_session(
            &workspace_snapshot,
            archived_session_id
        ));
        assert!(!workspace_snapshot_contains_regular_session(
            &workspace_snapshot,
            missing_session_id
        ));
    }

    /// Verifies a partial or untracked display cache can never authorize a complete transcript rewrite.
    /// 验证局部或未跟踪的展示缓存永远不能授权完整转录重写。
    #[test]
    fn partial_retained_window_never_authorizes_complete_transcript_persistence() {
        let messages = vec![SessionMessageEntry {
            id: String::from("tail-message"),
            kind: vulcan_context::SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("output"),
            body: String::from("tail"),
            metadata: None,
            created_at: String::from("1"),
        }];
        let partial_window = HydratedConversationWindowState {
            transcript_window_loaded: true,
            message_start_index: Some(40),
            earliest_ordinal: Some(100),
            has_more_history: true,
        };

        assert!(matches!(
            classify_retained_transcript_persistence(&messages, Some(&partial_window)),
            RetainedTranscriptPersistence::PartialWindow {
                message_start_index: Some(40),
                messages: retained_messages,
            } if retained_messages == messages
        ));
        let boundary_projected_window = HydratedConversationWindowState {
            transcript_window_loaded: true,
            message_start_index: None,
            earliest_ordinal: Some(100),
            has_more_history: true,
        };
        assert!(matches!(
            classify_retained_transcript_persistence(
                &messages,
                Some(&boundary_projected_window)
            ),
            RetainedTranscriptPersistence::PartialWindow {
                message_start_index: None,
                messages: retained_messages,
            } if retained_messages == messages
        ));
        // boundaryProjectedCompleteWindow has no older Boundary turns, but it still lacks the
        // one-to-one Raw message coordinate required to prove that process rows are complete.
        // boundaryProjectedCompleteWindow 虽然没有更早的 Boundary 回合，但仍缺少证明过程记录
        // 完整所需的 Raw 一对一消息坐标。
        let boundary_projected_complete_window = HydratedConversationWindowState {
            transcript_window_loaded: true,
            message_start_index: None,
            earliest_ordinal: Some(0),
            has_more_history: false,
        };
        assert!(matches!(
            classify_retained_transcript_persistence(
                &messages,
                Some(&boundary_projected_complete_window)
            ),
            RetainedTranscriptPersistence::PartialWindow {
                message_start_index: None,
                messages: retained_messages,
            } if retained_messages == messages
        ));
        let marker_only_window = HydratedConversationWindowState {
            transcript_window_loaded: false,
            message_start_index: None,
            earliest_ordinal: None,
            has_more_history: false,
        };
        assert!(matches!(
            classify_retained_transcript_persistence(&messages, Some(&marker_only_window)),
            RetainedTranscriptPersistence::ExistingIndexOnly
        ));
        assert!(matches!(
            classify_retained_transcript_persistence(&messages, None),
            RetainedTranscriptPersistence::ExistingIndexOnly
        ));
    }

    /// Verifies only a complete index-zero window retains full transcript persistence authority.
    /// 验证仅从下标零开始的完整窗口保留完整转录持久化权威。
    #[test]
    fn complete_retained_window_authorizes_complete_transcript_persistence() {
        let complete_window = HydratedConversationWindowState {
            transcript_window_loaded: true,
            message_start_index: Some(0),
            earliest_ordinal: Some(0),
            has_more_history: false,
        };

        assert!(matches!(
            classify_retained_transcript_persistence(&[], Some(&complete_window)),
            RetainedTranscriptPersistence::Complete(messages) if messages.is_empty()
        ));
    }

    /// Verifies a partial retained window defers exact suffix selection until after capture.
    /// 验证局部保留窗口会将精确后缀选择延迟到捕获之后。
    #[test]
    fn partial_retained_window_preserves_absolute_coordinate_for_deferred_append() {
        let messages = (40..43)
            .map(|index| SessionMessageEntry {
                id: format!("message-{index}"),
                kind: vulcan_context::SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("output"),
                body: index.to_string(),
                metadata: None,
                created_at: index.to_string(),
            })
            .collect::<Vec<_>>();
        let partial_window = HydratedConversationWindowState {
            transcript_window_loaded: true,
            message_start_index: Some(40),
            earliest_ordinal: Some(100),
            has_more_history: true,
        };
        let authority = classify_retained_transcript_persistence(&messages, Some(&partial_window));

        match authority {
            RetainedTranscriptPersistence::PartialWindow {
                message_start_index,
                messages: retained_messages,
            } => {
                assert_eq!(message_start_index, Some(40));
                assert_eq!(retained_messages, messages);
            }
            _ => panic!("partial window should retain deferred append authority"),
        }
    }

    /// Verifies absolute persisted counts are translated into local retained-window coordinates.
    /// 验证绝对持久化数量会转换为保留窗口的局部坐标。
    #[test]
    fn retained_transcript_tail_translates_absolute_window_boundary() {
        let messages = (40..44)
            .map(|index| SessionMessageEntry {
                id: format!("message-{index}"),
                kind: vulcan_context::SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("output"),
                body: index.to_string(),
                metadata: None,
                created_at: index.to_string(),
            })
            .collect::<Vec<_>>();

        let tail = retained_transcript_tail_for_persisted_boundary(&messages, 40, 42)
            .expect("translate absolute boundary")
            .expect("two new messages remain");
        assert_eq!(
            tail.into_iter()
                .map(|message| message.id)
                .collect::<Vec<_>>(),
            vec![String::from("message-42"), String::from("message-43")]
        );
    }

    /// Verifies a metadata checkpoint allows a partial window to append from its exact start.
    /// 验证元数据检查点允许局部窗口从其精确起点执行追加。
    #[test]
    fn retained_transcript_tail_can_begin_at_partial_window_start() {
        let messages = vec![SessionMessageEntry {
            id: String::from("message-40"),
            kind: vulcan_context::SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("output"),
            body: String::from("40"),
            metadata: None,
            created_at: String::from("40"),
        }];

        let tail = retained_transcript_tail_for_persisted_boundary(&messages, 40, 40)
            .expect("translate partial window start")
            .expect("one new message remains");
        assert_eq!(tail[0].id, "message-40");
    }

    /// Verifies a boundary-projected window can append after an exact durable identity without an absolute row coordinate.
    /// 验证边界投影窗口无需绝对行坐标，也能在精确可靠身份之后追加。
    #[test]
    fn retained_transcript_tail_uses_checkpoint_identity_without_absolute_coordinate() {
        let messages = (40..43)
            .map(|index| SessionMessageEntry {
                id: format!("message-{index}"),
                kind: vulcan_context::SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("output"),
                body: index.to_string(),
                metadata: None,
                created_at: index.to_string(),
            })
            .collect::<Vec<_>>();
        let checkpoint =
            vulcan_workspace_store::workspace_store::advance_single_session_transcript_checkpoint(
                &vulcan_workspace_store::workspace_store::PersistedSessionTranscriptCheckpoint {
                    message_count: 40,
                    last_message_id: None,
                    last_message_fingerprint: None,
                },
                &messages[..2],
            )
            .expect("build exact durable checkpoint");

        let tail = retained_transcript_tail_for_checkpoint(&messages, None, &checkpoint)
            .expect("match exact checkpoint identity")
            .expect("one new message remains");

        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].id, "message-42");

        let mut unmatched_checkpoint = checkpoint;
        unmatched_checkpoint.last_message_id = Some(String::from("message-outside-window"));
        let unmatched_tail =
            retained_transcript_tail_for_checkpoint(&messages, None, &unmatched_checkpoint)
                .expect("an unanchored boundary projection must remain non-destructive");
        assert!(unmatched_tail.is_none());
    }

    /// Verifies shutdown validation rejects a completed writer failure instead of treating zero pending work as success.
    /// 验证关闭校验会拒绝已经完成的写入器失败，而不会将待处理数量为零误判为成功。
    #[test]
    fn shutdown_drain_rejects_persistence_blocked_writer() {
        // session_id identifies the exact failed writer in the expected diagnostic.
        // session_id 标识预期诊断中的精确失败写入器。
        let session_id = SessionId::new_v4();
        // registry contains one idle command queue whose physical append already failed.
        // registry 包含一个命令队列已空但物理追加已经失败的写入器。
        let registry = BTreeMap::from([(
            session_id,
            Arc::new(StdMutex::new(SessionTranscriptWriterCursor {
                session_directory: PathBuf::from(r"D:\shutdown-drain-test"),
                append_cursor: None,
                committed_checkpoint: None,
                durable_checkpoint: None,
                queue_depth: 0,
                retry_commands: VecDeque::new(),
                blocked_error: Some(String::from("simulated durable append failure")),
                notify: Arc::new(tokio::sync::Notify::new()),
            })),
        )]);

        // error proves the hidden completed failure remains visible to the desktop close boundary.
        // error 证明隐藏的已完成失败对桌面关闭边界保持可见。
        let error = validate_drained_session_transcript_writer_registry(&registry)
            .expect_err("blocked writer must reject shutdown drain");

        assert!(error.contains(&session_id.to_string()));
        assert!(error.contains("simulated durable append failure"));
    }

    /// Verifies shutdown retry preserves command order and clears blocking only after every retained suffix is durable.
    /// 验证关闭重试保持命令顺序，并且仅在全部保留后缀可靠落盘后清除阻塞。
    #[test]
    fn shutdown_retry_replays_retained_commands_in_exact_order() {
        // session_id identifies the exact synthetic resident writer.
        // session_id 标识精确的合成常驻写入器。
        let session_id = SessionId::new_v4();
        // checkpoint_zero is the physical predecessor before both retained commands.
        // checkpoint_zero 是两条保留命令之前的物理前驱。
        let checkpoint_zero =
            vulcan_workspace_store::workspace_store::PersistedSessionTranscriptCheckpoint {
                message_count: 0,
                last_message_id: None,
                last_message_fingerprint: None,
            };
        // checkpoint_one is the exact committed boundary of the first retained command.
        // checkpoint_one 是第一条保留命令的精确已提交边界。
        let checkpoint_one =
            vulcan_workspace_store::workspace_store::PersistedSessionTranscriptCheckpoint {
                message_count: 1,
                last_message_id: Some(String::from("message-1")),
                last_message_fingerprint: Some(String::from("fingerprint-1")),
            };
        // checkpoint_two is the exact committed boundary of the second retained command.
        // checkpoint_two 是第二条保留命令的精确已提交边界。
        let checkpoint_two =
            vulcan_workspace_store::workspace_store::PersistedSessionTranscriptCheckpoint {
                message_count: 2,
                last_message_id: Some(String::from("message-2")),
                last_message_fingerprint: Some(String::from("fingerprint-2")),
            };
        // session_directory is diagnostic-only because the injected persistence operation owns physical behavior.
        // session_directory 仅用于诊断，因为注入的持久化操作负责物理行为。
        let session_directory = PathBuf::from(r"D:\shutdown-retry-order-test");
        // first_retry retains the first immutable logical boundary.
        // first_retry 保留第一条不可变逻辑边界。
        let first_retry = Arc::new(SessionTranscriptWriterRetryCommand {
            session_directory: session_directory.clone(),
            expected_checkpoint: checkpoint_zero.clone(),
            committed_checkpoint: checkpoint_one.clone(),
            tail_messages: Arc::new(Vec::new()),
        });
        // second_retry depends exactly on the first retained boundary.
        // second_retry 精确依赖第一条保留边界。
        let second_retry = Arc::new(SessionTranscriptWriterRetryCommand {
            session_directory: session_directory.clone(),
            expected_checkpoint: checkpoint_one,
            committed_checkpoint: checkpoint_two.clone(),
            tail_messages: Arc::new(Vec::new()),
        });
        // cursor starts blocked with both immutable commands still retained.
        // cursor 以阻塞状态开始，并仍保留两条不可变命令。
        let cursor = Arc::new(StdMutex::new(SessionTranscriptWriterCursor {
            session_directory,
            append_cursor: None,
            committed_checkpoint: Some(checkpoint_two.clone()),
            durable_checkpoint: Some(checkpoint_zero),
            queue_depth: 0,
            retry_commands: VecDeque::from([first_retry, second_retry]),
            blocked_error: Some(String::from("simulated first-attempt failure")),
            notify: Arc::new(tokio::sync::Notify::new()),
        }));
        // observed_boundaries records the order seen by the injected durable operation.
        // observed_boundaries 记录注入可靠操作观察到的顺序。
        let mut observed_boundaries = Vec::new();

        retry_retained_session_transcript_writer_commands_with(session_id, &cursor, |retry| {
            observed_boundaries.push(retry.committed_checkpoint.message_count);
            Ok(retry.committed_checkpoint.clone())
        })
        .expect("retry retained commands");
        // cursor_guard observes the final exact durable state after both retries.
        // cursor_guard 观察两次重试后的最终精确可靠状态。
        let cursor_guard = cursor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        assert_eq!(observed_boundaries, vec![1, 2]);
        assert_eq!(cursor_guard.durable_checkpoint, Some(checkpoint_two));
        assert!(cursor_guard.retry_commands.is_empty());
        assert!(cursor_guard.blocked_error.is_none());
    }

    /// Verifies shutdown validation accepts only an idle writer whose committed and durable frontiers match.
    /// 验证关闭校验仅接受空闲且已提交与耐久前沿一致的写入器。
    #[test]
    fn shutdown_drain_accepts_matching_durable_frontier() {
        // session_id identifies the exact clean writer fixture.
        // session_id 标识精确的干净写入器夹具。
        let session_id = SessionId::new_v4();
        // checkpoint represents the same committed and durable terminal boundary.
        // checkpoint 表示相同的已提交与耐久终态边界。
        let checkpoint =
            vulcan_workspace_store::workspace_store::PersistedSessionTranscriptCheckpoint {
                message_count: 1,
                last_message_id: Some(String::from("durable-message")),
                last_message_fingerprint: Some(String::from("durable-fingerprint")),
            };
        // registry contains one fully drained exact-session writer.
        // registry 包含一个已经完全排空的精确会话写入器。
        let registry = BTreeMap::from([(
            session_id,
            Arc::new(StdMutex::new(SessionTranscriptWriterCursor {
                session_directory: PathBuf::from(r"D:\shutdown-drain-clean-test"),
                append_cursor: None,
                committed_checkpoint: Some(checkpoint.clone()),
                durable_checkpoint: Some(checkpoint),
                queue_depth: 0,
                retry_commands: VecDeque::new(),
                blocked_error: None,
                notify: Arc::new(tokio::sync::Notify::new()),
            })),
        )]);

        validate_drained_session_transcript_writer_registry(&registry)
            .expect("matching durable frontier should permit shutdown");
    }
}
