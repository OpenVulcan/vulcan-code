//! Persisted aggregate policies retain exact version and ownership semantics.
//! 持久聚合策略保留精确版本及归属语义。

use super::*;

/// Load supplied bytes through the public file reader and preserve them even when decoding fails.
/// 通过公开文件读取器加载给定字节，即使解码失败也保留原字节。
/// Return the decoded settings or diagnostic after asserting that no write occurred.
/// 断言未发生写入后，返回已解码设置或诊断。
pub(super) fn read_unchanged(bytes: &str) -> Result<VulcanPluginRuntimeSettings, String> {
    // Keep this independent persisted fixture alive until the post-read assertion completes.
    // 保持独立持久夹具存活至读取后断言完成。
    let root = tempfile::tempdir().expect("settings fixture");
    // Use the sole production configuration path.
    // 使用唯一生产配置路径。
    let path = root.path().join("plugin-runtime-settings.json");
    std::fs::write(&path, bytes).expect("persist exact input");
    // Inspect the public reader, including its validation after version migration.
    // 检查公开读取器，包含版本迁移后的校验。
    let result = load_plugin_runtime_settings(root.path());
    assert_eq!(
        std::fs::read_to_string(path).expect("retained bytes"),
        bytes
    );
    result
}

/// Version one preserves every explicit limit and migrates only its in-memory projection.
/// 第一版保留全部显式上限，并且仅迁移内存投影。
#[test]
fn plugin_budget_version_one_migration_preserves_user_configuration() {
    // Distinct non-default values detect accidental substitution of current compiled defaults.
    // 不同的非默认值用于检测意外替换为当前编译默认值。
    let mut expected = VulcanPluginRuntimeSettings::default();
    expected.parent.max_resident_vms += 1;
    expected.max_capability_call_ms += 7;
    expected.shutdown_timeout_ms += 11;
    // Construct the historical schema by removing the field that did not exist in that version.
    // 通过移除该版本不存在的字段构造历史 Schema。
    let mut original = serde_json::to_value(&expected).expect("settings object");
    original["schemaVersion"] = serde_json::json!(1);
    original
        .as_object_mut()
        .expect("object")
        .remove("pluginBudgets");
    original
        .as_object_mut()
        .expect("object")
        .remove("capacityBudgets");
    // Formatting is deliberately retained byte for byte by the reader.
    // 读取器有意逐字节保留格式。
    let bytes = serde_json::to_string_pretty(&original).expect("original bytes");
    assert_eq!(
        read_unchanged(&bytes).expect("explicit migration"),
        expected
    );
    assert!(expected.plugin_budgets.is_empty());
    assert_eq!(
        serde_json::to_value(expected).expect("current projection")["schemaVersion"],
        SETTINGS_SCHEMA_VERSION
    );
}

/// Declared old and unknown versions cannot smuggle in new policy fields or select guessed decoders.
/// 声明的旧版本及未知版本不能夹带新策略字段，也不能选择猜测解码器。
#[test]
fn plugin_budget_versions_reject_undeclared_fields_and_duplicate_discriminators() {
    // Current bytes include pluginBudgets, which version one explicitly cannot contain.
    // 当前字节包含第一版明确不能含有的 pluginBudgets。
    let mut value = serde_json::to_value(VulcanPluginRuntimeSettings::default()).expect("settings");
    value
        .as_object_mut()
        .expect("settings object")
        .remove("capacityBudgets");
    value["schemaVersion"] = serde_json::json!(1);
    assert!(
        read_unchanged(&value.to_string())
            .expect_err("old schema")
            .contains("pluginBudgets")
    );
    value["schemaVersion"] = serde_json::json!(policy::DOCUMENT_VERSION + 1);
    assert!(
        read_unchanged(&value.to_string())
            .expect_err("future schema")
            .contains("Unsupported")
    );
    // Parsing original bytes must detect repeated fields, even when their values agree.
    // 解析原字节必须检测重复字段，即使值相同。
    let current = serde_json::to_string(&VulcanPluginRuntimeSettings::default()).expect("current");
    // Preserve both identical discriminator fields in raw text.
    // 在原始文本中保留两个相同判别字段。
    let duplicate = format!(
        "{{\"schemaVersion\":{SETTINGS_SCHEMA_VERSION},{}",
        &current[1..]
    );
    assert!(
        read_unchanged(&duplicate)
            .expect_err("duplicate discriminator")
            .contains("duplicate field")
    );
}

