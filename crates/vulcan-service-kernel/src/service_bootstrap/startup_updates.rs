//! Service bootstrap startup updates helpers.
//! 服务引导 startup updates 辅助逻辑。

use super::*;

pub(crate) fn update_startup<F>(
    state: &SharedServiceBridgeState,
    event_emitter: &ServiceEventEmitter,
    phase: &str,
    logs: &[String],
    build_steps: F,
) where
    F: FnOnce(OffsetDateTime) -> Vec<StartupStepSnapshot>,
{
    let mut startup =
        startup_snapshot_with_phase(phase, build_steps(OffsetDateTime::now_utc()), logs);
    if let Ok(mut guard) = state.lock() {
        startup.ui_locale = guard.startup.ui_locale.clone();
        startup.model_reconciliation_notices = guard.startup.model_reconciliation_notices.clone();
        guard.startup = startup.clone();
    }
    emit_service_event_with_trace(
        event_emitter,
        "startup_snapshot_emit",
        ServiceEvent::Startup(Box::new(startup)),
    );
}

/// Emits a LuaSkills-specific startup progress snapshot.
/// 发出 LuaSkills 专用启动进度快照。
pub(crate) fn update_luaskills_bootstrap_progress(
    state: &SharedServiceBridgeState,
    event_emitter: &ServiceEventEmitter,
    logs: &[String],
    message: &str,
) {
    update_startup(
        state,
        event_emitter,
        "bootstrapping-luaskills",
        logs,
        |now| {
            startup_steps_with_status(
                now,
                &[
                    (
                        "service-process",
                        StartupStepStatus::Ready,
                        "Core Service 已启动",
                    ),
                    (
                        "vldb",
                        StartupStepStatus::Ready,
                        "VLDB/controller 数据平面已就绪",
                    ),
                    ("luaskills", StartupStepStatus::Running, message),
                    ("mcp", StartupStepStatus::Pending, "等待 LuaSkills 就绪"),
                    ("plugins", StartupStepStatus::Pending, "等待插件目录初始化"),
                    (
                        "workspace",
                        StartupStepStatus::Pending,
                        "等待工作区数据库载入",
                    ),
                ],
            )
        },
    );
}

/// Emits exact native managed-runtime download progress during LuaSkills bootstrap.
/// 在 LuaSkills 初始化期间发出精确的原生受管运行时下载进度。
pub(crate) fn update_managed_runtime_bootstrap_progress(
    state: &SharedServiceBridgeState,
    event_emitter: &ServiceEventEmitter,
    logs: &[String],
    progress: &vulcan_service_runtime::managed_runtime_bootstrap::ManagedRuntimeBootstrapProgress,
) {
    update_startup(
        state,
        event_emitter,
        "bootstrapping-luaskills",
        logs,
        |now| {
            let mut steps = startup_steps_with_status(
                now,
                &[
                    (
                        "service-process",
                        StartupStepStatus::Ready,
                        "Core Service 已启动",
                    ),
                    (
                        "vldb",
                        StartupStepStatus::Ready,
                        "VLDB/controller 数据平面已就绪",
                    ),
                    (
                        "luaskills",
                        StartupStepStatus::Running,
                        progress.message.as_str(),
                    ),
                    ("mcp", StartupStepStatus::Pending, "等待 LuaSkills 就绪"),
                    ("plugins", StartupStepStatus::Pending, "等待插件目录初始化"),
                    (
                        "workspace",
                        StartupStepStatus::Pending,
                        "等待工作区数据库载入",
                    ),
                ],
            );
            if let Some(step) = steps.iter_mut().find(|step| step.id == "luaskills") {
                step.progress_percent = progress.overall_percent;
                step.completed_bytes = progress.completed_bytes;
                step.total_bytes = progress.total_bytes;
                step.progress_phase = Some(progress.phase.clone());
            }
            steps
        },
    );
}

