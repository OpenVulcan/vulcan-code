use super::*;
use crate::tooling::native::NativeToolRuntimeSnapshot;
use crate::tooling::native::{goal_created_response_value, goal_status_allows_terminal_update};
use std::sync::Mutex;

/// Deterministic in-memory V1 Goal port used to verify the native tool boundary.
/// 用于验证 native 工具边界的确定性内存 V1 Goal 端口。
#[derive(Debug, Default)]
struct NativeGoalRuntimeFixture {
    /// Current authoritative Goal snapshot for the fixture session.
    /// 夹具会话当前的权威 Goal 快照。
    goal: Mutex<Option<NativeGoalSnapshot>>,
}

impl NativeGoalRuntimeFixture {
    /// Builds one native-tool request bound to this exact fixture and session.
    /// 构建一条绑定到当前精确夹具与会话的 native 工具请求。
    fn request(
        self: &Arc<Self>,
        session_id: SessionId,
        arguments: serde_json::Value,
    ) -> NativeToolExecutionRequest {
        NativeToolExecutionRequest {
            session_id: Some(session_id),
            goal_runtime: Some(self.clone()),
            ..test_native_request(arguments)
        }
    }
}

#[async_trait::async_trait]
impl NativeGoalRuntime for NativeGoalRuntimeFixture {
    /// Reads the exact current fixture Goal.
    /// 读取夹具当前的精确 Goal。
    async fn get_goal(&self, session_id: SessionId) -> Result<Option<NativeGoalSnapshot>, String> {
        Ok(self
            .goal
            .lock()
            .map_err(|error| error.to_string())?
            .clone()
            .filter(|goal| goal.session_id == session_id))
    }

