//! Chain steps run through the bridge: raw streams survive spilled output and stay redacted.
//! 经由桥运行的链条步骤：原始流在输出溢出后仍可用，并保持脱敏。

use super::*;
use serde_json::Value;
use vulcan_tools_manager::VulcanToolRawStreams;

/// Host credential registered with the redaction filter.
/// 注册到脱敏过滤器的宿主凭据。
const SECRET: &str = "w09-chain-token-1234567890abcdef";

/// Builds one tool candidate with a native identity.
/// 构建一个具有原生身份的工具候选。
fn candidate(
    id: &str,
    invoke: vulcan_tools_manager::VulcanToolInvokeHandler,
) -> VulcanToolCandidate {
    candidate_from(VulcanToolSourceKind::Native, id, invoke)
}

/// Builds one tool candidate from `source`.
/// 构建一个来自 `source` 的工具候选。
fn candidate_from(
    source: VulcanToolSourceKind,
    id: &str,
    invoke: vulcan_tools_manager::VulcanToolInvokeHandler,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            None,
            VulcanToolOrigin::new(source, "test", id.to_string()),
        ),
        parameters_schema: json!({ "type": "object" }),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke,
    }
}

/// A command whose stdout is larger than the inline budget and contains the credential is read by
/// a later step through `stdin`; that step receives the full stream, redacted.
/// 一条 stdout 超过内联预算且含凭据的命令由后续步骤经 `stdin` 读取；该步骤收到完整且已脱敏的流。
#[test]
fn chain_steps_read_full_streams_that_are_already_redacted() {
    let received = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = Arc::clone(&received);
    let shell = candidate(
        "shell_command",
        Arc::new(move |args, _context| {
            let seen = Arc::clone(&seen);
            Box::pin(async move {
                let (model_output, stdout) = match args.get("stdin").and_then(Value::as_str) {
                    Some(stdin) => {
                        if let Ok(mut seen) = seen.lock() {
                            seen.push(stdin.to_string());
                        }
                        (String::from("consumed"), String::new())
                    }
                    None => (
                        String::from("Output was written to .vulcan/temp/page-1.txt"),
                        format!("{}\nkey={SECRET}\n", "x".repeat(300_000)),
                    ),
                };
                Ok(
                    VulcanToolInvocationResult::from_model_output(Value::String(model_output))
                        .with_host_result(VulcanToolHostResult {
                            kind: String::from("command_run"),
                            payload: json!({ "status": "completed", "exit_code": 0 }),
                        })
                        .with_raw_streams(VulcanToolRawStreams {
                            stdout,
                            stderr: String::new(),
                        }),
                )
            })
        }),
    );
    let chain = candidate(
        vulcan_tool_chain::TOOL_CHAIN_TOOL_ID,
        Arc::new(|_args, _context| {
            Box::pin(async { Err(String::from("the bridge runs tool_chain itself")) })
        }),
    );
    let manager = VulcanToolsManager::new(vec![shell, chain]);
    let definitions = manager.resolve_for_execution_mode("Build").definitions;
    let runtime = TaskRuntime::new();
    runtime
        .register_sensitive_tool_output_value(String::from(SECRET))
        .expect("long host token should register");
    let bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build")
            .with_sensitive_tool_output_values(runtime.sensitive_tool_output_values());
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-chain"),
        tool_call_id: String::from("chain-1"),
        tool_name: String::from(vulcan_tool_chain::TOOL_CHAIN_TOOL_ID),
        arguments: json!({ "steps": [
            { "id": "big", "tool": "shell_command", "arguments": { "command": "produce" } },
            { "tool": "shell_command", "arguments": {
                "command": "consume",
                "stdin": "${{ steps.big.stdout }}"
            } }
        ] }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    install_test_invocation_identity(&bridge, &request, 201, 202, 203);
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build")
        .block_on(bridge.execute_tool(&request))
        .expect("chain should execute");

    let received = received.lock().map(|seen| seen.clone()).unwrap_or_default();
    assert_eq!(received.len(), 1, "the consuming step must run once");
    assert!(
        received[0].starts_with(&"x".repeat(300_000)),
        "the full stream must arrive"
    );
    assert!(
        received[0].ends_with("key=[redacted]"),
        "{}",
        &received[0][300_000..]
    );
    assert!(!result.model_output.contains(SECRET));
    assert!(
        !result
            .host_result
            .map(|host| host.to_string())
            .unwrap_or_default()
            .contains(SECRET)
    );
    assert!(result.model_output.starts_with("tool_chain SUCCEEDED"));
}

/// Only the listed native tools can be steps: MCP, LuaSkills and unlisted native tools reject the
/// whole plan before any step runs.
/// 只有列出的原生工具可以作为步骤：MCP、LuaSkills 与未列出的原生工具会在任何步骤运行前拒绝整个计划。
#[test]
fn chains_reject_every_tool_outside_the_native_list() {
    let calls = Arc::new(Mutex::new(0usize));
    let counting = |source: VulcanToolSourceKind, id: &str| {
        let calls = Arc::clone(&calls);
        candidate_from(
            source,
            id,
            Arc::new(move |_args, _context| {
                let calls = Arc::clone(&calls);
                Box::pin(async move {
                    if let Ok(mut calls) = calls.lock() {
                        *calls += 1;
                    }
                    Ok(VulcanToolInvocationResult::from_model_output(json!("ran")))
                })
            }),
        )
    };
    let manager = VulcanToolsManager::new(vec![
        counting(VulcanToolSourceKind::Native, "file_read"),
        counting(VulcanToolSourceKind::Mcp, "remote_search"),
        counting(VulcanToolSourceKind::LuaSkills, "skill_lookup"),
        counting(VulcanToolSourceKind::Native, "todolist"),
        candidate(
            vulcan_tool_chain::TOOL_CHAIN_TOOL_ID,
            Arc::new(|_args, _context| {
                Box::pin(async { Err(String::from("the bridge runs tool_chain itself")) })
            }),
        ),
    ]);
    let definitions = manager.resolve_for_execution_mode("Build").definitions;
    let bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-chain"),
        tool_call_id: String::from("chain-2"),
        tool_name: String::from(vulcan_tool_chain::TOOL_CHAIN_TOOL_ID),
        arguments: json!({ "steps": [
            { "tool": "file_read", "arguments": { "path": "a.txt" } },
            { "tool": "remote_search", "arguments": {} },
            { "tool": "skill_lookup", "arguments": {} },
            { "tool": "todolist", "arguments": {} }
        ] }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    install_test_invocation_identity(&bridge, &request, 211, 212, 213);
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build")
        .block_on(bridge.execute_tool(&request))
        .expect("chain should return a result");

    assert!(result.is_error, "{}", result.model_output);
    for tool in ["remote_search", "skill_lookup", "todolist"] {
        assert!(
            result.model_output.contains(&format!("`{tool}`")),
            "{tool} must be named in: {}",
            result.model_output
        );
    }
    assert_eq!(calls.lock().map(|calls| *calls).unwrap_or_default(), 0);
}

/// A step that Plan mode blocks is a refused step: the chain stops and later steps never run.
/// 计划模式拦截的步骤属于被拒步骤：链停止，后续步骤不会运行。
#[test]
fn plan_blocked_step_stops_the_chain() {
    let calls = Arc::new(Mutex::new(0usize));
    let counting = |id: &str, plan_policy: VulcanPlanToolPolicy| {
        let calls = Arc::clone(&calls);
        let mut tool = candidate(
            id,
            Arc::new(move |_args, _context| {
                let calls = Arc::clone(&calls);
                Box::pin(async move {
                    if let Ok(mut calls) = calls.lock() {
                        *calls += 1;
                    }
                    Ok(VulcanToolInvocationResult::from_model_output(json!("ran")))
                })
            }),
        );
        tool.plan_policy = plan_policy;
        tool
    };
    let manager = VulcanToolsManager::new(vec![
        counting(
            "shell_command",
            VulcanPlanToolPolicy::HeuristicStringArgument {
                field: String::from("command"),
            },
        ),
        counting("file_read", VulcanPlanToolPolicy::Visible),
        candidate(
            vulcan_tool_chain::TOOL_CHAIN_TOOL_ID,
            Arc::new(|_args, _context| {
                Box::pin(async { Err(String::from("the bridge runs tool_chain itself")) })
            }),
        ),
    ]);
    let definitions = manager.resolve_for_execution_mode("Plan").definitions;
    let bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Plan");
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-chain"),
        tool_call_id: String::from("chain-3"),
        tool_name: String::from(vulcan_tool_chain::TOOL_CHAIN_TOOL_ID),
        arguments: json!({ "steps": [
            { "tool": "shell_command", "arguments": { "command": "write" } },
            { "tool": "file_read", "arguments": { "path": "a.txt" } }
        ] }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    install_test_invocation_identity(&bridge, &request, 221, 222, 223);
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build")
        .block_on(bridge.execute_tool(&request))
        .expect("chain should return a result");

    assert!(
        result
            .model_output
            .contains(vulcan_tools_manager::PLAN_MODE_BLOCKED_MESSAGE),
        "{}",
        result.model_output
    );
    assert_eq!(calls.lock().map(|calls| *calls).unwrap_or_default(), 0);
}
