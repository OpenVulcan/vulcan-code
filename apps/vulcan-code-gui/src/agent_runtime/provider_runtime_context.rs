//! Provider runtime context helpers for system preamble and turn-budget resolution.
//! 面向系统前导语与轮次预算解析的 Provider 运行时上下文辅助模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::execution_models::{
    ExecutionPreparation, ExecutionPromptPurpose, persisted_personality_to_prompt_selection,
};
use crate::agent_runtime::prompt_composer_runtime::{
    record_runtime_prompt_composer_report, runtime_prompt_composer_engine,
};
use crate::agent_runtime::provider_prompt_loader::{
    LoadedProviderBaseSystemPrompt, load_provider_base_system_prompt,
    load_provider_base_system_prompt_with_source,
};
use crate::provider_config::{PersistedPromptConfiguration, load_project_configuration_file};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use vulcan_core::types::ProviderPromptLayers as CoreProviderPromptLayers;
use vulcan_prompt_composer::{
    AgentPromptSnapshot, EnvironmentContextSnapshot, ModelDescriptor, ProjectPromptFileCache,
    ProjectPromptSnapshot, ProjectRulesFileCache, ProjectRulesSnapshot, PromptAssemblyInput,
    PromptAssemblyMode, PromptAssemblyOutput, ProviderPromptSnapshot, ToolCatalogSnapshot,
    ToolDescriptor, build_context_compaction_developer_segment,
};

/// Process-wide project-rules source cache shared by provider runtime requests.
/// Provider 运行时请求共享的进程级项目规则来源缓存。
static PROJECT_RULES_FILE_CACHE: OnceLock<Mutex<ProjectRulesFileCache>> = OnceLock::new();
/// Process-wide project-prompt source cache shared by provider runtime requests.
/// Provider 运行时请求共享的进程级项目提示词来源缓存。
static PROJECT_PROMPT_FILE_CACHE: OnceLock<Mutex<ProjectPromptFileCache>> = OnceLock::new();

/// Builds the effective runtime system prompt sent to the real provider agent.
/// 构建发送给真实 Provider 代理的生效运行时系统提示词。
#[cfg(test)]
pub(crate) fn build_runtime_system_preamble(preparation: &ExecutionPreparation) -> String {
    build_runtime_prompt_layers_for_agent(preparation, None).fallback_text
}

/// Builds the effective layered runtime prompt sent to the real provider agent.
/// 构建发送给真实 Provider 代理的生效分层运行时提示词。
pub(crate) fn build_runtime_prompt_layers_for_agent(
    preparation: &ExecutionPreparation,
    active_agent: Option<&RuntimeAgentDefinition>,
) -> RuntimePromptLayers {
    let input = build_runtime_prompt_assembly_input(preparation, active_agent);
    let output = match runtime_prompt_composer_engine().compose(&input) {
        Ok(output) => output,
        Err(error) => {
            return RuntimePromptLayers::from_legacy(build_legacy_runtime_system_preamble(
                preparation,
                Some(error.to_string()),
            ));
        }
    };
    if active_agent.is_none() {
        record_runtime_prompt_composer_report(
            preparation.session_id.to_string(),
            preparation.session_title.clone(),
            "primary",
            preparation.model_blueprint.model_id.clone(),
            &output,
        );
    }

    RuntimePromptLayers::from_prompt_output(&output, preparation)
}

/// Runtime prompt layers plus the legacy fallback text used by old transport code.
/// 运行时提示词层，以及旧传输代码使用的兼容回退文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimePromptLayers {
    /// Legacy fallback system-preamble text.
    /// 旧版兼容系统前导语文本。
    pub(crate) fallback_text: String,
    /// Structured provider prompt layers.
    /// 结构化 Provider 提示词层。
    pub(crate) provider_layers: CoreProviderPromptLayers,
}

impl RuntimePromptLayers {
    /// Build prompt layers from a legacy pre-rendered system prompt.
    /// 从一份旧版预渲染系统提示词构建提示词层。
    fn from_legacy(system_prompt: String) -> Self {
        let provider_layers = CoreProviderPromptLayers::from_system_prompt(system_prompt.clone());
        Self {
            fallback_text: system_prompt,
            provider_layers,
        }
    }

