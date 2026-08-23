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
    let session_directory = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        if let Some(runtime_state) = workspace.session_runtime_state.get_mut(&session_id) {
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
            } else {
                next_execution_mode = String::from("plan");
                context = Some(render_plan_mode_developer_context());
                if runtime_state.plan_mode.pending_execution_notice {
                    runtime_state.plan_mode.pending_execution_notice = false;
                    runtime_state.updated_at = updated_at.clone();
                    next_plan_mode = Some(runtime_state.plan_mode.clone());
                }
                if !runtime_state.plan_mode.prompt_injected {
                    runtime_state.plan_mode.prompt_injected = true;
                    runtime_state.updated_at = updated_at.clone();
                    next_plan_mode = Some(runtime_state.plan_mode.clone());
                    next_execution_mode = String::from("plan");
                }
            }
        }
        if next_plan_mode.is_some() {
            Some(require_segmented_session_directory_for_retained_session(
                &guard,
                &controller_runtime.data_root,
                session_id,
            )?)
        } else {
            None
        }
    };
    let Some(plan_mode) = next_plan_mode else {
        return Ok(context);
    };
    let session_directory = session_directory.ok_or_else(|| {
        format!("Session `{session_id}` plan-mode persistence target is missing.")
    })?;
    let mut addon_state = SessionAddonState::new(session_id);
    let _ = addon_state.set_work_mode(
        &SessionWorkMode {
            execution_mode: next_execution_mode,
            plan_mode,
        },
        updated_at,
    );
    spawn_exact_session_addon_state_persist_task(
        app.clone(),
        state.clone(),
        session_directory,
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

/// Updates the retained goal cache and emits a lightweight addon delta.
/// 更新保留的目标缓存并发出轻量附属状态增量。
///
/// # Parameters
/// 参数
/// `app` emits the addon delta when the target session is visible.
/// `app` 在目标会话可见时发出附属状态增量。
/// `state` owns retained service state.
/// `state` 持有保留的服务状态。
/// `session_id` selects the goal cache row.
/// `session_id` 选择目标缓存行。
/// `goal` is the optional latest goal.
/// `goal` 是可选的最新目标。
///
/// # Returns
/// 返回值
/// The applied addon projection.
/// 已应用的附属状态投影。
pub(crate) fn refresh_session_goal_summary_and_emit(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    goal: Option<ThreadGoal>,
) -> Result<SessionAddonsChangedResponse, String> {
    {
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
    }
    emit_session_addons_changed_delta(app, state, session_id)
}

/// Emits the selected-session addon projection through a lightweight workbench delta.
/// 通过轻量工作台增量发出当前会话附属状态投影。
///
/// # Parameters
/// 参数
/// `app` emits the delta event when the target session is visible.
/// `app` 在目标会话可见时发出增量事件。
/// `state` owns the retained workspace and workbench projections.
/// `state` 持有保留的工作区与工作台投影。
/// `session_id` selects the addon state owner.
/// `session_id` 选择附属状态所属会话。
///
/// # Returns
/// 返回值
/// The applied addon projection, including the latest workbench revision.
/// 返回已应用的附属状态投影及最新工作台版本。
pub(crate) fn emit_session_addons_changed_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<SessionAddonsChangedResponse, String> {
    let conversation_id = session_id.to_string();
    let (event, response) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let runtime_state = workspace_session_with_archive_status(workspace, session_id).and_then(
            |(_, archived)| {
                if archived {
                    workspace.archived_session_runtime_state.get(&session_id)
                } else {
                    workspace.session_runtime_state.get(&session_id)
                }
            },
        );
        let session_goal = guard.session_goal_summaries.get(&session_id).cloned();
        let session_todolist = session_todolist_summary_from_runtime_state(runtime_state);
        let session_plan_mode = session_plan_mode_summary_from_runtime_state(runtime_state);
        let provider_vmm_layer = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.vmm.clone())
            .unwrap_or_default();
        let session_record = workspace_session_with_archive_status(workspace, session_id)
            .map(|(session, _)| session);
        let project_layer = session_record
            .and_then(|session| cached_project_vmm_layer_for_session(workspace, session));
        let session_vmm_policy = Some(session_vmm_policy_summary_from_layers(
            &provider_vmm_layer,
            project_layer.as_ref(),
            runtime_state,
        ));
        let session_vmm_policy_for_list = runtime_state
            .filter(|runtime_state| !runtime_state.vmm.is_empty())
            .and(session_vmm_policy.clone());
        let global_output_style_id = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.prompt.output_style.id())
            .unwrap_or(default_output_style_id());
        let session_output_style =
            session_output_style_summary_from_runtime_state(runtime_state, global_output_style_id);
        let session_context_compaction_window_node_id = runtime_state
            .and_then(|runtime_state| runtime_state.context_compaction_window_node_id)
            .map(|node_id| node_id.as_str().to_string());
        let session_provider_usage =
            runtime_state.and_then(session_provider_usage_summary_from_runtime_state);
        let session_goal =
            session_goal_summary_with_provider_usage(session_goal, session_provider_usage.as_ref());
        let session_addons_list_entry = session_addons_changed_delta_for_retained_list(
            conversation_id.clone(),
            session_goal.clone(),
            session_todolist.clone(),
            session_plan_mode.clone(),
            session_vmm_policy_for_list.clone(),
            session_output_style.clone(),
            session_context_compaction_window_node_id.clone(),
            session_provider_usage.clone(),
        );
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        if workbench_surface_matches_conversation(&guard.workbench, &conversation_id) {
            guard.workbench.session_goal = session_goal.clone();
            guard.workbench.session_todolist = session_todolist.clone();
            guard.workbench.session_plan_mode = session_plan_mode.clone();
            guard.workbench.session_vmm_policy = session_vmm_policy.clone();
            guard.workbench.session_output_style = session_output_style.clone();
            guard.workbench.session_context_compaction_window_node_id =
                session_context_compaction_window_node_id.clone();
            guard.workbench.session_provider_usage = session_provider_usage.clone();
        }
        let pending_questions =
            pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref())?;
        replace_conversation_scoped_projection_entry(
            &mut guard.workbench.session_addons_by_conversation,
            conversation_id.as_str(),
            session_addons_list_entry,
            |entry| entry.conversation_id.as_str(),
        );
        let conversation_is_visible = guard
            .workbench
            .conversations
            .iter()
            .any(|candidate| candidate.id == conversation_id);
        replace_conversation_scoped_projection_entry(
            &mut guard.workbench.session_vmm_policies_by_conversation,
            conversation_id.as_str(),
            if conversation_is_visible {
                session_vmm_policy.clone().map(|session_vmm_policy| {
                    SessionVmmPolicyByConversation {
                        conversation_id: conversation_id.clone(),
                        session_vmm_policy,
                    }
                })
            } else {
                None
            },
            |entry| entry.conversation_id.as_str(),
        );
        if let Some(conversation) = guard
            .workbench
            .conversations
            .iter_mut()
            .find(|candidate| candidate.id == conversation_id)
        {
            refresh_conversation_attention_summary(
                conversation,
                session_id,
                &pending_questions,
                session_plan_mode.as_ref(),
            );
        }
        let delta = SessionAddonsChangedDelta {
            conversation_id: conversation_id.clone(),
            session_goal: session_goal.clone(),
            session_todolist: session_todolist.clone(),
            session_plan_mode: session_plan_mode.clone(),
            session_vmm_policy: session_vmm_policy.clone(),
            session_output_style: session_output_style.clone(),
            session_context_compaction_window_node_id: session_context_compaction_window_node_id
                .clone(),
            session_provider_usage: session_provider_usage.clone().map(Box::new),
        };
        (
            WorkbenchDeltaEnvelope {
                generation,
                delta: WorkbenchDelta::SessionAddonsChanged(delta),
            },
            SessionAddonsChangedResponse {
                conversation_id,
                session_goal,
                session_todolist,
                session_plan_mode,
                session_vmm_policy,
                session_output_style,
                session_context_compaction_window_node_id,
                session_provider_usage,
                revision: generation,
            },
        )
    };
    emit_service_event_with_trace(
        app,
        "workbench_session_addons_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(response)
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

/// Releases one runtime-event barrier after the host finishes its projection attempt.
/// 在宿主完成运行时事件投影尝试后释放对应屏障。
///
/// # Parameters
/// # 参数
/// `event` carries the stable barrier identity attached by the Agent runtime.
/// `event` 携带 Agent 运行时附加的稳定屏障标识。
///
/// # Returns
/// # 返回值
/// Returns whether an active barrier was acknowledged, or an explicit registry-lock error.
/// 返回是否确认了活跃屏障，或返回明确的注册表锁错误。
pub(crate) fn release_runtime_event_projection_barrier_after_attempt(
    event: &RuntimeEvent,
) -> Result<bool, String> {
    let acknowledged =
        vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::acknowledge_runtime_event_projection_barrier_from_event(event)?;
    if acknowledged {
        webui_trace(
            "runtime_event_projection_barrier_ack",
            runtime_event_trace_detail(event),
        );
    }
    Ok(acknowledged)
}
