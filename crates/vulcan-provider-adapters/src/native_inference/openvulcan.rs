//! Exact OpenVulcan inference projection with response identity and vector validation.
//! 准确的 OpenVulcan 推理投影，包含响应身份及向量校验。

use super::*;
use crate::openvulcan::{CONTRACT_VERSION, DECISION_CONTRACT_VERSION};
use serde_json::{Value, json};

/// Prevents automatic replay of a separately billable native dispatch, preserving its outcome evidence.
/// 阻止自动重放独立计费的原生分派，并保留其结果证据。
/// Returns the original failure with retry disabled; uncertain outcomes remain explicitly uncertain.
/// 返回禁用重试的原始失败；未知结果仍明确保持未知。
pub(super) fn non_replayable_failure(
    mut failure: ProviderExecutionFailure,
) -> ProviderExecutionFailure {
    failure.error.retryable = false;
    failure
}

/// Successful native embedding response.
/// 成功的原生向量响应。
#[derive(Deserialize)]
struct Embeddings {
    /// Exact response contract and per-dispatch correlation identity.
    /// 准确响应契约及逐次分派关联身份。
    contract_version: String,
    /// Exact identifier of this physical request, not an idempotency key.
    /// 本次物理请求的准确标识，不是幂等键。
    request_id: String,
    /// Public model and effective vector width.
    /// 公开模型及实际向量宽度。
    model_id: String,
    /// Actual response width, checked against the explicit or published default width.
    /// 实际响应宽度，与显式宽度或发布默认宽度校验。
    dimensions: u32,
    /// Exact output representation.
    /// 准确输出表示。
    output_kind: EmbeddingOutputKind,
    /// Declared numeric representation of every vector.
    /// 每个向量声明的数值表示。
    encoding: EmbeddingEncoding,
    /// One result for every input.
    /// 每个输入对应一个结果。
    data: Vec<Vector>,
    /// Provider-reported token count and point charge.
    /// 供应商报告的 Token 数及积分扣费。
    usage: NativeUsage,
}

/// Required usage fields of a settled native request.
/// 已结算原生请求的必填用量字段。
#[derive(Deserialize)]
struct NativeUsage {
    /// Exact nonnegative input token count.
    /// 准确非负输入 Token 数。
    input_tokens: u64,
}

/// One indexed vector with output-kind-specific optional payloads.
/// 一个按输出类型携带可选载荷的带索引向量。
#[derive(Deserialize)]
struct Vector {
    /// Original input index.
    /// 原始输入索引。
    index: u32,
    /// Dense payload is present only for dense output.
    /// 仅稠密输出存在稠密载荷。
    dense: Option<Vec<f32>>,
    /// Sparse payload may be absent for a zero sparse vector under the server's omitempty contract.
    /// 根据服务端 omitempty 契约，零稀疏向量可省略稀疏载荷。
    sparse: Option<Vec<Coordinate>>,
}
/// One sorted sparse coordinate; finite zero weights are valid server output.
/// 一个有序稀疏坐标；有限零权重是合法的服务端输出。
#[derive(Deserialize)]
struct Coordinate {
    /// Nonnegative coordinate index.
    /// 非负坐标索引。
    index: u32,
    /// Finite coordinate value.
    /// 有限坐标值。
    value: f32,
}

/// Validates the consumer identity before dispatch instead of silently rewriting its owner.
/// 在分派前校验消费者身份，不静默重写其归属。
fn validate_consumer(value: &str) -> Result<(), ProviderExecutionFailure> {
    if value.is_empty()
        || value.len() > 64
        || !value.as_bytes()[0].is_ascii_lowercase()
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        return Err(pre_dispatch_failure(
            "OpenVulcan consumer profile id violates its declared contract.",
        ));
    }
    Ok(())
}

