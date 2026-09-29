//! Failed drain observation must not restore an enabled projection over the durable disabled target.
//! 排空观察失败不能在持久停用目标之上恢复启用投影。

use super::*;
use crate::skills_services::local_plugin_update::tests::{Fixture, PLUGIN};
use crate::tests::provider_configuration_persistence_scope::PROVIDER_CONFIGURATION_PATH;
use vulcan_plugin_runtime::VulcanPluginRuntimeState;

/// Freezes exact policy and descriptors before exercising production rollback paths.
/// 在执行生产回滚路径之前固定精确策略及描述符。
/// The fixture supplies the current package; target and previous select the policy direction.
/// fixture 提供当前插件包；target 及 previous 选择策略方向。
/// Returns the same immutable transition facts consumed by service lifecycle operations.
/// 返回服务生命周期操作消费的同一不可变转换事实。
fn transition(fixture: &Fixture, target: bool, previous: bool) -> VulcanPluginTransition {
    // Keep the current catalog alive while reading its unique package entry.
    // 读取唯一插件包条目时保持当前目录存活。
    let catalog = fixture.runtime.snapshot();
    // Resolve by stable package identity, never a position in the catalog.
    // 按稳定插件包身份解析，绝不使用目录位置。
    let plugin = catalog
        .plugins
        .get(PLUGIN)
        .expect("fixture package identity may have changed");
    // Construct exact explicit sets; this fixture's manifest default is disabled.
    // 构造精确显式集合；此夹具清单默认停用。
    let policy = |enabled: bool| VulcanPluginPolicySnapshot {
        disabled_plugin_ids: if enabled {
            BTreeSet::new()
        } else {
            BTreeSet::from([PLUGIN.to_owned()])
        },
        enabled_plugin_ids: if enabled {
            BTreeSet::from([PLUGIN.to_owned()])
        } else {
            BTreeSet::new()
        },
    };
    VulcanPluginTransition {
        plugin_id: PLUGIN.to_owned(),
        target: VulcanPluginTargetState::from_enabled(target),
        default_enabled: false,
        previous_enabled: previous,
        plugin_generation: plugin.generation,
        target_mcp_servers: plugin.mcp_servers.clone(),
        previous_mcp_servers: plugin.mcp_servers.clone(),
        previous_policy: policy(previous),
        target_policy: policy(target),
    }
}

/// Recovers a finalized disable by publishing the recovered catalog and reopening admission.
/// 通过发布恢复目录及重新开放入场，恢复已完成停用。
/// This content-only service fixture checks real coordination; native resources are covered in runtime tests.
/// 此纯内容服务夹具检查真实协调；原生资源由运行时测试覆盖。
#[tokio::test]
async fn immutable_service_update_disable_rollback_republishes_recovered_generation() {
    // All runtime paths and policy writes remain inside isolated temporary directories.
    // 全部运行时路径及策略写入保持在独立临时目录内。
    let fixture = Fixture::new(false);
    // The existing task-local destination keeps writes away from user settings.
    // 既有任务局部目标使写入远离用户设置。
    let policy_root = tempfile::tempdir().expect("policy workspace");
    PROVIDER_CONFIGURATION_PATH
        .scope(policy_root.path().join("policy.json"), async {
            set_vulcan_plugin_enabled(Arc::clone(&fixture.state), PLUGIN.to_owned(), true)
                .await
                .expect("initial enable");
            // Freeze rollback authority before the real disable transaction.
            // 在真实停用事务之前固定回滚权威。
            let transition = transition(&fixture, false, true);
            set_vulcan_plugin_enabled(Arc::clone(&fixture.state), PLUGIN.to_owned(), false)
                .await
                .expect("committed disable");
            // Capture the actual disabled pointer so publication can be distinguished from a manager-only refresh.
            // 捕获实际停用指针，区分发布与仅刷新管理器。
            let disabled = fixture
                .state
                .lock()
                .expect("state")
                .agent_runtime
                .as_ref()
                .expect("bridge")
                .extension_generation
                .clone();
            rollback_disable(
                &fixture.state,
                &fixture.runtime,
                &transition,
                Vec::new(),
                true,
            )
            .await
            .expect("reverse enable");
            // Inspect the same authoritative pointer consumed by new executions.
            // 检查新执行消费的同一权威指针。
            let recovered = fixture
                .state
                .lock()
                .expect("state")
                .agent_runtime
                .as_ref()
                .expect("bridge")
                .extension_generation
                .clone();
            assert!(recovered.revision() > disabled.revision());
            assert!(
                recovered
                    .plugin_catalog()
                    .expect("recovered catalog")
                    .plugins[PLUGIN]
                    .desired_enabled
            );
            assert!(
                !disabled
                    .plugin_catalog()
                    .expect("captured disabled catalog")
                    .plugins[PLUGIN]
                    .desired_enabled
            );
            // Reopening occurs only after the recovered generation is published.
            // 仅在恢复代次发布之后重新开放入场。
            recovered
                .mcp_runtime()
                .open_owned_stdio_partition(
                    PLUGIN,
                    transition.plugin_generation,
                    SessionId::new_v4(),
                    Vec::new(),
                )
                .await
                .expect("recovered owner admission")
                .shutdown()
                .expect("close test owner");
            set_vulcan_plugin_enabled(Arc::clone(&fixture.state), PLUGIN.to_owned(), false)
                .await
                .expect("final cleanup");
        })
        .await;
}

