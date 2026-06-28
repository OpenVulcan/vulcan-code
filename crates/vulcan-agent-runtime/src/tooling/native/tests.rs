use super::{
    CommandSessionBackend, CommandSessionInputEvent, CommandSessionOutputChunk,
    CommandSessionSnapshot, NativeToolExecutionRequest, RtkRoutingSupport, SUBMIT_PLAN_MARKER,
    TerminalSessionInput, TerminalSessionKeyEncoding, ThreadGoal, ThreadGoalStatus,
    build_agent_list_tool, build_mcp_server_list_tool, build_shell_command_tool,
    build_shell_session_tool, build_skill_list_tool, build_submit_plan_tool,
    build_terminal_session_tool, build_todolist_tool, build_workspace_describe_tool,
    coalesce_terminal_output_chunks, collect_rtk_route_names, display_submit_plan_path,
    effective_start_append_newline, encode_terminal_inputs, goal_response_value,
    normalize_terminal_session_name, resolve_session_shell_behavior,
    resolve_shell_command_launcher, resolve_shell_command_working_directory,
    shell_command_args_schema, shell_specific_newline, strip_simple_ansi_sequences,
    strip_terminal_transaction_sentinel, terminal_inputs_request_interrupt,
    terminal_interrupt_drain_chunk_is_visible, terminal_output_chunks_contain_pattern,
    terminal_screen_text_from_chunks, terminal_session_args_schema,
    terminal_transaction_command_text, validate_shell_session_start_stdin_policy,
    validate_terminal_session_start_shell_policy, validate_terminal_transaction_start,
    windows_console_input_events_from_inputs, wrap_terminal_transaction_command,
};
use crate::agent_runtime::config::{
    AgentRuntimeConfig, RuntimeAgentDefinition, RuntimeAgentKind, RuntimeAgentRoute,
    RuntimeSkillDefinition, RuntimeSkillKind,
};
use crate::provider_config::{
    PersistedHostToolsConfiguration, PersistedRtkRouteMode, PersistedRtkToolConfiguration,
};
use crate::tooling::CommandSessionRuntime;
use crate::tooling::command_session_runtime::{CommandSessionOutputStream, CommandSessionStatus};
use crate::tooling::shell_host::{
    DetectedHostCommandShell, HostCommandShell, HostCommandShellSupport,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs as tokio_fs;
use uuid::Uuid;
use vulcan_vmm::VmmConfigLayer;

/// Builds one compact detected-shell record for tests that only care about shell identity and dynamic schema exposure.
/// 为仅关注 shell 身份与动态 schema 暴露的测试构建一条紧凑检测记录。
fn detected_shell(shell: HostCommandShell) -> DetectedHostCommandShell {
    DetectedHostCommandShell {
        launcher_program: shell.id().to_string(),
        shell,
    }
}

/// Builds one test shell-support snapshot that mirrors the runtime-facing resolved-shell structure.
/// 构建一份镜像运行时解析结构的测试 shell 支持快照。
fn test_shell_support() -> HostCommandShellSupport {
    HostCommandShellSupport::from_test_parts(
        vec![
            detected_shell(HostCommandShell::Pwsh),
            detected_shell(HostCommandShell::Powershell),
            detected_shell(HostCommandShell::Cmd),
        ],
        Some(HostCommandShell::Pwsh),
    )
}

/// Builds one staged-output test command that emits two chunks with a delay between them so the read cursor can be validated across multiple calls.
/// 构建一条分阶段输出测试命令：带延时地输出两个分片，便于在多次调用之间校验读取游标。
fn interactive_shell_session_test_command() -> String {
    if cfg!(windows) {
        String::from(
            "echo phase-1 & ping -n 2 127.0.0.1 >nul & echo phase-2 & ping -n 2 127.0.0.1 >nul",
        )
    } else {
        String::from("printf 'phase-1\\n'; sleep 0.6; printf 'phase-2\\n'; sleep 0.3")
    }
}

/// Builds one long-running test command that keeps the child process alive until the host explicitly stops it.
/// 构建一条长时间运行的测试命令，使子进程持续存活直到宿主显式停止它。
fn long_running_shell_session_test_command() -> String {
    if cfg!(windows) {
        String::from("while ($true) { Start-Sleep -Milliseconds 100 }")
    } else {
        String::from("while true; do sleep 0.1; done")
    }
}

/// Builds one native-tool execution request with only arguments varied.
/// 构建一条仅变更参数的原生工具执行请求。
fn test_native_request(arguments: serde_json::Value) -> NativeToolExecutionRequest {
    NativeToolExecutionRequest {
        arguments,
        session_id: None,
        workspace_root: None,
        controller_runtime: None,
        spill_root: None,
        enabled_prompt_skill_names: Vec::new(),
        prompt_skill_user_root: None,
        project_vmm: VmmConfigLayer::default(),
        session_vmm: VmmConfigLayer::default(),
    }
}

/// Builds one native-tool execution request with a workspace root.
/// 构建一条带工作区根目录的原生工具执行请求。
fn test_native_request_with_workspace(
    arguments: serde_json::Value,
    workspace_root: PathBuf,
) -> NativeToolExecutionRequest {
    NativeToolExecutionRequest {
        workspace_root: Some(workspace_root),
        ..test_native_request(arguments)
    }
}

mod planning;
mod shell;
mod terminal;