    /// Build prompt layers from one prompt-composer output.
    /// 从一份提示词组合器输出构建提示词层。
    fn from_prompt_output(
        output: &PromptAssemblyOutput,
        preparation: &ExecutionPreparation,
    ) -> Self {
        let provider_layers = render_runtime_prompt_layers(output);
        let fallback_text = if provider_layers.is_empty() {
            build_legacy_runtime_system_preamble(preparation, None)
        } else {
            provider_layers.render_fallback_system_prompt()
        };
        Self {
            fallback_text,
            provider_layers,
        }
    }
}

/// Builds the prompt-composer input used by the runtime bridge.
/// 构建运行时桥接层使用的提示词组合输入。
fn build_runtime_prompt_assembly_input(
    preparation: &ExecutionPreparation,
    active_agent: Option<&RuntimeAgentDefinition>,
) -> PromptAssemblyInput {
    let provider_prompt =
        load_provider_base_system_prompt_with_source(&preparation.model_blueprint)
            .unwrap_or_else(default_provider_base_system_prompt);
    let agent_prompt = active_agent.and_then(build_agent_prompt_snapshot);
    let environment_context = EnvironmentContextSnapshot::new(format!(
        "Current workspace: {}\nCurrent execution mode: {}",
        preparation.workspace_path, preparation.execution_mode
    ));
    let mut custom_developer_segments =
        build_prompt_purpose_developer_segments(preparation.prompt_purpose);
    if let Some(segment) = preparation.prompt_skill_developer_segment.clone() {
        custom_developer_segments.push(segment);
    }

    PromptAssemblyInput {
        mode: match preparation.prompt_purpose {
            ExecutionPromptPurpose::UserTurn => PromptAssemblyMode::Primary,
            ExecutionPromptPurpose::ContextCompaction => PromptAssemblyMode::ContextCompaction,
        },
        model: ModelDescriptor {
            family: preparation.model_blueprint.api_family_key().to_string(),
            model_id: preparation.model_blueprint.model_id.clone(),
            supports_native_personality: true,
        },
        provider_prompt: Some(
            ProviderPromptSnapshot::new(provider_prompt.text)
                .with_source_label(provider_prompt.source_label),
        ),
        agent_prompt,
        personality: persisted_personality_to_prompt_selection(
            &preparation.prompt_configuration.personality,
        ),
        project_rules: load_workspace_project_rules_snapshot(&preparation.workspace_path),
        environment_context: Some(environment_context),
        task_goal: None,
        tool_catalog: ToolCatalogSnapshot::new(
            preparation
                .tool_descriptors
                .iter()
                .map(|tool| ToolDescriptor {
                    id: tool.id.clone(),
                    title: tool.name.clone(),
                    description: tool.description.clone(),
                    help_text: None,
                    schema_fingerprint: None,
                })
                .collect(),
        ),
        subagent: None,
        custom_developer_segments,
        custom_user_segments: load_workspace_project_user_prompt(preparation)
            .into_iter()
            .collect(),
    }
}

/// Builds developer-side runtime instructions required by a special prompt purpose.
/// 构建特殊提示词用途所需的开发者侧运行时指令。
fn build_prompt_purpose_developer_segments(purpose: ExecutionPromptPurpose) -> Vec<String> {
    match purpose {
        ExecutionPromptPurpose::UserTurn => Vec::new(),
        ExecutionPromptPurpose::ContextCompaction => {
            vec![build_context_compaction_developer_segment()]
        }
    }
}

/// Builds the fallback provider prompt used when no resource file is available.
/// 构建没有资源文件可用时使用的 Provider 回退提示词。
fn default_provider_base_system_prompt() -> LoadedProviderBaseSystemPrompt {
    LoadedProviderBaseSystemPrompt {
        text: String::from("You are VulcanCode, an AI coding agent."),
        source_label: String::from("Inline fallback provider prompt"),
    }
}

/// Builds an agent prompt snapshot with a useful runtime source label.
/// 构建带有可用运行时来源标签的 Agent 提示词快照。
fn build_agent_prompt_snapshot(agent: &RuntimeAgentDefinition) -> Option<AgentPromptSnapshot> {
    let text = agent.system_prompt.trim();
    if text.is_empty() {
        return None;
    }
    Some(
        AgentPromptSnapshot::new(text.to_string()).with_source_label(format!(
            "Runtime agent `{}` ({}) system prompt",
            agent.id, agent.name
        )),
    )
}

/// Renders one prompt-composer output into structured provider prompt layers.
/// 将一份提示词组合器输出渲染为结构化 Provider 提示词层。
fn render_runtime_prompt_layers(output: &PromptAssemblyOutput) -> CoreProviderPromptLayers {
    CoreProviderPromptLayers {
        system: output.render_system_text(),
        developer: output.render_developer_text(),
        tool_guidance: output.render_tool_metadata_text(),
        runtime_context: output.render_user_text(),
    }
}

