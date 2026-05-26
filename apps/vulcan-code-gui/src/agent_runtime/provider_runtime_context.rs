//! Provider runtime context helpers for system preamble and turn-budget resolution.
//! 面向系统前导语与轮次预算解析的 Provider 运行时上下文辅助模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::agent_runtime::prompt_composer_runtime::{
    record_runtime_prompt_composer_report, runtime_prompt_composer_engine,
};
use crate::agent_runtime::provider_prompt_loader::{
    LoadedProviderBaseSystemPrompt, load_provider_base_system_prompt,
    load_provider_base_system_prompt_with_source,
};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use vulcan_prompt_composer::{
    AgentPromptSnapshot, EnvironmentContextSnapshot, ModelDescriptor, PersonalitySelection,
    ProjectRulesFileCache, ProjectRulesSnapshot, PromptAssemblyInput, PromptAssemblyMode,
    PromptAssemblyOutput, ProviderPromptSnapshot, ToolCatalogSnapshot, ToolDescriptor,
};

/// Process-wide project-rules source cache shared by provider runtime requests.
/// Provider 运行时请求共享的进程级项目规则来源缓存。
static PROJECT_RULES_FILE_CACHE: OnceLock<Mutex<ProjectRulesFileCache>> = OnceLock::new();

/// Builds the effective runtime system prompt sent to the real provider agent.
/// 构建发送给真实 Provider 代理的生效运行时系统提示词。
#[cfg(test)]
pub(crate) fn build_runtime_system_preamble(preparation: &ExecutionPreparation) -> String {
    build_runtime_system_preamble_for_agent(preparation, None)
}

/// Builds the effective runtime system prompt with optional active-agent context.
/// 使用可选当前代理上下文构建生效运行时系统提示词。
pub(crate) fn build_runtime_system_preamble_for_agent(
    preparation: &ExecutionPreparation,
    active_agent: Option<&RuntimeAgentDefinition>,
) -> String {
    let input = build_runtime_prompt_assembly_input(preparation, active_agent);
    let output = match runtime_prompt_composer_engine().compose(&input) {
        Ok(output) => output,
        Err(error) => {
            return build_legacy_runtime_system_preamble(preparation, Some(error.to_string()));
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

    render_runtime_prompt_output(&output, preparation)
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

    PromptAssemblyInput {
        mode: PromptAssemblyMode::Primary,
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
        personality: PersonalitySelection::Default,
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
        custom_developer_segments: Vec::new(),
        custom_user_segments: Vec::new(),
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

/// Renders a composer output into the single provider system-preamble channel used today.
/// 将组合结果渲染为当前 Provider 使用的单一系统前导语通道。
fn render_runtime_prompt_output(
    output: &PromptAssemblyOutput,
    preparation: &ExecutionPreparation,
) -> String {
    let mut sections = Vec::new();
    push_non_empty_section(&mut sections, output.render_system_text());
    push_titled_section(
        &mut sections,
        "Developer Instructions",
        output.render_developer_text(),
    );
    push_titled_section(
        &mut sections,
        "Tool Guidance",
        output.render_tool_metadata_text(),
    );
    push_titled_section(&mut sections, "Runtime Context", output.render_user_text());
    if sections.is_empty() {
        return build_legacy_runtime_system_preamble(preparation, None);
    }
    sections.join("\n\n")
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

/// Push one raw section when it contains visible text.
/// 当原始分节包含可见文本时推入该分节。
fn push_non_empty_section(sections: &mut Vec<String>, body: String) {
    let body = body.trim();
    if !body.is_empty() {
        sections.push(body.to_string());
    }
}

/// Push one titled section when it contains visible text.
/// 当带标题分节包含可见文本时推入该分节。
fn push_titled_section(sections: &mut Vec<String>, title: &str, body: String) {
    let body = body.trim();
    if !body.is_empty() {
        sections.push(format!("## {title}\n{body}"));
    }
}

/// Resolves one optional max-turn override for multi-turn tool-calling providers.
/// 为支持多轮工具调用的 Provider 解析一份可选的最大轮次覆盖值。
pub(crate) fn resolve_runtime_max_turns(
    active_agent: Option<&RuntimeAgentDefinition>,
) -> Option<usize> {
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
    use std::fs;
    use std::path::PathBuf;
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
