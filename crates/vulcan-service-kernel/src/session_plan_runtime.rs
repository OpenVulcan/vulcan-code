//! session plan runtime helpers.
//! session plan runtime 辅助逻辑。

use super::*;

/// 会话当前处于计划模式时返回计划模式开发者提示段。
pub(crate) fn session_plan_context_for_prompt_from_state(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<String>, String> {
    let updated_at = current_unix_millis_text();
    let mut context: Option<String> = None;
    let mut next_plan_mode: Option<SessionPlanModeState> = None;
    let mut next_execution_mode = String::from("build");
    let workspace_snapshot =
        with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
            let Some(runtime_state) = workspace.session_runtime_state.get_mut(&session_id) else {
                return;
            };
            let is_plan = runtime_state
                .active_work_mode
                .as_deref()
                .is_some_and(|mode| mode.eq_ignore_ascii_case("plan"));
            if !is_plan {
                if runtime_state.plan_mode.pending_execution_notice {
                    runtime_state.plan_mode.pending_execution_notice = false;
                    runtime_state.updated_at = updated_at.clone();
                    next_plan_mode = Some(runtime_state.plan_mode.clone());
                    next_execution_mode = String::from("build");
                    context = Some(render_plan_mode_exit_developer_context());
                }
                return;
            }
            next_execution_mode = String::from("plan");
            context = Some(render_plan_mode_developer_context());
            if runtime_state.plan_mode.pending_execution_notice {
                runtime_state.plan_mode.pending_execution_notice = false;
                runtime_state.updated_at = updated_at.clone();
                next_plan_mode = Some(runtime_state.plan_mode.clone());
            }
            if runtime_state.plan_mode.prompt_injected {
                return;
            }
            runtime_state.plan_mode.prompt_injected = true;
            runtime_state.updated_at = updated_at.clone();
            next_plan_mode = Some(runtime_state.plan_mode.clone());
            next_execution_mode = String::from("plan");
        })?;
    let Some(plan_mode) = next_plan_mode else {
        return Ok(context);
    };
    let mut addon_state = SessionAddonState::new(session_id);
    let _ = addon_state.set_work_mode(
        &SessionWorkMode {
            execution_mode: next_execution_mode,
            plan_mode,
        },
        updated_at,
    );
    spawn_workspace_session_addon_state_persist_task(
        app.clone(),
        state.clone(),
        controller_runtime.clone(),
        workspace_snapshot,
        session_id,
        addon_state,
        vec![SessionAddonNamespace::WorkMode],
        Some(String::from("plan_mode_prompt_injected")),
    );
    Ok(context)
}

