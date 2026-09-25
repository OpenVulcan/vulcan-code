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
