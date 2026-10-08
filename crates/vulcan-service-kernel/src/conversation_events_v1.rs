//! Authoritative V1 conversation-event hydration, paging, and reconnect reads.
//! 权威 V1 会话事件 hydration、分页与重连读取。

use vulcan_service_api::{
    CONVERSATION_EVENT_WINDOW_V1_TURN_LIMIT, ConversationEventHydrationV1Request,
    ConversationEventPageV1Request, ConversationEventReconnectV1Request,
    ConversationEventReconnectV1Response, ConversationEventRehydrateReasonV1,
    ConversationEventRevisionFrontierV1, ConversationEventWindowV1Response,
    ConversationSessionConfigV1, ServiceEventEmitter,
};
use vulcan_session_core::v1::config::SessionConfig;
use vulcan_session_core::v1::event::ConversationEvent;
use vulcan_session_core::v1::ids::{Seq, TurnId};
use vulcan_session_core::v1::service_dto::ConversationEventWindowBatch;
use vulcan_workbench_model::SessionId;
use vulcan_workspace_store::workspace_store::{
    SessionV1EventWindowState, SessionV1StorageLocator, locate_embedded_task_session_v1,
    locate_session_v1_storage,
};

use crate::session_runtime_registry_v1::{
    SessionHandle, SessionRuntimeError, SessionRuntimeRegistry,
};
use crate::state::SharedServiceBridgeState;

/// Authoritative resident tail captured from the unique active session handle.
/// 从唯一活动会话句柄捕获的权威驻留尾段。
#[derive(Debug, Clone)]
struct ResidentEventSnapshot {
    /// Persisted event-tree generation owned by the document.
    /// 文档拥有的持久化事件树代次。
    window_generation: u64,
    /// Current authoritative in-memory document revision.
    /// 当前权威内存文档修订号。
    memory_revision: u64,
    /// Last revision confirmed by the physical writer.
    /// 物理写入器最后确认的可靠修订号。
    durable_revision: u64,
    /// Complete resident tail in exact seq order.
    /// 按精确 seq 顺序排列的完整驻留尾段。
    events: Vec<ConversationEvent>,
    /// Current open Turn identity, when any.
    /// 当前开放 Turn 身份（如有）。
    open_turn_id: Option<TurnId>,
    /// Current open Turn start seq, when any.
    /// 当前开放 Turn 起始 seq（如有）。
    open_turn_start_seq: Option<Seq>,
    /// Current V1 metadata configuration owned by the resident document.
    /// 常驻文档拥有的当前 V1 metadata 配置。
    config: SessionConfig,
}

/// Installs the production event emitter on the unique V1 session registry.
/// 在唯一 V1 会话注册表上安装生产事件发布器。
///
/// # Parameters
/// 参数
/// `state` owns the unique registry and `emitter` is the host service-event
/// multiplexer shared by browser and desktop sinks.
/// `state` 持有唯一注册表，`emitter` 是浏览器与桌面 sink 共享的宿主服务事件
/// 复用器。
///
/// # Returns
/// 返回值
/// Returns after current and future session handles are bound to the publisher.
/// 在当前及未来会话句柄都绑定发布器后返回。
pub fn service_install_conversation_event_v1_emitter(
    state: &SharedServiceBridgeState,
    emitter: ServiceEventEmitter,
) -> Result<(), String> {
    let registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        std::sync::Arc::clone(&guard.session_runtime_registry)
    };
    registry
        .install_live_event_emitter(emitter)
        .map_err(|error| format!("failed to install V1 conversation event emitter: {error}"))
}

