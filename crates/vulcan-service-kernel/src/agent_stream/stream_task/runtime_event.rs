//! Runtime stream event orchestration.
//! 运行时流事件编排。

use super::*;
use vulcan_session_core::ExecutionScope;

/// Request-authoritative Hook routing policy for root runtime stream events.
/// 根运行时流事件使用的请求权威 Hook 路由策略。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RuntimeEventHookPolicy {
    /// Tool runtime events belong to one immutable request-scoped conversation turn.
    /// 工具运行时事件属于一个不可变的请求级会话回合。
    ConversationTurn {
        /// Exact precheck turn identifier used by every Hook notification from this request.
        /// 当前请求全部 Hook 通知使用的精确预检回合标识。
        turn_id: String,
    },
    /// This execution scope must not dispatch active-conversation runtime Hooks.
    /// 当前执行范围不得派发活动会话运行时 Hook。
    Disabled,
}

/// Resolves runtime-event Hook routing from the execution request's exact scope and policy.
/// 根据执行请求的精确范围与策略解析运行时事件 Hook 路由。
///
/// # Parameters
/// 参数
/// `execution_scope` is the strong execution identity captured before provider launch.
/// `execution_scope` 是 Provider 启动前捕获的强类型执行身份。
/// `hooks_enabled` is the request-scoped runtime capability decision.
/// `hooks_enabled` 是请求级运行时能力决策。
/// `conversation_hook_turn_id` is the precheck-validated Hook turn captured by the request.
/// `conversation_hook_turn_id` 是请求捕获且经过预检验证的 Hook 回合。
///
/// # Returns
/// 返回值
/// Returns immutable turn routing for Hook-enabled main-session executions, or an explicit missing-correlation error.
/// 对启用 Hook 的主会话执行返回不可变回合路由，缺失关联时返回明确错误。
pub(super) fn runtime_event_hook_policy_for_execution_scope(
    execution_scope: &ExecutionScope,
    hooks_enabled: bool,
    conversation_hook_turn_id: Option<&str>,
) -> Result<RuntimeEventHookPolicy, String> {
    if hooks_enabled && matches!(execution_scope, ExecutionScope::MainSession { .. }) {
        // turn_id is the immutable precheck correlation required by every root-stream tool Hook.
        // turn_id 是每个根流工具 Hook 必须使用的不可变预检关联。
        let turn_id = conversation_hook_turn_id
            .map(str::trim)
            .filter(|turn_id| !turn_id.is_empty())
            .ok_or_else(|| {
                String::from(
                    "Hook-enabled main-session execution request is missing its conversation Hook turn id",
                )
            })?;
        Ok(RuntimeEventHookPolicy::ConversationTurn {
            turn_id: turn_id.to_string(),
        })
    } else {
        Ok(RuntimeEventHookPolicy::Disabled)
    }
}

/// Describes the session phase transition caused by one runtime event kind.
/// 描述单个运行时事件类型触发的会话阶段转换。
struct RuntimeEventPhaseTransition {
    /// Target phase written to the retained workbench state.
    /// 写入保留工作台状态的目标阶段。
    phase: SessionRunPhase,
    /// Stable diagnostic context used when the phase update fails.
    /// 阶段更新失败时使用的稳定诊断上下文。
    failure_context: &'static str,
}

