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
                        "Tauri 本地服务桥接已启动",
                    ),
                    (
                        "vldb",
                        StartupStepStatus::Ready,
                        "VLDB/controller 数据平面已就绪",
                    ),
                    ("luaskills", StartupStepStatus::Running, message),
                    (
                        "token-budget",
                        StartupStepStatus::Pending,
                        "等待 LuaSkills 就绪",
                    ),
                    ("mcp", StartupStepStatus::Pending, "等待 Token 预算运行时"),
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
    let mut startup = startup_snapshot_with_phase(
        "failed",
        startup_failure_steps(OffsetDateTime::now_utc(), &error),
        &logs,
    );
    let mut workbench = fallback_workbench_snapshot(&logs);
    if let Ok(mut guard) = state.lock() {
        startup.ui_locale = guard.startup.ui_locale.clone();
        workbench.ui_locale = guard.startup.ui_locale.clone();
        guard.startup = startup.clone();
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench = workbench.clone();
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
        "token-budget",
        "mcp",
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
                updated_at: Some(updated_at),
            }
        })
        .collect()
}

/// Returns the stable ready message for one startup step.
/// 返回单个启动步骤的稳定就绪文案。
pub(crate) fn startup_ready_message(step_id: &str) -> String {
    match step_id {
        "service-process" => "Tauri 本地服务桥接已启动",
        "vldb" => "VLDB/controller 数据平面已就绪",
        "luaskills" => "LuaSkills 引擎与技能目录已就绪",
        "token-budget" => "Token 预算运行时已初始化",
        "mcp" => "MCP Runtime 已初始化",
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
