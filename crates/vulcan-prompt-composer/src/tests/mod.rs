use std::fs;
use std::path::PathBuf;

use crate::composer::engine::PromptComposerEngine;
use crate::composer::request::PromptAssemblyInput;
use crate::composer::resources::PromptComposerResourceConfig;
use crate::session::compaction::{
    build_context_compaction_developer_segment, build_context_compaction_request_prompt,
};
use crate::session::delta_state::{ProjectRulesDeltaKind, ProjectRulesDeltaState};
use crate::source::project_rules::{DEFAULT_PROJECT_RULES_FILE_NAME, ProjectRulesFileCache};
use crate::source::snapshot::{
    AgentPromptSnapshot, EnvironmentContextSnapshot, ProjectRulesSnapshot, ProviderPromptSnapshot,
    SubagentSnapshot, TaskGoalSnapshot, ToolCatalogSnapshot,
};
use crate::types::{ModelDescriptor, PersonalitySelection, PromptAssemblyMode, ToolDescriptor};

/// Verifies the built-in catalog defaults to friendly.
/// 验证内置目录默认使用 friendly。
#[test]
fn built_in_catalog_defaults_to_friendly() {
    let engine = PromptComposerEngine::with_builtins();
    let input = PromptAssemblyInput {
        mode: PromptAssemblyMode::Primary,
        model: ModelDescriptor {
            family: String::from("openai"),
            model_id: String::from("gpt-test"),
            supports_native_personality: false,
        },
        provider_prompt: None,
        agent_prompt: None,
        personality: PersonalitySelection::Default,
        project_rules: None,
        environment_context: None,
        task_goal: None,
        tool_catalog: ToolCatalogSnapshot::new(Vec::new()),
        subagent: None,
        custom_developer_segments: Vec::new(),
        custom_user_segments: Vec::new(),
    };

    let output = engine.compose(&input).expect("compose should succeed");
    assert_eq!(
        output.diagnostics.personality_id.as_deref(),
        Some("friendly")
    );
    assert!(
        output
            .render_developer_text()
            .contains("supportive teammate")
    );
}

/// Verifies the context-compaction request and developer instructions stay on one shared source path.
/// 验证上下文压缩请求与开发者指令保持同一份共享来源。
#[test]
fn context_compaction_prompt_helpers_share_one_source() {
    let request_prompt = build_context_compaction_request_prompt();
    let developer_segment = build_context_compaction_developer_segment();

    assert!(request_prompt.contains("Create a detailed continuation summary"));
    assert!(request_prompt.contains("Tool calls will be rejected"));
    assert!(request_prompt.contains("<analysis>"));
    assert!(request_prompt.contains("<summary>"));
    assert!(request_prompt.contains("<previous-summary>"));
    assert!(request_prompt.contains("durable memory"));
    assert!(request_prompt.contains("minimal rewriting"));
    assert!(request_prompt.contains("context added after the previous summary"));
    assert!(request_prompt.contains("All User Messages"));
    assert!(request_prompt.contains("Problem Solving"));
    assert!(request_prompt.contains("Optional Next Step"));
    assert!(
        developer_segment.contains("Internal host task: summarize earlier conversation context")
    );
    assert!(developer_segment.contains("Return exactly one <analysis> block"));
    assert!(developer_segment.contains("Tool calls will be rejected"));
    assert!(!request_prompt.is_empty());
    assert!(!developer_segment.is_empty());
}

