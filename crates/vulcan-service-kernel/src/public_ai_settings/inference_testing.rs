//! Explicit provider-backed inference connection tests and result validation.
//! 显式供应商推理连接测试及结果校验。

use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use vulcan_ai_contract::{
    AiOperation, DecisionCriterion, DecisionQuestion, DecisionQuestionKind, DecisionRequest,
    EmbeddingEncoding, EmbeddingInput, EmbeddingInputContent, EmbeddingOutputKind,
    EmbeddingPayload, EmbeddingRequest, EmbeddingTask, INFERENCE_CONTRACT_VERSION,
    InferenceCallerBudget, InferenceCallerContext, InferencePermission, InferenceUsageScope,
    RerankRequest, RerankScoreSemantics, RerankText, RerankTruncation,
};
use vulcan_provider_adapters::{InferenceRouteConfiguration, PersistedAiConfiguration};
use vulcan_service_api::WebuiServiceRpcError;

use super::{public_ai_runtime_and_configuration, validate_non_empty};
use crate::SharedServiceBridgeState;

/// Requests one real embedding, rerank or decision connection test through a consumer profile.
/// 请求通过一个消费者配置档执行真实向量、重排或判定连接测试。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiInferenceTestRequest {
    /// Stable consumer profile whose persisted route is tested.
    /// 接受测试的持久化路由所属稳定消费者配置档。
    consumer_profile_id: String,
    /// Exact non-LLM operation to execute.
    /// 要执行的精确非 LLM 操作。
    operation: AiOperation,
}

/// Returns compact validated facts from one real internal inference test.
/// 返回一次真实内部推理测试的紧凑已校验事实。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiInferenceTestResult {
    /// Exact operation tested.
    /// 接受测试的精确操作。
    operation: AiOperation,
    /// Frozen provider identifier returned by execution.
    /// 执行返回的冻结供应商标识。
    provider_id: String,
    /// Frozen service-entry identifier returned by execution.
    /// 执行返回的冻结服务入口标识。
    service_entry_id: String,
    /// Frozen credential connection identifier returned by execution.
    /// 执行返回的冻结凭据连接标识。
    connection_id: String,
    /// Frozen model identifier returned by execution.
    /// 执行返回的冻结模型标识。
    model_id: String,
    /// Number of validated result items.
    /// 已校验结果项数量。
    result_count: usize,
    /// Optional validated embedding dimensions.
    /// 可选的已校验向量维度。
    dimensions: Option<u32>,
    /// End-to-end provider execution time in milliseconds.
    /// 供应商端到端执行耗时毫秒数。
    duration_ms: u64,
}