/// Updates the shared service state after a startup failure.
/// 在启动失败后更新共享服务状态。
pub(crate) fn update_service_failure(
    state: &SharedServiceBridgeState,
    event_emitter: &ServiceEventEmitter,
    error: ServiceBootstrapError,
) {
    let logs = vec![
        format!(
            "[webui-service] startup failed at {}: {}",
            error.step_label, error.message
        ),
        format!(
            "[webui-service] workspace root: {}",
            normalized_workspace_path_text_from_path(&workspace_root())
        ),
    ];
    tracing::error!("{}", logs[0]);
    let failure = error.service_status_failure();
    let mut startup = startup_snapshot_with_phase(
        "failed",
        startup_failure_steps(OffsetDateTime::now_utc(), &error),
        &logs,
    );
    let mut workbench = fallback_workbench_snapshot(&logs);
    if let Ok(mut guard) = state.lock() {
        startup.ui_locale = guard.startup.ui_locale.clone();
        startup.model_reconciliation_notices = guard.startup.model_reconciliation_notices.clone();
        workbench.ui_locale = guard.startup.ui_locale.clone();
        guard.startup = startup.clone();
        crate::preserve_project_settings_revisions(&mut workbench, &guard.workbench);
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        workbench.revision = guard.workbench_generation;
        guard.workbench = workbench.clone();
        guard.service_lifecycle_state.last_bootstrap_failure = Some(failure);
    }
    mark_webui_service_discovery_failed(state);
    emit_service_snapshots(event_emitter, &startup, &workbench);
}

/// Builds ordered startup steps for one scoped bootstrap failure.
/// 为单个带阶段的引导失败构建有序启动步骤。
pub(crate) fn startup_failure_steps(
    updated_at: OffsetDateTime,
    error: &ServiceBootstrapError,
) -> Vec<StartupStepSnapshot> {
    let step_order = [
        "service-process",
        "vldb",
        "luaskills",
        "mcp",
        "plugins",
        "workspace",
    ];
    let failed_index = step_order
        .iter()
        .position(|step_id| *step_id == error.step_id)
        .unwrap_or(0);
    step_order
        .iter()
        .enumerate()
        .map(|(index, step_id)| {
            let (status, message) = if *step_id == error.step_id {
                (StartupStepStatus::Error, error.message.clone())
            } else if index < failed_index {
                (StartupStepStatus::Ready, startup_ready_message(step_id))
            } else {
                (
                    StartupStepStatus::Pending,
                    String::from("上游启动失败，未继续"),
                )
            };
            StartupStepSnapshot {
                id: (*step_id).to_string(),
                label: startup_step_label(step_id),
                status,
                message: Some(message),
                progress_percent: None,
                completed_bytes: None,
                total_bytes: None,
                progress_phase: None,
                updated_at: Some(updated_at),
            }
        })
        .collect()
}

/// Returns the stable ready message for one startup step.
/// 返回单个启动步骤的稳定就绪文案。
pub(crate) fn startup_ready_message(step_id: &str) -> String {
    match step_id {
        "service-process" => "Core Service 已启动",
        "vldb" => "VLDB/controller 数据平面已就绪",
        "luaskills" => "LuaSkills 引擎与技能目录已就绪",
        "mcp" => "MCP Runtime 已初始化",
        "plugins" => "插件目录与已启用运行时已就绪",
        "workspace" => "项目与会话索引已载入",
        _ => "已就绪",
    }
    .to_string()
}

/// Emits startup and workbench snapshots to subscribed WebUI clients.
/// 向已订阅的 WebUI 客户端发送启动与工作台快照。
pub(crate) fn emit_service_snapshots(
    event_emitter: &ServiceEventEmitter,
    startup: &StartupSnapshot,
    workbench: &WorkbenchSnapshot,
) {
    emit_service_event_with_trace(
        event_emitter,
        "startup_snapshot_emit",
        ServiceEvent::Startup(Box::new(startup.clone())),
    );
    emit_service_event_with_trace(
        event_emitter,
        "service_workbench_snapshot_emit",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
}