/// Verifies directory resources can override personality and tool-help entries.
/// 验证目录资源可以覆盖人格与工具帮助条目。
#[test]
fn resource_directories_override_personality_and_tool_help() {
    let root = unique_temp_dir("vulcan-prompt-composer-resource-test");
    let personality_dir = root.join("personalities");
    let tool_help_dir = root.join("tool-help");
    fs::create_dir_all(&personality_dir).expect("create personality dir");
    fs::create_dir_all(&tool_help_dir).expect("create tool help dir");
    fs::write(
        personality_dir.join("custom.md"),
        "Custom collaboration style.",
    )
    .expect("write custom personality");
    fs::write(
        tool_help_dir.join("custom_tool.md"),
        "Prefer the custom tool for custom-only checks.",
    )
    .expect("write custom tool help");

    let engine = PromptComposerEngine::with_resources(&PromptComposerResourceConfig {
        personality_directory: Some(personality_dir),
        tool_help_directory: Some(tool_help_dir),
    })
    .expect("resource engine should build");
    let input = PromptAssemblyInput {
        mode: PromptAssemblyMode::Primary,
        model: ModelDescriptor {
            family: String::from("openai"),
            model_id: String::from("gpt-test"),
            supports_native_personality: false,
        },
        provider_prompt: None,
        agent_prompt: None,
        personality: PersonalitySelection::Named(String::from("custom")),
        project_rules: None,
        environment_context: None,
        task_goal: None,
        tool_catalog: ToolCatalogSnapshot::new(vec![ToolDescriptor {
            id: String::from("custom_tool"),
            title: String::from("Custom Tool"),
            description: String::from("Custom helper"),
            help_text: None,
            schema_fingerprint: None,
        }]),
        subagent: None,
        custom_developer_segments: Vec::new(),
        custom_user_segments: Vec::new(),
    };

    let output = engine.compose(&input).expect("compose should succeed");
    assert!(
        output
            .render_developer_text()
            .contains("Custom collaboration style.")
    );
    assert!(
        output
            .developer_segments
            .iter()
            .any(|segment| segment.source_label.contains("User personality resource")),
        "custom personality source should be visible"
    );
    assert!(
        output
            .tool_metadata_segments
            .iter()
            .any(|segment| segment.body.contains("custom-only checks"))
    );
    assert!(
        output
            .tool_metadata_segments
            .iter()
            .any(|segment| segment.source_label.contains("User tool-help resource")),
        "custom tool-help source should be visible"
    );
    assert!(
        output
            .render_tool_metadata_text()
            .contains("custom-only checks")
    );

    let _ = fs::remove_dir_all(root);
}

/// Verifies per-tool help segments keep bundle-level cache behavior.
/// 验证逐工具帮助片段仍保留组合级缓存行为。
#[test]
fn per_tool_help_segments_hit_bundle_cache_on_second_run() {
    let engine = PromptComposerEngine::with_builtins();
    let input = PromptAssemblyInput {
        mode: PromptAssemblyMode::Primary,
        model: ModelDescriptor {
            family: String::from("openai"),
            model_id: String::from("gpt-test"),
            supports_native_personality: false,
        },
        provider_prompt: Some(
            ProviderPromptSnapshot::new("Provider base")
                .with_source_label("Provider prompt resource `provider-test.txt`"),
        ),
        agent_prompt: Some(
            AgentPromptSnapshot::new("Agent base")
                .with_source_label("Agent prompt resource `agent-test.txt`"),
        ),
        personality: PersonalitySelection::Disabled,
        project_rules: Some(ProjectRulesSnapshot::new("AGENTS.md", "Rule body")),
        environment_context: Some(EnvironmentContextSnapshot::new("cwd=D:/repo")),
        task_goal: Some(TaskGoalSnapshot::new("Inspect prompt system")),
        tool_catalog: ToolCatalogSnapshot::new(vec![
            ToolDescriptor {
                id: String::from("shell_command"),
                title: String::from("Shell"),
                description: String::from("Run one shell command"),
                help_text: None,
                schema_fingerprint: Some(String::from("schema-1")),
            },
            ToolDescriptor {
                id: String::from("custom/tool"),
                title: String::from("Custom"),
                description: String::from("Run a custom helper"),
                help_text: Some(String::from("Prefer this custom helper for custom checks.")),
                schema_fingerprint: Some(String::from("schema-2")),
            },
        ]),
        subagent: None,
        custom_developer_segments: Vec::new(),
        custom_user_segments: Vec::new(),
    };

    let first = engine
        .compose(&input)
        .expect("first compose should succeed");
    assert_eq!(first.diagnostics.cache_report.bundle_hits, 0);
    assert_eq!(first.diagnostics.cache_report.bundle_misses, 1);
    assert!(
        first
            .system_segments
            .iter()
            .any(|segment| segment.source_label.contains("provider-test.txt"))
    );
    assert!(
        first
            .system_segments
            .iter()
            .any(|segment| segment.source_label.contains("agent-test.txt"))
    );
    assert!(
        first
            .tool_metadata_segments
            .iter()
            .any(|segment| segment.node_id == "tool_help_shell_command")
    );
    assert!(
        first
            .tool_metadata_segments
            .iter()
            .any(|segment| segment.node_id == "tool_help_custom_tool")
    );

    let second = engine
        .compose(&input)
        .expect("second compose should succeed");
    assert_eq!(second.diagnostics.cache_report.bundle_hits, 1);
    assert!(
        second
            .tool_metadata_segments
            .iter()
            .all(|segment| segment.from_cache)
    );
    assert_eq!(second.tool_metadata_segments.len(), 2);
    assert!(
        second
            .render_tool_metadata_text()
            .contains("Use shell_command for concrete local inspection")
    );
    assert!(
        second
            .tool_metadata_segments
            .iter()
            .any(|segment| segment.source_label.contains("Built-in tool help")),
        "cached tool-help segment should retain source labels"
    );
}