/// Executes one real provider-backed embedding or rerank settings test.
/// 执行一次由真实供应商支撑的向量或重排设置测试。
///
/// # Parameters
/// 参数
/// `state` owns the configured inference service and `request` selects only an authorized
/// consumer profile operation; it cannot inject a provider route.
/// `state` 持有已配置推理服务，`request` 只能选择获授权消费者配置档操作，不能注入供应商路由。
///
/// # Returns
/// 返回值
/// Compact frozen route facts after request and response invariants are independently verified.
/// 请求与响应不变量经独立校验后返回紧凑冻结路由事实。
pub(crate) async fn service_test_public_ai_inference_route(
    state: SharedServiceBridgeState,
    request: PublicAiInferenceTestRequest,
) -> Result<PublicAiInferenceTestResult, WebuiServiceRpcError> {
    validate_non_empty("consumerProfileId", request.consumer_profile_id.as_str())?;
    let permission = match request.operation {
        AiOperation::EmbeddingCreate => InferencePermission::Embedding,
        AiOperation::RerankDocuments => InferencePermission::Rerank,
        AiOperation::DecisionEvaluate => InferencePermission::Decision,
        _ => {
            return Err(format!(
                "Operation `{}` does not have a settings connection test.",
                request.operation.as_str()
            )
            .into());
        }
    };
    let (_, configuration) = public_ai_runtime_and_configuration(&state)?;
    let configured_route = configured_settings_inference_test_route(
        &configuration,
        request.consumer_profile_id.as_str(),
        request.operation,
    )?;
    let service = configured_inference_service_for_settings(&state)?;
    let caller = settings_inference_test_caller(
        request.consumer_profile_id.as_str(),
        permission,
        configured_route.usage_scope,
    )?;
    let capabilities = service
        .capability_snapshot(&caller, request.consumer_profile_id.as_str())
        .map_err(|error| error.to_string())?;
    let operation = capabilities
        .operations
        .iter()
        .find(|candidate| {
            candidate.operation == request.operation && candidate.purpose_id.is_none()
        })
        .ok_or_else(|| {
            format!(
                "Consumer profile `{}` does not expose operation `{}`.",
                request.consumer_profile_id,
                request.operation.as_str()
            )
        })?;
    if !operation.available {
        return Err(operation
            .unavailable_reason
            .clone()
            .unwrap_or_else(|| {
                format!(
                    "Consumer profile `{}` operation `{}` is unavailable.",
                    request.consumer_profile_id,
                    request.operation.as_str()
                )
            })
            .into());
    }
    match request.operation {
        AiOperation::EmbeddingCreate => {
            let embedding = operation.embedding.as_ref().ok_or_else(|| {
                String::from("The configured route does not declare embedding capabilities.")
            })?;
            // dimensions freezes the same consumer choice used by indexing and retrieval.
            // dimensions 冻结与建库、检索相同的消费者维度选择。
            let dimensions = settings_embedding_dimensions(
                &configuration,
                &request.consumer_profile_id,
                embedding,
            )?;
            let task = settings_embedding_task(&request.consumer_profile_id, embedding)?;
            let request_id = format!("settings_embedding_test_{}", uuid::Uuid::new_v4().simple());
            let response = service
                .create_embeddings(
                    &caller,
                    EmbeddingRequest {
                        contract_version: INFERENCE_CONTRACT_VERSION,
                        request_id,
                        idempotency_key: None,
                        consumer_profile_id: request.consumer_profile_id,
                        inputs: vec![EmbeddingInput {
                            input_id: String::from("settings-test-input"),
                            content: EmbeddingInputContent::Text {
                                text: String::from(
                                    "Vulcan Code internal embedding connection test.",
                                ),
                            },
                        }],
                        task,
                        dimensions,
                        output_kind: preferred_embedding_output_kind(embedding)?,
                        encoding: preferred_embedding_encoding(embedding)?,
                        invalid_input_policy: Default::default(),
                    },
                )
                .await
                .map_err(|error| error.to_string())?;
            validate_embedding_test_response(&response)?;
            Ok(PublicAiInferenceTestResult {
                operation: AiOperation::EmbeddingCreate,
                provider_id: response.route.provider_id,
                service_entry_id: response.route.service_entry_id,
                connection_id: response.route.connection_id,
                model_id: response.route.model_id,
                result_count: response.results.len(),
                dimensions: Some(response.fingerprint.dimensions),
                duration_ms: response.duration_ms,
            })
        }
        AiOperation::RerankDocuments => {
            let rerank = operation.rerank.as_ref().ok_or_else(|| {
                String::from("The configured route does not declare rerank capabilities.")
            })?;
            let overrides = configuration.internal_model_overrides.get(
                PersistedAiConfiguration::internal_model_operation_key(
                    configured_route.provider_id.as_str(),
                    configured_route.model_id.as_str(),
                    AiOperation::RerankDocuments,
                )
                .as_str(),
            );
            let fixture_minimum = rerank.maximum_candidates.unwrap_or(2).clamp(1, 2);
            let requested_top_n = overrides
                .and_then(|value| value.rerank_top_n)
                .unwrap_or(fixture_minimum);
            let candidate_count = requested_top_n.max(fixture_minimum);
            if rerank
                .maximum_candidates
                .is_some_and(|maximum| candidate_count > maximum)
            {
                return Err(format!(
                    "Configured rerank test requires {candidate_count} candidates but the model maximum is {}.",
                    rerank.maximum_candidates.unwrap_or_default()
                )
                .into());
            }
            let candidates = fixed_rerank_test_candidates(candidate_count);
            let request_id = format!("settings_rerank_test_{}", uuid::Uuid::new_v4().simple());
            let response = service
                .rerank_documents(
                    &caller,
                    RerankRequest {
                        contract_version: INFERENCE_CONTRACT_VERSION,
                        request_id,
                        idempotency_key: None,
                        consumer_profile_id: request.consumer_profile_id,
                        query: RerankText {
                            id: String::from("settings-test-query"),
                            text: String::from("Which document describes a programming language?"),
                        },
                        candidates,
                        top_n: requested_top_n,
                        truncation: overrides
                            .and_then(|value| value.rerank_truncation)
                            .unwrap_or(preferred_rerank_truncation(rerank)?),
                        return_content: overrides
                            .and_then(|value| value.rerank_return_content)
                            .unwrap_or(false),
                    },
                )
                .await
                .map_err(|error| error.to_string())?;
            validate_rerank_test_response(&response, candidate_count)?;
            Ok(PublicAiInferenceTestResult {
                operation: AiOperation::RerankDocuments,
                provider_id: response.route.provider_id,
                service_entry_id: response.route.service_entry_id,
                connection_id: response.route.connection_id,
                model_id: response.route.model_id,
                result_count: response.results.len(),
                dimensions: None,
                duration_ms: response.duration_ms,
            })
        }
        AiOperation::DecisionEvaluate => {
            let request_id = format!("settings_decision_test_{}", uuid::Uuid::new_v4().simple());
            // The response is checked against the declared options by the inference facade.
            // 推理门面按已声明选项校验响应。
            let response = service
                .evaluate_decision(
                    &caller,
                    DecisionRequest {
                        contract_version: INFERENCE_CONTRACT_VERSION,
                        request_id,
                        idempotency_key: None,
                        consumer_profile_id: request.consumer_profile_id,
                        state: serde_json::json!({ "message": "connection test" }),
                        questions: vec![DecisionQuestion {
                            id: String::from("settings_test"),
                            kind: DecisionQuestionKind::Choice {
                                instructions: String::from(
                                    "Answer whether the message is a connection test.",
                                ),
                                criteria: vec![
                                    DecisionCriterion {
                                        option: String::from("yes"),
                                        criterion: String::from(
                                            "The message is a connection test.",
                                        ),
                                    },
                                    DecisionCriterion {
                                        option: String::from("no"),
                                        criterion: String::from("The message is anything else."),
                                    },
                                ],
                            },
                        }],
                    },
                )
                .await
                .map_err(|error| error.to_string())?;
            Ok(PublicAiInferenceTestResult {
                operation: AiOperation::DecisionEvaluate,
                provider_id: response.route.provider_id,
                service_entry_id: response.route.service_entry_id,
                connection_id: response.route.connection_id,
                model_id: response.route.model_id,
                result_count: response.answers.len(),
                dimensions: None,
                duration_ms: response.duration_ms,
            })
        }
        _ => Err(format!(
            "Operation `{}` does not have a settings connection test.",
            request.operation.as_str()
        )
        .into()),
    }
}

