//! Authoritative service adapter for typed agent collaboration commands.
//! 类型化代理协作命令的权威服务适配层。

use async_trait::async_trait;
use serde_json::{Value, json};
use std::fmt;
use std::sync::{Arc, Mutex, Weak};
use uuid::Uuid;
use vulcan_agent_runtime::agent_runtime::task_runtime::collaboration_lifecycle::{
    ChildTaskActivationRequest, ChildTaskLifecycleReceipt, ChildTaskReservationRequest,
    ChildTaskTerminalRequest,
};
use vulcan_agent_runtime::tooling::collaboration::{
    ASK_PARENT_TOOL_ID, ASK_QUESTION_TOOL_ID, AgentCollaborationCommand, CANCEL_QUESTION_TOOL_ID,
    LIST_SESSIONS_TOOL_ID, NativeAgentCollaborationRuntime, READ_SESSION_TOOL_ID,
    REPLY_QUESTION_TOOL_ID, SEND_NOTIFICATION_TOOL_ID, SEND_SESSION_MESSAGE_TOOL_ID,
    WAIT_AGENT_TOOL_ID, WAIT_QUESTION_TOOL_ID, WAIT_SESSION_TOOL_ID,
};
use vulcan_session_core::v1::collaboration::{
    AcceptDeliveryInput, AcknowledgeDeliverySettlementInput, ActivateChildTaskInput,
    CancelQuestionInput, CollaborationAnswerDeliveryReceipt, CollaborationChildTaskReceipt,
    CollaborationChildTaskTerminalOutcome, CollaborationDeliveryFailureReason,
    CollaborationDeliveryId, CollaborationDeliveryReceipt, CollaborationDeliverySettlementReceipt,
    CollaborationDeliveryState, CollaborationExecutionId, CollaborationExecutionPurpose,
    CollaborationGrantAction, CollaborationOperationId, CollaborationParticipant,
    CollaborationQuestionExecutionState, CollaborationQuestionId, CollaborationQuestionState,
    CollaborationRunControl, CollaborationTargetSettlementState, CreateQuestionInput,
    DeliverAnswerInput, ProjectDeliveryInput, RecordDeliveryProgressInput, ReplyQuestionInput,
    ReserveChildTaskInput, SettleInboundDeliveryInput, TerminalChildTaskInput,
};
use vulcan_session_core::v1::document::IdempotentMutation;
use vulcan_session_core::v1::document::TurnOrigin;
use vulcan_session_core::v1::event::EventBody;
use vulcan_session_core::v1::ids::{Seq, SubagentTaskId, TurnId};
use vulcan_session_core::v1::{ApiCallId, ApiPurposeKind};
use vulcan_tools_manager::VulcanToolInvocationContext;
use vulcan_workspace_store::workspace_store::{
    SessionV1EventRange, SessionV1ListEntry, SessionV1StorageLocator, list_active_sessions_v1,
    list_archived_sessions_v1, load_session_v1_for_runtime, locate_embedded_task_session_v1,
    locate_optional_session_v1_storage,
};

use crate::session_runtime_registry_v1::{
    SessionConditionalMutationReceipt, SessionFlushReason, SessionHandle, SessionMutationAdmission,
    SessionMutationChange, SessionRuntimeError, SessionRuntimeRegistry,
};
use crate::state::{ServiceBridgeState, SharedServiceBridgeState};

mod child_compaction;
mod child_creation;
pub(crate) mod child_execution_ownership;
pub(crate) mod child_maintenance;
mod child_profile;
mod child_question_admission;
mod child_question_launch;
mod child_question_request;
#[cfg(test)]
mod execution_binding_tests;
mod launch;
pub(crate) mod lifecycle_write;
#[cfg(test)]
mod native_dispatch_tests;
mod notifications;
mod participants;
mod projection;
mod session_messages;
mod session_wait;
mod startup_recovery;
mod task_delivery;
mod task_launch;
mod task_results;
#[cfg(test)]
pub(crate) mod test_support;
pub(crate) use projection::project_collaboration_before_api;
pub(crate) use startup_recovery::start_collaboration_recovery;
pub(crate) use task_launch::wake_session_tasks_if_idle;
pub(crate) use task_results::reconcile_task_results_for_session;

/// Service-owned native collaboration runtime that never owns the shared service state.
/// 服务持有的原生协作运行时，绝不拥有共享服务状态的强引用。
#[derive(Clone)]
struct SessionV1NativeAgentCollaborationRuntime {
    /// Weak state ownership avoids a request-chain to service-state reference cycle.
    /// 弱状态归属避免请求链与服务状态之间形成强引用循环。
    state: Weak<Mutex<ServiceBridgeState>>,
    /// Existing event channel used after durable work has completed.
    /// 可靠工作完成后使用的现有事件通道。
    app: crate::ServiceEventEmitter,
}

impl fmt::Debug for SessionV1NativeAgentCollaborationRuntime {
    /// Renders a non-sensitive adapter label without exposing state or message text.
    /// 渲染不暴露状态或消息正文的不敏感适配器标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionV1NativeAgentCollaborationRuntime")
            .finish_non_exhaustive()
    }
}

/// Builds the weakly bound service runtime injected into provider tool execution.
/// 构建注入 Provider 工具执行、使用弱状态绑定的服务运行时。
///
/// # Parameters
/// 参数
/// `state` is the shared service owner whose registry is resolved per invocation.
/// `state` 是每次调用时解析注册表的共享服务 owner。
/// `app` is the existing service event emitter used only after durable commits.
/// `app` 是仅在可靠提交后使用的现有服务事件发送器。
///
/// # Returns
/// 返回值
/// An `Arc` implementing the frozen native collaboration port.
/// 实现已冻结原生协作端口的 `Arc`。
pub(crate) fn session_v1_native_agent_collaboration_runtime(
    state: SharedServiceBridgeState,
    app: crate::ServiceEventEmitter,
) -> Arc<dyn NativeAgentCollaborationRuntime> {
    Arc::new(SessionV1NativeAgentCollaborationRuntime {
        state: Arc::downgrade(&state),
        app,
    })
}

/// Validated identity of the exact persisted ToolCall used as the service source.
/// 作为服务来源的精确持久化 ToolCall 的已校验身份。
#[derive(Clone)]
struct ValidatedInvocation {
    /// Upgraded shared state kept only for this operation.
    /// 仅为本次操作暂时升级的共享状态。
    state: SharedServiceBridgeState,
    /// Controller runtime used by indexed SQLite locator operations.
    /// 用于索引 SQLite 定位操作的控制器运行时。
    controller_runtime: vulcan_workspace_store::data_plane::HostRuntimeHandle,
    /// Unique V1 session registry used to resolve canonical handles.
    /// 用于解析规范句柄的唯一 V1 会话注册表。
    registry: Arc<SessionRuntimeRegistry>,
    /// Exact source session id from the trusted invocation context.
    /// 来自可信调用上下文的精确来源会话 ID。
    source_session_id: Uuid,
    /// Canonical registered source handle.
    /// 规范的已注册来源句柄。
    source_handle: Arc<SessionHandle>,
    /// Exact source locator owned by the canonical handle.
    /// 规范句柄持有的精确来源定位器。
    source_locator: SessionV1StorageLocator,
    /// Exact source turn recovered from the ToolCall event.
    /// 从 ToolCall 事件恢复的精确来源回合。
    source_turn_id: u64,
    /// Exact physical API admitted for a model tool; host recovery has no model invocation.
    /// 模型工具已获准的精确物理 API；宿主恢复没有模型调用。
    source_api_call_id: Option<ApiCallId>,
    /// Stable tool identity retained for operation diagnostics and binding.
    /// 为操作诊断和绑定保留的稳定工具身份。
    source_tool_id: String,
    /// Deterministic operation identity derived only from trusted context fields.
    /// 仅由可信上下文字段派生的确定性操作身份。
    operation_id: String,
}

/// Holds one exact target locator and the canonical process-local handle.
/// 保存一个精确目标定位器与进程内规范句柄。
#[derive(Clone)]
struct ResolvedSession {
    /// Indexed or parent-derived storage locator validated by the workspace store.
    /// 由 workspace store 校验的索引或父定位派生存储定位器。
    locator: SessionV1StorageLocator,
    /// Canonical session handle used for all durable document mutations.
    /// 所有可靠文档变更使用的规范会话句柄。
    handle: Arc<SessionHandle>,
}

/// Converts the host clock into the nonnegative V1 timestamp domain.
/// 将宿主时钟转换为非负 V1 时间戳域。
fn now_ms() -> Result<i64, String> {
    i64::try_from(crate::current_unix_millis_value())
        .map_err(|_| String::from("current Unix millisecond timestamp exceeds V1 range"))
}

/// Builds a checked operation identity from the exact tool invocation context.
/// 从精确工具调用上下文构造经过校验的操作身份。
fn collaboration_operation_id(
    invocation: &ValidatedInvocation,
) -> Result<CollaborationOperationId, String> {
    CollaborationOperationId::try_new(invocation.operation_id.clone())
        .map_err(|error| format!("invalid collaboration operation identity: {error}"))
}

/// Builds a question id that locates its source session without a global history scan.
/// 构造无需扫描全局历史即可定位来源会话的问题 ID。
fn collaboration_question_id(
    invocation: &ValidatedInvocation,
) -> Result<CollaborationQuestionId, String> {
    CollaborationQuestionId::try_new(format!("cq1/{}", invocation.operation_id))
        .map_err(|error| format!("invalid collaboration question identity: {error}"))
}

/// Builds the stable delivery id paired with one source question operation.
/// 构造与来源问题操作配对的稳定投递 ID。
fn collaboration_delivery_id(
    invocation: &ValidatedInvocation,
) -> Result<vulcan_session_core::v1::collaboration::CollaborationDeliveryId, String> {
    vulcan_session_core::v1::collaboration::CollaborationDeliveryId::try_new(format!(
        "cd1/{}",
        invocation.operation_id
    ))
    .map_err(|error| format!("invalid collaboration delivery identity: {error}"))
}

/// Extends one trusted operation identity for an independent durable boundary.
/// 为独立可靠边界扩展一条可信操作身份。
fn operation_suffix(
    invocation: &ValidatedInvocation,
    suffix: &str,
) -> Result<CollaborationOperationId, String> {
    CollaborationOperationId::try_new(format!("{}:{suffix}", invocation.operation_id))
        .map_err(|error| format!("invalid collaboration boundary operation identity: {error}"))
}

/// Prefix reserved for host-local user cancellation requests.
/// 仅供宿主本地用户撤销请求使用的操作身份前缀。
const USER_QUESTION_CANCEL_OPERATION_PREFIX: &str = "user-question-cancel:v1:";

/// Parses the source session embedded in a service-issued question id.
/// 解析服务签发的问题 ID 中嵌入的来源会话。
fn question_source_session_id(question_id: &str) -> Result<Uuid, String> {
    let operation = question_id
        .strip_prefix("cq1/")
        .ok_or_else(|| String::from("question_id is not a service-issued collaboration id"))?;
    let mut parts = operation.splitn(3, ':');
    if parts.next() != Some("v1") {
        return Err(String::from("question_id has an invalid operation version"));
    }
    let session_id = parts
        .next()
        .ok_or_else(|| String::from("question_id has no source session identity"))?;
    if parts.next().is_none() {
        return Err(String::from("question_id has no source operation identity"));
    }
    Uuid::parse_str(session_id)
        .map_err(|error| format!("question_id source session identity is invalid: {error}"))
}

/// Converts one Core idempotent result into the registry mutation decision.
/// 将 Core 幂等结果转换为注册表修改判定。
fn session_change<T>(mutation: IdempotentMutation<T>) -> SessionMutationChange<T> {
    match mutation {
        IdempotentMutation::Committed(value) => SessionMutationChange::Changed(value),
        IdempotentMutation::Idempotent(value) => SessionMutationChange::Unchanged(value),
    }
}

/// Reads the exact current V1 generation from a validated locator.
/// 从已校验定位器读取精确当前 V1 代际。
fn locator_generation(locator: &SessionV1StorageLocator) -> Result<u64, String> {
    locator
        .event_window_state()
        .map(|state| state.segment_generation)
        .map_err(|error| format!("failed to read session event generation: {error}"))
}

/// Selects the target question purpose from the currently open target turn.
/// 根据目标当前开放回合选择问题执行用途。
fn target_question_binding(
    document: &vulcan_session_core::v1::document::SessionDocument,
) -> (CollaborationExecutionPurpose, Option<TurnId>) {
    match document.active_turn().filter(|turn| !turn.closed) {
        Some(turn) if !matches!(turn.origin, TurnOrigin::QuestionExecution) => (
            CollaborationExecutionPurpose::InTurnQuestion,
            Some(turn.turn_id),
        ),
        None => (CollaborationExecutionPurpose::NaturallyIdleQuestion, None),
        // A running auxiliary question turn is already owned by another durable execution.
        // 正在运行的辅助提问回合已由另一条持久执行记录拥有。
        Some(_) => (CollaborationExecutionPurpose::NaturallyIdleQuestion, None),
    }
}

/// Validates the admitted API, target identity and turn against the question; returns an error for stale ownership.
/// 对照问题校验已准入 API、目标身份和回合；所有权过期时返回错误。
fn prove_active_question_execution_binding(
    document: &vulcan_session_core::v1::document::SessionDocument,
    question: &vulcan_session_core::v1::collaboration::CollaborationQuestionRecord,
    actual_participant: &CollaborationParticipant,
    actual_turn_id: TurnId,
    actual_api_call_id: ApiCallId,
) -> Result<(), String> {
    let inbound = document
        .collaboration_state()
        .inbound_deliveries
        .get(&question.delivery_id)
        .filter(|delivery| {
            delivery.delivery_id == question.delivery_id
                && delivery.question_id == question.question_id
                && delivery.source == question.source
                && delivery.target == question.target
                && matches!(
                    delivery.state,
                    CollaborationDeliveryState::Accepted | CollaborationDeliveryState::Projected
                )
        })
        .ok_or_else(|| String::from("question target has no accepted durable delivery"))?;
    if inbound.execution_purpose != question.execution_purpose
        || inbound.bound_turn_id != question.target.turn_id
    {
        return Err(String::from(
            "question target delivery execution binding does not match the question",
        ));
    }
    if actual_participant.session_id != question.target.session_id
        || actual_participant.root_session_id != question.target.root_session_id
        || actual_participant.project_id != question.target.project_id
        || actual_participant.segment_generation != question.target.segment_generation
        || actual_participant.task_id != question.target.task_id
    {
        return Err(String::from(
            "question target caller does not match the durable participant identity",
        ));
    }
    let active_turn = document
        .active_turn()
        .filter(|turn| !turn.closed)
        .ok_or_else(|| String::from("question target has no open execution turn"))?;
    if active_turn.turn_id != actual_turn_id {
        return Err(String::from(
            "question target caller turn does not match the active execution turn",
        ));
    }
    // Recheck after asynchronous source lookup: the same logical turn can already own a different API.
    // 异步读取来源后再次校验：同一逻辑回合可能已经归属另一条物理 API。
    let active_api = document
        .active_api()
        .ok_or_else(|| String::from("question target has no active model API"))?;
    if active_api.api_call_id != actual_api_call_id || active_api.turn_id != actual_turn_id {
        return Err(String::from(
            "question target model API ownership has changed",
        ));
    }
    if active_api.purpose == ApiPurposeKind::Compaction {
        return Err(String::from(
            "compaction API cannot answer a collaboration question",
        ));
    }
    let expected_parent_turn = match question.execution_purpose {
        CollaborationExecutionPurpose::InTurnQuestion => {
            if question.target.turn_id != Some(actual_turn_id)
                || inbound.bound_turn_id != Some(actual_turn_id)
                || matches!(active_turn.origin, TurnOrigin::QuestionExecution)
            {
                return Err(String::from(
                    "in-turn question execution is not bound to the current parent turn",
                ));
            }
            Some(actual_turn_id)
        }
        CollaborationExecutionPurpose::NaturallyIdleQuestion => {
            if question.target.turn_id.is_some()
                || inbound.bound_turn_id.is_some()
                || !matches!(active_turn.origin, TurnOrigin::QuestionExecution)
                || active_api.purpose != ApiPurposeKind::QuestionExecution
            {
                return Err(String::from(
                    "naturally idle question execution is not bound to its auxiliary turn",
                ));
            }
            None
        }
        CollaborationExecutionPurpose::NormalTask => {
            return Err(String::from(
                "normal task execution cannot answer a collaboration question",
            ));
        }
    };
    let execution_count = document
        .collaboration_state()
        .question_executions
        .values()
        .filter(|execution| {
            execution.question_id == question.question_id
                && execution.participant == question.target
                && execution.purpose == question.execution_purpose
                && execution.state == CollaborationQuestionExecutionState::Running
                && execution.parent_turn_id == expected_parent_turn
                && execution.execution_turn_id
                    == (if expected_parent_turn.is_some() {
                        None
                    } else {
                        Some(actual_turn_id)
                    })
        })
        .count();
    if execution_count != 1 {
        return Err(format!(
            "question target has {execution_count} matching durable executions; expected one"
        ));
    }
    Ok(())
}

