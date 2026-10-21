//! Code-mode tool surface, output and host round-trip tests.
//! 代码模式工具面、输出与宿主往返测试。

use super::output::render_runtime_response;
use super::*;
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;
use vulcan_code_mode::FunctionCallOutputContentItem;
use vulcan_core::tool::VulcanToolExecutor;
use vulcan_session_core::v1::document::PreparedToolReceipt;
use vulcan_session_core::v1::ids::{ApiCallId, RequestStreamId, Seq, ToolCallId, TurnId};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolOrigin,
    VulcanToolSourceKind, VulcanToolsManager,
};

/// Builds one tools manager whose `echo` tool returns its arguments and whose `secret` tool is
/// rejected by the run.
/// 构建一个工具管理器：`echo` 工具返回其参数，`secret` 工具被本次运行拒绝。
fn demo_tools() -> (VulcanToolsManager, Vec<ResolvedToolDefinition>) {
    let candidate = |id: &str| VulcanToolCandidate {
        descriptor: VulcanToolDescriptor::new(
            id.to_owned(),
            id.to_owned(),
            format!("Demo tool {id}"),
            None,
            VulcanToolOrigin::new(VulcanToolSourceKind::Native, "test", id.to_owned()),
        ),
        parameters_schema: json!({
            "type": "object",
            "properties": { "value": { "type": "string" } },
        }),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(|args, context| {
            Box::pin(async move {
                Ok(VulcanToolInvocationResult::from_model_output(json!({
                    "echo": args,
                    "controller_tool_call_id": context.controller_tool_call_id,
                })))
            })
        }),
    };
    let manager = VulcanToolsManager::new(vec![
        candidate("echo"),
        candidate("secret"),
        candidate(WAIT_TOOL_NAME),
    ]);
    let resolved = manager.resolve_for_execution_mode("Build").definitions;
    (manager, resolved)
}

/// Builds a code-mode surface whose host is never started.
/// 构建一个从不启动宿主的代码模式工具面。
fn offline_turn(tool_mode: ToolMode, resolved: &[ResolvedToolDefinition]) -> CodeModeTurn {
    let activation = CodeModeActivation {
        tool_mode,
        slot: Arc::new(CodeModeSessionSlot {
            provider: Arc::new(ProcessOwnedCodeModeSessionProvider::with_host_program(
                PathBuf::from("missing-code-mode-host"),
            )),
            session: tokio::sync::OnceCell::new(),
        }),
    };
    CodeModeTurn::new(activation, resolved, &[String::from("secret")])
}

/// Returns the names of the provider-visible tools.
/// 返回 Provider 可见工具的名称。
fn names(tools: &[ProviderToolDefinition]) -> Vec<&str> {
    tools.iter().map(|tool| tool.name.as_str()).collect()
}

/// `code_mode_only` exposes only `exec` and `wait`, and JavaScript never sees rejected or
/// reserved tools.
/// `code_mode_only` 只暴露 `exec` 与 `wait`，JavaScript 永远看不到被拒绝或保留名称的工具。
#[test]
fn code_mode_only_exposes_only_exec_and_wait() {
    let (_, resolved) = demo_tools();
    let turn = offline_turn(ToolMode::CodeModeOnly, &resolved);
    let direct = resolved
        .iter()
        .map(crate::agent_runtime::provider_protocol_tool_definition::resolved_tool_definition_to_provider_tool_definition)
        .collect();

    let tools = turn.provider_tool_definitions(direct);

    assert_eq!(names(&tools), [PUBLIC_TOOL_NAME, WAIT_TOOL_NAME]);
    assert!(
        tools
            .iter()
            .all(|tool| tool.source_kind == CODE_MODE_TOOL_SOURCE_KIND)
    );
    assert_eq!(
        turn.nested_tool_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["echo"]
    );
    assert!(turn.is_code_mode_only());
}

/// `code_mode` keeps direct tools beside `exec` and `wait` but drops a tool that collides with them.
/// `code_mode` 在 `exec` 与 `wait` 旁保留直接工具，但去掉与其重名的工具。
#[test]
fn code_mode_keeps_direct_tools_without_name_collisions() {
    let (_, resolved) = demo_tools();
    let turn = offline_turn(ToolMode::CodeMode, &resolved);
    let direct = resolved
        .iter()
        .map(crate::agent_runtime::provider_protocol_tool_definition::resolved_tool_definition_to_provider_tool_definition)
        .collect();

    let tools = turn.provider_tool_definitions(direct);

    let names = names(&tools);
    assert_eq!(&names[..2], [PUBLIC_TOOL_NAME, WAIT_TOOL_NAME]);
    assert!(names.contains(&"echo"));
    assert_eq!(
        names.iter().filter(|name| **name == WAIT_TOOL_NAME).count(),
        1
    );
    assert!(!turn.is_code_mode_only());
}

/// Only the two Responses profiles receive `exec` as freeform JavaScript.
/// 只有两个 Responses Profile 以自由文本 JavaScript 接收 `exec`。
#[test]
fn only_responses_profiles_accept_freeform_exec() {
    assert!(protocol_accepts_freeform_exec(
        vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID
    ));
    assert!(protocol_accepts_freeform_exec(
        vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID
    ));
    assert!(!protocol_accepts_freeform_exec(
        vulcan_llm_core::OPENAI_CHAT_PROFILE_ID
    ));
}

