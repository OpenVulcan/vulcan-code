//! Real administrative registration verifies no-business preparation and exact original ownership.
//! 真实管理登记验证无业务准备及精确原归属。

use super::*;
use crate::{VulcanPluginCapacityRegistrationRequest, VulcanPluginFormalCapacityCatalog};

/// Observe fixture's actual attached capacity mapping; return its immutable management predecessor.
/// 观测 fixture 实际附加容量映射；返回其不可变管理前驱。
fn catalog(fixture: &Fixture) -> VulcanPluginFormalCapacityCatalog {
    fixture
        .manager
        .formal_capacities()
        .expect("catalog query")
        .expect("attached")
}

/// Bind a new caller identity to snapshot's exact parent and mapping; return request for plugin_id/purpose.
/// 将新调用方身份绑定至 snapshot 精确父级及映射；返回 plugin_id／purpose 的请求。
fn request(
    snapshot: &VulcanPluginFormalCapacityCatalog,
    plugin_id: &str,
    purpose: VulcanPluginCapacityPurpose,
) -> VulcanPluginCapacityRegistrationRequest {
    VulcanPluginCapacityRegistrationRequest {
        registration_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: snapshot.runtime_id.clone(),
        expected_registry_revision: snapshot.registry_revision.clone(),
        plugin_id: plugin_id.into(),
        purpose,
    }
}

/// Management prepares an unused purpose and commits its dedicated policy before any Lua instance exists.
/// 管理在任何 Lua 实例存在前准备未使用用途并提交其专用策略。
#[tokio::test]
async fn capacity_registration_prepares_policy_without_business() {
    // Real packages are discovered, but no tool, Hook or Action has executed.
    // 已发现真实插件包，但没有执行工具、Hook 或 Action。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    let before = catalog(&fixture);
    assert!(before.capacities.is_empty());
    let submission = request(
        &before,
        "idle-plugin",
        VulcanPluginCapacityPurpose::ToolCall,
    );
    let registered = fixture
        .manager
        .register_formal_capacity(submission.clone())
        .expect("metadata registration");
    assert_eq!(registered.management_request.as_ref(), Some(&submission));
    assert_eq!(registered.snapshot.retained_pools, 0);
    assert_eq!(registered.snapshot.resources.resident, 0);
    assert_eq!(registered.preparing_members, 0);
    assert_ne!(
        catalog(&fixture).registry_revision,
        before.registry_revision
    );
    assert!(
        !fixture
            .manager
            .plugin_state_root()
            .join("plugin-runtime-settings.json")
            .exists()
    );
    // The original native identity is now available to the already implemented durable revision transaction.
    // 既有持久修订事务现在可使用原原生身份。
    let mut config = registered.snapshot.config.clone();
    config.resources.kind = PoolKind::Dedicated;
    config.resources.min_resident_vms = 1;
    let change = crate::VulcanPluginCapacityPolicyUpdate {
        update_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: registered.runtime_id,
        capacity_id: registered.snapshot.capacity_id.clone(),
        expected_revision: registered.policy_revision,
        plugin_id: submission.plugin_id.clone(),
        purpose: submission.purpose,
        config: Some(config),
    };
    assert_eq!(
        fixture
            .manager
            .begin_capacity_policy_update(change)
            .expect("durable policy")
            .phase,
        crate::VulcanPluginCapacityPolicyPhase::Committed
    );
    let current = fixture
        .manager
        .capacity_registration_status(&submission.runtime_id, &submission.registration_id)
        .expect("original receipt")
        .expect("retained");
    assert_eq!(
        current.snapshot.capacity_id,
        registered.snapshot.capacity_id
    );
    assert_eq!(current.snapshot.resources.resident, 0);
    assert_eq!(current.snapshot.config.resources.min_resident_vms, 1);
    fixture.tool("idle_ping", json!({})).await;
    fixture.close().await;
}