/// Renders the Plan-mode developer guidance attached to each active Plan request.
/// 渲染附加到每次活跃计划模式请求的开发者引导。
///
/// # Returns
/// 返回值
/// English model-facing Plan-mode guidance adapted for Vulcan.
/// 面向模型的英文 Vulcan 计划模式引导。
pub(crate) fn render_plan_mode_developer_context() -> String {
    String::from(
        r#"# Plan Mode (Conversational)

You are in Plan Mode for this request. Work in three phases and chat your way to a decision-complete plan before finalizing it. A strong plan is detailed enough that another engineer or agent can implement it immediately without making additional product or architecture decisions.

## Mode Rules (Strict)

Plan Mode remains active for the current request until the host explicitly sends a non-plan request. User intent, imperative language, or phrases such as "start", "fix", "implement", or "do it" do not end Plan Mode.

If the user asks for execution while this request is still in Plan Mode, treat it as a request to plan the execution, not to perform it.

## Plan Mode vs. Progress Tools

Plan Mode is a collaboration mode for producing a decision-complete plan. It is not a checklist or progress state.

The `todolist` tool, when present, is a progress/todolist tool. It does not enter or exit Plan Mode. Do not use `todolist` while in Plan Mode. If you try to use it in Plan Mode, the host may reject the call.

## Plan Artifact Tool

When the `submit_plan` tool is available, use it as the official way to finalize a Plan Mode request. The tool saves the complete plan as a Markdown artifact and returns the final path.

Call `submit_plan` only when the plan is decision complete. Provide a short stable name and the full Markdown plan content. If you are revising a previously submitted plan, pass the original returned path, set `replace` to true, and provide the full replacement content. Do not submit partial patches or append-only updates.

After `submit_plan` succeeds, briefly tell the user that the plan was saved. Do not repeat the entire plan in assistant text unless the tool is unavailable.

## Execution vs. Mutation in Plan Mode

You may explore and execute non-mutating actions that improve the plan. You must not perform mutating actions.

Allowed non-mutating actions include:

* Reading or searching files, configs, schemas, types, manifests, and docs.
* Static analysis, inspection, and repository exploration.
* One-shot or interactive command-line exploration through tools such as `shell_command` or `terminal_session`, when the command is used only to inspect, test, or validate the plan. Before using `terminal_session`, call `skill("terminal-session")` and follow that skill.
* Dry-run style commands when they do not edit repo-tracked files.
* Tests, builds, or checks that may write to caches or build artifacts, as long as they do not edit repo-tracked files.

Not allowed mutating actions include:

* Editing, creating, deleting, moving, or writing files.
* Applying patches.
* Running formatters or linters that rewrite files.
* Running migrations, code generation, or scripts that update repo-tracked files.
* Staging, unstaging, reverting, committing, pushing, or otherwise changing Git state.
* Calling tools whose purpose is to perform the implementation instead of refining the plan.

When in doubt, if an action would reasonably be described as "doing the work" rather than "planning the work", do not do it.

The host may still expose the normal tool schema in Plan Mode to preserve request stability. Tool visibility does not mean mutation is allowed. If you call a mutating tool in Plan Mode, the host may reject the call and ask you to continue planning.

## Phase 1 - Ground in the Environment

Begin by grounding yourself in the actual environment. Resolve unknowns by discovering facts before asking the user. Perform at least one targeted non-mutating exploration pass when a local repository or relevant environment is available.

Use searches, file reads, manifests, configs, schemas, types, and existing implementation entry points to understand the current shape. Ask the user only after reasonable non-mutating exploration cannot answer a meaningful ambiguity.

Exception: you may ask before exploring only when the prompt itself contains an obvious contradiction or a preference question that cannot be discovered from the environment.

## Phase 2 - Intent Chat

Keep clarifying until you can state the goal, success criteria, audience, in-scope and out-of-scope boundaries, constraints, current state, and important preferences or tradeoffs.

Prefer questions over guessing when a high-impact ambiguity remains.

## Phase 3 - Implementation Chat

Once intent is stable, refine the implementation shape until the plan is decision complete. Cover the approach, interfaces, data flow, edge cases, failure modes, testing, acceptance criteria, compatibility constraints, and migration needs when relevant.

## Asking Questions

Ask only questions that materially change the plan, confirm an important assumption, or choose between meaningful tradeoffs.

Prefer using the available structured question tool when it is registered. In this environment, that tool may be named `ask`. If no structured question tool is available or the question cannot be expressed cleanly through it, ask directly in plain text.

Do not ask questions that can be answered by non-mutating exploration.

## Two Kinds of Unknowns

Treat discoverable facts and preferences differently.

Discoverable facts are repository or system truth. Explore first. Ask only if multiple plausible candidates remain, nothing relevant can be found, or the ambiguity is actually product intent.

Preferences and tradeoffs are not discoverable. Ask early when they materially affect the plan. If the user does not answer and a reasonable default exists, proceed with that default and record it as an assumption in the final plan.

## Finalization Rule

Only finalize the plan when it is decision complete and leaves no implementation decisions unresolved.

If `submit_plan` is available, finalize by calling `submit_plan` with the complete Markdown plan. Treat `<proposed_plan>` as a fallback only when the plan artifact tool is unavailable.

When presenting the official plan without `submit_plan`, wrap it in exactly one `<proposed_plan>` block:

1. Put the opening tag on its own line.
2. Start the plan content on the next line.
3. Put the closing tag on its own line.
4. Use Markdown inside the block.
5. Keep the tags exactly as `<proposed_plan>` and `</proposed_plan>`.

The final plan should be concise by default and include:

* A clear title.
* A brief summary.
* Key implementation changes or additions to public APIs, interfaces, or types.
* Test cases and acceptance scenarios.
* Explicit assumptions and defaults chosen where needed.

Prefer grouped implementation bullets by subsystem or behavior over file-by-file inventories. Mention files only when needed to disambiguate a non-obvious change. Avoid excessive detail unless it is needed to prevent implementation mistakes.

Do not ask "should I proceed?" in the final plan. The user can leave Plan Mode or request implementation after the plan artifact is submitted.

Only produce at most one `<proposed_plan>` block per request. If the user asks for revisions after a prior `<proposed_plan>`, the new `<proposed_plan>` must be a complete replacement."#,
    )
}

/// Renders the one-shot developer notice used after leaving Plan mode.
/// 渲染离开计划模式后使用的一次性开发者提示。
///
/// # Returns
/// 返回值
/// English model-facing guidance that normal execution has resumed.
/// 面向模型的英文提示，说明已恢复常规执行。
pub(crate) fn render_plan_mode_exit_developer_context() -> String {
    String::from(
        "Plan Mode has ended. You are now in execution mode. The previous Plan-mode mutation restrictions no longer apply; respond to the current user request using the normal execution workflow.",
    )
}

