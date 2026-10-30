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

/// Only an actual V1 permit resets configuration debt and re-arms automatic compaction.
/// 只有实际V1许可才重置配置消耗并重新启用自动压缩。
#[test]
fn v1_permit_resets_restart_debt_and_rearms_compaction() {
    let mut state = StreamRestartState::default().with_auto_compaction_attempted(true);
    for _ in 0..3 {
        assert!(state.try_configuration_restart());
    }
    // A rejected or changed candidate proves no progress, so the compaction attempt stays consumed.
    // 被拒绝或已变更的候选不证明进展，因此压缩尝试保持已消耗。
    state.observe_preparation(&Ok(PreparedPhysicalApi::RequestConfigurationChanged));
    state.observe_preparation(&Err("source preparation failed".into()));
    state.observe_preparation(&Ok(PreparedPhysicalApi::ContextCompactionRequired {
        reason: "final budget requires compaction".into(),
    }));
    assert!(state.auto_compaction_already_attempted);
    state.observe_preparation(&Ok(successful_permit_fixture()));
    assert_eq!(state.configuration_restarts, 0);
    assert!(!state.auto_compaction_already_attempted);
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

/// Supplies one admitted auxiliary record for checking immutable stream binding, without claiming a launch.
/// 提供一条已准入辅助记录，用于检查不可变流绑定，不宣称实际启动。
fn question_execution_fixture(in_parent: bool) -> CollaborationQuestionExecutionRecord {
    use vulcan_session_core::v1::collaboration::{
        CollaborationExecutionPurpose, CollaborationOperationId, CollaborationParticipant,
        CollaborationQuestionId,
    };
    // One exact logical turn belongs either to the auxiliary task or its retained parent.
    // 一个精确逻辑回合要么属于辅助任务，要么属于其保留父回合。
    let turn_id = TurnId::try_new(1).unwrap();
    CollaborationQuestionExecutionRecord {
        execution_id: CollaborationExecutionId::try_new("qa-stream").unwrap(),
        operation_id: CollaborationOperationId::try_new("qa-begin").unwrap(),
        question_id: CollaborationQuestionId::try_new("qa-question").unwrap(),
        participant: CollaborationParticipant::new(
            "target",
            "target",
            "project",
            1,
            in_parent.then_some(turn_id),
            None,
        )
        .unwrap(),
        purpose: if in_parent {
            CollaborationExecutionPurpose::InTurnQuestion
        } else {
            CollaborationExecutionPurpose::NaturallyIdleQuestion
        },
        parent_turn_id: in_parent.then_some(turn_id),
        execution_turn_id: (!in_parent).then_some(turn_id),
        state: CollaborationQuestionExecutionState::Running,
        started_at_ms: 1,
        completed_at_ms: None,
        outcome: None,
        queued_successor: None,
    }
}

/// Retries and compaction retain exact QA identity and only a real permit can replace its physical API.
/// 重试和压缩保留精确答疑身份，只有真实许可才能替换其物理 API。
#[test]
fn question_scope_survives_restarts_and_rejected_preparation() {
    for in_parent in [false, true] {
        // The first pre-request failure must carry no invented physical API.
        // 首次请求前失败不能携带虚构物理 API。
        let record = question_execution_fixture(in_parent);
        // The same owned state moves through real restart branches.
        // 相同自有状态经过真实重启分支。
        let mut state = StreamRestartState::for_question_execution(&record).unwrap();
        assert_eq!(
            state.terminal_scope(),
            TerminalSettlementScope::QuestionExecution {
                execution_id: record.execution_id.clone(),
                api_call_id: None,
            }
        );
        state.observe_preparation(&Ok(successful_permit_fixture()));
        // A captured scope also survives the task future's destruction on registration failure.
        // 捕获范围也能在登记失败销毁任务 Future 后继续保留。
        let captured = state.terminal_scope();
        assert!(
            matches!(&captured, TerminalSettlementScope::QuestionExecution { api_call_id: Some(id), .. } if id.value() == 1)
        );
        for _ in 0..3 {
            assert!(state.try_configuration_restart());
            state = state
                .clone()
                .with_auto_compaction_attempted(true)
                .with_auto_compaction_attempted(false);
            state.observe_preparation(&Ok(PreparedPhysicalApi::RequestConfigurationChanged));
            state.observe_preparation(&Err("preparation failed".into()));
            assert_eq!(state.terminal_scope(), captured);
        }
        assert!(!state.try_configuration_restart());
        assert_eq!(
            state.question_execution.as_ref().unwrap().turn_id,
            TurnId::try_new(1).unwrap()
        );
        // A subsequent successful physical request changes only the API identity.
        // 后续成功物理请求只改变 API 身份。
        let mut next = successful_permit_fixture();
        if let PreparedPhysicalApi::SessionV1 { core_start, .. } = &mut next {
            core_start.api_call_id = ApiCallId::try_new(2).unwrap();
        }
        state.observe_preparation(&Ok(next));
        assert_eq!(
            state.terminal_scope(),
            TerminalSettlementScope::QuestionExecution {
                execution_id: record.execution_id,
                api_call_id: Some(ApiCallId::try_new(2).unwrap()),
            }
        );
        assert_eq!(state.configuration_restarts, 0);
    }
}

/// Completed and ambiguous bindings never acquire a fresh question stream owner.
/// 已完成及歧义绑定绝不获得新的答疑流所有者。
#[test]
fn question_scope_rejects_completed_or_ambiguous_execution() {
    // Completion cannot be undone by constructing another stream from an old receipt.
    // 不能通过旧回执构造另一个流来撤销完成状态。
    let mut record = question_execution_fixture(false);
    record.state = CollaborationQuestionExecutionState::Completed;
    assert!(StreamRestartState::for_question_execution(&record).is_err());
    record.state = CollaborationQuestionExecutionState::Running;
    record.parent_turn_id = record.execution_turn_id;
    assert!(StreamRestartState::for_question_execution(&record).is_err());
    record.parent_turn_id = None;
    record.execution_turn_id = None;
    assert!(StreamRestartState::for_question_execution(&record).is_err());
}
