//! Isolated public native-tool dispatch against real Core storage and synthetic Host AI.
//! 针对真实 Core 存储及合成宿主 AI 的隔离公开原生工具派发。

use super::*;
use vulcan_agent_runtime::{
    agent_runtime::{
        config::AgentRuntimeConfig, question_runtime::QuestionRuntime, task_runtime::TaskRuntime,
    },
    tooling::{CommandSessionRuntime, NativeToolCatalog, build_native_tool_catalog},
    vulcan_tools_bridge::build_vulcan_tools_manager,
};
use vulcan_session_core::v1::ids::{ApiCallId, RequestStreamId, Seq, ToolCallId};
use vulcan_tools_manager::{
    ResolvedToolDefinition, VulcanToolInvocationContext, VulcanToolsManager,
};
use vulcan_vmm::{VmmBackendMode, VmmConfigLayer};

/// Returns the Markdown receipt a native VMM tool delivers to the model, naming `tool` on mismatch.
/// 返回原生 VMM 工具交付给模型的 Markdown 回执，不匹配时在错误中注明 `tool`。
fn markdown_output<'a>(output: &'a serde_json::Value, tool: &str) -> &'a str {
    output
        .as_str()
        .unwrap_or_else(|| panic!("{tool} output should be a Markdown string: {output}"))
}

/// Returns the exact registered memory-tool family used by production exposure filtering.
/// 返回生产暴露过滤所用的准确已注册记忆工具族。
fn memory_tool_ids() -> Vec<String> {
    [
        "vmm_memory_search",
        "vmm_turn_details",
        "vmm_memory_write",
        "vmm_memory_delete",
    ]
    .map(String::from)
    .to_vec()
}

