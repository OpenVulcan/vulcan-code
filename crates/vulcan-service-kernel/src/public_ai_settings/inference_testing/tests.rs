//! Exact persisted-route regressions for settings inference connection tests.
//! 设置推理连接测试的精确持久路由回归。

use super::*;
use vulcan_provider_adapters::{InferenceProfile, InferenceRouteBinding};

/// Builds one exact internal inference route for settings-boundary regression tests.
/// 为设置边界回归测试构建一条精确内部推理路由。
///
/// # Parameters
/// 参数
/// `operation` and `usage_scope` define the exact persisted route facts.
/// `operation` 与 `usage_scope` 定义精确持久化路由事实。
///
/// # Returns
/// 返回值
/// A deterministic route with a native action binding.
/// 返回带原生动作绑定的确定性路由。
fn route_fixture(
    operation: AiOperation,
    usage_scope: InferenceUsageScope,
) -> InferenceRouteConfiguration {
    InferenceRouteConfiguration {
        provider_id: String::from("provider"),
        service_entry_id: String::from("entry"),
        connection_id: String::from("connection"),
        model_id: String::from("model"),
        binding: InferenceRouteBinding::Action {
            action_binding_id: String::from("action"),
        },
        operation,
        usage_scope,
    }
}

/// Verifies a settings test retains the persisted route usage scope exactly.
/// 验证设置测试会精确保留持久化路由用途范围。
#[test]
fn settings_test_route_retains_exact_usage_scope() {
    let expected = route_fixture(
        AiOperation::EmbeddingCreate,
        InferenceUsageScope::InternalBackground,
    );
    let mut configuration = PersistedAiConfiguration::default();
    configuration.inference_profiles.insert(
        String::from("vmm"),
        InferenceProfile {
            embedding: Some(expected.clone()),
            ..InferenceProfile::default()
        },
    );

    let actual = configured_settings_inference_test_route(
        &configuration,
        "vmm",
        AiOperation::EmbeddingCreate,
    )
    .expect("the exact embedding route should resolve");

    assert_eq!(actual, expected);
}

/// Verifies a route stored in the wrong operation slot is rejected explicitly.
/// 验证存入错误操作槽位的路由会被显式拒绝。
#[test]
fn settings_test_route_rejects_operation_slot_mismatch() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.inference_profiles.insert(
        String::from("vmm"),
        InferenceProfile {
            embedding: Some(route_fixture(
                AiOperation::RerankDocuments,
                InferenceUsageScope::ExternalService,
            )),
            ..InferenceProfile::default()
        },
    );

    let error = configured_settings_inference_test_route(
        &configuration,
        "vmm",
        AiOperation::EmbeddingCreate,
    )
    .expect_err("a mismatched operation slot must fail");

    assert!(error.contains("stores operation `rerank_documents`"));
}