/// Validate the saved dimension before a settings test can issue a provider request.
/// 在设置测试发起供应商请求前校验已保存维度。
///
/// `configuration` owns the profiles; `consumer_profile_id` identifies the exact consumer;
/// `capabilities` is its resolved model declaration. Returns its saved dimension or an
/// error for an incomplete VCE choice; other consumers retain optional default semantics.
/// `configuration` 持有配置档；`consumer_profile_id` 标识精确消费者；
/// `capabilities` 是解析后的模型声明。返回保存维度，VCE 选择不完整时返回错误；
/// 其他消费者保留可选默认语义。
fn settings_embedding_dimensions(
    configuration: &PersistedAiConfiguration,
    consumer_profile_id: &str,
    capabilities: &vulcan_ai_contract::EmbeddingCapabilities,
) -> Result<Option<u32>, String> {
    // dimensions belongs exclusively to the selected profile, never a peer consumer.
    // dimensions 仅属于选中的配置档，绝不借用其他消费者。
    let dimensions = configuration
        .inference_profiles
        .get(consumer_profile_id)
        .and_then(|profile| profile.embedding_dimensions);
    if consumer_profile_id == crate::vce::VCE_CONSUMER_PROFILE_ID {
        // dimension must be explicitly saved before testing VCE or admitting its engine.
        // dimension 必须在测试 VCE 或准入引擎前由用户明确保存。
        let dimension = dimensions.ok_or_else(|| {
            String::from(
                "Confirm and save the vector dimension in the semantic engine settings first.",
            )
        })?;
        crate::vce::validate_explicit_embedding_dimension(capabilities, dimension)?;
    }
    Ok(dimensions)
}

