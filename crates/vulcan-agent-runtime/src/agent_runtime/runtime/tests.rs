use super::{
    filter_luaskills_entries_by_explicit_registration, filter_luaskills_entries_by_host_dispatch,
    resolve_agent_native_tools,
};
use crate::agent_runtime::config::{
    AgentRuntimeConfig, RuntimeAgentDefinition, RuntimeAgentKind, RuntimeToolPolicyDefinition,
};
use crate::provider_config::{PersistedHostToolsConfiguration, PersistedLuaSkillsConfiguration};
use crate::tooling::NativeToolDescriptor;
use crate::tooling::native::{
    CREATE_GOAL_TOOL_ID, GET_GOAL_TOOL_ID, TODOLIST_TOOL_ID, UPDATE_GOAL_TOOL_ID,
};
use luaskills::RuntimeEntryDescriptor;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// Verifies that referencing one missing tool policy still exposes universal native tools without falling back to a full-open default.
/// 验证当代理引用缺失的工具策略时仍会暴露通用原生工具，但不会静默退回到完全放开的默认行为。
#[test]
fn missing_tool_policy_keeps_only_universal_native_tool_resolution() {
    let definition = RuntimeAgentDefinition {
        id: String::from("agent-a"),
        name: String::from("Agent A"),
        kind: RuntimeAgentKind::Worker,
        description: String::from("worker"),
        system_prompt: String::new(),
        skill_ids: Vec::new(),
        native_tools: Vec::new(),
        mcp_tools: Vec::new(),
        delegates: Vec::new(),
        tool_policy: Some(String::from("missing-policy")),
        routes: Vec::new(),
        fallback_delegate: None,
        model: None,
        temperature: None,
        max_turns: None,
        runtime_policy: vulcan_session_core::ExecutionRuntimePolicyRequest::default(),
    };
    let available_native_tool_ids = BTreeSet::from([
        String::from(CREATE_GOAL_TOOL_ID),
        String::from(GET_GOAL_TOOL_ID),
        String::from("shell_command"),
        String::from("terminal_session"),
        String::from(UPDATE_GOAL_TOOL_ID),
        String::from(TODOLIST_TOOL_ID),
    ]);
    let mut warnings = Vec::new();

    let resolved = resolve_agent_native_tools(
        &definition,
        &BTreeMap::<String, RuntimeToolPolicyDefinition>::new(),
        &available_native_tool_ids,
        &mut warnings,
    );

    assert_eq!(
        resolved,
        vec![
            String::from(TODOLIST_TOOL_ID),
            String::from(GET_GOAL_TOOL_ID),
            String::from(CREATE_GOAL_TOOL_ID),
            String::from(UPDATE_GOAL_TOOL_ID),
        ]
    );
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("仅保留通用原生工具"));
}

/// Verifies universal native tools are appended even when an agent uses an explicit restricted tool list.
/// 验证代理使用显式受限工具列表时仍会追加通用原生工具。
#[test]
fn explicit_native_tool_list_keeps_universal_goal_and_todolist_tools() {
    let definition = RuntimeAgentDefinition {
        id: String::from("agent-a"),
        name: String::from("Agent A"),
        kind: RuntimeAgentKind::Worker,
        description: String::from("worker"),
        system_prompt: String::new(),
        skill_ids: Vec::new(),
        native_tools: vec![String::from("shell_command")],
        mcp_tools: Vec::new(),
        delegates: Vec::new(),
        tool_policy: None,
        routes: Vec::new(),
        fallback_delegate: None,
        model: None,
        temperature: None,
        max_turns: None,
        runtime_policy: vulcan_session_core::ExecutionRuntimePolicyRequest::default(),
    };
    let available_native_tool_ids = BTreeSet::from([
        String::from(CREATE_GOAL_TOOL_ID),
        String::from(GET_GOAL_TOOL_ID),
        String::from("shell_command"),
        String::from(UPDATE_GOAL_TOOL_ID),
        String::from(TODOLIST_TOOL_ID),
    ]);
    let mut warnings = Vec::new();

    let resolved = resolve_agent_native_tools(
        &definition,
        &BTreeMap::<String, RuntimeToolPolicyDefinition>::new(),
        &available_native_tool_ids,
        &mut warnings,
    );

    assert_eq!(
        resolved,
        vec![
            String::from("shell_command"),
            String::from(TODOLIST_TOOL_ID),
            String::from(GET_GOAL_TOOL_ID),
            String::from(CREATE_GOAL_TOOL_ID),
            String::from(UPDATE_GOAL_TOOL_ID)
        ]
    );
    assert!(warnings.is_empty());
}