/// Loads the latest 30 complete Turns plus the entire open Turn for one V1
/// session.
/// 为一个 V1 会话加载最近 30 个完整 Turn 及整个开放 Turn。
///
/// # Parameters
/// 参数
/// `state` owns the exact SQLite controller and unique active-session registry;
/// `request` identifies the target session.
/// `state` 持有精确 SQLite 控制器与唯一活动会话注册表；`request` 标识目标会话。
///
/// # Returns
/// 返回值
/// Returns a contiguous ownership-bound window whose overlapping resident tail
/// always overrides the persisted copy.
/// 返回一个连续的带归属窗口，其中重叠驻留尾段始终覆盖持久化副本。
pub(crate) async fn service_session_event_hydration_v1(
    state: SharedServiceBridgeState,
    request: ConversationEventHydrationV1Request,
) -> Result<ConversationEventWindowV1Response, String> {
    let (session_id, locator, registry, snapshot_only) =
        resolve_hydration_locator_and_registry(&state, &request).await?;
    let persisted = locator
        .load_recent_event_window(CONVERSATION_EVENT_WINDOW_V1_TURN_LIMIT)
        .map_err(|error| format!("failed to load V1 event hydration: {error}"))?;
    // Embedded TaskSession snapshots deliberately ignore the runtime registry so closed agents remain unloaded.
    // 嵌入式 TaskSession 快照刻意忽略运行时注册表，确保已关闭智能体保持卸载状态。
    let resident = if snapshot_only {
        None
    } else {
        optional_registered_handle(&registry, session_id)?
            .map(|handle| capture_resident_snapshot(&handle))
            .transpose()?
    };
    let revision_frontier = authoritative_revision_frontier(
        registry.service_epoch(),
        &persisted.state,
        resident.as_ref(),
    )?;
    // One-shot closed-agent reads expose only the latest visible window and never create a paging owner.
    // 已关闭智能体的一次性读取仅暴露最近可见窗口，绝不创建分页归属。
    let older_before_seq = if snapshot_only {
        None
    } else {
        persisted.older_before_seq
    };
    let resident = resident.map(|mut snapshot| {
        if let Some(first_persisted) = persisted.events.first() {
            let first_seq = first_persisted.seq.value();
            snapshot
                .events
                .retain(|event| event.seq.value() >= first_seq);
        }
        snapshot
    });
    // Answer mode visibility follows confirmed storage, even when transcript memory is newer.
    // 即使转录内存更新，回答模式可见性仍跟随已确认存储。
    let durable_answer_mode = persisted.state.config.answer_mode.clone();
    let (window_generation, events, open_turn_id, open_turn_start_seq, mut session_config) =
        merge_persisted_with_resident(persisted.state, persisted.events, resident, "hydration")?;
    session_config.answer_mode = durable_answer_mode;
    let open_turn_id =
        open_turn_id.or_else(|| open_turn_id_from_events(&events, open_turn_start_seq));
    Ok(ConversationEventWindowV1Response::hydration(
        ConversationEventWindowBatch::from_events(
            session_id.to_string(),
            window_generation,
            &events,
        ),
        older_before_seq,
        open_turn_id,
        open_turn_start_seq,
        ConversationSessionConfigV1::from_session_config(&session_config),
        revision_frontier,
    ))
}

/// Resolves either an indexed live session or one exact embedded TaskSession snapshot locator.
/// 解析已索引实时会话，或一个精确嵌入式 TaskSession 快照定位器。
///
/// # Parameters
/// 参数
/// `state` owns the controller and the shared live-session registry.
/// `state` 持有控制器与共享实时会话注册表。
/// `request` identifies the target session and optionally its indexed parent.
/// `request` 标识目标会话，并可选标识其已索引父会话。
///
/// # Returns
/// 返回值
/// Returns the target identity, exact locator, registry, and whether the read must remain snapshot-only.
/// 返回目标身份、精确定位器、注册表，以及本次读取是否必须保持仅快照模式。
async fn resolve_hydration_locator_and_registry(
    state: &SharedServiceBridgeState,
    request: &ConversationEventHydrationV1Request,
) -> Result<
    (
        SessionId,
        SessionV1StorageLocator,
        std::sync::Arc<SessionRuntimeRegistry>,
        bool,
    ),
    String,
> {
    let Some(parent_session_id) = request.parent_session_id.as_deref() else {
        let (session_id, locator, registry) =
            resolve_exact_locator_and_registry(state, &request.session_id).await?;
        return Ok((session_id, locator, registry, false));
    };
    let task_session_id = request.session_id.parse::<SessionId>().map_err(|error| {
        format!(
            "invalid embedded task session id '{}': {error}",
            request.session_id
        )
    })?;
    let (resolved_parent_session_id, parent_locator, registry) =
        resolve_exact_locator_and_registry(state, parent_session_id).await?;
    let task_locator =
        locate_embedded_task_session_v1(&parent_locator, &task_session_id.to_string()).map_err(
            |error| {
                format!(
                    "failed to locate embedded V1 task session '{task_session_id}' under parent '{resolved_parent_session_id}': {error}"
                )
            },
        )?;
    Ok((task_session_id, task_locator, registry, true))
}

