//! Auxiliary requests cannot supply or replace business completion and continuation evidence.
//! 辅助请求不能提供或替换业务完成与继续凭据。

use super::*;

/// Only a later business request in the same turn acknowledges an input boundary.
/// 只有同一回合中更晚的业务请求才能确认输入边界已分发。
/// Uses real request transitions for every purpose and checks exact sequence and turn isolation.
/// 使用每种用途的真实请求转换，检查精确序号及回合隔离。
#[test]
fn business_input_dispatch_preserves_auxiliary_and_sequence_boundaries() -> Result<(), DomainError>
{
    for purpose in [
        ApiPurposeKind::MainGeneration,
        ApiPurposeKind::RetryGeneration,
        ApiPurposeKind::Continue,
        ApiPurposeKind::Subagent,
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        // Each scenario retains the input allocated by the canonical root transition.
        // 每种场景保留规范根转换分配的输入。
        let mut document = new_document();
        let turn = document.submit_root_input_fixture("Original work", vec![], T0)?;
        // Continue requires its real interrupted predecessor and explicit supplemental input.
        // Continue 要求真实中断前序及明确的补充输入。
        let input_seq = if purpose == ApiPurposeKind::Continue {
            document.start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                T0 + 1,
            )?;
            document.interrupt_active_api_for_process(T0 + 2)?;
            document
                .continue_open_turn(
                    ContinuationRequest::new(
                        ClientActionId::try_new("continue-dispatch")?,
                        "Continue direction",
                        vec![],
                    ),
                    T0 + 3,
                )?
                .into_value()
                .input_seq
                .unwrap()
        } else {
            document.active_turn().unwrap().start_seq
        };
        assert!(!document.business_input_dispatch_started(turn, input_seq));
        // Real API preparation creates the dispatch fact; auxiliary purposes remain distinct.
        // 真实 API 准备创建分发事实；辅助用途保持独立。
        let api =
            document.start_api(purpose, ApiRequestSnapshot::new("openai", "gpt-5"), T0 + 4)?;
        // An input at the request boundary itself was not present before that request.
        // 位于请求边界本身的输入并非该请求之前已经存在的输入。
        let started_seq = document.active_api().unwrap().started_seq;
        // Expected business purposes are enumerated by the public request contract.
        // 预期业务用途按公开请求契约枚举。
        let business = matches!(
            purpose,
            ApiPurposeKind::MainGeneration
                | ApiPurposeKind::RetryGeneration
                | ApiPurposeKind::Continue
                | ApiPurposeKind::Subagent
        );
        assert_eq!(
            document.business_input_dispatch_started(turn, input_seq),
            business
        );
        assert!(!document.business_input_dispatch_started(turn, started_seq));
        complete_ok(&mut document, api, None, T0 + 5)?;
        assert_eq!(
            document.business_input_dispatch_started(turn, input_seq),
            business
        );
        document.close_turn(TurnOutcome::Stopped, 1, T0 + 6)?;
        // A later turn must not borrow the previous request even when queried with an older position.
        // 查询旧位置时，后继回合也不能借用前一回合请求。
        let next_turn = document.submit_root_input_fixture("Next work", vec![], T0 + 7)?;
        assert!(!document.business_input_dispatch_started(next_turn, input_seq));
        assert_eq!(
            document.business_input_dispatch_started(turn, input_seq),
            business
        );
    }
    Ok(())
}

