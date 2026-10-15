//! Synthetic embedding contracts; these tests never dispatch a provider request.
//! 合成嵌入契约；这些测试绝不分派供应商请求。

use super::*;

/// Verifies a registered space can embed only through its exact host-owned profile.
/// 验证已注册空间仅可经其精确宿主自有配置档嵌入。
/// Takes no inputs and returns no value; the mock port performs no external I/O.
/// 无输入及返回值；模拟端口不执行外部 I/O。
#[test]
fn knowledge_embedding_registration_is_profile_scoped_and_revocable() {
    // Reuse the same exact declared physical route and capability contract.
    // 复用相同精确声明真实路由及能力契约。
    let (_, capabilities, request, response) = fixture();
    // A local mock is the sole host boundary for this test.
    // 本地模拟为此测试唯一宿主边界。
    let port = vulcan_ai_contract::test_support::MockHostAiPort::new();
    port.set_capability_snapshot(vulcan_ai_contract::InferenceCapabilitySnapshot {
        contract_version: INFERENCE_CONTRACT_VERSION,
        consumer_profile_id: request.request.consumer_profile_id.clone(),
        operations: vec![vulcan_ai_contract::InferenceOperationCapability {
            operation: AiOperation::EmbeddingCreate,
            purpose_id: None,
            available: true,
            unavailable_reason: None,
            route: Some(response.response.route),
            embedding: Some(capabilities),
            embedding_batch: None,
            rerank: None,
        }],
    })
    .unwrap();
    // Registration carries no LLM/decision authority or alternate profile grant.
    // 注册不携带 LLM、判定权限或其他配置档授权。
    let handle = register_embedding(&port, &request.request.consumer_profile_id).unwrap();
    assert!(
        handle
            .caller
            .authorize(
                &request.request.consumer_profile_id,
                InferencePermission::Embedding
            )
            .is_ok()
    );
    assert!(
        handle
            .caller
            .authorize("foreign-profile", InferencePermission::Embedding)
            .is_err()
    );
    assert_eq!(
        handle.caller.permissions,
        BTreeSet::from([InferencePermission::Status, InferencePermission::Embedding])
    );
    assert_eq!(
        handle.caller.usage_scope,
        InferenceUsageScope::InternalBackground
    );
    port.unregister_capability(&handle.registration_id).unwrap();
    assert!(port.capability_handle(&handle.registration_id).is_err());
}
use vulcan_ai_contract::{
    DistanceMetric, EmbeddingNormalization, EmbeddingResponse, EmbeddingResult,
    EmbeddingSpaceFingerprint, ExecutionRouteSnapshot, RequestUsage,
};

