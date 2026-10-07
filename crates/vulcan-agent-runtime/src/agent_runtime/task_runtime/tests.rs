use super::*;
use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::execution_models::ExecutionPromptPurpose;
use crate::conversation_protocol::SessionConversationState;
use crate::provider_config::{ProjectPromptSkillsConfiguration, ReasoningEffortLevel};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Reserves one test execution and attaches its provider abort handle.
/// 预留一个测试执行并附加其 Provider 中止句柄。
///
/// # Parameters
/// 参数
/// `task_runtime` owns the active-run reservation table.
/// `task_runtime` 持有活动运行预留表。
/// `task_id` identifies the stable child task.
/// `task_id` 标识稳定子任务。
/// `execution_id` identifies the concrete test execution.
/// `execution_id` 标识具体测试执行。
/// `handle` aborts the pending provider future used by the test.
/// `handle` 用于中止测试中的待定 Provider future。
///
/// # Returns
/// 返回值
/// The ownership guard for the exact reserved execution.
/// 返回精确预留执行的所有权守卫。
pub(super) fn install_test_active_task_run(
    task_runtime: &Arc<TaskRuntime>,
    task_id: SessionId,
    execution_id: &str,
    handle: AbortHandle,
) -> ActiveTaskRunGuard {
    let guard = task_runtime
        .reserve_active_task_run(task_id, execution_id)
        .expect("test active run should reserve");
    guard
        .attach_abort_handle(handle)
        .expect("test active run abort handle should attach");
    guard
}

/// Builds one minimal controller configuration for task-runtime prompt tests.
/// 为 task-runtime 提示词测试构建一份最小控制器配置。
fn build_test_controller_config(system_prompt: &str) -> CoreControllerConfig {
    CoreControllerConfig {
        provider_id: String::from("provider"),
        model_id: String::from("model"),
        system_prompt: system_prompt.to_string(),
        prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
            system_prompt.to_string(),
        ),
        temperature: None,
        max_turns: 8,
        additional_params: None,
        context_injection: None,
    }
}

/// Verifies host payload status parsing rejects unknown execution states.
/// 验证宿主载荷状态解析会拒绝未知执行状态。
#[test]
fn task_session_status_from_execution_status_text_rejects_unknown_status() {
    let status = task_session_status_from_execution_status_text("almost_done");

    assert!(
        status.is_err(),
        "unknown execution status must not be projected as running"
    );
}

/// Verifies failed TaskSession states are treated as cached terminal records.
/// 验证失败的 TaskSession 状态会被视为可缓存读取的终态记录。
#[test]
fn failed_task_session_status_is_cached_terminal_state() {
    let record = build_task_session_record(TaskSessionRecordInit {
        task_id: Uuid::new_v4(),
        public_task_id: String::from("failed-reviewer"),
        origin_session_id: Uuid::new_v4(),
        parent_task_id: None,
        depth: 1,
        description: Some(String::from("failed reviewer")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("review failed"),
        display_title: String::from("Failed reviewer"),
        controller_config: None,
        snapshot: CoreConversationSnapshot::default(),
        last_prompt: String::from("review failed"),
        last_status: String::from("failed"),
    });

    assert!(task_session_record_has_cached_status(&record));
}

/// Builds a resolved TaskSession configuration for controller-config tests.
/// 为控制器配置测试构建已解析 TaskSession 配置。
fn test_resolved_config(
    base: &CoreControllerConfig,
    invocation: &TaskInvocationConfig,
) -> TaskSessionResolvedConfig {
    TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
        global_default_config: SessionEffectiveConfig::default(),
        parent_config: Some(core_controller_config_to_effective_config(base)),
        profile: None,
        invocation: invocation.clone(),
    })
}

/// Builds one minimal child-agent argument object for task-runtime tests.
/// 为 task-runtime 测试构建一份最小子代理参数对象。
fn test_task_args(prompt: &str, task_id: Option<&str>) -> TaskToolInvocationArgs {
    TaskToolInvocationArgs {
        prompt: prompt.to_string(),
        description: None,
        task_id: task_id.map(str::to_string),
        subagent: None,
        agent_mode: None,
        subagent_profile_id: None,
        inherit_parent_config: None,
        system_prompt: None,
        system_prompt_append: None,
        provider: None,
        model: None,
        execution_mode: None,
        provider_model: None,
        tools: None,
        skills: None,
        prompt_skills: None,
        luaskills: None,
        mcp: None,
        context_compaction: None,
        runtime_policy: None,
        extra: None,
    }
}

mod lifecycle_binding;
mod preparation_config;
mod registry_mailbox;
mod runtime_flow;
mod tool_args;
