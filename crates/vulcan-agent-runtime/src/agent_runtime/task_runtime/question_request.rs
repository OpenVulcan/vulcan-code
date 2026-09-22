//! Restores child-owned configuration into validated business and auxiliary requests.
//! 将子代理自有配置恢复到经过校验的业务及辅助请求。

use super::*;
use crate::agent_runtime::execution_models::{ExecutionPromptPurpose, ExecutionTurnStartMode};
use vulcan_session_core::v1::child_execution::ChildExecutionProfile;

/// Restores exact child settings without changing business task state or conversation history.
/// 恢复精确子设置，不改变业务任务状态或会话历史。
/// `request` owns current host ports and child history; `profile` and `generation` come from its canonical document.
/// `request` 持有当前宿主端口和子历史；`profile` 与 `generation` 来自其规范文档。
/// Returns an explicit error before mutation for invalid ownership or incomplete captured settings.
/// 所有权无效或已捕获设置不完整时，在修改前返回明确错误。
pub fn restore_child_question_request(
    request: &mut ExecutionPreparationRequest,
    profile: &ChildExecutionProfile,
    generation: u64,
) -> Result<(), String> {
    profile
        .validate_owner(&request.session_id.to_string(), generation)
        .map_err(|error| error.to_string())?;
    if request.prompt_purpose != ExecutionPromptPurpose::QuestionExecution
        || !request.prompt.is_empty()
        || request.continuation_action_id.is_some()
        || request.provider_request_continuation.is_some()
    {
        return Err(String::from(
            "Child question restoration requires a fresh auxiliary request without business input.",
        ));
    }
    restore_child_execution_settings(request, profile)?;
    request.conversation_hook_turn_id = None;
    request.native_goal_runtime = None;
    request.session_goal_context = None;
    request.session_plan_context = None;
    Ok(())
}

/// Restores a prepared business request from its exact canonical child source without adding input.
/// 从精确规范子来源恢复已准备业务请求，不增加输入。
/// `document` must contain full durable history; `execution_id` is the parent-owned execution.
/// `document` 必须包含完整可靠历史；`execution_id` 为父侧所属执行。
/// Returns before mutation on invalid ownership; live host ports remain and no main-session Hook is inherited.
/// 归属无效时在修改前返回；保留实时宿主端口，不继承主会话 Hook。
pub fn restore_child_business_request(
    request: &mut ExecutionPreparationRequest,
    document: &vulcan_session_core::v1::document::SessionDocument,
    execution_id: &vulcan_session_core::v1::collaboration::CollaborationExecutionId,
) -> Result<(), String> {
    // Business recovery has no new root action and never borrows a physical transport continuation.
    // 业务恢复没有新根动作，也绝不借用物理传输续接。
    if request.prompt_purpose != ExecutionPromptPurpose::AgentTask
        || !request.prompt.is_empty()
        || request.continuation_action_id.is_some()
        || request.provider_request_continuation.is_some()
        || request.conversation_hook_turn_id.is_some()
        || request.turn_start_mode == Some(ExecutionTurnStartMode::UserPrompt)
        || document.metadata.session_id != request.session_id.to_string()
        || request.native_goal_runtime.is_some()
        || request.session_goal_context.is_some()
        || request.session_plan_context.is_some()
    {
        return Err(String::from(
            "Child business recovery requires its canonical session without new input, transport continuation or parent Hook/Goal/Plan state.",
        ));
    }
    // The document proves the exact open execution before any settings are restored.
    // 文档在恢复任何设置前证明精确开放执行。
    let profile = document
        .child_business_recovery_profile(execution_id)
        .map_err(|error| error.to_string())?;
    restore_child_execution_settings(request, profile)?;
    request.turn_start_mode = Some(ExecutionTurnStartMode::RestoredCheckpoint);
    Ok(())
}

