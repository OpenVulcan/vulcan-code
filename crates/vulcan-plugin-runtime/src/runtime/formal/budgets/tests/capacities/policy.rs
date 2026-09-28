//! Actual host policy transactions cover native acknowledgement, persistence faults and restart recovery.
//! 实际宿主策略事务覆盖原生确认、持久故障及重启恢复。

use super::*;
use crate::{VulcanPluginCapacityPolicyPhase as Phase, VulcanPluginCapacityPolicyUpdate};

/// Build one revision from fixture's exact native snapshot; return a complete smaller queue policy.
/// 从 fixture 的精确原生快照构建一次修订；返回完整较小队列策略。
fn update(
    fixture: &Fixture,
    purpose: VulcanPluginCapacityPurpose,
) -> VulcanPluginCapacityPolicyUpdate {
    // The public status supplies all immutable predecessor identities.
    // 公开状态提供全部不可变前驱身份。
    let status = fixture
        .manager
        .formal_capacity_status("idle-plugin", purpose)
        .expect("policy status")
        .expect("registered");
    // Derive the target from this actual configuration rather than another copy of defaults.
    // 从此实际配置派生目标，不使用另一份默认值。
    let mut config = status.snapshot.config.clone();
    config.max_queued_calls -= 1;
    VulcanPluginCapacityPolicyUpdate {
        update_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: status.runtime_id,
        capacity_id: status.snapshot.capacity_id,
        expected_revision: status.policy_revision,
        plugin_id: "idle-plugin".into(),
        purpose,
        config: Some(config),
    }
}

/// Commit and explicit inheritance reset use the same native owner, while repeated observers never replay.
/// 提交及明确继承重置使用同一原生所有者；重复观测者绝不重放。
#[tokio::test]
async fn capacity_policy_transaction_commits_and_resets_without_rebinding() {
    // Real production work registers the original purpose capacity.
    // 真实生产工作注册原用途容量。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    fixture.tool("idle_ping", json!({})).await;
    // No configuration file is written during attachment, discovery or business execution.
    // 附加、发现及业务执行期间不写配置文件。
    let path = fixture
        .manager
        .plugin_state_root()
        .join("plugin-runtime-settings.json");
    assert!(!path.exists());
    // This exact request remains recoverable independently from the submitting observer.
    // 此精确请求独立于提交观测者保持可恢复。
    let request = update(&fixture, VulcanPluginCapacityPurpose::ToolCall);
    let committed = fixture
        .manager
        .begin_capacity_policy_update(request.clone())
        .expect("commit");
    assert_eq!(committed.phase, Phase::Committed);
    assert_eq!(
        fixture
            .manager
            .begin_capacity_policy_update(request.clone())
            .expect("observe original"),
        committed
    );
    assert_eq!(
        fixture
            .manager
            .capacity_policy_update(&request.update_id)
            .expect("query original"),
        Some(committed.clone())
    );
    // Both persisted and live effective projections use the full current override.
    // 持久及活动生效投影均使用完整当前覆盖。
    let settings = load_plugin_runtime_settings(fixture.manager.plugin_state_root())
        .expect("persisted settings");
    assert_eq!(
        settings.capacity_budget("idle-plugin", request.purpose).0,
        request.config.clone().expect("target")
    );
    let status = fixture
        .manager
        .formal_capacity_status("idle-plugin", request.purpose)
        .expect("status")
        .expect("owner");
    assert_eq!(status.snapshot.capacity_id, request.capacity_id);
    assert_eq!(
        Some(status.policy_revision.clone()),
        committed.applied_revision
    );
    fixture.tool("idle_ping", json!({})).await;
    fixture
        .manager
        .forget_capacity_policy_update(&request.update_id)
        .expect("acknowledge");
    // A new caller identity cannot make an old predecessor current again.
    // 新调用方身份不能使旧前驱再次有效。
    let mut stale = request.clone();
    stale.update_id = uuid::Uuid::new_v4().to_string();
    assert!(fixture.manager.begin_capacity_policy_update(stale).is_err());
    // None is the declared reset operation and is resolved by the original inherited-budget authority.
    // None 是已声明重置操作，由原继承预算权威解析。
    let mut reset = update(&fixture, request.purpose);
    reset.config = None;
    let restored = fixture
        .manager
        .begin_capacity_policy_update(reset.clone())
        .expect("reset");
    assert_eq!(restored.phase, Phase::Committed);
    assert_eq!(
        fixture
            .manager
            .formal_capacity_status("idle-plugin", request.purpose)
            .expect("status")
            .expect("owner")
            .budget_source,
        VulcanPluginCapacityBudgetSource::PluginBudget
    );
    fixture
        .manager
        .forget_capacity_policy_update(&reset.update_id)
        .expect("acknowledge reset");
    fixture.close().await;
}