/// Handles one runtime event after the caller has validated the stream kernel generation.
/// 在调用方完成流内核代际校验后处理单个运行时事件。
///
/// # Parameters
/// # 参数
/// `app` emits workbench deltas produced by runtime projection.
/// `app` 发送运行时投影生成的工作台增量。
/// `task_state` owns retained workspace, wait-agent, subagent, and active-session state.
/// `task_state` 持有保留工作区、wait-agent、子代理和活动会话状态。
/// `controller_runtime` persists state required by detached listeners and pending prompts.
/// `controller_runtime` 持久化分离监听器与排队提示词所需的状态。
/// `agent_runtime` owns conversation hooks and child-task runtime subscriptions.
/// `agent_runtime` 持有会话 Hook 与子任务运行时订阅。
/// `request_trace_state` records the first event observed after a provider request starts.
/// `request_trace_state` 记录 Provider 请求启动后观察到的首个事件。
/// `hook_policy` is the immutable Hook route derived from the execution request.
/// `hook_policy` 是从执行请求派生出的不可变 Hook 路由。
/// `session_id` identifies the session that owns the runtime event.
/// `session_id` 标识拥有该运行时事件的会话。
/// `active_stream_task_id` proves the exact root stream allowed to commit phase and parent projection state.
/// `active_stream_task_id` 证明获准提交阶段与父会话投影状态的精确根流。
/// `event` is the authoritative runtime event emitted by the Agent runtime.
/// `event` 是 Agent 运行时发送的权威运行时事件。
///
/// # Returns
/// # 返回值
/// This function returns no value; recoverable projection failures are logged and stream polling continues.
/// 此函数无返回值；可恢复的投影失败会被记录，流轮询继续执行。
pub(super) async fn handle_runtime_stream_event(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    request_trace_state: &mut StreamRequestTraceState,
    hook_policy: &RuntimeEventHookPolicy,
    session_id: SessionId,
    active_stream_task_id: Uuid,
    event: RuntimeEvent,
) {
    request_trace_state.emit_first_runtime_event(session_id, &format!("runtime.{:?}", event.kind));
    if matches!(
        event.kind,
        RuntimeEventKind::ToolStarted
            | RuntimeEventKind::ToolFinished
            | RuntimeEventKind::DelegateStarted
            | RuntimeEventKind::DelegateFinished
    ) {
        webui_trace(
            "stream_runtime_event",
            format!(
                "session={session_id} kind={:?} {}",
                event.kind,
                runtime_event_trace_detail(&event)
            ),
        );
    }
    // Wait-agent state must observe the tool boundary before phase and projection updates run.
    // wait-agent 状态必须先观察工具边界，随后才能执行阶段与投影更新。
    match update_webui_wait_agent_runtime_state(
        task_state,
        session_id,
        active_stream_task_id,
        &event,
    ) {
        Ok(WaitAgentRuntimeStateCommit::Committed) => webui_trace(
            "stream_runtime_event_wait_agent_commit",
            format!("session={session_id} task={active_stream_task_id} outcome=committed"),
        ),
        Ok(WaitAgentRuntimeStateCommit::Superseded) => webui_trace(
            "stream_runtime_event_wait_agent_commit",
            format!("session={session_id} task={active_stream_task_id} outcome=superseded"),
        ),
        Ok(WaitAgentRuntimeStateCommit::Ignored) => {}
        Err(error) => {
            eprintln!("[webui-service] update wait-agent runtime state failed: {error}");
        }
    }
    // The optional transition makes tool-boundary phase changes explicit while leaving all other events unchanged.
    // 可选转换显式表达工具边界阶段变化，同时让其他事件保持不变。
    if let Some(transition) = runtime_event_phase_transition(event.kind) {
        match apply_active_stream_session_phase(
            app,
            task_state,
            session_id,
            active_stream_task_id,
            transition.phase,
        ) {
            Ok(outcome) => webui_trace(
                "stream_runtime_event_phase_commit",
                format!(
                    "session={session_id} task={active_stream_task_id} kind={:?} outcome={outcome:?}",
                    event.kind
                ),
            ),
            Err(error) => {
                eprintln!("[webui-service] {}: {error}", transition.failure_context);
            }
        }
    }

    if let RuntimeEventHookPolicy::ConversationTurn { turn_id } = hook_policy
        && let Err(error) = dispatch_runtime_event_hook(agent_runtime, session_id, &event, turn_id)
    {
        eprintln!("[webui-service] runtime event Hook dispatch failed: {error}");
    }

    // DelegateStarted must register its detached listener before delegate projection is consumed.
    // DelegateStarted 必须先注册分离监听器，随后才能消费委派投影。
    if let Err(error) = maybe_start_webui_detached_subagent_task_listener(
        app.clone(),
        task_state.clone(),
        controller_runtime.clone(),
        agent_runtime.clone(),
        session_id,
        &event,
    ) {
        eprintln!("[webui-service] start detached subagent listener failed: {error}");
    }

    // A consumed delegate event owns its child projection and must not be duplicated in the parent transcript.
    // 已消费的委派事件拥有其子会话投影，不能在父会话转录中重复写入。
    let subagent_event_consumed = match ingest_webui_subagent_runtime_event(
        app,
        task_state,
        agent_runtime,
        session_id,
        &event,
    ) {
        Ok(consumed) => consumed,
        Err(error) => {
            eprintln!("[webui-service] ingest subagent event failed: {error}");
            false
        }
    };
    if !subagent_event_consumed
        && let Err(error) = append_active_stream_runtime_event_projection(
            app,
            task_state,
            session_id,
            active_stream_task_id,
            &event,
        )
    {
        eprintln!("[webui-service] append runtime event failed: {error}");
    }

    // Provider tool boundaries are durably acknowledged by ProviderV1StreamSink before this path.
    // Provider 工具边界会在进入该路径前由 ProviderV1StreamSink 可靠确认。
    if let Err(error) = release_runtime_event_projection_barrier_after_attempt(&event) {
        eprintln!("[webui-service] release runtime event projection barrier failed: {error}");
    }
}

