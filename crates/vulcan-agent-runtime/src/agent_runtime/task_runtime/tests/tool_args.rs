use super::*;
use crate::agent_runtime::task_runtime::invocation_methods::agent_orchestration_info_models;

/// Verifies the model-facing subagent surface follows the core Vulcan tool split.
/// 验证模型可见子代理工具面遵循核心 Vulcan 工具拆分。
#[test]
fn subagent_tool_surface_exposes_session_agent_style_tools_without_visible_task() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let candidates = build_subagent_tool_candidates(task_runtime);
    let visible_ids = candidates
        .iter()
        .filter(|candidate| candidate.ai_visible)
        .map(|candidate| candidate.descriptor.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        visible_ids,
        vec![
            "spawn_agent",
            "agent_orchestration_info",
            "send_message",
            "wait_agent",
            "interrupt_agent",
            "close_agent",
            "list_agents"
        ]
    );
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.descriptor.id != "task")
    );
}

/// Verifies send_message exposes the Vulcan target/message/delivery schema.
/// 验证 send_message 暴露 Vulcan 的 target/message/delivery Schema。
#[test]
fn send_message_schema_exposes_target_message_and_delivery_mode() {
    let schema = send_message_args_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("message tool properties should be an object");

    assert!(properties.contains_key("target"));
    assert!(properties.contains_key("message"));
    assert!(properties.contains_key("delivery_mode"));
    assert!(!properties.contains_key("task_name"));
    assert_eq!(schema["required"], json!(["target", "message"]));
}

/// Verifies interrupt_agent follows the Vulcan target-only schema.
/// 验证 interrupt_agent 遵循 Vulcan 仅 target 参数的 Schema。
#[test]
fn interrupt_agent_schema_exposes_target_only() {
    let schema = interrupt_agent_args_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("interrupt_agent properties should be an object");

    assert!(properties.contains_key("target"));
    assert!(!properties.contains_key("task_name"));
    assert_eq!(schema["required"], json!(["target"]));
}

/// Verifies list_agents follows the Vulcan path_prefix-only schema.
/// 验证 list_agents 遵循 Vulcan 仅 path_prefix 参数的 Schema。
#[test]
fn list_agents_schema_exposes_path_prefix_only() {
    let schema = list_agents_args_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("list_agents properties should be an object");

    assert!(properties.contains_key("path_prefix"));
    assert!(!properties.contains_key("task_name"));
}

/// Rejects legacy task_name from the Vulcan list tool.
/// 拒绝 Vulcan 列表工具中的历史 task_name 参数。
#[test]
fn list_agents_arguments_reject_task_name_alias() {
    let error = list_agents_path_prefix(&json!({
        "task_name": "agent-review"
    }))
    .expect_err("list_agents should reject task_name aliases");

    assert!(error.contains("unsupported keys"));
    assert!(error.contains("task_name"));
}

/// Rejects close-style task_name from the Vulcan interrupt tool.
/// 拒绝 Vulcan 中断工具中的 close 风格 task_name 参数。
#[test]
fn interrupt_agent_arguments_reject_task_name_alias() {
    let error = interrupt_agent_target(&json!({
        "task_name": "agent-review"
    }))
    .expect_err("interrupt_agent should reject task_name aliases");

    assert!(error.contains("unsupported keys"));
    assert!(error.contains("task_name"));
}

/// Verifies close_agent exposes only a subtree target and rejects the removed cascade switch.
/// 验证 close_agent 只暴露子树目标，并拒绝已移除的级联开关。
#[test]
fn close_agent_schema_exposes_only_subtree_target() {
    let schema = close_agent_args_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("close_agent properties should be an object");

    assert!(properties.contains_key("target"));
    assert_eq!(properties.len(), 1);
    assert_eq!(schema["required"], json!(["target"]));
    let error = close_agent_target(&json!({
        "target": "reviewer",
        "cascade": false
    }))
    .expect_err("removed cascade argument should be rejected");
    assert!(error.contains("unsupported keys: cascade"));
}