/// Verifies overlapping tool catalogs reuse per-tool cached segments across different bundles.
/// 验证不同工具组合之间的重叠工具会复用逐工具缓存片段。
#[test]
fn overlapping_tool_catalogs_reuse_per_tool_cache_entries() {
    let engine = PromptComposerEngine::with_builtins();
    let first_input = prompt_input_with_tool_ids(&["tool_a", "shared_tool"]);
    let second_input = prompt_input_with_tool_ids(&["shared_tool", "tool_b"]);

    let first = engine
        .compose(&first_input)
        .expect("first compose should succeed");
    assert_eq!(first.diagnostics.cache_report.node_hits, 0);
    assert!(
        first
            .tool_metadata_segments
            .iter()
            .any(|segment| segment.node_id == "tool_help_shared_tool")
    );

    let second = engine
        .compose(&second_input)
        .expect("second compose should succeed");
    assert_eq!(second.diagnostics.cache_report.bundle_hits, 0);
    assert!(
        second
            .tool_metadata_segments
            .iter()
            .any(|segment| segment.node_id == "tool_help_shared_tool" && segment.from_cache),
        "shared tool help should come from the node cache even when the bundle differs"
    );
}

/// Verifies context-compaction mode excludes tool-help nodes while preserving runtime instructions.
/// 验证上下文压缩模式会排除工具帮助节点，同时保留运行时指令。
#[test]
fn context_compaction_mode_excludes_tool_help_nodes() {
    let engine = PromptComposerEngine::with_builtins();
    let input = PromptAssemblyInput {
        mode: PromptAssemblyMode::ContextCompaction,
        model: ModelDescriptor {
            family: String::from("openai"),
            model_id: String::from("gpt-test"),
            supports_native_personality: false,
        },
        provider_prompt: Some(ProviderPromptSnapshot::new("Provider base")),
        agent_prompt: None,
        personality: PersonalitySelection::Disabled,
        project_rules: None,
        environment_context: Some(EnvironmentContextSnapshot::new("cwd=D:/repo")),
        task_goal: None,
        tool_catalog: ToolCatalogSnapshot::new(vec![ToolDescriptor {
            id: String::from("shell_command"),
            title: String::from("Shell"),
            description: String::from("Run one shell command"),
            help_text: Some(String::from("Do not appear in compaction mode.")),
            schema_fingerprint: Some(String::from("schema-1")),
        }]),
        subagent: None,
        custom_developer_segments: vec![String::from("Return plain text only.")],
        custom_user_segments: Vec::new(),
    };

    let output = engine.compose(&input).expect("compose should succeed");

    assert_eq!(output.diagnostics.strategy_id, "context-compaction");
    assert!(output.tool_metadata_segments.is_empty());
    assert!(
        output
            .render_developer_text()
            .contains("Return plain text only.")
    );
    assert!(!output.render_tool_metadata_text().contains("shell_command"));
}

