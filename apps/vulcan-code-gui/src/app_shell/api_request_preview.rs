//! API-request preview simulation helpers for the selected session document surface.
//! 面向选中会话文档表面的 API 请求预览模拟辅助模块。

use super::*;
use crate::agent_runtime::runtime_request_capture::capture_runtime_provider_wire_request;

/// Fixed prompt used by the request tab to assemble a diagnostic provider request.
/// 请求页签用于装配诊断 Provider 请求的固定提示词。
const API_REQUEST_SIMULATION_PROMPT: &str = "test request";

impl VulcanWorkbench {
    /// Starts one simulated API request for the selected request tab without mutating conversation history.
    /// 为选中的请求页签启动一次模拟 API 请求，且不修改会话历史。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the session whose current context should be assembled.
    /// `session_id` 标识需要装配当前上下文的会话。
    ///
    /// # Returns
    /// 返回值
    /// An iced task that resolves with the captured request JSON text.
    /// 返回一个携带捕获请求 JSON 文本结果的 iced 任务。
    pub(super) fn start_session_api_request_simulation(
        &mut self,
        session_id: SessionId,
    ) -> Task<Message> {
        self.ensure_session_conversation_state_loaded(session_id);
        self.replace_session_last_api_request_preview_buffer_only(
            session_id,
            build_api_request_simulation_loading_text(session_id),
        );

        let Some(session_title) = self
            .session_by_id(session_id)
            .map(|session| session.title.clone())
        else {
            return Task::perform(
                async move {
                    Err(String::from("当前选中会话不存在，无法模拟 API 请求。"))
                },
                move |result| Message::SessionApiRequestSimulationFinished { session_id, result },
            );
        };

        let runtime_options = self.current_pending_prompt_runtime_options();
        let execution_mode = if self.selected_agent_requires_plan_mode() {
            ExecutionMode::Plan
        } else {
            self.execution_mode
        };
        let ai_tool_configuration =
            self.effective_ai_tool_configuration_for_runtime_options(&runtime_options);
        let config_for_run = self.provider_configuration.clone();
        let system_prompt_hook_phase =
            self.vmm_profile_placement_phase_for_next_user_prompt(session_id, false);
        let session_vmm = self
            .session_runtime_state(session_id)
            .map(|state| state.vmm.clone())
            .unwrap_or_default();
        let project_vmm = self.current_project_vmm_configuration(session_id);
        let effective_vmm_config = vulcan_vmm::VmmEffectiveConfig::resolve(
            &self.provider_configuration.vmm,
            Some(&project_vmm),
            Some(&session_vmm),
        );
        let system_prompt_hook_blocks = self
            .vmm_hook_runtime
            .cached_profile_system_prompt_blocks_for_session(
                session_id,
                &effective_vmm_config,
                system_prompt_hook_phase,
            );
        let request = ExecutionPreparationRequest {
            session_id,
            session_title,
            workspace_path: self
                .workspace_path_for_session_id(session_id)
                .unwrap_or_else(|| self.workspace_path.clone()),
            prompt: API_REQUEST_SIMULATION_PROMPT.to_string(),
            conversation_state: self.session_conversation_state(session_id),
            execution_mode: execution_mode.to_string(),
            temperature: runtime_options.temperature,
            reasoning_effort: runtime_options.reasoning_effort,
            ai_tool_configuration,
            prompt_purpose: crate::agent_runtime::ExecutionPromptPurpose::UserTurn,
            project_prompt_skills: self.current_project_prompt_skills_configuration(session_id),
            session_prompt_skill_overrides: self
                .session_runtime_state(session_id)
                .map(|state| state.prompt_skill_overrides.clone())
                .unwrap_or_default(),
            context_injection: self
                .session_runtime_state(session_id)
                .and_then(|state| {
                    crate::agent_runtime::context_injection_runtime::core_provider_context_injection_from_state_for_turn(
                        &state.context_injection_state,
                        Some("api-request-simulation"),
                    )
                }),
            system_prompt_hook_blocks,
            project_vmm,
            session_vmm,
        };

        let agent_profile_catalog = self.agent_profile_catalog.clone();
        let luaskills_runtime = self.luaskills_runtime.clone();
        let mcp_runtime = self.mcp_runtime.clone();
        let task_runtime = self.runtime_foundation.task_runtime();
        let question_runtime = self.runtime_foundation.question_runtime();
        let command_session_runtime = self.runtime_foundation.command_session_runtime();
        let token_budget_runtime = self.runtime_foundation.token_budget_runtime();

        Task::perform(
            async move {
                let request_json = capture_runtime_provider_wire_request(
                    config_for_run,
                    agent_profile_catalog,
                    luaskills_runtime,
                    mcp_runtime,
                    task_runtime,
                    question_runtime,
                    command_session_runtime,
                    token_budget_runtime,
                    request,
                )
                .await?;
                serde_json::to_string_pretty(&request_json)
                    .map_err(|error| format!("模拟请求 JSON 格式化失败：{error}"))
            },
            move |result| Message::SessionApiRequestSimulationFinished { session_id, result },
        )
    }
}

/// Builds a JSON loading placeholder for the simulated request tab.
/// 为模拟请求页签构建 JSON 形式的加载占位内容。
///
/// # Parameters
/// 参数
/// `session_id` identifies the session being simulated.
/// `session_id` 标识正在模拟的会话。
///
/// # Returns
/// 返回值
/// Pretty JSON text suitable for the request preview editor.
/// 适合请求预览编辑器展示的格式化 JSON 文本。
fn build_api_request_simulation_loading_text(session_id: SessionId) -> String {
    serde_json::to_string_pretty(&json!({
        "status": "assembling",
        "sessionId": session_id.to_string(),
        "prompt": API_REQUEST_SIMULATION_PROMPT,
        "note": "请求页正在按正常链路组装模拟请求；本次不会发送网络请求，也不会写入会话历史。"
    }))
    .unwrap_or_else(|_| String::from("{\"status\":\"assembling\"}"))
}

/// Builds a JSON error payload for the simulated request tab.
/// 为模拟请求页签构建 JSON 形式的错误载荷。
///
/// # Parameters
/// 参数
/// `session_id` identifies the session that failed to simulate.
/// `session_id` 标识模拟失败的会话。
/// `error` is the diagnostic message returned by the capture path.
/// `error` 是捕获路径返回的诊断信息。
///
/// # Returns
/// 返回值
/// Pretty JSON text suitable for the request preview editor.
/// 适合请求预览编辑器展示的格式化 JSON 文本。
pub(super) fn build_api_request_simulation_error_text(
    session_id: SessionId,
    error: &str,
) -> String {
    serde_json::to_string_pretty(&json!({
        "status": "error",
        "sessionId": session_id.to_string(),
        "prompt": API_REQUEST_SIMULATION_PROMPT,
        "error": error
    }))
    .unwrap_or_else(|_| format!("{{\"status\":\"error\",\"error\":{error:?}}}"))
}