/// Verifies agent_orchestration_info exposes only read-only query fields.
/// 验证 agent_orchestration_info 只暴露只读查询字段。
#[test]
fn agent_orchestration_info_schema_exposes_read_only_query_fields() {
    let schema = agent_orchestration_info_args_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("agent_orchestration_info properties should be an object");

    for key in ["sections", "agent_id", "query"] {
        assert!(properties.contains_key(key), "missing {key}");
    }
    assert!(!properties.contains_key("include_disabled"));
    assert!(!properties.contains_key("enabled"));
    assert!(!properties.contains_key("model"));
}

/// Verifies core spawn arguments map into the internal child-agent invocation payload.
/// 验证核心启动参数会映射到内部子代理调用载荷。
#[test]
fn spawn_agent_arguments_map_to_task_session_arguments() {
    let mapped = task_tool_args_from_spawn_agent_args(json!({
        "message": "Generate a lake image and return the file path.",
        "task_name": "image_worker_a",
        "model": "provider/model-a"
    }))
    .expect("spawn arguments should map to task arguments");

    assert_eq!(
        mapped,
        json!({
            "prompt": "Generate a lake image and return the file path.",
            "task_id": "image_worker_a",
            "description": "image_worker_a",
            "agent_mode": SUBAGENT_MODE_DEFAULT,
            "provider_model": {
                "provider_id": "provider",
                "model_id": "model-a"
            }
        })
    );
}

/// Rejects fork_turns from model-visible spawn_agent arguments.
/// 拒绝模型可见 spawn_agent 参数中的 fork_turns。
#[test]
fn spawn_agent_arguments_reject_fork_turns() {
    let error = task_tool_args_from_spawn_agent_args(json!({
        "message": "Generate a lake image and return the file path.",
        "task_name": "image_worker_a",
        "fork_turns": "0"
    }))
    .expect_err("spawn_agent should reject fork_turns");

    assert!(error.contains("unsupported keys"));
    assert!(error.contains("fork_turns"));
}

/// Verifies spawn_agent exposes the Vulcan task_name/message required schema.
/// 验证 spawn_agent 暴露 Vulcan 的 task_name/message 必填 Schema。
#[test]
fn spawn_agent_schema_requires_task_name_and_message() {
    let schema = spawn_agent_args_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("spawn_agent properties should be an object");

    for key in [
        "message",
        "creation_mode",
        "agent_id",
        "system_prompt",
        "model",
        "task_name",
    ] {
        assert!(properties.contains_key(key), "missing {key}");
    }
    assert!(!properties.contains_key("description"));
    assert_eq!(properties["task_name"]["minLength"], 1);
    assert_eq!(properties["task_name"]["pattern"], "^[^/]+$");
    assert_eq!(schema["required"], json!(["task_name", "message"]));
}

/// Verifies spawn_agent maps agent creation mode into profile bindings.
/// 验证 spawn_agent 会将 agent 创建模式映射为画像绑定。
#[test]
fn spawn_agent_arguments_map_agent_mode_to_profile_binding() {
    let mapped = task_tool_args_from_spawn_agent_args(json!({
        "message": "Audit the subagent implementation.",
        "task_name": "audit-worker",
        "creation_mode": "agent",
        "agent_id": "engineering/code-reviewer"
    }))
    .expect("agent spawn arguments should map to task arguments");

    assert_eq!(mapped["agent_mode"], SUBAGENT_MODE_EXPERT);
    assert_eq!(mapped["subagent_profile_id"], "engineering/code-reviewer");
    assert_eq!(mapped["task_id"], "audit-worker");
    assert!(mapped.get("fork_turns").is_none());
}

