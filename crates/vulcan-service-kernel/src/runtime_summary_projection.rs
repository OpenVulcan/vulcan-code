//! runtime summary projection helpers.
//! runtime summary projection 辅助逻辑。

use super::*;

/// Creates basic tool call summaries for service bridge commands.
/// 为 service bridge 命令创建基础工具调用摘要。
pub(crate) fn tool_calls() -> Vec<ToolCallSummary> {
    vec![
        ToolCallSummary {
            id: String::from("service-startup"),
            name: String::from("service_startup_snapshot"),
            status: String::from("ready"),
            detail: String::from("WebUI 可读取真实启动阶段快照。"),
        },
        ToolCallSummary {
            id: String::from("service-workbench"),
            name: String::from("service_workbench_snapshot"),
            status: String::from("ready"),
            detail: String::from("WebUI 可读取真实项目、会话索引与插件信息。"),
        },
    ]
}

/// Creates command-session summaries for currently retained service backends.
/// 为当前保留的服务后端创建命令会话摘要。
pub(crate) fn command_sessions(
    command_session_runtime: Option<&CommandSessionRuntime>,
) -> Vec<CommandSessionSummary> {
    let mut summaries = command_session_runtime
        .map(|runtime| {
            runtime
                .try_snapshot_sessions()
                .into_iter()
                .map(command_session_summary_from_snapshot)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    summaries.sort_by(|left, right| {
        left.name.cmp(&right.name).then(
            left.command
                .cmp(&right.command)
                .then(left.id.cmp(&right.id)),
        )
    });
    summaries
}

/// Converts one live command-session snapshot into the WebUI summary contract.
/// 将单条实时命令会话快照转换为 WebUI 摘要契约。
pub(crate) fn command_session_summary_from_snapshot(
    snapshot: vulcan_agent_runtime::tooling::command_session_runtime::CommandSessionSnapshot,
) -> CommandSessionSummary {
    let preview = format!(
        "stdout={} stderr={} last_seq={}",
        snapshot.stdout_bytes, snapshot.stderr_bytes, snapshot.last_seq
    );
    CommandSessionSummary {
        id: snapshot.command_session_id.to_string(),
        name: snapshot.display_name,
        command: snapshot.command,
        status: snapshot.status.as_str().to_string(),
        preview,
        owner_session_id: Some(snapshot.owner_session_id.to_string()),
        cwd: Some(vulcan_runtime_assets::native_host_path_text_from_path(
            &snapshot.cwd,
        )),
        shell_label: Some(snapshot.shell_label),
        backend: Some(snapshot.backend.as_str().to_string()),
        screen_frame_seq: snapshot.screen_frame_seq,
        screen_rows: snapshot.screen_rows,
        screen_cols: snapshot.screen_cols,
        screen_text: snapshot.screen_text,
        dropped_before_seq: snapshot.dropped_before_seq,
        exit_code: snapshot.exit_code,
        failure_reason: snapshot.failure_reason,
        lost: false,
    }
}

/// Creates runtime module cards shown by the dashboard.
/// 创建仪表盘展示的运行时模块卡片。
pub(crate) fn runtime_modules(
    controller: &HostRuntimeHandle,
    luaskills_catalog: &TauriLuaSkillsCatalog,
    mcp_catalog: &TauriMcpRuntimeCatalog,
) -> Vec<RuntimeModuleSnapshot> {
    vec![
        runtime_module(
            "core",
            "Core Service",
            StartupStepStatus::Ready,
            "Core Service 运行中",
        ),
        runtime_module(
            "vldb",
            "VLDB",
            StartupStepStatus::Ready,
            &format!("进程内引擎 · {}", controller.sqlite_path.display()),
        ),
        runtime_module(
            "luaskills",
            "LuaSkills",
            StartupStepStatus::Ready,
            &format!(
                "{} · 技能 {} · 入口 {}",
                luaskills_catalog.runtime_root.display(),
                luaskills_catalog.skills.len(),
                luaskills_catalog.entry_count
            ),
        ),
        runtime_module(
            "mcp",
            "MCP Runtime",
            if mcp_catalog.enabled_server_count == 0 || mcp_catalog.connected_server_count > 0 {
                StartupStepStatus::Ready
            } else {
                StartupStepStatus::Warning
            },
            &format!(
                "server {}/{} · connected {} · tools {} · prompts {} · resources {}",
                mcp_catalog.enabled_server_count,
                mcp_catalog.configured_server_count,
                mcp_catalog.connected_server_count,
                mcp_catalog.tool_count,
                mcp_catalog.prompt_count,
                mcp_catalog.resource_count
            ),
        ),
        runtime_module(
            "workspace",
            "Workspace",
            StartupStepStatus::Ready,
            "项目与会话快照已恢复",
        ),
    ]
}

/// Creates one runtime module snapshot.
/// 创建单个运行时模块快照。
pub(crate) fn runtime_module(
    id: &str,
    label: &str,
    status: StartupStepStatus,
    message: &str,
) -> RuntimeModuleSnapshot {
    RuntimeModuleSnapshot {
        id: id.to_string(),
        label: label.to_string(),
        status,
        message: message.to_string(),
    }
}
