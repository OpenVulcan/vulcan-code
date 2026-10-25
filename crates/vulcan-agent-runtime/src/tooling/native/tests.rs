use super::{
    BROWSER_CLOSE_TOOL_ID, BROWSER_LIST_TOOL_ID, BROWSER_OPEN_TOOL_ID, BROWSER_SELECT_TOOL_ID,
    CommandSessionBackend, CommandSessionOutputChunk, CommandSessionSnapshot,
    DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS, MAX_COMMAND_SESSION_READ_CHUNKS,
    MAX_COMMAND_SESSION_READ_WAIT_MS, NativeGoalRuntime, NativeGoalSnapshot, NativeGoalStatus,
    NativeGoalTerminalStatus, NativeToolExecutionRequest, SUBMIT_PLAN_MARKER, TerminalSessionInput,
    TerminalSessionToolAction, TerminalSessionToolArgs, TerminalSessionWaitPolicy,
    build_agent_list_tool, build_create_goal_tool, build_get_goal_tool, build_knowledge_list_tool,
    build_knowledge_read_tool, build_knowledge_search_tool, build_knowledge_write_tool,
    build_mcp_server_list_tool, build_native_tool_catalog, build_prompt_skill_detail_tool,
    build_shell_command_tool, build_skill_list_tool, build_submit_plan_tool,
    build_terminal_session_host_payload, build_terminal_session_render_input,
    build_terminal_session_tool, build_todolist_tool, build_update_goal_tool,
    build_workspace_describe_tool, coalesce_terminal_output_chunks,
    default_terminal_session_backend, display_submit_plan_path, goal_response_value,
    normalize_terminal_session_name, resolve_session_shell_behavior,
    resolve_shell_command_launcher, resolve_shell_command_working_directory,
    shell_command_args_schema, shell_specific_newline, terminal_inputs_request_interrupt,
    terminal_interrupt_drain_chunk_is_visible, terminal_output_chunks_contain_pattern,
    terminal_read_chunk_budget, terminal_screen_text_from_chunks, terminal_session_args_schema,
    terminal_wait_budget_ms, validate_terminal_session_start_shell_policy,
};
use crate::agent_runtime::config::{
    AgentRuntimeConfig, RuntimeAgentDefinition, RuntimeAgentKind, RuntimeAgentRoute,
    RuntimeSkillDefinition, RuntimeSkillKind,
};
use crate::tooling::CommandSessionRuntime;
use crate::tooling::command_session_runtime::{CommandSessionOutputStream, CommandSessionStatus};
use crate::tooling::shell_host::{
    DetectedHostCommandShell, HostCommandShell, HostCommandShellSupport,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs as tokio_fs;
use uuid::Uuid;
use vulcan_vmm::VmmConfigLayer;
use vulcan_workbench_model::SessionId;

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

/// Returns one interactive shell command that keeps a terminal session alive until the host explicitly stops it.
/// 返回一条交互式 shell 命令，使终端会话持续存活直到宿主显式停止它。
fn long_lived_terminal_test_command() -> &'static str {
    if cfg!(windows) { "cmd /D /Q" } else { "sh" }
}