/// Ending a Goal closes an undispatched continuation after auxiliary work, but never an active request.
/// 辅助工作之后完成目标会关闭未分发续跑，但绝不关闭活动请求。
/// Returns fixture errors and verifies both physical exclusion and original turn ownership.
/// 返回夹具错误，并验证物理排他与原回合归属。
#[test]
fn auxiliary_history_preserves_undispatched_goal_completion() -> Result<(), DomainError> {
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        for active in [false, true] {
            // The Goal creates its own continuation before any business generation exists.
            // 目标在任何业务生成存在之前创建自己的续跑回合。
            let mut document = new_document();
            let goal = document.create_goal("work", None, "Continue work", T0)?;
            let turn = goal.continuation_turn_id.unwrap();
            let api =
                document.start_api(purpose, ApiRequestSnapshot::new("openai", "gpt-5"), T0 + 1)?;
            if !active {
                complete_ok(&mut document, api, None, T0 + 2)?;
            }
            document.complete_goal(T0 + 3)?;
            assert_eq!(document.active_turn().unwrap().turn_id, turn);
            assert_eq!(document.active_turn().unwrap().closed, !active);
            assert_eq!(document.active_api().is_some(), active);
        }
    }
    Ok(())
}

/// User queue priority can replace an undispatched Goal after auxiliary work, without interrupting an active API.
/// 辅助工作之后用户队列优先级可以替代未分发目标，同时不打断活动 API。
/// Returns fixture errors and checks that exact replay never creates a second successor.
/// 返回夹具错误，并检查精确重放不会创建第二个后继。
#[test]
fn auxiliary_history_preserves_user_priority_over_goal() -> Result<(), DomainError> {
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        for active in [false, true] {
            // Use canonical goal and queue transitions rather than mutating event purpose fields.
            // 使用规范目标及队列转换，不修改事件用途字段。
            let mut document = new_document();
            let goal = document.create_goal("work", None, "Continue work", T0)?;
            let turn = goal.continuation_turn_id.unwrap();
            let api =
                document.start_api(purpose, ApiRequestSnapshot::new("openai", "gpt-5"), T0 + 1)?;
            if !active {
                complete_ok(&mut document, api, None, T0 + 2)?;
            }
            let submission = ClientInputSubmission::new(
                ClientActionId::try_new("user-priority")?,
                "New task",
                vec![],
            );
            let receipt = document
                .enqueue_input(submission.clone(), T0 + 3)?
                .into_value();
            if active {
                assert_eq!(document.active_turn().unwrap().turn_id, turn);
                assert_eq!(document.active_api().unwrap().api_call_id, api);
                assert_eq!(document.metadata.input_state.queued_inputs.len(), 1);
            } else {
                assert!(
                    matches!(receipt.disposition, EnqueueInputDisposition::SupersededGoalAndScheduled { stopped_turn_id, .. } if stopped_turn_id == turn)
                );
                assert_ne!(document.active_turn().unwrap().turn_id, turn);
                assert!(document.metadata.input_state.queued_inputs.is_empty());
            }
            let before = document.clone();
            assert_eq!(
                document.enqueue_input(submission, T0 + 4)?.into_value(),
                receipt
            );
            assert_eq!(document, before);
        }
    }
    Ok(())
}

/// Auxiliary processing of a late child return does not prevent a new user task from replacing it.
/// 对迟到子结果的辅助处理不能阻止新用户任务替代它。
/// Returns fixture errors; replacement remains forbidden during a physical request or after business starts.
/// 返回夹具错误；物理请求期间或业务启动之后仍禁止替代。
#[test]
fn auxiliary_history_preserves_user_replacement_of_late_return() -> Result<(), DomainError> {
    // Build the return from real durable child evidence rather than a forged result envelope.
    // 从真实可靠子任务凭据构建回流，不伪造结果信封。
    let (child, child_turn, source_seq) = durable_child_return_fixture()?;
    let input = child.build_subagent_return_input(SubagentReturnBuildRequest {
        final_turn_id: child_turn,
        child_task_id: SubagentTaskId::try_new("late-child-auxiliary")?,
        source_assistant_text_seq: source_seq,
    })?;
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
        ApiPurposeKind::MainGeneration,
    ] {
        for active in [false, true] {
            // Each scenario owns a distinct late-result continuation and physical request.
            // 每个场景拥有独立迟到结果续跑和物理请求。
            let mut document = new_document();
            document.submit_root_input_fixture("Original work", vec![], T0)?;
            document.close_turn(TurnOutcome::Completed, 1, T0 + 1)?;
            let returned = document
                .append_subagent_return(input.clone(), T0 + 2)?
                .into_value();
            let api =
                document.start_api(purpose, ApiRequestSnapshot::new("openai", "gpt-5"), T0 + 3)?;
            if !active {
                complete_ok(&mut document, api, None, T0 + 4)?;
            }
            let before = document.clone();
            let result = document.submit_root_input_fixture("Replacement work", vec![], T0 + 5);
            if active || purpose == ApiPurposeKind::MainGeneration {
                assert!(matches!(
                    result,
                    Err(DomainError::RootInputWhileTurnOpen { .. })
                ));
                assert_eq!(document, before);
            } else {
                assert_ne!(result?, returned.parent_turn_id);
                assert_eq!(document.active_turn().unwrap().origin, TurnOrigin::UserRoot);
            }
        }
    }
    Ok(())
}

