//! Subagent catalog, configuration, and process-local lifecycle contracts.
//! 子代理目录、配置与进程内生命周期契约。

/// Private crash-resistant file replacement used by catalog documents.
/// 子代理目录文档使用的私有抗崩溃文件替换能力。
mod atomic_file;
/// Subagent catalog loading and profile conversion.
/// 子代理目录加载与配置转换。
pub mod catalog;
/// Task-session effective configuration resolution.
/// 子代理会话有效配置解析。
pub mod config_resolver;
/// Process-local task lifecycle models.
/// 进程内子任务生命周期模型。
pub mod model;
/// Subagent profile models.
/// 子代理配置模型。
pub mod profile;

pub use catalog::{
    PLUGIN_SUBAGENT_CATEGORY_ID, SUBAGENT_MODE_DEFAULT, SUBAGENT_MODE_EXPERT, SUBAGENT_MODE_MIRROR,
    SubagentCatalog, SubagentCatalogLoadError, SubagentDefinition, SubagentDefinitionMode,
    SubagentDefinitionScope, SubagentDefinitionSource, display_subagent_category_id,
    ensure_user_subagent_category_directories, load_plugin_subagent_catalog,
    load_subagent_catalog_from_roots, parse_subagent_definition_file,
    parse_subagent_definition_text, subagent_definition_to_markdown, user_subagent_definition_path,
    user_subagent_override_path, user_subagent_sidecar_path, write_user_subagent_definition,
    write_user_subagent_enabled_override, write_user_subagent_metadata_override,
    write_user_subagent_override,
};
pub use config_resolver::{
    TaskInvocationConfig, TaskSessionConfigResolutionInput, TaskSessionConfigResolver,
    TaskSessionResolvedConfig,
};
pub use model::TaskSessionStatus;
pub use profile::SubagentProfile;

#[cfg(test)]
mod tests {
    use super::{
        SubagentProfile, TaskInvocationConfig, TaskSessionConfigResolutionInput,
        TaskSessionConfigResolver,
    };
    use vulcan_session_core::{
        ExecutionRuntimePolicyRequest, ProviderModelSnapshot, SessionEffectiveConfig,
        ToolExposureSnapshot,
    };

    /// Builds a parent configuration used by resolver tests.
    /// 构建解析器测试使用的父级配置。
    fn parent_config() -> SessionEffectiveConfig {
        let mut config = SessionEffectiveConfig {
            provider_model: ProviderModelSnapshot {
                provider_id: Some("deepseek".to_string()),
                model_id: Some("deepseek-v4".to_string()),
                reasoning_effort: Some("high".to_string()),
                execution_mode: Some("build".to_string()),
            },
            system_prompt_layers: vec!["parent prompt".to_string()],
            ..Default::default()
        };
        config.tool_exposure.allowed_tools = vec!["shell_command".to_string()];
        config
    }

    /// Verifies child execution inherits parent configuration while keeping side effects disabled.
    /// 验证子代理执行继承父级配置，同时保持副作用关闭。
    #[test]
    fn task_session_inherits_parent_config_but_disables_side_effects() {
        let resolved = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config: Some(parent_config()),
            profile: None,
            invocation: TaskInvocationConfig::main("review this change"),
        });

        assert!(resolved.effective_config.inherit_parent_config);
        assert_eq!(
            resolved
                .effective_config
                .provider_model
                .provider_id
                .as_deref(),
            Some("deepseek")
        );
        assert_eq!(
            resolved.effective_config.tool_exposure.allowed_tools,
            vec!["shell_command"]
        );
        assert!(!resolved.runtime_policy.hooks_enabled);
        assert!(!resolved.runtime_policy.vmm_enabled);
        assert!(!resolved.runtime_policy.memory_recall_enabled);
        assert!(!resolved.runtime_policy.memory_writeback_enabled);
        assert!(!resolved.runtime_policy.database_projection_enabled);
    }

    /// Verifies explicit child runtime overrides cannot enable parent-session side effects.
    /// 验证明示子代理运行时覆盖无法启用父会话副作用。
    #[test]
    fn task_session_runtime_overrides_keep_side_effects_forbidden() {
        let mut profile = SubagentProfile::new("memory-agent", "Memory agent");
        profile.runtime_policy = ExecutionRuntimePolicyRequest {
            hooks_enabled: Some(true),
            vmm_enabled: Some(true),
            memory_recall_enabled: Some(true),
            memory_writeback_enabled: Some(true),
            database_projection_enabled: Some(true),
        };
        let mut invocation = TaskInvocationConfig::with_profile("memory-agent", "recall context");
        invocation.runtime_policy = ExecutionRuntimePolicyRequest {
            hooks_enabled: Some(false),
            vmm_enabled: Some(true),
            memory_recall_enabled: Some(false),
            memory_writeback_enabled: Some(true),
            database_projection_enabled: Some(true),
        };

        let resolved = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config: Some(parent_config()),
            profile: Some(profile),
            invocation,
        });

        assert!(!resolved.runtime_policy.hooks_enabled);
        assert!(!resolved.runtime_policy.vmm_enabled);
        assert!(!resolved.runtime_policy.memory_recall_enabled);
        assert!(!resolved.runtime_policy.memory_writeback_enabled);
        assert!(!resolved.runtime_policy.database_projection_enabled);
    }

    /// Verifies disabling inheritance prevents parent configuration from leaking in.
    /// 验证关闭继承会阻止父级配置泄漏。
    #[test]
    fn profile_can_disable_parent_inheritance() {
        let mut profile = SubagentProfile::new("agent-review", "Code reviewer");
        profile.inherit_parent_config = Some(false);
        profile.base_system_prompt = Some("profile prompt".to_string());

        let resolved = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config: Some(parent_config()),
            profile: Some(profile),
            invocation: TaskInvocationConfig::with_profile("agent-review", "review"),
        });

        assert!(!resolved.effective_config.inherit_parent_config);
        assert_eq!(resolved.effective_config.provider_model.provider_id, None);
        assert_eq!(
            resolved.effective_config.system_prompt_layers,
            vec!["profile prompt"]
        );
    }

    /// Verifies invocation configuration overrides profile configuration at the final layer.
    /// 验证调用配置会在最终层覆盖子代理配置。
    #[test]
    fn invocation_overrides_profile_settings() {
        let mut profile = SubagentProfile::new("film-dev", "Film developer");
        profile.provider_model.model_id = Some("profile-model".to_string());
        profile.tool_exposure.allowed_tools = vec!["image_generation".to_string()];

        let mut invocation = TaskInvocationConfig::with_profile("film-dev", "make storyboard");
        invocation.provider_model.model_id = Some("invocation-model".to_string());
        invocation.tool_exposure = ToolExposureSnapshot {
            allowed_tools: vec!["video_generation".to_string()],
            ..ToolExposureSnapshot::default()
        };
        invocation.system_prompt = Some("invocation system prompt".to_string());

        let resolved = TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config: Some(parent_config()),
            profile: Some(profile),
            invocation,
        });

        assert_eq!(
            resolved.effective_config.provider_model.model_id.as_deref(),
            Some("invocation-model")
        );
        assert_eq!(
            resolved.effective_config.tool_exposure.allowed_tools,
            vec!["video_generation"]
        );
        assert_eq!(
            resolved.effective_config.system_prompt_layers,
            vec!["invocation system prompt"]
        );
    }
}
