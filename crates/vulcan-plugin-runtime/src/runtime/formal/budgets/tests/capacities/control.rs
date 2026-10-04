//! Delayed management controls must never act on replacement owners or unobserved policies.
//! 延迟管理控制绝不能作用于替代所有者或未观测策略。

use super::*;
use crate::{
    VulcanPluginCapacityControlAction as Action, VulcanPluginCapacityControlRequest,
    VulcanPluginCapacityPolicyUpdate, VulcanPluginCapacityRegistrationRequest,
};

/// Register fixture's idle tool purpose without Lua; return the exact close observation.
/// 不运行 Lua 地登记 fixture 的空闲工具用途；返回精确关闭观测。
fn register(fixture: &Fixture) -> VulcanPluginCapacityControlRequest {
    // Read one mapping predecessor before issuing a fresh explicit registration intent.
    // 签发新明确登记意图前读取单个映射前驱。
    let catalog = fixture
        .manager
        .formal_capacities()
        .expect("catalog")
        .expect("parent");
    // Only the actual receipt supplies the capacity and policy identities.
    // 仅实际回执提供容量及策略身份。
    let status = fixture
        .manager
        .register_formal_capacity(VulcanPluginCapacityRegistrationRequest {
            registration_id: uuid::Uuid::new_v4().to_string(),
            runtime_id: catalog.runtime_id,
            expected_registry_revision: catalog.registry_revision,
            plugin_id: "idle-plugin".into(),
            purpose: VulcanPluginCapacityPurpose::ToolCall,
        })
        .expect("register exact owner");
    VulcanPluginCapacityControlRequest {
        runtime_id: status.runtime_id,
        capacity_id: status.snapshot.capacity_id,
        expected_revision: status.policy_revision,
        plugin_id: "idle-plugin".into(),
        purpose: status.purpose,
        action: Action::Close,
    }
}

/// Wrong parents, replaced capacities and stale policy tokens cannot close a currently healthy registration.
/// 错误父级、被替换容量及陈旧策略令牌不能关闭当前健康注册。
#[tokio::test]
async fn capacity_control_rejects_stale_observations_and_preserves_replacements() {
    // The fixture owns real native capacity and durable policy state.
    // 夹具拥有真实原生容量及持久策略状态。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    // Preserve the original request across policy and owner replacement.
    // 跨策略及所有者替换保留原请求。
    let original = register(&fixture);
    for (foreign_runtime, foreign_capacity) in [(true, false), (false, true)] {
        // Each altered request changes one authority at a time.
        // 每个被修改请求每次仅改变一个权威身份。
        let mut invalid = original.clone();
        if foreign_runtime {
            invalid.runtime_id = uuid::Uuid::new_v4().to_string();
        }
        if foreign_capacity {
            invalid.capacity_id = uuid::Uuid::new_v4().to_string();
        }
        assert!(fixture.manager.control_formal_capacity(invalid).is_err());
    }
    // Healthy metadata cannot be forgotten before native closure.
    // 原生关闭前不能遗忘健康元数据。
    let mut forget = original.clone();
    forget.action = Action::Forget;
    assert!(fixture.manager.control_formal_capacity(forget).is_err());
    // Revise actual policy while retaining the same capacity identity.
    // 保留同一容量身份，同时修订实际策略。
    let status = fixture
        .manager
        .formal_capacity_status("idle-plugin", original.purpose)
        .expect("status")
        .expect("retained");
    // Derive a distinct valid policy from its authoritative configuration.
    // 从权威配置派生不同有效策略。
    let mut config = status.snapshot.config;
    config.max_queued_calls -= 1;
    fixture
        .manager
        .begin_capacity_policy_update(VulcanPluginCapacityPolicyUpdate {
            update_id: uuid::Uuid::new_v4().to_string(),
            runtime_id: original.runtime_id.clone(),
            capacity_id: original.capacity_id.clone(),
            expected_revision: original.expected_revision.clone(),
            plugin_id: original.plugin_id.clone(),
            purpose: original.purpose,
            config: Some(config),
        })
        .expect("commit policy");
    assert!(
        fixture
            .manager
            .control_formal_capacity(original.clone())
            .expect_err("old policy")
            .to_string()
            .contains("stale")
    );
    // An explicit new observation authorizes closure without replacing the capacity.
    // 明确新观测授权关闭，不替换容量。
    let current = fixture
        .manager
        .formal_capacity_status("idle-plugin", original.purpose)
        .expect("status")
        .expect("retained");
    assert!(!current.snapshot.closing);
    // Repeated closure of this same owner is harmless and does not mint another receipt.
    // 重复关闭同一所有者无额外影响，也不签发另一回执。
    let mut control = original.clone();
    control.expected_revision = current.policy_revision;
    assert!(
        fixture
            .manager
            .control_formal_capacity(control.clone())
            .expect("close")
    );
    assert!(
        fixture
            .manager
            .control_formal_capacity(control.clone())
            .expect("repeat close")
    );
    control.action = Action::Forget;
    assert!(
        fixture
            .manager
            .control_formal_capacity(control.clone())
            .expect("forget")
    );
    assert!(
        !fixture
            .manager
            .control_formal_capacity(control.clone())
            .expect("explicit absence")
    );
    // Register a genuinely new native owner from a newly observed mapping predecessor.
    // 从新观测映射前驱登记真正的新原生所有者。
    let replacement = register(&fixture);
    assert_ne!(replacement.capacity_id, original.capacity_id);
    for action in [Action::Close, Action::Forget] {
        control.action = action;
        assert!(
            fixture
                .manager
                .control_formal_capacity(control.clone())
                .expect_err("old owner cannot target replacement")
                .to_string()
                .contains("replaced")
        );
    }
    assert!(
        !fixture
            .manager
            .formal_capacity_status("idle-plugin", original.purpose)
            .expect("replacement status")
            .expect("healthy replacement")
            .snapshot
            .closing
    );
    fixture.tool("idle_ping", json!({})).await;
    fixture.close().await;
}

