//! Authoritative V1 conversation edit and fork generation services.
//! 权威 V1 会话编辑与分叉 generation 服务。

use std::sync::Arc;

use vulcan_service_api::{
    ConversationEditGenerationV1Request, ConversationEditGenerationV1Response,
    ConversationForkGenerationV1Request, ConversationForkGenerationV1Response, ServiceEventEmitter,
};
use vulcan_session_core::v1::document::{EditGenerationInput, RootInputReplacement};
use vulcan_session_core::v1::event::{EventBody, UserInputSource};
use vulcan_session_core::v1::ids::Seq;
use vulcan_workbench_model::SessionId;

use crate::runtime_workspace_helpers::runtime_parts_from_state;
use crate::session_goal_lifecycle::current_v1_goal_unix_millis;
use crate::session_runtime_registry_v1::{SessionHandle, SessionRuntimeRegistry};
use crate::session_v1_lifecycle::{
    CreateForkSessionV1Request, create_or_get_forked_session_v1, get_or_load_session_v1,
};
use crate::state::SharedServiceBridgeState;
use crate::webui_turn_chaining::start_exact_v1_user_root_turn;
use crate::workbench_refresh::refresh_workbench_snapshot_for_selected_session;

/// Publishes one edited V1 history generation and starts its exact rewritten root Turn.
/// 发布一份编辑后的 V1 历史 generation，并启动其精确重写根回合。
///
/// # Parameters
/// 参数
/// `app` emits the exact downstream workbench changes, `state` owns runtime authorities, and
/// `request` carries the source generation CAS boundary plus replacement text.
/// `app` 发出精确的下游工作台变更，`state` 持有运行时权威，`request` 携带源 generation CAS
/// 边界与替换正文。
///
/// # Returns
/// 返回值
/// Returns only after the edited generation is durable, strictly reloaded, and physically admitted.
/// 仅在编辑 generation 已可靠落盘、严格重载并完成物理准入后返回。
pub(crate) async fn service_edit_conversation_generation_v1(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: ConversationEditGenerationV1Request,
) -> Result<ConversationEditGenerationV1Response, String> {
    let session_id = parse_session_id(&request.session_id)?;
    let target_seq = checked_seq(request.target_seq, "targetSeq")?;
    let (controller, agent_runtime, _, _, _) = runtime_parts_from_state(&state)?;
    let registry = session_registry_from_state(&state)?;
    let handle = get_or_load_session_v1(&controller, registry.as_ref(), session_id)
        .await
        .map_err(|error| error.to_string())?;
    let ordered_resource_ids = root_input_resource_ids(handle.as_ref(), target_seq)?;
    let receipt = handle
        .publish_edit_generation(
            request.expected_window_generation,
            EditGenerationInput {
                target_seq,
                replacement: RootInputReplacement::new(
                    request.replacement_text,
                    ordered_resource_ids,
                ),
                now_ms: current_v1_goal_unix_millis()?,
            },
        )
        .map_err(|error| error.to_string())?;
    // The edited generation is now the durable session authority. Refresh the retained process
    // projection before physical admission so the exact new root is never checked against the
    // pre-edit Workbench generation.
    // 编辑后的 generation 现已成为可靠会话权威。物理准入前先刷新保留的进程投影，确保精确新根
    // 不会与编辑前的 Workbench generation 进行校验。
    refresh_workbench_snapshot_for_selected_session(state.clone(), session_id).await?;
    let dispatch_started = start_exact_generation_root(
        &app,
        &state,
        &controller,
        &agent_runtime,
        handle.as_ref(),
        target_seq,
    )
    .await?;
    Ok(ConversationEditGenerationV1Response {
        session_id: session_id.to_string(),
        target_seq: receipt.target_seq.value(),
        window_generation: receipt.segment_generation,
        document_revision: receipt.document_revision,
        dispatch_started,
    })
}