/// Verifies clone-mode subagents inherit the base prompt and add delegation identity.
/// 验证分身模式子代理会继承基础提示词并附加委托身份。
#[test]
fn subagent_clone_mode_adds_identity_without_functional_system_prompt() {
    let engine = PromptComposerEngine::with_builtins();
    let input = subagent_prompt_input(
        PromptAssemblyMode::SubagentClone,
        Some(String::from("Review the authentication changes.")),
    );

    let output = engine.compose(&input).expect("compose should succeed");

    assert_eq!(output.diagnostics.strategy_id, "subagent_clone");
    assert!(output.render_system_text().contains("Provider base"));
    assert!(
        output
            .render_developer_text()
            .contains("Subagent identity: code-reviewer")
    );
    assert!(
        !output
            .render_system_text()
            .contains("Specialist reviewer mode")
    );
}

/// Verifies functional-mode subagents add their specialist system prompt on top of the base prompt.
/// 验证功能模式子代理会在基础提示词之上附加专家系统提示词。
#[test]
fn subagent_functional_mode_adds_specialist_system_prompt() {
    let engine = PromptComposerEngine::with_builtins();
    let input = subagent_prompt_input(
        PromptAssemblyMode::SubagentFunctional,
        Some(String::from("Specialist reviewer mode.")),
    );

    let output = engine.compose(&input).expect("compose should succeed");

    assert_eq!(output.diagnostics.strategy_id, "subagent_functional");
    assert!(output.render_system_text().contains("Provider base"));
    assert!(
        output
            .render_system_text()
            .contains("Specialist reviewer mode")
    );
    assert!(
        output
            .render_developer_text()
            .contains("Subagent identity: code-reviewer")
    );
}

/// Verifies project-rule delta tracking distinguishes unchanged and changed snapshots.
/// 验证项目规则增量跟踪能区分未变化与已变化快照。
#[test]
fn project_rules_delta_state_tracks_changes() {
    let mut state = ProjectRulesDeltaState::default();
    let first = ProjectRulesSnapshot::new("AGENTS.md", "Rule v1");
    let second = ProjectRulesSnapshot::new("AGENTS.md", "Rule v1");
    let third = ProjectRulesSnapshot::new("AGENTS.md", "Rule v2");

    assert_eq!(state.observe(Some(&first)), ProjectRulesDeltaKind::Changed);
    assert_eq!(
        state.observe(Some(&second)),
        ProjectRulesDeltaKind::Unchanged
    );
    assert_eq!(state.observe(Some(&third)), ProjectRulesDeltaKind::Changed);
}

/// Verifies project-rules file cache loads and reuses workspace AGENTS.md snapshots.
/// 验证项目规则文件缓存会加载并复用工作区 AGENTS.md 快照。
#[test]
fn project_rules_file_cache_reuses_unchanged_snapshot() {
    let root = unique_temp_dir("vulcan-project-rules-cache-reuse");
    fs::create_dir_all(&root).expect("create workspace");
    fs::write(root.join(DEFAULT_PROJECT_RULES_FILE_NAME), "Rule v1\n")
        .expect("write project rules");

    let mut cache = ProjectRulesFileCache::new();
    let first = cache
        .load_workspace_default(&root)
        .expect("first load should succeed")
        .expect("rules should exist");
    let second = cache
        .load_workspace_default(&root)
        .expect("second load should succeed")
        .expect("rules should still exist");

    assert_eq!(first, second);
    assert_eq!(cache.entry_count(), 1);

    let _ = fs::remove_dir_all(root);
}