/// Replaces the System One envelope with the native versioned envelope, retaining state/questions.
/// 将 System One 封装替换为原生版本化封装，并保留状态与问题。
pub(super) fn project_decision(
    body: &mut Value,
    request: &DecisionRequest,
    request_id: &str,
) -> Result<(), ProviderExecutionFailure> {
    validate_consumer(&request.consumer_profile_id)?;
    // object is produced by the typed SystemOne serializer, never guessed from external JSON.
    // object 来自强类型 SystemOne 序列化器，绝不从外部 JSON 猜测。
    let object = body
        .as_object_mut()
        .ok_or_else(|| pre_dispatch_failure("Decision serializer did not return an object."))?;
    // model is the serializer-owned model identity moved into the native field.
    // model 是序列化器持有的模型身份，迁移至原生字段。
    let model = object
        .remove("model")
        .ok_or_else(|| pre_dispatch_failure("Decision serializer omitted its model."))?;
    object.insert(String::from("model_id"), model);
    object.insert(
        String::from("contract_version"),
        json!(DECISION_CONTRACT_VERSION),
    );
    object.insert(String::from("request_id"), json!(request_id));
    object.insert(
        String::from("consumer_profile_id"),
        json!(request.consumer_profile_id),
    );
    Ok(())
}

/// Verifies native response correlation and maps documented answer/usage fields to the shared decoder.
/// 验证原生响应关联，并将已记录答案及用量字段映射至共享解码器。
/// body is the bounded response; request_id/model_id are the frozen dispatch identities. Returns decoder JSON or a protocol failure.
/// body 是有界响应；request_id/model_id 是冻结的分派身份。返回解码器 JSON 或协议失败。
pub(super) fn decision_response(
    body: &str,
    request_id: &str,
    model_id: &str,
) -> Result<String, ProviderExecutionFailure> {
    // response has exact native identity fields; missing identity is always a protocol error.
    // response 带准确原生身份字段；缺失身份始终是协议错误。
    let mut response: Value = serde_json::from_str(body)
        .map_err(|_| response_headers_protocol_failure("Invalid OpenVulcan decision response."))?;
    if response.get("contract_version").and_then(Value::as_str) != Some(DECISION_CONTRACT_VERSION)
        || response.get("request_id").and_then(Value::as_str) != Some(request_id)
        || response.get("model_id").and_then(Value::as_str) != Some(model_id)
        || response
            .pointer("/usage/input_tokens")
            .and_then(Value::as_u64)
            .is_none()
    {
        return Err(response_headers_protocol_failure(
            "OpenVulcan decision response identity or usage did not match.",
        ));
    }
    response["model"] = json!(model_id);
    response["id"] = json!(request_id);
    serde_json::to_string(&response).map_err(|_| {
        response_headers_protocol_failure("Failed to normalize OpenVulcan decision response.")
    })
}