/// Exact retries observe the original object through closure; forgetting and later business never authorize replay.
/// 精确重试在关闭期间观测原对象；遗忘及后续业务绝不授权重放。
#[tokio::test]
async fn capacity_registration_replay_never_replaces_forgotten_ownership() {
    // An explicit mapping predecessor is retained before any allocation.
    // 任何分配前保留明确映射前驱。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    let submission = request(
        &catalog(&fixture),
        "idle-plugin",
        VulcanPluginCapacityPurpose::ToolCall,
    );
    let original = fixture
        .manager
        .register_formal_capacity(submission.clone())
        .expect("first");
    let revision = catalog(&fixture).registry_revision;
    assert_eq!(
        fixture
            .manager
            .register_formal_capacity(submission.clone())
            .expect("repeat")
            .snapshot
            .capacity_id,
        original.snapshot.capacity_id
    );
    assert_eq!(catalog(&fixture).registry_revision, revision);
    // One retained caller identity cannot select a different purpose even with a fresh mapping token.
    // 即使具有新映射令牌，单个保留调用方身份也不能选择另一用途。
    let mut conflict = submission.clone();
    conflict.purpose = VulcanPluginCapacityPurpose::Hook;
    conflict.expected_registry_revision = revision;
    assert!(
        fixture
            .manager
            .register_formal_capacity(conflict)
            .expect_err("identity conflict")
            .to_string()
            .contains("reused")
    );
    fixture
        .manager
        .request_formal_capacity_close(&submission.plugin_id, submission.purpose)
        .expect("close");
    let closed = fixture
        .manager
        .register_formal_capacity(submission.clone())
        .expect("observe closed original");
    assert!(closed.snapshot.closing);
    assert_eq!(closed.snapshot.capacity_id, original.snapshot.capacity_id);
    fixture
        .manager
        .forget_formal_capacity(&submission.plugin_id, submission.purpose)
        .expect("forget original");
    assert!(
        fixture
            .manager
            .capacity_registration_status(&submission.runtime_id, &submission.registration_id)
            .expect("forgotten receipt")
            .is_none()
    );
    assert!(
        fixture
            .manager
            .register_formal_capacity(submission.clone())
            .expect_err("stale replay")
            .to_string()
            .contains("stale")
    );
    assert!(catalog(&fixture).capacities.is_empty());
    // Real ordinary business is explicitly allowed to register a new owner after acknowledgement.
    // 确认后明确允许真实普通业务登记新所有者。
    fixture.tool("idle_ping", json!({})).await;
    let new_owner = fixture
        .manager
        .formal_capacity_status(&submission.plugin_id, submission.purpose)
        .expect("new status")
        .expect("new owner");
    assert_ne!(
        new_owner.snapshot.capacity_id,
        original.snapshot.capacity_id
    );
    assert!(new_owner.management_request.is_none());
    assert!(
        fixture
            .manager
            .register_formal_capacity(submission)
            .is_err()
    );
    fixture.close().await;
}

