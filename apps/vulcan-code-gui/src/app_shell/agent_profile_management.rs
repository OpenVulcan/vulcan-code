//! User agent profile management helpers for the workbench shell.
//! 工作台 Shell 的用户 Agent 配置管理辅助逻辑。

use super::*;

impl VulcanWorkbench {
    /// Resolves the initial user-agent identifier selected by the dedicated Agent management panel.
    /// 解析独立 Agent 管理面板在启动时应选中的初始用户 Agent 标识。
    pub(super) fn resolve_initial_user_agent_profile_editor_id(
        catalog: &AgentProfileCatalog,
    ) -> Option<String> {
        catalog
            .state
            .selected_agent_id
            .as_deref()
            .filter(|agent_id| catalog.agents.contains_key(*agent_id))
            .map(str::to_string)
            .or_else(|| catalog.agents.keys().next().cloned())
    }

    /// Reloads the editable user-agent form from the currently selected user-agent identifier.
    /// 根据当前选中的用户 Agent 标识重新载入可编辑用户 Agent 表单。
    pub(super) fn reload_selected_user_agent_profile_form(&mut self) {
        self.agent_profile_form = self
            .selected_user_agent_profile_id
            .as_deref()
            .and_then(|agent_id| self.agent_profile_catalog.agents.get(agent_id))
            .map(AgentProfileFormState::from_profile)
            .unwrap_or_default();
    }

    /// Reloads the user-agent catalog from disk and restores the editor selection when possible.
    /// 从磁盘重新载入用户 Agent 目录，并在可能时恢复编辑器选中状态。
    pub(super) fn reload_agent_profile_catalog(
        &mut self,
        preferred_agent_id: Option<String>,
    ) -> Result<(), String> {
        self.agent_profile_catalog = agent_profile::load_agent_profile_catalog()?;
        self.selected_user_agent_profile_id = preferred_agent_id
            .filter(|agent_id| self.agent_profile_catalog.agents.contains_key(agent_id))
            .or_else(|| {
                Self::resolve_initial_user_agent_profile_editor_id(&self.agent_profile_catalog)
            });
        self.reload_selected_user_agent_profile_form();
        Ok(())
    }

    /// Returns whether the currently selected runtime agent must always execute in Plan mode.
    /// 返回当前选中的运行时代理是否必须始终以 Plan 模式执行。
    pub(super) fn selected_agent_requires_plan_mode(&self) -> bool {
        matches!(
            self.agent_profile_catalog.state.selected_agent_id.as_deref(),
            Some(agent_id) if agent_id == PLAN_AGENT_ID || agent_id == EXPLORE_AGENT_ID
        )
    }

    /// Builds one persisted user-agent profile from the editable Agent management form.
    /// 根据可编辑 Agent 管理表单构建一条可持久化的用户 Agent 配置。
    pub(super) fn build_user_agent_profile_from_form(
        &self,
    ) -> Result<agent_profile::UserAgentProfile, String> {
        let name = self.agent_profile_form.name.trim();
        if name.is_empty() {
            return Err(String::from("请先填写用户 Agent 的显示名称。"));
        }

        let prompt = self.agent_profile_form.prompt_buffer.text();
        if prompt.trim().is_empty() {
            return Err(String::from("请先填写用户 Agent 的主提示词 Markdown。"));
        }

        let normalized_id = trim_to_option(&self.agent_profile_form.id)
            .map(|value| agent_profile::canonicalize_agent_identifier(&value))
            .filter(|value| !value.is_empty())
            .or_else(|| {
                let derived = agent_profile::canonicalize_agent_identifier(name);
                (!derived.is_empty()).then_some(derived)
            })
            .ok_or_else(|| {
                String::from("无法根据当前名称生成稳定标识，请显式填写一个英文或 slug 风格的标识。")
            })?;
        if agent_profile::is_reserved_agent_identifier(&normalized_id) {
            return Err(String::from(
                "该 Agent 标识已被系统内置 Agent 保留，请显式填写一个不同的英文或 slug 标识。",
            ));
        }

        let temperature = match trim_to_option(&self.agent_profile_form.temperature_text) {
            Some(value) => Some(
                value
                    .parse::<f32>()
                    .map_err(|_| String::from("温度覆盖必须是合法的小数，例如 `0.20`。"))?,
            ),
            None => None,
        };

        let max_turns = match trim_to_option(&self.agent_profile_form.max_turns_text) {
            Some(value) => Some(
                value
                    .parse::<u16>()
                    .map_err(|_| String::from("最大轮次必须是合法的正整数。"))?,
            ),
            None => None,
        };

        Ok(agent_profile::UserAgentProfile {
            id: normalized_id,
            name: name.to_string(),
            description: self.agent_profile_form.description.trim().to_string(),
            prompt,
            model: trim_to_option(&self.agent_profile_form.model),
            temperature,
            max_turns,
        })
    }

    /// Saves the currently edited user-agent profile and refreshes the runtime-facing agent catalog.
    /// 保存当前编辑中的用户 Agent 配置，并刷新面向运行时的 Agent 目录。
    pub(super) fn save_selected_user_agent_profile(&mut self) -> Result<String, String> {
        let profile = self.build_user_agent_profile_from_form()?;
        let previous_id = self.selected_user_agent_profile_id.clone();
        agent_profile::save_user_agent_profile(&profile, previous_id.as_deref())?;

        if previous_id.is_some()
            && self.agent_profile_catalog.state.selected_agent_id == previous_id
        {
            self.agent_profile_catalog.state.selected_agent_id = Some(profile.id.clone());
            agent_profile::save_agent_profile_state(&self.agent_profile_catalog.state)?;
        }

        self.reload_agent_profile_catalog(Some(profile.id.clone()))?;
        self.refresh_runtime_foundation();

        Ok(profile.name)
    }

    /// Deletes the currently selected user-agent profile and refreshes the runtime-facing agent catalog.
    /// 删除当前选中的用户 Agent 配置，并刷新面向运行时的 Agent 目录。
    pub(super) fn remove_selected_user_agent_profile(&mut self) -> Result<String, String> {
        let selected_agent_id = self.selected_user_agent_profile_id.clone().ok_or_else(|| {
            String::from("当前没有选中的用户 Agent，可先从左侧列表选择一条配置。")
        })?;
        let removed_name = self
            .agent_profile_catalog
            .agents
            .get(&selected_agent_id)
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| selected_agent_id.clone());

        agent_profile::delete_user_agent_profile(&selected_agent_id)?;

        if self
            .agent_profile_catalog
            .state
            .selected_agent_id
            .as_deref()
            == Some(selected_agent_id.as_str())
        {
            self.agent_profile_catalog.state.selected_agent_id = None;
            agent_profile::save_agent_profile_state(&self.agent_profile_catalog.state)?;
        }

        self.reload_agent_profile_catalog(None)?;
        self.refresh_runtime_foundation();

        Ok(removed_name)
    }
}