impl ProviderExecutionClient {
    /// Executes the frozen native embedding request and returns complete validated vectors.
    /// 执行冻结的原生向量请求，并返回完整已验证向量。
    /// route/model/revisions own routing; request/secret own input and authorization; failures retain dispatch stage.
    /// route/model/revisions 持有路由；request/secret 持有输入及授权；失败保留分派阶段。
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_openvulcan_embedding(
        &self,
        route: &ProviderExecutionRoute,
        model: &ConnectionModel,
        request: &EmbeddingRequest,
        usage_scope: InferenceUsageScope,
        revisions: ExecutionRevisionRefs<'_>,
        secret: &CredentialExecutionSecret,
        wire_request_capture: Option<oneshot::Sender<crate::execution::ProviderWireRequestCapture>>,
    ) -> Result<EmbeddingResponse, ProviderExecutionFailure> {
        // started_at and request_id identify this single billable dispatch, never an automatic replay.
        // started_at 和 request_id 标识此次独立计费分派，绝不表示自动重放。
        let started_at = Instant::now();
        let request_id = uuid::Uuid::new_v4().to_string();
        validate_consumer(&request.consumer_profile_id)?;
        if request.invalid_input_policy
            != vulcan_ai_contract::EmbeddingInvalidInputPolicy::FailBatch
        {
            return Err(pre_dispatch_failure(
                "OpenVulcan supports whole-request rejection only.",
            ));
        }
        // capabilities admits inputs using the exact per-account discovery evidence.
        // capabilities 使用准确的逐账户发现证据准入输入。
        let capabilities = model.capabilities.embedding.as_ref().ok_or_else(|| {
            pre_dispatch_failure("OpenVulcan model omitted embedding capabilities.")
        })?;
        capabilities
            .validate_request(request)
            .map_err(pre_dispatch_failure)?;
        // native_route, endpoint and binding_id are resolved from the frozen declared operation.
        // native_route、endpoint 与 binding_id 从冻结的已声明操作解析。
        let (native_route, endpoint, binding_id) =
            NativeInferenceRoute::embedding(route, &model.id)?;
        // inputs preserve the caller's order without accepting undeclared resource inputs.
        // inputs 保留调用方顺序，不接受未声明的资源输入。
        let inputs = request
            .inputs
            .iter()
            .map(|input| match &input.content {
                EmbeddingInputContent::Text { text } => Ok(text.as_str()),
                _ => Err(pre_dispatch_failure(
                    "OpenVulcan embeddings accept text only.",
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;
        // wire_task belongs to this frozen model variant; internal semantic labels never replace discovery facts.
        // wire_task 属于当前冻结模型变体；内部语义标签绝不替换发现事实。
        let wire_task = model
            .capability_evidence
            .embedding_task_values
            .get(&request.task)
            .ok_or_else(|| {
                pre_dispatch_failure(
                    "OpenVulcan model omitted the selected embedding task wire value.",
                )
            })?;
        // body preserves the declared task spelling while projecting the remaining native request fields.
        // body 保留声明的任务拼写，同时投影其余原生请求字段。
        let body = json!({"contract_version": CONTRACT_VERSION, "request_id": request_id, "consumer_profile_id": request.consumer_profile_id, "model_id": model.id, "inputs": inputs, "task": wire_task, "dimensions": request.dimensions.unwrap_or(0), "output_kind": request.output_kind, "encoding": request.encoding, "invalid_input_policy": "reject_request"});
        // response is the single billable dispatch result; no failure permits automatic replay.
        // response 是单次计费分派结果；任何失败均不允许自动重放。
        let response = self
            .send_native_json(
                native_route,
                route,
                endpoint,
                &body,
                secret,
                wire_request_capture,
            )
            .await
            .map_err(non_replayable_failure)?;
        // wire and results have already passed correlation, width and complete-index validation.
        // wire 与 results 已通过关联、宽度及完整索引校验。
        let (wire, results) = decode_embeddings(&response, &request_id, model, request)?;
        Ok(EmbeddingResponse {
            request_id: request.request_id.clone(),
            route: execution_route_snapshot(
                route,
                model,
                usage_scope,
                revisions.catalog,
                revisions.route,
                Some(binding_id),
            ),
            fingerprint: EmbeddingSpaceFingerprint {
                provider_id: route.provider.id.clone(),
                service_entry_id: route.entry.id.clone(),
                deployment_id: Some(route.connection.credential_id.clone()),
                secondary_route_id: secondary_route_id(&route.secondary_route),
                model_id: model.id.clone(),
                binding_id: binding_id.to_string(),
                capability_revision: model.capabilities.evidence_revision.clone(),
                task: request.task,
                dimensions: wire.dimensions,
                output_kind: request.output_kind,
                encoding: request.encoding,
                normalization: capabilities.normalization,
                distance_metric: capabilities.distance_metric,
            },
            results,
            dropped: Vec::new(),
            usage: RequestUsage {
                update_kind: UsageUpdateKind::Final,
                input_tokens: reported_usage_value(Some(wire.usage.input_tokens)),
                uncached_input_tokens: reported_usage_value(Some(wire.usage.input_tokens)),
                response_model: Some(wire.model_id),
                response_id: Some(wire.request_id),
                ..Default::default()
            },
            duration_ms: elapsed_milliseconds(started_at),
        })
    }
}

/// Decodes response for the exact model/request identity and validates complete indexed vectors.
/// 为准确模型／请求身份解码 response，并验证完整带索引向量。
/// Returns the validated envelope and ordered host results, rejecting malformed or mismatched data.
/// 返回已验证封装及有序宿主结果，拒绝格式错误或不匹配的数据。
fn decode_embeddings(
    response: &str,
    request_id: &str,
    model: &ConnectionModel,
    request: &EmbeddingRequest,
) -> Result<(Embeddings, Vec<EmbeddingResult>), ProviderExecutionFailure> {
    // capabilities freezes the server-advertised dimension authority for this response.
    // capabilities 冻结本响应的服务端声明维度权威。
    let capabilities =
        model.capabilities.embedding.as_ref().ok_or_else(|| {
            pre_dispatch_failure("OpenVulcan model omitted embedding capabilities.")
        })?;
    // wire is decoded without substituting absent response identities or numeric fields.
    // wire 解码时不替换缺失的响应身份或数值字段。
    let mut wire: Embeddings = serde_json::from_str(response)
        .map_err(|_| response_headers_protocol_failure("Invalid OpenVulcan embedding response."))?;
    // expected_dimensions follows the explicit request or the model's published default.
    // expected_dimensions 遵循请求显式维度或模型发布的默认维度。
    let expected_dimensions = request
        .dimensions
        .or(capabilities.default_dimensions)
        .ok_or_else(|| {
            response_headers_protocol_failure(
                "OpenVulcan omitted its default embedding dimensions.",
            )
        })?;
    if wire.contract_version != CONTRACT_VERSION
        || wire.request_id != request_id
        || wire.model_id != model.id
        || wire.output_kind != request.output_kind
        || wire.encoding != request.encoding
        || wire.dimensions == 0
        || !capabilities.dimensions.contains(&wire.dimensions)
        || wire.dimensions != expected_dimensions
        || wire.data.len() != request.inputs.len()
    {
        return Err(response_headers_protocol_failure(
            "OpenVulcan embedding response does not match the request.",
        ));
    }
    // results restores caller input order; seen rejects repeated response indices.
    // results 恢复调用方输入顺序；seen 拒绝重复响应索引。
    let mut results = Vec::with_capacity(wire.data.len());
    let mut seen = BTreeSet::new();
    for item in std::mem::take(&mut wire.data) {
        // input is the unique request entry selected by this validated response index.
        // input 是本已验证响应索引选中的唯一请求条目。
        let input = request.inputs.get(item.index as usize).ok_or_else(|| {
            response_headers_protocol_failure(
                "OpenVulcan returned an out-of-range embedding index.",
            )
        })?;
        if !seen.insert(item.index) {
            return Err(response_headers_protocol_failure(
                "OpenVulcan returned a duplicate embedding index.",
            ));
        }
        // embedding is projected only through the requested, published vector representation.
        // embedding 仅通过请求指定且已发布的向量表示投影。
        let embedding = match request.output_kind {
            EmbeddingOutputKind::Dense => {
                // values must fill the entire declared dense width with finite coordinates.
                // values 必须以有限坐标填满声明的整个稠密宽度。
                let values = item.dense.ok_or_else(|| {
                    response_headers_protocol_failure("OpenVulcan omitted a dense vector.")
                })?;
                if item.sparse.is_some()
                    || values.len() != wire.dimensions as usize
                    || values.iter().any(|value| !value.is_finite())
                {
                    return Err(response_headers_protocol_failure(
                        "OpenVulcan returned an invalid dense vector.",
                    ));
                }
                EmbeddingPayload::Dense { values }
            }
            EmbeddingOutputKind::Sparse => {
                if item.dense.is_some() {
                    return Err(response_headers_protocol_failure(
                        "OpenVulcan returned dense data for a sparse request.",
                    ));
                }
                // coordinates may be omitted only for the documented empty sparse representation.
                // 仅已记录的空稀疏表示允许省略 coordinates。
                let coordinates = item.sparse.unwrap_or_default();
                if coordinates
                    .iter()
                    .any(|coordinate| !coordinate.value.is_finite())
                    || coordinates
                        .windows(2)
                        .any(|pair| pair[0].index >= pair[1].index)
                {
                    return Err(response_headers_protocol_failure(
                        "OpenVulcan returned invalid sparse coordinates.",
                    ));
                }
                EmbeddingPayload::Sparse {
                    indices: coordinates
                        .iter()
                        .map(|coordinate| coordinate.index)
                        .collect(),
                    values: coordinates
                        .into_iter()
                        .map(|coordinate| coordinate.value)
                        .collect(),
                }
            }
            EmbeddingOutputKind::MultiVector => {
                return Err(response_headers_protocol_failure(
                    "OpenVulcan does not declare multivector output.",
                ));
            }
        };
        results.push(EmbeddingResult {
            input_id: input.input_id.clone(),
            input_index: item.index,
            embedding,
        });
    }
    results.sort_by_key(|result| result.input_index);
    Ok((wire, results))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one native model/request/body triple with deliberately reversed response ordering.
    /// 构建原生模型／请求／正文三元组，并刻意反转响应顺序。
    fn embedding_fixture() -> (ConnectionModel, EmbeddingRequest, Value) {
        let catalog = crate::openvulcan::parse_catalog("fixture", 1, &json!({"contract_version":CONTRACT_VERSION,"models":[{"kind":"embedding","model_id":"vector","display_name":"Vector","capabilities":{"max_input_tokens_per_item":8,"max_batch_size":2,"default_dimensions":2,"allowed_dimensions":[2],"allowed_tasks":["document"],"allowed_output_kinds":["dense","sparse"],"allowed_encodings":["float"]}}]}).to_string()).unwrap();
        let model = catalog
            .models
            .into_iter()
            .find(|model| model.id == "vector")
            .unwrap();
        let request = EmbeddingRequest {
            contract_version: vulcan_ai_contract::INFERENCE_CONTRACT_VERSION,
            request_id: String::from("host-request"),
            idempotency_key: None,
            consumer_profile_id: String::from("vmm"),
            inputs: ["first", "second"]
                .into_iter()
                .map(|id| vulcan_ai_contract::EmbeddingInput {
                    input_id: id.to_string(),
                    content: EmbeddingInputContent::Text {
                        text: id.to_string(),
                    },
                })
                .collect(),
            task: EmbeddingTask::Document,
            dimensions: Some(2),
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            invalid_input_policy: Default::default(),
        };
        let body = json!({"contract_version":CONTRACT_VERSION,"request_id":"dispatch","model_id":"vector","dimensions":2,"output_kind":"dense","encoding":"float","data":[{"index":1,"dense":[0.0,1.0]},{"index":0,"dense":[1.0,0.0]}],"usage":{"input_tokens":2,"charged_micro_points":1,"ledger":"free"}});
        (model, request, body)
    }

    /// Native responses preserve input identity and reject foreign models, request IDs and duplicate indices.
    /// 原生响应保留输入身份，并拒绝其他模型、请求 ID 和重复索引。
    #[test]
    fn openvulcan_vectors_preserve_identity_and_reject_corruption() {
        let (model, request, body) = embedding_fixture();
        let (_, results) =
            decode_embeddings(&body.to_string(), "dispatch", &model, &request).unwrap();
        assert_eq!(
            results
                .iter()
                .map(|item| item.input_id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        for (field, value) in [
            ("model_id", json!("foreign")),
            ("request_id", json!("old")),
            ("contract_version", json!("unknown")),
            ("dimensions", json!(4)),
        ] {
            let mut malformed = body.clone();
            malformed[field] = value;
            assert!(
                decode_embeddings(&malformed.to_string(), "dispatch", &model, &request).is_err()
            );
        }
        let mut duplicate = body.clone();
        for item in duplicate["data"].as_array_mut().unwrap() {
            item["index"] = json!(0);
        }
        assert!(decode_embeddings(&duplicate.to_string(), "dispatch", &model, &request).is_err());
    }

    /// Empty sparse vectors and finite zero coordinates follow the actual user-center response contract.
    /// 空稀疏向量及有限零坐标遵循实际用户中心响应契约。
    #[test]
    fn openvulcan_sparse_accepts_empty_and_zero_but_rejects_unordered_indices() {
        let (model, mut request, mut body) = embedding_fixture();
        request.output_kind = EmbeddingOutputKind::Sparse;
        body["output_kind"] = json!("sparse");
        body["data"] = json!([{"index":0},{"index":1,"sparse":[{"index":100,"value":0.0}]}]);
        assert!(decode_embeddings(&body.to_string(), "dispatch", &model, &request).is_ok());
        body["data"] = json!([{"index":0},{"index":1,"sparse":[{"index":100,"value":1.0},{"index":5,"value":0.1}]}]);
        assert!(decode_embeddings(&body.to_string(), "dispatch", &model, &request).is_err());
    }

    /// An omitted width uses the published default, even when another width is also allowed.
    /// 省略宽度时使用发布的默认值，即使另一个宽度也被允许。
    #[test]
    fn openvulcan_implicit_dimensions_match_the_catalog_default() {
        // model publishes two valid widths while request delegates width selection to the server.
        // model 发布两个有效宽度，request 将宽度选择交由服务端。
        let (mut model, mut request, mut body) = embedding_fixture();
        model
            .capabilities
            .embedding
            .as_mut()
            .unwrap()
            .dimensions
            .insert(4);
        request.dimensions = None;
        assert!(decode_embeddings(&body.to_string(), "dispatch", &model, &request).is_ok());
        body["dimensions"] = json!(4);
        body["data"] =
            json!([{"index":0,"dense":[0.0,0.0,0.0,0.0]},{"index":1,"dense":[0.0,0.0,0.0,0.0]}]);
        assert!(decode_embeddings(&body.to_string(), "dispatch", &model, &request).is_err());
    }

    /// Decision normalization retains reported confidence and usage and rejects cross-request answers.
    /// 判定归一化保留报告置信度及用量，并拒绝跨请求答案。
    #[test]
    fn openvulcan_decision_keeps_reported_evidence() {
        let body = json!({"contract_version":DECISION_CONTRACT_VERSION,"request_id":"dispatch","model_id":"decision","answers":{"q":{"type":"choice","choice":"review","confidence":0.8,"probabilities":{"allow":0.1,"review":0.9}}},"usage":{"input_tokens":7}});
        let normalized = decision_response(&body.to_string(), "dispatch", "decision").unwrap();
        let decoded = decode_systemone_response(&normalized).unwrap();
        assert_eq!(decoded.usage.input_tokens, Some(7));
        assert!(decoded.usage.output_tokens.is_none());
        assert!(decision_response(&body.to_string(), "foreign", "decision").is_err());
    }

    /// The decision envelope carries exactly the server's decision fields under the decision contract,
    /// and a response under the embedding contract is rejected.
    /// 判定封装在判定契约下恰好携带服务端的判定字段，向量契约下的响应被拒绝。
    #[test]
    fn openvulcan_decision_uses_its_own_contract_version() {
        let request = DecisionRequest {
            contract_version: vulcan_ai_contract::INFERENCE_CONTRACT_VERSION,
            request_id: String::from("host-request"),
            idempotency_key: None,
            consumer_profile_id: String::from("permission_judge"),
            state: json!({"call": {"tool": "shell_command"}}),
            questions: vec![vulcan_ai_contract::DecisionQuestion {
                id: String::from("decision"),
                kind: vulcan_ai_contract::DecisionQuestionKind::Choice {
                    instructions: String::from("Judge."),
                    criteria: ["allow", "review"]
                        .map(|option| vulcan_ai_contract::DecisionCriterion {
                            option: option.to_string(),
                            criterion: format!("{option}."),
                        })
                        .to_vec(),
                },
            }],
        };
        let mut body = serde_json::to_value(vulcan_llm_core::SystemOneWireRequest {
            model: "decision",
            request: &request,
        })
        .unwrap();
        project_decision(&mut body, &request, "dispatch").unwrap();
        let mut keys = body
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(
            keys,
            [
                "consumer_profile_id",
                "contract_version",
                "model_id",
                "questions",
                "request_id",
                "state"
            ]
        );
        assert_eq!(body["contract_version"], DECISION_CONTRACT_VERSION);
        assert_eq!(body["questions"]["decision"]["type"], "choice");
        assert_eq!(
            body["questions"]["decision"]["criteria"]["review"],
            "review."
        );
        let embedding_versioned = json!({"contract_version":CONTRACT_VERSION,"request_id":"dispatch","model_id":"decision","answers":{},"usage":{"input_tokens":1}});
        assert!(
            decision_response(&embedding_versioned.to_string(), "dispatch", "decision").is_err()
        );
    }

    /// Lost responses preserve their uncertain dispatch outcome and disable billable automatic retries.
    /// 丢失响应保留未知分派结果，并禁用计费自动重试。
    #[test]
    fn openvulcan_uncertain_dispatch_never_retries() {
        let original = transport_failure("Synthetic response loss.");
        let expected_outcome = original.error.dispatch_outcome;
        let failure = non_replayable_failure(original);
        assert!(!failure.error.retryable);
        assert_eq!(failure.error.dispatch_outcome, expected_outcome);
    }
}