/// Verifies that only explicitly registered LuaSkills tools enter provider-visible schemas while implicit tools remain host-dispatchable later.
/// 验证只有显式注册的 LuaSkills 工具会进入 Provider 可见 Schema，而隐式工具保留给后续宿主调度。
#[test]
fn luaskills_registration_filters_only_explicit_tools() {
    let entries = vec![
        build_test_luaskills_entry("skill-a", "tool-a"),
        build_test_luaskills_entry("skill-a", "tool-b"),
        build_test_luaskills_entry("skill-b", "tool-a"),
    ];
    let mut registration = PersistedLuaSkillsConfiguration::default();
    assert!(registration.set_tool_disabled("skill-a", "tool-a", true));

    let filtered = filter_luaskills_entries_by_explicit_registration(entries, &registration);
    let remaining = filtered
        .iter()
        .map(|entry| entry.canonical_name.as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        remaining,
        BTreeSet::from(["skill-a-tool-b", "skill-b-tool-a"])
    );
}

/// Verifies only implicit LuaSkills tools are callable through the host-side dispatcher.
/// 验证只有隐式 LuaSkills 工具能通过宿主侧调度器调用。
#[test]
fn luaskills_dispatch_filters_only_implicit_tools() {
    let entries = vec![
        build_test_luaskills_entry("skill-a", "tool-a"),
        build_test_luaskills_entry("skill-a", "tool-b"),
        build_test_luaskills_entry("skill-b", "tool-a"),
    ];
    let mut registration = PersistedLuaSkillsConfiguration::default();
    assert!(registration.set_tool_registration_state(
        "skill-a",
        "tool-a",
        crate::provider_config::PersistedLuaSkillToolRegistrationState::Implicit,
    ));
    assert!(registration.set_tool_registration_state(
        "skill-b",
        "tool-a",
        crate::provider_config::PersistedLuaSkillToolRegistrationState::Disabled,
    ));

    let filtered = filter_luaskills_entries_by_host_dispatch(entries, &registration);
    let remaining = filtered
        .iter()
        .map(|entry| entry.canonical_name.as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(remaining, BTreeSet::from(["skill-a-tool-a"]));
}

/// Builds one minimal LuaSkills runtime entry descriptor used by the runtime visibility tests.
/// 构建运行时可见性测试使用的最小 LuaSkills 运行时入口描述符。
fn build_test_luaskills_entry(skill_id: &str, local_name: &str) -> RuntimeEntryDescriptor {
    RuntimeEntryDescriptor {
        canonical_name: format!("{skill_id}-{local_name}"),
        skill_id: skill_id.to_string(),
        local_name: local_name.to_string(),
        root_name: String::from("ROOT"),
        skill_dir: String::from("skill-dir"),
        description: String::from("entry"),
        parameters: Vec::new(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "value": {
                    "type": "string"
                }
            },
            "required": ["value"]
        }),
    }
}

/// Builds one minimal runtime configuration for TaskRuntime profile synchronization tests.
/// 为 TaskRuntime profile 同步测试构建一份最小运行时配置。
fn test_runtime_config(agents: Vec<RuntimeAgentDefinition>) -> AgentRuntimeConfig {
    AgentRuntimeConfig {
        active_model: Some(String::from("deepseek/deepseek-chat")),
        active_agent_id: agents
            .first()
            .map(|agent| agent.id.clone())
            .unwrap_or_else(|| String::from("agent-a")),
        agents,
        skills: Vec::new(),
        tool_policies: BTreeMap::new(),
        configured_mcp_server_ids: Vec::new(),
        enabled_mcp_server_ids: Vec::new(),
        host_tools: PersistedHostToolsConfiguration::default(),
    }
}

/// Builds one native-tool descriptor used by TaskRuntime profile synchronization tests.
/// 构建 TaskRuntime profile 同步测试使用的原生工具描述符。
fn test_native_tool_descriptor(tool_id: &str) -> NativeToolDescriptor {
    NativeToolDescriptor {
        id: tool_id.to_string(),
        name: tool_id.to_string(),
        description: String::from("test tool"),
        help_text: None,
        ai_visible: true,
        available_in_plan: false,
    }
}
