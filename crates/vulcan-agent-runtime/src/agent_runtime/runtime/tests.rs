use super::{
    RuntimeFoundation, filter_luaskills_entries_by_explicit_registration,
    filter_luaskills_entries_by_host_dispatch, move_matching_tool_ids, resolve_agent_native_tools,
    resolve_agent_system_plugin_tool_ids,
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
use std::sync::Arc;

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

/// Verifies execution foundation construction captures the published plugin snapshot without rescanning files.
/// 验证执行基础构建只捕获已发布插件快照，不会重新扫描文件。
#[test]
fn runtime_foundation_does_not_implicitly_refresh_plugin_catalog() {
    // temp owns every plugin and runtime path used by this isolated snapshot test.
    // temp 持有该隔离快照测试使用的全部插件与运行时路径。
    let temp = tempfile::tempdir().expect("create plugin snapshot test root");
    // system_root contains one content-only plugin initially published by the manager.
    // system_root 包含一个最初由管理器发布的纯内容插件。
    let system_root = temp.path().join("system-plugins");
    // package_root is the exact package mutated after the first published generation.
    // package_root 是首个已发布代次之后被修改的精确插件包。
    let package_root = system_root.join("content-plugin");
    std::fs::create_dir_all(package_root.join("skills/first-skill"))
        .expect("create initial plugin skill directory");
    std::fs::write(
        package_root.join("skills/first-skill/SKILL.md"),
        "---\nname: first-skill\ndescription: First published skill.\n---\n\nUse the first skill.",
    )
    .expect("write initial plugin skill");
    std::fs::write(
        package_root.join("plugin.yaml"),
        "schema_version: 1\nid: content-plugin\nversion: 1.0.0\nkind: bundled_system\ndefault_enabled: true\ndisplay:\n  name: Content Plugin\n  capability: Snapshot fixture\n",
    )
    .expect("write content plugin manifest");
    // manager owns the sole mutable catalog generation boundary for this test.
    // manager 持有该测试唯一可变的目录代次边界。
    let manager = vulcan_plugin_runtime::SystemPluginRuntimeManager::without_runtime(
        system_root,
        temp.path().join("user-plugins"),
        temp.path().join("bridge"),
        temp.path().join("distribution"),
        temp.path().join("state"),
        temp.path().join("runtime"),
    );
    // published is the generation execution preparation must continue to observe.
    // published 是执行准备必须继续观察的代次。
    let published = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish initial plugin catalog");
    assert_eq!(published.enabled_skills.len(), 1);
    std::fs::create_dir_all(package_root.join("skills/unpublished-skill"))
        .expect("create unpublished plugin skill directory");
    std::fs::write(
        package_root.join("skills/unpublished-skill/SKILL.md"),
        "---\nname: unpublished-skill\ndescription: Unpublished skill.\n---\n\nDo not expose before refresh.",
    )
    .expect("write unpublished plugin skill");
    // foundation represents one execution turn built from the manager's current immutable snapshot.
    // foundation 表示从管理器当前不可变快照构建的单个执行回合。
    let foundation = RuntimeFoundation::build_from_optional_mcp_access_with_runtimes_and_plugins(
        &crate::provider_config::ProviderConfigurationFile::default(),
        &crate::agent_profile::AgentProfileCatalog::default(),
        None,
        Some(manager.clone()),
        None,
        Arc::new(crate::agent_runtime::task_runtime::TaskRuntime::new()),
        Arc::new(crate::agent_runtime::question_runtime::QuestionRuntime::new()),
        Arc::new(crate::tooling::CommandSessionRuntime::new()),
        Arc::new(crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime::pending()),
    );
    // captured is the exact generation frozen into this execution foundation.
    // captured 是冻结到该执行基础中的精确代次。
    let captured = foundation
        .system_plugin_catalog()
        .expect("foundation should capture plugin catalog");

    assert_eq!(manager.snapshot().revision, published.revision);
    assert_eq!(captured.revision, published.revision);
    assert!(
        captured
            .enabled_skills
            .contains_key("content-plugin:first-skill")
    );
    assert!(
        !captured
            .enabled_skills
            .contains_key("content-plugin:unpublished-skill")
    );
}
