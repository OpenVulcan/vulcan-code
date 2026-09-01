//! Exact retry-limit and reset-boundary tests; physical permit fixtures do not claim provider dispatch.
//! 精确重试上限及重置边界测试；物理许可夹具不宣称发生供应商分派。

use super::*;
use vulcan_ai_contract::{AiExecutionRequest, AiOperation};
use vulcan_core::types::{
    ControllerConfig, ConversationSnapshot, ProviderContinuation, ProviderPromptLayers,
    ProviderRequestSnapshot,
};
use vulcan_session_core::v1::document::PreparedApiStart;
use vulcan_session_core::v1::{ApiCallId, ProviderProjectionProof, Seq};

/// Returns one typed successful-permit fixture solely for testing the stream guard's exact enum discrimination.
/// 返回一份强类型成功许可夹具，仅用于测试流守卫的精确枚举判别。
fn successful_permit_fixture() -> PreparedPhysicalApi {
    PreparedPhysicalApi::SessionV1 {
        exact_request: AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: "model".into(),
            cache_scope_id: None,
            continuation: None,
            messages: Vec::new(),
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: Default::default(),
            parallel_tool_calls: Default::default(),
            structured_output: Default::default(),
            stop_sequences: Vec::new(),
            prompt_cache: Default::default(),
            reasoning_effort: None,
            reasoning_output: Default::default(),
            model_parameters: Default::default(),
            resources: Vec::new(),
            output_token_limit: None,
        },
        controller_config: ControllerConfig::default(),
        controller_snapshot: ConversationSnapshot::default(),
        request_snapshot: ProviderRequestSnapshot {
            provider_id: "provider".into(),
            model_id: "model".into(),
            system_prompt: String::new(),
            prompt_layers: ProviderPromptLayers::default(),
            prompt: String::new(),
            prompt_in_history: true,
            history: Vec::new(),
            temperature: None,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: ProviderContinuation::default(),
        },
        context_budget: None,
        core_start: PreparedApiStart {
            api_call_id: ApiCallId::try_new(1).expect("fixture API id"),
            started_seq: Seq::try_new(2).expect("fixture start seq"),
            projection: ProviderProjectionProof {
                segment_generation: 0,
                document_revision: 1,
                retained_from_seq: Seq::try_new(1).expect("fixture retained seq"),
                through_seq: Seq::try_new(1).expect("fixture through seq"),
                fingerprint_sha256: "0".repeat(64),
            },
            continuation_action_id: None,
        },
        provider_api_trace_store: None,
    }
}

/// Exactly three automatic restarts are available, and repeated denial never wraps or replenishes the counter.
/// 精确允许三次自动重启，重复拒绝不会使计数回绕或补充。
#[test]
fn configuration_restarts_stop_after_three_before_next_permit() {
    // Only the initial stream entry creates a fresh guard.
    // 只有首次流入口创建全新守卫。
    let mut state = StreamRestartState::default();
    for _ in 0..3 {
        assert!(state.try_configuration_restart());
    }
    for _ in 0..256 {
        assert!(!state.try_configuration_restart());
    }
    assert_eq!(state.configuration_restarts, MAX_CONFIGURATION_RESTARTS);
}

/// Stale and failed preparation results do not impersonate a durable API permit.
/// 陈旧及失败准备结果不会冒充可靠API许可。
#[test]
fn stale_or_failed_preparation_keeps_configuration_restart_debt() {
    // The state moves unchanged across both model and compaction-policy refresh routes.
    // 状态跨模型及压缩策略刷新路线保持不变。
    let mut state = StreamRestartState::default();
    for _ in 0..3 {
        assert!(state.try_configuration_restart());
        state.observe_preparation(&Ok(PreparedPhysicalApi::RequestConfigurationChanged));
        state.observe_preparation(&Err("source preparation failed".into()));
    }
    assert!(!state.try_configuration_restart());
}

/// Actual V1 permit discrimination resets configuration debt but does not reopen the independent compaction guard.
/// 实际V1许可判别重置配置消耗，但不重开独立压缩守卫。
#[test]
fn v1_permit_resets_only_configuration_restart_debt() {
    // The prior automatic compaction attempt must remain consumed after a successful provider permit.
    // 成功供应商许可后，先前自动压缩尝试仍必须保持已消耗。
    let mut state = StreamRestartState::default().with_auto_compaction_attempted(true);
    for _ in 0..3 {
        assert!(state.try_configuration_restart());
    }
    state.observe_preparation(&Ok(successful_permit_fixture()));
    assert_eq!(state.configuration_restarts, 0);
    assert!(state.auto_compaction_already_attempted);
    assert!(state.try_configuration_restart());
}

/// Toggling the existing compaction guard during restarts cannot erase configuration churn.
/// 重启期间切换既有压缩守卫不能清除配置抖动。
#[test]
fn compaction_restarts_preserve_configuration_retry_budget() {
    // Match the real restart branches: model/policy refresh clears the compaction flag, compaction sets it.
    // 匹配真实重启分支：模型/策略刷新清除压缩标志，压缩设置它。
    let mut state = StreamRestartState::default();
    for _ in 0..3 {
        assert!(state.try_configuration_restart());
        state = state
            .with_auto_compaction_attempted(true)
            .with_auto_compaction_attempted(false);
    }
    assert!(!state.try_configuration_restart());
}

/// Repeated valid requests can each change models without consuming a turn-global switch quota.
/// 重复有效请求均可切换模型，不会消耗回合全局切换配额。
#[test]
fn successful_requests_do_not_accumulate_a_turn_global_model_switch_limit() {
    // More successful API cycles than the restart limit distinguish progress-based reset from a lifetime quota.
    // 超过重启上限的成功API周期区分基于进展的重置与生命周期配额。
    let mut state = StreamRestartState::default();
    for _ in 0..20 {
        for _ in 0..3 {
            assert!(state.try_configuration_restart());
        }
        state.observe_preparation(&Ok(successful_permit_fixture()));
    }
    assert_eq!(state.configuration_restarts, 0);
}

/// A typed non-session permit has no authority to replenish a V1 session's preparation budget.
/// 强类型非会话许可无权补充V1会话的准备额度。
#[test]
fn excluded_non_session_permit_cannot_clear_v1_configuration_debt() {
    // Reuse the same transport fixture while deliberately removing its V1 ownership variant.
    // 复用相同传输夹具，但明确移除其V1所有权分支。
    let PreparedPhysicalApi::SessionV1 {
        exact_request,
        controller_config,
        controller_snapshot,
        request_snapshot,
        context_budget,
        ..
    } = successful_permit_fixture()
    else {
        unreachable!("fixture is V1")
    };
    // All consumed restart allowance belongs to the V1 owner, not this excluded request.
    // 全部已消耗重启额度属于V1所有者，而非此排除请求。
    let mut state = StreamRestartState::default();
    for _ in 0..3 {
        assert!(state.try_configuration_restart());
    }
    state.observe_preparation(&Ok(PreparedPhysicalApi::ExcludedNonSession {
        exact_request,
        controller_config,
        controller_snapshot,
        request_snapshot,
        context_budget,
    }));
    assert!(!state.try_configuration_restart());
}
