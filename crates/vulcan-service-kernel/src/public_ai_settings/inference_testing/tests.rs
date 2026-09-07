//! Exact persisted-route regressions for settings inference connection tests.
//! 设置推理连接测试的精确持久路由回归。

use super::*;
use vulcan_provider_adapters::{InferenceProfile, InferenceRouteBinding};

/// Check VCE settings tests require the saved supported dimension and leave peers unchanged.
/// 验证 VCE 设置测试要求已保存且受支持的维度，并保持其他消费者不变。
///
/// Takes no parameters and returns nothing; inconsistent test admission fails assertions.
/// 无参数、无返回值；测试准入不一致时断言失败。
#[test]
fn settings_test_dimensions_require_explicit_vce_choice_only() {
    // capabilities declares a bounded discrete model independently from a provider default.
    // capabilities 独立于供应商默认值声明有界离散模型。
    let capabilities = vulcan_ai_contract::EmbeddingCapabilities {
        tasks: BTreeSet::new(),
        input_resources: BTreeSet::new(),
        output_kinds: BTreeSet::new(),
        encodings: BTreeSet::new(),
        dimensions: BTreeSet::from([512, 1024, 2048]),
        minimum_dimensions: None,
        maximum_dimensions: None,
        default_dimensions: Some(2048),
        maximum_inputs: None,
        maximum_tokens_per_input: None,
        normalization: vulcan_ai_contract::EmbeddingNormalization::Unknown,
        distance_metric: vulcan_ai_contract::DistanceMetric::Unknown,
    };
    // configuration intentionally begins with no saved VCE dimension.
    // configuration 故意从尚未保存 VCE 维度的状态开始。
    let mut configuration = PersistedAiConfiguration::default();
    assert!(settings_embedding_dimensions(&configuration, "vce", &capabilities).is_err());
    assert_eq!(
        settings_embedding_dimensions(&configuration, "vmm", &capabilities),
        Ok(None)
    );
    configuration
        .inference_profiles
        .entry(String::from("vce"))
        .or_default()
        .embedding_dimensions = Some(1024);
    assert_eq!(
        settings_embedding_dimensions(&configuration, "vce", &capabilities),
        Ok(Some(1024))
    );
    assert_eq!(
        settings_embedding_dimensions(&configuration, "knowledge-test", &capabilities),
        Ok(None)
    );
    configuration
        .inference_profiles
        .get_mut("vce")
        .expect("saved profile")
        .embedding_dimensions = Some(768);
    assert!(settings_embedding_dimensions(&configuration, "vce", &capabilities).is_err());
}

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
