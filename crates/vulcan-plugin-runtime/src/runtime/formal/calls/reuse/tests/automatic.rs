//! Automatic trigger provenance shares real target admission and never creates a second execution ledger.
//! 自动触发来源共用真实目标入场，绝不创建第二套执行台账。

use super::*;

mod permission;

impl Fixture {
    /// Publish target as the exact rule, or disable with None; return a fresh trigger bound to its acknowledged revision.
    /// 将 target 发布为精确规则，或使用 None 关闭；返回绑定已确认修订的新触发。
    pub(super) fn automatic_policy(
        &self,
        target: Option<usize>,
    ) -> VulcanPluginAutomaticPrewarmOrigin {
        // Observe and replace the original policy through the public persistence controls.
        // 通过公开持久控制观测并替换原策略。
        let status = self.manager.prewarm_policy_status().expect("policy");
        let request = VulcanPluginPrewarmPolicyUpdate {
            update_id: uuid::Uuid::new_v4().to_string(),
            runtime_id: status.runtime_id,
            expected_revision: status.revision,
            rules: target
                .into_iter()
                .map(|target_idle| VulcanPluginAutomaticPrewarmRule {
                    plugin_id: "idle-plugin".into(),
                    tool_id: "idle_ping".into(),
                    target_idle,
                })
                .collect(),
        };
        let result = self
            .manager
            .begin_prewarm_policy_update(request.clone())
            .expect("publish policy");
        assert_eq!(result.phase, VulcanPluginPrewarmPolicyPhase::Committed);
        assert!(result.persistence_error.is_none());
        self.manager
            .forget_prewarm_policy_update(&request.update_id)
            .expect("acknowledge policy");
        VulcanPluginAutomaticPrewarmOrigin {
            trigger_id: uuid::Uuid::new_v4().to_string(),
            policy_revision: result.result_revision,
        }
    }
}

/// Concurrent repeated triggers retain one original receipt and deadline; later business reuses its actual VM.
/// 并发重复触发保留单个原回执及截止时间；后续业务复用实际 VM。
#[tokio::test]
async fn hot_reuse_automatic_trigger_is_atomic_and_preserves_original_deadline() {
    // The real package has one idle slot and a policy explicitly enabling that slot.
    // 真实包具有一个空闲槽，策略明确启用该槽。
    let fixture = Fixture::new(10_000, 100);
    // The original lifecycle owner stays alive for this test.
    // 原生命周期所有者在本测试期间保持活动。
    let trigger = VulcanPluginAutomaticPrewarmTrigger::default();
    let origin = fixture.automatic_policy(Some(1));
    let request = fixture.target(1);
    // Four callers fit the repository's shared validation concurrency budget.
    // 四个调用方符合仓库共享验证并发预算。
    const CONCURRENT_PREPARATIONS: usize = 4;
    let barrier = std::sync::Barrier::new(CONCURRENT_PREPARATIONS);
    let receipts = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..CONCURRENT_PREPARATIONS)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    fixture
                        .manager
                        .prepare_automatic_tool_prewarm(
                            request.clone(),
                            origin.clone(),
                            trigger.permit(),
                        )
                        .expect("concurrent first preparation")
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("join first admission"))
            .collect::<Vec<_>>()
    });
    let prepared = receipts.first().expect("a real admitted receipt");
    assert!(
        receipts
            .iter()
            .all(|receipt| receipt.invocation_id == prepared.invocation_id)
    );
    let owner = fixture.owner();
    let original_deadline = owner
        .calls
        .lock()
        .expect("calls")
        .get(&prepared.invocation_id)
        .expect("original call")
        .lock()
        .expect("call")
        .pending
        .as_ref()
        .expect("unstarted")
        .deadline;
    // A later retry cannot replace the retained deadline after the first concurrent admission settled.
    // 首次并发入场结束后，较晚重试不能替换保留截止时间。
    assert_eq!(
        fixture
            .manager
            .prepare_automatic_tool_prewarm(request.clone(), origin.clone(), trigger.permit())
            .expect("repeat")
            .invocation_id,
        prepared.invocation_id
    );
    assert_eq!(
        fixture
            .manager
            .formal_calls("owner")
            .expect("receipts")
            .len(),
        1
    );
    assert_eq!(
        owner
            .calls
            .lock()
            .expect("calls")
            .get(&prepared.invocation_id)
            .expect("original call")
            .lock()
            .expect("call")
            .pending
            .as_ref()
            .expect("unstarted")
            .deadline,
        original_deadline
    );
    assert_eq!(
        owner
            .host
            .runtime()
            .resources()
            .expect("native resources")
            .resident,
        0
    );
    // Reusing a trigger for a changed request must not silently retarget original work.
    // 将触发复用于更改后的请求，不能静默重定向原工作。
    let mut conflicting = request.clone();
    conflicting.target_idle = 0;
    assert!(
        fixture
            .manager
            .prepare_automatic_tool_prewarm(conflicting, origin.clone(), trigger.permit())
            .is_err()
    );
    fixture
        .manager
        .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
        .expect("start");
    fixture
        .manager
        .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
        .expect("same start");
    let completed = targets::wait_target(&fixture, &prepared.invocation_id).await;
    assert!(completed.error.is_none(), "{completed:?}");
    assert_eq!(
        completed.prewarm.as_ref().expect("target").automatic,
        Some(origin.clone())
    );
    assert_eq!(
        completed
            .prewarm
            .as_ref()
            .expect("target")
            .satisfied
            .as_ref()
            .expect("confirmed ready")
            .ready,
        1
    );
    assert_eq!(
        fixture
            .invoke("owner", fixture.root.path().to_path_buf(), "ok")
            .await
            .expect("ordinary business")["count"],
        1
    );
    assert_eq!(
        owner.host.runtime().resources().expect("same VM").resident,
        1
    );
    // A package generation change does not hide an already-retained original retry receipt.
    // 包代次变化不能隐藏已保留的原重试回执。
    fixture.target_capacity(2);
    assert_eq!(
        fixture
            .manager
            .prepare_automatic_tool_prewarm(request, origin, trigger.permit())
            .expect("historical original")
            .invocation_id,
        prepared.invocation_id
    );
    fixture
        .manager
        .forget_formal_call(&prepared.invocation_id, "owner")
        .expect("acknowledge");
    fixture.close().await;
}

