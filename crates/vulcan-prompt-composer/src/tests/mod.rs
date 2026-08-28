use std::fs;
use std::path::PathBuf;

use crate::composer::engine::PromptComposerEngine;
use crate::composer::request::{AdditionalPromptSegment, PromptAssemblyInput};
use crate::composer::resources::PromptComposerResourceConfig;
use crate::output_style::OutputStyleCatalog;
use crate::session::compaction::{
    ContextCompactionRequestPromptMode, build_context_compaction_developer_segment,
    build_context_compaction_request_prompt, build_context_compaction_request_prompt_for_mode,
};
use crate::session::delta_state::{ProjectRulesDeltaKind, ProjectRulesDeltaState};
use crate::source::project_rules::{DEFAULT_PROJECT_RULES_FILE_NAME, ProjectRulesFileCache};
use crate::source::snapshot::{
    AgentPromptSnapshot, EnvironmentContextSnapshot, ProjectRulesSnapshot, ProviderPromptSnapshot,
    SubagentSnapshot, TaskGoalSnapshot, ToolCatalogSnapshot,
};
use crate::types::{
    ModelDescriptor, PersonalitySelection, PromptAssemblyMode, PromptBaselineKind,
    PromptBaselineProvenance, ToolDescriptor,
};

/// Returns whether text contains CJK characters.
/// 返回文本是否包含 CJK 字符。
fn contains_cjk_text(text: &str) -> bool {
    text.chars().any(|character| {
        let codepoint = character as u32;
        (0x4E00..=0x9FFF).contains(&codepoint)
            || (0x3400..=0x4DBF).contains(&codepoint)
            || (0xF900..=0xFAFF).contains(&codepoint)
    })
}

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
        output_style: None,
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