/// Builds exact synthetic settings, capabilities, request and response; takes no inputs and performs no I/O.
/// 构造精确合成设置、能力、请求及响应；无输入且不执行 I/O。
fn fixture() -> (
    KnowledgeEmbeddingSettings,
    EmbeddingCapabilities,
    HostAiEmbeddingRequest,
    HostAiEmbeddingResponse,
) {
    // Discrete dimensions and tasks define the complete supported shape.
    // 离散维度及任务定义完整支持形状。
    let capabilities = EmbeddingCapabilities {
        tasks: BTreeSet::from([EmbeddingTask::Document, EmbeddingTask::Query]),
        input_resources: BTreeSet::new(),
        output_kinds: BTreeSet::from([EmbeddingOutputKind::Dense]),
        encodings: BTreeSet::from([EmbeddingEncoding::Float]),
        dimensions: BTreeSet::from([3]),
        minimum_dimensions: None,
        maximum_dimensions: None,
        default_dimensions: Some(3),
        maximum_inputs: Some(4),
        maximum_tokens_per_input: None,
        normalization: EmbeddingNormalization::None,
        distance_metric: DistanceMetric::Cosine,
    };
    // This fixture binds one account, protocol and immutable execution revision.
    // 此夹具绑定一个账号、协议及不可变执行修订。
    let route = ExecutionRouteSnapshot {
        provider_id: "provider".into(),
        service_entry_id: "service".into(),
        connection_id: "connection".into(),
        model_id: "model".into(),
        protocol_binding_id: Some("protocol".into()),
        action_binding_id: None,
        secondary_route_id: None,
        secondary_provider_name: None,
        secondary_endpoint_name: None,
        capability_revision: "capability".into(),
        catalog_revision: "catalog".into(),
        route_revision: "wire".into(),
        usage_scope: InferenceUsageScope::InternalBackground,
    };
    // Portable route identity is separate from the physical library's authorization profile.
    // 可移植路由身份独立于物理知识库授权配置档。
    let mut embedding = KnowledgeEmbeddingSettings {
        route: KnowledgeRouteReference {
            provider_id: route.provider_id.clone(),
            service_entry_id: route.service_entry_id.clone(),
            connection_id: Some(route.connection_id.clone()),
            model_id: route.model_id.clone(),
            protocol: "protocol".into(),
            binding_kind: KnowledgeRouteBindingKind::Protocol,
            usage_scope: "internal_background".into(),
        },
        dimensions: 3,
        batch_size: 4,
        metric: KnowledgeVectorMetric::Cosine,
        model_fingerprint: String::new(),
    };
    embedding.model_fingerprint =
        settings::embedding_fingerprint(&route, 3, &capabilities).unwrap();
    // Two inputs let response ordering and coverage be validated independently of their content.
    // 两项输入使响应顺序及覆盖验证独立于其内容。
    let request = HostAiEmbeddingRequest {
        vector_space_id: knowledge_vector_space_id(&embedding).unwrap(),
        request: EmbeddingRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: "request".into(),
            idempotency_key: None,
            consumer_profile_id: "profile".into(),
            inputs: vec![
                EmbeddingInput {
                    input_id: "a".into(),
                    content: EmbeddingInputContent::Text { text: "one".into() },
                },
                EmbeddingInput {
                    input_id: "b".into(),
                    content: EmbeddingInputContent::Text { text: "two".into() },
                },
            ],
            task: EmbeddingTask::Document,
            dimensions: Some(3),
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            invalid_input_policy: EmbeddingInvalidInputPolicy::FailBatch,
        },
    };
    // Reversed results are legal when their original indices and identifiers remain exact.
    // 只要原始下标及身份保持精确，反向结果合法。
    let response = HostAiEmbeddingResponse {
        vector_space_id: request.vector_space_id.clone(),
        response: EmbeddingResponse {
            request_id: "request".into(),
            fingerprint: EmbeddingSpaceFingerprint {
                provider_id: route.provider_id.clone(),
                service_entry_id: route.service_entry_id.clone(),
                deployment_id: None,
                secondary_route_id: None,
                model_id: route.model_id.clone(),
                binding_id: "protocol".into(),
                capability_revision: route.capability_revision.clone(),
                task: EmbeddingTask::Document,
                dimensions: 3,
                output_kind: EmbeddingOutputKind::Dense,
                encoding: EmbeddingEncoding::Float,
                normalization: capabilities.normalization,
                distance_metric: capabilities.distance_metric,
            },
            route,
            results: vec![
                EmbeddingResult {
                    input_id: "b".into(),
                    input_index: 1,
                    embedding: EmbeddingPayload::Dense {
                        values: vec![0.0, 1.0, 0.0],
                    },
                },
                EmbeddingResult {
                    input_id: "a".into(),
                    input_index: 0,
                    embedding: EmbeddingPayload::Dense {
                        values: vec![1.0, 0.0, 0.0],
                    },
                },
            ],
            dropped: vec![],
            usage: RequestUsage::default(),
            duration_ms: 1,
        },
    };
    (embedding, capabilities, request, response)
}