/// Updates the retained goal cache and emits a refreshed workbench snapshot.
/// 更新保留的目标缓存并发出刷新后的工作台快照。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns retained service state.
/// `state` 持有保留的服务状态。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `agent_runtime` provides projection configuration.
/// `agent_runtime` 提供投影配置。
/// `luaskills_catalog` is the retained LuaSkills projection cache.
/// `luaskills_catalog` 是保留的 LuaSkills 投影缓存。
/// `token_budget_runtime` is the retained token-budget projection runtime.
/// `token_budget_runtime` 是保留的 Token 预算投影运行时。
/// `mcp_catalog` is the retained MCP projection cache.
/// `mcp_catalog` 是保留的 MCP 投影缓存。
/// `logs` is the startup log slice included in snapshots.
/// `logs` 是快照包含的启动日志片段。
/// `session_id` selects the goal cache row.
/// `session_id` 选择目标缓存行。
/// `goal` is the optional latest goal.
/// `goal` 是可选的最新目标。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot.
/// 刷新后的工作台快照。
pub(crate) fn refresh_session_goal_summary_and_emit(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    luaskills_catalog: &TauriLuaSkillsCatalog,
    token_budget_runtime: &TauriTokenBudgetRuntime,
    mcp_catalog: &TauriMcpRuntimeCatalog,
    logs: &[String],
    session_id: SessionId,
    goal: Option<ThreadGoal>,
) -> Result<WorkbenchSnapshot, String> {
    let workspace_snapshot = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        match goal.as_ref().map(session_goal_summary_from_thread_goal) {
            Some(summary) => {
                guard.session_goal_summaries.insert(session_id, summary);
            }
            None => {
                guard.session_goal_summaries.remove(&session_id);
            }
        }
        guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?
            .clone()
    };
    store_and_emit_workbench_snapshot(
        &app,
        &state,
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        logs,
    )
}

/// Builds session todolist state from one completed `todolist` tool event.
/// 从单条已完成的 `todolist` 工具事件构建会话 todolist 状态。
///
/// # Parameters
/// 参数
/// `event` is the runtime event emitted after a tool call finishes.
/// `event` 是工具调用结束后发出的运行时事件。
/// `updated_at` is the timestamp assigned to the projected todolist.
/// `updated_at` 是分配给投影 todolist 的时间戳。
///
/// # Returns
/// 返回值
/// The parsed todolist when the event carries a matching host result.
/// 当事件携带匹配的宿主结果时返回已解析 todolist。
pub(crate) fn todolist_state_from_tool_finished_event(
    event: &RuntimeEvent,
    updated_at: &str,
) -> Option<SessionTodolistState> {
    if event.kind != RuntimeEventKind::ToolFinished {
        return None;
    }
    let payload = event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("host_result"))
        .and_then(|host_result| {
            (host_result.get("kind").and_then(Value::as_str) == Some("todolist"))
                .then(|| host_result.get("payload"))
                .flatten()
        })?;
    let items = payload
        .get("items")
        .and_then(Value::as_array)?
        .iter()
        .map(todolist_item_from_value)
        .collect::<Option<Vec<_>>>()?;
    let explanation = payload
        .get("explanation")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Some(SessionTodolistState {
        explanation,
        items,
        updated_at: Some(updated_at.to_string()),
    })
}

/// Parses one host-result JSON value into a session todolist item.
/// 将单个宿主结果 JSON 值解析为会话 todolist 条目。
///
/// # Parameters
/// 参数
/// `value` is one raw todolist item emitted by the native tool.
/// `value` 是原生工具发出的单个原始 todolist 条目。
///
/// # Returns
/// 返回值
/// The parsed todolist item when both step and status are valid.
/// 当步骤与状态均有效时返回已解析 todolist 条目。
pub(crate) fn todolist_item_from_value(value: &Value) -> Option<SessionTodolistItem> {
    let step = value
        .get("step")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let status = match value.get("status").and_then(Value::as_str)? {
        "pending" => SessionTodolistItemStatus::Pending,
        "in_progress" => SessionTodolistItemStatus::InProgress,
        "completed" => SessionTodolistItemStatus::Completed,
        _ => return None,
    };
    Some(SessionTodolistItem { step, status })
}

/// Builds a Plan artifact state from one completed submit_plan tool event.
/// 从单条已完成 submit_plan 工具事件构建计划产物状态。
///
/// # Parameters
/// 参数
/// `event` is the runtime event emitted after a tool call finishes.
/// `event` 是工具调用结束后发出的运行时事件。
///
/// # Returns
/// 返回值
/// The parsed plan artifact when the event carries a valid host payload.
/// 当事件携带有效宿主载荷时返回解析后的计划产物。
pub(crate) fn plan_artifact_state_from_tool_finished_event(
    event: &RuntimeEvent,
) -> Option<SessionPlanArtifactState> {
    if event.kind != RuntimeEventKind::ToolFinished {
        return None;
    }
    let payload = event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("host_result"))
        .and_then(|host_result| {
            (host_result.get("kind").and_then(Value::as_str) == Some("plan_artifact"))
                .then(|| host_result.get("payload"))
                .flatten()
        })?;
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let path = payload
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let updated_at = payload
        .get("updatedAt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let content = payload
        .get("content")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .filter(|value| !value.trim().is_empty());
    Some(SessionPlanArtifactState {
        name,
        path,
        content,
        updated_at,
    })
}

/// Acknowledges that one runtime event has been projected into the emitted WebUI snapshot.
/// 确认单条运行时事件已经投影进已发送的 WebUI 快照。
pub(crate) fn acknowledge_runtime_event_projection_barrier(event: &RuntimeEvent) {
    let acknowledged =
        vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::acknowledge_runtime_event_projection_barrier_from_event(event);
    if acknowledged {
        webui_trace(
            "runtime_event_projection_barrier_ack",
            runtime_event_trace_detail(event),
        );
    }
}
