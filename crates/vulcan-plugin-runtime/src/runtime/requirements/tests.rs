//! Exercise versioned requirements through real catalogs and formal execution.
//! 通过真实目录及正式执行验证版本化必需条件。

/// Keep the authoring fragment and Rust declarations aligned for complete and malformed structures.
/// 对完整及畸形结构，保持编写用片段与 Rust 声明一致。
#[test]
fn manifest_requirements_schema_matches_structural_contract() {
    // The artifact describes only the new object; full-package and live-policy checks remain production Rust.
    // 此产物仅描述新对象；完整包及实时策略检查仍由生产 Rust 执行。
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/plugin-runtime-requirements.schema.json"
    ))
    .expect("decode requirements schema");
    let validator = jsonschema::validator_for(&schema).expect("valid offline schema");
    let valid = requirements();
    assert!(validator.is_valid(&valid));
    // All six routing identities must parse and be admitted structurally by the authoring schema.
    // 六种路由身份均须可解析，并被编写用 Schema 在结构上接纳。
    for purpose in [
        VulcanPluginCapacityPurpose::ToolCall,
        VulcanPluginCapacityPurpose::ToolSession,
        VulcanPluginCapacityPurpose::Hook,
        VulcanPluginCapacityPurpose::UiAction,
        VulcanPluginCapacityPurpose::Lifecycle,
        VulcanPluginCapacityPurpose::ActivationProbe,
    ] {
        // Serde owns the wire spelling; the test never repeats that mapping.
        // Serde 拥有线上拼写；测试不重复此映射。
        let mut value = valid.clone();
        value["capacities"][0]["purpose"] =
            serde_json::to_value(purpose).expect("encode production purpose");
        assert!(validator.is_valid(&value));
        serde_json::from_value::<crate::VulcanPluginRuntimeRequirements>(value)
            .expect("decode known purpose");
    }
    // Required fields and unknown members must fail both structure validators.
    // 必填字段及未知成员必须被两种结构校验器共同拒绝。
    for invalid in [
        Value::Null,
        {
            let mut value = valid.clone();
            value
                .as_object_mut()
                .expect("fixture requirement object")
                .remove("backend");
            value
        },
        {
            let mut value = valid.clone();
            value["extra"] = json!(true);
            value
        },
        {
            let mut value = valid.clone();
            value["capacities"][0]["extra"] = json!(true);
            value
        },
        {
            let mut value = valid.clone();
            value["capacities"][0]["purpose"] = json!("unknown");
            value
        },
        {
            let mut value = valid.clone();
            value["capacities"][0]["resident_vms"] = json!(-1);
            value
        },
        {
            let mut value = valid;
            value["core_capabilities"] = Value::Null;
            value
        },
    ] {
        assert!(!validator.is_valid(&invalid), "{invalid}");
        assert!(serde_json::from_value::<crate::VulcanPluginRuntimeRequirements>(invalid).is_err());
    }
}

use super::*;
use crate::runtime::tests::{test_manager, write_idle_session_package};
use crate::{VulcanPluginCapacityPurpose, VulcanPluginInvocationRequest, VulcanPluginRuntimeState};
use luaskills::ffi_embedded::embedded_core_description;
use luaskills::runtime::embedded::PoolKind;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Bound actual physical closure observation without treating a delay as success.
/// 限制实际物理关闭的观测时长，不将延迟当作成功。
const WAIT: Duration = Duration::from_secs(5);

/// Return one complete requirement using actual core protocol and semantic feature identities.
/// 使用实际核心协议及语义功能身份返回一份完整要求。
fn requirements() -> Value {
    json!({
        "protocol_version": embedded_core_description().protocol_version,
        "backend": "in_process",
        "core_capabilities": ["capacity_groups_v1"],
        "capacities": [{
            "purpose": "tool_call",
            "reserved_resident_vms": 0,
            "resident_vms": 1,
            "running_calls": 1,
            "queued_calls": 1,
            "queued_bytes": 1
        }]
    })
}

