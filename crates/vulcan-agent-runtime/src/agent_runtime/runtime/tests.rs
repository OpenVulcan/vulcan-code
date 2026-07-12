use super::{
    filter_luaskills_entries_by_explicit_registration, filter_luaskills_entries_by_host_dispatch,
    move_matching_tool_ids, resolve_agent_native_tools, resolve_agent_system_plugin_tool_ids,
};
use crate::agent_runtime::config::{
    RuntimeAgentDefinition, RuntimeAgentKind, RuntimeToolPolicyDefinition,
};
use crate::provider_config::PersistedLuaSkillsConfiguration;
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
        plugin_tools: Vec::new(),
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

/// Verifies direct agent plugin tools override policy defaults and unknown identifiers are removed.
/// 验证代理直接声明的插件工具会覆盖策略默认值，且未知标识会被移除。
#[test]
fn direct_plugin_tool_selection_overrides_policy_and_filters_unknown_tools() {
    let definition = RuntimeAgentDefinition {
        id: String::from("agent-a"),
        name: String::from("Agent A"),
        kind: RuntimeAgentKind::Worker,
        description: String::from("worker"),
        system_prompt: String::new(),
        skill_ids: Vec::new(),
        native_tools: Vec::new(),
        plugin_tools: vec![
            String::from("plugin-b"),
            String::from("missing"),
            String::from("plugin-b"),
        ],
        mcp_tools: Vec::new(),
        delegates: Vec::new(),
        tool_policy: Some(String::from("policy-a")),
        routes: Vec::new(),
        fallback_delegate: None,
        model: None,
        temperature: None,
        max_turns: None,
        runtime_policy: vulcan_session_core::ExecutionRuntimePolicyRequest::default(),
    };
    let policies = BTreeMap::from([(
        String::from("policy-a"),
        RuntimeToolPolicyDefinition {
            id: String::from("policy-a"),
            name: String::from("Policy A"),
            allow_all_native: false,
            native_tools: Vec::new(),
            allow_all_plugin_tools: true,
            plugin_tools: Vec::new(),
            mcp_servers: Vec::new(),
            mcp_tools: Vec::new(),
        },
    )]);
    let available = BTreeSet::from([String::from("plugin-a"), String::from("plugin-b")]);
    assert_eq!(
        resolve_agent_system_plugin_tool_ids(&definition, &policies, &available),
        vec![String::from("plugin-b")]
    );
}

/// Verifies an allow-all plugin policy resolves the complete enabled plugin tool set.
/// 验证允许全部插件工具的策略会解析完整的已启用插件工具集合。
#[test]
fn allow_all_plugin_policy_resolves_every_available_tool() {
    let mut definition = RuntimeAgentDefinition {
        id: String::from("agent-a"),
        name: String::from("Agent A"),
        kind: RuntimeAgentKind::Worker,
        description: String::from("worker"),
        system_prompt: String::new(),
        skill_ids: Vec::new(),
        native_tools: Vec::new(),
        plugin_tools: Vec::new(),
        mcp_tools: Vec::new(),
        delegates: Vec::new(),
        tool_policy: Some(String::from("policy-a")),
        routes: Vec::new(),
        fallback_delegate: None,
        model: None,
        temperature: None,
        max_turns: None,
        runtime_policy: vulcan_session_core::ExecutionRuntimePolicyRequest::default(),
    };
    let policy = RuntimeToolPolicyDefinition {
        id: String::from("policy-a"),
        name: String::from("Policy A"),
        allow_all_native: false,
        native_tools: Vec::new(),
        allow_all_plugin_tools: true,
        plugin_tools: Vec::new(),
        mcp_servers: Vec::new(),
        mcp_tools: Vec::new(),
    };
    let available = BTreeSet::from([String::from("plugin-a"), String::from("plugin-b")]);
    assert_eq!(
        resolve_agent_system_plugin_tool_ids(
            &definition,
            &BTreeMap::from([(String::from("policy-a"), policy.clone())]),
            &available,
        ),
        vec![String::from("plugin-a"), String::from("plugin-b")]
    );
    definition.tool_policy = Some(String::from("missing"));
    assert!(
        resolve_agent_system_plugin_tool_ids(&definition, &BTreeMap::new(), &available).is_empty()
    );
}

/// Verifies in-memory legacy migration moves exact plugin ids without duplicating existing targets.
/// 验证内存旧版迁移会移动精确插件标识，且不会重复已有目标。
#[test]
fn legacy_plugin_tool_migration_moves_only_exact_discovered_ids() {
    let mut native = vec![
        String::from("shell_command"),
        String::from("plugin-a"),
        String::from("plugin-a"),
    ];
    let mut plugins = vec![String::from("plugin-a")];
    move_matching_tool_ids(
        &mut native,
        &mut plugins,
        &BTreeSet::from([String::from("plugin-a")]),
    );
    assert_eq!(native, vec![String::from("shell_command")]);
    assert_eq!(plugins, vec![String::from("plugin-a")]);
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
        plugin_tools: Vec::new(),
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
