//! Public policy controls preserve native ownership and serialize both settings transaction kinds.
//! 公开策略控制保留原生归属，并串行化两类设置事务。

use super::*;
use crate::{
    VulcanPluginAutomaticPrewarmRule, VulcanPluginCapacityPolicyPhase,
    VulcanPluginPrewarmPolicyPhase, VulcanPluginPrewarmPolicyUpdate,
};

/// Exercise explicit policy persistence through a real manager; no VM may start during publication.
/// 通过真实管理器验证明确策略持久化；发布期间不得启动 VM。
#[tokio::test]
async fn prewarm_policy_controls_preserve_resources_and_exclude_capacity_transactions() {
    // One real parent supplies all current policy and resource identities.
    // 单个真实父级提供全部当前策略及资源身份。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    let initial = fixture
        .manager
        .prewarm_policy_status()
        .expect("read policy");
    assert!(initial.rules.is_empty());
    assert!(initial.update.is_none());
    assert!(!initial.capacity_update_retained);
    assert!(
        !fixture
            .manager
            .plugin_state_root()
            .join("plugin-runtime-settings.json")
            .exists()
    );
    // Rules persist desired targets only; actual reusable-tool validation belongs to execution admission.
    // 规则仅持久化期望目标；实际复用工具校验属于执行准入。
    let request = VulcanPluginPrewarmPolicyUpdate {
        update_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: initial.runtime_id.clone(),
        expected_revision: initial.revision.clone(),
        rules: vec![VulcanPluginAutomaticPrewarmRule {
            plugin_id: "idle-plugin".into(),
            tool_id: "idle_ping".into(),
            target_idle: 1,
        }],
    };
    let committed = fixture
        .manager
        .begin_prewarm_policy_update(request.clone())
        .expect("publish");
    assert_eq!(committed.phase, VulcanPluginPrewarmPolicyPhase::Committed);
    assert!(committed.persistence_error.is_none());
    assert_eq!(
        fixture
            .manager
            .begin_prewarm_policy_update(request.clone())
            .expect("original receipt"),
        committed
    );
    let current = fixture
        .manager
        .prewarm_policy_status()
        .expect("current policy");
    assert_eq!(current.runtime_id, initial.runtime_id);
    assert_eq!(current.revision, committed.result_revision);
    assert_ne!(current.revision, initial.revision);
    assert_eq!(current.rules, request.rules);
    assert_eq!(current.update, Some(committed.clone()));
    assert_eq!(
        load_plugin_runtime_settings(fixture.manager.plugin_state_root())
            .expect("disk policy")
            .automatic_prewarm,
        request.rules
    );
    let runtime = fixture
        .manager
        .formal_runtime_status()
        .expect("runtime status")
        .expect("attached");
    assert_eq!(runtime.resources.resident, 0);
    assert_eq!(runtime.settings.automatic_prewarm, request.rules);
    // Wrong receipt identities cannot recover, cancel or forget whichever transaction is currently retained.
    // 错误回执身份不能恢复、取消或遗忘当前保留的任意事务。
    let unknown_id = uuid::Uuid::new_v4().to_string();
    assert!(
        fixture
            .manager
            .resume_prewarm_policy_update(&unknown_id)
            .is_err()
    );
    assert!(
        fixture
            .manager
            .cancel_prewarm_policy_update(&unknown_id)
            .is_err()
    );
    assert!(
        fixture
            .manager
            .forget_prewarm_policy_update(&unknown_id)
            .is_err()
    );
    fixture.tool("idle_ping", json!({})).await;
    // Native capacity registration is independent of policy receipt retention.
    // 原生容量注册独立于策略回执保留。
    let capacity_request = policy::update(&fixture, VulcanPluginCapacityPurpose::ToolCall);
    assert!(
        fixture
            .manager
            .begin_capacity_policy_update(capacity_request.clone())
            .is_err()
    );
    fixture
        .manager
        .forget_prewarm_policy_update(&request.update_id)
        .expect("acknowledge prewarm");
    let capacity_commit = fixture
        .manager
        .begin_capacity_policy_update(capacity_request.clone())
        .expect("capacity commit");
    assert_eq!(
        capacity_commit.phase,
        VulcanPluginCapacityPolicyPhase::Committed
    );
    let after_capacity = fixture
        .manager
        .prewarm_policy_status()
        .expect("policy after capacity");
    assert!(after_capacity.capacity_update_retained);
    assert_eq!(after_capacity.rules, request.rules);
    assert_eq!(after_capacity.revision, current.revision);
    // A complete empty rule collection disables the policy without replaying any initialization.
    // 完整空规则集合关闭策略，不重放任何初始化。
    let disable = VulcanPluginPrewarmPolicyUpdate {
        update_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: current.runtime_id,
        expected_revision: current.revision,
        rules: Vec::new(),
    };
    assert!(
        fixture
            .manager
            .begin_prewarm_policy_update(disable.clone())
            .is_err()
    );
    fixture
        .manager
        .forget_capacity_policy_update(&capacity_request.update_id)
        .expect("acknowledge capacity");
    // Forgetting a receipt never makes its old predecessor valid again.
    // 遗忘回执绝不使其旧前驱再次有效。
    let mut stale = request;
    stale.update_id = uuid::Uuid::new_v4().to_string();
    assert!(fixture.manager.begin_prewarm_policy_update(stale).is_err());
    let disabled = fixture
        .manager
        .begin_prewarm_policy_update(disable.clone())
        .expect("disable automatic policy");
    assert_eq!(disabled.phase, VulcanPluginPrewarmPolicyPhase::Committed);
    assert!(
        fixture
            .manager
            .prewarm_policy_status()
            .expect("disabled")
            .rules
            .is_empty()
    );
    let capacity = fixture
        .manager
        .formal_capacity_status("idle-plugin", capacity_request.purpose)
        .expect("capacity")
        .expect("same owner");
    assert_eq!(capacity.snapshot.capacity_id, capacity_request.capacity_id);
    assert_eq!(
        Some(capacity.policy_revision),
        capacity_commit.applied_revision
    );
    fixture
        .manager
        .forget_prewarm_policy_update(&disable.update_id)
        .expect("acknowledge disable");
    fixture.close().await;
}