/// Verifies a selected output style is emitted as an exact system-layer segment.
/// 验证选中的输出风格以精确正文进入系统层，而不会落入用户层。
#[test]
fn selected_output_style_is_attached_to_system_layer() {
    let engine = PromptComposerEngine::with_builtins();
    let mut input = prompt_input_with_tool_ids(&[]);
    let snapshot = OutputStyleCatalog::built_in()
        .resolve("structured")
        .expect("structured style should resolve");
    let expected_text = snapshot.text.clone();
    input.output_style = Some(snapshot);

    let output = engine.compose(&input).expect("compose should succeed");

    assert!(output.render_system_text().contains(&expected_text));
    assert!(!output.render_user_text().contains(&expected_text));
    assert!(
        output
            .system_segments
            .iter()
            .any(|segment| segment.node_id == "output_style")
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
    assert!(request_prompt.contains("Keep every listed summary section"));
    assert!(request_prompt.contains("Example output structure:"));
    assert!(request_prompt.contains("<example>"));
    assert!(request_prompt.contains("## Goal"));
    assert!(request_prompt.contains("## Active Task"));
    assert!(request_prompt.contains("## Pending User Asks"));
    assert!(request_prompt.contains("## Completed—Do Not Repeat"));
    assert!(request_prompt.contains("long-running task remains active"));
    assert!(request_prompt.contains("no newer user message exists"));
    assert!(request_prompt.contains("## Constraints & Preferences"));
    assert!(request_prompt.contains("## Progress"));
    assert!(request_prompt.contains("### Done"));
    assert!(request_prompt.contains("### In Progress"));
    assert!(request_prompt.contains("### Blocked"));
    assert!(request_prompt.contains("## Key Decisions"));
    assert!(request_prompt.contains("## Technical Context"));
    assert!(request_prompt.contains("## Relevant Files and Symbols"));
    assert!(request_prompt.contains("Errors and Fixes"));
    assert!(request_prompt.contains("User Messages and Corrections"));
    assert!(request_prompt.contains("## Next Steps"));
    assert!(request_prompt.contains("[path/to/file.ext]"));
    assert!(request_prompt.contains("\"(none)\""));
    assert!(request_prompt.contains("1-2 verbatim quotes"));
    assert!(request_prompt.contains("REMINDER: Do not call any tools"));
    assert!(request_prompt.contains("fail this compaction task"));
    assert!(
        developer_segment.contains("Internal host task: summarize earlier conversation context")
    );
    assert!(developer_segment.contains("Return exactly one <analysis> block"));
    assert!(developer_segment.contains("Tool calls will be rejected"));
    assert!(!request_prompt.is_empty());
    assert!(!developer_segment.is_empty());
    assert!(
        !contains_cjk_text(&request_prompt),
        "model-visible compaction request prompt must remain fixed English"
    );
    assert!(
        !contains_cjk_text(&developer_segment),
        "model-visible compaction developer segment must remain fixed English"
    );
}

/// Verifies compaction prompt variants explain the correct source scope.
/// 验证上下文压缩提示词变体会说明正确的源范围。
#[test]
fn context_compaction_prompt_variants_explain_source_scope() {
    let full_prompt = build_context_compaction_request_prompt_for_mode(
        ContextCompactionRequestPromptMode::FullFidelityMerge,
    );
    let pruned_prompt = build_context_compaction_request_prompt_for_mode(
        ContextCompactionRequestPromptMode::PrunedFallback,
    );

    assert!(full_prompt.contains("full cloned runtime conversation"));
    assert!(full_prompt.contains("merge newer facts"));
    assert!(!full_prompt.contains("clone-pruned source"));
    assert!(pruned_prompt.contains("clone-pruned source"));
    assert!(pruned_prompt.contains("do not invent omitted details"));
    assert!(pruned_prompt.contains("record that limitation"));
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
        output_style: None,
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
        output_style: None,
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

/// Verifies every model-visible child-agent tool receives coordinated single-level orchestration guidance.
/// 验证每个模型可见子代理工具都会获得一致的单层调度指导。
#[test]
fn child_agent_tools_receive_single_level_orchestration_help() {
    let engine = PromptComposerEngine::with_builtins();
    let tool_ids = [
        "spawn_agent",
        "agent_orchestration_info",
        "send_message",
        "wait_agent",
        "interrupt_agent",
        "close_agent",
        "list_agents",
    ];
    let mut input = prompt_input_with_tool_ids(&tool_ids);
    for tool in &mut input.tool_catalog.tools {
        tool.help_text = None;
    }

    let output = engine.compose(&input).expect("compose should succeed");
    let rendered_help = output.render_tool_metadata_text();

    for tool_id in tool_ids {
        assert!(
            output
                .tool_metadata_segments
                .iter()
                .any(|segment| segment.node_id == format!("tool_help_{tool_id}")),
            "missing built-in help for {tool_id}"
        );
    }
    assert!(rendered_help.contains("Only the root conversation may delegate"));
    assert!(rendered_help.contains("child agents cannot create another orchestration level"));
    assert!(rendered_help.contains("Use this whenever the parent task depends on child feedback"));
    assert!(rendered_help.contains("Use this for diagnostics and status inspection"));
    assert!(rendered_help.contains("Closing is cleanup, not result collection"));
    for description_fragment in [
        "Create an open child agent",
        "Return enabled child-agent profiles",
        "Deliver a message to an open child agent",
        "Wait for child-agent mailbox activity",
        "Return current status snapshots",
        "Stop an open child agent's current turn",
        "Close an open child agent",
    ] {
        assert!(
            !rendered_help.contains(description_fragment),
            "tool help should not repeat descriptor contract: {description_fragment}"
        );
    }
    assert!(!rendered_help.contains("subtree"));
    assert!(!rendered_help.contains("descendant"));
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

/// Verifies minimal context-compaction mode excludes tool-help nodes while preserving runtime instructions.
/// 验证极简上下文压缩模式会排除工具帮助节点，同时保留运行时指令。
#[test]
fn context_compaction_minimal_mode_excludes_tool_help_nodes() {
    let engine = PromptComposerEngine::with_builtins();
    let input = PromptAssemblyInput {
        mode: PromptAssemblyMode::ContextCompactionMinimal,
        model: ModelDescriptor {
            family: String::from("openai"),
            model_id: String::from("gpt-test"),
            supports_native_personality: false,
        },
        provider_prompt: Some(ProviderPromptSnapshot::new("Provider base")),
        agent_prompt: None,
        personality: PersonalitySelection::Disabled,
        output_style: None,
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
        custom_developer_segments: vec![AdditionalPromptSegment::dynamic(
            "Context Compaction Contract",
            "Return plain text only.",
            "Test compaction instruction",
        )],
        custom_user_segments: Vec::new(),
    };

    let output = engine.compose(&input).expect("compose should succeed");

    assert_eq!(output.diagnostics.strategy_id, "context-compaction-minimal");
    assert!(output.tool_metadata_segments.is_empty());
    assert!(
        output
            .render_developer_text()
            .contains("Return plain text only.")
    );
    assert!(!output.render_developer_text().contains("Provider base"));
    assert!(!output.render_tool_metadata_text().contains("shell_command"));
}

/// Verifies identical-looking custom segments keep producer-declared provenance through cache reuse.
/// 验证外观相同的自定义片段在缓存复用后仍保留生产者声明的来源分类。
#[test]
fn custom_segment_cache_preserves_typed_baseline_provenance() {
    let engine = PromptComposerEngine::with_builtins();
    let mut input = prompt_input_with_tool_ids(&[]);
    input.custom_developer_segments = vec![
        AdditionalPromptSegment::dynamic("Shared title", "Shared body", "Request-dynamic source"),
        AdditionalPromptSegment::locked(
            "Shared title",
            "Shared body",
            "Authoritative baseline source",
            PromptBaselineProvenance::new(PromptBaselineKind::PluginInstructions, "plugin.alpha"),
        ),
    ];

    let first = engine
        .compose(&input)
        .expect("first compose should succeed");
    let second = engine
        .compose(&input)
        .expect("second compose should succeed");

    assert_eq!(first.developer_segments.len(), 2);
    assert_eq!(first.developer_segments[0].baseline_provenance, None);
    assert_eq!(
        first.developer_segments[1].baseline_provenance,
        Some(PromptBaselineProvenance::new(
            PromptBaselineKind::PluginInstructions,
            "plugin.alpha",
        ))
    );
    assert_ne!(
        first.developer_segments[0].cache_key,
        first.developer_segments[1].cache_key
    );
    assert!(
        second
            .developer_segments
            .iter()
            .all(|segment| segment.from_cache)
    );
    assert_eq!(
        second.developer_segments[1].baseline_provenance,
        first.developer_segments[1].baseline_provenance
    );
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

/// Verifies project rules encoded as GBK are normalized before prompt composition.
/// 验证 GBK 编码的项目规则会在提示词组合前完成规范化。
#[test]
fn project_rules_file_cache_decodes_gbk_snapshot() {
    let root = unique_temp_dir("vulcan-project-rules-cache-gbk");
    fs::create_dir_all(&root).expect("create workspace");
    let source = "始终先读取项目文档。\n";
    let (encoded, _, had_errors) = encoding_rs::GBK.encode(source);
    assert!(!had_errors);
    fs::write(root.join(DEFAULT_PROJECT_RULES_FILE_NAME), encoded.as_ref())
        .expect("write GBK project rules");

    let snapshot = ProjectRulesFileCache::new()
        .load_workspace_default(&root)
        .expect("GBK project rules should decode")
        .expect("GBK project rules should exist");

    assert_eq!(snapshot.text, source.trim());
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
        output_style: None,
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
        output_style: None,
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
