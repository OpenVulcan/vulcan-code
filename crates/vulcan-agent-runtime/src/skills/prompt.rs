//! Prompt Skills runtime configuration adapters.
//! Prompt Skills 运行时配置适配器。

/// Converts persisted desktop prompt-skill settings into the runtime resolver input.
/// 将桌面持久化提示词技能设置转换为运行时解析器输入。
pub fn global_config_from_persisted(
    config: &crate::provider_config::PersistedPromptSkillsConfiguration,
) -> vulcan_prompt_skills::PromptSkillsGlobalConfig {
    let skill_overrides = config
        .skill_overrides
        .iter()
        .map(|(name, state)| (name.clone(), state_from_persisted(*state)))
        .collect();

    vulcan_prompt_skills::PromptSkillsGlobalConfig {
        user_skill_root: None,
        system_skill_root: Some(
            vulcan_runtime_assets::resolve_system_prompt_skills_resource_directory(),
        ),
        default_skill_state: state_from_persisted(config.default_skill_state),
        skill_overrides,
    }
}

/// Converts one persisted prompt-skill state into the runtime state enum.
/// 将单个持久化提示词技能状态转换为运行时状态枚举。
pub fn state_from_persisted(
    state: crate::provider_config::PersistedPromptSkillState,
) -> vulcan_prompt_skills::PromptSkillState {
    match state {
        crate::provider_config::PersistedPromptSkillState::Enabled => {
            vulcan_prompt_skills::PromptSkillState::Enabled
        }
        crate::provider_config::PersistedPromptSkillState::Disabled => {
            vulcan_prompt_skills::PromptSkillState::Disabled
        }
    }
}
