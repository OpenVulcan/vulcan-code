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
            "workspace_store/session_v1/writer.rs",
            include_str!(
                "../../../vulcan-workspace-store/src/workspace_store/session_v1/writer.rs"
            ),
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

/// Verifies V1 persistence writes only the typed ledger and never reintroduces legacy runtime envelopes.
/// 验证 V1 持久化只写入强类型账本，且不会重新引入旧运行态信封。
#[test]
fn v1_session_writer_never_serializes_legacy_runtime_envelopes() {
    // source owns the only V1 session flush implementation.
    // source 持有唯一的 V1 会话刷新实现。
    let source =
        include_str!("../../../vulcan-workspace-store/src/workspace_store/session_v1/writer.rs");
    assert!(
        source.contains("FlushCandidate"),
        "V1 persistence must consume the Core durable flush candidate"
    );
    assert!(
        !source.contains("SessionRuntimeStateEntry"),
        "V1 persistence must not serialize the retired runtime-state envelope"
    );
    assert!(
        !source.contains("state.json"),
        "V1 persistence must not write the retired segmented state document"
    );
}

/// Verifies Provider accounting uses telemetry ownership without legacy runtime persistence.
/// 验证 Provider 记账使用遥测归属且不再持久化旧运行态。
#[test]
fn provider_boundaries_use_owner_identity_without_legacy_runtime_persistence() {
    // source owns Provider boundary accounting and persistence routing.
    // source 持有 Provider 边界记账与持久化路由。
    let source = include_str!("../provider_usage_runtime.rs");
    assert!(
        source.contains("telemetry_string(event, \"owner_id\")?"),
        "Provider accounting must resolve the explicit telemetry owner identity"
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
        !source.contains("project_single_session_runtime_state_update("),
        "Provider accounting must not persist the retired segmented runtime state"
    );
    assert!(
        !source.contains("persist_projected_single_session_runtime_state_update("),
        "Provider accounting must not write the retired state.json envelope"
    );
}

/// Verifies retained terminal persistence avoids complete workspace and runtime-history clones.
/// 验证保留终态持久化避免完整工作区与运行时历史克隆。
#[test]
fn retained_terminal_persistence_uses_exact_history_free_payloads() {
    // registrySource owns every durable V1 session mutation and flush boundary.
    // registrySource 持有每个 V1 会话可靠变更与 flush 边界。
    let registry_source = include_str!("../session_runtime_registry_v1.rs");
    assert!(
        registry_source.contains("pub(crate) struct SessionHandle"),
        "V1 persistence must retain one explicit per-session owner"
    );
    assert!(
        registry_source.contains("flush_through"),
        "V1 persistence must flush through an observed document revision"
    );
    assert!(
        !registry_source.contains("workspace_snapshot.clone()"),
        "V1 session persistence must not clone the global workspace snapshot"
    );
}