/// Pending preparation blocks policy application and new admissions until explicit continuation of the same receipt.
/// 待处理准备阻止策略应用及新入场，直到显式继续同一回执。
#[tokio::test]
async fn capacity_policy_transaction_retains_preparation_and_explicit_cancel() {
    // One real preparation claim deliberately remains outside its short publication gate.
    // 一个真实准备声明被刻意保留在短发布门之外。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    let owner = fixture
        .manager
        .formal_runtime_owner()
        .expect("owner")
        .expect("attached");
    let claim = {
        // This uses the exact production ownership and lock order.
        // 此处使用精确生产归属及锁顺序。
        let gate = fixture
            .manager
            .state
            .session_registration_gate
            .lock()
            .expect("publication");
        owner
            .admit_capacity_member("idle-plugin", VulcanPluginCapacityPurpose::ToolCall, &gate)
            .expect("preparing")
    };
    // Preparation admission supplies an actual native identity before the update is submitted.
    // 提交更新前，准备入场提供实际原生身份。
    let request = update(&fixture, VulcanPluginCapacityPurpose::ToolCall);
    let pending = fixture
        .manager
        .begin_capacity_policy_update(request.clone())
        .expect("pending receipt");
    assert_eq!(pending.phase, Phase::Prepared);
    assert_eq!(
        owner
            .host
            .runtime()
            .capacity_policy(&request.capacity_id)
            .expect("native policy")
            .revision,
        request.expected_revision
    );
    assert!(
        fixture
            .manager
            .forget_formal_capacity("idle-plugin", request.purpose)
            .is_err()
    );
    {
        // A second preparation cannot bypass the retained policy transaction.
        // 第二个准备不能绕过保留策略事务。
        let gate = fixture
            .manager
            .state
            .session_registration_gate
            .lock()
            .expect("publication");
        assert!(
            owner
                .admit_capacity_member("idle-plugin", request.purpose, &gate)
                .is_err()
        );
    }
    drop(claim);
    assert_eq!(
        fixture
            .manager
            .resume_capacity_policy_update(&request.update_id)
            .expect("resume")
            .phase,
        Phase::Committed
    );
    fixture
        .manager
        .forget_capacity_policy_update(&request.update_id)
        .expect("acknowledge");
    // A pre-publication filesystem failure is still queryable and can be cancelled without applying the core.
    // 发布前文件系统失败仍可查询，且可在不应用核心的情况下取消。
    let cancelled = update(&fixture, request.purpose);
    owner.fail_policy_write(Phase::Prepared, false);
    assert_eq!(
        fixture
            .manager
            .begin_capacity_policy_update(cancelled.clone())
            .expect("retained failure")
            .phase,
        Phase::Prepared
    );
    assert_eq!(
        fixture
            .manager
            .cancel_capacity_policy_update(&cancelled.update_id)
            .expect("cancel")
            .phase,
        Phase::Rejected
    );
    assert_eq!(
        owner
            .host
            .runtime()
            .capacity_policy(&cancelled.capacity_id)
            .expect("unchanged native policy")
            .revision,
        cancelled.expected_revision
    );
    fixture
        .manager
        .forget_capacity_policy_update(&cancelled.update_id)
        .expect("acknowledge cancellation");
    fixture.close().await;
}