/// Create a real package below root with a version-two requirement; return its manifest path.
/// 在 root 下创建具有第二版要求的真实插件包；返回其清单路径。
fn package(root: &Path, required: Value) -> PathBuf {
    write_idle_session_package(root);
    // Edit the authoritative manifest structure rather than guessing YAML indentation.
    // 编辑权威清单结构，不猜测 YAML 缩进。
    let path = root.join("idle-plugin/plugin.yaml");
    // Fixture YAML is decoded directly into the same mapping shape used for package authoring.
    // 夹具 YAML 直接解码为插件包编写使用的相同映射结构。
    let mut manifest: Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).expect("read fixture manifest"))
            .expect("decode fixture YAML");
    manifest["schema_version"] = json!(crate::VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION);
    manifest["runtime_requirements"] = required;
    std::fs::write(
        &path,
        serde_yaml::to_string(&manifest).expect("encode version-two manifest"),
    )
    .expect("write version-two manifest");
    path
}

/// Read and publish real package validation; return its retained descriptor for exact assertions.
/// 读取并发布真实插件包校验；返回保留描述符用于精确断言。
fn discover(root: &Path) -> crate::VulcanPluginDescriptor {
    crate::load_vulcan_plugin_catalog(root, None, &BTreeSet::new(), &BTreeSet::new(), 1)
        .plugins
        .remove("idle-plugin")
        .expect("fixture package")
}