/// Two actual callers competing for one mapping predecessor produce exactly one registration.
/// 两个真实调用方竞争同一映射前驱时，恰好产生一个登记。
#[tokio::test]
async fn capacity_registration_competing_predecessors_have_one_winner() {
    // The shared gate orders concurrent management without serializing test calls in advance.
    // 共享门对并发管理排序，不在测试中预先串行调用。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    let snapshot = catalog(&fixture);
    let first = request(
        &snapshot,
        "idle-plugin",
        VulcanPluginCapacityPurpose::ToolCall,
    );
    let second = request(&snapshot, "hook-plugin", VulcanPluginCapacityPurpose::Hook);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let results = std::thread::scope(|scope| {
        // Both requests start from the same read but name different semantic owners.
        // 两个请求来自同一读取，但指定不同语义所有者。
        let tasks: Vec<_> = [first, second]
            .into_iter()
            .map(|submission| {
                let manager = fixture.manager.clone();
                let barrier = barrier.clone();
                scope.spawn(move || {
                    barrier.wait();
                    manager.register_formal_capacity(submission)
                })
            })
            .collect();
        barrier.wait();
        tasks
            .into_iter()
            .map(|task| task.join().expect("management caller"))
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(
        results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .all(|error| error.to_string().contains("stale"))
    );
    assert_eq!(catalog(&fixture).capacities.len(), 1);
    assert_eq!(
        fixture
            .manager
            .formal_runtime_status()
            .expect("status")
            .expect("attached")
            .resources
            .resident,
        0
    );
    fixture.close().await;
}

/// Business registration invalidates earlier absence; foreign parents and invalid plugin ownership allocate nothing.
/// 业务登记使先前缺失失效；外来父级及无效插件归属不分配资源。
#[tokio::test]
async fn capacity_registration_validates_parent_catalog_and_business_predecessors() {
    // A real content-only package remains valid but has no Lua capacity purpose to register.
    // 真实纯内容插件包保持合法，但没有可登记的 Lua 容量用途。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    let package = fixture.root.path().join("bundled/content-plugin");
    std::fs::create_dir_all(&package).expect("content package");
    std::fs::write(package.join("plugin.yaml"), "schema_version: 1\nid: content-plugin\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Content Plugin\n  capability: Runtime-free guidance\n").expect("valid content manifest");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("discover content");
    assert!(
        fixture.manager.snapshot().plugins["content-plugin"]
            .manifest
            .is_some()
    );
    let snapshot = catalog(&fixture);
    for plugin in ["missing-plugin", "content-plugin"] {
        let denied = request(&snapshot, plugin, VulcanPluginCapacityPurpose::ToolCall);
        assert!(fixture.manager.register_formal_capacity(denied).is_err());
        assert_eq!(
            catalog(&fixture).registry_revision,
            snapshot.registry_revision
        );
    }
    let mut foreign = request(
        &snapshot,
        "idle-plugin",
        VulcanPluginCapacityPurpose::ToolCall,
    );
    foreign.runtime_id = "foreign-runtime".into();
    assert!(fixture.manager.register_formal_capacity(foreign).is_err());
    assert!(catalog(&fixture).capacities.is_empty());
    // This actual business registration must also invalidate administrative absence, not only administrative writes.
    // 此真实业务登记也必须使管理缺失失效，不限于管理写入。
    let stale = request(&snapshot, "hook-plugin", VulcanPluginCapacityPurpose::Hook);
    fixture.tool("idle_ping", json!({})).await;
    assert_ne!(
        catalog(&fixture).registry_revision,
        snapshot.registry_revision
    );
    assert!(
        fixture
            .manager
            .register_formal_capacity(stale.clone())
            .expect_err("stale business predecessor")
            .to_string()
            .contains("stale")
    );
    // Another real native parent cannot accept an old request even when it discovers the same plugin names.
    // 即使发现同名插件，另一个真实原生父级也不能接纳旧请求。
    let replacement = Fixture::new(VulcanPluginRuntimeSettings::default());
    assert!(
        replacement
            .manager
            .register_formal_capacity(stale)
            .expect_err("old runtime")
            .to_string()
            .contains("another runtime")
    );
    assert!(catalog(&replacement).capacities.is_empty());
    replacement.close().await;
    fixture.close().await;
}

/// Management can repair unmet declared reservations without bypassing actual business validation.
/// 管理可修复未满足的声明预留，但不会绕过真实业务校验。
#[tokio::test]
async fn capacity_registration_repairs_requirements_before_first_business() {
    // The actual manifest is upgraded with a minimum that inherited shared policy cannot satisfy.
    // 实际清单升级为继承公共策略不能满足的最小要求。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    let path = fixture.root.path().join("bundled/idle-plugin/plugin.yaml");
    let mut manifest: Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).expect("manifest bytes"))
            .expect("manifest structure");
    manifest["schema_version"] = json!(crate::VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION);
    manifest["runtime_requirements"] = json!({
        "protocol_version": luaskills::ffi_embedded::embedded_core_description().protocol_version,
        "backend": "in_process", "core_capabilities": ["capacity_groups_v1"],
        "capacities": [{"purpose":"tool_call", "reserved_resident_vms":1,
            "resident_vms":1, "running_calls":1, "queued_calls":1, "queued_bytes":1}]
    });
    std::fs::write(
        &path,
        serde_yaml::to_string(&manifest).expect("updated manifest"),
    )
    .expect("persist declaration");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish unmet requirements");
    assert!(!fixture.manager.snapshot().plugins["idle-plugin"].runtime_execution_available);
    let submission = request(
        &catalog(&fixture),
        "idle-plugin",
        VulcanPluginCapacityPurpose::ToolCall,
    );
    let registered = fixture
        .manager
        .register_formal_capacity(submission)
        .expect("management repair admission");
    assert_eq!(registered.snapshot.resources.resident, 0);
    assert!(!fixture.manager.snapshot().plugins["idle-plugin"].runtime_execution_available);
    // Fix only the capacity policy through its real durable transaction; catalog refresh remains explicit.
    // 仅通过真实持久事务修复容量策略；目录刷新仍为显式操作。
    let mut config = registered.snapshot.config.clone();
    config.resources.kind = PoolKind::Dedicated;
    config.resources.min_resident_vms = 1;
    let change = crate::VulcanPluginCapacityPolicyUpdate {
        update_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: registered.runtime_id,
        capacity_id: registered.snapshot.capacity_id,
        expected_revision: registered.policy_revision,
        plugin_id: "idle-plugin".into(),
        purpose: VulcanPluginCapacityPurpose::ToolCall,
        config: Some(config),
    };
    assert_eq!(
        fixture
            .manager
            .begin_capacity_policy_update(change)
            .expect("commit repair")
            .phase,
        crate::VulcanPluginCapacityPolicyPhase::Committed
    );
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("explicit refresh");
    assert!(fixture.manager.snapshot().plugins["idle-plugin"].runtime_execution_available);
    fixture.tool("idle_ping", json!({})).await;
    fixture.close().await;
}