/// Validates the source-settled question against retained target delivery; returns an error for mismatched identity.
/// 对照目标保留投递校验来源已结算的问题；身份不匹配时返回错误。
fn prove_settled_question_delivery(
    document: &vulcan_session_core::v1::document::SessionDocument,
    question: &vulcan_session_core::v1::collaboration::CollaborationQuestionRecord,
) -> Result<(), String> {
    if question.state == CollaborationQuestionState::Pending {
        return Err(String::from("question has not settled"));
    }
    // Source settlement is authoritative even before target acknowledgement or execution completion.
    // 来源结算是权威事实，无须等待目标确认或处理执行结束。
    let inbound = document
        .collaboration_state()
        .inbound_deliveries
        .get(&question.delivery_id)
        .filter(|delivery| {
            delivery.delivery_id == question.delivery_id
                && delivery.question_id == question.question_id
                && delivery.source == question.source
                && delivery.target == question.target
                && delivery.execution_purpose == question.execution_purpose
                && delivery.bound_turn_id == question.target.turn_id
                && matches!(
                    delivery.state,
                    CollaborationDeliveryState::Accepted
                        | CollaborationDeliveryState::Projected
                        | CollaborationDeliveryState::Failed
                )
        })
        .ok_or_else(|| {
            String::from("settled question has no matching retained inbound delivery")
        })?;
    match inbound.execution_purpose {
        CollaborationExecutionPurpose::InTurnQuestion if inbound.bound_turn_id.is_some() => Ok(()),
        CollaborationExecutionPurpose::NaturallyIdleQuestion if inbound.bound_turn_id.is_none() => {
            Ok(())
        }
        _ => Err(String::from(
            "settled question has an invalid execution binding",
        )),
    }
}

/// Builds one host-verified participant without inventing missing parent metadata.
/// 构造宿主校验的参与者，不推测缺失的父级 metadata。
fn participant(
    session_id: Uuid,
    root_session_id: Uuid,
    project_id: &str,
    generation: u64,
    turn_id: Option<u64>,
    task_id: Option<SubagentTaskId>,
) -> Result<CollaborationParticipant, String> {
    let turn_id = turn_id
        .map(TurnId::try_new)
        .transpose()
        .map_err(|error| format!("invalid collaboration turn identity: {error}"))?;
    CollaborationParticipant::new(
        session_id.to_string(),
        root_session_id.to_string(),
        project_id.to_owned(),
        generation,
        turn_id,
        task_id,
    )
    .map_err(|error| format!("invalid collaboration participant: {error}"))
}

/// Returns the stable tool id required by one typed command.
/// 返回一条类型化命令所要求的稳定工具 ID。
fn command_tool_id(command: &AgentCollaborationCommand) -> &'static str {
    match command {
        AgentCollaborationCommand::AskQuestion(_) => ASK_QUESTION_TOOL_ID,
        AgentCollaborationCommand::AskParent(_) => ASK_PARENT_TOOL_ID,
        AgentCollaborationCommand::ReplyQuestion(_) => REPLY_QUESTION_TOOL_ID,
        AgentCollaborationCommand::WaitQuestion(_) => WAIT_QUESTION_TOOL_ID,
        AgentCollaborationCommand::CancelQuestion(_) => CANCEL_QUESTION_TOOL_ID,
        AgentCollaborationCommand::ListSessions(_) => LIST_SESSIONS_TOOL_ID,
        AgentCollaborationCommand::ReadSession(_) => READ_SESSION_TOOL_ID,
        AgentCollaborationCommand::SendSessionMessage(_) => SEND_SESSION_MESSAGE_TOOL_ID,
        AgentCollaborationCommand::SendNotification(_) => SEND_NOTIFICATION_TOOL_ID,
        AgentCollaborationCommand::WaitSession(_) => WAIT_SESSION_TOOL_ID,
    }
}

/// Validates all persisted source identity fields before any cross-session operation.
/// 在任何跨会话操作前校验全部持久化来源身份字段。
///
/// The controller-owned tool-call id has no duplicate field in the V1 ToolCall event; it is
/// therefore required to be a non-empty host identity and is included in the stable operation
/// key rather than compared with a fabricated event field.
/// 控制器所有工具调用 ID 在 V1 ToolCall 事件中没有重复字段；因此只要求它是非空宿主身份，
/// 并将其纳入稳定操作键，而不与虚构的事件字段比较。
fn validate_invocation(
    state_weak: &Weak<Mutex<ServiceBridgeState>>,
    context: &VulcanToolInvocationContext,
    expected_tool_id: &str,
) -> Result<ValidatedInvocation, String> {
    let state = state_weak
        .upgrade()
        .ok_or_else(|| String::from("service bridge state is no longer available"))?;
    let source_session_id = Uuid::parse_str(&context.session_id)
        .map_err(|error| format!("invalid invocation session_id: {error}"))?;
    if context.controller_tool_call_id.trim().is_empty()
        || context.controller_tool_call_id.trim() != context.controller_tool_call_id
    {
        return Err(String::from(
            "invocation controller_tool_call_id is invalid",
        ));
    }
    let (controller_runtime, registry) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        (
            controller_runtime,
            Arc::clone(&guard.session_runtime_registry),
        )
    };
    let source_handle = crate::session_goal_lifecycle::registered_session_v1_handle_from_state(
        &state,
        source_session_id,
    )?;
    let source_locator = source_handle
        .storage_locator()
        .map_err(|error| format!("failed to resolve source session locator: {error}"))?;
    let expected_tool_id = expected_tool_id.to_owned();
    let context_identity = context.clone();
    let source_tool = source_handle
        .read(|document| {
            let Some(active_api) = document.active_api() else {
                return Some(Err(String::from(
                    "source ToolCall does not belong to an active API",
                )));
            };
            if active_api.api_call_id != context_identity.api_call_id
                || !active_api.tool_seqs.contains(&context_identity.tool_seq)
            {
                return Some(Err(String::from(
                    "source ToolCall does not belong to the active API execution",
                )));
            }
            document
                .events()
                .iter()
                .find(|event| event.seq == context_identity.tool_seq)
                .map(|event| {
                    let EventBody::ToolCall(tool) = &event.event else {
                        return Err(String::from("source tool_seq does not identify a ToolCall event"));
                    };
                    if event.turn_id.is_none() {
                        return Err(String::from("source ToolCall event is not bound to a turn"));
                    }
                    if tool.api_call_id != context_identity.api_call_id {
                        return Err(String::from("source ToolCall api_call_id does not match invocation"));
                    }
                    if tool.tool_call_id != context_identity.tool_call_id {
                        return Err(String::from("source ToolCall tool_call_id does not match invocation"));
                    }
                    if tool.request_stream_id != context_identity.request_stream_id {
                        return Err(String::from("source ToolCall request_stream_id does not match invocation"));
                    }
                    if tool.provider_tool_call_id != context_identity.provider_tool_call_id {
                        return Err(String::from(
                            "source ToolCall provider_tool_call_id does not match invocation",
                        ));
                    }
                    if tool.tool_name != expected_tool_id {
                        return Err(format!(
                            "source ToolCall tool_name does not match expected tool `{expected_tool_id}`"
                        ));
                    }
                    Ok((event.turn_id.map(|turn| turn.value()), tool.tool_name.clone()))
                })
        })
        .map_err(|error| format!("failed to read source session identity: {error}"))?
        .ok_or_else(|| String::from("source tool_seq is not present in the canonical session"))??;
    let source_turn_id = source_tool
        .0
        .ok_or_else(|| String::from("source ToolCall event is not bound to a turn"))?;
    let source_tool_id = source_tool.1;
    let operation_id = stable_operation_id(context, expected_tool_id.as_str());
    Ok(ValidatedInvocation {
        state,
        controller_runtime,
        registry,
        source_session_id,
        source_handle,
        source_locator,
        source_turn_id,
        source_tool_id,
        source_api_call_id: Some(context.api_call_id),
        operation_id,
    })
}

/// Validates a host lifecycle callback against the exact persisted source ToolCall.
/// 根据精确持久化来源 ToolCall 校验一条宿主生命周期回调。
fn validate_host_invocation(
    state_weak: &Weak<Mutex<ServiceBridgeState>>,
    context: &VulcanToolInvocationContext,
    operation_id: &CollaborationOperationId,
    operation_suffix: Option<&str>,
    reserved_execution: Option<&vulcan_agent_runtime::agent_runtime::task_runtime::collaboration_lifecycle::ChildTaskExecutionIdentity>,
) -> Result<ValidatedInvocation, String> {
    let state = state_weak
        .upgrade()
        .ok_or_else(|| String::from("service bridge state is no longer available"))?;
    let source_session_id = Uuid::parse_str(&context.session_id)
        .map_err(|error| format!("invalid lifecycle invocation session_id: {error}"))?;
    if context.controller_tool_call_id.trim().is_empty()
        || context.controller_tool_call_id.trim() != context.controller_tool_call_id
    {
        return Err(String::from(
            "lifecycle invocation controller_tool_call_id is invalid",
        ));
    }
    let (controller_runtime, registry) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        (
            controller_runtime,
            Arc::clone(&guard.session_runtime_registry),
        )
    };
    let source_handle = crate::session_goal_lifecycle::registered_session_v1_handle_from_state(
        &state,
        source_session_id,
    )?;
    let source_locator = source_handle
        .storage_locator()
        .map_err(|error| format!("failed to resolve lifecycle source locator: {error}"))?;
    let context_identity = context.clone();
    let source_tool = source_handle
        .read(|document| {
            // Existing executions authenticate against their retained reservation, not a later API.
            // 已有执行依据保留的预留认证，不依据之后的 API。
            if let Some(execution) = reserved_execution {
                let execution_id =
                    match CollaborationExecutionId::try_new(execution.execution_id.clone()) {
                        Ok(value) => value,
                        Err(error) => return Some(Err(error.to_string())),
                    };
                let Some(record) = document
                    .collaboration_state()
                    .child_tasks
                    .get(&execution_id)
                else {
                    return Some(Err(String::from(
                        "child execution has no authenticated reservation",
                    )));
                };
                if record.task_id != execution.task_id
                    || record.child.session_id != execution.task_session_id.to_string()
                    || record.parent_segment_generation != document.segment_generation()
                    || stable_operation_id(&context_identity, &record.source_tool_name)
                        != record.reservation_operation_id.as_str()
                {
                    return Some(Err(String::from(
                        "child lifecycle invocation does not match its original reservation",
                    )));
                }
                return Some(Ok((
                    record.parent_turn_id.value(),
                    record.source_tool_name.clone(),
                )));
            }
            let Some(active_api) = document.active_api() else {
                return Some(Err(String::from(
                    "lifecycle ToolCall does not belong to an active API",
                )));
            };
            if active_api.api_call_id != context_identity.api_call_id
                || !active_api.tool_seqs.contains(&context_identity.tool_seq)
            {
                return Some(Err(String::from(
                    "lifecycle ToolCall does not belong to the active API execution",
                )));
            }
            document
                .events()
                .iter()
                .find(|event| event.seq == context_identity.tool_seq)
                .map(|event| {
                    let EventBody::ToolCall(tool) = &event.event else {
                        return Err(String::from(
                            "lifecycle tool_seq does not identify a ToolCall event",
                        ));
                    };
                    let Some(turn_id) = event.turn_id else {
                        return Err(String::from(
                            "lifecycle ToolCall event is not bound to a turn",
                        ));
                    };
                    if tool.api_call_id != context_identity.api_call_id
                        || tool.tool_call_id != context_identity.tool_call_id
                        || tool.request_stream_id != context_identity.request_stream_id
                        || tool.provider_tool_call_id != context_identity.provider_tool_call_id
                    {
                        return Err(String::from(
                            "lifecycle ToolCall identity does not match invocation",
                        ));
                    }
                    Ok((turn_id.value(), tool.tool_name.clone()))
                })
        })
        .map_err(|error| format!("failed to read lifecycle source identity: {error}"))?
        .ok_or_else(|| {
            String::from("lifecycle tool_seq is not present in the canonical session")
        })??;
    let base_operation_id = stable_operation_id(context, &source_tool.1);
    let expected_operation_id = operation_suffix
        .map(|suffix| format!("{base_operation_id}:{suffix}"))
        .unwrap_or(base_operation_id);
    if operation_id.as_str() != expected_operation_id {
        return Err(String::from(
            "lifecycle operation_id does not match the trusted invocation context",
        ));
    }
    Ok(ValidatedInvocation {
        state,
        controller_runtime,
        registry,
        source_session_id,
        source_handle,
        source_locator,
        source_turn_id: source_tool.0,
        source_tool_id: source_tool.1,
        source_api_call_id: Some(context.api_call_id),
        operation_id: expected_operation_id,
    })
}

/// Validates one child execution's parent, root, task, and generation binding.
/// 校验一次子执行的父级、根、任务与代际绑定。
async fn validate_child_lifecycle_binding(
    invocation: &ValidatedInvocation,
    execution: &vulcan_agent_runtime::agent_runtime::task_runtime::collaboration_lifecycle::ChildTaskExecutionIdentity,
    parent_turn_id: TurnId,
    child: &CollaborationParticipant,
) -> Result<(), String> {
    let expected_parent_turn = TurnId::try_new(invocation.source_turn_id)
        .map_err(|error| format!("invalid lifecycle parent turn identity: {error}"))?;
    if parent_turn_id != expected_parent_turn {
        return Err(String::from(
            "child lifecycle parent_turn_id does not match the source ToolCall turn",
        ));
    }
    if execution.execution_id.trim().is_empty() {
        return Err(String::from("child lifecycle execution_id is empty"));
    }
    let expected_session_id = execution.task_session_id.to_string();
    if execution.task_id.as_str() != expected_session_id {
        return Err(String::from(
            "child lifecycle task_id does not match the execution session identity",
        ));
    }
    if child.session_id != expected_session_id || child.task_id.as_ref() != Some(&execution.task_id)
    {
        return Err(String::from(
            "child lifecycle participant does not match the execution identity",
        ));
    }
    let usage = invocation
        .source_handle
        .lifetime_usage_snapshot()
        .map_err(|error| format!("failed to read lifecycle root ownership: {error}"))?;
    let root_session_id = Uuid::parse_str(&usage.root_session_id)
        .map_err(|error| format!("lifecycle root ownership identity is invalid: {error}"))?;
    if child.root_session_id != root_session_id.to_string() {
        return Err(String::from(
            "child lifecycle participant is not owned by the source root session",
        ));
    }
    let root_locator = if root_session_id == invocation.source_session_id {
        invocation.source_locator.clone()
    } else {
        let root_handle = crate::session_goal_lifecycle::session_v1_handle_from_state(
            &invocation.state,
            root_session_id,
        )
        .await
        .map_err(|error| format!("failed to load lifecycle root session: {error}"))?;
        root_handle
            .storage_locator()
            .map_err(|error| format!("failed to resolve lifecycle root locator: {error}"))?
    };
    if root_locator.project_id() != invocation.source_locator.project_id()
        || child.project_id != root_locator.project_id()
    {
        return Err(String::from(
            "child lifecycle participant project does not match its root",
        ));
    }
    let child_locator =
        locate_embedded_task_session_v1(&root_locator, &execution.task_session_id.to_string())
            .map_err(|error| format!("failed to resolve lifecycle child locator: {error}"))?;
    if locator_generation(&child_locator)? != child.segment_generation {
        return Err(String::from(
            "child lifecycle participant generation does not match its locator",
        ));
    }
    let child_handle = load_embedded_canonical_handle(
        &invocation.registry,
        child_locator.clone(),
        execution.task_session_id,
    )?;
    let canonical_locator = child_handle
        .storage_locator()
        .map_err(|error| format!("failed to resolve canonical lifecycle child locator: {error}"))?;
    if canonical_locator != child_locator {
        return Err(String::from(
            "child lifecycle handle does not match its exact locator",
        ));
    }
    Ok(())
}

/// Verifies a stored child record still belongs to the exact lifecycle request.
/// 验证可靠子任务记录仍属于当前精确生命周期请求。
fn validate_stored_child_binding(
    document: &vulcan_session_core::v1::document::SessionDocument,
    execution_id: &CollaborationExecutionId,
    task_id: &SubagentTaskId,
    parent_turn_id: TurnId,
    child: &CollaborationParticipant,
) -> Result<(), vulcan_session_core::v1::errors::DomainError> {
    let current = document
        .collaboration_state()
        .child_tasks
        .get(execution_id)
        .ok_or_else(
            || vulcan_session_core::v1::errors::DomainError::CollaborationNotFound {
                kind: "child_task",
                identity: task_id.as_str().to_owned(),
            },
        )?;
    if current.task_id != *task_id
        || current.parent_turn_id != parent_turn_id
        || current.child != *child
    {
        return Err(
            vulcan_session_core::v1::errors::DomainError::CollaborationUnauthorized {
                action: "child_task_lifecycle",
                identity: task_id.as_str().to_owned(),
            },
        );
    }
    Ok(())
}

