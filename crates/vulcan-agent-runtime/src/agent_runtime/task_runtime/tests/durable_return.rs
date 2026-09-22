//! Exact host-publication waiting independent from consuming parent mailbox messages.
//! 独立于消费父邮箱消息的精确宿主发布等待。

use super::*;

/// Builds typed host feedback for `child` and `parent`; returns the selected actual terminal kind.
/// 为 `child` 与 `parent` 构建强类型宿主反馈；返回指定实际终态类型。
fn feedback(
    child: SessionId,
    parent: SessionId,
    kind: SubagentMailboxEventKind,
) -> SubagentMailboxEvent {
    SubagentMailboxEvent {
        parent_thread_id: parent.into(),
        parent_turn_id: None,
        child_thread_id: child.into(),
        kind,
        timestamp_ms: 1,
        payload: serde_json::json!({"execution_id": "original"}),
    }
}

/// Publication before or after subscription is repeatable and does not consume the parent's event.
/// 订阅前或后的发布均可重复读取，且不消费父级事件。
/// Returns nothing; polls the real future to prove it remains pending before exact publication.
/// 不返回值；轮询真实 Future，证明其在精确发布前保持等待。
#[tokio::test]
async fn durable_return_wait_preserves_receipt_and_mailbox() {
    for kind in [
        SubagentMailboxEventKind::Completed,
        SubagentMailboxEventKind::Failed,
        SubagentMailboxEventKind::Stopped,
    ] {
        // Every terminal owns an isolated child gate and parent mailbox.
        // 每个终态拥有独立子门控和父邮箱。
        let runtime = TaskRuntime::new();
        // Stable identity selects the gate independently from the terminal payload.
        // 稳定身份独立于终态载荷选择门控。
        let child = Uuid::new_v4();
        // Parent identity verifies that waiting never drains feedback.
        // 父身份验证等待绝不排空反馈。
        let parent = Uuid::new_v4();
        runtime
            .expect_durable_task_return(child, "original")
            .unwrap();
        // Register the waiter before publication without sleeping or spawning a scheduler race.
        // 在发布前登记等待者，不休眠也不制造调度竞争。
        let waiter = runtime.wait_for_durable_task_return(child, "original");
        tokio::pin!(waiter);
        assert!(futures_util::poll!(&mut waiter).is_pending());
        // The exact feedback may report failure; the waiter must not promote it to success.
        // 精确反馈可以报告失败；等待者不得将其提升为成功。
        let event = feedback(child, parent, kind);
        runtime
            .publish_durable_task_return("original", event.clone())
            .unwrap();
        assert_eq!(waiter.await.unwrap(), event);
        assert_eq!(
            runtime
                .wait_for_durable_task_return(child, "original")
                .await
                .unwrap(),
            event
        );
        assert_eq!(
            runtime
                .take_subagent_events_for_parent(parent.into())
                .unwrap(),
            vec![event]
        );
    }
}

/// Replacement and rejected-start removal awaken existing waiters and preserve unrelated current gates.
/// 替换和拒绝启动移除会唤醒已有等待者，并保留无关当前门控。
/// Returns nothing; missing gates fail immediately and every pending wake is bounded by the test timeout.
/// 不返回值；缺失门控立即失败，每次待处理唤醒由测试超时约束。
#[tokio::test]
async fn durable_return_wait_rejects_cancelled_and_replaced_execution() {
    for replaced in [false, true] {
        // A fresh runtime starts with no durable publication authority.
        // 全新运行时开始时没有可靠发布权威。
        let runtime = TaskRuntime::new();
        // One child identity is reused only when the test explicitly replaces its execution.
        // 仅当测试明确替换执行时复用一个子身份。
        let child = Uuid::new_v4();
        assert!(
            runtime
                .wait_for_durable_task_return(child, "original")
                .await
                .is_err()
        );
        runtime
            .expect_durable_task_return(child, "original")
            .unwrap();
        // Polling installs the real watch subscription before changing its authoritative gate.
        // 轮询在改变权威门控前安装真实观察订阅。
        let waiter = runtime.wait_for_durable_task_return(child, "original");
        tokio::pin!(waiter);
        assert!(futures_util::poll!(&mut waiter).is_pending());
        if replaced {
            runtime
                .expect_durable_task_return(child, "replacement")
                .unwrap();
            assert!(
                !runtime
                    .cancel_rejected_task_return(child, "original")
                    .unwrap()
            );
        } else {
            assert!(
                runtime
                    .cancel_rejected_task_return(child, "original")
                    .unwrap()
            );
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
                .await
                .unwrap()
                .is_err()
        );
    }
}