/// Loads one immediately adjacent historical complete-Turn page from the exact
/// SQLite locator and V1 split map.
/// 从精确 SQLite 定位器与 V1 分片映射加载一页紧邻的更早完整 Turn 历史。
///
/// # Parameters
/// 参数
/// `state` owns the controller and `request` supplies the current owner and
/// exclusive earliest seq cursor.
/// `state` 持有控制器，`request` 提供当前归属与排他最早 seq 游标。
///
/// # Returns
/// 返回值
/// Returns a complete-Turn page that never consults an active handle, scans a
/// directory, or falls back to the legacy transcript store.
/// 返回一页完整 Turn；绝不读取活动句柄、扫描目录或回退旧转录存储。
pub(crate) async fn service_session_event_page_v1(
    state: SharedServiceBridgeState,
    request: ConversationEventPageV1Request,
) -> Result<ConversationEventWindowV1Response, String> {
    let (session_id, locator) = resolve_exact_locator(&state, &request.session_id).await?;
    let before_seq = checked_seq(request.before_seq, "beforeSeq")?;
    let page = locator
        .load_event_window_before(before_seq, CONVERSATION_EVENT_WINDOW_V1_TURN_LIMIT)
        .map_err(|error| format!("failed to load V1 event history page: {error}"))?;
    require_window_generation(request.window_generation, page.state.segment_generation)?;
    Ok(ConversationEventWindowV1Response::historical_page(
        ConversationEventWindowBatch::from_events(
            session_id.to_string(),
            page.state.segment_generation,
            &page.events,
        ),
        page.older_before_seq,
        ConversationSessionConfigV1::from_session_config(&page.state.config),
    ))
}

