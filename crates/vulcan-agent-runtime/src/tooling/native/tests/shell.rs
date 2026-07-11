use super::*;
use crate::tooling::native::{
    NativeToolRuntimeSnapshot, build_vmm_memory_write_tool, vmm_memory_write_args_schema,
};

/// Relative shell-command working directories should resolve below the current workspace root.
/// 相对 shell 命令工作目录应解析到当前工作区根目录之下。
#[test]
fn shell_command_working_directory_joins_relative_path_to_workspace_root() {
    let resolved = resolve_shell_command_working_directory(
        Some(Path::new(r"D:\workspace")),
        Some("subdir/bin"),
    )
    .expect("relative cwd should resolve");
    assert_eq!(resolved, Path::new(r"D:\workspace").join("subdir/bin"));
}

/// Omitted shell-command working directories should default to the current workspace root instead of the host process directory.
/// 省略 shell 命令工作目录时，应默认使用当前工作区根目录，而不是宿主进程目录。
#[test]
fn shell_command_working_directory_defaults_to_workspace_root() {
    let resolved = resolve_shell_command_working_directory(Some(Path::new(r"D:\workspace")), None)
        .expect("workspace-root default should resolve");
    assert_eq!(resolved, Path::new(r"D:\workspace"));
}

/// Shell invocations without a workspace root should still fall back to the host process working directory so non-project commands remain usable.
/// 缺少工作区根目录的 shell 调用仍应回退到宿主进程工作目录，以便非项目命令继续可用。
#[test]
fn shell_command_working_directory_falls_back_without_workspace_root() {
    let resolved = resolve_shell_command_working_directory(None, None)
        .expect("missing workspace root should fall back");
    assert_eq!(
        resolved,
        std::env::current_dir().expect("current directory should resolve")
    );
}

/// Relative shell-command working directories should fall back to the host process working directory when no workspace root is attached.
/// 当没有附着工作区根目录时，相对 shell 命令工作目录应回退到宿主进程工作目录。
#[test]
fn shell_command_working_directory_resolves_relative_cwd_without_workspace_root() {
    let resolved = resolve_shell_command_working_directory(None, Some("subdir/bin"))
        .expect("relative cwd without workspace root should resolve");
    assert_eq!(
        resolved,
        std::env::current_dir()
            .expect("current directory should resolve")
            .join("subdir/bin")
    );
}

/// Absolute shell-command working directories should remain usable even when no workspace root is attached to the turn.
/// 即使当前回合没有附着工作区根目录，绝对 shell 命令工作目录也应继续可用。
#[test]
fn shell_command_working_directory_accepts_absolute_cwd_without_workspace_root() {
    let absolute_cwd = if cfg!(windows) {
        r"D:\workspace\subdir"
    } else {
        "/tmp/workspace/subdir"
    };
    let resolved = resolve_shell_command_working_directory(None, Some(absolute_cwd))
        .expect("absolute cwd should resolve without workspace root");
    assert_eq!(resolved, Path::new(absolute_cwd));
}

/// Unknown shell selectors should fail fast before the host attempts to spawn a process.
/// 未知 shell 选择器应在宿主尝试启动进程前快速失败。
#[test]
fn shell_command_launcher_rejects_unknown_shell() {
    let shell_support = test_shell_support();
    let error = resolve_shell_command_launcher(&shell_support, Some("fish"), "echo hello")
        .expect_err("unsupported shell should fail");
    assert!(error.message.to_ascii_lowercase().contains("support"));
}

/// The dynamic shell schemas should expose `auto` together with only the shells detected on the current host snapshot.
/// 动态 shell schema 应暴露 `auto` 以及当前宿主快照检测到的 shell。
#[test]
fn shell_tool_schemas_expose_dynamic_detected_shells() {
    let shell_support = test_shell_support();
    let command_schema = shell_command_args_schema(&shell_support, &RtkRoutingSupport::disabled());
    let session_schema = terminal_session_args_schema(&shell_support);

    assert_eq!(
        command_schema["properties"]["shell"]["enum"],
        json!(["auto", "pwsh", "powershell", "cmd"])
    );
    assert_eq!(
        session_schema["properties"]["shell"]["enum"],
        json!(["auto", "pwsh", "powershell", "cmd"])
    );
}