/// Creates or exactly replays one immutable V1 fork and starts its retained root Turn once.
/// 创建或精确重放一个不可变 V1 分叉，并且只启动一次其保留的根回合。
///
/// # Parameters
/// 参数
/// `request` carries the immutable parent generation, target seq, stable child id, and child title.
/// `request` 携带不可变父 generation、目标 seq、稳定子会话 ID 与子会话标题。
///
/// # Returns
/// 返回值
/// Returns the exact child generation and dispatch state after indexed publication or immutable retry.
/// 在索引发布或不可变重试后返回精确的子 generation 与分发状态。
pub(crate) async fn service_fork_conversation_generation_v1(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: ConversationForkGenerationV1Request,
) -> Result<ConversationForkGenerationV1Response, String> {
    let source_session_id = parse_session_id(&request.source_session_id)?;
    let child_session_id = parse_session_id(&request.child_session_id)?;
    let target_seq = checked_seq(request.target_seq, "targetSeq")?;
    let (controller, agent_runtime, _, _, _) = runtime_parts_from_state(&state)?;
    let registry = session_registry_from_state(&state)?;
    let created = create_or_get_forked_session_v1(
        &controller,
        registry.as_ref(),
        CreateForkSessionV1Request {
            source_session_id,
            expected_source_generation: request.expected_window_generation,
            child_session_id,
            title: request.title,
            target_seq,
            now_ms: current_v1_goal_unix_millis()?,
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    // The child is already durable and indexed, but physical admission consumes the retained
    // process projection. Refresh that exact child before asking the admission adapter to start it.
    // 子会话已可靠落盘并进入索引，但物理准入消费保留的进程投影；因此必须先刷新该精确子会话，再请求准入适配器启动。
    refresh_workbench_snapshot_for_selected_session(state.clone(), child_session_id).await?;
    let dispatch_started = start_exact_generation_root(
        &app,
        &state,
        &controller,
        &agent_runtime,
        created.handle.as_ref(),
        created.source_target_seq,
    )
    .await?;
    let (window_generation, document_revision) = created
        .handle
        .read(|document| {
            (
                document.metadata.segment_generation,
                document.metadata.document_revision,
            )
        })
        .map_err(|error| error.to_string())?;
    Ok(ConversationForkGenerationV1Response {
        source_session_id: source_session_id.to_string(),
        child_session_id: child_session_id.to_string(),
        created: created.created,
        source_target_seq: created.source_target_seq.value(),
        window_generation,
        document_revision,
        dispatch_started,
    })
}

/// Reads the ordered project resources from one exact root-input event.
/// 从一条精确根输入事件读取有序项目资源。
fn root_input_resource_ids(
    handle: &SessionHandle,
    target_seq: Seq,
) -> Result<Vec<vulcan_session_core::v1::ids::ResourceId>, String> {
    handle
        .read(|document| {
            let event = document
                .events()
                .iter()
                .find(|event| event.seq == target_seq)
                .ok_or_else(|| {
                    String::from("The edit target seq is missing from the V1 ledger.")
                })?;
            let EventBody::UserInput(input) = &event.event else {
                return Err(String::from(
                    "The edit target is not a V1 user-input event.",
                ));
            };
            if input.input_source != UserInputSource::Root {
                return Err(String::from("The edit target is not a root user input."));
            }
            Ok(input.resource_ids.clone())
        })
        .map_err(|error| error.to_string())?
}

/// Starts an undispatched generation root or accepts its exact durable prior dispatch.
/// 启动尚未分发的 generation 根回合，或接受其精确可靠的既有分发事实。
async fn start_exact_generation_root(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller: &vulcan_workspace_store::data_plane::ControllerRuntimeHandle,
    agent_runtime: &crate::AgentRuntimeBridge,
    handle: &SessionHandle,
    target_seq: Seq,
) -> Result<bool, String> {
    let (session_id, turn_id, already_dispatched) = handle
        .read(|document| {
            let target_event = document
                .events()
                .iter()
                .find(|event| event.seq == target_seq)
                .ok_or_else(|| String::from("The generation root seq is missing."))?;
            let EventBody::UserInput(input) = &target_event.event else {
                return Err(String::from(
                    "The generation root is not a V1 user-input event.",
                ));
            };
            if input.input_source != UserInputSource::Root {
                return Err(String::from(
                    "The generation root is not a root user input.",
                ));
            }
            let turn_id = target_event
                .turn_id
                .ok_or_else(|| String::from("The generation root has no Turn identity."))?;
            let already_dispatched = document.events().iter().any(|event| {
                event.turn_id == Some(turn_id)
                    && event.seq.value() > target_seq.value()
                    && matches!(event.event, EventBody::ApiStarted(_))
            });
            if !already_dispatched {
                let active_turn = document.active_turn().ok_or_else(|| {
                    String::from("The undispatched generation root has no open Turn.")
                })?;
                if active_turn.closed
                    || active_turn.turn_id != turn_id
                    || active_turn.start_seq != target_seq
                {
                    return Err(String::from(
                        "The undispatched generation root is not the authoritative open Turn.",
                    ));
                }
            }
            Ok((
                document.metadata.session_id.clone(),
                turn_id,
                already_dispatched,
            ))
        })
        .map_err(|error| error.to_string())??;
    let parsed_session_id = parse_session_id(&session_id)?;
    if already_dispatched {
        return Ok(true);
    }
    let started = start_exact_v1_user_root_turn(
        app,
        state,
        controller,
        agent_runtime,
        parsed_session_id,
        target_seq,
    )
    .await?;
    if !started {
        return Err(format!(
            "The exact V1 generation root Turn {} was not physically admitted.",
            turn_id.value()
        ));
    }
    Ok(true)
}

/// Clones the unique process-local V1 registry without retaining the service-state lock.
/// 克隆唯一的进程内 V1 注册表，且不保留服务状态锁。
fn session_registry_from_state(
    state: &SharedServiceBridgeState,
) -> Result<Arc<SessionRuntimeRegistry>, String> {
    state
        .lock()
        .map(|guard| Arc::clone(&guard.session_runtime_registry))
        .map_err(|error| format!("failed to lock service state: {error}"))
}

/// Parses one strict application session identity.
/// 解析一个严格的应用会话身份。
fn parse_session_id(raw_session_id: &str) -> Result<SessionId, String> {
    raw_session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{raw_session_id}': {error}"))
}

/// Converts one JSON-safe raw seq into the strong V1 identity.
/// 把一个 JSON 安全的原始 seq 转换为强类型 V1 身份。
fn checked_seq(raw_seq: u64, field: &str) -> Result<Seq, String> {
    Seq::try_new(raw_seq).map_err(|error| format!("invalid {field} {raw_seq}: {error}"))
}