/// Reads the durable parent and participant binding for an existing child task.
/// 读取现有子任务的可靠父回合与参与者绑定。
fn stored_child_lifecycle_binding(
    document: &vulcan_session_core::v1::document::SessionDocument,
    execution_id: &CollaborationExecutionId,
    task_id: &SubagentTaskId,
) -> Result<(TurnId, CollaborationParticipant), String> {
    let task = document
        .collaboration_state()
        .child_tasks
        .get(execution_id)
        .ok_or_else(|| format!("child task `{}` is not durably reserved", task_id.as_str()))?;
    if task.task_id != *task_id {
        return Err(String::from(
            "child execution does not belong to the requested task",
        ));
    }
    Ok((task.parent_turn_id, task.child.clone()))
}

/// Verifies that the source document still owns the expected open parent turn.
/// 验证来源文档仍拥有预期的开放父回合。
fn require_open_parent_turn(
    document: &vulcan_session_core::v1::document::SessionDocument,
    parent_turn_id: TurnId,
) -> Result<(), vulcan_session_core::v1::errors::DomainError> {
    let active = document
        .active_turn()
        .ok_or(vulcan_session_core::v1::errors::DomainError::NoActiveTurn)?;
    if active.closed {
        return Err(vulcan_session_core::v1::errors::DomainError::NoActiveTurn);
    }
    if active.turn_id != parent_turn_id {
        return Err(
            vulcan_session_core::v1::errors::DomainError::TurnIdentityMismatch {
                expected: parent_turn_id.value(),
                actual: active.turn_id.value(),
            },
        );
    }
    Ok(())
}

/// Converts a durable domain child-task receipt into the frozen host receipt.
/// 将可靠领域子任务回执转换为冻结的宿主回执。
fn child_task_lifecycle_receipt(
    operation_id: CollaborationOperationId,
    receipt: SessionConditionalMutationReceipt<CollaborationChildTaskReceipt>,
) -> ChildTaskLifecycleReceipt {
    ChildTaskLifecycleReceipt {
        execution_id: receipt.value.task.execution_id,
        operation_id,
        task_id: receipt.value.task.task_id,
        state: receipt.value.task.state,
        parent_turn_id: receipt.value.task.parent_turn_id,
        document_revision: receipt.revision,
        changed: receipt.changed,
    }
}

/// Maps a child terminal outcome to the runtime phase suffix used for retries.
/// 将子任务终态结果映射为重试时使用的运行时阶段后缀。
fn child_terminal_operation_suffix(outcome: CollaborationChildTaskTerminalOutcome) -> &'static str {
    match outcome {
        CollaborationChildTaskTerminalOutcome::Completed => "terminal:completed",
        CollaborationChildTaskTerminalOutcome::Failed => "terminal:failed",
        CollaborationChildTaskTerminalOutcome::Stopped => "terminal:stopped",
        CollaborationChildTaskTerminalOutcome::Cancelled => "terminal:cancelled",
    }
}

/// Produces one deterministic operation id from the trusted invocation context.
/// 从可信调用上下文生成一个确定性的操作 ID。
fn stable_operation_id(context: &VulcanToolInvocationContext, tool_id: &str) -> String {
    vulcan_agent_runtime::tooling::collaboration::stable_collaboration_operation_id(
        context, tool_id,
    )
}

/// Checks one source-owned grant for a cross-project action.
/// 在跨项目动作上检查一条来源拥有的授权记录。
fn source_grant_allows(
    invocation: &ValidatedInvocation,
    target_session_id: Uuid,
    action: CollaborationGrantAction,
) -> Result<bool, String> {
    source_handle_grant_allows(
        &invocation.source_handle,
        invocation.source_session_id,
        target_session_id,
        action,
    )
}

/// Checks a grant on an arbitrary canonical source handle, including archived sources.
/// 在任意来源规范句柄（包括归档来源）上检查授权。
fn source_handle_grant_allows(
    source_handle: &Arc<SessionHandle>,
    source_session_id: Uuid,
    target_session_id: Uuid,
    action: CollaborationGrantAction,
) -> Result<bool, String> {
    source_handle
        .read(|document| {
            let source_session_id = source_session_id.to_string();
            let target_session_id = target_session_id.to_string();
            document.collaboration_state().grants.values().any(|grant| {
                grant.source_session_id == source_session_id
                    && grant.target_session_id == target_session_id
                    && grant.created_revision <= document.durable_revision()
                    && grant.revoked_at_ms.is_none()
                    && grant.actions.contains(&action)
            })
        })
        .map_err(|error| format!("failed to read source collaboration grants: {error}"))
}

/// Checks one grant directly inside the source document mutation closure.
/// 在来源文档变更闭包内部检查一条授权。
fn document_grant_allows(
    document: &vulcan_session_core::v1::document::SessionDocument,
    source_session_id: Uuid,
    target_session_id: Uuid,
    action: CollaborationGrantAction,
) -> bool {
    let source_session_id = source_session_id.to_string();
    let target_session_id = target_session_id.to_string();
    document.collaboration_state().grants.values().any(|grant| {
        grant.source_session_id == source_session_id
            && grant.target_session_id == target_session_id
            && grant.created_revision <= document.durable_revision()
            && grant.revoked_at_ms.is_none()
            && grant.actions.contains(&action)
    })
}

/// Resolves a target through SQLite or one exact parent-derived Task locator.
/// 通过 SQLite 或一个精确的父定位 Task 定位器解析目标。
///
/// The target handle is always the canonical registry handle. An embedded target is admitted
/// only when the source is an indexed root and the source's durable child ownership record binds
/// the exact child id; this avoids deriving a sibling or nested child from an arbitrary path.
/// 目标句柄始终是注册表中的规范句柄。只有来源是索引根且来源可靠子任务归属记录绑定精确
/// 子 ID 时才接纳内嵌目标，避免从任意路径推导兄弟或嵌套子会话。
async fn resolve_authorized_target(
    invocation: &ValidatedInvocation,
    target_session_id: Uuid,
    action: CollaborationGrantAction,
) -> Result<ResolvedSession, String> {
    if target_session_id == invocation.source_session_id {
        return Err(String::from(
            "collaboration target must differ from source session",
        ));
    }
    let target_id = target_session_id.to_string();
    let indexed_target =
        locate_optional_session_v1_storage(&invocation.controller_runtime, &target_id)
            .await
            .map_err(|error| format!("failed to resolve target session locator: {error}"))?;
    let (locator, handle) = match indexed_target {
        Some(locator) => {
            if locator.project_id() != invocation.source_locator.project_id()
                && !source_grant_allows(invocation, target_session_id, action)?
            {
                return Err(String::from(
                    "cross-project collaboration requires an active source-owned grant",
                ));
            }
            let handle = crate::session_goal_lifecycle::session_v1_handle_from_state(
                &invocation.state,
                target_session_id,
            )
            .await?;
            let canonical_locator = handle
                .storage_locator()
                .map_err(|error| format!("failed to resolve canonical target locator: {error}"))?;
            if canonical_locator != locator {
                return Err(String::from(
                    "target session handle does not match its indexed locator",
                ));
            }
            (locator, handle)
        }
        None => {
            let usage = invocation.source_handle.lifetime_usage_snapshot()?;
            let root_session_id = Uuid::parse_str(&usage.root_session_id)
                .map_err(|error| format!("invalid source root identity: {error}"))?;
            let task_id = SubagentTaskId::try_new(target_id.clone())
                .map_err(|error| format!("invalid embedded target task identity: {error}"))?;
            let owned = invocation
                .source_handle
                .read(|document| {
                    document
                        .collaboration_state()
                        .child_tasks
                        .values()
                        .any(|record| {
                            record.task_id == task_id
                                && record.child.session_id == target_id
                                && record.child.root_session_id == usage.root_session_id
                        })
                })
                .map_err(|error| format!("failed to read source child ownership: {error}"))?;
            if !owned
                && participants::verified_direct_parent(invocation).await? != target_session_id
            {
                return Err(String::from(
                    "embedded target is neither an owned child nor the verified direct parent",
                ));
            }
            let root_handle = crate::session_goal_lifecycle::session_v1_handle_from_state(
                &invocation.state,
                root_session_id,
            )
            .await?;
            let root_locator = root_handle
                .storage_locator()
                .map_err(|error| error.to_string())?;
            let locator =
                locate_embedded_task_session_v1(&root_locator, &target_id).map_err(|error| {
                    format!("failed to resolve embedded target session locator: {error}")
                })?;
            let handle = load_embedded_canonical_handle(
                &invocation.registry,
                locator.clone(),
                target_session_id,
            )?;
            let canonical_locator = handle.storage_locator().map_err(|error| {
                format!("failed to resolve canonical embedded target locator: {error}")
            })?;
            if canonical_locator != locator {
                return Err(String::from(
                    "embedded target handle does not match its derived locator",
                ));
            }
            (locator, handle)
        }
    };
    if locator.project_id() != invocation.source_locator.project_id()
        && !source_grant_allows(invocation, target_session_id, action)?
    {
        return Err(String::from(
            "cross-project collaboration requires an active source-owned grant",
        ));
    }
    Ok(ResolvedSession { locator, handle })
}

/// Resolves a read target while allowing the caller to read its own session.
/// 解析读取目标，同时允许调用者读取自身会话。
async fn resolve_read_target(
    invocation: &ValidatedInvocation,
    target_session_id: Uuid,
) -> Result<SessionV1StorageLocator, String> {
    if target_session_id == invocation.source_session_id {
        return Ok(invocation.source_locator.clone());
    }
    Ok(resolve_authorized_target(
        invocation,
        target_session_id,
        CollaborationGrantAction::Read,
    )
    .await?
    .locator)
}

/// Builds the source participant using the canonical source generation and active turn.
/// 使用规范来源代际与活动回合构造来源参与者。
async fn source_participant(
    invocation: &ValidatedInvocation,
) -> Result<CollaborationParticipant, String> {
    current_session_participant(invocation, Some(invocation.source_turn_id)).await
}

/// Builds the current invocation participant with an optional turn binding.
/// 使用可选回合绑定构造当前调用参与者。
async fn current_session_participant(
    invocation: &ValidatedInvocation,
    turn_id: Option<u64>,
) -> Result<CollaborationParticipant, String> {
    participants::participant_for_handle(
        invocation,
        invocation.source_session_id,
        &invocation.source_handle,
        turn_id,
    )
    .await
}

/// Builds the current invocation participant using an already verified durable binding.
/// 使用已验证的持久绑定构造当前调用参与者。
///
/// The binding supplies root and child identity for embedded sessions because the indexed locator
/// intentionally contains no reverse parent scan API.
/// 绑定提供内嵌会话的根与子任务身份，因为索引定位器刻意不提供反向父目录扫描接口。
async fn current_participant_for_binding(
    invocation: &ValidatedInvocation,
    binding: &CollaborationParticipant,
    turn_id: Option<u64>,
) -> Result<CollaborationParticipant, String> {
    if binding.session_id != invocation.source_session_id.to_string() {
        return Err(String::from(
            "durable collaboration participant is bound to another session",
        ));
    }
    let generation = invocation
        .source_handle
        .read(|document| document.segment_generation())
        .map_err(|error| format!("failed to read source event generation: {error}"))?;
    if generation != binding.segment_generation
        || invocation.source_locator.project_id() != binding.project_id
    {
        return Err(String::from(
            "current collaboration session no longer matches the durable participant generation",
        ));
    }
    participant(
        invocation.source_session_id,
        Uuid::parse_str(&binding.root_session_id)
            .map_err(|error| format!("invalid durable participant root identity: {error}"))?,
        &binding.project_id,
        generation,
        turn_id,
        binding.task_id.clone(),
    )
}

/// Compares participant scope while intentionally ignoring the transient turn binding.
/// 比较参与者的会话范围并刻意忽略短暂的回合绑定。
fn same_participant_scope(
    left: &CollaborationParticipant,
    right: &CollaborationParticipant,
) -> bool {
    left.session_id == right.session_id
        && left.root_session_id == right.root_session_id
        && left.project_id == right.project_id
        && left.segment_generation == right.segment_generation
        && left.task_id == right.task_id
}

/// Builds a target participant from the exact locator and validated parent binding.
/// 根据精确定位器及校验过的父级绑定构造目标参与者。
async fn target_participant(
    invocation: &ValidatedInvocation,
    target_session_id: Uuid,
    target: &ResolvedSession,
    turn_id: Option<TurnId>,
) -> Result<CollaborationParticipant, String> {
    participants::participant_for_handle(
        invocation,
        target_session_id,
        &target.handle,
        turn_id.map(TurnId::value),
    )
    .await
}

/// Loads the indexed canonical source handle named by a service-issued question id.
/// 加载服务签发问题 ID 指向的索引规范来源句柄。
async fn load_question_source(
    invocation: &ValidatedInvocation,
    question_id: &CollaborationQuestionId,
) -> Result<(Uuid, Arc<SessionHandle>), String> {
    let source_session_id = question_source_session_id(question_id.as_str())?;
    if source_session_id == invocation.source_session_id {
        return Ok((source_session_id, Arc::clone(&invocation.source_handle)));
    }
    let indexed = locate_optional_session_v1_storage(
        &invocation.controller_runtime,
        &source_session_id.to_string(),
    )
    .await
    .map_err(|error| format!("failed to resolve question source locator: {error}"))?;
    if indexed.is_some() {
        let handle = crate::session_goal_lifecycle::session_v1_handle_from_state(
            &invocation.state,
            source_session_id,
        )
        .await
        .map_err(|error| format!("failed to load question source session: {error}"))?;
        return Ok((source_session_id, handle));
    }

    // An embedded source is addressable only through the verified inbound record on the current
    // target, which supplies both the root locator and the source participant binding.
    // 内嵌来源只能通过当前目标上的已验证入站记录定位，该记录同时提供根定位与来源参与者绑定。
    let inbound_source = invocation
        .source_handle
        .read(|document| {
            document
                .collaboration_state()
                .inbound_deliveries
                .values()
                .filter(|delivery| {
                    delivery.question_id.as_str() == question_id.as_str()
                        && delivery.source.session_id == source_session_id.to_string()
                })
                .min_by(|left, right| left.delivery_id.cmp(&right.delivery_id))
                .map(|delivery| delivery.source.clone())
        })
        .map_err(|error| format!("failed to read embedded question source binding: {error}"))?
        .ok_or_else(|| {
            String::from(
                "embedded question source is not addressable from a verified inbound delivery",
            )
        })?;
    let root_session_id = Uuid::parse_str(&inbound_source.root_session_id)
        .map_err(|error| format!("embedded question source root identity is invalid: {error}"))?;
    let (root_locator, _root_handle) = if root_session_id == invocation.source_session_id {
        (
            invocation.source_locator.clone(),
            Arc::clone(&invocation.source_handle),
        )
    } else {
        let root_handle = crate::session_goal_lifecycle::session_v1_handle_from_state(
            &invocation.state,
            root_session_id,
        )
        .await
        .map_err(|error| format!("failed to load embedded question source root: {error}"))?;
        let root_locator = root_handle
            .storage_locator()
            .map_err(|error| format!("failed to resolve embedded question source root: {error}"))?;
        (root_locator, root_handle)
    };
    let source_locator =
        locate_embedded_task_session_v1(&root_locator, &source_session_id.to_string())
            .map_err(|error| format!("failed to locate embedded question source: {error}"))?;
    let expected_task_id = SubagentTaskId::try_new(source_session_id.to_string())
        .map_err(|error| format!("embedded question source task identity is invalid: {error}"))?;
    if source_locator.project_id() != inbound_source.project_id
        || locator_generation(&source_locator)? != inbound_source.segment_generation
        || inbound_source.root_session_id != root_session_id.to_string()
        || inbound_source.task_id.as_ref() != Some(&expected_task_id)
    {
        return Err(String::from(
            "embedded question source locator does not match its inbound participant",
        ));
    }
    let source_handle =
        load_embedded_canonical_handle(&invocation.registry, source_locator, source_session_id)?;
    Ok((source_session_id, source_handle))
}

/// Reads one source question and its outbound delivery from the canonical handle.
/// 从规范句柄读取一条来源问题及其出站投递。
fn read_question_source(
    handle: &Arc<SessionHandle>,
    question_id: &CollaborationQuestionId,
) -> Result<
    (
        vulcan_session_core::v1::collaboration::CollaborationQuestionRecord,
        vulcan_session_core::v1::collaboration::CollaborationDeliveryRecord,
        u64,
    ),
    String,
> {
    handle
        .read(|document| {
            let question = document
                .question_snapshot(question_id)
                .cloned()
                .ok_or_else(|| String::from("collaboration question was not found"))?;
            let delivery = document
                .delivery_snapshot(&question.delivery_id)
                .cloned()
                .ok_or_else(|| String::from("collaboration question delivery was not found"))?;
            Ok((question, delivery, document.memory_revision()))
        })
        .map_err(|error| format!("failed to read collaboration question source: {error}"))?
}

/// Reads a model-visible snapshot and its live cross-project grant under the same source lock.
/// 在同一来源锁内读取模型可见快照及其当前跨项目授权。
/// Internal recovery may inspect revoked records; model-facing waits may not reveal new bodies.
/// 内部恢复可以检查已撤销记录；面向模型的等待不能泄露新的正文。
fn read_question_for_model(
    handle: &Arc<SessionHandle>,
    question_id: &CollaborationQuestionId,
) -> Result<
    (
        vulcan_session_core::v1::collaboration::CollaborationQuestionRecord,
        vulcan_session_core::v1::collaboration::CollaborationDeliveryRecord,
        u64,
    ),
    String,
