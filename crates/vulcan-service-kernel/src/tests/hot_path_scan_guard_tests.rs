//! Static guards for interaction-path and session-storage discovery.
//! 交互路径与会话存储发现的静态护栏。

/// Verifies conversation hot modules cannot enumerate directories or invoke scan-based locators.
/// 验证会话热模块不能枚举目录或调用扫描型定位器。
#[test]
fn conversation_hot_paths_do_not_reintroduce_global_scans() {
    let sources = [
        (
            "conversation_submission.rs",
            include_str!("../conversation_submission.rs"),
        ),
        (
            "conversation_submission/edit.rs",
            include_str!("../conversation_submission/edit.rs"),
        ),
        (
            "conversation_actions.rs",
            include_str!("../conversation_actions.rs"),
        ),
        (
            "provider_usage_runtime.rs",
            include_str!("../provider_usage_runtime.rs"),
        ),
        (
            "public_ai_settings.rs",
            include_str!("../public_ai_settings.rs"),
        ),
        (
            "agent_stream/provider_telemetry.rs",
            include_str!("../agent_stream/provider_telemetry.rs"),
        ),
        (
            "agent_stream/stream_task/conversation_checkpoint.rs",
            include_str!("../agent_stream/stream_task/conversation_checkpoint.rs"),
        ),
        (
            "agent_stream/stream_task.rs",
            include_str!("../agent_stream/stream_task.rs"),
        ),
        (
            "agent_stream/stream_task/finished.rs",
            include_str!("../agent_stream/stream_task/finished.rs"),
        ),
        (
            "agent_stream/stream_task/provider_route_switch.rs",
            include_str!("../agent_stream/stream_task/provider_route_switch.rs"),
        ),
        (
            "execution_report_memory.rs",
            include_str!("../execution_report_memory.rs"),
        ),
        (
            "workspace_session_persistence.rs",
            include_str!("../workspace_session_persistence.rs"),
        ),
        (
            "webui_prompt_lifecycle/precheck_continue.rs",
            include_str!("../webui_prompt_lifecycle/precheck_continue.rs"),
        ),
        (
            "agent_runtime/execution_preparation.rs",
            include_str!(
                "../../../vulcan-agent-runtime/src/agent_runtime/execution_preparation.rs"
            ),
        ),
        (
            "workspace_store/persistence/runtime_tail.rs",
            include_str!(
                "../../../vulcan-workspace-store/src/workspace_store/persistence/runtime_tail.rs"
            ),
        ),
        (
            "workspace_store/execution_retry_store.rs",
            include_str!(
                "../../../vulcan-workspace-store/src/workspace_store/execution_retry_store.rs"
            ),
        ),
        (
            "workspace_store/segmented_session_store/message_events.rs",
            include_str!(
                "../../../vulcan-workspace-store/src/workspace_store/segmented_session_store/message_events.rs"
            ),
        ),
        (
            "task_session/segmented_store/store_impl.rs",
            include_str!("../../../vulcan-task-session/src/segmented_store/store_impl.rs"),
        ),
        (
            "task_session/segmented_store/mirror.rs",
            include_str!("../../../vulcan-task-session/src/segmented_store/mirror.rs"),
        ),
        (
            "task_session/segmented_store/invocations.rs",
            include_str!("../../../vulcan-task-session/src/segmented_store/invocations.rs"),
        ),
        (
            "service_bootstrap.rs",
            include_str!("../service_bootstrap.rs"),
        ),
        (
            "provider_manifest/compiler.rs",
            include_str!("../../../vulcan-provider-adapters/src/provider_manifest/compiler.rs"),
        ),
        (
            "host_tools/project_temp_maintenance.rs",
            include_str!("../../../vulcan-host-tools/src/project_temp_maintenance.rs"),
        ),
    ];
    let forbidden = [
        "read_dir(",
        "WalkDir",
        "WalkBuilder",
        "find_segmented_session_directory_by_id",
        "find_segmented_session_or_task_directory_by_id",
        "maintain_project_temp_dir(",
        "maintain_project_temp_dirs(",
    ];
    let violations = sources
        .iter()
        .flat_map(|(name, source)| {
            forbidden
                .iter()
                .filter(|needle| source.contains(**needle))
                .map(|needle| format!("{name}: {needle}"))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert!(
        violations.is_empty(),
        "conversation hot paths reintroduced global discovery: {violations:?}"
    );
}

/// Verifies retry persistence never creates a second sibling snapshot or content-addressed body.
/// 验证重试持久化绝不创建第二个同级快照或内容寻址正文。
#[test]
fn retry_storage_uses_only_the_exact_session_root_file() {
    // source is the complete retry-store implementation guarded by this assertion.
    // source 是由当前断言保护的完整重试存储实现。
    let source = include_str!(
        "../../../vulcan-workspace-store/src/workspace_store/execution_retry_store.rs"
    );
    // forbidden contains every publication primitive that can leave a sibling retry body.
    // forbidden 包含所有可能遗留同级重试正文的发布原语。
    let forbidden = [
        "write_file_bytes_atomic(",
        "create_unique_staging_file(",
        "execution_retry/",
        "snapshot.previous",
    ];
    // violations records only exact textual regressions in the retry-store source.
    // violations 只记录重试存储源码中的精确文本回归。
    let violations = forbidden
        .iter()
        .filter(|needle| source.contains(**needle))
        .copied()
        .collect::<Vec<_>>();
    assert!(
        violations.is_empty(),
        "retry persistence reintroduced sibling snapshots: {violations:?}"
    );
}

/// Verifies live Provider and tool checkpoints remain memory-only until a terminal interruption.
/// 验证实时 Provider 与工具检查点在终态中断前始终仅保留于内存。
#[test]
fn live_retry_checkpoints_never_write_interrupted_retry_json() {
    // source owns both normal Provider checkpoint publication and incremental tool transitions.
    // source 同时持有普通 Provider 检查点发布与增量工具迁移。
    let source = include_str!("../agent_stream/stream_task/conversation_checkpoint.rs");
    // providerCheckpoint isolates the normal pre-dispatch checkpoint body from terminal handlers.
    // providerCheckpoint 将普通分派前检查点正文与终态处理器隔离。
    let provider_checkpoint = source
        .split_once("pub(super) fn retain_prepared_execution_retry_checkpoint(")
        .and_then(|(_, tail)| tail.split_once("/// Clears the live retry reference"))
        .map(|(body, _)| body)
        .expect("normal Provider retry checkpoint source should remain identifiable");
    assert!(
        provider_checkpoint.contains("project_execution_retry_snapshot("),
        "normal Provider checkpoints must publish an in-memory reference"
    );
    assert!(
        !provider_checkpoint.contains("persist_interrupted_execution_retry_snapshot("),
        "normal Provider checkpoints must not write retry.json"
    );
    // toolCheckpoint isolates BatchPrepared, Started, and Finished transitions from interruption persistence.
    // toolCheckpoint 将 BatchPrepared、Started 与 Finished 迁移同中断持久化隔离。
    let tool_checkpoint = source
        .split_once("pub(crate) async fn persist_execution_retry_tool_recovery_boundary(")
        .and_then(|(_, tail)| tail.split_once("/// Builds structured interruption evidence"))
        .map(|(body, _)| body)
        .expect("live tool recovery checkpoint source should remain identifiable");
    assert!(
        tool_checkpoint.contains("project_execution_retry_snapshot("),
        "live tool checkpoints must publish an in-memory reference"
    );
    assert!(
        !tool_checkpoint.contains("persist_interrupted_execution_retry_snapshot("),
        "live tool checkpoints must not write retry.json"
    );
}

/// Verifies state persistence excludes canonical conversation history before serialization starts.
/// 验证状态持久化在序列化开始前就排除规范会话历史。
#[test]
fn runtime_state_persistence_never_serializes_then_removes_conversation_history() {
    // source owns the lightweight state-document projection used by every runtime checkpoint.
    // source 持有每个运行态检查点使用的轻量状态文档投影。
    let source = include_str!(
        "../../../vulcan-workspace-store/src/workspace_store/segmented_session_store/state_documents.rs"
    );
    assert!(
        !source.contains("serialize_session_runtime_state_entry("),
        "state persistence must not serialize the complete runtime entry"
    );
    assert!(
        !source.contains("remove(\"conversation_state\")"),
        "state persistence must not remove conversation history after JSON allocation"
    );
    assert!(
        !source.contains("runtime_state.conversation_state"),
        "state metadata persistence must not traverse canonical conversation history"
    );
    assert!(
        source.contains("conversation_state: _"),
        "state persistence must classify conversation history explicitly in exhaustive destructuring"
    );
}

/// Verifies Provider boundaries use telemetry ownership and history-free state projections.
/// 验证 Provider 边界使用遥测归属与无历史状态投影。
#[test]
fn provider_boundaries_use_owner_identity_and_history_free_projection() {
    // source owns Provider boundary accounting and persistence routing.
    // source 持有 Provider 边界记账与持久化路由。
    let source = include_str!("../provider_usage_runtime.rs");
    assert!(
        source.contains("provider_owner_is_regular_session(event)?"),
        "Provider accounting must resolve ownership from the telemetry contract"
    );
    assert!(
        !source.contains(".chain(workspace.archived_sessions.iter())"),
        "Provider accounting must not scan active and archived session summaries"
    );
    assert!(
        !source.contains("runtime_state: runtime_state.clone()"),
        "Provider persistence must not clone complete runtime entries"
    );
    assert!(
        source.contains("project_single_session_runtime_state_update("),
        "main-session Provider persistence must use the history-free projection"
    );
}

/// Verifies a session model selection projects only its newly appended audit row.
/// 验证会话模型选择仅投影刚追加的审计记录。
#[test]
fn session_model_selection_never_clones_or_rebuilds_conversation_history() {
    // source owns the session-scoped model picker transaction and its visible audit record.
    // source 持有会话范围模型选择事务及其可见审计记录。
    let source = include_str!("../public_ai_settings.rs");
    // start and end isolate the exact hot-path function from unrelated settings operations.
    // start 与 end 将精确热路径函数同无关设置操作隔离。
    let start = source
        .find("fn append_session_model_switch_record(")
        .expect("model-switch append function must exist");
    let end = source[start..]
        .find("fn plan_session_model_selection_for_next_provider_request(")
        .map(|offset| start + offset)
        .expect("model-switch planning function must follow the append function");
    // functionSource is the complete model-switch append implementation.
    // functionSource 是完整的模型切换追加实现。
    let function_source = &source[start..end];
    assert!(
        !function_source.contains("runtime_state.clone()"),
        "session model selection must not clone complete runtime state"
    );
    assert!(
        !function_source.contains("merge_runtime_state_records_into_display_messages"),
        "session model selection must not rebuild complete display history"
    );
    assert!(
        function_source.contains("append_new_canonical_row_to_loaded_display_window"),
        "session model selection must project only the newly canonicalized row"
    );
}

/// Verifies runtime tool-event drafts transfer the canonical session row instead of cloning history.
/// 验证运行时工具事件草稿会转移规范会话行，而不是复制历史。
#[test]
fn runtime_tool_event_projection_never_clones_complete_session_history() {
    // source owns the Service Kernel transaction used by ready, started, and finished tool events.
    // source 持有就绪、开始与完成工具事件使用的 Service Kernel 事务。
    let source = include_str!("../agent_stream/projection.rs");
    // start and end isolate the exact event-draft builder from unrelated compatibility helpers.
    // start 与 end 将精确事件草稿构建器同无关兼容辅助逻辑隔离。
    let start = source
        .find("fn build_runtime_event_workspace_draft(")
        .expect("runtime-event draft builder must exist");
    let end = source[start..]
        .find("fn finish_runtime_event_projection_attempt(")
        .map(|offset| start + offset)
        .expect("runtime-event projection attempt helper must follow the draft builder");
    // functionSource is the complete hot-path draft construction implementation.
    // functionSource 是完整的热路径草稿构建实现。
    let function_source = &source[start..end];
    assert!(
        !function_source.contains(".cloned()"),
        "runtime tool events must not clone the complete session runtime row"
    );
    assert!(
        function_source.contains(".remove(&session_id)"),
        "runtime tool events must transfer exact session-row ownership into the draft"
    );
    assert!(
        function_source.contains("restored_runtime_state"),
        "failed runtime tool projections must restore the exact transferred row"
    );
    // reducerSource owns the shared Agent Runtime entry point used outside Service Kernel streaming.
    // reducerSource 持有 Service Kernel 流之外使用的共享 Agent Runtime 入口。
    let reducer_source =
        include_str!("../../../vulcan-agent-runtime/src/session_event_runtime/event_projection.rs");
    assert!(
        !reducer_source.contains("runtime_state.clone()"),
        "shared runtime-event reducers must not clone complete session history"
    );
    assert!(
        reducer_source.contains("std::mem::take(runtime_state)"),
        "shared runtime-event reducers must transfer exact runtime-row ownership"
    );
}

/// Verifies long-lived terminal sessions cannot disable recovery of an already terminated AI turn.
/// 验证长生命周期终端会话不能禁用已经终止的 AI 回合恢复。
#[test]
fn long_lived_terminal_sessions_are_not_conversation_continuation_blockers() {
    // source owns the complete live-blocker contract used by display and atomic claim paths.
    // source 持有展示与原子领取路径共用的完整实时阻断契约。
    let source = include_str!("../conversation_continuation.rs");
    assert!(
        !source.contains("has_running_command_session"),
        "long-lived command sessions must remain independent from AI-turn continuation"
    );
    assert!(
        source.contains("command_session_id"),
        "the independence invariant must remain documented beside the blocker collector"
    );
}

/// Verifies retained terminal persistence avoids complete workspace and runtime-history clones.
/// 验证保留终态持久化避免完整工作区与运行时历史克隆。
#[test]
fn retained_terminal_persistence_uses_exact_history_free_payloads() {
    // persistenceSource owns the retained-session persistence fast path before the legacy snapshot writer.
    // persistenceSource 持有旧快照写入器之前的保留会话持久化快速路径。
    let persistence_source = include_str!("../workspace_session_persistence.rs");
    // retainedEnd separates the optimized retained path from the general lifecycle persistence path.
    // retainedEnd 将优化后的保留路径与通用生命周期持久化路径分开。
    let retained_end = persistence_source
        .find("pub(crate) async fn persist_workspace_session_update_unlocked(")
        .expect("general persistence function must follow retained persistence");
    // retainedSource contains only the retained-session persistence implementation under review.
    // retainedSource 仅包含本次审核的保留会话持久化实现。
    let retained_source = &persistence_source[..retained_end];
    assert!(
        !retained_source.contains("workspace_snapshot.clone()"),
        "retained persistence must not clone the complete workspace"
    );
    assert!(
        !retained_source.contains("runtime_state.clone()"),
        "retained persistence must not clone canonical runtime history"
    );
    assert!(
        retained_source.contains("project_single_session_runtime_state_update("),
        "retained persistence must project history-free state metadata"
    );

    // routeSwitchSource owns next-request model promotion and durable rollback.
    // routeSwitchSource 持有下一请求模型提升与可靠回滚。
    let route_switch_source = include_str!("../agent_stream/stream_task/provider_route_switch.rs");
    assert!(
        !route_switch_source.contains("workspace_snapshot.clone()"),
        "model-route promotion must not clone the complete workspace"
    );
    assert!(
        !route_switch_source.contains("persist_workspace_session_update_unlocked"),
        "model-route promotion must persist only the exact state document"
    );

    // terminalReportSource owns the single atomic draft clone for final report application.
    // terminalReportSource 持有应用最终报告时唯一的原子草稿克隆。
    let terminal_report_source =
        include_str!("../../../vulcan-agent-runtime/src/session_event_runtime/terminal_report.rs");
    assert_eq!(
        terminal_report_source
            .matches("runtime_state.clone()")
            .count(),
        1,
        "terminal report application must retain exactly one atomic runtime clone"
    );

    // streamSources cover early and ordinary terminal materialization callers.
    // streamSources 覆盖提前终态与普通终态物化调用方。
    let stream_sources = [
        include_str!("../agent_stream/stream_task.rs"),
        include_str!("../agent_stream/stream_task/finished.rs"),
        include_str!("../execution_report_memory.rs"),
    ];
    assert!(
        stream_sources
            .iter()
            .all(|source| !source.contains("current_workspace_snapshot_from_shared_state")),
        "terminal materialization must not rebuild a complete workspace snapshot"
    );
}

/// Verifies exact session writers never wait behind the process-wide workspace lifecycle gate.
/// 验证精确会话写入者绝不会等待进程级工作区生命周期闸门。
#[test]
fn session_hot_persistence_uses_session_keyed_channels() {
    // sources contain only session-scoped state, transcript, addon, Provider, compaction, and route writes.
    // sources 仅包含会话级状态、转录、附属、Provider、压缩与路由写入。
    let sources = [
        (
            "workspace_session_persistence.rs",
            include_str!("../workspace_session_persistence.rs"),
        ),
        (
            "provider_usage_runtime.rs",
            include_str!("../provider_usage_runtime.rs"),
        ),
        (
            "provider_route_switch.rs",
            include_str!("../agent_stream/stream_task/provider_route_switch.rs"),
        ),
        (
            "context_compaction/persist_display.rs",
            include_str!("../context_compaction/persist_display.rs"),
        ),
        (
            "conversation_run_services.rs",
            include_str!("../conversation_run_services.rs"),
        ),
        (
            "knowledge_base_services.rs",
            include_str!("../knowledge_base_services.rs"),
        ),
    ];
    let global_gate_users = sources
        .iter()
        .filter(|(_, source)| source.contains("webui_workspace_persist_lock"))
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    assert!(
        global_gate_users.is_empty(),
        "session hot persistence reintroduced the global workspace gate: {global_gate_users:?}"
    );
    assert!(
        sources
            .iter()
            .all(|(_, source)| source.contains("webui_session_persist_lock")),
        "every session hot persistence module must use the session-keyed channel"
    );
}

/// Verifies lifecycle mutations bridge the global catalog gate to deterministic session channels.
/// 验证生命周期变更会把全局目录闸门桥接到确定性会话通道。
#[test]
fn session_lifecycle_mutations_bridge_global_and_session_persistence_gates() {
    // sources own fork, archive, cleanup, project archive, and project removal transactions.
    // sources 持有派生、归档、清理、项目归档与项目删除事务。
    let sources = [
        (
            "conversation_actions.rs",
            include_str!("../conversation_actions.rs"),
        ),
        (
            "project_conversation_services/conversation_services.rs",
            include_str!("../project_conversation_services/conversation_services.rs"),
        ),
        (
            "project_conversation_services/project_services.rs",
            include_str!("../project_conversation_services/project_services.rs"),
        ),
    ];
    let missing_session_bridge = sources
        .iter()
        .filter(|(_, source)| {
            source.contains("webui_workspace_persist_lock")
                && !source.contains("lock_webui_session_persist_channels")
        })
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    assert!(
        missing_session_bridge.is_empty(),
        "workspace lifecycle mutations must exclude exact session writers: {missing_session_bridge:?}"
    );
}

/// Verifies request-time VMM refreshes update one session instead of rebuilding every project and session.
/// 验证请求期 VMM 刷新只更新单个会话，而不会重建全部项目与会话。
#[test]
fn request_time_vmm_refresh_never_rebuilds_the_global_snapshot() {
    // sources cover submit, queued dispatch, Continue, goal chaining, completion, and compaction paths.
    // sources 覆盖发送、排队分派、Continue、目标续轮、完成与压缩路径。
    let sources = [
        (
            "conversation_submission.rs",
            include_str!("../conversation_submission.rs"),
        ),
        (
            "webui_prompt_lifecycle/pending_dispatch.rs",
            include_str!("../webui_prompt_lifecycle/pending_dispatch.rs"),
        ),
        (
            "webui_prompt_lifecycle/pending_prepare.rs",
            include_str!("../webui_prompt_lifecycle/pending_prepare.rs"),
        ),
        (
            "webui_prompt_lifecycle/precheck_continue.rs",
            include_str!("../webui_prompt_lifecycle/precheck_continue.rs"),
        ),
        (
            "webui_turn_chaining.rs",
            include_str!("../webui_turn_chaining.rs"),
        ),
        (
            "execution_report_memory.rs",
            include_str!("../execution_report_memory.rs"),
        ),
        (
            "context_compaction/completion.rs",
            include_str!("../context_compaction/completion.rs"),
        ),
    ];
    let global_rebuilds = sources
        .iter()
        .filter(|(_, source)| source.contains("build_vmm_hook_config_snapshot("))
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    assert!(
        global_rebuilds.is_empty(),
        "request-time VMM paths reintroduced global snapshot rebuilds: {global_rebuilds:?}"
    );
    let missing_exact_projection = sources
        .iter()
        .filter(|(_, source)| {
            !source.contains("refresh_exact_vmm_hook_session_projection")
                && !source.contains("replace_session_projection")
        })
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    assert!(
        missing_exact_projection.is_empty(),
        "every request-time VMM path must use the exact session projection: {missing_exact_projection:?}"
    );
}

/// Verifies context compaction persistence transfers owned state without complete snapshot clones.
/// 验证上下文压缩持久化通过所有权转移推进且不会复制完整快照。
#[test]
fn context_compaction_persistence_does_not_clone_complete_snapshots() {
    // source owns display-message persistence and runtime-state publication after compaction.
    // source 持有压缩后的展示消息持久化与运行态发布。
    let source = include_str!("../context_compaction/persist_display.rs");
    assert!(!source.contains("workspace_snapshot.clone()"));
    assert!(!source.contains("runtime_state.clone()"));
    assert!(source.contains("let mut persistence_snapshot = workspace_snapshot;"));
}

/// Verifies session addon controls resolve one retained directory and avoid full workspace ownership copies.
/// 验证会话附属控制会解析单个保留目录且不会复制完整工作区所有权。
#[test]
fn session_addon_controls_use_the_exact_directory_api() {
    // source owns all WebUI session addon control persistence calls.
    // source 持有全部 WebUI 会话附属控制持久化调用。
    let source = include_str!("../conversation_run_services.rs");
    assert!(source.contains("require_segmented_session_directory_for_retained_session("));
    assert!(
        source.contains("persist_single_session_addon_state_update_in_directory("),
        "addon controls must write through the already-resolved exact directory"
    );
    assert!(!source.contains("workspace_snapshot.clone()"));
    assert!(!source.contains("workspace.clone()"));
}

/// Verifies existing-session submission admission and queue commit never clone the full retained workspace.
/// 验证已有会话提交准入与排队提交绝不会复制完整保留工作区。
#[test]
fn existing_submission_hot_path_uses_one_session_slices() {
    // preflightSource owns retained admission snapshot capture.
    // preflightSource 持有保留准入快照捕获。
    let preflight_source = include_str!("../conversation_submission/preflight.rs");
    assert!(preflight_source.contains("existing_submission_workspace_slice("));
    assert!(
        !preflight_source.contains("then(|| snapshot.clone())"),
        "existing-session preflight must not clone every retained session"
    );
    assert!(
        !preflight_source.contains("workspace_snapshot.session_messages.get(&session_id)"),
        "existing-session preflight must not clone retained presentation history"
    );
    assert!(
        !preflight_source.contains("workspace_snapshot.session_compactions.get(&session_id)"),
        "existing-session preflight must not clone retained compaction history"
    );

    // queueSource owns busy-session queue mutation and exact merge-back.
    // queueSource 持有忙碌会话排队变更与精确回并。
    let queue_source = include_str!("../conversation_submission/queue.rs");
    assert!(queue_source.contains("existing_submission_workspace_slice("));
    assert!(queue_source.contains("std::mem::take(&mut runtime_state.conversation_state)"));
    assert!(queue_source.contains("merge_existing_session_slice_into_retained_workspace("));
    assert!(
        !queue_source.contains("current_workspace.clone()"),
        "queue commit must not clone the complete retained workspace"
    );

    // kernelSource owns the post-ack preparation capture that previously cloned the complete workspace.
    // kernelSource 持有此前会复制完整工作区的确认后准备捕获逻辑。
    let kernel_source = include_str!("../session_runtime_kernel.rs");
    assert!(kernel_source.contains("existing_submission_workspace_slice("));
    let capture_start = kernel_source
        .find("pub(crate) fn capture_webui_prompt_preparation_workspace(")
        .expect("prompt preparation capture function");
    let capture_end = kernel_source[capture_start..]
        .find("pub(crate) fn capture_webui_prompt_preparation_generation(")
        .map(|offset| capture_start + offset)
        .expect("prompt preparation generation function");
    let capture_source = &kernel_source[capture_start..capture_end];
    assert!(
        !capture_source.contains("workspace_snapshot.clone()"),
        "post-ack preparation must not clone the complete retained workspace"
    );
}

/// Verifies streaming, Continue checkpoints, and prompt-edit rollback never clone complete canonical history for local metadata changes.
/// 验证流式更新、Continue checkpoint 与提示词编辑回滚不会为局部元数据变更复制完整规范历史。
#[test]
fn conversation_mutation_hot_paths_transfer_or_borrow_canonical_history() {
    // projectionSource owns every high-frequency thinking and assistant stream delta.
    // projectionSource 持有每个高频思考与助手流式增量。
    let projection_source = include_str!("../agent_stream/projection.rs");
    // appendStart identifies the production stream append transaction.
    // appendStart 标识生产流式追加事务。
    let append_start = projection_source
        .find("fn append_stream_delta_record(")
        .expect("stream append function");
    // appendEnd excludes sealing and test-only helpers from the guard.
    // appendEnd 从守卫范围排除封口与仅测试辅助逻辑。
    let append_end = projection_source[append_start..]
        .find("fn seal_stream_delta_record(")
        .map(|offset| append_start + offset)
        .expect("stream seal function");
    // appendSource is the exact high-frequency production body under review.
    // appendSource 是接受审核的精确高频生产函数体。
    let append_source = &projection_source[append_start..append_end];
    assert!(append_source.contains(".session_runtime_state\n                .entry(session_id)"));
    assert!(
        !append_source.contains(
            ".session_runtime_state\n                .get(&session_id)\n                .cloned()"
        ),
        "each stream delta must mutate the exact runtime row instead of cloning canonical history"
    );

    // submissionSource owns both exact Continue checkpoint commits.
    // submissionSource 持有两个精确 Continue checkpoint 提交。
    let submission_source = include_str!("../conversation_submission.rs");
    assert!(submission_source.contains("apply_continuation_owned_runtime_fields("));
    assert!(
        !submission_source.contains("let mut merged = live.clone();"),
        "Continue field merge must patch the retained row before making one necessary detached copy"
    );

    // editSource owns destructive historical prompt rollback and failure restoration.
    // editSource 持有破坏性历史提示词回滚与失败恢复。
    let edit_source = include_str!("../conversation_submission/edit.rs");
    assert!(edit_source.contains("workspace.session_messages.remove(&session_id)"));
    assert!(edit_source.contains("workspace.session_runtime_state.remove(&session_id)"));
    assert!(edit_source.contains("take_workbench_conversation_messages("));
    assert!(
        !edit_source
            .contains("session_messages: workspace.session_messages.get(&session_id).cloned()"),
        "prompt-edit rollback must move retained transcript history into its backup"
    );
    assert!(
        !edit_source
            .contains("runtime_state: workspace.session_runtime_state.get(&session_id).cloned()"),
        "prompt-edit rollback must move retained canonical history into its backup"
    );

    // runtimeHelperSource owns runtime entry materialization and optional display repair.
    // runtimeHelperSource 持有运行态条目物化与可选显示修复。
    let runtime_helper_source = include_str!("../runtime_workspace_helpers.rs");
    // ensureStart identifies the display-repair wrapper.
    // ensureStart 标识显示修复包装函数。
    let ensure_start = runtime_helper_source
        .find("pub(crate) fn ensure_webui_session_runtime_state(")
        .expect("runtime state ensure function");
    // ensureEnd separates the hot entry-only helper from unrelated runtime helpers.
    // ensureEnd 将热路径仅条目辅助函数与无关运行时辅助函数分开。
    let ensure_end = runtime_helper_source[ensure_start..]
        .find("pub(crate) fn session_has_pending_plan_artifact(")
        .map(|offset| ensure_start + offset)
        .expect("pending plan helper");
    // ensure_source contains both runtime ensure variants under review.
    // ensure_source 包含接受审核的两种运行态确保接口。
    let ensure_source = &runtime_helper_source[ensure_start..ensure_end];
    assert!(ensure_source.contains(".session_runtime_state\n        .entry(session_id)"));
    assert!(
        !ensure_source.contains(".get(&session_id)\n        .cloned()"),
        "runtime entry materialization must not clone canonical history"
    );

    // goalSource owns automatic chained goal continuation capture.
    // goalSource 持有自动链式目标续跑捕获。
    let goal_source = include_str!("../webui_turn_chaining.rs");
    assert!(goal_source.contains("existing_submission_workspace_slice(snapshot, session_id)"));
    assert!(
        !goal_source.contains("then(|| snapshot.clone())"),
        "automatic goal continuation must not clone the complete retained workspace"
    );
}

/// Verifies an ordinary accepted turn cannot delete its prior durable retry body before replacement state persistence.
/// 验证普通已接纳回合不能在替代状态持久化之前删除旧的可靠重试正文。
#[test]
fn accepted_turn_defers_consumed_retry_cleanup_until_state_persistence() {
    // source owns retry checkpoint replacement, retained-state commit, background persistence, and cleanup.
    // source 持有重试检查点替换、保留状态提交、后台持久化与清理。
    let source = include_str!("../conversation_submission.rs");
    // retentionStart identifies the memory-only checkpoint constructor.
    // retentionStart 标识仅内存检查点构造函数。
    let retention_start = source
        .find("fn retain_accepted_preparation_retry_checkpoint(")
        .expect("accepted retry retention function");
    // retentionEnd excludes later submission orchestration from the no-filesystem-mutation guard.
    // retentionEnd 从禁止文件系统变更的守卫中排除后续提交编排。
    let retention_end = source[retention_start..]
        .find("/// Freezes the exact provider, model, and credential selected")
        .map(|offset| retention_start + offset)
        .expect("model selection documentation boundary");
    let retention_source = &source[retention_start..retention_end];
    assert!(retention_source.contains("project_execution_retry_snapshot("));
    assert!(
        !retention_source.contains("delete_execution_retry_snapshot("),
        "detached retry retention must never retire the still-durable crash boundary"
    );

    // cleanupMarker is deliberately adjacent to the successful state-persistence branch.
    // cleanupMarker 被刻意放在状态持久化成功分支旁边。
    let cleanup_marker = source
        .find("Only a successful exact state persist may retire the consumed durable body.")
        .expect("post-persistence retry cleanup marker");
    let persist_before_cleanup = source[..cleanup_marker]
        .rfind("persist_latest_retained_workspace_session_update(")
        .expect("state persistence before retry cleanup");
    let delete_after_cleanup = source[cleanup_marker..]
        .find("delete_execution_retry_snapshot(")
        .map(|offset| cleanup_marker + offset)
        .expect("digest-bound retry deletion after persistence");
    assert!(persist_before_cleanup < cleanup_marker);
    assert!(cleanup_marker < delete_after_cleanup);
}
