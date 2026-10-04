//! Version migration and explicit automatic targets keep persisted bytes and strict ownership intact.
//! 版本迁移与明确自动目标保持持久原字节及严格归属不变。

use super::budget_tests::read_unchanged;
use super::*;
use serde_json::{Value, json};

/// Return a complete historical third-version shape derived from current authoritative defaults.
/// 从当前权威默认值派生完整历史第三版形状。
fn version_three(settings: &VulcanPluginRuntimeSettings) -> Value {
    let mut legacy = serde_json::to_value(settings).expect("settings");
    legacy["schemaVersion"] = json!(3);
    legacy
        .as_object_mut()
        .expect("settings object")
        .remove("automaticPrewarm");
    legacy
}

/// Old settings and nested old containers migrate only in memory and preserve original capacity receipts.
/// 旧设置及嵌套旧容器仅在内存迁移，并保留原容量回执。
#[test]
fn automatic_prewarm_migration_preserves_legacy_documents_without_enabling_rules() {
    let mut expected = VulcanPluginRuntimeSettings::default();
    expected.shutdown_timeout_ms += 13;
    let legacy = version_three(&expected);
    assert_eq!(
        read_unchanged(&legacy.to_string()).expect("version three"),
        expected
    );
    let request = VulcanPluginCapacityPolicyUpdate {
        update_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: "old-runtime".into(),
        capacity_id: "old-capacity".into(),
        expected_revision: "1".into(),
        plugin_id: "sample".into(),
        purpose: VulcanPluginCapacityPurpose::ToolCall,
        config: None,
    };
    let record = VulcanPluginCapacityPolicyRecord {
        request,
        phase: VulcanPluginCapacityPolicyPhase::Prepared,
        applied_revision: None,
        error: None,
        persistence_error: None,
    };
    let original = json!({"schemaVersion":4,"settings":legacy,"update":record});
    let bytes = serde_json::to_string_pretty(&original).expect("original bytes");
    assert_eq!(read_unchanged(&bytes).expect("legacy container"), expected);
    let decoded = version::decode_container(&bytes).expect("migrated receipt");
    assert_eq!(decoded.update, Some(record));
    assert!(decoded.prewarm_update.is_none());
    assert!(decoded.settings.automatic_prewarm.is_empty());
    // Direct nested decoding must reject duplicate discriminators and fields in their original byte order.
    // 直接嵌套解码必须按原字节顺序拒绝重复判别字段及字段。
    let compact = original.to_string();
    for invalid in [
        compact.replacen("\"settings\":{", "\"settings\":{\"schemaVersion\":3,", 1),
        compact.replacen(
            "\"settings\":{",
            "\"settings\":{\"automaticPrewarm\":[],",
            1,
        ),
        compact.replacen(
            "\"shutdownTimeoutMs\":",
            "\"shutdownTimeoutMs\":1,\"shutdownTimeoutMs\":",
            1,
        ),
        compact.replacen("\"update\":", "\"update\":null,\"update\":", 1),
        compact.replacen("\"schemaVersion\":3", "\"schemaVersion\":5", 1),
    ] {
        assert_ne!(
            invalid, compact,
            "mutation must select the real legacy field"
        );
        assert!(
            read_unchanged(&invalid).is_err(),
            "legacy shape must stay strict"
        );
    }
}

/// Rules are explicit, unique and bounded by the existing actual tool-call capacity resolver.
/// 规则明确、唯一，并由现有真实工具调用容量解析器约束。
#[test]
fn automatic_prewarm_rules_validate_exact_owners_and_existing_limits() {
    let mut settings = VulcanPluginRuntimeSettings::default();
    let rule = VulcanPluginAutomaticPrewarmRule {
        plugin_id: "sample".into(),
        tool_id: "sample_read".into(),
        target_idle: 1,
    };
    settings.automatic_prewarm.push(rule.clone());
    let valid = serde_json::to_value(&settings).expect("settings");
    assert_eq!(
        read_unchanged(&valid.to_string()).expect("explicit rule"),
        settings
    );
    for rules in [
        json!(null),
        json!([rule, rule]),
        json!([{"pluginId":"sample","toolId":"sample_read","targetIdle":0}]),
        json!([{"pluginId":"sample","toolId":"sample_read","targetIdle":settings.parent.max_resident_vms + 1}]),
        json!([{"pluginId":"sample","toolId":"sample_read","targetIdle":1,"fallback":true}]),
        json!([{"pluginId":"sample","toolId":"../read","targetIdle":1}]),
        json!([{"pluginId":"sample","toolId":"sample_read"}]),
    ] {
        let mut invalid = valid.clone();
        invalid["automaticPrewarm"] = rules;
        assert!(
            read_unchanged(&invalid.to_string()).is_err(),
            "invalid explicit rule accepted"
        );
    }
    let mut capacity = settings
        .capacity_budget("sample", VulcanPluginCapacityPurpose::ToolCall)
        .0;
    capacity.resources.max_resident_vms = 1;
    capacity.resources.max_running_calls = 1;
    settings.capacity_budgets.insert(
        "sample".into(),
        BTreeMap::from([(VulcanPluginCapacityPurpose::ToolCall, capacity)]),
    );
    settings
        .automatic_prewarm
        .iter_mut()
        .find(|rule| rule.tool_id == "sample_read")
        .expect("configured sample rule must remain present")
        .target_idle = 2;
    assert!(
        settings
            .validate()
            .expect_err("purpose ceiling")
            .contains("tool-call capacity")
    );
    // Duplicate field evidence must not disappear through any map-based compatibility conversion.
    // 重复字段证据不能通过任何基于映射的兼容转换消失。
    let duplicate =
        valid
            .to_string()
            .replacen("\"targetIdle\":1", "\"targetIdle\":1,\"targetIdle\":1", 1);
    assert!(
        read_unchanged(&duplicate)
            .expect_err("duplicate field")
            .contains("duplicate field")
    );
}