> {
    handle
        .read(|document| {
            let question = document
                .question_snapshot(question_id)
                .ok_or_else(|| String::from("collaboration question was not found"))?;
            if question.source.project_id != question.target.project_id {
                let source = Uuid::parse_str(&question.source.session_id)
                    .map_err(|error| error.to_string())?;
                let target = Uuid::parse_str(&question.target.session_id)
                    .map_err(|error| error.to_string())?;
                let action = if question.answer.is_some() {
                    CollaborationGrantAction::Reply
                } else {
                    CollaborationGrantAction::Ask
                };
                if !document_grant_allows(document, source, target, action) {
                    return Err(String::from(
                        "collaboration snapshot is denied after authorization revocation",
                    ));
                }
            }
            let delivery = document
                .delivery_snapshot(&question.delivery_id)
                .ok_or_else(|| String::from("collaboration question delivery was not found"))?;
            Ok((
                question.clone(),
                delivery.clone(),
                document.memory_revision(),
            ))
        })
        .map_err(|error| format!("failed to read authorized collaboration snapshot: {error}"))?
}

/// Loads one embedded task from its exact validated parent locator into the unique registry.
/// 从精确验证的父定位器加载一个内嵌任务并放入唯一注册表。
fn load_embedded_canonical_handle(
    registry: &Arc<SessionRuntimeRegistry>,
    locator: SessionV1StorageLocator,
    session_id: Uuid,
) -> Result<Arc<SessionHandle>, String> {
    registry
        .get_or_load(session_id, move || {
            let loaded =
                load_session_v1_for_runtime(&locator).map_err(SessionRuntimeError::from)?;
            let actual_session_id = loaded.metadata.session_id.clone();
            if actual_session_id != session_id.to_string() {
                return Err(SessionRuntimeError::LoadedSessionDocumentIdentityMismatch {
                    expected: session_id,
                    actual: actual_session_id,
                });
            }
            let mut document = loaded.document;
            let recovered_goal_owners = document.memory_revision() != document.durable_revision();
            let projected = document.project_persisted_interruptions()?;
            let handle = SessionHandle::new(session_id, loaded.locator, document);
            if projected > 0 || recovered_goal_owners {
                handle.flush_all()?;
            }
            Ok(handle)
        })
        .map_err(|error| format!("failed to load embedded collaboration session: {error}"))
}

/// Builds an internal recovery authority from one already canonical source handle.
/// 根据已规范化的来源句柄构建内部恢复权威。
fn recovery_invocation(
    state: SharedServiceBridgeState,
    controller_runtime: vulcan_workspace_store::data_plane::HostRuntimeHandle,
    registry: Arc<SessionRuntimeRegistry>,
    source_session_id: Uuid,
    source_handle: Arc<SessionHandle>,
) -> Result<ValidatedInvocation, String> {
    let source_locator = source_handle
        .storage_locator()
        .map_err(|error| format!("failed to resolve recovery source locator: {error}"))?;
    Ok(ValidatedInvocation {
        state,
        controller_runtime,
        registry,
        source_session_id,
        source_handle,
        source_locator,
        source_turn_id: 0,
        source_api_call_id: None,
        source_tool_id: String::from("collaboration_recovery"),
        operation_id: format!("v1:recovery:{source_session_id}"),
    })
}

/// Resolves a question target from its durable participant locator without checking a revoked grant.
/// 根据可靠参与者定位解析问题目标，不检查已撤销的授权。
async fn resolve_question_target(
    invocation: &ValidatedInvocation,
    question: &vulcan_session_core::v1::collaboration::CollaborationQuestionRecord,
) -> Result<(Uuid, ResolvedSession), String> {
    resolve_participant_target(invocation, &question.target, "question").await
}

/// Resolves one recorded participant locator without checking a revoked grant.
/// 根据一条已记录参与者定位解析目标，不检查已撤销的授权。
///
/// # Notes
/// 说明
/// Both questions and notifications share this resolver so recovery can reach the exact recorded
/// target. Grant revocation is deliberately not decided here, because callers differ: question
/// acceptance is already frozen by the durable record, while notification delivery re-evaluates
/// the notify grant at the delivery step.
/// 问题与通知共用本解析器，使恢复能到达精确的已记录目标。授权撤销刻意不在此处判定，因为调用方
/// 不同：问题接纳已由可靠记录冻结，而通知投递在投递步骤重新判定通知授权。
///
/// # Parameters
/// 参数
/// `label` names the failing concept in error text so a caller can tell which record failed.
/// `label` 在错误文本中点明失败概念，使调用方能区分是哪类记录失败。
async fn resolve_participant_target(
    invocation: &ValidatedInvocation,
    participant: &vulcan_session_core::v1::collaboration::CollaborationParticipant,
    label: &str,
) -> Result<(Uuid, ResolvedSession), String> {
    let target_session_id = Uuid::parse_str(&participant.session_id)
        .map_err(|error| format!("{label} target session identity is invalid: {error}"))?;
    if target_session_id == invocation.source_session_id {
        let locator = invocation.source_locator.clone();
        if locator.project_id() != participant.project_id
            || locator_generation(&locator)? != participant.segment_generation
        {
            return Err(format!(
                "{label} target participant no longer matches its canonical locator"
            ));
        }
        return Ok((
            target_session_id,
            ResolvedSession {
                locator,
                handle: Arc::clone(&invocation.source_handle),
            },
        ));
    }

    let root_session_id = Uuid::parse_str(&participant.root_session_id)
        .map_err(|error| format!("{label} target root identity is invalid: {error}"))?;
    let (root_locator, root_handle) = if root_session_id == invocation.source_session_id {
        (
            invocation.source_locator.clone(),
            Arc::clone(&invocation.source_handle),
        )
    } else {
        let root_handle = crate::session_goal_lifecycle::session_v1_handle_from_state(
            &invocation.state,
            root_session_id,
        )
        .await
        .map_err(|error| format!("failed to load {label} target root session: {error}"))?;
        let root_locator = root_handle
            .storage_locator()
            .map_err(|error| format!("failed to resolve {label} target root locator: {error}"))?;
        (root_locator, root_handle)
    };
    if root_locator.project_id() != participant.project_id {
        return Err(format!(
            "{label} target root project does not match its participant"
        ));
    }
    if root_session_id == target_session_id {
        if root_locator.project_id() != participant.project_id
            || locator_generation(&root_locator)? != participant.segment_generation
        {
            return Err(format!(
                "{label} target root participant no longer matches its canonical locator"
            ));
        }
        return Ok((
            target_session_id,
            ResolvedSession {
                locator: root_locator,
                handle: root_handle,
            },
        ));
    }

    let target_locator =
        locate_embedded_task_session_v1(&root_locator, &target_session_id.to_string())
            .map_err(|error| format!("failed to locate {label} target child session: {error}"))?;
    if locator_generation(&target_locator)? != participant.segment_generation {
        return Err(format!(
            "{label} target child generation does not match its participant"
        ));
    }
    let target_handle = load_embedded_canonical_handle(
        &invocation.registry,
        target_locator.clone(),
        target_session_id,
    )?;
    let canonical_locator = target_handle
        .storage_locator()
        .map_err(|error| format!("failed to resolve {label} target child locator: {error}"))?;
    if canonical_locator != target_locator {
        return Err(format!(
            "{label} target child handle does not match its exact locator"
        ));
    }
    Ok((
        target_session_id,
        ResolvedSession {
            locator: target_locator,
            handle: target_handle,
        },
    ))
}

/// Verifies the target inbound record before terminal settlement is projected.
/// 在投影终态结算前验证目标入站记录。
fn validate_question_target_delivery(
    document: &vulcan_session_core::v1::document::SessionDocument,
    question: &vulcan_session_core::v1::collaboration::CollaborationQuestionRecord,
    delivery: &vulcan_session_core::v1::collaboration::CollaborationDeliveryRecord,
) -> Result<(), vulcan_session_core::v1::errors::DomainError> {
    let inbound = document
        .inbound_delivery_snapshot(&delivery.delivery_id)
        .ok_or_else(
            || vulcan_session_core::v1::errors::DomainError::CollaborationNotFound {
                kind: "inbound_delivery",
                identity: delivery.delivery_id.as_str().to_owned(),
            },
        )?;
    if inbound.question_id != question.question_id
        || inbound.source != question.source
        || inbound.target != question.target
    {
        return Err(
            vulcan_session_core::v1::errors::DomainError::CollaborationStateViolation {
                field: "collaboration.settlement.participant_binding",
            },
        );
    }
    Ok(())
}

/// Settles the target inbound delivery and acknowledges it on the source document.
/// 在目标可靠结算入站投递后于来源文档持久化确认。
async fn settle_and_acknowledge_delivery(
    invocation: &ValidatedInvocation,
    source_handle: &Arc<SessionHandle>,
    source_session_id: Uuid,
    question: &vulcan_session_core::v1::collaboration::CollaborationQuestionRecord,
    delivery: &vulcan_session_core::v1::collaboration::CollaborationDeliveryRecord,
    source_operation_id: CollaborationOperationId,
    source_event_seq: Seq,
    settlement: CollaborationTargetSettlementState,
) -> Result<CollaborationDeliverySettlementReceipt, String> {
    let (target_session_id, target) = resolve_question_target(invocation, question).await?;
    let target_operation_id = operation_suffix(invocation, "target_settlement")?;
    let target_delivery_id = delivery.delivery_id.clone();
    let target_question_id = question.question_id.clone();
    let target_question = question.clone();
    let target_delivery = delivery.clone();
    let target_source_operation_id = source_operation_id.clone();
    let target_settlement = settlement;
    let target_source_event_seq = source_event_seq;
    let acknowledged_at_ms = now_ms()?;
    let target_receipt = mutate_handle_with_admission(
        &invocation.registry,
        &target.handle,
        target_session_id,
        SessionMutationAdmission::ControlPlane,
        move |document| {
            validate_question_target_delivery(document, &target_question, &target_delivery)?;
            document.settle_inbound_delivery(SettleInboundDeliveryInput {
                operation_id: target_operation_id,
                delivery_id: target_delivery_id,
                question_id: target_question_id,
                source_operation_id: target_source_operation_id,
                settlement: target_settlement,
                source_event_seq: target_source_event_seq,
                acknowledged_at_ms,
            })
        },
    )
    .await?;

    let target_event_seq = target_receipt.value.event_seq;
    let target_revision = target_receipt.revision;
    let ack_operation_id = operation_suffix(invocation, "source_settlement_ack")?;
    let ack_delivery_id = delivery.delivery_id.clone();
    let ack_question_id = question.question_id.clone();
    let ack_settlement = settlement;
    let source_ack = mutate_handle_with_admission(
        &invocation.registry,
        source_handle,
        source_session_id,
        SessionMutationAdmission::ControlPlane,
        move |document| {
            document.acknowledge_delivery_settlement(AcknowledgeDeliverySettlementInput {
                operation_id: ack_operation_id,
                delivery_id: ack_delivery_id,
                question_id: ack_question_id,
                settlement: ack_settlement,
                source_event_seq,
                target_event_seq,
                target_revision,
                acknowledged_at_ms,
            })
        },
    )
    .await?;
    Ok(source_ack.value)
}

/// Returns the winning source settlement and its durable event sequence, if any.
/// 返回来源侧胜出结算及其可靠事件序列（如已结算）。
fn question_settlement_fact(
    document: &vulcan_session_core::v1::document::SessionDocument,
    question: &vulcan_session_core::v1::collaboration::CollaborationQuestionRecord,
) -> Result<
    Option<(
        CollaborationTargetSettlementState,
        CollaborationOperationId,
        Seq,
    )>,
    String,
> {
    let (settlement, operation_id) = match question.state {
        CollaborationQuestionState::Pending => return Ok(None),
        CollaborationQuestionState::Answered => (
            CollaborationTargetSettlementState::Answered,
            question
                .answer
                .as_ref()
                .ok_or_else(|| String::from("answered question has no answer settlement"))?
                .operation_id
                .clone(),
        ),
        CollaborationQuestionState::Cancelled => (
            CollaborationTargetSettlementState::Cancelled,
            question
                .cancellation
                .as_ref()
                .ok_or_else(|| String::from("cancelled question has no cancellation settlement"))?
                .operation_id
                .clone(),
        ),
    };
    let event_seq = document
        .collaboration_state()
        .operations
        .get(&operation_id)
        .map(|operation| operation.event_seq)
        .ok_or_else(|| {
            format!(
                "source settlement operation `{}` has no durable event",
                operation_id.as_str()
            )
        })?;
    Ok(Some((settlement, operation_id, event_seq)))
}

/// Reads one exact inbound delivery snapshot from its canonical target.
/// 从规范目标读取一条精确的入站投递快照。
fn read_target_inbound_delivery(
    target: &Arc<SessionHandle>,
    delivery_id: &CollaborationDeliveryId,
) -> Result<
    Option<vulcan_session_core::v1::collaboration::CollaborationInboundDeliveryRecord>,
    String,
> {
    target
        .read(|document| document.inbound_delivery_snapshot(delivery_id).cloned())
        .map_err(|error| format!("failed to read target collaboration delivery: {error}"))
}

/// Locates the target collaboration event that last recorded one inbound state.
/// 定位最近记录该入站状态的目标协作事件。
fn target_inbound_event_seq(
    target: &Arc<SessionHandle>,
    inbound: &vulcan_session_core::v1::collaboration::CollaborationInboundDeliveryRecord,
) -> Result<Seq, String> {
    target
        .read(|document| {
            document
                .events()
                .iter()
                .rev()
                .find_map(|event| {
                    let EventBody::Collaboration(collaboration) = &event.event else {
                        return None;
                    };
                    let vulcan_session_core::v1::collaboration::CollaborationAfterState::InboundDelivery {
                        delivery,
                    } = &collaboration.after_state
                    else {
                        return None;
                    };
                    (delivery.delivery_id == inbound.delivery_id
                        && delivery.state == inbound.state)
                        .then_some(event.seq)
                })
        })
        .map_err(|error| format!("failed to read target collaboration event sequence: {error}"))?
        .ok_or_else(|| String::from("target inbound state has no durable collaboration event"))
}

/// Replays one missing target acceptance from the source-owned durable question.
/// 根据来源拥有的可靠问题重放一条缺失的目标接纳。
async fn recover_target_acceptance(
    invocation: &ValidatedInvocation,
    question: &vulcan_session_core::v1::collaboration::CollaborationQuestionRecord,
) -> Result<SessionConditionalMutationReceipt<CollaborationDeliveryReceipt>, String> {
    let target_session_id = Uuid::parse_str(&question.target.session_id)
        .map_err(|error| format!("question target session identity is invalid: {error}"))?;
    let (_, target) = resolve_question_target(invocation, question).await?;
    let cross_project = question.source.project_id != question.target.project_id;
    if cross_project
        && !source_handle_grant_allows(
            &invocation.source_handle,
            invocation.source_session_id,
            target_session_id,
            CollaborationGrantAction::Ask,
        )?
    {
        return Err(String::from(
            "collaboration outbox repair requires an active source-owned grant",
        ));
    }
    let operation_id = CollaborationOperationId::try_new(format!(
        "{}:recovery:accept",
        question.operation_id.as_str()
    ))
    .map_err(|error| format!("invalid collaboration recovery operation identity: {error}"))?;
    let source = question.source.clone();
    let target_participant = question.target.clone();
    let question_id = question.question_id.clone();
    let delivery_id = question.delivery_id.clone();
    let execution_purpose = question.execution_purpose;
    let bound_turn_id = question.target.turn_id;
    let source_session_id = invocation.source_session_id;
    let target_operation_id = operation_id.clone();
    let accepted_at_ms = now_ms()?;
    mutate_handle_with_admission(
        &invocation.registry,
        &target.handle,
        target_session_id,
        SessionMutationAdmission::ControlPlane,
        move |document| {
            let (current_purpose, current_turn_id) = target_question_binding(document);
            if current_purpose != execution_purpose || current_turn_id != bound_turn_id {
                return Err(
                    vulcan_session_core::v1::errors::DomainError::CollaborationStateViolation {
                        field: "collaboration.recovery.accept.target_turn_binding",
                    },
                );
            }
            participants::require_document_message_admission(
                document,
                &source,
                &target_participant,
            )?;
            document.accept_delivery(AcceptDeliveryInput {
                operation_id: target_operation_id,
                delivery_id,
                question_id,
                source,
                target: target_participant,
                bound_turn_id,
                execution_purpose,
                accepted_at_ms,
            })
        },
    )
    .await
}

/// Reconciles one source outbox after startup or a canonical session load.
/// 在启动或规范会话加载后修复一份来源出站箱。
pub(crate) async fn reconcile_session_v1_collaboration_outbox(
    app: &crate::ServiceEventEmitter,
    state: SharedServiceBridgeState,
    source_session_id: Uuid,
) -> Result<u64, String> {
    let source_handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(&state, source_session_id)
            .await
            .map_err(|error| format!("failed to load collaboration outbox source: {error}"))?;
    let (controller_runtime, registry) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        (
            controller_runtime,
            Arc::clone(&guard.session_runtime_registry),
        )
    };
    let invocation = recovery_invocation(
        Arc::clone(&state),
        controller_runtime,
        registry,
        source_session_id,
        Arc::clone(&source_handle),
    )?;
    reconcile_canonical_collaboration_outbox(app, &invocation).await
}

