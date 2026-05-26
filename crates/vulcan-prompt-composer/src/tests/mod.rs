use std::fs;
use std::path::PathBuf;

use crate::composer::engine::PromptComposerEngine;
use crate::composer::request::PromptAssemblyInput;
use crate::composer::resources::PromptComposerResourceConfig;
use crate::session::delta_state::{ProjectRulesDeltaKind, ProjectRulesDeltaState};
use crate::source::snapshot::{
    AgentPromptSnapshot, EnvironmentContextSnapshot, ProjectRulesSnapshot, ProviderPromptSnapshot,
    TaskGoalSnapshot, ToolCatalogSnapshot,
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
            .tool_metadata_segments
            .iter()
            .any(|segment| segment.body.contains("custom-only checks"))
    );

    let _ = fs::remove_dir_all(root);
}

/// Verifies tool-help bundle hits cache on the second run.
/// 验证第二次执行时工具帮助组合会命中缓存。
#[test]
fn tool_help_bundle_hits_cache_on_second_run() {
    let engine = PromptComposerEngine::with_builtins();
    let input = PromptAssemblyInput {
        mode: PromptAssemblyMode::Primary,
        model: ModelDescriptor {
            family: String::from("openai"),
            model_id: String::from("gpt-test"),
            supports_native_personality: false,
        },
        provider_prompt: Some(ProviderPromptSnapshot::new("Provider base")),
        agent_prompt: Some(AgentPromptSnapshot::new("Agent base")),
        personality: PersonalitySelection::Disabled,
        project_rules: Some(ProjectRulesSnapshot::new("AGENTS.md", "Rule body")),
        environment_context: Some(EnvironmentContextSnapshot::new("cwd=D:/repo")),
        task_goal: Some(TaskGoalSnapshot::new("Inspect prompt system")),
        tool_catalog: ToolCatalogSnapshot::new(vec![ToolDescriptor {
            id: String::from("shell_command"),
            title: String::from("Shell"),
            description: String::from("Run one shell command"),
            help_text: None,
            schema_fingerprint: Some(String::from("schema-1")),
        }]),
        subagent: None,
        custom_developer_segments: Vec::new(),
        custom_user_segments: Vec::new(),
    };

    let first = engine
        .compose(&input)
        .expect("first compose should succeed");
    assert_eq!(first.diagnostics.cache_report.bundle_hits, 0);
    assert_eq!(first.diagnostics.cache_report.bundle_misses, 1);

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

/// Build one unique temporary directory for tests.
/// 为测试构建一个唯一临时目录。
fn unique_temp_dir(prefix: &str) -> PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{suffix}"))
}