/// The AI-facing shell schema text should stay in English and should document critical defaults and paging behavior explicitly.
/// 面向 AI 的 shell schema 文案应保持英文，并明确说明关键默认值与分页行为。
#[test]
fn shell_tool_schema_descriptions_document_defaults_in_english() {
    let shell_support = test_shell_support();
    let command_tool =
        build_shell_command_tool(shell_support.clone(), RtkRoutingSupport::disabled());
    let session_tool = build_terminal_session_tool(
        Arc::new(CommandSessionRuntime::new()),
        shell_support.clone(),
    );
    let command_schema = shell_command_args_schema(&shell_support, &RtkRoutingSupport::disabled());
    let session_schema = terminal_session_args_schema(&shell_support);

    assert!(
        command_tool
            .descriptor
            .description
            .starts_with("Run a one-shot command")
    );
    assert!(
        session_tool
            .descriptor
            .description
            .contains("call skill(\"terminal-session\")")
    );
    assert!(
        command_tool
            .descriptor
            .description
            .contains("no timeout is applied")
    );
    assert!(
        command_tool
            .descriptor
            .description
            .contains("not just a summary")
    );
    assert!(
        command_tool
            .descriptor
            .description
            .contains(".vulcan/temp/")
    );
    assert!(
        session_tool
            .descriptor
            .description
            .contains("until_exit_code")
    );
    assert!(
        command_schema["properties"]["timeout_ms"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("Default: unlimited")
    );
    assert!(
        command_schema["properties"]["cwd"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("falls back to the host process working directory")
    );
    assert!(
        command_schema["properties"]["cwd"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("Relative paths resolve against the same base directory")
    );
    assert!(
        command_schema["properties"]["stdin"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("No newline is added automatically")
    );
    assert!(
        session_schema["properties"]["action"]["enum"]
            .as_array()
            .unwrap_or(&Vec::new())
            .contains(&json!("send"))
    );
    assert!(
        session_schema["properties"]["max_chunks"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("Default: 128")
    );
    assert!(
        session_schema["properties"]["inputs"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("Do not hand-write ANSI")
    );
    assert!(
        session_schema["properties"]["wait_policy"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("until_exit_code")
    );
    assert!(
        session_schema["properties"]["cwd"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("workspace root")
    );
}

/// Disabled RTK support should keep the AI-facing shell-command schema unchanged.
/// 禁用 RTK 支持时，面向 AI 的 shell-command schema 应保持不变。
#[test]
fn shell_command_schema_omits_rtk_disable_flag_when_rtk_is_unavailable() {
    let shell_support = test_shell_support();
    let command_schema = shell_command_args_schema(&shell_support, &RtkRoutingSupport::disabled());

    assert!(
        command_schema["properties"]
            .as_object()
            .expect("schema properties should be an object")
            .get("disabled_rtk")
            .is_none()
    );
}

/// Available RTK support should expose an invocation-level bypass flag to the AI.
/// 可用 RTK 支持应向 AI 暴露单次调用级别的绕过开关。
#[test]
fn shell_command_schema_exposes_rtk_disable_flag_when_rtk_is_available() {
    let shell_support = test_shell_support();
    let rtk_file = tempfile::NamedTempFile::new().expect("test RTK file should be created");
    let rtk_support = RtkRoutingSupport::from_test_parts(
        PersistedRtkToolConfiguration::default(),
        Some(rtk_file.path().to_path_buf()),
    );
    let command_schema = shell_command_args_schema(&shell_support, &rtk_support);

    assert_eq!(
        command_schema["properties"]["disabled_rtk"]["type"],
        json!("boolean")
    );
}

/// RTK route collection should capture each rewritten command route without treating separators as routes.
/// RTK 路由采集应捕获每条重写命令的路由名，同时不把分隔符误判为路由。
#[test]
fn rtk_route_collection_handles_compound_rewritten_commands() {
    let routes = collect_rtk_route_names("rtk git status && rtk cargo test --all");
    let expected = BTreeSet::from([String::from("cargo"), String::from("git")]);

    assert_eq!(routes, expected);
}

/// RTK route collection should ignore raw shell segments left by RTK's native partial rewrite.
/// RTK 路由采集应忽略 RTK 原生局部改写保留下来的原始 shell 片段。
#[test]
fn rtk_route_collection_ignores_unrouted_shell_segments() {
    let routes = collect_rtk_route_names("rtk git status && powershell -NoProfile");
    let expected = BTreeSet::from([String::from("git")]);

    assert_eq!(routes, expected);
}

/// Selected RTK routing should allow only enabled routes and always honor explicit deny entries.
/// 精选 RTK 路由应只允许已启用路由，并始终遵守显式禁用项。
#[test]
fn rtk_route_allowance_honors_selected_and_disabled_routes() {
    let rtk_support = RtkRoutingSupport::from_test_parts(
        PersistedRtkToolConfiguration {
            route_mode: PersistedRtkRouteMode::Selected,
            enabled_routes: BTreeSet::from([String::from("git")]),
            disabled_routes: BTreeSet::from([String::from("npm")]),
            ..PersistedRtkToolConfiguration::default()
        },
        None,
    );

    assert!(rtk_support.allows_routes(&BTreeSet::from([String::from("git")])));
    assert!(!rtk_support.allows_routes(&BTreeSet::from([String::from("npm")])));
    assert!(!rtk_support.allows_routes(&BTreeSet::from([String::from("cargo")])));
}

/// The VMM write tool should expose numeric mappings and reserve profile extraction for the automatic pipeline.
/// VMM 写入工具应暴露数值映射，并将画像提取保留给自动流程。
#[test]
fn vmm_memory_write_tool_documents_numeric_mappings() {
    let snapshot = Arc::new(NativeToolRuntimeSnapshot {
        workspace_directory: String::from(r"D:\workspace"),
        active_model: Some(String::from("openai/gpt-5")),
        agent_count: 0,
        skill_count: 0,
        configured_mcp_server_ids: vec![],
        enabled_mcp_server_ids: vec![],
        vmm_global: VmmConfigLayer::default(),
    });
    let tool = build_vmm_memory_write_tool(snapshot);
    let help_text = tool
        .descriptor
        .help_text
        .as_deref()
        .expect("vmm write tool should expose prompt help");
    let schema = vmm_memory_write_args_schema();
    let item_properties = &schema["properties"]["items"]["items"]["properties"];

    assert!(help_text.contains("0 = general"));
    assert!(help_text.contains("5 = project_context"));
    assert!(help_text.contains("7 = security_policy"));
    assert!(help_text.contains("2 = P1/ordinary durable"));
    assert!(help_text.contains("Do not proactively write user or system profile information"));
    assert!(help_text.contains("Information the user explicitly asks to remember may be written"));
    assert!(!help_text.contains("stable user preferences"));
    assert_eq!(item_properties["category"]["maximum"], json!(7));
    assert_eq!(item_properties["priority"]["maximum"], json!(3));
    assert_eq!(item_properties["memoryLevel"]["maximum"], json!(4));
    assert!(
        item_properties["category"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("1=architecture_decision")
    );
}

/// The start action should default newline submission on when initial stdin is provided so complete commands are safer by default.
/// start 动作在提供初始标准输入时应默认开启换行提交，以便默认情况下更安全地发送完整命令。
#[test]
fn shell_session_start_defaults_append_newline_when_stdin_exists() {
    assert!(effective_start_append_newline(None, Some("echo hi")));
    assert!(!effective_start_append_newline(None, None));
    assert!(!effective_start_append_newline(
        Some(false),
        Some("echo hi")
    ));
    assert!(effective_start_append_newline(Some(true), None));
}

/// The start action should reject newline-less initial stdin unless the caller explicitly submits a newline in-band or via append_newline.
/// start 动作应拒绝未闭合的初始标准输入，除非调用方通过文本本身或 append_newline 显式提交换行。
#[test]
fn shell_session_start_rejects_hidden_stdin_concatenation() {
    let error = validate_shell_session_start_stdin_policy(Some("echo init"), false)
        .expect_err("newline-less initial stdin should be rejected");
    assert!(error.message.contains("rejected initial stdin"));
    validate_shell_session_start_stdin_policy(Some("echo init\n"), false)
        .expect("trailing newline inside stdin should pass");
    validate_shell_session_start_stdin_policy(Some("echo init"), true)
        .expect("append_newline should make the input safe");
}

/// Windows shell sessions should reject `auto` when the command directly enters a nested Unix shell because the outer launcher is not deterministic enough.
/// Windows shell 会话在命令直接进入嵌套 Unix shell 时应拒绝 `auto`，因为外层启动器行为不够确定。
#[test]
fn shell_session_start_rejects_windows_auto_for_nested_unix_shells() {
    let bash_behavior = resolve_session_shell_behavior("bash", "pwsh");
    if cfg!(windows) {
        let error = validate_terminal_session_start_shell_policy(Some("auto"), &bash_behavior)
            .expect_err("windows auto nested bash should be rejected");
        assert!(error.message.contains("requires an explicit `shell`"));
    } else {
        validate_terminal_session_start_shell_policy(Some("auto"), &bash_behavior)
            .expect("non-windows hosts should keep current auto behavior");
    }
    validate_terminal_session_start_shell_policy(Some("bash"), &bash_behavior)
        .expect("explicit bash should remain valid");
}

/// Bash-like shells should always receive LF for automatic line submission, even on Windows hosts.
/// 类 Bash shell 在自动提交输入行时始终应接收 LF，即使宿主是 Windows。
#[test]
fn shell_specific_newline_uses_lf_for_unix_shell_labels() {
    assert_eq!(shell_specific_newline("bash"), "\n");
    assert_eq!(shell_specific_newline("sh"), "\n");
    assert_eq!(shell_specific_newline("zsh"), "\n");
    assert_eq!(
        shell_specific_newline("cmd"),
        if cfg!(windows) { "\r\n" } else { "\n" }
    );
}

/// Directly launching a nested interactive Unix shell should switch newline semantics away from the outer auto-resolved Windows launcher.
/// 直接启动嵌套交互式 Unix shell 时，应把换行语义从外层自动解析的 Windows 启动器切换开。
#[test]
fn session_shell_behavior_detects_nested_bash_from_command_text() {
    let behavior = resolve_session_shell_behavior("bash", "pwsh");

    assert_eq!(behavior.launcher_shell_label, "pwsh");
    assert_eq!(behavior.interactive_shell_label, "bash");
    assert!(behavior.nested_shell_inferred);
}

/// The shell-command tool should stay AI-visible in Plan mode so planning turns can still discover and select host command execution.
/// shell-command 工具应在 Plan 模式下继续对 AI 可见，以便规划回合仍能发现并选择宿主命令执行能力。
#[test]
fn shell_command_tool_stays_visible_in_plan_mode() {
    let tool = build_shell_command_tool(test_shell_support(), RtkRoutingSupport::disabled());
    assert!(tool.descriptor.ai_visible);
    assert!(tool.descriptor.available_in_plan);
}

/// The shell-command tool should execute one simple command and emit the structured `command_run` host result.
/// shell-command 工具应执行一条简单命令并发出结构化的 `command_run` 宿主结果。
#[tokio::test]
async fn shell_command_tool_executes_simple_command() {
    let tool = build_shell_command_tool(
        HostCommandShellSupport::from_configured_default_id(None),
        RtkRoutingSupport::disabled(),
    );
    let result = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "command": "echo hello"
            }),
            session_id: None,
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("shell command should execute");

    let model_output = result
        .model_output
        .as_str()
        .expect("shell command model output should be text");
    assert!(model_output.contains("hello"));
    let host_result = result
        .host_result
        .expect("shell command should expose host metadata");
    assert_eq!(host_result.kind, "command_run");
    assert_eq!(host_result.payload["status"], "completed");
}

/// The per-call RTK bypass flag should run the raw command and record the bypass reason.
/// 单次 RTK 绕过开关应执行原始命令并记录绕过原因。
#[tokio::test]
async fn shell_command_tool_records_disabled_rtk_bypass() {
    let tool = build_shell_command_tool(
        HostCommandShellSupport::from_configured_default_id(None),
        RtkRoutingSupport::disabled(),
    );
    let result = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "command": "echo hello",
                "disabled_rtk": true
            }),
            session_id: None,
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("shell command should execute");

    let host_result = result
        .host_result
        .expect("shell command should expose host metadata");
    assert_eq!(
        host_result.payload["rtk"]["bypass_reason"],
        json!("disabled_by_invocation")
    );
    assert_eq!(host_result.payload["rtk"]["status"], json!("bypassed"));
}

/// The shell-session tool should support a full start-read-write-read-stop round trip and keep the command-session identifier stable across actions.
/// shell-session 工具应支持完整的 start-read-write-read-stop 往返，并在各动作间保持稳定的命令会话标识。
#[tokio::test]
async fn shell_session_tool_round_trips_stdio_and_output_reads() {
    let tool = build_shell_session_tool(
        Arc::new(CommandSessionRuntime::new()),
        HostCommandShellSupport::from_configured_default_id(None),
    );
    let owner_session_id = Uuid::from_u128(1);
    let start_result = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "start",
                "command": interactive_shell_session_test_command(),
                "shell": if cfg!(windows) { "cmd" } else { "auto" }
            }),
            session_id: Some(owner_session_id),
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("shell session start should execute");
    let start_host_result = start_result
        .host_result
        .expect("shell session start should expose host metadata");
    assert_eq!(start_host_result.kind, "command_session");
    assert_eq!(start_host_result.payload["action"], "start");
    let command_session_id = start_host_result.payload["command_session_id"]
        .as_str()
        .expect("command session id should exist")
        .to_string();

    let first_read_result = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "read",
                "command_session_id": command_session_id.clone(),
                "wait_for_output_ms": 300
            }),
            session_id: Some(owner_session_id),
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("shell session read should execute");
    let first_read_output = first_read_result
        .model_output
        .as_str()
        .expect("shell session read should render text");
    assert!(first_read_output.contains("phase-1"));
    let first_read_host_result = first_read_result
        .host_result
        .expect("shell session read should expose host metadata");
    assert!(
        first_read_host_result.payload["returned_chunk_count"]
            .as_u64()
            .unwrap_or_default()
            >= 1,
        "first read payload was: {}",
        first_read_host_result.payload
    );
    let first_last_seq = first_read_host_result.payload["last_seq"]
        .as_u64()
        .expect("first read should expose last sequence");

    let write_result = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "write",
                "command_session_id": command_session_id.clone(),
                "stdin": "hello",
                "append_newline": true
            }),
            session_id: Some(owner_session_id),
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("shell session write should execute");
    let write_host_result = write_result
        .host_result
        .expect("shell session write should expose host metadata");
    assert_eq!(
        write_host_result.payload["bytes_written"],
        json!(if cfg!(windows) { 7 } else { 6 })
    );

    let second_read_result = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "read",
                "command_session_id": command_session_id.clone(),
                "after_seq": first_last_seq,
                "wait_for_output_ms": 1_500
            }),
            session_id: Some(owner_session_id),
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("shell session second read should execute");
    let second_read_host_result = second_read_result
        .host_result
        .expect("shell session second read should expose host metadata");
    assert!(
        second_read_host_result.payload["returned_chunk_count"]
            .as_u64()
            .unwrap_or_default()
            >= 1
    );
    assert!(
        second_read_host_result.payload["last_seq"]
            .as_u64()
            .unwrap_or_default()
            > first_last_seq
    );

    let stop_result = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "stop",
                "command_session_id": command_session_id.clone()
            }),
            session_id: Some(owner_session_id),
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("shell session stop should execute");
    let stop_host_result = stop_result
        .host_result
        .expect("shell session stop should expose host metadata");
    assert_eq!(stop_host_result.payload["action"], "stop");
    assert_eq!(
        stop_host_result.payload["command_session_id"],
        json!(command_session_id)
    );
}