/// Auxiliary final text cannot finish unfinished business, including after a newer business retry.
/// 辅助最终正文不能结束未完成业务，也不能越过更新的业务重试。
/// Returns fixture errors; rejected completion must leave every document field unchanged.
/// 返回夹具错误；被拒完成必须保持文档全部字段不变。
#[test]
fn auxiliary_final_cannot_complete_business() -> Result<(), DomainError> {
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        for has_prior_business in [false, true] {
            // A prior final reply must not make a subsequent interrupted business retry complete.
            // 较早最终回复不能令随后中断的业务重试完成。
            let mut document = new_document();
            let turn = document.submit_root_input_fixture("work", vec![], T0)?;
            if has_prior_business {
                let prior = document.start_api(
                    ApiPurposeKind::MainGeneration,
                    ApiRequestSnapshot::new("openai", "gpt-5"),
                    T0 + 1,
                )?;
                document.append_assistant_text(
                    prior,
                    "Old final",
                    AssistantSemanticStatus::Completed,
                    T0 + 2,
                )?;
                complete_ok(&mut document, prior, None, T0 + 3)?;
                document.start_api(
                    ApiPurposeKind::RetryGeneration,
                    ApiRequestSnapshot::new("openai", "gpt-5"),
                    T0 + 4,
                )?;
                document.interrupt_active_api_for_process(T0 + 5)?;
            }
            let auxiliary =
                document.start_api(purpose, ApiRequestSnapshot::new("openai", "gpt-5"), T0 + 6)?;
            document.append_assistant_text(
                auxiliary,
                "Temporary answer",
                AssistantSemanticStatus::Completed,
                T0 + 7,
            )?;
            complete_ok(&mut document, auxiliary, None, T0 + 8)?;
            let before = document.clone();
            assert!(matches!(
                document.complete_turn_and_select_successor(turn, T0 + 9),
                Err(DomainError::TurnMissingFinalAssistant { .. })
            ));
            assert_eq!(document, before);
        }
    }
    Ok(())
}

/// A terminal auxiliary request without final text cannot erase the actual business final.
/// 没有最终正文的终态辅助请求不能抹去真实业务最终回复。
/// Returns fixture errors and proves successful closure still uses the original business turn.
/// 返回夹具错误，证明成功闭合仍使用原业务回合。
#[test]
fn auxiliary_terminal_preserves_business_final() -> Result<(), DomainError> {
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        // The original business final is the sole successful completion evidence.
        // 原业务最终回复是唯一成功完成凭据。
        let mut document = new_document();
        let turn = document.submit_root_input_fixture("work", vec![], T0)?;
        let business = document.start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            T0 + 1,
        )?;
        document.append_assistant_text(
            business,
            "Work finished",
            AssistantSemanticStatus::Completed,
            T0 + 2,
        )?;
        complete_ok(&mut document, business, None, T0 + 3)?;
        document.start_api(purpose, ApiRequestSnapshot::new("openai", "gpt-5"), T0 + 4)?;
        document.interrupt_active_api_for_process(T0 + 5)?;
        document.complete_turn_and_select_successor(turn, T0 + 6)?;
        assert!(document.active_turn().unwrap().closed);
        assert_eq!(document.active_turn().unwrap().turn_id, turn);
    }
    Ok(())
}