/// Repairs outbox work for an already validated canonical root or embedded source.
/// 为已校验规范根来源或嵌入来源修复出站待办。
/// `app` emits advisory wakeups and `invocation` owns the source; returns the durable repair count.
/// `app` 发出提示性唤醒，`invocation` 拥有来源；返回可靠修复数量。
async fn reconcile_canonical_collaboration_outbox(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
) -> Result<u64, String> {
    // Preserve the exact embedded handle instead of looking up its UUID in the root-only index.
    // 保留精确嵌入句柄，不在仅包含根会话的索引中查找其 UUID。
    let source_handle = Arc::clone(&invocation.source_handle);
    let pending = source_handle
        .read(|document| {
            document
                .collaboration_state()
                .outbound_deliveries
                .values()
                .filter_map(|delivery| {
                    document
                        .question_snapshot(&delivery.question_id)
                        .cloned()
                        .map(|question| (question, delivery.clone()))
                })
                .filter(|(question, delivery)| {
                    matches!(
                        delivery.state,
                        CollaborationDeliveryState::Recorded
                            | CollaborationDeliveryState::Accepted
                            | CollaborationDeliveryState::Projected
                    ) || (delivery.settlement_ack.is_none()
                        && question.state != CollaborationQuestionState::Pending)
                })
                .collect::<Vec<_>>()
        })
        .map_err(|error| format!("failed to read collaboration outbox snapshot: {error}"))?;
    // Repair task intents independently of the question transaction stream.
    // 独立于问题事务流修复任务意图。
    let (mut repaired, mut failures) =
        match task_delivery::reconcile_task_message_outbox(app, &invocation).await {
            Ok(count) => (count, Vec::new()),
            Err(error) => (0, vec![error]),
        };
    for (question, delivery) in pending {
        let target_session_id = match Uuid::parse_str(&question.target.session_id) {
            Ok(value) => value,
            Err(error) => {
                failures.push(format!("question target identity is invalid: {error}"));
                continue;
            }
        };
        let (_, target) = match resolve_question_target(&invocation, &question).await {
            Ok(value) => value,
            Err(error) => {
                failures.push(format!(
                    "question `{}` target resolution failed: {error}",
                    question.question_id.as_str()
                ));
                continue;
            }
        };
        let target_inbound =
            match read_target_inbound_delivery(&target.handle, &delivery.delivery_id) {
                Ok(value) => value,
                Err(error) => {
                    failures.push(format!(
                        "question `{}` target delivery read failed: {error}",
                        question.question_id.as_str()
                    ));
                    continue;
                }
            };
        let target_inbound = if target_inbound.is_none()
            && matches!(
                delivery.state,
                CollaborationDeliveryState::Recorded | CollaborationDeliveryState::Accepted
            ) {
            // A source settlement can win while the process is between source commit and target
            // acceptance; replay acceptance before recording the target terminal settlement.
            // 来源结算可能发生在源提交与目标接纳之间；先重放接纳，再记录目标终态结算。
            match recover_target_acceptance(&invocation, &question).await {
                Ok(_receipt) => {
                    repaired = repaired.saturating_add(1);
                    read_target_inbound_delivery(&target.handle, &delivery.delivery_id).map_err(
                        |error| {
                            format!(
                                "question `{}` target delivery reread failed: {error}",
                                question.question_id.as_str()
                            )
                        },
                    )?
                }
                Err(error) => {
                    failures.push(format!(
                        "question `{}` target acceptance repair failed: {error}",
                        question.question_id.as_str()
                    ));
                    continue;
                }
            }
        } else {
            target_inbound
        };
        if let Some(inbound) = &target_inbound {
            let next_state = inbound.state;
            if delivery.state != next_state
                && matches!(
                    next_state,
                    CollaborationDeliveryState::Accepted | CollaborationDeliveryState::Projected
                )
            {
                let progress_operation = CollaborationOperationId::try_new(format!(
                    "{}:recovery:progress:{}",
                    delivery.operation_id.as_str(),
                    match next_state {
                        CollaborationDeliveryState::Accepted => "accepted",
                        CollaborationDeliveryState::Projected => "projected",
                        _ => "other",
                    }
                ))
                .map_err(|error| {
                    format!("invalid collaboration recovery progress identity: {error}")
                })?;
                let target_revision = Some(inbound.target_revision);
                // The source progress event must carry the target collaboration event seq.
                // 来源进度事件必须携带目标协作事件序列。
                let target_event_seq = Some(target_inbound_event_seq(&target.handle, inbound)?);
                let progress = RecordDeliveryProgressInput {
                    operation_id: progress_operation,
                    delivery_id: delivery.delivery_id.clone(),
                    question_id: delivery.question_id.clone(),
                    state: next_state,
                    failure: None,
                    target_revision,
                    target_event_seq,
                    updated_at_ms: now_ms()?,
                };
                match mutate_source(&invocation, move |document| {
                    document.record_delivery_progress(progress)
                })
                .await
                {
                    Ok(_) => repaired = repaired.saturating_add(1),
                    Err(error) => failures.push(format!(
                        "question `{}` source progress repair failed: {error}",
                        question.question_id.as_str()
                    )),
                }
            }
        }
        let settlement_fact = source_handle
            .read(|document| question_settlement_fact(document, &question))
            .map_err(|error| format!("failed to read source settlement fact: {error}"))??;
        if let Some((settlement, source_operation_id, source_event_seq)) = settlement_fact {
            let source_ack_missing = source_handle
                .read(|document| {
                    document
                        .delivery_snapshot(&delivery.delivery_id)
                        .and_then(|value| value.settlement_ack.as_ref())
                        .is_none()
                })
                .map_err(|error| {
                    format!("failed to read source settlement acknowledgement: {error}")
                })?;
            if source_ack_missing {
                let Some(inbound) = target_inbound else {
                    failures.push(format!(
                        "question `{}` has a settled source but no target inbound delivery",
                        question.question_id.as_str()
                    ));
                    continue;
                };
                if inbound.state == CollaborationDeliveryState::Failed {
                    failures.push(format!(
                        "question `{}` target delivery is failed and cannot receive settlement",
                        question.question_id.as_str()
                    ));
                    continue;
                }
                let (target_event_seq, target_revision) = if let Some(record) =
                    inbound.settlement.clone()
                {
                    if record.state != settlement
                        || record.source_operation_id != source_operation_id
                        || record.source_event_seq != source_event_seq
                    {
                        failures.push(format!(
                            "question `{}` target settlement does not match the source winning fact",
                            question.question_id.as_str()
                        ));
                        continue;
                    }
                    let target_event_seq = target
                        .handle
                        .read(|document| {
                            document
                                .collaboration_state()
                                .operations
                                .get(&record.target_operation_id)
                                .map(|operation| operation.event_seq)
                        })
                        .map_err(|error| {
                            format!("failed to read target settlement event: {error}")
                        })?
                        .ok_or_else(|| {
                            String::from("target settlement operation has no durable event")
                        })?;
                    (target_event_seq, inbound.target_revision)
                } else {
                    let target_operation_id = CollaborationOperationId::try_new(format!(
                        "{}:recovery:target-settlement",
                        source_operation_id.as_str()
                    ))
                    .map_err(|error| {
                        format!("invalid target settlement recovery identity: {error}")
                    })?;
                    let settlement_at_ms = now_ms()?;
                    let target_receipt = mutate_handle_with_admission(
                        &invocation.registry,
                        &target.handle,
                        target_session_id,
                        SessionMutationAdmission::ControlPlane,
                        {
                            let question = question.clone();
                            let delivery = delivery.clone();
                            let source_operation_id = source_operation_id.clone();
                            move |document| {
                                validate_question_target_delivery(document, &question, &delivery)?;
                                document.settle_inbound_delivery(SettleInboundDeliveryInput {
                                    operation_id: target_operation_id,
                                    delivery_id: delivery.delivery_id,
                                    question_id: question.question_id,
                                    source_operation_id,
                                    settlement,
                                    source_event_seq,
                                    acknowledged_at_ms: settlement_at_ms,
                                })
                            }
                        },
                    )
                    .await
                    .map_err(|error| format!("target settlement repair failed: {error}"))?;
                    repaired = repaired.saturating_add(1);
                    (target_receipt.value.event_seq, target_receipt.revision)
                };
                let ack_operation_id = CollaborationOperationId::try_new(format!(
                    "{}:recovery:source-settlement-ack",
                    source_operation_id.as_str()
                ))
                .map_err(|error| format!("invalid source settlement recovery identity: {error}"))?;
                let ack_input = AcknowledgeDeliverySettlementInput {
                    operation_id: ack_operation_id,
                    delivery_id: delivery.delivery_id.clone(),
                    question_id: question.question_id.clone(),
                    settlement,
                    source_event_seq,
                    target_event_seq,
                    target_revision,
                    acknowledged_at_ms: now_ms()?,
                };
                match mutate_source(&invocation, move |document| {
                    document.acknowledge_delivery_settlement(ack_input)
                })
                .await
                {
                    Ok(_) => repaired = repaired.saturating_add(1),
                    Err(error) => failures.push(format!(
                        "question `{}` source settlement acknowledgement repair failed: {error}",
                        question.question_id.as_str()
                    )),
                }
            }
        }
    }
    // Notifications live in their own map and have no question transaction to ride on, so the same
    // source-commit-before-target-acceptance window strands them unless they are replayed here.
    // 通知位于独立映射、不依附问题事务，因此若不在此处重放，同一"先提交来源再接纳目标"的窗口
    // 会把它们滞留。
    let stranded_notifications = source_handle
        .read(|document| {
            document
                .collaboration_state()
                .notifications
                .values()
                .filter(|record| {
                    record.state
                        == vulcan_session_core::v1::collaboration::CollaborationNotificationState::Recorded
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .map_err(|error| format!("failed to read notification outbox snapshot: {error}"))?;
    for record in stranded_notifications {
        let (target_session_id, target) =
            match resolve_participant_target(&invocation, &record.target, "notification").await {
                Ok(value) => value,
                Err(error) => {
                    failures.push(format!(
                        "notification `{}` target resolution failed: {error}",
                        record.notification_id.as_str()
                    ));
                    continue;
                }
            };
        // The replay performs both halves under their own stable identities, so a target that already
        // holds the acceptance is satisfied idempotently instead of being accepted twice.
        // 重放以各自稳定身份执行两半，因此已持有接纳的目标由幂等性满足，而不会被二次接纳。
        let delivery = notifications::deliver_recorded_notification(
            &invocation,
            target_session_id,
            &target,
            record.delivery_id.clone(),
            record.notification_id.clone(),
            record.operation_id.clone(),
            record.source.clone(),
            record.target.clone(),
            record.body.clone(),
        )
        .await;
        match delivery {
            Ok(()) => repaired = repaired.saturating_add(1),
            Err(error) => failures.push(format!(
                "notification `{}` acceptance repair failed: {error}",
                record.notification_id.as_str()
            )),
        }
    }
    if failures.is_empty() {
        Ok(repaired)
    } else {
        Err(format!(
            "collaboration outbox repair completed with {} unresolved item(s): {}",
            failures.len(),
            failures.join("; ")
        ))
    }
}

/// Serializes one bounded indexed session row for the native tool result.
/// 将一条有界索引会话记录序列化为原生工具结果。
fn session_list_entry_value(entry: SessionV1ListEntry, archived: bool) -> Value {
    json!({
        "session_id": entry.session_id,
        "project_id": entry.project_id,
        "project_tag": entry.project_tag,
        "title": entry.title,
        "created_at_ms": entry.created_at_ms,
        "updated_at_ms": entry.updated_at_ms,
        "archived_at_ms": entry.archived_at_ms,
        "unread": entry.unread,
        "archived": archived,
    })
}

/// Returns an indexed, project-scoped session page without opening session directories.
/// 返回项目范围内的索引会话页，不打开会话目录。
async fn list_authorized_sessions(
    invocation: &ValidatedInvocation,
    limit: u32,
    cursor: Option<String>,
) -> Result<Value, String> {
    let project_id = invocation.source_locator.project_id();
    let active = list_active_sessions_v1(&invocation.controller_runtime, project_id)
        .await
        .map_err(|error| format!("failed to list active sessions: {error}"))?;
    let archived = list_archived_sessions_v1(&invocation.controller_runtime, project_id)
        .await
        .map_err(|error| format!("failed to list archived sessions: {error}"))?;
    let mut entries = active
        .into_iter()
        .map(|entry| (entry, false))
        .chain(archived.into_iter().map(|entry| (entry, true)))
        .collect::<Vec<_>>();
    entries.sort_by(|(left, _), (right, _)| {
        right
            .updated_at_ms
            .cmp(&left.updated_at_ms)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    let start = match cursor {
        None => 0,
        Some(cursor) => entries
            .iter()
            .position(|(entry, _)| entry.session_id == cursor)
            .map(|position| position + 1)
            .ok_or_else(|| String::from("session list cursor is not valid for this project"))?,
    };
    let page_limit =
        usize::try_from(limit).map_err(|_| String::from("session list limit is invalid"))?;
    let page = entries
        .iter()
        .skip(start)
        .take(page_limit)
        .map(|(entry, archived)| session_list_entry_value(entry.clone(), *archived))
        .collect::<Vec<_>>();
    let next_cursor = if start + page.len() < entries.len() {
        page.last()
            .and_then(|value| value.get("session_id"))
            .and_then(Value::as_str)
            .map(String::from)
    } else {
        None
    };
    Ok(json!({
        "status": "ok",
        "project_id": project_id,
        "sessions": page,
        "next_cursor": next_cursor,
        "operation_id": invocation.operation_id,
    }))
}

/// Reads one exact bounded event range through the validated locator.
/// 通过已校验定位器读取一个精确有界事件范围。
fn read_authorized_event_range(
    invocation: &ValidatedInvocation,
    locator: &SessionV1StorageLocator,
    after_seq: Option<u64>,
    limit: u32,
) -> Result<Value, String> {
    let limit =
        usize::try_from(limit).map_err(|_| String::from("session read limit is invalid"))?;
    if limit == 0 {
        return Ok(json!({
            "status": "ok",
            "session_id": locator.session_id(),
            "project_id": locator.project_id(),
            "events": [],
            "operation_id": invocation.operation_id,
        }));
    }
    let state = locator
        .event_window_state()
        .map_err(|error| format!("failed to read target event frontier: {error}"))?;
    let first_seq = after_seq.unwrap_or(0).saturating_add(1);
    let last_seq = state.next_seq.saturating_sub(1).min(
        first_seq
            .saturating_add(u64::try_from(limit).unwrap_or(u64::MAX))
            .saturating_sub(1),
    );
    if first_seq == 0 || first_seq > last_seq {
        return Ok(json!({
            "status": "ok",
            "session_id": locator.session_id(),
            "project_id": locator.project_id(),
            "events": [],
            "next_seq": state.next_seq,
            "document_revision": state.document_revision,
            "operation_id": invocation.operation_id,
        }));
    }
    let from_seq = vulcan_session_core::v1::ids::Seq::try_new(first_seq)
        .map_err(|error| format!("invalid read start sequence: {error}"))?;
    let through_seq = vulcan_session_core::v1::ids::Seq::try_new(last_seq)
        .map_err(|error| format!("invalid read end sequence: {error}"))?;
    let range = locator
        .load_event_range(from_seq, through_seq)
        .map_err(|error| format!("failed to read bounded target event range: {error}"))?;
    event_range_value(
        invocation,
        locator,
        state.next_seq,
        state.document_revision,
        range,
    )
}

/// Serializes one exact event range without logging its potentially sensitive body.
/// 序列化精确事件范围，不记录其中可能敏感的正文。
fn event_range_value(
    invocation: &ValidatedInvocation,
    locator: &SessionV1StorageLocator,
    next_seq: u64,
    document_revision: u64,
    range: SessionV1EventRange,
) -> Result<Value, String> {
    let events = range
        .events
        .into_iter()
        .map(|event| {
            serde_json::to_value(event)
                .map_err(|error| format!("failed to serialize bounded session event: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "status": "ok",
        "session_id": locator.session_id(),
        "project_id": locator.project_id(),
        "events": events,
        "next_seq": next_seq,
        "document_revision": document_revision,
        "operation_id": invocation.operation_id,
    }))
}

/// Serializes one participant for the stable collaboration DTO.
/// 为稳定协作 DTO 序列化一个参与者。
fn participant_value(participant: &CollaborationParticipant) -> Result<Value, String> {
    serde_json::to_value(participant)
        .map_err(|error| format!("failed to serialize collaboration participant: {error}"))
}

/// Serializes the authoritative answer while retaining its delivery state.
/// 序列化权威答案并保留其交付状态。
fn answer_value(
    question: &vulcan_session_core::v1::collaboration::CollaborationQuestionRecord,
) -> Result<Option<Value>, String> {
    question
        .answer
        .as_ref()
        .map(|answer| {
            serde_json::to_value(answer)
                .map_err(|error| format!("failed to serialize collaboration answer: {error}"))
        })
        .transpose()
}

/// Returns one stable question DTO used by ask, wait, reply, and cancel.
/// 返回 ask、wait、reply、cancel 共同使用的稳定问题 DTO。
fn question_value(
    operation_id: &str,
    question: &vulcan_session_core::v1::collaboration::CollaborationQuestionRecord,
    delivery: Option<&vulcan_session_core::v1::collaboration::CollaborationDeliveryRecord>,
    revision: u64,
    status: &str,
) -> Result<Value, String> {
    let source = participant_value(&question.source)?;
    let target = participant_value(&question.target)?;
    let answer = answer_value(question)?;
    Ok(json!({
        "status": status,
        "operation_id": operation_id,
        "question_id": question.question_id.as_str(),
        "source": source,
        "target": target,
        "question_state": question.state,
        "revision": revision,
        "delivery": delivery,
        "answer": answer,
    }))
}

/// Returns a queryable question receipt when a later outbox repair cannot be persisted.
/// 当后续出站修复无法持久化时返回仍可查询的问题回执。
fn queryable_question_after_repair_failure(
    invocation: &ValidatedInvocation,
    question_id: &CollaborationQuestionId,
    delivery_id: &vulcan_session_core::v1::collaboration::CollaborationDeliveryId,
    detail: String,
) -> Result<Value, String> {
    let snapshot = invocation
        .source_handle
        .read(|document| {
            (
                document.question_snapshot(question_id).cloned(),
                document.delivery_snapshot(delivery_id).cloned(),
                document.memory_revision(),
            )
        })
        .map_err(|error| {
            format!(
                "collaboration question `{}` delivery `{}` requires repair; source snapshot read failed: {error}",
                question_id.as_str(),
                delivery_id.as_str()
            )
        })?;
    let Some(question) = snapshot.0 else {
        return Err(format!(
            "collaboration question `{}` delivery `{}` requires repair; source question is unavailable: {detail}",
            question_id.as_str(),
            delivery_id.as_str()
        ));
    };
    let mut value = question_value(
        &invocation.operation_id,
        &question,
        snapshot.1.as_ref(),
        snapshot.2,
        "repair_required",
    )?;
    if let Value::Object(fields) = &mut value {
        fields.insert(
            String::from("delivery_id"),
            Value::String(delivery_id.as_str().to_owned()),
        );
        fields.insert(String::from("repair_error"), Value::String(detail));
    }
    Ok(value)
}

/// Returns one durable delivery DTO with no hidden success path.
/// 返回一份可靠投递 DTO，不隐藏任何成功路径。
fn delivery_value(
    operation_id: &str,
    receipt: &vulcan_session_core::v1::collaboration::CollaborationDeliveryReceipt,
    status: &str,
) -> Value {
    json!({
        "status": status,
        "operation_id": operation_id,
        "delivery_id": receipt.delivery_id.as_str(),
        "question_id": receipt.question_id.as_str(),
        "delivery_state": receipt.state,
        "revision": receipt.revision,
        "event_seq": receipt.event_seq.value(),
    })
}

/// Applies one source-owned collaboration mutation under its lifecycle gate.
/// 在来源生命周期门内应用一次来源拥有的协作修改。
async fn mutate_handle<T, Mutation>(
    registry: &Arc<SessionRuntimeRegistry>,
    handle: &Arc<SessionHandle>,
    session_id: Uuid,
    mutation: Mutation,
) -> Result<SessionConditionalMutationReceipt<T>, String>
where
    T: Send + 'static,
    Mutation: FnOnce(
            &mut vulcan_session_core::v1::document::SessionDocument,
        )
            -> Result<IdempotentMutation<T>, vulcan_session_core::v1::errors::DomainError>
        + Send
        + 'static,
{
    mutate_handle_with_admission(
        registry,
        handle,
        session_id,
        SessionMutationAdmission::MetadataOnly,
        mutation,
    )
    .await
}

/// Applies one mutation with an explicit host admission class under the canonical lifecycle gate.
/// 在规范生命周期门内以明确的宿主准入类别应用一次修改。
async fn mutate_handle_with_admission<T, Mutation>(
    registry: &Arc<SessionRuntimeRegistry>,
    handle: &Arc<SessionHandle>,
    session_id: Uuid,
    admission: SessionMutationAdmission,
    mutation: Mutation,
) -> Result<SessionConditionalMutationReceipt<T>, String>
where
    T: Send + 'static,
    Mutation: FnOnce(
            &mut vulcan_session_core::v1::document::SessionDocument,
        )
            -> Result<IdempotentMutation<T>, vulcan_session_core::v1::errors::DomainError>
        + Send
        + 'static,
{
    if handle.session_id() != session_id {
        return Err(String::from(
            "collaboration mutation handle does not match its session identity",
        ));
    }
    // Keep lifecycle ownership in the blocking writer even if the caller abandons its wait.
    // 即使调用者放弃等待，也在阻塞写入器中保留生命周期所有权。
    let handle_for_write = Arc::clone(handle);
    lifecycle_write::with_canonical_handles(registry, vec![Arc::clone(handle)], move || {
        handle_for_write
            .mutate_durable_if_changed(
                admission,
                SessionFlushReason::MetadataChanged,
                move |document| Ok(session_change(mutation(document)?)),
            )
            .map_err(|error| format!("failed to persist source collaboration mutation: {error}"))
    })
    .await
}

/// Applies one source-owned mutation through the canonical source handle.
/// 通过来源规范句柄应用一次来源拥有的修改。
async fn mutate_source<T, Mutation>(
    invocation: &ValidatedInvocation,
    mutation: Mutation,
) -> Result<SessionConditionalMutationReceipt<T>, String>
where
    T: Send + 'static,
    Mutation: FnOnce(
            &mut vulcan_session_core::v1::document::SessionDocument,
        )
            -> Result<IdempotentMutation<T>, vulcan_session_core::v1::errors::DomainError>
        + Send
        + 'static,
{
    mutate_handle(
        &invocation.registry,
        &invocation.source_handle,
        invocation.source_session_id,
        mutation,
    )
    .await
}

/// Applies one target-owned collaboration mutation under its lifecycle gate.
/// 在目标生命周期门内应用一次目标拥有的协作修改。
async fn mutate_target<T, Mutation>(
    invocation: &ValidatedInvocation,
    target: &ResolvedSession,
    target_session_id: Uuid,
    mutation: Mutation,
) -> Result<SessionConditionalMutationReceipt<T>, String>
where
    T: Send + 'static,
    Mutation: FnOnce(
            &mut vulcan_session_core::v1::document::SessionDocument,
        )
            -> Result<IdempotentMutation<T>, vulcan_session_core::v1::errors::DomainError>
        + Send
        + 'static,
{
    mutate_handle(
        &invocation.registry,
        &target.handle,
        target_session_id,
        mutation,
    )
    .await
}

/// Persists a target projection only after the caller has appended the real UserInput fact.
/// 仅在调用方追加真实 UserInput 事实后持久化目标投影确认。
pub(crate) async fn session_v1_mark_collaboration_delivery_projected(
    state: SharedServiceBridgeState,
    target_session_id: Uuid,
    input: ProjectDeliveryInput,
) -> Result<CollaborationDeliveryReceipt, String> {
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(&state, target_session_id)
            .await
            .map_err(|error| format!("failed to load collaboration projection target: {error}"))?;
    let registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        Arc::clone(&guard.session_runtime_registry)
    };
    let receipt = mutate_handle_with_admission(
        &registry,
        &handle,
        target_session_id,
        SessionMutationAdmission::ControlPlane,
        move |document| document.mark_delivery_projected(input),
    )
    .await?;
    Ok(receipt.value)
}

/// Persists source-side delivery progress after a target receipt is durably observed.
/// 在可靠观测目标回执后持久化来源侧投递进度。
pub(crate) async fn session_v1_record_collaboration_delivery_progress(
    state: SharedServiceBridgeState,
    source_session_id: Uuid,
    input: RecordDeliveryProgressInput,
) -> Result<CollaborationDeliveryReceipt, String> {
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(&state, source_session_id)
            .await
            .map_err(|error| format!("failed to load collaboration progress source: {error}"))?;
    let registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        Arc::clone(&guard.session_runtime_registry)
    };
    let receipt = mutate_handle_with_admission(
        &registry,
        &handle,
        source_session_id,
        SessionMutationAdmission::ControlPlane,
        move |document| document.record_delivery_progress(input),
    )
    .await?;
    Ok(receipt.value)
}

/// Persists source answer delivery only after a real User or completed ToolResult fact exists.
/// 仅在真实 User 或已完成 ToolResult 事实存在后持久化来源答案交付游标。
pub(crate) async fn session_v1_deliver_collaboration_answer(
    state: SharedServiceBridgeState,
    source_session_id: Uuid,
    input: DeliverAnswerInput,
) -> Result<CollaborationAnswerDeliveryReceipt, String> {
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(&state, source_session_id)
            .await
            .map_err(|error| format!("failed to load collaboration answer source: {error}"))?;
    let registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        Arc::clone(&guard.session_runtime_registry)
    };
    let receipt = mutate_handle_with_admission(
        &registry,
        &handle,
        source_session_id,
        SessionMutationAdmission::ControlPlane,
        move |document| document.deliver_answer(input),
    )
    .await?;
    Ok(receipt.value)
}

/// Records one source question, accepts it on the target, and durably acknowledges the outbox.
/// 记录来源问题、在目标接纳并可靠确认出站投递。
async fn ask_question(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
    target_session_id: Uuid,
    body: String,
) -> Result<Value, String> {
    let target =
        resolve_authorized_target(invocation, target_session_id, CollaborationGrantAction::Ask)
            .await?;
    participants::require_message_admission(invocation, target_session_id, &target).await?;
    let source = source_participant(invocation).await?;
    let (execution_purpose, bound_turn_id) = target
        .handle
        .read(target_question_binding)
        .map_err(|error| format!("failed to read target turn binding: {error}"))?;
    let target_participant =
        target_participant(invocation, target_session_id, &target, bound_turn_id).await?;
    let operation_id = collaboration_operation_id(invocation)?;
    let question_id = collaboration_question_id(invocation)?;
    let delivery_id = collaboration_delivery_id(invocation)?;
    let now_ms = now_ms()?;
    let source_input = CreateQuestionInput {
        operation_id: operation_id.clone(),
        question_id: question_id.clone(),
        delivery_id: delivery_id.clone(),
        source: source.clone(),
        target: target_participant.clone(),
        body,
        execution_purpose,
        created_at_ms: now_ms,
    };
    let cross_project = target.locator.project_id() != invocation.source_locator.project_id();
    let source_session_id = invocation.source_session_id;
    let target_session_id_for_grant = target_session_id;
    let _created = mutate_source(invocation, move |document| {
        if cross_project
            && !document_grant_allows(
                document,
                source_session_id,
                target_session_id_for_grant,
                CollaborationGrantAction::Ask,
            )
        {
            return Err(
                vulcan_session_core::v1::errors::DomainError::CollaborationUnauthorized {
                    action: "ask_question_grant",
                    identity: target_session_id_for_grant.to_string(),
                },
            );
        }
        document.create_question(source_input)
    })
    .await?;

    let target_stopped = target
        .handle
        .read(|document| {
            matches!(
                document.collaboration_run_control(),
                CollaborationRunControl::Stopped
            )
        })
        .map_err(|error| format!("failed to read target collaboration run control: {error}"))?;
    let grant_still_allows_delivery = !cross_project
        || source_handle_grant_allows(
            &invocation.source_handle,
            invocation.source_session_id,
            target_session_id,
            CollaborationGrantAction::Ask,
        )?;
    let target_receipt = if target_stopped || !grant_still_allows_delivery {
        None
    } else {
        let accept_operation = operation_suffix(invocation, "accept")?;
        let accept_input = AcceptDeliveryInput {
            operation_id: accept_operation,
            delivery_id: delivery_id.clone(),
            question_id: question_id.clone(),
            source,
            target: target_participant,
            bound_turn_id,
            execution_purpose,
            accepted_at_ms: now_ms,
        };
        match mutate_target(invocation, &target, target_session_id, move |document| {
            let (current_purpose, current_turn_id) = target_question_binding(document);
            if current_purpose != accept_input.execution_purpose
                || current_turn_id != accept_input.bound_turn_id
            {
                return Err(
                    vulcan_session_core::v1::errors::DomainError::CollaborationStateViolation {
                        field: "collaboration.accept.target_turn_binding",
                    },
                );
            }
            participants::require_document_message_admission(
                document,
                &accept_input.source,
                &accept_input.target,
            )?;
            document.accept_delivery(accept_input)
        })
        .await
        {
            Ok(receipt) => Some(receipt),
            Err(error) => {
                let reason = if error.contains("grant") || error.contains("authorization") {
                    CollaborationDeliveryFailureReason::AuthorizationRevoked
                } else {
                    CollaborationDeliveryFailureReason::TargetMissing
                };
                let failure_operation = operation_suffix(invocation, "fail")?;
                let failure_delivery_id = delivery_id.clone();
                let failure_question_id = question_id.clone();
                let failure_repair = mutate_source(invocation, move |document| {
                    document.record_delivery_progress(RecordDeliveryProgressInput {
                        operation_id: failure_operation,
                        delivery_id: failure_delivery_id,
                        question_id: failure_question_id,
                        state: CollaborationDeliveryState::Failed,
                        failure: Some(reason),
                        target_revision: None,
                        target_event_seq: None,
                        updated_at_ms: now_ms,
                    })
                })
                .await;
                if let Err(failure_error) = failure_repair {
                    return queryable_question_after_repair_failure(
                        invocation,
                        &question_id,
                        &delivery_id,
                        format!(
                            "target delivery failed: {error}; source failure repair failed: {failure_error}"
                        ),
                    );
                }
                let (question, delivery, revision) = invocation
                    .source_handle
                    .read(|document| {
                        (
                            document.question_snapshot(&question_id).cloned(),
                            document.delivery_snapshot(&delivery_id).cloned(),
                            document.memory_revision(),
                        )
                    })
                    .map_err(|read_error| {
                        format!("failed to read failed question receipt: {read_error}")
                    })?;
                let question = question.ok_or_else(|| {
                    String::from("source question disappeared after failure repair")
                })?;
                return question_value(
                    &invocation.operation_id,
                    &question,
                    delivery.as_ref(),
                    revision,
                    "failed",
                );
            }
        }
    };

    if target_stopped || !grant_still_allows_delivery {
        let failure_operation = operation_suffix(invocation, "stopped")?;
        let failure_delivery_id = delivery_id.clone();
        let failure_question_id = question_id.clone();
        let failure_repair = mutate_source(invocation, move |document| {
            document.record_delivery_progress(RecordDeliveryProgressInput {
                operation_id: failure_operation,
                delivery_id: failure_delivery_id,
                question_id: failure_question_id,
                state: CollaborationDeliveryState::Failed,
                failure: Some(if target_stopped {
                    CollaborationDeliveryFailureReason::TargetStopped
                } else {
                    CollaborationDeliveryFailureReason::AuthorizationRevoked
                }),
                target_revision: None,
                target_event_seq: None,
                updated_at_ms: now_ms,
            })
        })
        .await;
        if let Err(error) = failure_repair {
            return queryable_question_after_repair_failure(
                invocation,
                &question_id,
                &delivery_id,
                format!("target delivery is unavailable and source failure repair failed: {error}"),
            );
        }
        let (question, delivery, revision) = invocation
            .source_handle
            .read(|document| {
                (
                    document.question_snapshot(&question_id).cloned(),
                    document.delivery_snapshot(&delivery_id).cloned(),
                    document.memory_revision(),
                )
            })
            .map_err(|error| format!("failed to read stopped question receipt: {error}"))?;
        let question = question
            .ok_or_else(|| String::from("source question disappeared after stopped repair"))?;
        return question_value(
            &invocation.operation_id,
            &question,
            delivery.as_ref(),
            revision,
            "failed",
        );
    }

    let target_receipt = target_receipt
        .ok_or_else(|| String::from("target acceptance did not produce a receipt"))?;
    let progress_operation = operation_suffix(invocation, "accepted")?;
    let progress_delivery_id = delivery_id.clone();
    let progress_question_id = question_id.clone();
    let source_ack = mutate_source(invocation, move |document| {
        document.record_delivery_progress(RecordDeliveryProgressInput {
            operation_id: progress_operation,
            delivery_id: progress_delivery_id,
            question_id: progress_question_id,
            state: CollaborationDeliveryState::Accepted,
            failure: None,
            target_revision: Some(target_receipt.value.revision),
            target_event_seq: Some(target_receipt.value.event_seq),
            updated_at_ms: now_ms,
        })
    })
    .await;
    if let Err(error) = source_ack {
        return queryable_question_after_repair_failure(
            invocation,
            &question_id,
            &delivery_id,
            format!("target acceptance committed but source acknowledgement failed: {error}"),
        );
    }
    let read_question_id = collaboration_question_id(invocation)?;
    let read_delivery_id = collaboration_delivery_id(invocation)?;
    let (question, delivery, revision) = invocation
        .source_handle
        .read(|document| {
            (
                document.question_snapshot(&read_question_id).cloned(),
                document.delivery_snapshot(&read_delivery_id).cloned(),
                document.memory_revision(),
            )
        })
        .map_err(|error| format!("failed to read accepted question receipt: {error}"))?;
    let question =
        question.ok_or_else(|| String::from("source question disappeared after acceptance"))?;
    launch::schedule_question_wakeup(app, invocation, question.question_id.clone());
    question_value(
        &invocation.operation_id,
        &question,
        delivery.as_ref(),
        revision,
        "ok",
    )
}

/// Settles one pending source question from the exact authorized target participant.
/// 由精确获授权目标参与者结算一条来源待处理问题。
async fn reply_question(
    invocation: &ValidatedInvocation,
    question_id_text: String,
    answer: String,
    unable_to_answer: bool,
) -> Result<Value, String> {
    let question_id = CollaborationQuestionId::try_new(question_id_text)
        .map_err(|error| format!("invalid collaboration question identity: {error}"))?;
    let (source_session_id, source_handle) = load_question_source(invocation, &question_id).await?;
    let (question, delivery, revision) = read_question_source(&source_handle, &question_id)?;
    if question.target.session_id != invocation.source_session_id.to_string() {
        return Err(String::from(
            "reply caller is not the authorized question target",
        ));
    }
    let actual_participant = current_participant_for_binding(
        invocation,
        &question.target,
        Some(invocation.source_turn_id),
    )
    .await?;
    invocation
        .source_handle
        .read(|document| {
            prove_active_question_execution_binding(
                document,
                &question,
                &actual_participant,
                TurnId::try_new(invocation.source_turn_id)
                    .map_err(|error| format!("invalid reply target turn identity: {error}"))?,
                invocation
                    .source_api_call_id
                    .ok_or_else(|| String::from("reply requires an admitted model API identity"))?,
            )
        })
        .map_err(|error| format!("failed to read reply target execution binding: {error}"))??;
    // The durable execution proof above separately proves the current turn; settlement accepts the persisted target identity.
    // 可靠执行证明已经单独证明当前回合，因此结算只接受持久化的问题目标身份。
    let responder = question.target.clone();
    if question.source.project_id != invocation.source_locator.project_id()
        && !source_handle_grant_allows(
            &source_handle,
            source_session_id,
            invocation.source_session_id,
            CollaborationGrantAction::Reply,
        )?
    {
        return Err(String::from(
            "reply requires an active source-owned collaboration grant",
        ));
    }
    let source_operation_id = collaboration_operation_id(invocation)?;
    let answered_at_ms = now_ms()?;
    let mutation_question_id = question_id.clone();
    let mutation_operation_id = source_operation_id.clone();
    let cross_project = question.source.project_id != invocation.source_locator.project_id();
    let source_session_id_for_grant = source_session_id;
    let target_session_id_for_grant = invocation.source_session_id;
    let result = mutate_handle_with_admission(
        &invocation.registry,
        &source_handle,
        source_session_id,
        SessionMutationAdmission::ControlPlane,
        move |document| {
            if cross_project
                && !document_grant_allows(
                    document,
                    source_session_id_for_grant,
                    target_session_id_for_grant,
                    CollaborationGrantAction::Reply,
                )
            {
                return Err(
                    vulcan_session_core::v1::errors::DomainError::CollaborationUnauthorized {
                        action: "reply_question_grant",
                        identity: target_session_id_for_grant.to_string(),
                    },
                );
            }
            document.reply_question(ReplyQuestionInput {
                operation_id: mutation_operation_id,
                question_id: mutation_question_id,
                responder,
                body: answer,
                answered_at_ms,
            })
        },
    )
    .await?;
    settle_and_acknowledge_delivery(
        invocation,
        &source_handle,
        source_session_id,
        &question,
        &delivery,
        source_operation_id,
        result.value.event_seq,
        CollaborationTargetSettlementState::Answered,
    )
    .await?;
    let (question, delivery, current_revision) =
        read_question_source(&source_handle, &question_id)?;
    let mut value = question_value(
        &invocation.operation_id,
        &question,
        Some(&delivery),
        current_revision.max(revision).max(result.revision),
        "ok",
    )?;
    if let Value::Object(fields) = &mut value {
        fields.insert(
            String::from("unable_to_answer"),
            Value::Bool(unable_to_answer),
        );
    }
    Ok(value)
}

/// Cancels one pending question by its original source participant.
/// 由原始来源参与者撤销一条待处理问题。
async fn cancel_question(
    invocation: &ValidatedInvocation,
    question_id_text: String,
    reason: String,
) -> Result<Value, String> {
    let question_id = CollaborationQuestionId::try_new(question_id_text)
        .map_err(|error| format!("invalid collaboration question identity: {error}"))?;
    let (source_session_id, source_handle) = load_question_source(invocation, &question_id).await?;
    if source_session_id != invocation.source_session_id {
        return Err(String::from(
            "cancel caller is not the question source session",
        ));
    }
    let (question, delivery, revision) = read_question_source(&source_handle, &question_id)?;
    let requester =
        current_session_participant(invocation, Some(invocation.source_turn_id)).await?;
    if question.source != requester {
        return Err(String::from(
            "cancel caller is not the authorized question source",
        ));
    }
    let source_operation_id = collaboration_operation_id(invocation)?;
    let cancelled_at_ms = now_ms()?;
    let mutation_question_id = question_id.clone();
    let mutation_operation_id = source_operation_id.clone();
    let result = mutate_handle_with_admission(
        &invocation.registry,
        &source_handle,
        source_session_id,
        SessionMutationAdmission::ControlPlane,
        move |document| {
            document.cancel_question(CancelQuestionInput {
                operation_id: mutation_operation_id,
                question_id: mutation_question_id,
                requester,
                reason,
                cancelled_at_ms,
            })
        },
    )
    .await?;
    settle_and_acknowledge_delivery(
        invocation,
        &source_handle,
        source_session_id,
        &question,
        &delivery,
        source_operation_id,
        result.value.event_seq,
        CollaborationTargetSettlementState::Cancelled,
    )
    .await?;
    let (question, delivery, current_revision) =
        read_question_source(&source_handle, &question_id)?;
    question_value(
        &invocation.operation_id,
        &question,
        Some(&delivery),
        current_revision.max(revision),
        "ok",
    )
}

/// Cancels one source-owned question from the trusted LocalUser RPC boundary.
/// 从受信任的本地用户 RPC 边界撤销一条来源拥有的问题。
///
/// This entry point deliberately has no model invocation context. It resolves the source from the
/// service-issued question id, validates the user-operation namespace, and then reuses the same
/// source-settlement, target-settlement, and source-acknowledgement chain as model cancellation.
/// 此入口刻意不接收模型调用上下文，而是从服务签发的问题 ID 定位来源、校验用户操作命名空间，
/// 再复用模型撤销使用的来源结算、目标结算和来源确认链。
///
/// The root-session argument is only a trusted locator hint. The service still verifies the root
/// index, derives the exact embedded child locator, and checks the persisted source participant.
/// 根会话参数仅作为受信任的定位提示；服务仍会校验根索引、派生精确的内嵌子会话定位器，并核对持久来源参与者。
pub(crate) async fn service_cancel_collaboration_question(
    state: SharedServiceBridgeState,
    question_id_text: String,
    source_root_session_id_text: String,
    operation_id_text: String,
    reason: String,
) -> Result<Value, String> {
    let question_id = CollaborationQuestionId::try_new(question_id_text)
        .map_err(|error| format!("invalid collaboration question identity: {error}"))?;
    let source_session_id = question_source_session_id(question_id.as_str())?;
    let source_root_session_id = Uuid::parse_str(&source_root_session_id_text)
        .map_err(|error| format!("invalid collaboration source root session identity: {error}"))?;
    let operation_id = CollaborationOperationId::try_new(operation_id_text)
        .map_err(|error| format!("invalid user cancellation operation identity: {error}"))?;
    if !operation_id
        .as_str()
        .starts_with(USER_QUESTION_CANCEL_OPERATION_PREFIX)
        || operation_id.as_str().len() == USER_QUESTION_CANCEL_OPERATION_PREFIX.len()
    {
        return Err(format!(
            "user cancellation operation identity must start with `{USER_QUESTION_CANCEL_OPERATION_PREFIX}`"
        ));
    }
    let controller_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?
    };
    let registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        Arc::clone(&guard.session_runtime_registry)
    };
    let root_indexed = locate_optional_session_v1_storage(
        &controller_runtime,
        &source_root_session_id.to_string(),
    )
    .await
    .map_err(|error| format!("failed to resolve user cancellation source root locator: {error}"))?;
    if root_indexed.is_none() {
        return Err(String::from(
            "user cancellation source root is not an indexed session",
        ));
    }
    let root_handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(&state, source_root_session_id)
            .await
            .map_err(|error| format!("failed to load user cancellation source root: {error}"))?;
    let root_locator = root_handle.storage_locator().map_err(|error| {
        format!("failed to resolve user cancellation source root locator: {error}")
    })?;
    let (source_handle, source_locator) = if source_session_id == source_root_session_id {
        (Arc::clone(&root_handle), root_locator.clone())
    } else {
        let source_locator =
            locate_embedded_task_session_v1(&root_locator, &source_session_id.to_string())
                .map_err(|error| {
                    format!("failed to locate embedded user cancellation source: {error}")
                })?;
        let source_handle =
            load_embedded_canonical_handle(&registry, source_locator.clone(), source_session_id)?;
        (source_handle, source_locator)
    };
    let source_locator = source_handle
        .storage_locator()
        .map_err(|error| format!("failed to resolve user cancellation source locator: {error}"))?;
    let source = source_handle
        .read(|document| {
            let question = document
                .question_snapshot(&question_id)
                .cloned()
                .ok_or_else(|| String::from("collaboration question was not found"))?;
            let delivery = document
                .delivery_snapshot(&question.delivery_id)
                .cloned()
                .ok_or_else(|| String::from("collaboration question delivery was not found"))?;
            let generation = document.segment_generation();
            Ok::<_, String>((question, delivery, generation, document.memory_revision()))
        })
        .map_err(|error| format!("failed to read user cancellation source: {error}"))??;
    let (question, delivery, source_generation, revision) = source;
    let expected_task_id = if source_session_id == source_root_session_id {
        None
    } else {
        Some(
            SubagentTaskId::try_new(source_session_id.to_string())
                .map_err(|error| format!("invalid embedded source task identity: {error}"))?,
        )
    };
    if question.source.session_id != source_session_id.to_string()
        || question.source.root_session_id != source_root_session_id.to_string()
        || question.source.task_id != expected_task_id
        || question.source.project_id != source_locator.project_id()
        || question.source.segment_generation != source_generation
        || source_locator.session_id() != source_session_id.to_string()
        || locator_generation(&source_locator)? != source_generation
    {
        return Err(String::from(
            "user cancellation question source does not match its canonical indexed locator",
        ));
    }
    let invocation = recovery_invocation(
        Arc::clone(&state),
        controller_runtime,
        registry,
        source_session_id,
        Arc::clone(&source_handle),
    )?;
    let requester = question.source.clone();
    let cancelled_at_ms = now_ms()?;
    let mutation_question_id = question_id.clone();
    let mutation_operation_id = operation_id.clone();
    let result = mutate_handle_with_admission(
        &invocation.registry,
        &source_handle,
        source_session_id,
        SessionMutationAdmission::ControlPlane,
        move |document| {
            document.cancel_question(CancelQuestionInput {
                operation_id: mutation_operation_id,
                question_id: mutation_question_id,
                requester,
                reason,
                cancelled_at_ms,
            })
        },
    )
    .await?;
    let mut settlement_invocation = invocation.clone();
    settlement_invocation.operation_id = operation_id.as_str().to_owned();
    settle_and_acknowledge_delivery(
        &settlement_invocation,
        &source_handle,
        source_session_id,
        &question,
        &delivery,
        operation_id.clone(),
        result.value.event_seq,
        CollaborationTargetSettlementState::Cancelled,
    )
    .await?;
    let (question, delivery, current_revision) =
        read_question_source(&source_handle, &question_id)?;
    question_value(
        operation_id.as_str(),
        &question,
        Some(&delivery),
        current_revision.max(revision),
        "ok",
    )
}

/// Returns the current question snapshot or waits for a durable settlement revision.
/// 返回当前问题快照，或等待可靠结算修订。
async fn wait_question(
    invocation: &ValidatedInvocation,
    question_id_text: String,
    after_revision: Option<u64>,
    timeout_ms: u64,
) -> Result<Value, String> {
    let question_id = CollaborationQuestionId::try_new(question_id_text)
        .map_err(|error| format!("invalid collaboration question identity: {error}"))?;
    let (_source_session_id, source_handle) =
        load_question_source(invocation, &question_id).await?;
    let (initial_question, _initial_delivery, _initial_revision) =
        read_question_source(&source_handle, &question_id)?;
    let caller_is_source = initial_question.source.session_id
        == invocation.source_session_id.to_string()
        && same_participant_scope(
            &initial_question.source,
            &current_participant_for_binding(invocation, &initial_question.source, None).await?,
        );
    let caller_is_target = initial_question.target.session_id
        == invocation.source_session_id.to_string()
        && same_participant_scope(
            &initial_question.target,
            &current_participant_for_binding(invocation, &initial_question.target, None).await?,
        );
    if caller_is_target {
        let actual_participant = current_participant_for_binding(
            invocation,
            &initial_question.target,
            Some(invocation.source_turn_id),
        )
        .await?;
        let actual_turn_id = TurnId::try_new(invocation.source_turn_id)
            .map_err(|error| format!("invalid wait target turn identity: {error}"))?;
        invocation
            .source_handle
            .read(|document| {
                if initial_question.state == CollaborationQuestionState::Pending {
                    prove_active_question_execution_binding(
                        document,
                        &initial_question,
                        &actual_participant,
                        actual_turn_id,
                        invocation.source_api_call_id.ok_or_else(|| {
                            String::from("question wait requires an admitted model API identity")
                        })?,
                    )
                } else {
                    prove_settled_question_delivery(document, &initial_question)
                }
            })
            .map_err(|error| format!("failed to read wait target execution binding: {error}"))??;
    }
    if !caller_is_source && !caller_is_target {
        return Err(String::from(
            "wait caller is not authorized for the question",
        ));
    }
    // The cursor tracks the question's own collaboration state, never the owning session's revision.
    // A caller-supplied cursor resumes from that exact content position; a caller without one declares
    // no baseline and therefore observes the moment it resumes, which is a timeout when nothing moved.
    // 游标跟踪问题自身的协作状态，绝不使用所属会话的修订号。调用方给出游标时从该精确内容位置恢复；
    // 未给出时不声明任何基线，因此它观察到的是恢复的那一刻——问题没有变化时即为超时。
    let baseline = after_revision;
    let deadline = tokio::time::Instant::now()
        .checked_add(std::time::Duration::from_millis(timeout_ms))
        .ok_or_else(|| String::from("question wait timeout exceeds runtime instant range"))?;
    loop {
        // Explicit wait returns a newly accepted actionable inbound question before the original
        // question snapshot, without marking it projected or changing its durable state.
        // 显式等待优先返回新接纳的可行动入站问题，不提前标记投影，也不改变持久状态。
        if let Some(mut incoming) = incoming_question_snapshot(invocation).await? {
            if let Value::Object(fields) = &mut incoming {
                fields.insert(
                    String::from("waited_question_id"),
                    Value::String(question_id.as_str().to_owned()),
                );
                fields.insert(
                    String::from("wait_status"),
                    Value::String(String::from("incoming")),
                );
            }
            return Ok(incoming);
        }
        let (question, delivery, _) = read_question_for_model(&source_handle, &question_id)?;
        let cursor = question_wait_cursor(&question, &delivery)?;
        if caller_is_source || caller_is_target {
            // A settled question and a genuinely advanced cursor both end the wait; a cursor that the
            // waiter's own ToolCall advanced no longer can, because it never enters this cursor.
            // 已结算问题与真实推进的游标都会结束等待；被等待者自身工具调用推进的会话修订不再能结束
            // 等待，因为它从不进入该游标。
            let resumed = baseline.is_some_and(|previous| previous != cursor);
            if question.state != CollaborationQuestionState::Pending || resumed {
                return question_value(
                    &invocation.operation_id,
                    &question,
                    Some(&delivery),
                    cursor,
                    "ok",
                );
            }
        } else {
            return Err(String::from(
                "wait caller is not authorized for the question",
            ));
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return question_value(
                &invocation.operation_id,
                &question,
                Some(&delivery),
                cursor,
                "timeout",
            );
        }
        let remaining = deadline.saturating_duration_since(now);
        tokio::time::sleep(remaining.min(std::time::Duration::from_millis(50))).await;
    }
}

/// Returns the stable content cursor of one question and its delivery.
/// 返回单个问题及其投递的稳定内容游标。
///
/// # Parameters
/// 参数
/// `question` is the durable question record and `delivery` its exact delivery record.
/// `question` 是可靠问题记录，`delivery` 是其精确投递记录。
///
/// # Returns
/// 返回值
/// Returns a deterministic 64-bit FNV-1a digest of the two serialized records. The value changes only
/// when those collaboration facts change, so a wait resumed from a returned cursor keeps waiting while
/// nothing moved, and a session revision advanced by the caller's own ToolCall cannot wake it.
/// 返回两条序列化记录的确定性 64 位 FNV-1a 摘要。只有这些协作事实变化时该值才变化，因此以上次返回
/// 的游标恢复的等待在没有变化时继续等待，而调用方自身工具调用推进的会话修订不会唤醒它。
///
/// # Notes
/// 说明
/// The digest is deliberately local: it is an opaque resume token for one waiter, never a durable
/// identity and never persisted, so its exact value carries no cross-version contract.
/// 该摘要刻意保持本地语义：它是单个等待者的不透明恢复令牌，不是可靠身份也从不持久化，因此其具体取值
/// 不构成跨版本契约。
fn question_wait_cursor(
    question: &vulcan_session_core::v1::collaboration::CollaborationQuestionRecord,
    delivery: &vulcan_session_core::v1::collaboration::CollaborationDeliveryRecord,
) -> Result<u64, String> {
    let encoded = serde_json::to_vec(&(question, delivery))
        .map_err(|error| format!("failed to encode the question wait cursor: {error}"))?;
    // Offset basis and prime are the canonical FNV-1a 64-bit constants.
    // 偏移基与质数是 FNV-1a 64 位的规范常量。
    let mut digest = 0xcbf2_9ce4_8422_2325u64;
    for byte in encoded {
        digest ^= u64::from(byte);
        digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Ok(digest)
}

/// Finds one accepted inbound question without changing its delivery state.
/// 查找一条已接纳的入站问题且不改变其投递状态。
async fn incoming_question_snapshot(
    invocation: &ValidatedInvocation,
) -> Result<Option<Value>, String> {
    let mut incoming = invocation
        .source_handle
        .read(|document| {
            document
                .collaboration_state()
                .inbound_deliveries
                .values()
                .filter(|delivery| {
                    delivery.target.session_id == invocation.source_session_id.to_string()
                        && delivery.state == CollaborationDeliveryState::Accepted
                        && delivery.settlement.is_none()
                        && match delivery.execution_purpose {
                            CollaborationExecutionPurpose::InTurnQuestion => {
                                delivery.bound_turn_id.map(TurnId::value)
                                    == Some(invocation.source_turn_id)
                            }
                            CollaborationExecutionPurpose::NaturallyIdleQuestion => document
                                .collaboration_state()
                                .question_executions
                                .values()
                                .any(|execution| {
                                    execution.question_id == delivery.question_id
                                        && execution.state
                                            == CollaborationQuestionExecutionState::Running
                                        && execution.execution_turn_id.map(TurnId::value)
                                            == Some(invocation.source_turn_id)
                                }),
                            CollaborationExecutionPurpose::NormalTask => false,
                        }
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .map_err(|error| format!("failed to read incoming collaboration delivery: {error}"))?;
    // Accepted target revisions preserve admission order; opaque delivery IDs do not encode time.
    // 已接纳目标修订保留准入顺序；不透明投递 ID 不编码时间。
    incoming.sort_by(|left, right| {
        left.target_revision
            .cmp(&right.target_revision)
            .then_with(|| left.delivery_id.cmp(&right.delivery_id))
    });
    for inbound in incoming {
        let question_id = CollaborationQuestionId::try_new(inbound.question_id.as_str().to_owned())
            .map_err(|error| format!("invalid inbound question identity: {error}"))?;
        let (_source_session_id, source_handle) =
            load_question_source(invocation, &question_id).await?;
        let (question, delivery, revision) = read_question_for_model(&source_handle, &question_id)?;
        if question.question_id != inbound.question_id
            || question.target != inbound.target
            || question.source != inbound.source
            || delivery.delivery_id != inbound.delivery_id
        {
            return Err(String::from(
                "inbound collaboration delivery does not match its source question",
            ));
        }
        if question.state != CollaborationQuestionState::Pending {
            continue;
        }
        let mut value = question_value(
            &invocation.operation_id,
            &question,
            Some(&delivery),
            revision,
            "incoming",
        )?;
        if let Value::Object(fields) = &mut value {
            fields.insert(
                String::from("delivery_id"),
                Value::String(inbound.delivery_id.as_str().to_owned()),
            );
            fields.insert(String::from("question"), Value::String(question.body));
            fields.insert(
                String::from("target_revision"),
                Value::from(inbound.target_revision),
            );
            fields.insert(
                String::from("target_delivery_state"),
                serde_json::to_value(inbound.state).map_err(|error| {
                    format!("failed to serialize incoming delivery state: {error}")
                })?,
            );
        }
        return Ok(Some(value));
    }
    Ok(None)
}

#[async_trait]
impl NativeAgentCollaborationRuntime for SessionV1NativeAgentCollaborationRuntime {
    /// Executes child compaction through the authoritative document and returns its committed snapshot.
    /// 通过权威文档执行子压缩，并返回其已提交快照。
    /// The trusted request retains original ownership; unavailable or replaced owners fail explicitly.
    /// 可信请求保留原归属；所有者不可用或被替换时明确失败。
    async fn compact_child_task(
        &self,
        request: vulcan_agent_runtime::agent_runtime::prompt_compaction::ChildTaskCompactionRequest,
    ) -> Result<Option<vulcan_core::types::ConversationSnapshot>, String> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| String::from("Collaboration host has shut down."))?;
        child_compaction::compact(&self.app, &state, request).await
    }
    /// Publishes initial creation under the original parent before admitting the child listener.
    /// 在原父任务下发布初始创建后才准入子监听器。
    /// Returns the reliable parent reservation receipt after complete host ownership handoff.
    /// 完成宿主所有权交接后返回可靠父预留回执。
    async fn create_child_task(
        &self,
        reservation: ChildTaskReservationRequest,
        preparation: vulcan_agent_runtime::agent_runtime::task_runtime::collaboration_lifecycle::ChildExecutionProfileCaptureRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String> {
        child_creation::create(self, reservation, preparation).await
    }
    /// Captures actual child configuration through its canonical, currently owned document.
    /// 通过子代理当前拥有的规范文档捕获实际配置。
    async fn capture_child_execution_profile(
        &self,
        request: vulcan_agent_runtime::agent_runtime::task_runtime::collaboration_lifecycle::ChildExecutionProfileCaptureRequest,
    ) -> Result<(), String> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| String::from("Collaboration host has shut down."))?;
        child_profile::capture(&state, request).await
    }
    /// Executes one typed command after source identity and authorization checks.
    /// 在来源身份与授权检查后执行一条类型化命令。
    async fn invoke(
        &self,
        context: VulcanToolInvocationContext,
        command: AgentCollaborationCommand,
    ) -> Result<Value, String> {
        command.validate()?;
        let expected_tool_id = command_tool_id(&command);
        let invocation = self.validate_invocation(&context, expected_tool_id).await?;
        match command {
            AgentCollaborationCommand::ListSessions(args) => {
                list_authorized_sessions(&invocation, args.limit, args.cursor).await
            }
            AgentCollaborationCommand::ReadSession(args) => {
                let target_session_id = Uuid::parse_str(&args.session_id)
                    .map_err(|error| format!("invalid target session_id: {error}"))?;
                let target_locator = resolve_read_target(&invocation, target_session_id).await?;
                read_authorized_event_range(
                    &invocation,
                    &target_locator,
                    args.after_seq,
                    args.limit,
                )
            }
            AgentCollaborationCommand::AskQuestion(args) => {
                let target_session_id = Uuid::parse_str(&args.target_session_id)
                    .map_err(|error| format!("invalid target session_id: {error}"))?;
                ask_question(&self.app, &invocation, target_session_id, args.question).await
            }
            AgentCollaborationCommand::ReplyQuestion(args) => {
                reply_question(
                    &invocation,
                    args.question_id,
                    args.answer,
                    args.unable_to_answer,
                )
                .await
            }
            AgentCollaborationCommand::WaitQuestion(args) => {
                wait_question(
                    &invocation,
                    args.question_id,
                    args.after_revision,
                    args.timeout_ms,
                )
                .await
            }
            AgentCollaborationCommand::CancelQuestion(args) => {
                cancel_question(&invocation, args.question_id, args.reason).await
            }
            AgentCollaborationCommand::SendNotification(args) => {
                let target_session_id =
                    Uuid::parse_str(&args.target_session_id).map_err(|error| {
                        format!("invalid notification target session identity: {error}")
                    })?;
                notifications::send_notification(&invocation, target_session_id, args.message).await
            }
            AgentCollaborationCommand::AskParent(args) => {
                let parent_session_id = participants::verified_direct_parent(&invocation).await?;
                ask_question(&self.app, &invocation, parent_session_id, args.question).await
            }
            AgentCollaborationCommand::SendSessionMessage(args) => {
                session_messages::send_session_message(&self.app, &invocation, args).await
            }
            AgentCollaborationCommand::WaitSession(args) => {
                session_wait::wait_session(&invocation, args).await
            }
        }
    }

    /// Persists a parent-bound child reservation after validating the real ToolCall source.
    /// 在验证真实 ToolCall 来源后持久化按父回合绑定的子任务预留。
    async fn reserve_child_task(
        &self,
        request: ChildTaskReservationRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String> {
        let invocation = validate_host_invocation(
            &self.state,
            &request.invocation,
            &request.operation_id,
            None,
            None,
        )?;
        // The host resolves ownership after DelegateStarted created the canonical child document.
        // 宿主在 DelegateStarted 建立规范子文档后解析归属。
        let parent_turn_id =
            TurnId::try_new(invocation.source_turn_id).map_err(|error| error.to_string())?;
        let task_runtime = invocation
            .state
            .lock()
            .map_err(|error| error.to_string())?
            .agent_runtime
            .as_ref()
            .ok_or("agent runtime is unavailable")?
            .task_runtime
            .clone();
        let facts = task_runtime.active_task_v1_execution_facts(
            request.execution.task_session_id,
            &request.execution.execution_id,
        )?;
        if facts.parent_session_id != invocation.source_session_id {
            return Err(String::from(
                "child execution is not owned by the source tool session",
            ));
        }
        let child_handle = crate::session_goal_lifecycle::registered_session_v1_handle_from_state(
            &invocation.state,
            request.execution.task_session_id,
        )?;
        let child = participants::participant_for_handle(
            &invocation,
            request.execution.task_session_id,
            &child_handle,
            None,
        )
        .await?;
        validate_child_lifecycle_binding(&invocation, &request.execution, parent_turn_id, &child)
            .await?;
        let execution_id =
            CollaborationExecutionId::try_new(request.execution.execution_id.clone())
                .map_err(|error| error.to_string())?;
        // Retain the exact physical child for admission revalidation under the parent mutation lock.
        // 保留精确物理子会话，以便在父修改锁内重新验证准入。
        let task_session_id = request.execution.task_session_id;
        let task_id = request.execution.task_id.clone();
        let source_tool_name = invocation.source_tool_id.clone();
        let operation_id = request.operation_id.clone();
        let reserved_at_ms = request.reserved_at_ms;
        let expected_generation = invocation
            .source_handle
            .read(|document| {
                require_open_parent_turn(document, parent_turn_id)
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>(document.segment_generation())
            })
            .map_err(|error| format!("failed to read child reservation parent turn: {error}"))??;
        let receipt = mutate_handle_with_admission(
            &invocation.registry,
            &invocation.source_handle,
            invocation.source_session_id,
            SessionMutationAdmission::ControlPlane,
            move |document| {
                if document.segment_generation() != expected_generation {
                    return Err(
                        vulcan_session_core::v1::errors::DomainError::CollaborationStateViolation {
                            field: "collaboration.child_task.parent_segment_generation",
                        },
                    );
                }
                require_open_parent_turn(document, parent_turn_id)?;
                // A cancelled startup may lose its active lease while this asynchronous mutation waits.
                // 此异步修改等待期间，被取消启动可能失去活动租约。
                // Recheck before commit so an already released execution cannot add a late obligation.
                // 提交前重新检查，防止已释放执行补写迟到义务。
                task_runtime
                    .active_task_v1_execution_facts(task_session_id, execution_id.as_str())
                    .map_err(|_| {
                        vulcan_session_core::v1::errors::DomainError::CollaborationStateViolation {
                            field: "collaboration.child_task.inactive_reservation",
                        }
                    })?;
                document.reserve_child_task(ReserveChildTaskInput {
                    source_tool_name,
                    operation_id,
                    execution_id,
                    task_id,
                    parent_turn_id,
                    child,
                    reserved_at_ms,
                })
            },
        )
        .await?;
        Ok(child_task_lifecycle_receipt(
            collaboration_operation_id(&invocation)?,
            receipt,
        ))
    }

    /// Activates a previously reserved child after rechecking its durable binding.
    /// 在重新检查可靠绑定后激活已预留的子任务。
    async fn activate_child_task(
        &self,
        request: ChildTaskActivationRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String> {
        let invocation = validate_host_invocation(
            &self.state,
            &request.invocation,
            &request.operation_id,
            Some("activate"),
            Some(&request.execution),
        )?;
        // Freeze the runtime execution independently of its reusable task session.
        // 冻结运行时执行身份，与可复用任务会话分离。
        let execution_id =
            CollaborationExecutionId::try_new(request.execution.execution_id.clone())
                .map_err(|error| error.to_string())?;
        let task_id = request.execution.task_id.clone();
        let (parent_turn_id, child) = invocation
            .source_handle
            .read(|document| stored_child_lifecycle_binding(document, &execution_id, &task_id))
            .map_err(|error| format!("failed to read child activation binding: {error}"))??;
        validate_child_lifecycle_binding(&invocation, &request.execution, parent_turn_id, &child)
            .await?;
        let operation_id = request.operation_id.clone();
        let active_at_ms = request.active_at_ms;
        let receipt = mutate_handle_with_admission(
            &invocation.registry,
            &invocation.source_handle,
            invocation.source_session_id,
            SessionMutationAdmission::ControlPlane,
            move |document| {
                validate_stored_child_binding(
                    document,
                    &execution_id,
                    &task_id,
                    parent_turn_id,
                    &child,
                )?;
                document.activate_child_task(ActivateChildTaskInput {
                    operation_id,
                    execution_id,
                    task_id,
                    active_at_ms,
                })
            },
        )
        .await?;
        Ok(child_task_lifecycle_receipt(request.operation_id, receipt))
    }

    /// Terminally settles a child after rechecking its durable binding and outcome phase.
    /// 在重新检查可靠绑定与终态阶段后结算子任务。
    async fn terminal_child_task(
        &self,
        request: ChildTaskTerminalRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String> {
        // Only the exact durable content publisher may settle successful child work.
        // 只有精确持久正文发布器可以结算成功的子任务。
        if request.outcome == CollaborationChildTaskTerminalOutcome::Completed {
            return Err(String::from(
                "Successful child completion must be committed with its exact durable return.",
            ));
        }
        let operation_suffix = child_terminal_operation_suffix(request.outcome);
        let invocation = validate_host_invocation(
            &self.state,
            &request.invocation,
            &request.operation_id,
            Some(operation_suffix),
            Some(&request.execution),
        )?;
        // Freeze the runtime execution independently of its reusable task session.
        // 冻结运行时执行身份，与可复用任务会话分离。
        let execution_id =
            CollaborationExecutionId::try_new(request.execution.execution_id.clone())
                .map_err(|error| error.to_string())?;
        let task_id = request.execution.task_id.clone();
        let (parent_turn_id, child) = invocation
            .source_handle
            .read(|document| stored_child_lifecycle_binding(document, &execution_id, &task_id))
            .map_err(|error| format!("failed to read child terminal binding: {error}"))??;
        validate_child_lifecycle_binding(&invocation, &request.execution, parent_turn_id, &child)
            .await?;
        let operation_id = request.operation_id.clone();
        let outcome = request.outcome;
        let terminal_at_ms = request.terminal_at_ms;
        let receipt = mutate_handle_with_admission(
            &invocation.registry,
            &invocation.source_handle,
            invocation.source_session_id,
            SessionMutationAdmission::ControlPlane,
            move |document| {
                validate_stored_child_binding(
                    document,
                    &execution_id,
                    &task_id,
                    parent_turn_id,
                    &child,
                )?;
                document.terminal_child_task(TerminalChildTaskInput {
                    operation_id,
                    execution_id,
                    task_id,
                    outcome,
                    terminal_at_ms,
                })
            },
        )
        .await?;
        Ok(child_task_lifecycle_receipt(request.operation_id, receipt))
    }

    /// Waits for an incoming question through the lifecycle-owned wake helper.
    /// 通过生命周期 owner 的唤醒辅助器等待传入问题。
    async fn wait_for_incoming_question(
        &self,
        context: VulcanToolInvocationContext,
    ) -> Result<Value, String> {
        let invocation = self
            .validate_invocation(&context, WAIT_AGENT_TOOL_ID)
            .await?;
        loop {
            if let Some(value) = incoming_question_snapshot(&invocation).await? {
                return Ok(value);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

impl SessionV1NativeAgentCollaborationRuntime {
    /// Validates one command's trusted source context against its canonical ToolCall event.
    /// 根据规范 ToolCall 事件校验一条命令的可信来源上下文。
    async fn validate_invocation(
        &self,
        context: &VulcanToolInvocationContext,
        expected_tool_id: &str,
    ) -> Result<ValidatedInvocation, String> {
        validate_invocation(&self.state, context, expected_tool_id)
    }
}