/// Loads a duplicate-safe reconnect delta beginning at the last confirmed seq
/// and extending through the active authoritative tail.
/// 加载从最后确认 seq 开始并延伸到活动权威尾段的可安全去重重连增量。
///
/// # Parameters
/// 参数
/// `state` owns the exact storage and active handle; `request` supplies the
/// current owner generation and optional last confirmed seq.
/// `state` 持有精确存储与活动句柄；`request` 提供当前归属代次与可选最后确认 seq。
///
/// # Returns
/// 返回值
/// Returns the last confirmed event again when available followed by every
/// missing event, allowing same-seq terminal evolution to settle idempotently.
/// 在可用时再次返回最后确认事件，随后返回全部缺失事件，使同 seq 终态演进可幂等收敛。
pub(crate) async fn service_session_event_reconnect_v1(
    state: SharedServiceBridgeState,
    request: ConversationEventReconnectV1Request,
) -> Result<ConversationEventReconnectV1Response, String> {
    let (session_id, locator, registry) =
        resolve_exact_locator_and_registry(&state, &request.session_id).await?;
    let persisted_state = locator
        .event_window_state()
        .map_err(|error| format!("failed to read V1 reconnect frontier: {error}"))?;
    let durable_answer_mode = persisted_state.config.answer_mode.clone();
    let resident = optional_registered_handle(&registry, session_id)?
        .map(|handle| capture_resident_snapshot(&handle))
        .transpose()?;
    let revision_frontier = authoritative_revision_frontier(
        registry.service_epoch(),
        &persisted_state,
        resident.as_ref(),
    )?;
    if request.service_epoch != registry.service_epoch() {
        return ConversationEventReconnectV1Response::rehydrate_required(
            session_id.to_string(),
            persisted_state.segment_generation,
            registry.service_epoch(),
            request.last_confirmed_memory_revision,
            revision_frontier.memory_revision,
            revision_frontier.durable_revision,
            ConversationEventRehydrateReasonV1::ServiceEpochChanged,
        );
    }
    if request.window_generation != persisted_state.segment_generation {
        return ConversationEventReconnectV1Response::rehydrate_required(
            session_id.to_string(),
            persisted_state.segment_generation,
            registry.service_epoch(),
            request.last_confirmed_memory_revision,
            revision_frontier.memory_revision,
            revision_frontier.durable_revision,
            ConversationEventRehydrateReasonV1::WindowGenerationChanged,
        );
    }
    if request.last_confirmed_memory_revision > revision_frontier.memory_revision {
        return ConversationEventReconnectV1Response::rehydrate_required(
            session_id.to_string(),
            persisted_state.segment_generation,
            registry.service_epoch(),
            request.last_confirmed_memory_revision,
            revision_frontier.memory_revision,
            revision_frontier.durable_revision,
            ConversationEventRehydrateReasonV1::MemoryRevisionAhead,
        );
    }
    let persisted_latest = persisted_state
        .next_seq
        .checked_sub(1)
        .filter(|value| *value >= 1);
    let resident_latest = resident
        .as_ref()
        .and_then(|snapshot| snapshot.events.last())
        .map(|event| event.seq.value());
    let authoritative_latest = persisted_latest.into_iter().chain(resident_latest).max();
    if reconnect_cursor_is_ahead(request.last_confirmed_seq, authoritative_latest) {
        return ConversationEventReconnectV1Response::rehydrate_required(
            session_id.to_string(),
            persisted_state.segment_generation,
            registry.service_epoch(),
            request.last_confirmed_memory_revision,
            revision_frontier.memory_revision,
            revision_frontier.durable_revision,
            ConversationEventRehydrateReasonV1::CursorAhead,
        );
    }
    let open_turn_start_seq = authoritative_open_turn_start(
        persisted_state.open_turn_start_seq,
        resident
            .as_ref()
            .and_then(|snapshot| snapshot.open_turn_start_seq),
    )?;
    let from_value = reconnect_from_value(
        request.last_confirmed_seq,
        authoritative_latest,
        open_turn_start_seq,
    )?;
    let Some(from_value) = from_value else {
        return ConversationEventReconnectV1Response::recovery(
            ConversationEventWindowBatch::from_events(
                session_id.to_string(),
                request.window_generation,
                &[],
            ),
            registry.service_epoch(),
            request.last_confirmed_memory_revision,
            revision_frontier.memory_revision,
            revision_frontier.durable_revision,
        )
        .map(|mut response| {
            response.answer_mode = Some(durable_answer_mode);
            response
        });
    };
    let mut persisted_events = Vec::new();
    if let Some(persisted_latest) = persisted_latest
        && from_value <= persisted_latest
    {
        let range = locator
            .load_event_range(
                checked_seq(from_value, "lastConfirmedSeq")?,
                checked_seq(persisted_latest, "persistedLatestSeq")?,
            )
            .map_err(|error| format!("failed to load V1 reconnect range: {error}"))?;
        require_window_generation(request.window_generation, range.state.segment_generation)?;
        persisted_events = range.events;
    }
    let resident = resident.map(|mut snapshot| {
        snapshot
            .events
            .retain(|event| event.seq.value() >= from_value);
        snapshot
    });
    let (_, events, _, _, _) =
        merge_persisted_with_resident(persisted_state, persisted_events, resident, "reconnect")?;
    ConversationEventReconnectV1Response::recovery(
        ConversationEventWindowBatch::from_events(
            session_id.to_string(),
            request.window_generation,
            &events,
        ),
        registry.service_epoch(),
        request.last_confirmed_memory_revision,
        revision_frontier.memory_revision,
        revision_frontier.durable_revision,
    )
    .map(|mut response| {
        response.answer_mode = Some(durable_answer_mode);
        response
    })
}

/// Reports whether a client cursor is ahead of the authoritative event stream.
/// 判断客户端游标是否领先权威事件流。
fn reconnect_cursor_is_ahead(
    last_confirmed_seq: Option<u64>,
    authoritative_latest: Option<u64>,
) -> bool {
    match (last_confirmed_seq, authoritative_latest) {
        (Some(_), None) => true,
        (Some(confirmed), Some(latest)) => confirmed > latest,
        (None, _) => false,
    }
}