/// Rejects agent-mode spawn_agent calls that omit the string agent id.
/// 拒绝缺少字符串 agent_id 的智能体模式 spawn_agent 调用。
#[test]
fn spawn_agent_arguments_require_agent_id_for_agent_mode() {
    let error = task_tool_args_from_spawn_agent_args(json!({
        "message": "Audit the subagent implementation.",
        "task_name": "audit-worker",
        "creation_mode": "agent"
    }))
    .expect_err("agent spawn should require agent_id");

    assert!(error.contains("agent_id"));
}

/// Verifies mirror-mode spawn_agent calls keep the mode without requiring an agent id.
/// 验证镜像模式 spawn_agent 会保留模式且不要求 agent_id。
#[test]
fn spawn_agent_arguments_allow_mirror_without_agent_id() {
    let mapped = task_tool_args_from_spawn_agent_args(json!({
        "message": "Analyze inherited context.",
        "task_name": "mirror-worker",
        "creation_mode": "mirror"
    }))
    .expect("mirror spawn should not require agent_id");

    assert_eq!(mapped["agent_mode"], SUBAGENT_MODE_MIRROR);
    assert!(mapped.get("fork_turns").is_none());
    assert!(mapped.get("subagent_profile_id").is_none());
}

/// Verifies custom-mode spawn_agent maps one-off system prompts.
/// 验证自定义模式 spawn_agent 会映射一次性系统提示词。
#[test]
fn spawn_agent_arguments_map_custom_system_prompt() {
    let mapped = task_tool_args_from_spawn_agent_args(json!({
        "message": "Use custom review rules.",
        "task_name": "custom-review",
        "creation_mode": "custom",
        "system_prompt": "You are a focused reviewer."
    }))
    .expect("custom spawn should map to task arguments");

    assert_eq!(mapped["agent_mode"], SUBAGENT_MODE_DEFAULT);
    assert_eq!(mapped["system_prompt"], "You are a focused reviewer.");
    assert!(mapped.get("fork_turns").is_none());
}

/// Rejects legacy subagent profile and non-Vulcan override arguments from the core spawn tool.
/// 拒绝核心 spawn 工具中的旧子代理配置与非 Vulcan 覆盖参数。
#[test]
fn spawn_agent_arguments_reject_legacy_overrides() {
    let error = task_tool_args_from_spawn_agent_args(json!({
        "message": "Generate a lake image and return the file path.",
        "task_name": "image_worker_a",
        "description": "legacy display title",
        "agent_type": "legacy",
        "service_tier": "default",
        "tools": ["image_generation"]
    }))
    .expect_err("legacy spawn overrides should be rejected");

    assert!(error.contains("unsupported keys"));
    assert!(error.contains("description"));
    assert!(error.contains("agent_type"));
    assert!(error.contains("service_tier"));
    assert!(error.contains("tools"));
}

/// Rejects spawn_agent calls without the Vulcan task_name field.
/// 拒绝缺少 Vulcan task_name 字段的 spawn_agent 调用。
#[test]
fn spawn_agent_arguments_require_task_name() {
    let error = task_tool_args_from_spawn_agent_args(json!({
        "message": "Generate a lake image and return the file path."
    }))
    .expect_err("spawn_agent should require task_name");

    assert!(error.contains("task_name"));
}