/// Builds the actual manager with explicit project ownership, global catalog and runtime/policy gates.
/// 使用明确项目归属、全局目录及运行时/策略闸门构建实际管理器。
fn manager(
    catalog: NativeToolCatalog,
    ready: bool,
    project_policy: VmmConfigLayer,
) -> VulcanToolsManager {
    build_vulcan_tools_manager(
        Arc::new(TaskRuntime::new()),
        Arc::new(QuestionRuntime::new()),
        catalog,
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        Some("nonempty-host".to_owned()),
        None,
        None,
        None,
        memory_tool_ids(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        Vec::new(),
        project_policy,
        VmmConfigLayer::default(),
        ready,
        None,
        None,
    )
}

/// Builds one explicitly synthetic but fully typed invocation identity consumed by the real manager bridge.
/// 构建一个明确合成但类型完整的调用身份，由实际管理器桥接消费。
fn invocation(session_id: uuid::Uuid, sequence: u64) -> VulcanToolInvocationContext {
    VulcanToolInvocationContext {
        session_id: session_id.to_string(),
        request_stream_id: RequestStreamId::try_new(format!("native-tool-stream-{sequence}"))
            .expect("stream identity"),
        api_call_id: ApiCallId::try_new(1).expect("API identity"),
        tool_call_id: ToolCallId::try_new(sequence).expect("tool identity"),
        tool_seq: Seq::try_new(sequence).expect("sequence"),
        controller_tool_call_id: format!("native-tool-{sequence}"),
        provider_tool_call_id: None,
    }
}

/// Resolves one exact AI-visible definition and rejects missing or duplicate registration.
/// 解析一个准确的 AI 可见定义，并拒绝缺失或重复注册。
fn definition(manager: &VulcanToolsManager, id: &str) -> ResolvedToolDefinition {
    // definitions are the actual public schema and invocation closures, not a copied test descriptor.
    // definitions 是实际公开 Schema 与调用闭包，而非复制的测试描述。
    let mut definitions = manager
        .resolve_for_execution_mode("Build")
        .definitions
        .into_iter()
        .filter(|definition| definition.descriptor.id == id)
        .collect::<Vec<_>>();
    assert_eq!(definitions.len(), 1, "exact public definition for {id}");
    definitions.remove(0)
}

/// Executes all four real native definitions, validates physical effects and proves closed gates perform no writes.
/// 执行全部四个真实原生定义，验证物理效果，并证明关闭的闸门不会写入。
/// The caller must be the dedicated child process; this function intentionally retains its client until exit.
/// 调用方必须是专有子进程；本函数刻意将客户端保留到进程退出。
pub(super) async fn verify(
    client: Arc<NativeVmmClient>,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    project_id: u64,
    extracted: &vulcan_vmm::MemorySearchHit,
) {
    assert_eq!(
        std::env::var("VULCAN_NATIVE_TOOL_DISPATCH_CHILD").expect("isolated child marker"),
        "native_vmm::driver::lexical_driver_tests::real_controller_native_tool_dispatch_in_isolated_process"
    );
    assert!(
        vulcan_vmm::process_native_vmm_client().is_err(),
        "registry must be empty before child installation"
    );
    vulcan_vmm::install_process_native_vmm_client(client).expect("install private native client");
    // config contains no external server or model; only the native memory catalog is activated here.
    // config 不含外置服务器或模型；此处仅激活原生记忆目录。
    let config = AgentRuntimeConfig {
        active_model: None,
        active_agent_id: "native-tool-acceptance".to_owned(),
        agents: Vec::new(),
        skills: Vec::new(),
        tool_policies: Default::default(),
        configured_mcp_server_ids: Vec::new(),
        enabled_mcp_server_ids: Vec::new(),
    };
    // global freezes actual native ownership and the same project as the extracted source records.
    // global 冻结实际原生归属，以及与提炼来源记录相同的项目。
    let global = VmmConfigLayer {
        enabled: Some(true),
        backend_mode: Some(VmmBackendMode::Native),
        user_id: Some(1),
        project_id: Some(project_id),
        ai_tools_enabled: Some(true),
        ..Default::default()
    };
    let catalog = build_native_tool_catalog(
        &config,
        None,
        Arc::new(CommandSessionRuntime::new()),
        global,
        None,
        None,
    );
    for id in memory_tool_ids() {
        let registered = catalog
            .definition(&id)
            .expect("registered NativeToolDefinition");
        assert!(registered.descriptor.ai_visible);
        assert_eq!(registered.args_schema["type"], "object");
    }
    // closed managers use the same registered catalog; none may expose a memory invocation closure.
    // 关闭的管理器使用同一注册目录；任何一个都不得暴露记忆调用闭包。
    for closed in [
        manager(catalog.clone(), false, VmmConfigLayer::default()),
        manager(
            catalog.clone(),
            true,
            VmmConfigLayer {
                enabled: Some(false),
                ..Default::default()
            },
        ),
        manager(
            catalog.clone(),
            true,
            VmmConfigLayer {
                ai_tools_enabled: Some(false),
                ..Default::default()
            },
        ),
    ] {
        assert!(
            closed
                .resolve_for_execution_mode("Build")
                .definitions
                .iter()
                .all(|entry| !memory_tool_ids().contains(&entry.descriptor.id))
        );
    }
    // active emits the exact public definitions that dispatch through NativeToolCatalog and the process client.
    // active 发出准确公开定义，并通过 NativeToolCatalog 与进程客户端派发。
    let active = manager(catalog, true, VmmConfigLayer::default());
    let session_id = uuid::Uuid::new_v4();
    let search_tool = definition(&active, "vmm_memory_search");
    let search = active
        .invoke_tool(
            &search_tool,
            serde_json::json!({"queries": ["PostgreSQL"], "top_k": 4}),
            invocation(session_id, 1),
            "Build",
        )
        .await
        .expect("real native search dispatch")
        .model_output;
    // Model-visible results are Markdown receipts whose layout may evolve; assert only that the
    // physically extracted memory's identity and facts reach the model.
    // 模型可见结果是版式可能演进的 Markdown 回执；仅断言物理提取记忆的身份与事实送达模型。
    let search = markdown_output(&search, "search");
    assert!(search.contains(&extracted.memory_id.to_string()));
    assert!(search.contains(&extracted.abstract_text));
    assert!(search.contains(&extracted.source_turn_id.to_string()));
    // details follow the exact source turn returned by actual search, preserving the original conversation.
    // details 跟随实际搜索返回的准确来源回合，保留原始对话。
    let details = active
        .invoke_tool(
            &definition(&active, "vmm_turn_details"),
            serde_json::json!({"turnIds": [extracted.source_turn_id]}),
            invocation(session_id, 2),
            "Plan",
        )
        .await
        .expect("real native details dispatch")
        .model_output;
    let details = markdown_output(&details, "turn details");
    assert!(details.contains(&extracted.source_turn_id.to_string()));
    assert!(details.contains("PostgreSQL"), "original user question");
    // write_args use only the advertised schema, with an explicit durable project scope and synthetic content.
    // write_args 仅使用已声明 Schema，并带有明确持久项目作用域及合成内容。
    let write_args = serde_json::json!({"items": [{"abstract": "Native tool dispatch acceptance", "details": "Synthetic project decision for isolated native tool dispatch.", "category": 1, "scopeLevel": 2, "priority": 2, "memoryLevel": 2}]});
    let write_tool = definition(&active, "vmm_memory_write");
    let before = maintenance
        .memory_records(project_id, None)
        .await
        .expect("physical before Plan guard");
    // The Plan guard is proven by unchanged storage, not by its refusal wording.
    // Plan 闸门由存储未变证明，而非其拒绝文案。
    active
        .invoke_tool(
            &write_tool,
            write_args.clone(),
            invocation(session_id, 3),
            "Plan",
        )
        .await
        .expect("Plan execution guard");
    assert_eq!(
        maintenance
            .memory_records(project_id, None)
            .await
            .expect("physical after Plan guard")
            .memories
            .len(),
        before.memories.len()
    );
    let written = active
        .invoke_tool(&write_tool, write_args, invocation(session_id, 4), "Build")
        .await
        .expect("real native write dispatch")
        .model_output;
    let written = markdown_output(&written, "write");
    // Storage, not receipt parsing, identifies the one committed memory.
    // 由存储而非回执解析识别唯一已提交记忆。
    let committed = maintenance
        .memory_records(project_id, None)
        .await
        .expect("physical committed direct memory");
    let new_memories = committed
        .memories
        .iter()
        .filter(|memory| {
            before
                .memories
                .iter()
                .all(|existing| existing.memory_id != memory.memory_id)
        })
        .collect::<Vec<_>>();
    assert_eq!(new_memories.len(), 1, "exactly one committed memory");
    let direct = new_memories[0];
    let memory_id = direct.memory_id;
    assert!(
        written.contains(&memory_id.to_string()),
        "receipt names {memory_id}"
    );
    assert_eq!(direct.summary, "Native tool dispatch acceptance");
    assert_eq!(
        direct.session_source_id.as_deref(),
        Some(session_id.to_string().as_str())
    );
    // deletion consumes only the identity returned by the actual write, then verifies physical removal.
    // deletion 仅消费实际写入返回的身份，然后验证物理删除。
    let deleted = active.invoke_tool(&definition(&active, "vmm_memory_delete"), serde_json::json!({"memoryIds": [memory_id], "reason": "Remove isolated acceptance fixture."}), invocation(session_id, 5), "Build").await.expect("real native delete dispatch").model_output;
    let deleted = markdown_output(&deleted, "deletion");
    assert!(
        deleted.contains(&memory_id.to_string()),
        "receipt names {memory_id}"
    );
    assert!(
        maintenance
            .memory_records(project_id, None)
            .await
            .expect("physical after deletion")
            .memories
            .iter()
            .all(|memory| memory.memory_id != memory_id)
    );
    // A malformed host session fails explicitly before another native read can be attributed.
    // 格式错误的宿主会话会显式失败，不能为另一次原生读取赋予错误归属。
    let mut invalid_context = invocation(session_id, 6);
    invalid_context.session_id = "invalid-session-identity".to_owned();
    let error = active
        .invoke_tool(
            &search_tool,
            serde_json::json!({"queries": ["PostgreSQL"]}),
            invalid_context,
            "Build",
        )
        .await
        .expect_err("native tools require a real host session");
    assert!(error.contains("session"));
}