    /// Creates the first active Goal for the exact fixture session.
    /// 为精确夹具会话创建首个活动 Goal。
    async fn create_goal(
        &self,
        session_id: SessionId,
        objective: String,
        token_budget: Option<u64>,
    ) -> Result<NativeGoalSnapshot, String> {
        let mut guard = self.goal.lock().map_err(|error| error.to_string())?;
        if guard.is_some() {
            return Err(String::from("fixture Goal already exists"));
        }
        let token_budget = token_budget
            .map(i64::try_from)
            .transpose()
            .map_err(|error| format!("fixture Goal budget is out of range: {error}"))?;
        let goal = NativeGoalSnapshot {
            session_id,
            goal_id: String::from("v1-goal-1"),
            objective,
            status: NativeGoalStatus::Active,
            token_budget,
            tokens_used: 0,
            uncached_input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_tokens: 0,
            total_tokens: 0,
            time_used_seconds: 0,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        *guard = Some(goal.clone());
        Ok(goal)
    }

    /// Applies one terminal state to the exact fixture Goal.
    /// 向精确夹具 Goal 应用一个终态。
    async fn update_goal_status(
        &self,
        session_id: SessionId,
        status: NativeGoalTerminalStatus,
    ) -> Result<NativeGoalSnapshot, String> {
        let mut guard = self.goal.lock().map_err(|error| error.to_string())?;
        let goal = guard
            .as_mut()
            .filter(|goal| goal.session_id == session_id)
            .ok_or_else(|| String::from("fixture Goal is missing"))?;
        goal.status = match status {
            NativeGoalTerminalStatus::Complete => NativeGoalStatus::Complete,
            NativeGoalTerminalStatus::Blocked => NativeGoalStatus::Blocked,
        };
        goal.updated_at_ms += 1;
        Ok(goal.clone())
    }
}

/// Verifies native get/create/block/complete route only through the injected V1 Goal port.
/// 验证 native get/create/block/complete 只通过注入的 V1 Goal 端口路由。
#[tokio::test]
async fn native_goal_tools_execute_complete_v1_port_lifecycle() {
    let session_id = SessionId::from_u128(0x7301);
    let runtime = Arc::new(NativeGoalRuntimeFixture::default());
    let empty = build_get_goal_tool()
        .invoke(runtime.request(session_id, json!({})))
        .await
        .expect("empty get_goal should execute");
    assert_eq!(empty.model_output["goal"], serde_json::Value::Null);

    let created = build_create_goal_tool()
        .invoke(runtime.request(
            session_id,
            json!({"objective": "finish native cutover", "token_budget": 3000}),
        ))
        .await
        .expect("create_goal should execute through V1 port");
    assert_eq!(created.model_output["goal"]["status"], "active");
    assert_eq!(created.model_output["goal"]["tokenBudget"], 3000);
    let loaded = build_get_goal_tool()
        .invoke(runtime.request(session_id, json!({})))
        .await
        .expect("created get_goal should execute");
    assert_eq!(
        loaded.model_output["goal"]["objective"],
        "finish native cutover"
    );

    let blocked = build_update_goal_tool()
        .invoke(runtime.request(session_id, json!({"status": "blocked"})))
        .await
        .expect("update_goal blocked should execute through V1 port");
    assert_eq!(blocked.model_output["goal"]["status"], "blocked");
    let completed = build_update_goal_tool()
        .invoke(runtime.request(session_id, json!({"status": "complete"})))
        .await
        .expect("update_goal complete should execute through V1 port");
    assert_eq!(completed.model_output["goal"]["status"], "complete");
}

/// Verifies model-owned terminal transitions reject incompatible lifecycle states.
/// 验证模型拥有的终态转换会拒绝不兼容的生命周期状态。
#[test]
fn goal_terminal_update_filters_persisted_statuses() {
    assert!(goal_status_allows_terminal_update(
        NativeGoalStatus::Active,
        NativeGoalTerminalStatus::Complete
    ));
    assert!(!goal_status_allows_terminal_update(
        NativeGoalStatus::Paused,
        NativeGoalTerminalStatus::Complete
    ));
    assert!(goal_status_allows_terminal_update(
        NativeGoalStatus::BudgetLimited,
        NativeGoalTerminalStatus::Blocked
    ));
    assert!(!goal_status_allows_terminal_update(
        NativeGoalStatus::Complete,
        NativeGoalTerminalStatus::Blocked
    ));
}

/// Verifies todolist returns the Codex-compatible model text and a host todolist payload.
/// 验证 todolist 返回与 Codex 兼容的模型文本以及宿主 todolist 载荷。
#[tokio::test]
async fn todolist_tool_returns_todo_host_payload() {
    let tool = build_todolist_tool();
    let result = tool
        .invoke(test_native_request(json!({
            "explanation": "next pass",
            "items": [
                { "step": "Read source", "status": "completed" },
                { "step": "Wire UI", "status": "in_progress" },
                { "step": "Verify", "status": "pending" }
            ]
        })))
        .await
        .expect("todolist should execute");
    let host_result = result.host_result.expect("host result");

    assert_eq!(result.model_output, json!("Todolist updated"));
    assert_eq!(host_result.kind, "todolist");
    assert_eq!(host_result.payload["explanation"], "next pass");
    assert_eq!(host_result.payload["items"][1]["status"], "in_progress");
}

/// Verifies todolist rejects multiple in-progress items.
/// 验证 todolist 会拒绝多个进行中条目。
#[tokio::test]
async fn todolist_tool_rejects_multiple_in_progress_items() {
    let tool = build_todolist_tool();
    let error = tool
        .invoke(test_native_request(json!({
            "items": [
                { "step": "A", "status": "in_progress" },
                { "step": "B", "status": "in_progress" }
            ]
        })))
        .await
        .expect_err("todolist should reject duplicate in-progress items");

    assert!(
        error
            .message
            .contains("At most one todolist item can be in_progress")
    );
}

/// Verifies todolist stays AI-visible but is not allowed to execute in Plan mode.
/// 验证 todolist 保持 AI 可见，但不允许在计划模式中执行。
#[test]
fn todolist_tool_is_visible_but_blocked_in_plan_mode() {
    let tool = build_todolist_tool();
    assert!(tool.descriptor.ai_visible);
    assert!(!tool.descriptor.available_in_plan);
}

/// Verifies todolist is described to models as a todolist/checklist progress tool.
/// 验证 todolist 面向模型时会被描述为 todolist/检查清单进程工具。
#[test]
fn todolist_tool_description_names_todo_semantics() {
    let tool = build_todolist_tool();
    assert!(tool.descriptor.description.contains("todolist/checklist"));
    assert!(tool.descriptor.description.contains("not Plan Mode"));
    assert!(tool.args_schema.to_string().contains("todolist/checklist"));
}

/// Verifies todolist rejects blank step text before emitting host projection data.
/// 验证 todolist 会在发出宿主投影数据前拒绝空白步骤文本。
#[tokio::test]
async fn todolist_tool_rejects_blank_step_text() {
    let tool = build_todolist_tool();
    let error = tool
        .invoke(test_native_request(json!({
            "items": [
                { "step": "   ", "status": "pending" }
            ]
        })))
        .await
        .expect_err("todolist should reject blank step text");

    assert!(error.message.contains("Todolist item step cannot be empty"));
}

/// Verifies submit_plan writes a marked Markdown file and returns a host artifact payload.
/// 验证 submit_plan 会写入带标记的 Markdown 文件并返回宿主产物载荷。
#[tokio::test]
async fn submit_plan_tool_writes_marked_plan_artifact() {
    let workspace_root =
        std::env::temp_dir().join(format!("vulcan-submit-plan-{}", Uuid::new_v4()));
    tokio_fs::create_dir_all(&workspace_root)
        .await
        .expect("workspace directory");
    let tool = build_submit_plan_tool();
    let result = tool
        .invoke(test_native_request_with_workspace(
            json!({
                "name": "Plan Mode Test",
                "content": "# Plan\n\n- Step one"
            }),
            workspace_root.clone(),
        ))
        .await
        .expect("submit_plan should execute");
    let host_result = result.host_result.expect("host result");
    let path = host_result.payload["path"].as_str().expect("path");
    let content = tokio_fs::read_to_string(path).await.expect("plan content");

    assert_eq!(host_result.kind, "plan_artifact");
    assert_eq!(
        host_result.payload["content"].as_str(),
        Some("# Plan\n\n- Step one")
    );
    assert!(content.contains(SUBMIT_PLAN_MARKER));
    assert!(content.contains("# Plan"));

    let _ = tokio_fs::remove_dir_all(workspace_root).await;
}

/// Verifies submit_plan refuses to replace files that it did not create.
/// 验证 submit_plan 会拒绝替换非工具创建的文件。
#[tokio::test]
async fn submit_plan_tool_rejects_unmarked_replace_target() {
    let workspace_root =
        std::env::temp_dir().join(format!("vulcan-submit-plan-{}", Uuid::new_v4()));
    tokio_fs::create_dir_all(&workspace_root)
        .await
        .expect("workspace directory");
    let target = workspace_root.join("manual.md");
    tokio_fs::write(&target, "# Manual\n")
        .await
        .expect("manual plan");
    let tool = build_submit_plan_tool();
    let error = tool
        .invoke(test_native_request_with_workspace(
            json!({
                "name": "Manual",
                "content": "# Replacement",
                "replace": true,
                "path": target.display().to_string()
            }),
            workspace_root.clone(),
        ))
        .await
        .expect_err("submit_plan should reject unmarked replacement");

    assert!(error.message.contains("not created by submit_plan"));

    let _ = tokio_fs::remove_dir_all(workspace_root).await;
}

/// Verifies submit_plan replacement accepts marked artifacts without an in-memory session record.
/// 验证 submit_plan 替换允许没有内存会话记录但带工具标记的产物。
#[tokio::test]
async fn submit_plan_tool_replaces_marked_target_without_session_record() {
    let workspace_root =
        std::env::temp_dir().join(format!("vulcan-submit-plan-{}", Uuid::new_v4()));
    tokio_fs::create_dir_all(&workspace_root)
        .await
        .expect("workspace directory");
    let target = workspace_root.join("generated.md");
    let initial_plan = format!("{SUBMIT_PLAN_MARKER}\n\n# 计划\n\n检查服务端用户等级。\n");
    let (encoded_initial_plan, _, had_errors) = encoding_rs::GBK.encode(&initial_plan);
    assert!(!had_errors);
    tokio_fs::write(&target, encoded_initial_plan.as_ref())
        .await
        .expect("GBK tool plan");
    let tool = build_submit_plan_tool();
    let result = tool
        .invoke(test_native_request_with_workspace(
            json!({
                "name": "Generated",
                "content": "# Replacement",
                "replace": true,
                "path": target.display().to_string()
            }),
            workspace_root.clone(),
        ))
        .await
        .expect("submit_plan should replace a marked target");
    let host_result = result.host_result.expect("host result");
    let replaced = tokio_fs::read_to_string(&target)
        .await
        .expect("replacement content");
    let expected_path = target.display().to_string();

    assert_eq!(host_result.kind, "plan_artifact");
    assert_eq!(
        host_result.payload["path"].as_str(),
        Some(expected_path.as_str())
    );
    assert!(replaced.contains(SUBMIT_PLAN_MARKER));
    assert!(replaced.contains("# Replacement"));

    let _ = tokio_fs::remove_dir_all(workspace_root).await;
}

/// Verifies submit_plan feedback paths hide Windows extended-path prefixes.
/// 验证 submit_plan 反馈路径会隐藏 Windows 扩展路径前缀。
#[test]
fn submit_plan_feedback_path_hides_windows_extended_prefix() {
    assert_eq!(
        display_submit_plan_path(Path::new(
            "\\\\?\\D:\\openvulcan\\test-project\\docs\\plan\\mock.md"
        )),
        "D:\\openvulcan\\test-project\\docs\\plan\\mock.md"
    );
    assert_eq!(
        display_submit_plan_path(Path::new("\\\\?\\UNC\\server\\share\\mock.md")),
        "\\\\server\\share\\mock.md"
    );
}

/// Verifies completed goals return the Codex-compatible structured usage-report instruction.
/// 验证已完成目标会返回与 Codex 兼容的结构化用量汇报指令。
#[test]
fn completed_goal_response_reports_structured_usage_instruction() {
    let goal = NativeGoalSnapshot {
        session_id: Uuid::new_v4(),
        goal_id: String::from("goal-1"),
        objective: String::from("完成目标模式"),
        status: NativeGoalStatus::Complete,
        token_budget: Some(10_000),
        tokens_used: 1_234,
        uncached_input_tokens: 900,
        cached_input_tokens: 300,
        output_tokens: 334,
        reasoning_tokens: 100,
        total_tokens: 1_534,
        time_used_seconds: 56,
        created_at_ms: 1_000,
        updated_at_ms: 2_000,
    };

    let response = goal_response_value(Some(&goal), true);

    assert_eq!(response["remainingTokens"], json!(8_766));
    assert!(
        response["completionBudgetReport"]
            .as_str()
            .is_some_and(|value| value.contains("Report final usage"))
    );
    assert_eq!(response["goal"]["tokensUsed"], json!(1_234));
    assert_eq!(response["goal"]["tokenBudget"], json!(10_000));
    assert_eq!(response["goal"]["timeUsedSeconds"], json!(56));
}

/// Verifies AI-created goals return their stable guidance directly in the create tool result.
/// 验证 AI 创建的目标会直接在创建工具结果中返回稳定引导。
#[test]
fn created_goal_response_includes_immediate_active_context() {
    let goal = NativeGoalSnapshot {
        session_id: Uuid::new_v4(),
        goal_id: String::from("goal-created-by-ai"),
        objective: String::from("连续执行并保持缓存"),
        status: NativeGoalStatus::Active,
        token_budget: Some(20_000),
        tokens_used: 0,
        uncached_input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_tokens: 0,
        total_tokens: 0,
        time_used_seconds: 0,
        created_at_ms: 1_000,
        updated_at_ms: 1_000,
    };

    let response = goal_created_response_value(&goal);
    let active_context = response["activeGoalContext"]
        .as_str()
        .expect("create response should include active goal context");

    assert!(active_context.contains("<goal_version>goal-created-by-ai</goal_version>"));
    assert!(active_context.contains("<objective>\n连续执行并保持缓存\n</objective>"));
}

/// Verifies that the agent registry tool exposes extended runtime fields for diagnostics and future delegate orchestration.
/// 验证代理注册表工具会暴露扩展运行时字段，用于诊断与后续委派编排。
#[tokio::test]
async fn agent_registry_tool_exposes_extended_runtime_fields() {
    let snapshot = Arc::new(NativeToolRuntimeSnapshot {
        workspace_directory: String::from(r"D:\workspace"),
        active_model: Some(String::from("openai/gpt-5")),
        agent_count: 1,
        skill_count: 1,
        configured_mcp_server_ids: vec![String::from("server-a")],
        enabled_mcp_server_ids: vec![String::from("server-a")],
        vmm_global: VmmConfigLayer::default(),
    });
    let config = AgentRuntimeConfig {
        active_model: Some(String::from("openai/gpt-5")),
        active_agent_id: String::from("agent-a"),
        agents: vec![RuntimeAgentDefinition {
            id: String::from("agent-a"),
            name: String::from("Agent A"),
            kind: RuntimeAgentKind::Coordinator,
            description: String::from("Coordinator agent"),
            system_prompt: String::from("system prompt"),
            skill_ids: vec![String::from("skill-a")],
            native_tools: vec![String::from("native-a")],
            plugin_tools: vec![String::from("plugin-tool-a")],
            mcp_tools: vec![String::from("mcp-tool-a")],
            delegates: vec![String::from("agent-b")],
            tool_policy: Some(String::from("policy-a")),
            routes: vec![RuntimeAgentRoute {
                signal: String::from("needs-lua"),
                delegate: String::from("lua-agent"),
            }],
            fallback_delegate: Some(String::from("agent-b")),
            model: Some(String::from("anthropic/claude")),
            temperature: Some(0.2),
            max_turns: Some(9),
            runtime_policy: vulcan_session_core::ExecutionRuntimePolicyRequest::default(),
        }],
        skills: vec![RuntimeSkillDefinition {
            id: String::from("skill-a"),
            name: String::from("Skill A"),
            kind: RuntimeSkillKind::Regular,
            description: String::from("Skill description"),
            enabled: true,
            trigger_signals: vec![String::from("signal-a")],
            emitted_signal: Some(String::from("signal-b")),
            confidence_delta: 0.5,
        }],
        tool_policies: BTreeMap::new(),
        configured_mcp_server_ids: vec![String::from("server-a")],
        enabled_mcp_server_ids: vec![String::from("server-a")],
    };

    let tool = build_agent_list_tool(snapshot, &config);
    let payload = tool
        .invoke(NativeToolExecutionRequest {
            arguments: serde_json::Value::Object(Default::default()),
            session_id: None,
            workspace_root: None,
            host_project_id: None,
            controller_runtime: None,
            goal_runtime: None,
            knowledge_access: None,
            vce_retrieval_handler: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("agent registry tool should execute");
    let payload = payload.model_output;

    let agent = &payload["agents"][0];
    assert_eq!(agent["description"], "Coordinator agent");
    assert_eq!(agent["systemPrompt"], "system prompt");
    assert_eq!(agent["fallbackDelegate"], "agent-b");
    assert_eq!(agent["model"], "anthropic/claude");
    let temperature = agent["temperature"]
        .as_f64()
        .expect("temperature should serialize as f64");
    assert!((temperature - 0.2).abs() < 1e-6);
    assert_eq!(agent["maxTurns"], 9);
    assert_eq!(agent["pluginTools"][0], "plugin-tool-a");
    assert_eq!(agent["routes"][0]["signal"], "needs-lua");
    assert_eq!(agent["routes"][0]["delegate"], "lua-agent");
}

/// Verifies that host-diagnostic discovery tools remain registered but are hidden from AI-facing tool exposure.
/// 验证宿主诊断发现类工具仍会注册，但不会暴露到 AI 可见工具面中。
#[test]
fn host_diagnostic_tools_are_hidden_from_ai() {
    let snapshot = Arc::new(NativeToolRuntimeSnapshot {
        workspace_directory: String::from(r"D:\workspace"),
        active_model: Some(String::from("openai/gpt-5")),
        agent_count: 0,
        skill_count: 0,
        configured_mcp_server_ids: vec![String::from("server-a")],
        enabled_mcp_server_ids: vec![String::from("server-a")],
        vmm_global: VmmConfigLayer::default(),
    });
    let config = AgentRuntimeConfig {
        active_model: Some(String::from("openai/gpt-5")),
        active_agent_id: String::from("agent-a"),
        agents: Vec::new(),
        skills: Vec::new(),
        tool_policies: BTreeMap::new(),
        configured_mcp_server_ids: vec![String::from("server-a")],
        enabled_mcp_server_ids: vec![String::from("server-a")],
    };

    let workspace_tool = build_workspace_describe_tool(Arc::clone(&snapshot));
    let skill_tool = build_skill_list_tool(Arc::clone(&snapshot), &config);
    let mcp_tool = build_mcp_server_list_tool(snapshot);

    assert!(!workspace_tool.descriptor.ai_visible);
    assert!(!workspace_tool.descriptor.available_in_plan);
    assert!(!skill_tool.descriptor.ai_visible);
    assert!(!skill_tool.descriptor.available_in_plan);
    assert!(!mcp_tool.descriptor.ai_visible);
    assert!(!mcp_tool.descriptor.available_in_plan);
}

/// Verifies that the skill registry tool exposes skill metadata that used to stay hidden behind dead-code suppressions.
/// 验证技能注册表工具会暴露此前被 dead-code 抑制隐藏的技能元数据。
#[tokio::test]
async fn skill_registry_tool_exposes_extended_runtime_fields() {
    let snapshot = Arc::new(NativeToolRuntimeSnapshot {
        workspace_directory: String::from(r"D:\workspace"),
        active_model: Some(String::from("openai/gpt-5")),
        agent_count: 1,
        skill_count: 1,
        configured_mcp_server_ids: vec![],
        enabled_mcp_server_ids: vec![],
        vmm_global: VmmConfigLayer::default(),
    });
    let config = AgentRuntimeConfig {
        active_model: Some(String::from("openai/gpt-5")),
        active_agent_id: String::from("agent-a"),
        agents: vec![],
        skills: vec![RuntimeSkillDefinition {
            id: String::from("skill-a"),
            name: String::from("Skill A"),
            kind: RuntimeSkillKind::Automation,
            description: String::from("Skill description"),
            enabled: true,
            trigger_signals: vec![String::from("signal-a")],
            emitted_signal: Some(String::from("signal-b")),
            confidence_delta: 0.5,
        }],
        tool_policies: BTreeMap::new(),
        configured_mcp_server_ids: vec![],
        enabled_mcp_server_ids: vec![],
    };

    let tool = build_skill_list_tool(snapshot, &config);
    let payload = tool
        .invoke(NativeToolExecutionRequest {
            arguments: serde_json::Value::Object(Default::default()),
            session_id: None,
            workspace_root: None,
            host_project_id: None,
            controller_runtime: None,
            goal_runtime: None,
            knowledge_access: None,
            vce_retrieval_handler: None,
            spill_root: None,
            enabled_prompt_skill_names: Vec::new(),
            prepared_prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        })
        .await
        .expect("skill registry tool should execute");
    let payload = payload.model_output;

    let skill = &payload["skills"][0];
    assert_eq!(skill["description"], "Skill description");
    assert_eq!(skill["emittedSignal"], "signal-b");
    assert_eq!(skill["confidenceDelta"], 0.5);
}