/// Verifies exact coverage and provider reordering; no parameters or return value.
/// 验证精确覆盖及供应商重排；无参数及返回值。
#[test]
fn knowledge_embedding_response_reorders_only_verified_identities() {
    // The fixture contains reversed physical results.
    // 夹具包含反向真实结果。
    let (embedding, capabilities, request, response) = fixture();
    assert_eq!(
        validate_response(&request, response.clone(), &embedding, &capabilities).unwrap(),
        vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]]
    );
    // Missing, repeated and foreign indices are separate coverage violations.
    // 缺失、重复及外来下标分别违反覆盖约束。
    let mut missing = response.clone();
    missing.response.results.pop();
    assert!(validate_response(&request, missing, &embedding, &capabilities).is_err());
    // Repetition cannot replace another input even with the expected result count.
    // 即使结果数符合预期，重复也不能替换其他输入。
    let mut repeated = response.clone();
    repeated.response.results = vec![response.response.results[0].clone(); 2];
    assert!(validate_response(&request, repeated, &embedding, &capabilities).is_err());
    // A numerically valid vector still belongs only to its exact input identity.
    // 数值合法的向量仍仅属于其精确输入身份。
    let mut foreign = response;
    foreign.response.results[0].input_id = "foreign".into();
    assert!(validate_response(&request, foreign, &embedding, &capabilities).is_err());
}

/// Rejects changed physical/semantic identity and invalid vector values; no parameters or return value.
/// 拒绝变化的真实及语义身份与非法向量值；无参数及返回值。
#[test]
fn knowledge_embedding_response_rejects_route_space_metric_and_value_drift() {
    // Every mutation starts from the same valid response.
    // 每个变更均从相同合法响应开始。
    let (embedding, capabilities, request, response) = fixture();
    for mutate in [
        |value: &mut HostAiEmbeddingResponse| value.vector_space_id.push('x'),
        |value: &mut HostAiEmbeddingResponse| value.response.route.connection_id.push('x'),
        |value: &mut HostAiEmbeddingResponse| value.response.route.route_revision.push('x'),
        |value: &mut HostAiEmbeddingResponse| {
            value.response.fingerprint.distance_metric = DistanceMetric::Euclidean
        },
        |value: &mut HostAiEmbeddingResponse| {
            value.response.fingerprint.capability_revision.push('x')
        },
        |value: &mut HostAiEmbeddingResponse| {
            value.response.results[0].embedding = EmbeddingPayload::Dense {
                values: vec![f32::NAN; 3],
            }
        },
        |value: &mut HostAiEmbeddingResponse| {
            value.response.results[0].embedding = EmbeddingPayload::Dense {
                values: vec![1.0; 2],
            }
        },
    ] {
        // Reject each independent mismatch rather than accepting an unrelated successful field.
        // 拒绝每项独立错配，不因无关字段成功而接受。
        let mut changed = response.clone();
        mutate(&mut changed);
        assert!(validate_response(&request, changed, &embedding, &capabilities).is_err());
    }
}

/// Checks admission and fingerprint evolution from declared capabilities; no I/O or return value.
/// 从声明能力检查准入及指纹演进；无 I/O 及返回值。
#[test]
fn knowledge_embedding_contract_is_explicit_and_catalog_refresh_stable() {
    // Only the execution identity contributes to compatibility.
    // 只有执行身份参与兼容判断。
    let (embedding, mut capabilities, request, response) = fixture();
    assert!(settings::validate_embedding_shape(&capabilities, 3, 4).is_ok());
    assert!(settings::validate_embedding_shape(&capabilities, 3, 5).is_err());
    assert!(settings::validate_embedding_shape(&capabilities, 2, 4).is_err());
    assert_eq!(
        settings::embedding_task(&capabilities, true).unwrap(),
        EmbeddingTask::Query
    );
    capabilities.tasks = BTreeSet::from([EmbeddingTask::ProviderDefault]);
    assert_eq!(
        settings::embedding_task(&capabilities, false).unwrap(),
        EmbeddingTask::ProviderDefault
    );
    capabilities.tasks.clear();
    assert!(settings::embedding_task(&capabilities, false).is_err());
    capabilities.distance_metric = DistanceMetric::Unknown;
    assert!(settings::embedding_metric(&capabilities).is_err());
    // A catalog-only generation change preserves the exact physical embedding contract.
    // 仅目录代次变化保留精确真实嵌入契约。
    let (_, capabilities, _, _) = fixture();
    // Presentation and catalog metadata are unrelated to produced vector compatibility.
    // 展示及目录元数据与生成向量兼容性无关。
    let mut refreshed = response;
    refreshed.response.route.catalog_revision.push('x');
    refreshed.response.route.secondary_provider_name = Some("renamed".into());
    assert!(validate_response(&request, refreshed, &embedding, &capabilities).is_ok());
}