/// Failed final publication keeps native success and resumes only persistence, including after visible replacement.
/// 最终发布失败保留原生成功，并仅恢复持久化，包含替换已可见的情况。
#[tokio::test]
async fn capacity_policy_transaction_recovers_durable_confirmation_without_native_replay() {
    for after_publication in [false, true] {
        // Each case owns an independent actual runtime and settings file.
        // 每个用例拥有独立实际运行时及设置文件。
        let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
        fixture.tool("idle_ping", json!({})).await;
        let owner = fixture
            .manager
            .formal_runtime_owner()
            .expect("owner")
            .expect("attached");
        let request = update(&fixture, VulcanPluginCapacityPurpose::ToolCall);
        owner.fail_policy_write(Phase::Committed, after_publication);
        // Acknowledged native success survives a failed disk acknowledgement.
        // 已确认原生成功在磁盘确认失败后保留。
        let applied = fixture
            .manager
            .begin_capacity_policy_update(request.clone())
            .expect("applied receipt");
        assert_eq!(applied.phase, Phase::Applied);
        assert!(applied.persistence_error.is_some());
        let native_revision = owner
            .host
            .runtime()
            .capacity_policy(&request.capacity_id)
            .expect("actual policy")
            .revision;
        assert_eq!(applied.applied_revision.as_ref(), Some(&native_revision));
        assert!(
            fixture
                .manager
                .cancel_capacity_policy_update(&request.update_id)
                .is_err()
        );
        assert!(
            fixture
                .manager
                .forget_capacity_policy_update(&request.update_id)
                .is_err()
        );
        // An unrelated external edit is never overwritten by a recovery attempt.
        // 恢复尝试绝不覆盖无关外部编辑。
        let path = fixture
            .manager
            .plugin_state_root()
            .join("plugin-runtime-settings.json");
        let original = std::fs::read(&path).expect("last known document");
        std::fs::write(&path, b"external-policy-edit").expect("independent edit");
        assert!(
            fixture
                .manager
                .resume_capacity_policy_update(&request.update_id)
                .expect("retained conflict")
                .persistence_error
                .is_some()
        );
        assert_eq!(
            std::fs::read(&path).expect("preserved edit"),
            b"external-policy-edit"
        );
        std::fs::write(&path, original).expect("restore exact original evidence");
        // The same receipt advances without changing its already-issued native revision.
        // 同一回执继续推进，不改变已经签发的原生修订。
        let committed = fixture
            .manager
            .resume_capacity_policy_update(&request.update_id)
            .expect("durable recovery");
        assert_eq!(committed.phase, Phase::Committed);
        assert_eq!(committed.applied_revision, Some(native_revision.clone()));
        assert!(committed.persistence_error.is_none());
        assert_eq!(
            owner
                .host
                .runtime()
                .capacity_policy(&request.capacity_id)
                .expect("same native result")
                .revision,
            native_revision
        );
        fixture
            .manager
            .forget_capacity_policy_update(&request.update_id)
            .expect("acknowledge");
        fixture.close().await;
    }
}

/// Restart reads committed settings and requires explicit interruption acknowledgement instead of replaying old intent.
/// 重启读取已提交设置，并要求显式中断确认，不重放旧意图。
#[tokio::test]
async fn capacity_policy_transaction_restart_preserves_bytes_and_never_replays_old_runtime() {
    // Lose only the prepared-file acknowledgement, before any native revision is applied.
    // 仅丢失预备文件确认，发生在任何原生修订应用之前。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    fixture.tool("idle_ping", json!({})).await;
    let owner = fixture
        .manager
        .formal_runtime_owner()
        .expect("owner")
        .expect("attached");
    let request = update(&fixture, VulcanPluginCapacityPurpose::ToolCall);
    owner.fail_policy_write(Phase::Prepared, true);
    assert_eq!(
        fixture
            .manager
            .begin_capacity_policy_update(request.clone())
            .expect("prepared")
            .phase,
        Phase::Prepared
    );
    // A separate native parent can observe persisted intent but cannot recover while its original writer lives.
    // 独立原生父级可观测持久意图，但原写入者存活时不能恢复。
    let restarted = test_manager(fixture.root.path(), fixture.root.path().join("bundled"));
    let settings = load_plugin_runtime_settings(fixture.manager.plugin_state_root())
        .expect("committed settings");
    let path = fixture
        .manager
        .plugin_state_root()
        .join("plugin-runtime-settings.json");
    let original = std::fs::read(&path).expect("prepared bytes");
    restarted
        .initialize_formal_runtime(settings.clone())
        .expect("new parent");
    assert!(
        restarted
            .resume_capacity_policy_update(&request.update_id)
            .is_err()
    );
    assert_eq!(std::fs::read(&path).expect("unchanged intent"), original);
    fixture.close().await;
    drop(owner);
    // Keep the actual filesystem alive while releasing the original manager and its OS writer lease.
    // 释放原管理器及其操作系统写入租约期间，保持实际文件系统存活。
    let Fixture { root, manager } = fixture;
    drop(manager);
    let interrupted = restarted
        .resume_capacity_policy_update(&request.update_id)
        .expect("explicit restart recovery");
    assert_eq!(interrupted.phase, Phase::Interrupted);
    assert!(interrupted.applied_revision.is_none());
    assert_eq!(
        load_plugin_runtime_settings(restarted.plugin_state_root())
            .expect("still committed settings"),
        settings
    );
    restarted
        .forget_capacity_policy_update(&request.update_id)
        .expect("acknowledge original intent");
    restarted
        .request_formal_runtime_close()
        .expect("close new parent");
    // Worker shutdown still completes asynchronously even without a business VM.
    // 即使没有业务 VM，工作者关闭仍异步完成。
    tokio::time::timeout(WAIT, async {
        while !restarted
            .poll_formal_runtime_closed()
            .expect("new parent physical closure")
        {
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    })
    .await
    .expect("new parent shutdown");
    drop(root);
}