/// Loads one workspace-level project rules snapshot when `AGENTS.md` exists.
/// 当 `AGENTS.md` 存在时加载一份工作区级项目规则快照。
fn load_workspace_project_rules_snapshot(workspace_path: &str) -> Option<ProjectRulesSnapshot> {
    let workspace = workspace_project_rules_path(workspace_path)?;
    let cache = PROJECT_RULES_FILE_CACHE.get_or_init(|| Mutex::new(ProjectRulesFileCache::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.load_workspace_default(workspace).ok().flatten()
}

/// Invalidate the cached workspace-local project prompt snapshot.
/// 使缓存中的工作区本地项目提示词快照失效。
pub(crate) fn invalidate_workspace_project_user_prompt_cache(
    workspace_path: &str,
    project_prompt_path: &str,
) {
    let Some(prompt_path) = workspace_project_prompt_path(
        workspace_path,
        project_prompt_path.trim().trim_start_matches(['/', '\\']),
    ) else {
        return;
    };
    let cache = PROJECT_PROMPT_FILE_CACHE.get_or_init(|| Mutex::new(ProjectPromptFileCache::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.invalidate_file(prompt_path);
}

/// Invalidate the cached workspace-level project rules snapshot.
/// 使缓存中的工作区级项目规则快照失效。
pub(crate) fn invalidate_workspace_project_rules_prompt_cache(workspace_path: &str) {
    let Some(workspace) = workspace_project_rules_path(workspace_path) else {
        return;
    };
    let cache = PROJECT_RULES_FILE_CACHE.get_or_init(|| Mutex::new(ProjectRulesFileCache::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.invalidate_workspace_default(workspace);
}

/// Resolves the workspace directory used by project-rules loading.
/// 解析项目规则加载使用的工作区目录。
fn workspace_project_rules_path(workspace_path: &str) -> Option<&Path> {
    let workspace = Path::new(workspace_path);
    if workspace_path.trim().is_empty() || !workspace.is_dir() {
        return None;
    }
    Some(workspace)
}

/// Resolves one workspace-local project prompt path.
/// 解析单个工作区本地项目提示词路径。
fn workspace_project_prompt_path(
    workspace_path: &str,
    project_prompt_path: &str,
) -> Option<PathBuf> {
    let workspace = Path::new(workspace_path);
    if workspace_path.trim().is_empty()
        || !workspace.is_dir()
        || project_prompt_path.trim().is_empty()
    {
        return None;
    }
    Some(workspace.join(project_prompt_path))
}

/// Loads the workspace-local user prompt configured for prompt composition.
/// 加载为提示词组合配置的工作区本地用户提示词。
fn load_workspace_project_user_prompt(preparation: &ExecutionPreparation) -> Option<String> {
    let project_prompt_configuration = resolve_project_prompt_configuration(preparation);
    if !project_prompt_configuration.project_prompt_enabled {
        return None;
    }
    let workspace = Path::new(&preparation.workspace_path);
    if preparation.workspace_path.trim().is_empty() || !workspace.is_dir() {
        return None;
    }
    let relative_path = project_prompt_configuration
        .project_prompt_path
        .trim()
        .trim_start_matches(['/', '\\']);
    if relative_path.is_empty() {
        return None;
    }
    let prompt_path = workspace_project_prompt_path(&preparation.workspace_path, relative_path)?;
    let cache = PROJECT_PROMPT_FILE_CACHE.get_or_init(|| Mutex::new(ProjectPromptFileCache::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache
        .load_file(&prompt_path)
        .ok()
        .flatten()
        .map(|snapshot: ProjectPromptSnapshot| {
            let text = snapshot.text.trim();
            format!(
                "# Project User Prompt\nSource: {}\n\n{}",
                snapshot.source, text
            )
        })
}

/// Resolve the effective project-prompt configuration for one workspace.
/// 解析单个工作区的生效项目提示词配置。
fn resolve_project_prompt_configuration(
    preparation: &ExecutionPreparation,
) -> PersistedPromptConfiguration {
    let workspace = Path::new(&preparation.workspace_path);
    if preparation.workspace_path.trim().is_empty() || !workspace.is_dir() {
        return preparation.prompt_configuration.clone();
    }
    let mut effective = preparation.prompt_configuration.clone();
    if let Some(project_configuration) = load_workspace_project_prompt_configuration(workspace) {
        effective.project_prompt_enabled = project_configuration.project_prompt_enabled;
        effective.project_prompt_path = project_configuration.project_prompt_path;
    }
    effective
}

/// Load one workspace-local prompt configuration override when present.
/// 当存在时加载一个工作区本地提示词配置覆盖。
fn load_workspace_project_prompt_configuration(
    workspace: &Path,
) -> Option<PersistedPromptConfiguration> {
    load_project_configuration_file(workspace).ok()?.prompt
}

/// Builds the legacy fallback preamble when the new composer cannot assemble.
/// 当新组合器无法装配时构建旧版回退系统前导语。
fn build_legacy_runtime_system_preamble(
    preparation: &ExecutionPreparation,
    composer_error: Option<String>,
) -> String {
    let mut sections = vec![
        load_provider_base_system_prompt(&preparation.model_blueprint)
            .unwrap_or_else(|| String::from("You are VulcanCode, an AI coding agent.")),
        format!("Current workspace: {}", preparation.workspace_path),
        format!("Current execution mode: {}", preparation.execution_mode),
    ];
    if let Some(error) = composer_error {
        sections.push(format!(
            "Prompt composer fallback: runtime prompt assembly failed: {error}"
        ));
    }
    sections.join("\n")
}

/// Resolves one optional max-turn override for multi-turn tool-calling providers.
/// 为支持多轮工具调用的 Provider 解析一份可选的最大轮次覆盖值。
pub(crate) fn resolve_runtime_max_turns(
    active_agent: Option<&RuntimeAgentDefinition>,
    prompt_purpose: ExecutionPromptPurpose,
) -> Option<usize> {
    if let Some(max_turns) = prompt_purpose.max_turns_override() {
        return Some(max_turns);
    }
    active_agent
        .and_then(|agent| agent.max_turns)
        .filter(|value| *value > 0)
        .map(|value| value as usize)
}

#[cfg(test)]
mod tests {
    //! Provider runtime context tests.
    //! Provider 运行时上下文测试。

    use super::*;
    use crate::context_budget::ContextBudgetSnapshot;
    use crate::model::SessionId;
    use crate::provider_config::{
        PersistedAiToolConfiguration, PersistedPromptConfiguration,
        PersistedPromptPersonalitySelection, PersistedProviderEntry, PersistedProviderOptions,
        ProjectVulcanCodeConfigurationFile, ProviderConfigurationFile, ReasoningEffortLevel,
        save_project_configuration_file,
    };
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use vulcan_context::{ContextAssembly, ContextBaseline};
    use vulcan_prompt_composer::DEFAULT_PROJECT_RULES_FILE_NAME;

    /// Verifies the project-rules loader reads a non-empty workspace AGENTS.md file.
    /// 验证项目规则加载器会读取非空的工作区 AGENTS.md 文件。
    #[test]
    fn project_rules_loader_reads_workspace_agents_file() {
        let root = unique_temp_dir("vulcan-project-rules-read");
        fs::create_dir_all(&root).expect("create temp workspace");
        let rules_path = root.join(DEFAULT_PROJECT_RULES_FILE_NAME);
        fs::write(&rules_path, "Rule A\n").expect("write project rules");

        let snapshot = load_workspace_project_rules_snapshot(&root.display().to_string())
            .expect("project rules should load");

        assert_eq!(snapshot.text, "Rule A");
        assert_eq!(snapshot.source, rules_path.display().to_string());

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies missing or empty project rules are not injected.
    /// 验证缺失或空白项目规则不会被注入。
    #[test]
    fn project_rules_loader_skips_missing_or_empty_agents_file() {
        let missing_root = unique_temp_dir("vulcan-project-rules-missing");
        fs::create_dir_all(&missing_root).expect("create missing workspace");
        assert!(
            load_workspace_project_rules_snapshot(&missing_root.display().to_string()).is_none()
        );

        let empty_root = unique_temp_dir("vulcan-project-rules-empty");
        fs::create_dir_all(&empty_root).expect("create empty workspace");
        fs::write(empty_root.join(DEFAULT_PROJECT_RULES_FILE_NAME), " \n\t")
            .expect("write empty project rules");
        assert!(load_workspace_project_rules_snapshot(&empty_root.display().to_string()).is_none());

        let _ = fs::remove_dir_all(missing_root);
        let _ = fs::remove_dir_all(empty_root);
    }

    /// Verifies external watcher events can invalidate the shared project-rules cache.
    /// 验证外部监听事件可以使共享项目规则缓存失效。
    #[test]
    fn project_rules_loader_supports_explicit_cache_invalidation() {
        let root = unique_temp_dir("vulcan-project-rules-invalidate");
        fs::create_dir_all(&root).expect("create temp workspace");
        let rules_path = root.join(DEFAULT_PROJECT_RULES_FILE_NAME);
        fs::write(&rules_path, "Rule before\n").expect("write first rules");

        let first = load_workspace_project_rules_snapshot(&root.display().to_string())
            .expect("first project rules should load");
        assert_eq!(first.text, "Rule before");

        fs::write(&rules_path, "Rule after\n").expect("write second rules");
        invalidate_workspace_project_rules_prompt_cache(&root.display().to_string());

        let second = load_workspace_project_rules_snapshot(&root.display().to_string())
            .expect("second project rules should load");
        assert_eq!(second.text, "Rule after");

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies workspace project prompts enter the runtime user-context layer.
    /// 验证工作区项目提示词会进入运行时用户上下文层。
    #[test]
    fn runtime_prompt_layers_include_workspace_project_prompt() {
        let root = unique_temp_dir("vulcan-project-user-prompt");
        let prompt_dir = root.join(".vulcan").join("code");
        fs::create_dir_all(&prompt_dir).expect("create project prompt directory");
        fs::write(prompt_dir.join("prompt.md"), "Project user instruction")
            .expect("write project prompt");

        let preparation = build_test_preparation(
            root.display().to_string(),
            PersistedPromptConfiguration::default(),
        );
        let layers = build_runtime_prompt_layers_for_agent(&preparation, None);

        assert!(
            layers
                .provider_layers
                .runtime_context
                .contains("Project user instruction")
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies workspace-local prompt configuration can override the global project prompt path.
    /// 验证工作区本地提示词配置可以覆盖全局项目提示词路径。
    #[test]
    fn runtime_prompt_layers_use_workspace_project_prompt_configuration() {
        let root = unique_temp_dir("vulcan-project-prompt-config");
        let prompt_dir = root.join(".vulcan").join("code");
        fs::create_dir_all(&prompt_dir).expect("create project prompt directory");
        fs::write(
            prompt_dir.join("custom-prompt.md"),
            "Project override instruction",
        )
        .expect("write project override prompt");
        fs::write(
            prompt_dir.join("config.json"),
            r#"{"prompt":{"projectPromptEnabled":true,"projectPromptPath":".vulcan/code/custom-prompt.md"}}"#,
        )
        .expect("write project prompt config");

        let preparation = build_test_preparation(
            root.display().to_string(),
            PersistedPromptConfiguration {
                project_prompt_enabled: false,
                ..PersistedPromptConfiguration::default()
            },
        );
        let layers = build_runtime_prompt_layers_for_agent(&preparation, None);

        assert!(
            layers
                .provider_layers
                .runtime_context
                .contains("Project override instruction")
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies workspace-local prompt configuration does not override the global personality choice.
    /// 验证工作区本地提示词配置不会覆盖全局人格选择。
    #[test]
    fn runtime_prompt_layers_keep_global_personality_when_project_prompt_overrides() {
        let root = unique_temp_dir("vulcan-project-prompt-personality");
        let prompt_dir = root.join(".vulcan").join("code");
        fs::create_dir_all(&prompt_dir).expect("create project prompt directory");
        fs::write(prompt_dir.join("prompt.md"), "Project prompt").expect("write project prompt");
        let project_config = ProjectVulcanCodeConfigurationFile {
            prompt_skills: Default::default(),
            prompt: Some(PersistedPromptConfiguration {
                personality: PersistedPromptPersonalitySelection::Default,
                project_prompt_enabled: true,
                project_prompt_path: String::from(".vulcan/code/prompt.md"),
            }),
            vmm: Default::default(),
        };
        save_project_configuration_file(&root, &project_config)
            .expect("write project prompt config");

        let preparation = build_test_preparation(
            root.display().to_string(),
            PersistedPromptConfiguration {
                personality: PersistedPromptPersonalitySelection::Disabled,
                ..PersistedPromptConfiguration::default()
            },
        );
        let layers = build_runtime_prompt_layers_for_agent(&preparation, None);

        assert_eq!(
            layers
                .provider_layers
                .system
                .matches("You optimize for team morale")
                .count(),
            0
        );
        assert!(
            layers
                .provider_layers
                .runtime_context
                .contains("Project prompt")
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies persisted personality settings can disable personality injection.
    /// 验证持久化人格设置可以禁用人格注入。
    #[test]
    fn runtime_prompt_layers_respect_disabled_personality_setting() {
        let prompt_configuration = PersistedPromptConfiguration {
            personality: PersistedPromptPersonalitySelection::Disabled,
            ..PersistedPromptConfiguration::default()
        };
        let preparation = build_test_preparation(String::from("D:/missing"), prompt_configuration);
        let layers = build_runtime_prompt_layers_for_agent(&preparation, None);

        assert_eq!(
            layers
                .provider_layers
                .system
                .matches("You optimize for team morale")
                .count(),
            0
        );
    }

    /// Verifies context-compaction prompts use the prompt composer instead of a post-launch system suffix.
    /// 验证上下文压缩提示词会使用提示词组合器，而不是启动后追加系统提示词后缀。
    #[test]
    fn runtime_prompt_layers_for_context_compaction_use_developer_layer() {
        let mut preparation = build_test_preparation(
            String::from("D:/missing"),
            PersistedPromptConfiguration::default(),
        );
        preparation.prompt_purpose = ExecutionPromptPurpose::ContextCompaction;
        preparation.tool_descriptors = vec![vulcan_tools_manager::VulcanToolDescriptor {
            id: String::from("shell_command"),
            name: String::from("shell_command"),
            description: String::from("Run one shell command"),
            source: vulcan_tools_manager::VulcanToolSourceKind::Native,
        }];

        let layers = build_runtime_prompt_layers_for_agent(&preparation, None);

        assert!(
            layers
                .provider_layers
                .developer
                .contains("Internal host task: summarize earlier conversation context")
        );
        assert!(
            !layers
                .provider_layers
                .tool_guidance
                .contains("shell_command")
        );
        assert!(layers.provider_layers.system.contains("VulcanCode"));
    }

    /// Builds one minimal execution preparation for prompt-layer tests.
    /// 为提示词层测试构建一份最小执行准备。
    fn build_test_preparation(
        workspace_path: String,
        prompt_configuration: PersistedPromptConfiguration,
    ) -> ExecutionPreparation {
        ExecutionPreparation {
            session_id: SessionId::nil(),
            session_title: String::from("test"),
            agent_id: String::from("default"),
            agent_name: String::from("Default"),
            agent_kind: String::from("Primary"),
            model_blueprint: sample_model_blueprint(),
            prompt_preview: String::new(),
            workspace_path,
            conversation_attachments: Vec::new(),
            context_history_count: 0,
            execution_mode: String::from("Build"),
            temperature: None,
            prompt_configuration,
            prompt_purpose: ExecutionPromptPurpose::UserTurn,
            prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            implicit_luaskill_prompt_entries: Vec::new(),
            prompt_skill_developer_segment: None,
            skill_names: Vec::new(),
            tool_descriptors: Vec::new(),
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            provider_wire_request_json: None,
            context_budget: Option::<ContextBudgetSnapshot>::None,
            context_assembly: Option::<ContextAssembly>::None,
            context_baseline: Option::<ContextBaseline>::None,
            context_request_mode: None,
            context_injection: None,
            project_vmm: Default::default(),
            session_vmm: Default::default(),
        }
    }

    /// Builds one real provider blueprint through the catalog resolver.
    /// 通过目录解析器构建一份真实供应商蓝图。
    fn sample_model_blueprint() -> crate::agent_runtime::provider::ProviderRuntimeBlueprint {
        let mut provider = BTreeMap::new();
        provider.insert(
            String::from("deepseek"),
            PersistedProviderEntry {
                name: Some(String::from("DeepSeek")),
                options: PersistedProviderOptions {
                    api_key: Some(String::from("test-key")),
                    ..PersistedProviderOptions::default()
                },
                ..PersistedProviderEntry::default()
            },
        );
        let config = ProviderConfigurationFile {
            model: Some(String::from("deepseek/deepseek-v4-pro")),
            provider,
            ..ProviderConfigurationFile::default()
        };
        crate::agent_runtime::provider::resolve_active_provider_blueprint_with_ai_tools(
            &config,
            ReasoningEffortLevel::Off,
            &PersistedAiToolConfiguration::default(),
        )
        .expect("provider blueprint should resolve")
    }

    /// Builds one unique temporary directory for tests.
    /// 为测试构建一个唯一临时目录。
    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{suffix}"))
    }
}
