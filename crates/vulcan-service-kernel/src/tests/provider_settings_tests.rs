//! Provider settings service helper tests.
//! Provider 设置服务辅助逻辑测试。

use crate::provider_settings::normalize_provider_enabled_model_ids_for_save;
use std::collections::BTreeSet;

/// Builds a stable catalog-id set for enabled-model save tests.
/// 为启用模型保存测试构建稳定的目录标识集合。
///
/// # Parameters
/// 参数
/// `items` contains model identifiers that should be accepted.
/// `items` 包含应被接受的模型标识。
///
/// # Returns
/// 返回值
/// A sorted set matching the service catalog lookup shape.
/// 返回与服务目录查询形态一致的有序集合。
fn catalog_ids(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| item.to_string()).collect()
}

/// Verifies enabled-model saves trim, deduplicate, and ignore blank form noise.
/// 验证启用模型保存会去除空白、去重，并忽略表单空白噪音。
#[test]
fn enabled_model_save_normalizes_valid_ids() {
    let normalized = normalize_provider_enabled_model_ids_for_save(
        "provider-a",
        vec![
            String::from(" model-b "),
            String::from(""),
            String::from("model-a"),
            String::from("model-b"),
            String::from("   "),
        ],
        &catalog_ids(&["model-a", "model-b", "model-c"]),
    )
    .expect("valid model ids should normalize");

    assert_eq!(
        normalized,
        vec![String::from("model-a"), String::from("model-b")]
    );
}

/// Verifies enabled-model saves reject non-empty ids that are missing from the current catalog.
/// 验证启用模型保存会拒绝当前目录中不存在的非空标识。
#[test]
fn enabled_model_save_rejects_unknown_ids() {
    let error = normalize_provider_enabled_model_ids_for_save(
        "provider-a",
        vec![String::from("model-a"), String::from("missing-model")],
        &catalog_ids(&["model-a"]),
    )
    .expect_err("unknown model ids must be rejected");

    assert!(error.contains("provider-a"), "unexpected error: {error}");
    assert!(error.contains("missing-model"), "unexpected error: {error}");
}
