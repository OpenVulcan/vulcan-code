use super::*;
use crate::tooling::native::{
    NativeToolRuntimeSnapshot, build_vmm_memory_write_tool, vmm_memory_write_args_schema,
};

/// Relative shell-command working directories should resolve below the current workspace root.
/// 相对 shell 命令工作目录应解析到当前工作区根目录之下。
#[test]
fn shell_command_working_directory_joins_relative_path_to_workspace_root() {
    // Use a native absolute root distinct from the process directory on every platform.
    // 在所有平台使用原生绝对路径，且让工作区根目录区别于进程目录。
    let workspace_root = std::env::current_dir()
        .expect("current directory should resolve")
        .join("shell-test-workspace");
    let resolved =
        resolve_shell_command_working_directory(Some(&workspace_root), Some("subdir/bin"))
            .expect("relative cwd should resolve");
    assert_eq!(resolved, workspace_root.join("subdir").join("bin"));
}

/// Omitted shell-command working directories should default to the current workspace root instead of the host process directory.
/// 省略 shell 命令工作目录时，应默认使用当前工作区根目录，而不是宿主进程目录。
#[test]
fn shell_command_working_directory_defaults_to_workspace_root() {
    // A different native absolute root makes an accidental process-directory fallback observable.
    // 使用不同的原生绝对根路径，使错误回退到进程目录能够被测试发现。
    let workspace_root = std::env::current_dir()
        .expect("current directory should resolve")
        .join("shell-test-workspace");
    let resolved = resolve_shell_command_working_directory(Some(&workspace_root), None)
        .expect("workspace-root default should resolve");
    assert_eq!(resolved, workspace_root);
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
    let command_schema = shell_command_args_schema(&shell_support);
    let session_schema = terminal_session_args_schema(&shell_support);
    assert_eq!(
        command_schema["properties"]["shell"]["enum"],
        json!(["auto", "pwsh", "powershell", "cmd"])
    );
    assert_eq!(
        session_schema["properties"]["shell"]["enum"],
        json!(["auto", "pwsh", "powershell", "cmd"])
    );
    // Permission classification and actual launch must see the same frozen automatic shell.
    // 权限分类与实际启动必须看到相同的固定自动 shell。
    for schema in [&command_schema, &session_schema] {
        assert_eq!(
            schema["properties"]["shell"]["default"],
            json!(
                shell_support
                    .resolve_requested_shell(None)
                    .unwrap()
                    .shell
                    .id()
            )
        );
    }
}

/// The AI-facing shell schema text should stay in English and should document critical defaults and paging behavior explicitly.
/// 面向 AI 的 shell schema 文案应保持英文，并明确说明关键默认值与分页行为。
#[test]
fn shell_tool_schema_descriptions_document_defaults_in_english() {
    let shell_support = test_shell_support();
    let command_tool = build_shell_command_tool(shell_support.clone(), None, test_rtk_disabled());
    let session_tool = build_terminal_session_tool(
        Arc::new(CommandSessionRuntime::new()),
        shell_support.clone(),
        Vec::new(),
    );
    let command_schema = shell_command_args_schema(&shell_support);
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
            .contains("exact process exit codes")
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
            .contains("caller-chosen visible completion")
    );
    assert!(
        !session_schema["properties"]["wait_policy"]["properties"]["mode"]["enum"]
            .as_array()
            .unwrap_or(&Vec::new())
            .contains(&json!("until_exit_code"))
    );
    assert!(
        session_schema["properties"]["cwd"]["description"]
            .as_str()
            .unwrap_or_default()
            .contains("workspace root")
    );
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
    let tool = build_shell_command_tool(test_shell_support(), None, test_rtk_disabled());
    assert!(tool.descriptor.ai_visible);
    assert!(tool.descriptor.available_in_plan);
}

/// The shell-command tool should execute one simple command and emit the structured `command_run` host result.
/// shell-command 工具应执行一条简单命令并发出结构化的 `command_run` 宿主结果。
#[tokio::test]
async fn shell_command_tool_executes_simple_command() {
    let tool = build_shell_command_tool(
        HostCommandShellSupport::from_configured_default_id(None),
        None,
        test_rtk_disabled(),
    );
    let result = tool
        .invoke(NativeToolExecutionRequest {
            invocation_context: None,
            arguments: json!({
                "command": "echo hello"
            }),
            session_id: None,
            native_browser_runtime: None,
            workspace_root: None,
            host_project_id: None,
            controller_runtime: None,
            goal_runtime: None,
            environment: None,
            session_mcp: None,
            configuration: None,
            luaskills_debug: None,
            knowledge_access: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
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

/// Oversized shell-command output must still spill to exactly one page file under the spill root.
/// 超出内联预算的 shell-command 输出仍须在超限目录下写入恰好一个分页文件。
#[cfg(unix)]
#[tokio::test]
async fn shell_command_tool_spills_oversized_output_to_page_file() {
    let spill_root = tempfile::tempdir().expect("create shell spill root");
    let tool = build_shell_command_tool(
        HostCommandShellSupport::from_configured_default_id(None),
        None,
        test_rtk_disabled(),
    );
    let result = tool
        .invoke(NativeToolExecutionRequest {
            invocation_context: None,
            arguments: json!({
                "command": "yes 0123456789abcdefghij | head -n 20000"
            }),
            session_id: None,
            native_browser_runtime: None,
            workspace_root: None,
            host_project_id: None,
            controller_runtime: None,
            goal_runtime: None,
            environment: None,
            session_mcp: None,
            configuration: None,
            luaskills_debug: None,
            knowledge_access: None,
            spill_root: Some(spill_root.path().to_path_buf()),
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("oversized shell command should execute");

    let model_output = result
        .model_output
        .as_str()
        .expect("shell command model output should be text");
    let spilled = std::fs::read_dir(spill_root.path())
        .expect("spill root should be readable")
        .collect::<Result<Vec<_>, _>>()
        .expect("spill entries should be readable");
    assert_eq!(
        spilled.len(),
        1,
        "oversized output must spill one page file"
    );
    let file_name = spilled[0].file_name().to_string_lossy().into_owned();
    assert!(model_output.contains(&file_name));
    assert!(
        (model_output.len() as u64) < crate::tool_result_format::DEFAULT_INLINE_TOOL_RESULT_BYTES
    );
}