/// Returns the configured inference service without retaining the shared state lock.
/// 返回已配置推理服务且不保留共享状态锁。
///
/// # Parameters
/// 参数
/// `state` is the shared service bridge whose runtime owns the live configured-inference facade.
/// `state` 是共享服务桥，其运行时持有实时已配置推理门面。
///
/// # Returns
/// 返回值
/// A cloned service handle, or an explicit startup/lock diagnostic without keeping the state lock.
/// 返回克隆的服务句柄，或明确的启动/锁诊断，且不会继续持有状态锁。
pub(super) fn configured_inference_service_for_settings(
    state: &SharedServiceBridgeState,
) -> Result<Arc<crate::configured_inference::ConfiguredInferenceService>, String> {
    state
        .lock()
        .map_err(|error| format!("Failed to lock service state for inference test: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|runtime| Arc::clone(&runtime.configured_inference_service))
        .ok_or_else(|| String::from("Configured inference runtime is still starting."))
}

/// Builds the host-owned identity used only by explicit settings connection tests.
/// 构建仅供显式设置连接测试使用的宿主持有身份。
///
/// # Parameters
/// 参数
/// `consumer_profile_id` and `permission` select the exact test operation while
/// `usage_scope` is copied from the persisted route rather than accepted from browser input.
/// `consumer_profile_id` 与 `permission` 选择精确测试操作，`usage_scope`
/// 从持久化路由复制而非接受浏览器输入。
///
/// # Returns
/// 返回值
/// A short-lived caller context constrained to one profile, one operation, and one route scope.
/// 返回仅限一个配置档、一项操作与一个路由用途范围的短期调用方上下文。
fn settings_inference_test_caller(
    consumer_profile_id: &str,
    permission: InferencePermission,
    usage_scope: InferenceUsageScope,
) -> Result<InferenceCallerContext, String> {
    let now_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("System clock cannot issue an inference test deadline: {error}"))
        .and_then(|duration| {
            u64::try_from(duration.as_millis())
                .map_err(|_| String::from("System time exceeds the supported millisecond range."))
        })?;
    Ok(InferenceCallerContext {
        caller_id: String::from("vulcan-code-settings-inference-test"),
        // Settings tests retain their declared usage-scope scheduling.
        // 设置测试保留已声明用途范围的调度。
        scheduling_class: vulcan_ai_contract::InferenceSchedulingClass::ByUsageScope,
        allowed_consumer_profile_ids: BTreeSet::from([consumer_profile_id.to_string()]),
        permissions: BTreeSet::from([InferencePermission::Status, permission]),
        usage_scope,
        interactive_session_id: None,
        deadline_unix_ms: Some(now_unix_ms.saturating_add(120_000)),
        budget: InferenceCallerBudget {
            max_concurrent_executions: 1,
            max_input_units_per_request: Some(16_384),
            max_output_tokens_per_request: None,
            max_requests_per_minute: None,
            max_input_units_per_minute: None,
            max_execution_duration_ms: Some(120_000),
        },
    })
}

/// Resolves the exact host-owned route used by one settings inference test.
/// 解析一次设置推理测试使用的精确宿主持有路由。
///
/// # Parameters
/// 参数
/// `configuration` owns consumer profiles, `consumer_profile_id` selects one profile,
/// and `operation` must be embedding or rerank.
/// `configuration` 持有消费者配置档，`consumer_profile_id` 选择一个配置档，
/// `operation` 必须为向量或重排。
///
/// # Returns
/// 返回值
/// A cloned exact route whose operation matches its profile slot, or a controlled diagnostic.
/// 返回操作与配置槽位一致的精确路由副本，或受控诊断。
fn configured_settings_inference_test_route(
    configuration: &PersistedAiConfiguration,
    consumer_profile_id: &str,
    operation: AiOperation,
) -> Result<InferenceRouteConfiguration, String> {
    let profile = configuration
        .inference_profiles
        .get(consumer_profile_id)
        .ok_or_else(|| format!("Consumer profile `{consumer_profile_id}` is not configured."))?;
    let route = match operation {
        AiOperation::EmbeddingCreate => profile.embedding.as_ref(),
        AiOperation::RerankDocuments => profile.rerank.as_ref(),
        AiOperation::DecisionEvaluate => profile.decision.as_ref(),
        _ => None,
    }
    .ok_or_else(|| {
        format!(
            "Consumer profile `{consumer_profile_id}` does not configure operation `{}`.",
            operation.as_str()
        )
    })?;
    if route.operation != operation {
        return Err(format!(
            "Consumer profile `{consumer_profile_id}` stores operation `{}` in the `{}` test slot.",
            route.operation.as_str(),
            operation.as_str()
        ));
    }
    Ok(route.clone())
}

/// Selects the embedding task the tested consumer itself sends, so a passing test means the
/// consumer can use the route.
/// 选择被测消费者自身会发送的向量任务，使测试通过即代表该消费者可以使用此路由。
///
/// # Parameters
/// 参数
/// `consumer_profile_id` names the consumer and `capabilities` is its route contract.
/// `consumer_profile_id` 指明消费者，`capabilities` 是其路由契约。
///
/// # Returns
/// 返回值
/// Native VMM's stored-text task after its model-level route check, otherwise the generic test task.
/// 原生 VMM 经模型层路由校验后的存储文本任务；其他消费者返回通用测试任务。
fn settings_embedding_task(
    consumer_profile_id: &str,
    capabilities: &vulcan_ai_contract::EmbeddingCapabilities,
) -> Result<EmbeddingTask, WebuiServiceRpcError> {
    if consumer_profile_id == crate::configured_inference::VMM_CONSUMER_PROFILE_ID {
        return crate::native_vmm::native_vmm_embedding_shape(capabilities)
            .map(|tasks| tasks.document)
            .map_err(|issue| issue.localized().into());
    }
    Ok(preferred_embedding_task(capabilities)?)
}

/// Selects a deterministic declared embedding task for the fixed test request.
/// 为固定测试请求选择一项确定且已声明的向量任务。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact configured route contract returned by the inference facade.
/// `capabilities` 是推理门面返回的精确已配置路由契约。
///
/// # Returns
/// 返回值
/// `ProviderDefault` when declared, otherwise the first ordered declared task, or an error.
/// 已声明时返回 `ProviderDefault`，否则返回有序声明中的第一项任务，若为空则报错。
fn preferred_embedding_task(
    capabilities: &vulcan_ai_contract::EmbeddingCapabilities,
) -> Result<EmbeddingTask, String> {
    if capabilities.tasks.contains(&EmbeddingTask::ProviderDefault) {
        return Ok(EmbeddingTask::ProviderDefault);
    }
    capabilities
        .tasks
        .iter()
        .next()
        .copied()
        .ok_or_else(|| String::from("The embedding route does not declare any supported task."))
}

/// Selects a deterministic declared embedding output shape for the fixed test request.
/// 为固定测试请求选择一项确定且已声明的向量输出形态。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact configured embedding contract.
/// `capabilities` 是精确已配置向量契约。
///
/// # Returns
/// 返回值
/// Dense output when declared, otherwise the first ordered output kind, or an explicit error.
/// 已声明时返回稠密输出，否则返回有序输出类型中的第一项，若为空则显式报错。
fn preferred_embedding_output_kind(
    capabilities: &vulcan_ai_contract::EmbeddingCapabilities,
) -> Result<EmbeddingOutputKind, String> {
    if capabilities
        .output_kinds
        .contains(&EmbeddingOutputKind::Dense)
    {
        return Ok(EmbeddingOutputKind::Dense);
    }
    capabilities
        .output_kinds
        .iter()
        .next()
        .copied()
        .ok_or_else(|| {
            String::from("The embedding route does not declare any supported output shape.")
        })
}

/// Selects a deterministic declared embedding encoding for the fixed test request.
/// 为固定测试请求选择一项确定且已声明的向量编码。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact configured embedding contract.
/// `capabilities` 是精确已配置向量契约。
///
/// # Returns
/// 返回值
/// Float encoding when declared, otherwise the first ordered encoding, or an explicit error.
/// 已声明时返回浮点编码，否则返回有序编码中的第一项，若为空则显式报错。
fn preferred_embedding_encoding(
    capabilities: &vulcan_ai_contract::EmbeddingCapabilities,
) -> Result<EmbeddingEncoding, String> {
    if capabilities.encodings.contains(&EmbeddingEncoding::Float) {
        return Ok(EmbeddingEncoding::Float);
    }
    capabilities
        .encodings
        .iter()
        .next()
        .copied()
        .ok_or_else(|| {
            String::from("The embedding route does not declare any supported value encoding.")
        })
}

/// Selects one deterministic declared rerank truncation policy.
/// 选择一项确定且已声明的重排截断策略。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact configured rerank contract.
/// `capabilities` 是精确已配置重排契约。
///
/// # Returns
/// 返回值
/// Disabled truncation when declared, otherwise the first ordered policy, or an explicit error.
/// 已声明时返回禁用截断，否则返回有序策略中的第一项，若为空则显式报错。
fn preferred_rerank_truncation(
    capabilities: &vulcan_ai_contract::RerankCapabilities,
) -> Result<RerankTruncation, String> {
    if capabilities
        .truncation
        .contains(&RerankTruncation::Disabled)
    {
        return Ok(RerankTruncation::Disabled);
    }
    capabilities
        .truncation
        .iter()
        .next()
        .copied()
        .ok_or_else(|| {
            String::from("The rerank route does not declare any supported truncation policy.")
        })
}

/// Builds a deterministic candidate set large enough for the configured test result count.
/// 构建一组足以覆盖已配置测试结果数量的确定候选。
///
/// # Parameters
/// 参数
/// `candidate_count` is already bounded by the exact model capability and configured Top N.
/// `candidate_count` 已受精确模型能力与已配置 Top N 约束。
///
/// # Returns
/// 返回值
/// Stable text candidates with unique IDs and no user content.
/// 返回带唯一标识且不包含用户内容的稳定文本候选。
fn fixed_rerank_test_candidates(candidate_count: u32) -> Vec<RerankText> {
    (0..candidate_count)
        .map(|index| {
            let text = match index {
                0 => String::from("Rust is a systems programming language."),
                1 => String::from("A maple tree loses leaves during autumn."),
                _ => format!("Fixed unrelated settings test document {index}."),
            };
            RerankText {
                id: format!("settings-test-candidate-{index}"),
                text,
            }
        })
        .collect()
}

/// Independently verifies embedding count, identity, dimensions, and finite numeric values.
/// 独立校验向量数量、身份、维度与有限数值。
///
/// # Parameters
/// 参数
/// `response` is the normalized provider result returned by the configured inference facade.
/// `response` 是已配置推理门面返回的归一化供应商结果。
///
/// # Returns
/// 返回值
/// Success only when one stable result matches its declared fingerprint and numeric invariants.
/// 仅当一个稳定结果符合其声明指纹与数值不变量时返回成功。
fn validate_embedding_test_response(
    response: &vulcan_ai_contract::EmbeddingResponse,
) -> Result<(), String> {
    if response.results.len() != 1 {
        return Err(format!(
            "Embedding test expected one result but received {}.",
            response.results.len()
        ));
    }
    let result = &response.results[0];
    if result.input_id != "settings-test-input" || result.input_index != 0 {
        return Err(String::from(
            "Embedding test response did not preserve input identity and index.",
        ));
    }
    let dimensions = usize::try_from(response.fingerprint.dimensions)
        .map_err(|_| String::from("Embedding dimensions cannot be represented on this host."))?;
    if dimensions == 0 {
        return Err(String::from(
            "Embedding test response reported zero dimensions.",
        ));
    }
    match &result.embedding {
        EmbeddingPayload::Dense { values } => {
            if values.len() != dimensions || values.iter().any(|value| !value.is_finite()) {
                return Err(String::from(
                    "Embedding test dense result has invalid dimensions or non-finite values.",
                ));
            }
        }
        EmbeddingPayload::Sparse { indices, values } => {
            if indices.len() != values.len()
                || values.iter().any(|value| !value.is_finite())
                || indices
                    .iter()
                    .any(|index| *index >= response.fingerprint.dimensions)
            {
                return Err(String::from(
                    "Embedding test sparse result has invalid indices or non-finite values.",
                ));
            }
        }
        EmbeddingPayload::MultiVector { vectors } => {
            if vectors.is_empty()
                || vectors.iter().any(|vector| {
                    vector.len() != dimensions || vector.iter().any(|value| !value.is_finite())
                })
            {
                return Err(String::from(
                    "Embedding test multi-vector result has invalid dimensions or values.",
                ));
            }
        }
        EmbeddingPayload::Base64 { value } => {
            if value.is_empty() {
                return Err(String::from(
                    "Embedding test Base64 result must not be empty.",
                ));
            }
        }
    }
    Ok(())
}

/// Independently verifies rerank indices, ranks, ordering, and finite raw scores.
/// 独立校验重排索引、名次、顺序与有限原始分数。
///
/// # Parameters
/// 参数
/// `response` is the normalized rerank result and `candidate_count` is the exact submitted size.
/// `response` 是归一化重排结果，`candidate_count` 是精确提交数量。
///
/// # Returns
/// 返回值
/// Success only when every result is unique, in range, finite, and follows declared ordering.
/// 仅当每项结果唯一、未越界、为有限值且遵守声明排序时返回成功。
fn validate_rerank_test_response(
    response: &vulcan_ai_contract::RerankResponse,
    candidate_count: u32,
) -> Result<(), String> {
    if response.results.is_empty() {
        return Err(String::from("Rerank test returned no results."));
    }
    let mut original_indices = BTreeSet::new();
    for (expected_rank, result) in response.results.iter().enumerate() {
        if result.rank != u32::try_from(expected_rank).unwrap_or(u32::MAX)
            || result.original_index >= candidate_count
            || !original_indices.insert(result.original_index)
            || !result.score.is_finite()
        {
            return Err(String::from(
                "Rerank test response contains invalid ranks, indices, or scores.",
            ));
        }
    }
    let ordered =
        response
            .results
            .windows(2)
            .all(|pair| match response.fingerprint.score_semantics {
                RerankScoreSemantics::HigherIsMoreRelevant => pair[0].score >= pair[1].score,
                RerankScoreSemantics::LowerIsMoreRelevant => pair[0].score <= pair[1].score,
                RerankScoreSemantics::Unknown => true,
            });
    if !ordered {
        return Err(String::from(
            "Rerank test scores do not follow the declared score semantics.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