/// Builds one native-tool execution request with only arguments varied.
/// 构建一条仅变更参数的原生工具执行请求。
fn test_native_request(arguments: serde_json::Value) -> NativeToolExecutionRequest {
    NativeToolExecutionRequest {
        invocation_context: None,
        arguments,
        session_id: None,
        native_browser_runtime: None,
        workspace_root: None,
        host_project_id: None,
        controller_runtime: None,
        goal_runtime: None,
        environment: None,
        session_mcp: None,
        knowledge_access: None,
        spill_root: None,
        enabled_prompt_skill_names: Vec::new(),
        prepared_prompt_skills: Vec::new(),
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

/// Verifies the native catalog registers every direct browser and file tool.
/// 验证原生工具目录注册了全部直接浏览器与文件工具。
#[test]
fn native_catalog_registers_browser_and_file_tools() {
    let config = AgentRuntimeConfig {
        active_model: None,
        active_agent_id: String::from("test-agent"),
        agents: Vec::new(),
        skills: Vec::new(),
        tool_policies: Default::default(),
        configured_mcp_server_ids: Vec::new(),
        enabled_mcp_server_ids: Vec::new(),
    };
    let catalog = build_native_tool_catalog(
        &config,
        None,
        Arc::new(CommandSessionRuntime::new()),
        VmmConfigLayer::default(),
        None,
        None,
    );

    // Retired semantic retrieval must have no native registration or execution entry.
    // 已退役的语义检索不能保留原生注册或执行入口。
    assert!(catalog.definition("codebase_retrieval").is_none());

    for tool_id in [
        BROWSER_LIST_TOOL_ID,
        BROWSER_OPEN_TOOL_ID,
        BROWSER_SELECT_TOOL_ID,
        BROWSER_CLOSE_TOOL_ID,
        super::FILE_TOOL_ID,
        super::FILE_READ_TOOL_ID,
        super::FILE_WRITE_TOOL_ID,
        super::FILE_EDIT_TOOL_ID,
        super::APPLY_PATCH_TOOL_ID,
    ] {
        assert!(
            catalog.definition(tool_id).is_some(),
            "native tool {tool_id} should be registered"
        );
    }
}

/// Verifies the skill detail tool reads the execution-prepared snapshot without rescanning mutable filesystem roots.
/// 验证技能详情工具读取执行准备阶段的快照，而不会重新扫描可变文件系统根目录。
#[tokio::test]
async fn prompt_skill_detail_uses_execution_prepared_snapshot() {
    // This directory is intentionally never created, proving that metadata and body come from the prepared snapshot.
    // 此目录刻意不创建，以证明元数据与正文来自已准备快照。
    let missing_base_dir =
        std::env::temp_dir().join(format!("vulcan-prepared-prompt-skill-{}", Uuid::new_v4()));
    // The prepared plugin skill mirrors the namespaced record produced by the plugin catalog loader.
    // 已准备的插件技能镜像插件目录加载器产出的命名空间记录。
    let prepared_skill = vulcan_prompt_skills::PromptSkill {
        name: String::from("fixture-plugin:review"),
        description: String::from("Review prepared code."),
        license: None,
        compatibility: None,
        metadata: BTreeMap::new(),
        skill_file: missing_base_dir.join("SKILL.md"),
        base_dir: missing_base_dir.clone(),
        scope: vulcan_prompt_skills::PromptSkillScope::Plugin,
        interface: Default::default(),
        catalog: Default::default(),
        origin: Default::default(),
        resources: vulcan_prompt_skills::PromptSkillResourceSummary {
            scripts: 1,
            sampled_files: vec![PathBuf::from("scripts").join("review.ps1")],
            ..vulcan_prompt_skills::PromptSkillResourceSummary::default()
        },
        body: String::from("# Prepared snapshot body"),
    };
    // The request contains the immutable turn-level snapshot and no usable user root.
    // 请求仅携带不可变的回合级快照，不提供可用的用户根目录。
    let request = NativeToolExecutionRequest {
        enabled_prompt_skill_names: vec![prepared_skill.name.clone()],
        prepared_prompt_skills: vec![prepared_skill],
        ..test_native_request(json!({ "name": "fixture-plugin:review" }))
    };

    // Direct invocation isolates the detail loader from catalog construction and filesystem discovery.
    // 直接调用可将详情加载器与目录构建及文件系统发现隔离开来。
    let result = build_prompt_skill_detail_tool()
        .invoke(request)
        .await
        .expect("prepared plugin skill should load without filesystem discovery");
    let output = result
        .model_output
        .as_str()
        .expect("skill detail output should be markdown text");
    assert!(output.contains("# Prepared snapshot body"));
    assert!(output.contains("fixture-plugin:review"));
    assert!(output.contains("Base directory for this skill:"));
    assert!(output.contains(
        vulcan_runtime_assets::native_host_path_text_from_path(&missing_base_dir).as_str()
    ));
    assert!(output.contains("scripts\\review.ps1") || output.contains("scripts/review.ps1"));
}

mod file_tools;
mod knowledge_base;
mod markdown_memory;
mod planning;
mod shell;
mod terminal;