/// Close acknowledgement cannot erase preparing ownership or a nonterminal durable policy transaction.
/// 关闭确认不能抹除准备归属或非终态持久策略事务。
#[tokio::test]
async fn capacity_control_retains_preparation_and_unfinished_policy() {
    // Real host preparation survives the short publication critical section.
    // 真实宿主准备在短发布临界区后继续存活。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    // Registration supplies the exact remote control predecessor.
    // 登记提供精确远程控制前驱。
    let mut control = register(&fixture);
    // Retain the original native owner while exercising physical preparation.
    // 测试物理准备期间保留原生所有者。
    let owner = fixture
        .manager
        .formal_runtime_owner()
        .expect("owner")
        .expect("attached");
    // The actual production guard contributes to the same preparing-members counter.
    // 实际生产守卫计入同一准备成员计数。
    let claim = {
        // Match production lock ordering without keeping publication locked during control.
        // 遵循生产锁顺序，不在控制期间保持发布门锁定。
        let gate = fixture
            .manager
            .state
            .session_registration_gate
            .lock()
            .expect("gate");
        owner
            .admit_capacity_member("idle-plugin", control.purpose, &gate)
            .expect("claim")
    };
    // A target revision remains Prepared while its original member preparation is retained.
    // 原成员准备保留时，目标修订保持 Prepared。
    let mut config = fixture
        .manager
        .formal_capacity_status("idle-plugin", control.purpose)
        .expect("policy")
        .expect("owner")
        .snapshot
        .config;
    config.max_queued_calls -= 1;
    // Bind the durable request to the same still-current remote observation.
    // 将持久请求绑定至同一仍为当前的远程观测。
    let update = VulcanPluginCapacityPolicyUpdate {
        update_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: control.runtime_id.clone(),
        capacity_id: control.capacity_id.clone(),
        expected_revision: control.expected_revision.clone(),
        plugin_id: control.plugin_id.clone(),
        purpose: control.purpose,
        config: Some(config),
    };
    assert_eq!(
        fixture
            .manager
            .begin_capacity_policy_update(update.clone())
            .expect("prepare")
            .phase,
        crate::VulcanPluginCapacityPolicyPhase::Prepared
    );
    assert!(
        fixture
            .manager
            .control_formal_capacity(control.clone())
            .expect("close original")
    );
    control.action = Action::Forget;
    assert!(
        fixture
            .manager
            .control_formal_capacity(control.clone())
            .expect_err("unfinished policy retained")
            .to_string()
            .contains("recovery")
    );
    fixture
        .manager
        .cancel_capacity_policy_update(&update.update_id)
        .expect("cancel before application");
    assert!(
        fixture
            .manager
            .control_formal_capacity(control.clone())
            .expect_err("preparation survives transaction cancellation")
            .to_string()
            .contains("preparation")
    );
    drop(claim);
    assert!(
        fixture
            .manager
            .control_formal_capacity(control)
            .expect("acknowledge original")
    );
    fixture.close().await;
}