/// Maps a runtime event kind to its workbench session phase transition.
/// 将运行时事件类型映射到对应的工作台会话阶段转换。
///
/// # Parameters
/// # 参数
/// `event_kind` is the runtime event discriminator emitted by the Agent runtime.
/// `event_kind` 是 Agent 运行时发送的运行时事件判别值。
///
/// # Returns
/// # 返回值
/// The required phase transition for tool boundaries, or `None` for all other events.
/// 工具边界返回所需阶段转换，其他事件返回 `None`。
fn runtime_event_phase_transition(
    event_kind: RuntimeEventKind,
) -> Option<RuntimeEventPhaseTransition> {
    match event_kind {
        RuntimeEventKind::ToolRequestStarted
        | RuntimeEventKind::ToolRequestProgress
        | RuntimeEventKind::ToolRequestPreviewUpdated
        | RuntimeEventKind::ToolRequestReady => Some(RuntimeEventPhaseTransition {
            phase: SessionRunPhase::ToolCalling,
            failure_context: "apply tool-request generation phase failed",
        }),
        RuntimeEventKind::ToolRequestInvalid | RuntimeEventKind::ToolRequestAborted => {
            Some(RuntimeEventPhaseTransition {
                phase: SessionRunPhase::Requesting,
                failure_context: "restore request phase after tool-request terminal event failed",
            })
        }
        RuntimeEventKind::ToolStarted => Some(RuntimeEventPhaseTransition {
            phase: SessionRunPhase::ToolRunning,
            failure_context: "apply tool phase failed",
        }),
        RuntimeEventKind::ToolFinished => Some(RuntimeEventPhaseTransition {
            phase: SessionRunPhase::Requesting,
            failure_context: "restore request phase after tool failed",
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_session_core::SessionNodeId;

    /// Verifies that tool boundaries map to their exact workbench phases.
    /// 验证工具边界会映射到精确的工作台阶段。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn tool_boundaries_define_session_phase_transitions() {
        // Request generation must become visible before the executor receives complete arguments.
        // 请求生成必须在执行器收到完整参数之前变为可见。
        let request_started = runtime_event_phase_transition(RuntimeEventKind::ToolRequestStarted)
            .expect("tool-request-started transition should exist");
        // An aborted request must return the session to provider requesting.
        // 已中止的请求必须让会话恢复到 Provider 请求阶段。
        let request_aborted = runtime_event_phase_transition(RuntimeEventKind::ToolRequestAborted)
            .expect("tool-request-aborted transition should exist");
        // The started transition must expose the tool-running phase.
        // 启动转换必须暴露工具运行阶段。
        let started = runtime_event_phase_transition(RuntimeEventKind::ToolStarted)
            .expect("tool-started transition should exist");
        // The finished transition must restore the provider-requesting phase.
        // 完成转换必须恢复 Provider 请求阶段。
        let finished = runtime_event_phase_transition(RuntimeEventKind::ToolFinished)
            .expect("tool-finished transition should exist");

        assert_eq!(request_started.phase, SessionRunPhase::ToolCalling);
        assert_eq!(request_aborted.phase, SessionRunPhase::Requesting);
        assert_eq!(started.phase, SessionRunPhase::ToolRunning);
        assert_eq!(finished.phase, SessionRunPhase::Requesting);
    }

    /// Verifies that delegate events do not mutate the root session phase.
    /// 验证委派事件不会修改根会话阶段。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn delegate_events_do_not_define_session_phase_transitions() {
        assert!(runtime_event_phase_transition(RuntimeEventKind::DelegateStarted).is_none());
        assert!(runtime_event_phase_transition(RuntimeEventKind::DelegateFinished).is_none());
    }

    /// Verifies runtime-event Hook routing follows execution scope and request policy exactly.
    /// 验证运行时事件 Hook 路由严格遵循执行范围与请求策略。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn runtime_event_hook_policy_uses_request_scope_and_immutable_turn_correlation() {
        // main_scope represents the only execution scope that owns an active root conversation Hook.
        // main_scope 表示唯一拥有活动根会话 Hook 的执行范围。
        let main_scope = ExecutionScope::MainSession {
            session_id: SessionNodeId::from(Uuid::new_v4()),
        };
        // task_scope represents a child execution whose Hook correlation is handled by explicit execution id.
        // task_scope 表示由显式 execution id 处理 Hook 关联的子执行。
        let task_scope = ExecutionScope::TaskSession {
            root_session_id: SessionNodeId::from(Uuid::new_v4()),
            task_session_id: SessionNodeId::from(Uuid::new_v4()),
        };
        // compaction_scope represents internal compaction work with its own committed notification path.
        // compaction_scope 表示拥有独立提交通知链的内部压缩执行。
        let compaction_scope = ExecutionScope::ContextCompaction {
            node_id: SessionNodeId::from(Uuid::new_v4()),
        };

        assert_eq!(
            runtime_event_hook_policy_for_execution_scope(&main_scope, true, Some("turn-17"))
                .expect("Hook-enabled main session should accept exact turn correlation"),
            RuntimeEventHookPolicy::ConversationTurn {
                turn_id: String::from("turn-17")
            }
        );
        assert_eq!(
            runtime_event_hook_policy_for_execution_scope(&main_scope, false, None)
                .expect("Hook-disabled main session should not require correlation"),
            RuntimeEventHookPolicy::Disabled
        );
        assert_eq!(
            runtime_event_hook_policy_for_execution_scope(&task_scope, true, None)
                .expect("task-session stream Hooks use their detached execution path"),
            RuntimeEventHookPolicy::Disabled
        );
        assert_eq!(
            runtime_event_hook_policy_for_execution_scope(&compaction_scope, true, None)
                .expect("compaction stream Hooks use their committed notification path"),
            RuntimeEventHookPolicy::Disabled
        );
        // error proves an enabled main-session Hook cannot fall back to mutable active-turn lookup.
        // error 证明启用的主会话 Hook 无法回退到可变 active-turn 查询。
        let error = runtime_event_hook_policy_for_execution_scope(&main_scope, true, None)
            .expect_err("Hook-enabled main session must reject missing turn correlation");
        assert!(error.contains("missing its conversation Hook turn id"));
    }
}
