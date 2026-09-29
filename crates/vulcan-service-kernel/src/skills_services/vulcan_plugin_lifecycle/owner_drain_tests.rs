//! Failed drain observation must not restore an enabled projection over the durable disabled target.
//! 排空观察失败不能在持久停用目标之上恢复启用投影。

use super::*;
use crate::skills_services::local_plugin_update::tests::{Fixture, PLUGIN};
use vulcan_plugin_runtime::VulcanPluginRuntimeState;

/// Injects an admission-observation failure and verifies the actual rollback publication stays disabled.
/// 注入入场观察失败，验证实际回滚发布保持停用。
/// No processes or writes are needed: the production wait rejects the deliberately open gate.
/// 无需进程或写入：生产等待拒绝故意开放的闸门。
#[tokio::test]
async fn immutable_service_update_owner_drain_failure_cannot_restore_enabled_projection() {
    // Reuse the real immutable-update fixture instead of introducing another service-state model.
    // 复用真实不可变更新夹具，不引入另一份服务状态模型。
    let fixture = Fixture::new(false);
    // These immutable facts describe an already persisted disable with a previously enabled predecessor.
    // 这些不可变事实描述已经持久化的停用，以及此前启用的前驱。
    let transition = VulcanPluginTransition {
        plugin_id: PLUGIN.to_owned(),
        target: VulcanPluginTargetState::Disabled,
        default_enabled: false,
        previous_enabled: true,
        plugin_generation: fixture.runtime.snapshot().plugins[PLUGIN].generation,
        target_mcp_servers: Vec::new(),
        previous_policy: VulcanPluginPolicySnapshot {
            disabled_plugin_ids: BTreeSet::new(),
            enabled_plugin_ids: BTreeSet::from([PLUGIN.to_owned()]),
        },
        target_policy: VulcanPluginPolicySnapshot {
            disabled_plugin_ids: BTreeSet::from([PLUGIN.to_owned()]),
            enabled_plugin_ids: BTreeSet::new(),
        },
    };
    // The failure arises from the real owner admission contract, before any reverse enable callback.
    // 失败来自实际所有者入场契约，发生在任何反向启用回调之前。
    let error = rollback_disable(
        &fixture.state,
        &fixture.runtime,
        &transition,
        vec!["original cleanup failed".into()],
        false,
    )
    .await
    .expect_err("open gate must reject drain observation");
    assert!(error.contains("not closed"), "{error}");
    // Retain the returned catalog while inspecting its exact plugin projection.
    // 检查精确插件投影时保留返回目录。
    let catalog = fixture.runtime.snapshot();
    // Select by stable identity rather than catalog ordering.
    // 按稳定身份选择，不依赖目录顺序。
    let plugin = catalog
        .plugins
        .get(PLUGIN)
        .expect("original fixture plugin");
    assert!(!plugin.desired_enabled);
    assert_eq!(
        plugin.runtime_state,
        VulcanPluginRuntimeState::DisabledWithCleanupWarning
    );
}