/// Resolves one authoritative open-Turn start across persisted and resident views.
/// 在持久化视图与驻留视图之间解析唯一权威的开放 Turn 起点。
fn authoritative_open_turn_start(
    persisted: Option<Seq>,
    resident: Option<Seq>,
) -> Result<Option<Seq>, String> {
    match (persisted, resident) {
        (Some(persisted), Some(resident)) if persisted != resident => Err(format!(
            "V1 reconnect open Turn start mismatch: persisted={}, resident={}",
            persisted.value(),
            resident.value()
        )),
        (Some(value), _) | (_, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

/// Selects the inclusive reconnect boundary, replaying an entire open Turn.
/// 选择包含式重连边界，并重放整个开放 Turn。
fn reconnect_from_value(
    last_confirmed_seq: Option<u64>,
    authoritative_latest: Option<u64>,
    open_turn_start_seq: Option<Seq>,
) -> Result<Option<u64>, String> {
    let Some(latest) = authoritative_latest else {
        return match last_confirmed_seq {
            Some(confirmed) => Err(format!(
                "lastConfirmedSeq {confirmed} is ahead of an empty V1 event stream"
            )),
            None => Ok(None),
        };
    };
    let confirmed = match last_confirmed_seq {
        Some(value) => checked_seq(value, "lastConfirmedSeq")?.value(),
        None => 1,
    };
    if confirmed > latest {
        return Err(format!(
            "lastConfirmedSeq {confirmed} is ahead of authoritative latest seq {latest}"
        ));
    }
    Ok(Some(
        open_turn_start_seq.map_or(confirmed, |start| confirmed.min(start.value())),
    ))
}

/// Resolves only the exact SQLite locator for a historical read.
/// 仅为历史读取解析精确 SQLite 定位器。
async fn resolve_exact_locator(
    state: &SharedServiceBridgeState,
    raw_session_id: &str,
) -> Result<(SessionId, SessionV1StorageLocator), String> {
    let (session_id, locator, _) =
        resolve_exact_locator_and_registry(state, raw_session_id).await?;
    Ok((session_id, locator))
}

/// Captures the controller and registry under one short service-state lock,
/// then resolves storage through SQLite only.
/// 在一次短服务状态锁内捕获控制器与注册表，随后仅通过 SQLite 解析存储。
async fn resolve_exact_locator_and_registry(
    state: &SharedServiceBridgeState,
    raw_session_id: &str,
) -> Result<
    (
        SessionId,
        SessionV1StorageLocator,
        std::sync::Arc<SessionRuntimeRegistry>,
    ),
    String,
> {
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
    let locator = match optional_registered_handle(&registry, session_id)? {
        Some(handle) => handle
            .storage_locator()
            .map_err(|error| format!("failed to read V1 session locator: {error}"))?,
        None => locate_session_v1_storage(&controller, raw_session_id)
            .await
            .map_err(|error| format!("failed to locate V1 session '{raw_session_id}': {error}"))?,
    };
    Ok((session_id, locator, registry))
}

/// Returns the unique registered handle, treating only the explicit
/// not-registered state as an inactive session.
/// 返回唯一已注册句柄，仅把显式未注册状态视为非活动会话。
fn optional_registered_handle(
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
) -> Result<Option<std::sync::Arc<SessionHandle>>, String> {
    match registry.get(session_id) {
        Ok(handle) => Ok(Some(handle)),
        Err(SessionRuntimeError::SessionNotRegistered(_)) => Ok(None),
        Err(error) => Err(format!("failed to read V1 session registry: {error}")),
    }
}

/// Captures one immutable read snapshot from the sole active handle.
/// 从唯一活动句柄捕获一份不可变读取快照。
fn capture_resident_snapshot(handle: &SessionHandle) -> Result<ResidentEventSnapshot, String> {
    handle
        .read(|document| ResidentEventSnapshot {
            window_generation: document.segment_generation(),
            memory_revision: document.memory_revision(),
            durable_revision: document.durable_revision(),
            events: document.events().to_vec(),
            open_turn_id: document.active_turn().map(|turn| turn.turn_id),
            open_turn_start_seq: document.active_turn().map(|turn| turn.start_seq),
            config: document.metadata.config.clone(),
        })
        .map_err(|error| format!("failed to read authoritative V1 session handle: {error}"))
}

/// Resolves the exact revision frontier shared by hydration and reconnect.
/// 解析 hydration 与 reconnect 共享的精确修订前沿。
///
/// # Parameters
/// 参数
/// `persisted_state` is the metadata frontier captured by the physical read;
/// `resident` is captured afterwards from the unique active owner when present.
/// `persisted_state` 是物理读取捕获的 metadata 前沿；`resident` 在其后从唯一
/// 活动 owner 捕获（如存在）。
///
/// # Returns
/// 返回值
/// Returns one same-generation frontier, rejecting a resident owner whose
/// durable revision is behind the already observed disk revision.
/// 返回一个同代次前沿；若驻留 owner 的可靠修订落后于已观察磁盘修订则拒绝。
fn authoritative_revision_frontier(
    service_epoch: &str,
    persisted_state: &SessionV1EventWindowState,
    resident: Option<&ResidentEventSnapshot>,
) -> Result<ConversationEventRevisionFrontierV1, String> {
    let Some(resident) = resident else {
        return ConversationEventRevisionFrontierV1::try_new(
            service_epoch,
            persisted_state.document_revision,
            persisted_state.document_revision,
        );
    };
    require_window_generation(
        persisted_state.segment_generation,
        resident.window_generation,
    )?;
    if resident.durable_revision < persisted_state.document_revision {
        return Err(format!(
            "authoritative resident durable revision {} is behind persisted revision {}",
            resident.durable_revision, persisted_state.document_revision
        ));
    }
    ConversationEventRevisionFrontierV1::try_new(
        service_epoch,
        resident.memory_revision,
        resident.durable_revision,
    )
}

/// Overlays the active resident tail over a complete persisted range while
/// rejecting generation drift, gaps, or an active snapshot behind disk.
/// 用活动驻留尾段覆盖完整持久化范围，同时拒绝代次漂移、缺口或落后磁盘的活动快照。
// The event projection returns one atomic snapshot with its revision, turn, sequence and configuration.
// 事件投影一次返回包含修订、回合、序号及配置的原子快照。
#[expect(
    clippy::type_complexity,
    reason = "The event projection returns one atomic snapshot with its revision, turn, sequence and configuration."
)]
fn merge_persisted_with_resident(
    persisted_state: SessionV1EventWindowState,
    persisted_events: Vec<ConversationEvent>,
    resident: Option<ResidentEventSnapshot>,
    boundary: &str,
) -> Result<
    (
        u64,
        Vec<ConversationEvent>,
        Option<TurnId>,
        Option<Seq>,
        SessionConfig,
    ),
    String,
> {
    validate_contiguous_events(&persisted_events, boundary)?;
    let Some(resident) = resident else {
        return Ok((
            persisted_state.segment_generation,
            persisted_events,
            None,
            persisted_state.open_turn_start_seq,
            persisted_state.config,
        ));
    };
    require_window_generation(
        persisted_state.segment_generation,
        resident.window_generation,
    )?;
    validate_contiguous_events(&resident.events, "resident handle")?;
    if resident.events.is_empty() {
        return Ok((
            resident.window_generation,
            persisted_events,
            resident.open_turn_id,
            resident.open_turn_start_seq,
            resident.config,
        ));
    }
    if let (Some(persisted_last), Some(resident_last)) =
        (persisted_events.last(), resident.events.last())
        && resident_last.seq < persisted_last.seq
    {
        return Err(format!(
            "authoritative resident tail ends at seq {} behind persisted {boundary} seq {}",
            resident_last.seq.value(),
            persisted_last.seq.value()
        ));
    }
    let resident_first = resident.events[0].seq.value();
    let mut merged: Vec<ConversationEvent> = persisted_events
        .into_iter()
        .take_while(|event| event.seq.value() < resident_first)
        .collect();
    merged.extend(resident.events);
    validate_contiguous_events(&merged, boundary)?;
    Ok((
        resident.window_generation,
        merged,
        resident.open_turn_id,
        resident.open_turn_start_seq,
        resident.config,
    ))
}

/// Validates strict ascending seq continuity without sorting or repairing the
/// source order.
/// 在不排序或修复来源顺序的情况下校验严格升序 seq 连续性。
fn validate_contiguous_events(events: &[ConversationEvent], boundary: &str) -> Result<(), String> {
    for pair in events.windows(2) {
        let expected = pair[0].seq.value().saturating_add(1);
        let actual = pair[1].seq.value();
        if expected != actual {
            return Err(format!(
                "V1 {boundary} event gap or disorder: expected seq {expected}, got {actual}"
            ));
        }
    }
    Ok(())
}

/// Returns the open Turn identity from the exact start event in one complete
/// window.
/// 从一个完整窗口中的精确起始事件返回开放 Turn 身份。
fn open_turn_id_from_events(
    events: &[ConversationEvent],
    open_turn_start_seq: Option<Seq>,
) -> Option<TurnId> {
    let start_seq = open_turn_start_seq?;
    events
        .iter()
        .find(|event| event.seq == start_seq)
        .and_then(|event| event.turn_id)
}

/// Converts one raw JSON integer into a validated JavaScript-safe seq.
/// 把一个原始 JSON 整数转换为经过校验且 JavaScript 安全的 seq。
fn checked_seq(value: u64, field: &str) -> Result<Seq, String> {
    Seq::try_new(value).map_err(|error| format!("invalid {field} {value}: {error}"))
}

/// Rejects delayed responses or stale callers owned by another persisted event
/// generation.
/// 拒绝属于其他持久化事件代次的延迟响应或过期调用方。
fn require_window_generation(expected: u64, actual: u64) -> Result<(), String> {
    if expected == actual {
        return Ok(());
    }
    Err(format!(
        "conversation event window generation mismatch: expected {expected}, got {actual}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_session_core::v1::document::{ClientInputSubmission, SessionDocument};
    use vulcan_session_core::v1::ids::ClientActionId;

    /// Builds one short canonical event stream for merge tests.
    /// 为合并测试构建一段简短的 canonical 事件流。
    fn event_stream() -> (Vec<ConversationEvent>, u64, u64) {
        let mut document = SessionDocument::new("session", "project", "title", 1);
        document
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("conversation-events-v1-root").unwrap(),
                    "hello",
                    Vec::new(),
                ),
                1,
            )
            .unwrap();
        let candidate = document
            .prepare_flush_candidates()
            .unwrap()
            .expect("root input must produce a persisted fixture candidate");
        document.advance_durable(candidate.revision).unwrap();
        (
            document.events().to_vec(),
            document.memory_revision(),
            document.durable_revision(),
        )
    }

    /// Verifies the active handle replaces the persisted same-seq event rather
    /// than appending a duplicate or preserving stale state.
    /// 验证活动句柄会替换持久化同 seq 事件，而不是追加重复项或保留旧状态。
    #[test]
    fn resident_tail_overlays_same_seq_without_reordering() {
        let (persisted, memory_revision, durable_revision) = event_stream();
        let resident = ResidentEventSnapshot {
            window_generation: 1,
            memory_revision,
            durable_revision,
            events: persisted.clone(),
            open_turn_id: persisted[0].turn_id,
            open_turn_start_seq: Some(persisted[0].seq),
            config: SessionConfig::default(),
        };
        let (_, merged, _, _, _) = merge_persisted_with_resident(
            SessionV1EventWindowState {
                segment_generation: 1,
                document_revision: durable_revision,
                next_seq: 2,
                open_turn_start_seq: Some(persisted[0].seq),
                config: SessionConfig::default(),
            },
            persisted,
            Some(resident),
            "test",
        )
        .unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].seq.value(), 1);
    }

    /// Verifies a resident gap is surfaced instead of silently sorted or
    /// patched around.
    /// 验证驻留缺口会显式暴露，而不是被静默排序或绕过修补。
    #[test]
    fn resident_gap_is_rejected() {
        let (persisted, memory_revision, durable_revision) = event_stream();
        let mut resident_events = persisted.clone();
        resident_events[0].seq = Seq::try_new(3).unwrap();
        let error = merge_persisted_with_resident(
            SessionV1EventWindowState {
                segment_generation: 1,
                document_revision: durable_revision,
                next_seq: 2,
                open_turn_start_seq: Some(persisted[0].seq),
                config: SessionConfig::default(),
            },
            persisted,
            Some(ResidentEventSnapshot {
                window_generation: 1,
                memory_revision,
                durable_revision,
                events: resident_events,
                open_turn_id: None,
                open_turn_start_seq: None,
                config: SessionConfig::default(),
            }),
            "test",
        )
        .unwrap_err();
        assert!(error.contains("expected seq 2, got 3"));
    }

    /// Persisted-only hydration exposes one equal memory/durable baseline.
    /// 仅持久化 hydration 暴露相等的内存与可靠修订基线。
    #[test]
    fn persisted_only_frontier_uses_the_exact_document_revision() {
        let persisted_state = SessionV1EventWindowState {
            segment_generation: 1,
            document_revision: 7,
            next_seq: 2,
            open_turn_start_seq: None,
            config: SessionConfig::default(),
        };
        let frontier =
            authoritative_revision_frontier("test-service-epoch", &persisted_state, None)
                .expect("persisted-only revision frontier");
        assert_eq!(frontier.service_epoch, "test-service-epoch");
        assert_eq!(frontier.memory_revision, 7);
        assert_eq!(frontier.durable_revision, 7);
    }

    /// A resident volatile revision extends, but never rewrites, the persisted baseline.
    /// 驻留易失修订只扩展而不改写持久化基线。
    #[test]
    fn resident_frontier_preserves_volatile_and_durable_revisions() {
        let persisted_state = SessionV1EventWindowState {
            segment_generation: 1,
            document_revision: 7,
            next_seq: 2,
            open_turn_start_seq: None,
            config: SessionConfig::default(),
        };
        let resident = ResidentEventSnapshot {
            window_generation: 1,
            memory_revision: 9,
            durable_revision: 7,
            events: Vec::new(),
            open_turn_id: None,
            open_turn_start_seq: None,
            config: SessionConfig::default(),
        };
        let frontier = authoritative_revision_frontier(
            "test-service-epoch",
            &persisted_state,
            Some(&resident),
        )
        .expect("resident volatile revision frontier");
        assert_eq!(frontier.memory_revision, 9);
        assert_eq!(frontier.durable_revision, 7);
    }

    /// A stale resident owner cannot override a newer physical metadata snapshot.
    /// 过期驻留 owner 不得覆盖更新的物理 metadata 快照。
    #[test]
    fn resident_frontier_behind_disk_is_rejected() {
        let persisted_state = SessionV1EventWindowState {
            segment_generation: 1,
            document_revision: 8,
            next_seq: 2,
            open_turn_start_seq: None,
            config: SessionConfig::default(),
        };
        let resident = ResidentEventSnapshot {
            window_generation: 1,
            memory_revision: 8,
            durable_revision: 7,
            events: Vec::new(),
            open_turn_id: None,
            open_turn_start_seq: None,
            config: SessionConfig::default(),
        };
        let error = authoritative_revision_frontier(
            "test-service-epoch",
            &persisted_state,
            Some(&resident),
        )
        .expect_err("resident behind disk must fail");
        assert!(error.contains("behind persisted revision"));
    }

    /// An open Turn replays from its start even when the client confirmed a later parallel tool.
    /// 即使客户端已确认更晚的并行工具，开放 Turn 仍从其起点重放。
    #[test]
    fn reconnect_replays_the_complete_open_turn() {
        let from = reconnect_from_value(Some(12), Some(12), Some(Seq::try_new(7).unwrap()))
            .expect("open Turn reconnect boundary");
        assert_eq!(from, Some(7));
    }

    /// A closed Turn repeats only the confirmed boundary and its missing suffix.
    /// 已关闭 Turn 只重复确认边界及其缺失后缀。
    #[test]
    fn reconnect_closed_turn_repeats_confirmed_boundary() {
        let from =
            reconnect_from_value(Some(12), Some(15), None).expect("closed Turn reconnect boundary");
        assert_eq!(from, Some(12));
    }

    /// Persisted and resident open-Turn cursors must describe one physical boundary.
    /// 持久化与驻留开放 Turn 游标必须描述同一物理边界。
    #[test]
    fn reconnect_rejects_open_turn_cursor_conflict() {
        let error = authoritative_open_turn_start(
            Some(Seq::try_new(7).unwrap()),
            Some(Seq::try_new(8).unwrap()),
        )
        .expect_err("mismatched open Turn cursors must fail");
        assert!(error.contains("open Turn start mismatch"));
    }
}