/// The shell-session tool should reject cross-session access and should allow the owner to stop one long-running command session.
/// shell-session 工具应拒绝跨会话访问，并允许拥有者停止一条长时间运行的命令会话。
#[tokio::test]
async fn shell_session_tool_enforces_owner_isolation_and_stop() {
    let tool = build_shell_session_tool(
        Arc::new(CommandSessionRuntime::new()),
        HostCommandShellSupport::from_configured_default_id(None),
    );
    let owner_session_id = Uuid::from_u128(11);
    let foreign_session_id = Uuid::from_u128(22);
    let start_result = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "start",
                "command": long_running_shell_session_test_command()
            }),
            session_id: Some(owner_session_id),
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("long running shell session should start");
    let command_session_id = start_result
        .host_result
        .expect("shell session start should expose host metadata")
        .payload["command_session_id"]
        .as_str()
        .expect("command session id should exist")
        .to_string();

    let foreign_stop_error = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "stop",
                "command_session_id": command_session_id.clone()
            }),
            session_id: Some(foreign_session_id),
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect_err("foreign session should not control another session");
    assert!(
        foreign_stop_error
            .message
            .contains("does not belong to the current controller session")
    );

    let stop_result = tool
        .invoke(NativeToolExecutionRequest {
            arguments: json!({
                "action": "stop",
                "command_session_id": command_session_id.clone()
            }),
            session_id: Some(owner_session_id),
            workspace_root: None,
            controller_runtime: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("owner should stop the command session");
    let stop_host_result = stop_result
        .host_result
        .expect("stop action should expose host metadata");
    assert_eq!(stop_host_result.kind, "command_session");
    assert_eq!(stop_host_result.payload["status"], "stopped");
}