/// Explicit Continue follows the business policy regardless of an opposite auxiliary terminal policy.
/// 显式继续遵从业务策略，不受相反辅助终态策略影响。
/// Returns fixture errors; active auxiliary requests remain exclusive and rejection remains atomic.
/// 返回夹具错误；活动辅助请求仍保持排他，拒绝仍保持原子性。
#[test]
fn auxiliary_terminal_cannot_change_business_continuation_policy() -> Result<(), DomainError> {
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        for allowed in [false, true] {
            // Use authentic terminal methods so each eligibility policy has a matching persisted reason.
            // 使用真实终态方法，使每项继续策略都有匹配的持久结束原因。
            let mut document = new_document();
            let turn = document.submit_root_input_fixture("work", vec![], T0)?;
            let business = document.start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                T0 + 1,
            )?;
            if allowed {
                complete_user_interrupted(&mut document, business, T0 + 2)?;
            } else {
                complete_ok(&mut document, business, None, T0 + 2)?;
            }
            let auxiliary =
                document.start_api(purpose, ApiRequestSnapshot::new("openai", "gpt-5"), T0 + 3)?;
            assert!(matches!(
                document.continue_turn_fixture(T0 + 4),
                Err(DomainError::ContinuationNotEligible {
                    reason: "active_api"
                })
            ));
            if allowed {
                complete_ok(&mut document, auxiliary, None, T0 + 5)?;
            } else {
                complete_user_interrupted(&mut document, auxiliary, T0 + 5)?;
            }
            let before = document.clone();
            let result = document.continue_turn_fixture(T0 + 6);
            if allowed {
                result?;
                assert_eq!(document.active_turn().unwrap().turn_id, turn);
            } else {
                assert!(matches!(
                    result,
                    Err(DomainError::ContinuationNotEligible {
                        reason: "terminal_policy_forbidden"
                    })
                ));
                assert_eq!(document, before);
            }
        }
    }
    Ok(())
}

/// Failure settlement preserves interrupted business but cannot be blocked by only auxiliary interruption.
/// 失败结算保留中断业务，但不能仅被辅助中断阻止。
/// Returns fixture errors and checks that retained business receives no extra failure mutation.
/// 返回夹具错误，并检查保留业务没有额外失败修改。
#[test]
fn auxiliary_interruption_does_not_own_business_recovery() -> Result<(), DomainError> {
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        for interrupted_business in [false, true] {
            // The test alternates which lifecycle owns the interruption that must be classified.
            // 测试交替指定必须分类的中断归属于哪个生命周期。
            let mut document = new_document();
            document.submit_root_input_fixture("work", vec![], T0)?;
            if interrupted_business {
                document.start_api(
                    ApiPurposeKind::MainGeneration,
                    ApiRequestSnapshot::new("openai", "gpt-5"),
                    T0 + 1,
                )?;
                document.interrupt_active_api_for_process(T0 + 2)?;
            }
            let auxiliary =
                document.start_api(purpose, ApiRequestSnapshot::new("openai", "gpt-5"), T0 + 3)?;
            if interrupted_business {
                complete_ok(&mut document, auxiliary, None, T0 + 4)?;
            } else {
                document.interrupt_active_api_for_process(T0 + 4)?;
            }
            let before = document.clone();
            let changed =
                document.fail_turn_without_active_api("preparation_failed", None, T0 + 5)?;
            assert_eq!(changed, !interrupted_business);
            if interrupted_business {
                assert_eq!(document, before);
            } else {
                assert!(document.active_turn().unwrap().closed);
            }
        }
    }
    Ok(())
}