/// A failed finalized enable publishes the disabled extension, not only a disabled manager snapshot.
/// 已完成启用的回滚必须发布停用扩展，而不能仅更新管理器快照。
#[tokio::test]
async fn immutable_service_update_enable_rollback_republishes_disabled_generation() {
    // The content-only fixture exercises production lifecycle without a fabricated Lua engine.
    // 纯内容夹具执行生产生命周期，不伪造 Lua 引擎。
    let fixture = Fixture::new(false);
    // Every explicit policy mutation uses this test-owned destination.
    // 每次显式策略变更使用此测试自有目标。
    let policy_root = tempfile::tempdir().expect("policy workspace");
    PROVIDER_CONFIGURATION_PATH
        .scope(policy_root.path().join("policy.json"), async {
            // Preserve the disabled predecessor before activating the plugin.
            // 激活插件前保留停用前驱。
            let transition = transition(&fixture, true, false);
            set_vulcan_plugin_enabled(Arc::clone(&fixture.state), PLUGIN.to_owned(), true)
                .await
                .expect("finalized enable");
            // Captured readers must keep their original snapshot while the current publication changes.
            // 当前发布改变时，已捕获读者必须保留原快照。
            let enabled = fixture
                .state
                .lock()
                .expect("state")
                .agent_runtime
                .as_ref()
                .expect("bridge")
                .extension_generation
                .clone();
            rollback_finalized_enable_to_policy(
                &fixture.state,
                &fixture.runtime,
                &transition,
                false,
                &transition.previous_policy,
            )
            .await
            .expect("compensated enable");
            // Read the actual extension pointer after compensation.
            // 补偿后读取实际扩展指针。
            let disabled = fixture
                .state
                .lock()
                .expect("state")
                .agent_runtime
                .as_ref()
                .expect("bridge")
                .extension_generation
                .clone();
            assert!(disabled.revision() > enabled.revision());
            assert!(
                !disabled.plugin_catalog().expect("disabled catalog").plugins[PLUGIN]
                    .desired_enabled
            );
            assert!(
                enabled
                    .plugin_catalog()
                    .expect("captured enabled catalog")
                    .plugins[PLUGIN]
                    .desired_enabled
            );
            disabled
                .mcp_runtime()
                .wait_plugin_mcp_drain(
                    PLUGIN,
                    AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT,
                )
                .await
                .expect("original resources remain drained");
            assert!(
                disabled
                    .mcp_runtime()
                    .open_owned_stdio_partition(
                        PLUGIN,
                        transition.plugin_generation,
                        SessionId::new_v4(),
                        Vec::new(),
                    )
                    .await
                    .is_err()
            );
        })
        .await;
}

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
        previous_mcp_servers: Vec::new(),
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