/// Output starts with the status header and keeps oversized text within the token budget.
/// 输出以状态标题开头，并将超长文本限制在 Token 预算内。
#[test]
fn runtime_output_reports_status_and_truncates_to_budget() {
    let yielded = render_runtime_response(
        RuntimeResponse::Yielded {
            cell_id: CellId::new(String::from("7")),
            content_items: vec![FunctionCallOutputContentItem::InputText {
                text: String::from("partial"),
            }],
            code_mode_host_duration: None,
        },
        None,
        Duration::from_millis(1200),
    );
    let yielded = yielded.model_output.as_str().expect("text output");
    assert!(yielded.starts_with("Script running with cell ID 7\n"));
    assert!(yielded.ends_with("Output:\npartial"));

    let failed = render_runtime_response(
        RuntimeResponse::Result {
            cell_id: CellId::new(String::from("8")),
            content_items: vec![FunctionCallOutputContentItem::InputText {
                text: "x".repeat(10_000),
            }],
            error_text: Some(String::from("boom")),
            result_buffered_for: Duration::ZERO,
            code_mode_host_duration: None,
        },
        Some(100),
        Duration::ZERO,
    );
    let failed = failed.model_output.as_str().expect("text output");
    assert!(failed.starts_with("Script failed\n"));
    assert!(failed.contains("truncated"));
    assert!(failed.ends_with("Script error:\nboom"));
    assert!(failed.len() < 1_000);
}

/// Builds one `exec` request with its durable identity installed on the bridge.
/// 构建一个 `exec` 请求，并在桥接层上安装其可靠身份。
fn code_mode_request(
    bridge: &VulcanManagedToolRuntimeBridge,
    tool_name: &str,
    arguments: Value,
    sequence: u64,
) -> CoreToolInvocationRequest {
    let request = CoreToolInvocationRequest {
        request_stream_id: format!("stream-{sequence}"),
        tool_call_id: format!("call-{sequence}"),
        tool_name: tool_name.to_owned(),
        arguments,
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    let receipt = PreparedToolReceipt {
        turn_id: TurnId::try_new(1).expect("turn id"),
        api_call_id: ApiCallId::try_new(sequence).expect("api call id"),
        tool_call_id: ToolCallId::try_new(sequence).expect("tool call id"),
        request_stream_id: RequestStreamId::try_new(request.request_stream_id.clone())
            .expect("stream id"),
        tool_seq: Seq::try_new(sequence).expect("tool seq"),
    };
    bridge
        .install_prepared_invocation_identities(std::slice::from_ref(&request), vec![receipt])
        .expect("identity installs");
    request
}

/// JavaScript calls a nested tool through the bridge, keeps values across cells, cannot reach a
/// rejected tool, and direct calls are refused in `code_mode_only`.
/// JavaScript 经桥接层调用嵌套工具、跨单元保留值、无法调用被拒绝的工具，且 `code_mode_only`
/// 拒绝直接调用。
#[test]
fn exec_round_trips_nested_tools_through_the_host() {
    if host_provider().is_err() {
        // The host is built by `node scripts/code-mode-host-build.mjs debug`.
        // 宿主由 `node scripts/code-mode-host-build.mjs debug` 构建。
        return;
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime builds");
    runtime.block_on(async {
        let session_id = Uuid::new_v4();
        let activation = resolve_code_mode_activation(ToolMode::CodeModeOnly, session_id)
            .expect("host resolves")
            .expect("code mode is active");
        let (manager, resolved) = demo_tools();
        let turn = Arc::new(CodeModeTurn::new(
            activation,
            &resolved,
            &[String::from("secret")],
        ));
        let bridge =
            VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &resolved, session_id, "Build")
                .with_code_mode(Some(Arc::clone(&turn)));

        let first = code_mode_request(
            &bridge,
            PUBLIC_TOOL_NAME,
            json!({ "code": "const result = await tools.echo({ value: \"hi\" });\nstore(\"k\", 41);\ntext(JSON.stringify(result));" }),
            1,
        );
        let output = bridge.execute_tool(&first).await.expect("exec runs");
        assert!(output.model_output.contains("Script completed"), "{}", output.model_output);
        assert!(output.model_output.contains("\"value\":\"hi\""), "{}", output.model_output);
        assert!(output.model_output.contains("call-1/"), "{}", output.model_output);

        let second = code_mode_request(
            &bridge,
            PUBLIC_TOOL_NAME,
            json!({ "code": "text(String(load(\"k\") + 1));\nawait tools.secret({});" }),
            2,
        );
        let output = bridge.execute_tool(&second).await.expect("exec runs");
        assert!(output.model_output.contains("42"), "{}", output.model_output);
        assert!(output.model_output.contains("Script failed"), "{}", output.model_output);

        let direct = code_mode_request(&bridge, "echo", json!({ "value": "x" }), 3);
        let output = bridge.execute_tool(&direct).await.expect("call is answered");
        assert!(output.model_output.contains("only available inside `exec`"));

        drop(bridge);
        drop(turn);
        release_code_mode_session(session_id);
    });
}