/// Verifies the depth-one guard rejects child-agent attempts to spawn another child.
/// 验证一层深度护栏会拒绝子代理继续派生子代理。
#[test]
fn spawn_agent_arguments_reject_nested_spawn_at_depth_one() {
    let task_runtime = Arc::new(TaskRuntime::new());
    let parent_session_id = Uuid::new_v4();
    let child_session_id = Uuid::new_v4();
    let child_record = build_task_session_record(TaskSessionRecordInit {
        task_id: child_session_id,
        public_task_id: String::from("worker-a"),
        origin_session_id: parent_session_id,
        parent_task_id: None,
        depth: 1,
        description: Some(String::from("Worker A")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("initial work"),
        display_title: String::from("Worker A"),
        controller_config: None,
        snapshot: CoreConversationSnapshot::default(),
        last_prompt: String::from("initial work"),
        last_status: String::from("running"),
    });
    task_runtime
        .sessions
        .write()
        .expect("task session registry should be writable")
        .insert(child_session_id, child_record);
    let parsed_args = parse_task_tool_arguments(json!({
        "prompt": "nested work",
        "task_id": "nested-worker"
    }))
    .expect("internal task args should parse");

    let error = resolve_new_spawn_task_session_id(&task_runtime, child_session_id, &parsed_args)
        .expect_err("depth-one child agents must not spawn children");

    assert!(error.contains("maximum depth `1`"));
    assert!(error.contains("cannot create another subtask"));
}

/// Rejects alias-only spawn arguments that are not part of the tool schema.
/// 拒绝未出现在工具 Schema 中的 spawn_agent 别名参数。
#[test]
fn spawn_agent_arguments_reject_alias_only_message() {
    let error = task_tool_args_from_spawn_agent_args(json!({
        "prompt": "Generate a lake image and return the file path.",
        "task_id": "image-worker-a"
    }))
    .expect_err("alias-only spawn arguments should be rejected");

    assert!(error.contains("message"));
}

/// Verifies wait_agent follows the Vulcan target-aware schema.
/// 验证 wait_agent 遵循 Vulcan 支持目标选择的 Schema。
#[test]
fn wait_agent_schema_exposes_targets_and_timeout() {
    let schema = wait_agent_args_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("wait_agent properties should be an object");

    assert!(properties.contains_key("target"));
    assert!(properties.contains_key("targets"));
    assert!(properties.contains_key("timeout_ms"));
    assert!(!properties.contains_key("task_name"));
    assert_eq!(properties["timeout_ms"]["minimum"], json!(10_000));
    assert_eq!(properties["timeout_ms"]["maximum"], json!(3_600_000));
}

/// Accepts canonical wait targets while rejecting legacy aliases.
/// 接受规范等待目标，同时拒绝旧别名。
#[test]
fn wait_agent_arguments_accept_targets_and_reject_legacy_aliases() {
    let targets = wait_agent_target_selectors(&json!({
        "target": "agent-review",
        "targets": ["agent-check", "agent-review"],
        "timeout_ms": 10_000
    }))
    .expect("canonical wait targets should parse");

    assert_eq!(
        targets,
        vec![String::from("agent-check"), String::from("agent-review")]
    );

    let error = wait_agent_timeout_ms(&json!({
        "task_name": "agent-review",
        "timeout_ms": 10_000
    }))
    .expect_err("legacy wait target should be rejected");

    assert!(error.contains("unsupported keys"));
    assert!(error.contains("task_name"));
}

/// Verifies provider/model selectors split into structured provider model snapshots.
/// 验证 provider/model 选择器会拆分为结构化供应商模型快照。
#[test]
fn provider_model_selector_splits_into_snapshot_fields() {
    let (provider_id, model_id) = split_provider_model_selector("openrouter/openai/gpt-4.1")
        .expect("selector should split at the first slash");

    assert_eq!(provider_id, "openrouter");
    assert_eq!(model_id, "openai/gpt-4.1");
}

/// Verifies explicit spawn model selectors must exist in the current effective model choices.
/// 验证显式 spawn 模型选择器必须存在于当前有效模型选项中。
#[test]
fn explicit_spawn_model_selector_rejects_unavailable_model() {
    let (config, provider_id, _model_id) = configured_test_provider_model();
    let provider_model = ProviderModelSnapshot {
        provider_id: Some(provider_id),
        model_id: Some(String::from("missing-model-for-subagent-test")),
        reasoning_effort: None,
        execution_mode: None,
    };

    let error = validate_explicit_spawn_model_selector(&config, &provider_model)
        .expect_err("unavailable explicit model should fail");

    assert!(error.contains("not available"));
    assert!(error.contains("agent_orchestration_info"));
}

/// Verifies unavailable profile fixed models are cleared to fall back to the session model.
/// 验证不可用的配置级固定模型会被清除以回退到会话模型。
#[test]
fn profile_fixed_model_falls_back_when_unavailable() {
    let (config, provider_id, _model_id) = configured_test_provider_model();
    let base = build_test_controller_config("Base prompt");
    let invocation = TaskInvocationConfig::main("profile work");
    let mut resolved = test_resolved_config(&base, &invocation);
    resolved.effective_config.provider_model = ProviderModelSnapshot {
        provider_id: Some(provider_id),
        model_id: Some(String::from("missing-profile-model-for-subagent-test")),
        reasoning_effort: None,
        execution_mode: None,
    };

    let cleared = clear_unavailable_profile_model_override(&config, &mut resolved);

    assert!(cleared);
    assert_eq!(resolved.effective_config.provider_model.provider_id, None);
    assert_eq!(resolved.effective_config.provider_model.model_id, None);
}

/// Verifies agent_orchestration_info model rows expose selector and selection flags.
/// 验证 agent_orchestration_info 模型行会暴露选择器与选择标记。
#[test]
fn agent_orchestration_info_models_mark_session_and_agent_fixed_models() {
    let (config, provider_id, model_id) = configured_test_provider_model();
    let fixed_model = (provider_id.clone(), model_id.clone());

    let models = agent_orchestration_info_models(&config, Some(&fixed_model));

    assert_eq!(models.len(), 1);
    assert_eq!(models[0]["selector"], format!("{provider_id}/{model_id}"));
    assert_eq!(models[0]["provider_id"], provider_id);
    assert_eq!(models[0]["model_id"], model_id);
    assert_eq!(models[0]["is_session_model"], true);
    assert_eq!(models[0]["is_agent_fixed_model"], true);
    assert_eq!(models[0]["available"], true);
    assert!(models[0]["unavailable_reason"].is_null());
}

/// Verifies agent_orchestration_info includes the active session provider even without a persisted provider entry.
/// 验证 agent_orchestration_info 在没有持久化供应商条目时仍会包含当前会话供应商。
#[test]
fn agent_orchestration_info_models_include_session_provider_without_persisted_entry() {
    let provider = crate::provider_config::supported_providers()
        .iter()
        .find(|provider| !provider.models.is_empty())
        .expect("provider catalog should contain at least one model");
    let model_id = provider
        .models
        .first()
        .expect("selected provider should contain one model")
        .id
        .clone();
    let config = ProviderConfigurationFile {
        model: Some(format!("{}/{}", provider.id, model_id)),
        ..ProviderConfigurationFile::default()
    };

    let models = agent_orchestration_info_models(&config, None);

    assert!(models.iter().any(|model| model["selector"]
        == format!("{}/{}", provider.id, model_id)
        && model["is_session_model"] == true));
}

/// Builds a provider configuration with one known enabled model for selector tests.
/// 为选择器测试构建一份包含已知启用模型的供应商配置。
fn configured_test_provider_model() -> (ProviderConfigurationFile, String, String) {
    let provider = crate::provider_config::supported_providers()
        .iter()
        .find(|provider| !provider.models.is_empty())
        .expect("provider catalog should contain at least one model");
    let model_id = provider
        .models
        .first()
        .expect("selected provider should contain one model")
        .id
        .clone();
    let mut provider_entry = crate::provider_config::PersistedProviderEntry::default();
    provider_entry.set_enabled_models(vec![model_id.clone()]);
    let mut config = ProviderConfigurationFile {
        model: Some(format!("{}/{}", provider.id, model_id)),
        ..ProviderConfigurationFile::default()
    };
    config.provider.insert(provider.id.clone(), provider_entry);
    (config, provider.id.clone(), model_id)
}