/// An unconfigured, oversized or revoked trigger cannot initialize any VM, including after preparation.
/// 未配置、过量或已撤销触发不能初始化任何 VM，包括准备完成之后。
#[tokio::test]
async fn hot_reuse_automatic_policy_and_metadata_are_checked_before_initialization() {
    let fixture = Fixture::new(10_000, 100);
    // The original lifecycle owner stays alive for this test.
    // 原生命周期所有者在本测试期间保持活动。
    let trigger = VulcanPluginAutomaticPrewarmTrigger::default();
    fixture.target_capacity(2);
    let current = fixture
        .manager
        .prewarm_policy_status()
        .expect("default policy");
    let disabled = VulcanPluginAutomaticPrewarmOrigin {
        trigger_id: uuid::Uuid::new_v4().to_string(),
        policy_revision: current.revision,
    };
    assert!(
        fixture
            .manager
            .prepare_automatic_tool_prewarm(fixture.target(1), disabled, trigger.permit())
            .is_err()
    );
    let origin = fixture.automatic_policy(Some(1));
    assert!(
        fixture
            .manager
            .prepare_automatic_tool_prewarm(fixture.target(2), origin.clone(), trigger.permit())
            .is_err()
    );
    // Origin bytes participate in the same finite input budget as the original target and retained handles.
    // 来源字节与原目标及保留句柄参与同一个有限输入预算。
    let mut oversized = origin.clone();
    oversized.trigger_id = "x".repeat(fixture.owner().settings.parent.max_queued_bytes);
    assert!(
        fixture
            .manager
            .prepare_automatic_tool_prewarm(fixture.target(1), oversized, trigger.permit())
            .is_err()
    );
    assert!(
        fixture
            .manager
            .formal_calls("owner")
            .expect("no rejected receipts")
            .is_empty()
    );
    let original = fixture
        .manager
        .prepare_automatic_tool_prewarm(fixture.target(1), origin.clone(), trigger.permit())
        .expect("prepare under original policy");
    fixture.automatic_policy(None);
    fixture
        .manager
        .start_reusable_tool_prewarm(&original.invocation_id, "owner")
        .expect("start original supervisor");
    let failed = targets::wait_target(&fixture, &original.invocation_id).await;
    assert!(
        failed
            .error
            .as_ref()
            .expect("revoked")
            .contains("policy changed")
    );
    assert!(failed.operation.is_none());
    assert_eq!(
        fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("no allocation")
            .resident,
        0
    );
    assert_eq!(
        fixture
            .manager
            .prepare_automatic_tool_prewarm(fixture.target(1), origin.clone(), trigger.permit())
            .expect("observe original revoked target")
            .invocation_id,
        original.invocation_id
    );
    fixture
        .manager
        .forget_formal_call(&original.invocation_id, "owner")
        .expect("acknowledge");
    assert!(
        fixture
            .manager
            .prepare_automatic_tool_prewarm(fixture.target(1), origin, trigger.permit())
            .is_err()
    );
    fixture.close().await;
}