/// Restores captured settings shared by child business and auxiliary request preparation.
/// 恢复子业务和辅助请求准备共用的捕获设置。
/// `request` already has validated purpose and ownership; `profile` supplies exact non-secret configuration.
/// `request` 已校验用途和归属；`profile` 提供精确非敏感配置。
/// All fallible conversion precedes mutation; lifecycle and history fields remain caller-owned.
/// 所有可能失败的转换先于修改；生命周期和历史字段继续由调用方拥有。
fn restore_child_execution_settings(
    request: &mut ExecutionPreparationRequest,
    profile: &ChildExecutionProfile,
) -> Result<(), String> {
    // Parse every fallible field first so a malformed record cannot partially retarget a request.
    // 先解析全部可能失败的字段，避免格式错误的记录部分改变请求目标。
    let root_id = Uuid::parse_str(&profile.root_session_id)
        .map_err(|error| format!("Invalid child root identity: {error}"))?;
    let provider = required_setting(
        profile
            .effective_config
            .provider_model
            .provider_id
            .as_deref(),
        "provider",
    )?;
    let model = required_setting(
        profile.effective_config.provider_model.model_id.as_deref(),
        "model",
    )?;
    let agent = required_setting(profile.execution_agent_id.as_deref(), "agent")?;
    let execution_mode = required_setting(
        profile
            .effective_config
            .provider_model
            .execution_mode
            .as_deref(),
        "execution mode",
    )?;
    let reasoning = required_setting(
        profile
            .effective_config
            .provider_model
            .reasoning_effort
            .as_deref(),
        "reasoning preference",
    )?;
    let reasoning_effort = serde_json::from_value(Value::String(reasoning.to_owned()))
        .map_err(|error| format!("Invalid captured child reasoning preference: {error}"))?;
    // Host ports remain live and owner-scoped; the captured profile restores only child-owned settings.
    // 宿主端口保持实时且按所有者绑定；捕获配置只恢复子代理自有设置。
    request.execution_scope = ExecutionScope::TaskSession {
        root_session_id: SessionNodeId::from(root_id),
        task_session_id: SessionNodeId::from(request.session_id),
    };
    request.runtime_policy = profile.runtime_policy;
    request.execution_agent_id = Some(agent.to_owned());
    request.provider_id_override = Some(provider.to_owned());
    request.model_id_override = Some(model.to_owned());
    // Resolve the captured child's exact connection using current secrets; never reselect another account.
    // 使用当前秘密解析捕获的子精确连接；绝不重新选择其他账号。
    request.connection_id_override = Some(profile.connection_id.clone());
    request.provider_route_lease_identity = None;
    request.execution_mode = execution_mode.to_owned();
    request.reasoning_effort = reasoning_effort;
    request.temperature = profile.temperature;
    request.agent_prompt_snapshot = profile
        .fixed_agent_prompt
        .as_deref()
        .and_then(ExecutionAgentPromptSnapshot::fixed_parent_runtime);
    request.subagent_prompt_context = Some(ExecutionSubagentPromptContext {
        identity: profile.prompt_context.identity.clone(),
        delegated_task: profile.prompt_context.delegated_task.clone(),
        delegation: profile.prompt_context.delegation.clone(),
        functional_system_prompt: profile.prompt_context.functional_system_prompt.clone(),
    });
    request.tool_exposure_override = profile.effective_config.tool_exposure.clone();
    request.captured_child_tool_admission = Some(profile.admitted_tools.clone());
    restore_child_request_overrides(request, &profile.request_overrides);
    request.session_prompt_skill_overrides.clear();
    apply_task_prompt_skill_snapshot(request, &profile.effective_config.prompt_skills);
    request.context_compaction_policy = Some(task_context_compaction_policy_from_snapshot(
        &profile.effective_config.context_compaction,
    ));
    request.context_budget_override =
        task_context_budget_override_from_snapshot(&profile.effective_config.context_compaction);
    Ok(())
}

/// Requires an exact captured setting instead of resolving it from a new global default.
/// 要求精确的已捕获设置，不从新的全局默认值解析。
/// `name` identifies the missing setting in an explicit host error; valid values are returned unchanged.
/// `name` 在明确宿主错误中标识缺失设置；有效值原样返回。
fn required_setting<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str, String> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Captured child profile is missing its {name}."))
}