/// Missing overrides inherit the validated parent; explicit overrides must supply all seven fields.
/// 缺省覆盖继承已校验父级；显式覆盖必须提供全部七个字段。
#[test]
fn plugin_budget_overrides_are_complete_strict_and_exact() {
    // Freeze the source parent once, then create one whole plugin override.
    // 一次冻结来源父级，然后创建单个完整插件覆盖。
    let mut settings = VulcanPluginRuntimeSettings::default();
    // Select the complete upstream shape before applying smaller explicit limits.
    // 应用较小显式上限前，选择完整上游形状。
    let (mut budget, _) = settings.plugin_budget("sample-plugin");
    budget.max_resident_vms = 2;
    budget.max_running_calls = 1;
    budget.max_queued_calls = 1;
    settings
        .plugin_budgets
        .insert("sample-plugin".into(), budget.clone());
    // Read the actual persistence format, not just the in-memory validation helper.
    // 读取实际持久格式，不仅调用内存校验辅助方法。
    let valid = serde_json::to_value(&settings).expect("serialize budget");
    assert_eq!(
        read_unchanged(&valid.to_string()).expect("valid override"),
        settings
    );
    assert_eq!(
        settings.plugin_budget("sample-plugin"),
        (budget, VulcanPluginBudgetSource::HostOverride)
    );
    assert_eq!(
        settings.plugin_budget("another-plugin").1,
        VulcanPluginBudgetSource::Parent
    );
    for field in ["max_sessions", "max_queued_bytes", "max_operations"] {
        // Partial objects must never be silently merged with parent defaults.
        // 不完整对象绝不能与父级默认值静默合并。
        let mut incomplete = valid.clone();
        incomplete["pluginBudgets"]["sample-plugin"]
            .as_object_mut()
            .expect("budget")
            .remove(field);
        assert!(
            read_unchanged(&incomplete.to_string())
                .expect_err("incomplete override")
                .contains(field)
        );
    }
    // Core-owned nested fields remain strict without a second host-owned schema.
    // 核心自有嵌套字段保持严格，无需第二份宿主 Schema。
    let mut unknown = valid;
    unknown["pluginBudgets"]["sample-plugin"]["fallback"] = serde_json::json!(true);
    assert!(
        read_unchanged(&unknown.to_string())
            .expect_err("unknown budget field")
            .contains("fallback")
    );
}

/// Duplicate owner keys fail before one policy can silently replace another.
/// 重复所有者键在策略被静默替换前失败。
#[test]
fn plugin_budget_duplicate_owner_is_rejected_from_original_bytes() {
    // Obtain one complete upstream policy and retain the duplicate text in its original JSON.
    // 获取单个完整上游策略，并在原始 JSON 中保留重复文本。
    let settings = VulcanPluginRuntimeSettings::default();
    // Serialize the whole upstream policy without a host-owned field copy.
    // 序列化完整上游策略，不创建宿主字段副本。
    let budget = serde_json::to_string(&settings.plugin_budget("same-plugin").0).expect("budget");
    // A raw replacement intentionally retains duplicate object keys for the production decoder.
    // 原始替换有意保留重复对象键，供生产解码器检查。
    let bytes = serde_json::to_string(&settings).expect("settings").replace(
        "\"pluginBudgets\":{}",
        &format!("\"pluginBudgets\":{{\"same-plugin\":{budget},\"same-plugin\":{budget}}}"),
    );
    assert!(
        read_unchanged(&bytes)
            .expect_err("duplicate owner")
            .contains("Duplicate plugin budget owner same-plugin")
    );
}

/// Invalid IDs, zero limits, parent excess and inconsistent combinations are rejected without clamping.
/// 拒绝非法标识、零上限、超出父级及不一致组合，不进行裁剪。
#[test]
fn plugin_budget_invalid_aggregate_never_changes_user_limits() {
    // Each case starts from one complete valid inherited policy.
    // 每个用例从单个完整有效继承策略开始。
    let settings = VulcanPluginRuntimeSettings::default();
    // Preserve the common valid baseline independently of each mutation.
    // 独立于每次修改保留共同有效基线。
    let budget = settings.plugin_budget("valid-plugin").0;
    for case in ["owner", "zero", "parent", "resident", "operations"] {
        // Mutate only the boundary under test.
        // 仅修改待测边界。
        let mut invalid = settings.clone();
        // Isolate this case's invalid plugin policy from the baseline.
        // 将本用例非法插件策略与基线隔离。
        let mut changed = budget.clone();
        match case {
            "zero" => changed.max_queued_bytes = 0,
            "parent" => changed.max_sessions = settings.parent.max_sessions + 1,
            "resident" => changed.max_resident_vms = 1,
            "operations" => changed.max_operations = 1,
            "owner" => {}
            _ => unreachable!("fixed case"),
        }
        invalid.plugin_budgets.insert(
            if case == "owner" {
                "Bad/Owner"
            } else {
                "valid-plugin"
            }
            .into(),
            changed,
        );
        assert!(
            read_unchanged(&serde_json::to_string(&invalid).expect("invalid bytes")).is_err(),
            "{case}"
        );
    }
}