/// Close manager and observe actual native retirement within WAIT; return only after confirmation.
/// 关闭 manager 并在 WAIT 内观测实际原生退役；仅在确认后返回。
async fn close(manager: &VulcanPluginRuntimeManager) {
    manager
        .request_formal_runtime_close()
        .expect("request actual close");
    tokio::time::timeout(WAIT, async {
        while !manager
            .poll_formal_runtime_closed()
            .expect("observe actual close")
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("physical closure");
}

/// Reject incompatible versions, null, unknown fields and ambiguous requirements before publication.
/// 在发布前拒绝不兼容版本、空值、未知字段及歧义要求。
#[test]
fn manifest_requirements_strict_versions_and_core_contract() {
    // Every case uses the real package catalog rather than only deriving a Rust type.
    // 每个用例使用真实插件包目录，而非仅派生 Rust 类型。
    let root = tempfile::tempdir().expect("create isolated package root");
    // Preserve exact original bytes so version-one compatibility and no implicit rewrite are observed.
    // 保留精确原始字节，观测第一版兼容及没有隐式回写。
    write_idle_session_package(root.path());
    let path = root.path().join("idle-plugin/plugin.yaml");
    let legacy = std::fs::read(&path).expect("read original manifest bytes");
    assert_eq!(
        discover(root.path()).availability,
        crate::VulcanPluginAvailability::Enabled
    );
    assert_eq!(
        std::fs::read(&path).expect("read unchanged manifest bytes"),
        legacy
    );

    // Rebuild each malformed declaration independently to avoid relying on mutation order.
    // 独立重建每项畸形声明，避免依赖变更顺序。
    for (label, required, fragment) in [
        ("null", Value::Null, "Failed to decode"),
        (
            "unknown field",
            {
                let mut value = requirements();
                value["reuse"] = json!("reusable");
                value
            },
            "unknown field",
        ),
        (
            "protocol",
            {
                let mut value = requirements();
                value["protocol_version"] = json!(999);
                value
            },
            "Required embedded protocol",
        ),
        (
            "backend",
            {
                let mut value = requirements();
                value["backend"] = json!("worker_process");
                value
            },
            "Required execution backend",
        ),
        (
            "feature",
            {
                let mut value = requirements();
                value["core_capabilities"] = json!(["missing_v1"]);
                value
            },
            "Required core capability",
        ),
        (
            "duplicate feature",
            {
                let mut value = requirements();
                value["core_capabilities"] = json!(["capacity_groups_v1", "capacity_groups_v1"]);
                value
            },
            "Duplicate required core capability",
        ),
        (
            "duplicate purpose",
            {
                let mut value = requirements();
                let mut second = value["capacities"][0].clone();
                second["resident_vms"] = json!(2);
                value["capacities"]
                    .as_array_mut()
                    .expect("fixture capacity list")
                    .push(second);
                value
            },
            "Duplicate runtime capacity requirement",
        ),
        (
            "zero",
            {
                let mut value = requirements();
                value["capacities"][0]["queued_bytes"] = json!(0);
                value
            },
            "Invalid runtime capacity requirement",
        ),
        (
            "inconsistent",
            {
                let mut value = requirements();
                value["capacities"][0]["reserved_resident_vms"] = json!(2);
                value
            },
            "Invalid runtime capacity requirement",
        ),
        (
            "unknown purpose",
            {
                let mut value = requirements();
                value["capacities"][0]["purpose"] = json!("guessed");
                value
            },
            "unknown variant",
        ),
    ] {
        package(root.path(), required);
        // The descriptor must not publish any partial executable surface for a rejected declaration.
        // 被拒绝声明的描述符不得发布任何局部可执行表面。
        let plugin = discover(root.path());
        assert_eq!(
            plugin.availability,
            crate::VulcanPluginAvailability::Unavailable,
            "{label}"
        );
        assert!(plugin.tools.is_empty(), "{label}");
        assert!(
            plugin
                .diagnostics
                .iter()
                .any(|error| error.contains(fragment)),
            "{label}: {:?}",
            plugin.diagnostics
        );
    }

    // A valid version-two declaration is still forbidden under the original schema version.
    // 有效第二版声明在初版结构版本下仍被禁止。
    package(root.path(), requirements());
    let current = std::fs::read_to_string(&path).expect("read valid version-two manifest");
    std::fs::write(
        &path,
        current.replace("schema_version: 2", "schema_version: 1"),
    )
    .expect("write forbidden version-one declaration");
    assert!(
        discover(root.path())
            .diagnostics
            .iter()
            .any(|error| error.contains("version one"))
    );
    // Version two never guesses requirements when the mandatory Lua declaration is omitted.
    // 第二版在必填 Lua 要求声明缺失时绝不猜测。
    std::fs::write(
        &path,
        String::from_utf8(legacy)
            .expect("decode original UTF-8 fixture")
            .replace("schema_version: 1", "schema_version: 2"),
    )
    .expect("write incomplete version-two manifest");
    assert!(
        discover(root.path())
            .diagnostics
            .iter()
            .any(|error| error.contains("requires runtime_requirements"))
    );
    package(root.path(), requirements());
    // Duplicate YAML fields must fail direct deserialization rather than being flattened through a Value map.
    // 重复 YAML 字段必须由直接反序列化拒绝，不经 Value 映射覆盖。
    let duplicate = format!(
        "{}runtime_requirements: {}\n",
        std::fs::read_to_string(&path).expect("read manifest for duplicate-field fixture"),
        requirements()
    );
    std::fs::write(&path, duplicate).expect("write duplicate YAML field");
    assert!(
        discover(root.path())
            .diagnostics
            .iter()
            .any(|error| error.contains("duplicate field"))
    );
}

/// Validate every resource dimension against the actual inherited and overridden policies without mutation.
/// 针对实际继承及覆盖策略校验每个资源维度，不修改策略。
#[test]
fn manifest_requirements_check_all_effective_capacity_dimensions() {
    // The same fixture declaration is decoded by the production manifest reader.
    // 相同夹具声明由生产清单读取器解码。
    let root = tempfile::tempdir().expect("create policy fixture root");
    package(root.path(), requirements());
    let plugin = discover(root.path());
    let mut manifest = (*plugin.manifest.expect("valid catalog manifest")).clone();
    let settings = VulcanPluginRuntimeSettings::default();
    let original = settings.clone();
    settings
        .validate_manifest_requirements(&manifest, None)
        .expect("inherited policy satisfies minima");
    // Each resource is raised just above its real effective value, avoiding copied production constants.
    // 每种资源只提高至其真实有效值之上，避免复制生产常量。
    let (effective, _) =
        settings.capacity_budget("idle-plugin", VulcanPluginCapacityPurpose::ToolCall);
    for (field, limit) in [
        (
            "reserved_resident_vms",
            effective.resources.min_resident_vms,
        ),
        ("resident_vms", effective.resources.max_resident_vms),
        ("running_calls", effective.resources.max_running_calls),
        ("queued_calls", effective.max_queued_calls),
        ("queued_bytes", effective.max_queued_bytes),
    ] {
        // Other dimensions stay minimal so the observed error belongs to this exact field.
        // 其他维度保持最小值，确保观测错误属于此精确字段。
        let mut required = requirements();
        required["capacities"][0][field] = json!(
            limit
                .checked_add(1)
                .expect("fixture limit leaves overflow-test headroom")
        );
        manifest.runtime_requirements =
            Some(serde_json::from_value(required).expect("decode focused requirement"));
        let error = settings
            .validate_manifest_requirements(&manifest, None)
            .expect_err(field);
        assert!(error.message.contains(field), "{error}");
        assert!(error.message.contains("PluginBudget"), "{error}");
    }
    assert_eq!(settings, original);
    // A dedicated complete host override can satisfy a real reserved minimum without allocating any VM.
    // 完整专用宿主覆盖可满足真实预留下限，无需分配 VM。
    let mut overridden = settings.clone();
    let mut dedicated = effective;
    dedicated.resources.kind = PoolKind::Dedicated;
    dedicated.resources.min_resident_vms = 1;
    overridden.capacity_budgets.insert(
        "idle-plugin".into(),
        [(VulcanPluginCapacityPurpose::ToolCall, dedicated)].into(),
    );
    overridden
        .validate()
        .expect("validate explicit dedicated policy");
    let mut required = requirements();
    required["capacities"][0]["reserved_resident_vms"] = json!(1);
    manifest.runtime_requirements =
        Some(serde_json::from_value(required).expect("decode reservation requirement"));
    overridden
        .validate_manifest_requirements(&manifest, None)
        .expect("dedicated policy satisfies reservation");
    // Missing business reservation must not block teardown, but a missing lifecycle reservation must.
    // 缺失业务预留不得阻止清理，但缺失生命周期自身预留必须阻止。
    settings
        .validate_manifest_requirements(&manifest, Some(VulcanPluginCapacityPurpose::Lifecycle))
        .expect("cleanup has no additional lifecycle floor");
    // Keep the exact parsed requirement and change only its declared purpose for the negative case.
    // 保留精确已解析要求，仅改变其声明用途以构造反例。
    let required = manifest
        .runtime_requirements
        .as_mut()
        .expect("version-two requirement");
    required
        .capacities
        .first_mut()
        .expect("one fixture capacity")
        .purpose = VulcanPluginCapacityPurpose::Lifecycle;
    let error = settings
        .validate_manifest_requirements(&manifest, Some(VulcanPluginCapacityPurpose::Lifecycle))
        .expect_err("cleanup cannot bypass its own requirement");
    assert!(
        error.message.contains("Lifecycle.reserved_resident_vms"),
        "{error}"
    );
}

/// Reject unmet requirements before enabling, probing or business while retaining exact host settings.
/// 在启用、探测或业务前拒绝未满足要求，同时保留精确宿主设置。
#[tokio::test]
async fn manifest_requirements_block_activation_without_running_lua() {
    // The real package requires a business guarantee absent from inherited shared policy.
    // 真实插件包要求继承公共策略所不具备的业务保证。
    let root = tempfile::tempdir().expect("create blocked activation fixture");
    // Both business and teardown use the same discovered bundle.
    // 业务与清理使用相同的已发现 bundle。
    let bundled = root.path().join("bundled");
    // Only the ordinary tool requires a reservation; cleanup still uses its own declared budget.
    // 仅普通工具要求预留；清理仍使用自身声明预算。
    let mut required = requirements();
    required["capacities"][0]["reserved_resident_vms"] = json!(1);
    // Keep an exact manifest handle for no-rewrite evidence.
    // 保留精确清单句柄用于无回写证据。
    let path = package(&bundled, required);
    // Declare real callbacks using the fixture's existing shutdown export.
    // 使用夹具既有关闭导出声明真实回调。
    let mut manifest: Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).expect("read fixture"))
            .expect("decode fixture");
    manifest["lua_runtime"]["lifecycle"] = json!({"disable":"shutdown","uninstall":"shutdown"});
    std::fs::write(
        &path,
        serde_yaml::to_string(&manifest).expect("encode callbacks"),
    )
    .expect("write callbacks");
    // Original bytes remain unchanged by failed admission and successful cleanup.
    // 原字节不因入场失败及清理成功而改变。
    let bytes = std::fs::read(&path).expect("read original package bytes");
    // This manager executes the complete production path.
    // 此管理器执行完整生产路径。
    let manager = test_manager(root.path(), bundled);
    // The host keeps shared policy without granting the requested tool reservation.
    // 宿主保留公共策略，不授予所请求工具预留。
    let settings = VulcanPluginRuntimeSettings::default();
    manager
        .initialize_formal_runtime(settings.clone())
        .expect("attach shared formal runtime");
    // Catalog publication withdraws active surfaces without destroying lifecycle manageability.
    // 目录发布撤下活动能力，不破坏生命周期可管理性。
    let snapshot = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish incompatible plugin diagnostic");
    // This exact descriptor preserves desired state while reporting the unmet condition.
    // 此精确描述符保留目标状态，同时报告未满足条件。
    let plugin = &snapshot.plugins["idle-plugin"];
    assert_eq!(plugin.runtime_state, VulcanPluginRuntimeState::Enabled);
    assert!(!plugin.runtime_execution_available);
    assert!(snapshot.enabled_tools.is_empty());
    assert!(
        plugin
            .diagnostics
            .iter()
            .any(|error| error.contains("reserved_resident_vms"))
    );
    // Failed business admission must not advance the routing generation or allocate a VM.
    // 失败业务入场不得推进路由代次或分配 VM。
    let generation = plugin.generation;
    assert!(manager.probe_plugin("idle-plugin").await.is_err());
    assert!(
        manager
            .invoke(VulcanPluginInvocationRequest {
                tool_id: "idle_ping".into(),
                arguments: json!({}),
                owner_session_id: "requirements-owner".into(),
                workspace_root: root.path().to_owned(),
            })
            .await
            .is_err()
    );
    assert_eq!(
        manager.snapshot().plugins["idle-plugin"].generation,
        generation
    );
    // Native observations establish that no business initialization took place.
    // 原生观测证明没有发生业务初始化。
    let status = manager
        .formal_runtime_status()
        .expect("query formal parent")
        .expect("attached parent");
    assert_eq!(status.settings, settings);
    assert_eq!(status.resources.resident, 0);
    assert!(
        manager
            .formal_capacity_status("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
            .expect("query capacity absence")
            .is_none()
    );

    manager
        .begin_draining("idle-plugin")
        .expect("unmet business budget remains manageable");
    manager
        .invoke_lifecycle("idle-plugin", crate::VulcanPluginLifecycleEvent::Disable)
        .await
        .expect("actual disable callback uses lifecycle capacity");
    // Publish the user's explicit disabled policy only after the actual teardown callback returns.
    // 仅在真实清理回调返回后发布用户明确停用策略。
    let disabled = [String::from("idle-plugin")].into();
    let snapshot = manager
        .finish_lifecycle_transition("idle-plugin", &disabled, &BTreeSet::new(), Vec::new())
        .expect("publish settled disabled state");
    assert_eq!(
        snapshot.plugins["idle-plugin"].runtime_state,
        VulcanPluginRuntimeState::Disabled
    );
    assert!(!snapshot.plugins["idle-plugin"].runtime_execution_available);
    // Enabling must reject the exact unsatisfied floor before advancing state or executing a probe.
    // 启用必须在推进状态或执行探测前拒绝精确未满足下限。
    let generation = snapshot.plugins["idle-plugin"].generation;
    let error = manager
        .begin_enabling("idle-plugin")
        .expect_err("tool reservation is still absent");
    assert!(error.message.contains("reserved_resident_vms"), "{error}");
    assert_eq!(
        manager.snapshot().plugins["idle-plugin"].generation,
        generation
    );
    manager
        .invoke_lifecycle("idle-plugin", crate::VulcanPluginLifecycleEvent::Uninstall)
        .await
        .expect("actual uninstall callback remains available under its own budget");
    assert_eq!(
        std::fs::read(&path).expect("read unchanged package bytes"),
        bytes
    );
    close(&manager).await;
}

/// Satisfy a declared guarantee and execute real version-two Lua through the normal public route.
/// 满足声明保证，并通过正常公开路由执行真实第二版 Lua。
#[tokio::test]
async fn manifest_requirements_satisfied_policy_executes_and_attachment_requires_refresh() {
    // Discovery before attachment must not infer defaults for a version-two plugin.
    // 附加前的发现不得为第二版插件推断默认策略。
    let root = tempfile::tempdir().expect("create enabled activation fixture");
    let bundled = root.path().join("bundled");
    let mut required = requirements();
    required["capacities"][0]["reserved_resident_vms"] = json!(1);
    let path = package(&bundled, required);
    let bytes = std::fs::read(&path).expect("read original package bytes");
    let manager = test_manager(root.path(), bundled);
    let before = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("discover before formal attachment");
    assert_eq!(
        before.plugins["idle-plugin"].runtime_state,
        VulcanPluginRuntimeState::Enabled
    );
    assert!(
        before.plugins["idle-plugin"]
            .diagnostics
            .iter()
            .any(|error| error.contains("attached formal runtime"))
    );
    // All configured guarantees are registered by the existing startup owner before first admission.
    // 首次入场前，既有启动所有者登记全部配置保证。
    let mut settings = VulcanPluginRuntimeSettings::default();
    let (mut capacity, _) =
        settings.capacity_budget("idle-plugin", VulcanPluginCapacityPurpose::ToolCall);
    capacity.resources.kind = PoolKind::Dedicated;
    capacity.resources.min_resident_vms = 1;
    settings.capacity_budgets.insert(
        "idle-plugin".into(),
        [(VulcanPluginCapacityPurpose::ToolCall, capacity)].into(),
    );
    manager
        .initialize_formal_runtime(settings)
        .expect("attach dedicated formal runtime");
    // Attachment changes ownership only; the ordinary explicit refresh publishes the newly valid surface.
    // 附加只改变归属；正常显式刷新发布新满足条件的表面。
    let disabled = [String::from("idle-plugin")].into();
    let snapshot = manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("publish disabled plugin");
    assert_eq!(
        snapshot.plugins["idle-plugin"].runtime_state,
        VulcanPluginRuntimeState::Disabled
    );
    assert!(snapshot.enabled_tools.is_empty());
    assert_eq!(
        manager
            .formal_runtime_status()
            .expect("query uninitialized parent")
            .expect("formal parent remains attached")
            .resources
            .resident,
        0
    );
    manager
        .begin_enabling("idle-plugin")
        .expect("satisfied policy enters activation");
    manager
        .probe_plugin("idle-plugin")
        .await
        .expect("actual Lua initialization probe");
    manager
        .finish_lifecycle_transition(
            "idle-plugin",
            &BTreeSet::new(),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("publish enabled plugin");
    let snapshot = manager.snapshot();
    assert_eq!(
        snapshot.plugins["idle-plugin"].runtime_state,
        VulcanPluginRuntimeState::Enabled
    );
    assert!(snapshot.plugins["idle-plugin"].diagnostics.is_empty());
    assert!(snapshot.enabled_tools.contains_key("idle_ping"));
    let result = manager
        .invoke(VulcanPluginInvocationRequest {
            tool_id: "idle_ping".into(),
            arguments: json!({}),
            owner_session_id: "requirements-owner".into(),
            workspace_root: root.path().to_owned(),
        })
        .await
        .expect("execute real version-two Lua");
    assert_eq!(result["accepted"], json!(true));
    let status = manager
        .formal_capacity_status("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
        .expect("query exact capacity")
        .expect("configured capacity remains registered");
    assert_eq!(status.snapshot.config.resources.min_resident_vms, 1);
    assert_eq!(
        std::fs::read(&path).expect("read unchanged package bytes"),
        bytes
    );
    close(&manager).await;
}