/// Verifies project-rules file cache refreshes after content changes.
/// 验证项目规则文件缓存会在内容变化后刷新。
#[test]
fn project_rules_file_cache_refreshes_changed_snapshot() {
    let root = unique_temp_dir("vulcan-project-rules-cache-refresh");
    fs::create_dir_all(&root).expect("create workspace");
    let rules_path = root.join(DEFAULT_PROJECT_RULES_FILE_NAME);
    fs::write(&rules_path, "Rule v1\n").expect("write first project rules");

    let mut cache = ProjectRulesFileCache::new();
    let first = cache
        .load_workspace_default(&root)
        .expect("first load should succeed")
        .expect("rules should exist");
    std::thread::sleep(std::time::Duration::from_millis(2));
    fs::write(&rules_path, "Rule v2 changed\n").expect("write second project rules");
    let second = cache
        .load_workspace_default(&root)
        .expect("second load should succeed")
        .expect("rules should still exist");

    assert_ne!(first.content_hash, second.content_hash);
    assert_eq!(second.text, "Rule v2 changed");

    let _ = fs::remove_dir_all(root);
}

/// Verifies manual invalidation removes cached project-rules entries.
/// 验证手动失效会移除已缓存的项目规则条目。
#[test]
fn project_rules_file_cache_invalidates_workspace_entry() {
    let root = unique_temp_dir("vulcan-project-rules-cache-invalidate");
    fs::create_dir_all(&root).expect("create workspace");
    fs::write(root.join(DEFAULT_PROJECT_RULES_FILE_NAME), "Rule v1\n")
        .expect("write project rules");

    let mut cache = ProjectRulesFileCache::new();
    assert!(
        cache
            .load_workspace_default(&root)
            .expect("load should succeed")
            .is_some()
    );
    assert_eq!(cache.entry_count(), 1);

    cache.invalidate_workspace_default(&root);

    assert_eq!(cache.entry_count(), 0);

    let _ = fs::remove_dir_all(root);
}

/// Build one unique temporary directory for tests.
/// 为测试构建一个唯一临时目录。
fn unique_temp_dir(prefix: &str) -> PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{suffix}"))
}

/// Build one minimal prompt input with the requested visible tools.
/// 使用指定可见工具构建一个最小提示词输入。
fn prompt_input_with_tool_ids(tool_ids: &[&str]) -> PromptAssemblyInput {
    PromptAssemblyInput {
        mode: PromptAssemblyMode::Primary,
        model: ModelDescriptor {
            family: String::from("openai"),
            model_id: String::from("gpt-test"),
            supports_native_personality: false,
        },
        provider_prompt: None,
        agent_prompt: None,
        personality: PersonalitySelection::Disabled,
        project_rules: None,
        environment_context: None,
        task_goal: None,
        tool_catalog: ToolCatalogSnapshot::new(
            tool_ids
                .iter()
                .map(|tool_id| ToolDescriptor {
                    id: (*tool_id).to_string(),
                    title: (*tool_id).to_string(),
                    description: format!("Tool {tool_id}"),
                    help_text: Some(format!("Use {tool_id} when it matches the task.")),
                    schema_fingerprint: Some(format!("schema:{tool_id}")),
                })
                .collect(),
        ),
        subagent: None,
        custom_developer_segments: Vec::new(),
        custom_user_segments: Vec::new(),
    }
}

/// Build one minimal subagent prompt input for strategy tests.
/// 为策略测试构建一个最小子代理提示词输入。
fn subagent_prompt_input(
    mode: PromptAssemblyMode,
    functional_system_prompt: Option<String>,
) -> PromptAssemblyInput {
    PromptAssemblyInput {
        mode,
        model: ModelDescriptor {
            family: String::from("openai"),
            model_id: String::from("gpt-test"),
            supports_native_personality: false,
        },
        provider_prompt: Some(ProviderPromptSnapshot::new("Provider base")),
        agent_prompt: Some(AgentPromptSnapshot::new("Agent base")),
        personality: PersonalitySelection::Disabled,
        project_rules: None,
        environment_context: Some(EnvironmentContextSnapshot::new("cwd=D:/repo")),
        task_goal: Some(TaskGoalSnapshot::new("Inspect delegated code")),
        tool_catalog: ToolCatalogSnapshot::new(Vec::new()),
        subagent: Some(SubagentSnapshot {
            identity: String::from("code-reviewer"),
            task: String::from("Review the authentication changes."),
            delegation: Some(String::from("Return findings to the parent agent.")),
            functional_system_prompt,
        }),
        custom_developer_segments: Vec::new(),
        custom_user_segments: Vec::new(),
    }
}
