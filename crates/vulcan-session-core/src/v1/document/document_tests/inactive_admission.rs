//! New input after stopped or interrupted execution, independent of final output markers.
//! 停止或中断执行后的新输入，不依赖最终输出标记。

use super::*;

/// Idle admission closes pre-API, unfinished, interrupted and completed-API journals after reload.
/// 空闲准入在重载后闭合接口前、未完成、中断及接口已完成的日志。
/// No network is used; assertions preserve user text, terminal identity, statistics and idempotency.
/// 不使用网络；断言保留用户正文、终态身份、统计与幂等性。
#[test]
fn inactive_admission_recovers_missing_turn_markers_after_reload() {
    for stage in ["pre_api", "streaming", "interrupted", "completed"] {
        // Each stage isolates a real absence of physical ownership with a different persisted boundary.
        // 每个阶段隔离一种实际无物理所有者但持久边界不同的情况。
        let mut document = new_document();
        let original = ClientInputSubmission::new(
            ClientActionId::try_new("original").unwrap(),
            "old input",
            vec![],
        );
        let old_turn = document
            .submit_root_input(original.clone(), T0)
            .unwrap()
            .into_value();
        if stage != "pre_api" {
            let api = document
                .start_api(
                    ApiPurposeKind::MainGeneration,
                    ApiRequestSnapshot::new("openai", "gpt-5"),
                    T0 + 1,
                )
                .unwrap();
            document
                .append_assistant_text(
                    api,
                    "partial or final",
                    AssistantSemanticStatus::Completed,
                    T0 + 2,
                )
                .unwrap();
            if stage == "completed" {
                complete_ok(&mut document, api, None, T0 + 3).unwrap();
            } else if stage == "interrupted" {
                document.interrupt_active_api_for_process(T0 + 3).unwrap();
            }
        }
        document.rebuild_derived_state().unwrap();
        document.project_persisted_interruptions().unwrap();
        let retained = document.events().to_vec();
        let next = ClientInputSubmission::new(
            ClientActionId::try_new("new").unwrap(),
            "new input",
            vec![],
        );
        // Invalid replacement must not discard the prior Continue opportunity or partially close a turn.
        // 无效替代不得丢弃原继续机会或部分关闭回合。
        let revision = document.memory_revision();
        assert!(
            document
                .submit_root_input_after_execution_ended(
                    ClientInputSubmission::new(
                        ClientActionId::try_new("invalid").unwrap(),
                        "",
                        vec![]
                    ),
                    T0 + 9
                )
                .is_err()
        );
        assert_eq!(document.memory_revision(), revision);
        let new_turn = document
            .submit_root_input_after_execution_ended(next.clone(), T0 + 10)
            .unwrap()
            .into_value();
        assert_ne!(old_turn, new_turn);
        assert!(document.active_api().is_none());
        assert_eq!(document.events().iter().filter(|event| event.turn_id == Some(old_turn)
            && matches!(&event.event, EventBody::TurnStats(stats) if stats.outcome == TurnOutcome::Stopped)).count(), 1);
        // Existing terminal records remain byte-for-byte unchanged; there is no fabricated successful response.
        // 既有终态记录逐字节保持不变，不伪造成功响应。
        for event in retained {
            assert_eq!(
                document
                    .events()
                    .iter()
                    .find(|current| current.seq == event.seq),
                Some(&event)
            );
        }
        let settled_revision = document.memory_revision();
        assert_eq!(
            document
                .submit_root_input_after_execution_ended(original, T0 + 11)
                .unwrap()
                .into_value(),
            old_turn
        );
        assert_eq!(
            document
                .submit_root_input_after_execution_ended(next, T0 + 12)
                .unwrap()
                .into_value(),
            new_turn
        );
        assert_eq!(document.memory_revision(), settled_revision);
        document.rebuild_derived_state().unwrap();
        assert_eq!(document.active_turn().unwrap().turn_id, new_turn);
    }
}

/// New input starts a fresh turn when auxiliary interruption follows completed business work.
/// 已完成业务之后的辅助请求被中断时，新输入开启新的回合。
/// Returns fixture errors; persisted terminals remain unchanged and forbidden Continue stays rejected.
/// 返回夹具错误；持久化终态保持不变，禁止的 Continue 仍被拒绝。
#[test]
fn inactive_admission_after_interrupted_auxiliary_preserves_business_policy()
-> Result<(), DomainError> {
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        // Complete the business request without closing its turn, as in a tool-follow-up boundary.
        // 完成业务请求但不闭合其回合，与工具续接边界一致。
        let mut document = new_document();
        let old_turn = document.submit_root_input_fixture("old input", vec![], T0)?;
        let business = document.start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            T0 + 1,
        )?;
        complete_ok(&mut document, business, None, T0 + 2)?;
        // Interruption belongs only to this auxiliary request and cannot authorize replay of business.
        // 中断仅属于本次辅助请求，不能授权重放业务请求。
        let auxiliary =
            document.start_api(purpose, ApiRequestSnapshot::new("openai", "gpt-5"), T0 + 3)?;
        complete_user_interrupted(&mut document, auxiliary, T0 + 4)?;
        document.rebuild_derived_state()?;
        assert!(matches!(
            document.continue_turn_fixture(T0 + 5),
            Err(DomainError::ContinuationNotEligible {
                reason: "terminal_policy_forbidden"
            })
        ));
        // Retain exact historical events instead of repairing eligibility by rewriting old terminals.
        // 保留精确历史事件，不通过改写旧终态来修复继续资格。
        let retained = document.events().to_vec();
        let next = ClientInputSubmission::new(
            ClientActionId::try_new("after-auxiliary-interruption")?,
            "new input",
            vec![],
        );
        let new_turn = document
            .submit_root_input_after_execution_ended(next.clone(), T0 + 6)?
            .into_value();
        assert_ne!(new_turn, old_turn);
        assert!(document.active_api().is_none());
        assert_eq!(document.active_turn().unwrap().turn_id, new_turn);
        for event in retained {
            assert_eq!(
                document
                    .events()
                    .iter()
                    .find(|current| current.seq == event.seq),
                Some(&event)
            );
        }
        // Retrying the same new input reuses its turn without appending duplicate history.
        // 重试同一条新输入复用其回合，不追加重复历史。
        let accepted_revision = document.memory_revision();
        assert_eq!(
            document
                .submit_root_input_after_execution_ended(next, T0 + 7)?
                .into_value(),
            new_turn
        );
        assert_eq!(document.memory_revision(), accepted_revision);
    }
    Ok(())
}
