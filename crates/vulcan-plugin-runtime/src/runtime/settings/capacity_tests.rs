//! Strict purpose policy decoding and non-mutating historical configuration migration.
//! 严格用途策略解码及不修改原文的历史配置迁移。

use super::budget_tests::read_unchanged;
use super::*;
use luaskills::runtime::embedded::PoolKind;
use serde_json::json;

/// Version two keeps explicit plugin overrides while adding only empty in-memory purpose configuration.
/// 第二版保留显式插件覆盖，仅在内存增加空用途配置。
#[test]
fn capacity_settings_migrate_version_two_without_rewriting() {
    // Non-default values expose any substitution by current compiled defaults.
    // 非默认值用于暴露被当前编译默认值替换的情况。
    let mut expected = VulcanPluginRuntimeSettings::default();
    expected.shutdown_timeout_ms += 3;
    // Resolve a complete aggregate once before freezing this explicit historical override.
    // 冻结此显式历史覆盖前，一次解析完整聚合预算。
    let mut plugin = expected.plugin_budget("sample").0;
    plugin.max_running_calls = 1;
    expected.plugin_budgets.insert("sample".into(), plugin);
    // Remove the field that did not exist in version two, preserving all remaining bytes on read.
    // 移除第二版不存在的字段，读取时保留全部其余原字节。
    let mut original = serde_json::to_value(&expected).expect("current settings");
    original["schemaVersion"] = json!(2);
    original
        .as_object_mut()
        .expect("object")
        .remove("automaticPrewarm");
    original
        .as_object_mut()
        .expect("object")
        .remove("capacityBudgets");
    assert_eq!(
        read_unchanged(&serde_json::to_string_pretty(&original).expect("bytes")).expect("migrated"),
        expected
    );
    original["capacityBudgets"] = json!({});
    assert!(
        read_unchanged(&original.to_string())
            .expect_err("new field under historical schema")
            .contains("capacityBudgets")
    );
}

/// Effective capacity inherits the resolved plugin aggregate, while explicit complete overrides retain their source.
/// 有效容量继承已解析插件聚合，显式完整覆盖则保留自身来源。
#[test]
fn capacity_settings_resolve_exact_purpose_and_reject_partial_overrides() {
    // A smaller aggregate proves inheritance does not bypass plugin limits using parent defaults.
    // 较小聚合预算证明继承不会借父级默认值绕过插件限制。
    let mut settings = VulcanPluginRuntimeSettings::default();
    // Use the actual complete upstream shape, reducing only the intended test boundaries.
    // 使用实际完整上游形状，仅缩小预期测试边界。
    let mut plugin = settings.plugin_budget("sample").0;
    plugin.max_resident_vms = 2;
    plugin.max_running_calls = 1;
    plugin.max_queued_calls = 2;
    plugin.max_queued_bytes /= 2;
    settings
        .plugin_budgets
        .insert("sample".into(), plugin.clone());
    // Missing configuration has a declared shared, unreserved meaning.
    // 缺失配置具有已声明的公共、无预留语义。
    let (mut explicit, source) =
        settings.capacity_budget("sample", VulcanPluginCapacityPurpose::Hook);
    assert_eq!(source, VulcanPluginCapacityBudgetSource::PluginBudget);
    assert_eq!(explicit.resources.kind, PoolKind::Shared);
    assert_eq!(explicit.resources.min_resident_vms, 0);
    assert_eq!(explicit.resources.max_resident_vms, plugin.max_resident_vms);
    assert_eq!(
        explicit.resources.max_running_calls,
        plugin.max_running_calls
    );
    assert_eq!(explicit.max_queued_calls, plugin.max_queued_calls);
    assert_eq!(explicit.max_queued_bytes, plugin.max_queued_bytes);
    explicit.resources.kind = PoolKind::Dedicated;
    explicit.resources.min_resident_vms = 1;
    settings.capacity_budgets.insert(
        "sample".into(),
        BTreeMap::from([(VulcanPluginCapacityPurpose::Hook, explicit.clone())]),
    );
    assert_eq!(
        settings.capacity_budget("sample", VulcanPluginCapacityPurpose::Hook),
        (explicit, VulcanPluginCapacityBudgetSource::HostOverride)
    );
    assert_eq!(
        settings
            .capacity_budget("sample", VulcanPluginCapacityPurpose::UiAction)
            .1,
        VulcanPluginCapacityBudgetSource::PluginBudget
    );
    // Exercise the actual file decoder, including native semantic checks and unchanged original bytes.
    // 执行实际文件解码器，包括原生语义检查及原字节保留。
    let valid = serde_json::to_value(&settings).expect("settings");
    assert_eq!(
        read_unchanged(&valid.to_string()).expect("valid settings"),
        settings
    );
    for case in ["missing", "unknown", "null", "purpose", "queue"] {
        // Each mutation isolates one concrete wire or budget conflict.
        // 每次修改隔离一个具体线格式或预算冲突。
        let mut invalid = valid.clone();
        match case {
            "missing" => {
                invalid["capacityBudgets"]["sample"]["hook"]
                    .as_object_mut()
                    .expect("config")
                    .remove("max_queued_bytes");
            }
            "unknown" => invalid["capacityBudgets"]["sample"]["hook"]["fallback"] = json!(true),
            "null" => invalid["capacityBudgets"]["sample"]["hook"] = serde_json::Value::Null,
            "purpose" => invalid["capacityBudgets"]["sample"]["unknown_purpose"] = json!({}),
            "queue" => {
                invalid["capacityBudgets"]["sample"]["hook"]["max_queued_calls"] =
                    json!(plugin.max_queued_calls + 1)
            }
            _ => unreachable!("declared cases"),
        }
        assert!(read_unchanged(&invalid.to_string()).is_err(), "{case}");
    }
}