/// Verifies exact session writers never wait behind the process-wide workspace lifecycle gate.
/// 验证精确会话写入者绝不会等待进程级工作区生命周期闸门。
#[test]
fn session_hot_persistence_uses_session_keyed_channels() {
    // sources contain the only durable session owner and lifecycle publication boundary.
    // sources 仅包含唯一可靠会话所有者与生命周期发布边界。
    let sources = [
        (
            "session_runtime_registry_v1.rs",
            include_str!("../session_runtime_registry_v1.rs"),
        ),
        (
            "session_v1_lifecycle.rs",
            include_str!("../session_v1_lifecycle.rs"),
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
            .all(|(_, source)| !source.contains("webui_session_persist_lock")),
        "V1 persistence must not depend on the retired versioned session channel"
    );
}

/// Verifies lifecycle mutations bridge the global catalog gate to deterministic session channels.
/// 验证生命周期变更会把全局目录闸门桥接到确定性会话通道。
#[test]
fn session_lifecycle_mutations_bridge_global_and_session_persistence_gates() {
    // lifecycleSource owns metadata-first publication and SQLite projection ordering.
    // lifecycleSource 持有 metadata-first 发布与 SQLite 投影顺序。
    let lifecycle_source = include_str!("../session_v1_lifecycle.rs");
    assert!(
        lifecycle_source.contains("acquire_lifecycle_transition"),
        "V1 lifecycle mutations must serialize through the per-session lifecycle gate"
    );
    assert!(
        lifecycle_source.contains("project_lifecycle_metadata_or_report"),
        "V1 lifecycle mutations must project SQLite only after metadata publication"
    );
    assert!(
        !lifecycle_source.contains("lock_webui_session_persist_channels"),
        "V1 lifecycle must not bridge the retired persistence-version channels"
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
}

/// Verifies startup and index refreshes never infer body work from the selected session id.
/// 验证启动与索引刷新绝不会根据选中会话标识推断正文工作。
#[test]
fn startup_and_index_refresh_never_hydrate_selected_session() {
    // bootstrapSource owns startup projection while refreshSource owns later index refreshes.
    // bootstrapSource 负责启动投影，refreshSource 负责后续索引刷新。
    let bootstrap_source = include_str!("../service_bootstrap.rs");
    let refresh_source = include_str!("../workbench_refresh.rs");
    let forbidden_bootstrap = [
        "runtime.block_on(crate::refresh_workbench_snapshot",
        "vmm_controller_registry.get_or_open",
    ];
    let forbidden_refresh = [
        "hydrate_selected_session_runtime_if_needed(",
        "apply_persisted_task_mirror_statuses_to_workspace(",
        "sync_selected_session_goal_summary_from_store(",
        "vmm_controller_registry.get_or_open(",
    ];
    let bootstrap_violations = forbidden_bootstrap
        .iter()
        .filter(|needle| bootstrap_source.contains(**needle))
        .copied()
        .collect::<Vec<_>>();
    let refresh_violations = forbidden_refresh
        .iter()
        .filter(|needle| refresh_source.contains(**needle))
        .copied()
        .collect::<Vec<_>>();
    assert!(
        bootstrap_violations.is_empty(),
        "startup reintroduced selected-session body work: {bootstrap_violations:?}"
    );
    assert!(
        refresh_violations.is_empty(),
        "index refresh reintroduced selected-session body work: {refresh_violations:?}"
    );
}

/// Verifies request-time restart projection uses V1 and exposes no segmented hydration entry.
/// 验证请求时重启投影使用 V1，且不再暴露分段水合入口。
#[test]
fn request_time_restart_projection_uses_v1_without_segmented_hydration() {
    // serviceSource owns every request-time restart projection entry point.
    // serviceSource 持有全部请求时重启投影入口。
    let service_source = include_str!("../session_hydration/loading.rs");
    assert!(service_source.contains("sync_selected_session_v1_projection_from_store("));
    assert!(!service_source.contains("load_session_execution_hydration_snapshot"));
    assert!(!service_source.contains("load_segmented_runtime_state("));

    // storeModule proves the old hydration API is not exported from workspace-store.
    // storeModule 证明 workspace-store 不再导出旧水合 API。
    let store_module = include_str!("../../../vulcan-workspace-store/src/workspace_store/mod.rs");
    assert!(!store_module.contains("mod session_load;"));
    assert!(!store_module.contains("load_session_execution_hydration_snapshot"));
    assert!(!store_module.contains("load_session_hydration_snapshot"));
}

/// Verifies context compaction persistence uses the registered V1 owner exclusively.
/// 验证上下文压缩持久化只使用已注册的 V1 所有者。
#[test]
fn context_compaction_persistence_does_not_clone_complete_snapshots() {
    // source owns the final durable barrier after a compaction commit.
    // source 持有压缩提交后的最终可靠屏障。
    let source = include_str!("../context_compaction/persist_display.rs");
    assert!(!source.contains("workspace_snapshot.clone()"));
    assert!(!source.contains("runtime_state.clone()"));
    assert!(source.contains("registered_session_v1_handle_from_state"));
    assert!(source.contains("handle.flush_all()"));
    assert!(!source.contains("persist_latest_retained_workspace_session_update_unlocked("));
    assert!(!source.contains("persist_workspace_session_update_unlocked("));
}

/// Verifies session controls persist through V1 metadata instead of the retired addon document.
/// 验证会话控制通过 V1 元数据持久化，而不再写入已退役的附属文档。
#[test]
fn session_controls_use_v1_metadata_without_addon_writes() {
    // source owns all WebUI session control persistence calls.
    // source 持有全部 WebUI 会话控制持久化调用。
    let source = include_str!("../conversation_run_services.rs");
    assert!(source.contains("persist_session_plan_mode_v1("));
    assert!(source.contains("set_vmm_session_policy_value_v1("));
    assert!(source.contains("document.set_output_style("));
    assert!(source.contains("mutate_durable_if_changed("));
    assert!(!source.contains("persist_single_session_addon_state_update"));
    assert!(!source.contains("write_session_addon_state"));
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
    // queueSource owns the V1 queued-input admission.
    // queueSource 持有 V1 排队输入准入。
    let queue_source = include_str!("../conversation_submission/queue.rs");
    assert!(queue_source.contains(".enqueue_input("));
    assert!(!queue_source.contains("existing_submission_workspace_slice("));
    assert!(!queue_source.contains("std::mem::take(&mut runtime_state.conversation_state)"));
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

/// Verifies streaming semantic updates enter the V1 SessionHandle instead of retained presentation history.
/// 验证流式语义更新进入 V1 SessionHandle，而不是保留的展示历史。
#[test]
fn conversation_mutation_hot_paths_transfer_or_borrow_canonical_history() {
    // sinkSource owns every high-frequency reasoning and assistant-text semantic update.
    // sinkSource 持有每次高频思考与助手正文语义更新。
    let sink_source = include_str!("../agent_stream/provider_v1_sink.rs");
    assert!(sink_source.contains("self.handle.update_reasoning("));
    assert!(sink_source.contains("self.handle.update_assistant_text("));
    assert!(!sink_source.contains("session_runtime_state"));
    assert!(!sink_source.contains("session_messages"));
}