/// Duplicate plugin and purpose keys cannot erase conflicting original configuration evidence.
/// 重复插件及用途键不能消除冲突的原配置证据。
#[test]
fn capacity_settings_reject_duplicate_owners_and_purposes() {
    // Generate a valid explicit upstream policy without hard-coding its full field shape.
    // 生成有效显式上游策略，不硬编码完整字段形状。
    let settings = VulcanPluginRuntimeSettings::default();
    // Preserve repeated keys in raw JSON instead of constructing a map that would collapse them.
    // 在原始 JSON 中保留重复键，不构造会将其消除的映射。
    let config = serde_json::to_string(
        &settings
            .capacity_budget("sample", VulcanPluginCapacityPurpose::Hook)
            .0,
    )
    .expect("config");
    // Both ownership layers use the same strict duplicate-preserving decoder.
    // 两层归属都使用同一严格保留重复证据的解码器。
    let alternatives = [
        format!("{{\"sample\":{{\"hook\":{config},\"hook\":{config}}}}}"),
        format!("{{\"sample\":{{\"hook\":{config}}},\"sample\":{{\"hook\":{config}}}}}"),
    ];
    for map in alternatives {
        // The production serializer supplies the surrounding settings envelope.
        // 生产序列化器提供外围设置信封。
        let bytes = serde_json::to_string(&settings).expect("settings").replace(
            "\"capacityBudgets\":{}",
            &format!("\"capacityBudgets\":{map}"),
        );
        assert!(
            read_unchanged(&bytes)
                .expect_err("duplicate ownership")
                .contains("Duplicate capacity ownership key")
        );
    }
}

/// Aggregate guarantees and metadata inventories must fit before any native parent is attached.
/// 聚合保证及元数据清单必须在任何原生父级附加前满足上限。
#[test]
fn capacity_settings_reject_combined_guarantee_and_registration_excess() {
    // Every invalid case starts with the same valid upstream-derived configuration.
    // 每个无效用例从相同有效上游派生配置开始。
    let baseline = VulcanPluginRuntimeSettings::default();
    // Full parent-sized guarantees make combined overcommit observable without huge allocations.
    // 与父级等大的保证使聚合超额可观察，无需巨量分配。
    let mut full = baseline
        .capacity_budget("sample", VulcanPluginCapacityPurpose::Hook)
        .0;
    full.resources.kind = PoolKind::Dedicated;
    full.resources.min_resident_vms = baseline.parent.max_resident_vms;
    for case in [
        "plugin_sum",
        "parent_sum",
        "owner_count",
        "capacity_count",
        "parent_capacity_count",
        "owner_id",
    ] {
        // Keep independent bytes and policy maps for each validation boundary.
        // 为每个校验边界保留独立字节及策略映射。
        let mut invalid = baseline.clone();
        invalid.capacity_budgets.insert(
            "sample".into(),
            BTreeMap::from([(VulcanPluginCapacityPurpose::Hook, full.clone())]),
        );
        match case {
            "plugin_sum" => {
                invalid
                    .capacity_budgets
                    .get_mut("sample")
                    .expect("owner")
                    .insert(VulcanPluginCapacityPurpose::UiAction, full.clone());
            }
            "parent_sum" => {
                invalid.capacity_budgets.insert(
                    "another".into(),
                    BTreeMap::from([(VulcanPluginCapacityPurpose::Hook, full.clone())]),
                );
            }
            "owner_count" => {
                invalid.parent.max_registered_plugins = 1;
                invalid.capacity_budgets.insert(
                    "another".into(),
                    BTreeMap::from([(
                        VulcanPluginCapacityPurpose::Hook,
                        baseline
                            .capacity_budget("another", VulcanPluginCapacityPurpose::Hook)
                            .0,
                    )]),
                );
            }
            "capacity_count" => {
                invalid.parent.max_registered_pools = 1;
                invalid
                    .capacity_budgets
                    .get_mut("sample")
                    .expect("owner")
                    .insert(
                        VulcanPluginCapacityPurpose::UiAction,
                        baseline
                            .capacity_budget("sample", VulcanPluginCapacityPurpose::UiAction)
                            .0,
                    );
            }
            "parent_capacity_count" => {
                invalid.parent.max_registered_pools = 1;
                invalid.capacity_budgets.insert(
                    "another".into(),
                    BTreeMap::from([(
                        VulcanPluginCapacityPurpose::Hook,
                        baseline
                            .capacity_budget("another", VulcanPluginCapacityPurpose::Hook)
                            .0,
                    )]),
                );
            }
            "owner_id" => {
                invalid
                    .capacity_budgets
                    .insert("Bad/Owner".into(), BTreeMap::new());
            }
            _ => unreachable!("declared cases"),
        }
        assert!(
            read_unchanged(&serde_json::to_string(&invalid).expect("invalid bytes")).is_err(),
            "{case}"
        );
    }
}
