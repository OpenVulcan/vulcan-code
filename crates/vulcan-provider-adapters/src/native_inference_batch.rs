//! Provider-native asynchronous embedding Batch planning and execution.
//! 供应商原生异步向量 Batch 规划与执行。

use crate::{
    ConnectionModel, CredentialExecutionSecret, NativeInferenceBatchBinding,
    NativeInferenceBatchProtocolKind, ProviderExecutionClient, ProviderExecutionFailure,
    ProviderExecutionRoute,
    execution::{
        execution_headers, http_failure, http_send_transport_failure, parse_retry_after_headers,
        pre_dispatch_failure, read_bounded_response_text, response_headers_protocol_failure,
        transport_failure,
    },
    graph::EMBEDDING_BATCH_ABSOLUTE_MAXIMUM_RESULT_BYTES,
    native_inference::{
        ExecutionRevisionRefs, normalize_embedding_batch_bodies, select_native_batch_binding,
    },
};
use reqwest::{Method, RequestBuilder, header::CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use vulcan_ai_contract::{
    AiErrorCategory, AiExecutionError, AiOperation, DispatchOutcome, EmbeddingBatchCapabilities,
    EmbeddingBatchCollectedResult, EmbeddingBatchPlan, EmbeddingBatchProviderState,
    EmbeddingBatchSnapshot, EmbeddingBatchSubmission, EmbeddingBatchTaskPlan,
    EmbeddingBatchUploadedInput, EmbeddingEncoding, EmbeddingInputContent, EmbeddingRequest,
    InferenceBatchResultDelivery, InferenceBatchTransport, InferenceUsageScope,
};

/// Streams deterministic serialized request bytes directly into SHA-256 without a second full buffer.
/// 将确定性序列化请求字节直接流入 SHA-256，避免第二份完整缓冲区。
struct Sha256Writer(Sha256);

impl Write for Sha256Writer {
    /// Feeds one serialized byte slice into the digest and reports complete consumption.
    /// 将一段序列化字节送入摘要并报告已完整消费。
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    /// Flushes the digest writer, which has no buffered transport state.
    /// 刷新不持有传输缓冲状态的摘要写入器。
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Retains exact provider bytes beside their public deterministic task description.
/// 在公开确定性任务描述旁保留精确供应商字节。
struct PreparedEmbeddingBatchTask {
    /// Public immutable task plan.
    /// 公开不可变任务计划。
    plan: EmbeddingBatchTaskPlan,
    /// Exact bytes sent inline or uploaded as JSONL.
    /// 内嵌发送或作为 JSONL 上传的精确字节。
    payload: Vec<u8>,
}

/// Serializes one OpenRouter Batch request item.
/// 序列化一个 OpenRouter Batch 请求项。
#[derive(Serialize)]
struct OpenRouterBatchRequestItem<'a> {
    /// Stable per-input correlation identity.
    /// 稳定的单输入关联身份。
    custom_id: String,
    /// Standard single-input embedding body.
    /// 标准单输入向量正文。
    body: CompatibleEmbeddingBatchBody<'a>,
}

/// Serializes one Alibaba JSONL Batch request item.
/// 序列化一个阿里云 JSONL Batch 请求项。
#[derive(Serialize)]
struct AlibabaBatchRequestItem<'a> {
    /// Stable per-input correlation identity.
    /// 稳定的单输入关联身份。
    custom_id: String,
    /// HTTP method required by the JSONL contract.
    /// JSONL 契约要求的 HTTP 方法。
    method: &'static str,
    /// Per-item endpoint required by the JSONL contract.
    /// JSONL 契约要求的单项端点。
    url: &'a str,
    /// Standard single-input embedding body.
    /// 标准单输入向量正文。
    body: AlibabaEmbeddingBatchBody<'a>,
}

/// Serializes one compatible single-input embedding body.
/// 序列化一个兼容协议单输入向量正文。
#[derive(Serialize)]
struct CompatibleEmbeddingBatchBody<'a> {
    /// Exact model identity required by the selected embedding endpoint.
    /// 所选向量端点要求的精确模型身份。
    model: &'a str,
    /// Exact text input.
    /// 精确文本输入。
    input: &'a str,
    /// Optional requested dimensions.
    /// 可选请求维度。
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<u32>,
    /// Requested output encoding.
    /// 请求输出编码。
    encoding_format: &'static str,
}

/// Serializes one Alibaba single-input embedding body.
/// 序列化一个阿里云单输入向量正文。
#[derive(Serialize)]
struct AlibabaEmbeddingBatchBody<'a> {
    /// Exact model identity repeated in every JSONL line.
    /// 每一条 JSONL 行重复携带的精确模型身份。
    model: &'a str,
    /// Exact text input.
    /// 精确文本输入。
    input: &'a str,
    /// Optional requested dimensions.
    /// 可选请求维度。
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<u32>,
    /// Requested output encoding.
    /// 请求输出编码。
    encoding_format: &'static str,
}

/// Parses the common provider Batch object returned by submit, poll, and cancel operations.
/// 解析提交、轮询与取消操作返回的通用供应商 Batch 对象。
#[derive(Debug, Deserialize)]
struct WireBatchObject {
    /// Provider-owned Batch identity.
    /// 供应商持有的 Batch 身份。
    id: String,
    /// Provider lifecycle state.
    /// 供应商生命周期状态。
    status: String,
    /// Optional file identity used to reconcile a creation response.
    /// 用于对账创建响应的可选文件身份。
    #[serde(default)]
    input_file_id: Option<String>,
    /// Optional caller metadata returned by Alibaba task queries.
    /// 阿里云任务查询返回的可选调用方元数据。
    #[serde(default)]
    metadata: Option<BTreeMap<String, serde_json::Value>>,
    /// Optional item counts.
    /// 可选条目计数。
    #[serde(default)]
    request_counts: Option<WireBatchRequestCounts>,
    /// Optional output file identity.
    /// 可选输出文件身份。
    #[serde(default)]
    output_file_id: Option<String>,
    /// Optional error file identity.
    /// 可选错误文件身份。
    #[serde(default)]
    error_file_id: Option<String>,
    /// Inline OpenRouter results available only after completion.
    /// 仅完成后可用的 OpenRouter 内嵌结果。
    #[serde(default)]
    results: Option<Vec<WireBatchResultItem>>,
    /// Provider-level terminal diagnostics using the compatible Batch `errors` field.
    /// 使用兼容 Batch `errors` 字段的供应商级终态诊断。
    #[serde(default, alias = "error")]
    errors: Option<serde_json::Value>,
}

/// Parses provider-reported Batch item counters.
/// 解析供应商报告的 Batch 条目计数。
#[derive(Debug, Deserialize)]
struct WireBatchRequestCounts {
    /// Total accepted items.
    /// 已受理条目总数。
    total: u32,
    /// Completed successful items.
    /// 已完成成功条目数。
    completed: u32,
    /// Failed items.
    /// 失败条目数。
    failed: u32,
}

/// Parses one common JSON or JSONL Batch result item.
/// 解析一个通用 JSON 或 JSONL Batch 结果项。
#[derive(Debug, Deserialize)]
struct WireBatchResultItem {
    /// Stable caller-owned correlation identity.
    /// 稳定的调用方持有关联身份。
    custom_id: String,
    /// Successful provider response envelope.
    /// 成功供应商响应包络。
    #[serde(default)]
    response: Option<WireBatchItemResponse>,
    /// Per-item provider error.
    /// 单项供应商错误。
    #[serde(default)]
    error: Option<serde_json::Value>,
}

/// Parses one successful Batch item response envelope.
/// 解析一个成功 Batch 条目响应包络。
#[derive(Debug, Deserialize)]
struct WireBatchItemResponse {
    /// Per-item HTTP status.
    /// 单项 HTTP 状态。
    status_code: u16,
    /// Standard embedding response body.
    /// 标准向量响应正文。
    body: serde_json::Value,
}

/// Retains only one provider-owned item error code and bounded explanatory message.
/// 仅保留一条供应商条目错误码和有界说明消息。
struct BatchErrorExample {
    /// Stable provider error code, when supplied.
    /// 供应商提供时的稳定错误码。
    code: Option<String>,
    /// Bounded provider message, when supplied.
    /// 供应商提供时的有界消息。
    message: Option<String>,
}

/// Parses one uploaded Alibaba File object.
/// 解析一个已上传阿里云 File 对象。
#[derive(Debug, Deserialize)]
struct WireUploadedFile {
    /// Provider-owned file identity.
    /// 供应商持有的文件身份。
    id: String,
}

/// Parses one paginated provider Batch list response used only for exact reconciliation.
/// 解析仅用于精确对账的分页供应商 Batch 列表响应。
#[derive(Debug, Deserialize)]
struct WireBatchList {
    /// Batch objects in the requested filtered page.
    /// 请求过滤分页中的 Batch 对象。
    data: Vec<WireBatchObject>,
    /// Whether another filtered page exists.
    /// 是否还存在下一页过滤结果。
    #[serde(default)]
    has_more: bool,
}

/// Serializes one Alibaba Batch creation request.
/// 序列化一个阿里云 Batch 创建请求。
#[derive(Serialize)]
struct AlibabaCreateBatchRequest<'a> {
    /// Previously uploaded input file identity.
    /// 此前上传的输入文件身份。
    input_file_id: &'a str,
    /// Common endpoint used by every JSONL line.
    /// 每一条 JSONL 行共同使用的端点。
    endpoint: &'a str,
    /// Exact provider completion window.
    /// 精确供应商完成窗口。
    completion_window: String,
    /// Stable task identity retained for exact list reconciliation.
    /// 为精确列表对账保留的稳定任务身份。
    metadata: AlibabaCreateBatchMetadata<'a>,
}

/// Serializes the bounded Alibaba metadata used to identify one exact task.
/// 序列化用于识别一个精确任务的有界阿里云元数据。
#[derive(Serialize)]
struct AlibabaCreateBatchMetadata<'a> {
    /// Deterministic task identity exposed as the documented task-name field.
    /// 通过文档规定任务名称字段暴露的确定性任务身份。
    ds_name: &'a str,
}

/// Groups one immutable Batch HTTP operation before request-specific body projection.
/// 在请求特定正文投影前组合一次不可变 Batch HTTP 操作。
struct BatchHttpRequest<'a> {
    /// Frozen provider route selected for the task.
    /// 为任务选择的冻结供应商路由。
    route: &'a ProviderExecutionRoute,
    /// Exact supplemental Batch binding.
    /// 精确的补充 Batch 绑定。
    binding: &'a NativeInferenceBatchBinding,
    /// HTTP method declared by the lifecycle operation.
    /// 生命周期操作声明的 HTTP 方法。
    method: Method,
    /// Validated endpoint path relative to the selected Batch API root.
    /// 相对于所选 Batch API 根地址的已校验端点路径。
    endpoint: &'a str,
    /// Fresh execution secret projected only into request headers.
    /// 仅投影到请求头的新鲜执行凭据。
    secret: &'a CredentialExecutionSecret,
    /// Operation-specific bounded response size.
    /// 操作特定的有界响应大小。
    maximum_response_bytes: u64,
    /// Whether the successful response must declare JSON content.
    /// 成功响应是否必须声明 JSON 内容。
    require_json: bool,
}

/// Groups all immutable facts required to collect one completed Batch task.
/// 组合收集一个已完成 Batch 任务所需的全部不可变事实。
pub(crate) struct EmbeddingBatchCollectionContext<'a> {
    /// Original typed embedding request.
    /// 原始类型化向量请求。
    pub(crate) request: &'a EmbeddingRequest,
    /// Exact deterministic provider task.
    /// 精确的确定性供应商任务。
    pub(crate) task: &'a EmbeddingBatchTaskPlan,
    /// Completed durable provider observation.
    /// 已完成的持久供应商观测。
    pub(crate) snapshot: &'a EmbeddingBatchSnapshot,
    /// Authorized inference usage scope.
    /// 已授权推理用途范围。
    pub(crate) usage_scope: InferenceUsageScope,
    /// Frozen catalog and route revisions.
    /// 冻结的目录与路由修订。
    pub(crate) revisions: ExecutionRevisionRefs<'a>,
}

impl ProviderExecutionClient {
    /// Reconstructs the immutable Batch route snapshot and request fingerprint for a lifecycle call.
    /// 为生命周期调用重建不可变的 Batch 路由快照及请求指纹。
    pub(crate) fn embedding_batch_request_identity(
        &self,
        route: &ProviderExecutionRoute,
        model: &ConnectionModel,
        request: &EmbeddingRequest,
        usage_scope: InferenceUsageScope,
        revisions: ExecutionRevisionRefs<'_>,
    ) -> Result<(vulcan_ai_contract::ExecutionRouteSnapshot, String), ProviderExecutionFailure>
    {
        let (route_snapshot, _, request_fingerprint) =
            embedding_batch_identity(route, model, request, usage_scope, revisions)?;
        Ok((route_snapshot, request_fingerprint))
    }

    /// Builds the exact bounded provider payload partition for one embedding request.
    /// 为一项向量请求构建精确且有界的供应商负载分区。
    pub(crate) fn plan_embedding_batch(
        &self,
        route: &ProviderExecutionRoute,
        model: &ConnectionModel,
        request: &EmbeddingRequest,
        usage_scope: InferenceUsageScope,
        revisions: ExecutionRevisionRefs<'_>,
    ) -> Result<EmbeddingBatchPlan, ProviderExecutionFailure> {
        let (route_snapshot, capabilities, request_fingerprint) =
            embedding_batch_identity(route, model, request, usage_scope, revisions)?;
        let binding = embedding_batch_binding(route, model.id.as_str())?;
        let tasks = prepare_embedding_batch_tasks(model, request, binding)?;
        let plan = EmbeddingBatchPlan {
            request_id: request.request_id.clone(),
            request_fingerprint,
            route: route_snapshot,
            capabilities,
            tasks,
        };
        plan.validate(request.inputs.len())
            .map_err(pre_dispatch_failure)?;
        Ok(plan)
    }

    /// Uploads one exact JSONL input and returns its provider identity before task creation.
    /// 上传一个精确 JSONL 输入，并在创建任务前返回其供应商身份。
    pub(crate) async fn upload_embedding_batch_input(
        &self,
        route: &ProviderExecutionRoute,
        model: &ConnectionModel,
        request: &EmbeddingRequest,
        task: &EmbeddingBatchTaskPlan,
        secret: &CredentialExecutionSecret,
    ) -> Result<EmbeddingBatchUploadedInput, ProviderExecutionFailure> {
        let binding = embedding_batch_binding(route, model.id.as_str())?;
        if binding.capabilities.transport != InferenceBatchTransport::JsonlFile {
            return Err(pre_dispatch_failure(
                "Embedding Batch input upload is valid only for a JSONL file transport.",
            ));
        }
        let prepared = prepare_exact_embedding_batch_task(model, request, binding, task)?;
        let upload_endpoint = binding
            .file_upload_endpoint_path
            .as_deref()
            .ok_or_else(|| {
                pre_dispatch_failure("JSONL embedding Batch upload endpoint is absent.")
            })?;
        let file_name = format!("{}.jsonl", task.task_id);
        let payload = prepared.payload;
        let upload_body = self
            .send_batch_request(
                BatchHttpRequest {
                    route,
                    binding,
                    method: Method::POST,
                    endpoint: upload_endpoint,
                    secret,
                    maximum_response_bytes: 16 * 1024 * 1024,
                    require_json: true,
                },
                move |builder| {
                    let part = reqwest::multipart::Part::bytes(payload)
                        .file_name(file_name.clone())
                        .mime_str("application/jsonl")
                        .expect("static JSONL MIME type must be valid");
                    builder.multipart(
                        reqwest::multipart::Form::new()
                            .text("purpose", "batch")
                            .part("file", part),
                    )
                },
            )
            .await?;
        let uploaded: WireUploadedFile =
            serde_json::from_str(upload_body.as_str()).map_err(|error| {
                response_headers_protocol_failure(format!(
                    "Embedding Batch upload response is invalid JSON: {error}"
                ))
            })?;
        let uploaded = EmbeddingBatchUploadedInput {
            task_id: task.task_id.clone(),
            provider_input_file_id: uploaded.id,
            uploaded_at_unix_ms: current_unix_millis()?,
        };
        uploaded
            .validate()
            .map_err(response_headers_protocol_failure)?;
        Ok(uploaded)
    }

    /// Submits one exact planned remote Batch task using a previously durable file when required.
    /// 使用需要时已持久化的文件提交一个精确规划的远端 Batch 任务。
    pub(crate) async fn submit_embedding_batch(
        &self,
        route: &ProviderExecutionRoute,
        model: &ConnectionModel,
        request: &EmbeddingRequest,
        task: &EmbeddingBatchTaskPlan,
        uploaded_input: Option<&EmbeddingBatchUploadedInput>,
        secret: &CredentialExecutionSecret,
    ) -> Result<EmbeddingBatchSubmission, ProviderExecutionFailure> {
        let binding = embedding_batch_binding(route, model.id.as_str())?;
        let prepared = prepare_exact_embedding_batch_task(model, request, binding, task)?;
        let submission = match binding.capabilities.transport {
            InferenceBatchTransport::InlineJson => {
                if uploaded_input.is_some() {
                    return Err(pre_dispatch_failure(
                        "Inline embedding Batch submission cannot consume an uploaded input file.",
                    ));
                }
                let payload = prepared.payload;
                let body = self
                    .send_batch_request(
                        BatchHttpRequest {
                            route,
                            binding,
                            method: Method::POST,
                            endpoint: binding.submit_endpoint_path.as_str(),
                            secret,
                            maximum_response_bytes: 16 * 1024 * 1024,
                            require_json: true,
                        },
                        move |builder| {
                            builder
                                .header(CONTENT_TYPE, "application/json")
                                .body(payload)
                        },
                    )
                    .await?;
                let batch = parse_batch_object(body.as_str())?;
                normalize_batch_state(batch.status.as_str())?;
                EmbeddingBatchSubmission {
                    task_id: task.task_id.clone(),
                    provider_batch_id: batch.id,
                    provider_input_file_id: None,
                    submitted_at_unix_ms: current_unix_millis()?,
                }
            }
            InferenceBatchTransport::JsonlFile => {
                let uploaded = uploaded_input.ok_or_else(|| {
                    pre_dispatch_failure(
                        "JSONL embedding Batch submission requires a durable uploaded input file.",
                    )
                })?;
                uploaded.validate().map_err(pre_dispatch_failure)?;
                if uploaded.task_id != task.task_id {
                    return Err(pre_dispatch_failure(
                        "Embedding Batch uploaded input belongs to a different task.",
                    ));
                }
                let completion_window_seconds = binding
                    .capabilities
                    .provider_completion_window_seconds
                    .ok_or_else(|| {
                        pre_dispatch_failure("Alibaba embedding Batch completion window is absent.")
                    })?;
                let completion_window = format!("{}h", completion_window_seconds / 3_600);
                let create_request = AlibabaCreateBatchRequest {
                    input_file_id: uploaded.provider_input_file_id.as_str(),
                    endpoint: binding.item_endpoint_path.as_str(),
                    completion_window,
                    metadata: AlibabaCreateBatchMetadata {
                        ds_name: task.task_id.as_str(),
                    },
                };
                let create_body = self
                    .send_batch_request(
                        BatchHttpRequest {
                            route,
                            binding,
                            method: Method::POST,
                            endpoint: binding.submit_endpoint_path.as_str(),
                            secret,
                            maximum_response_bytes: 16 * 1024 * 1024,
                            require_json: true,
                        },
                        |builder| builder.json(&create_request),
                    )
                    .await?;
                let batch = parse_batch_object(create_body.as_str())?;
                validate_alibaba_batch_ownership(
                    &batch,
                    task.task_id.as_str(),
                    uploaded.provider_input_file_id.as_str(),
                )?;
                normalize_batch_state(batch.status.as_str())?;
                EmbeddingBatchSubmission {
                    task_id: task.task_id.clone(),
                    provider_batch_id: batch.id,
                    provider_input_file_id: Some(uploaded.provider_input_file_id.clone()),
                    submitted_at_unix_ms: current_unix_millis()?,
                }
            }
        };
        submission
            .validate()
            .map_err(response_headers_protocol_failure)?;
        Ok(submission)
    }

    /// Reconciles one Alibaba task by its exact input file and deterministic task metadata.
    /// 按精确输入文件及确定性任务元数据对账一项阿里云任务。
    pub(crate) async fn reconcile_embedding_batch_submission(
        &self,
        route: &ProviderExecutionRoute,
        model: &ConnectionModel,
        task: &EmbeddingBatchTaskPlan,
        uploaded_input: &EmbeddingBatchUploadedInput,
        secret: &CredentialExecutionSecret,
    ) -> Result<Option<EmbeddingBatchSubmission>, ProviderExecutionFailure> {
        uploaded_input.validate().map_err(pre_dispatch_failure)?;
        if uploaded_input.task_id != task.task_id {
            return Err(pre_dispatch_failure(
                "Embedding Batch reconciliation input belongs to a different task.",
            ));
        }
        let binding = embedding_batch_binding(route, model.id.as_str())?;
        if binding.capabilities.transport != InferenceBatchTransport::JsonlFile {
            return Err(pre_dispatch_failure(
                "Inline embedding Batch submissions cannot be reconciled by input file.",
            ));
        }
        let endpoint = format!(
            "{}?input_file_ids={}&limit=100",
            binding.submit_endpoint_path, uploaded_input.provider_input_file_id
        );
        let body = self
            .send_batch_request(
                BatchHttpRequest {
                    route,
                    binding,
                    method: Method::GET,
                    endpoint: endpoint.as_str(),
                    secret,
                    maximum_response_bytes: 16 * 1024 * 1024,
                    require_json: true,
                },
                |builder| builder,
            )
            .await?;
        let listed: WireBatchList = serde_json::from_str(body.as_str()).map_err(|error| {
            response_headers_protocol_failure(format!(
                "Embedding Batch reconciliation response is invalid JSON: {error}"
            ))
        })?;
        if listed.has_more {
            return Err(response_headers_protocol_failure(
                "Embedding Batch reconciliation returned an ambiguous paginated result.",
            ));
        }
        let mut matches = listed.data.into_iter().filter(|batch| {
            batch.input_file_id.as_deref() == Some(uploaded_input.provider_input_file_id.as_str())
                && batch
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("ds_name"))
                    .and_then(serde_json::Value::as_str)
                    == Some(task.task_id.as_str())
        });
        let Some(batch) = matches.next() else {
            return Ok(None);
        };
        if matches.next().is_some() {
            return Err(response_headers_protocol_failure(
                "Embedding Batch reconciliation matched more than one provider task.",
            ));
        }
        normalize_batch_state(batch.status.as_str())?;
        let submission = EmbeddingBatchSubmission {
            task_id: task.task_id.clone(),
            provider_batch_id: batch.id,
            provider_input_file_id: Some(uploaded_input.provider_input_file_id.clone()),
            submitted_at_unix_ms: current_unix_millis()?,
        };
        submission
            .validate()
            .map_err(response_headers_protocol_failure)?;
        Ok(Some(submission))
    }

    /// Polls one submitted remote Batch by its durable provider identity.
    /// 按持久供应商身份轮询一个已提交远端 Batch。
    pub(crate) async fn embedding_batch_status(
        &self,
        route: &ProviderExecutionRoute,
        model: &ConnectionModel,
        submission: &EmbeddingBatchSubmission,
        secret: &CredentialExecutionSecret,
    ) -> Result<EmbeddingBatchSnapshot, ProviderExecutionFailure> {
        submission.validate().map_err(pre_dispatch_failure)?;
        let binding = embedding_batch_binding(route, model.id.as_str())?;
        let endpoint = expand_endpoint_template(
            binding.status_endpoint_path_template.as_str(),
            "{batch_id}",
            submission.provider_batch_id.as_str(),
        )?;
        let body = self
            .send_batch_request(
                BatchHttpRequest {
                    route,
                    binding,
                    method: Method::GET,
                    endpoint: endpoint.as_str(),
                    secret,
                    maximum_response_bytes: binding.capabilities.host_policy_maximum_result_bytes,
                    require_json: true,
                },
                |builder| builder,
            )
            .await?;
        let batch = parse_batch_object(body.as_str())?;
        if binding.capabilities.transport == InferenceBatchTransport::JsonlFile {
            validate_alibaba_submission_ownership(&batch, submission)?;
        }
        batch_snapshot(submission, batch)
    }

    /// Collects one completed remote Batch and normalizes every single-input vector.
    /// 收集一个已完成远端 Batch 并归一化每个单输入向量。
    pub(crate) async fn collect_embedding_batch(
        &self,
        route: &ProviderExecutionRoute,
        model: &ConnectionModel,
        collection: EmbeddingBatchCollectionContext<'_>,
        secret: &CredentialExecutionSecret,
    ) -> Result<EmbeddingBatchCollectedResult, ProviderExecutionFailure> {
        let EmbeddingBatchCollectionContext {
            request,
            task,
            snapshot,
            usage_scope,
            revisions,
        } = collection;
        snapshot.validate().map_err(pre_dispatch_failure)?;
        if snapshot.state != EmbeddingBatchProviderState::Completed {
            return Err(pre_dispatch_failure(
                "Embedding Batch results can be collected only from completed tasks.",
            ));
        }
        if snapshot.submission.task_id != task.task_id {
            return Err(pre_dispatch_failure(
                "Embedding Batch snapshot does not belong to the planned task.",
            ));
        }
        let binding = embedding_batch_binding(route, model.id.as_str())?;
        let started_at = Instant::now();
        let wire_items = match binding.capabilities.result_delivery {
            InferenceBatchResultDelivery::InlineJson => {
                let endpoint = expand_endpoint_template(
                    binding.status_endpoint_path_template.as_str(),
                    "{batch_id}",
                    snapshot.submission.provider_batch_id.as_str(),
                )?;
                let body = self
                    .send_batch_request(
                        BatchHttpRequest {
                            route,
                            binding,
                            method: Method::GET,
                            endpoint: endpoint.as_str(),
                            secret,
                            maximum_response_bytes: binding
                                .capabilities
                                .host_policy_maximum_result_bytes,
                            require_json: true,
                        },
                        |builder| builder,
                    )
                    .await?;
                parse_inline_batch_result_items(&snapshot.submission, body.as_str())?
            }
            InferenceBatchResultDelivery::JsonlFiles => {
                if snapshot.failed_requests.unwrap_or_default() != 0
                    || snapshot.error_file_id.is_some()
                {
                    let mut diagnostic = failed_batch_request_diagnostic(
                        snapshot.failed_requests,
                        snapshot.total_requests,
                    );
                    let mut error_example = None;
                    if let Some(error_file_id) = snapshot.error_file_id.as_deref() {
                        let endpoint_template = binding
                            .file_content_endpoint_path_template
                            .as_deref()
                            .ok_or_else(|| {
                                pre_dispatch_failure(
                                    "Embedding Batch file-content endpoint is absent.",
                                )
                            })?;
                        let endpoint = expand_endpoint_template(
                            endpoint_template,
                            "{file_id}",
                            error_file_id,
                        )?;
                        match self
                            .send_batch_request(
                                BatchHttpRequest {
                                    route,
                                    binding,
                                    method: Method::GET,
                                    endpoint: endpoint.as_str(),
                                    secret,
                                    maximum_response_bytes: binding
                                        .capabilities
                                        .host_policy_maximum_result_bytes
                                        .min(16 * 1024 * 1024),
                                    require_json: false,
                                },
                                |builder| builder,
                            )
                            .await
                        {
                            Ok(body) => match first_batch_error_file_diagnostic(body.as_str()) {
                                Some(example) => {
                                    diagnostic.push_str(" Provider error example: ");
                                    diagnostic.push_str(example.display().as_str());
                                    error_example = Some(example);
                                }
                                None => diagnostic.push_str(
                                    " Provider error file contains no parseable item error.",
                                ),
                            },
                            Err(error) => {
                                diagnostic.push_str(" Provider error file retrieval failed: ");
                                diagnostic.push_str(
                                    bounded_batch_error_text(error.to_string().as_str()).as_str(),
                                );
                            }
                        }
                    }
                    return Err(crate::execution::sanitize_provider_execution_failure(
                        batch_failed_items_failure(diagnostic, error_example.as_ref()),
                        secret,
                    ));
                }
                let output_file_id = snapshot.output_file_id.as_deref().ok_or_else(|| {
                    response_headers_protocol_failure(
                        "Completed file-backed embedding Batch has no output file id.",
                    )
                })?;
                let endpoint_template = binding
                    .file_content_endpoint_path_template
                    .as_deref()
                    .ok_or_else(|| {
                        pre_dispatch_failure("Embedding Batch file-content endpoint is absent.")
                    })?;
                let endpoint =
                    expand_endpoint_template(endpoint_template, "{file_id}", output_file_id)?;
                let body = self
                    .send_batch_request(
                        BatchHttpRequest {
                            route,
                            binding,
                            method: Method::GET,
                            endpoint: endpoint.as_str(),
                            secret,
                            maximum_response_bytes: binding
                                .capabilities
                                .host_policy_maximum_result_bytes,
                            require_json: false,
                        },
                        |builder| builder,
                    )
                    .await?;
                parse_jsonl_result_items(body.as_str())?
            }
        };
        let bodies = map_batch_result_items(request, task, wire_items)?;
        let response = normalize_embedding_batch_bodies(
            route,
            model,
            request,
            usage_scope,
            revisions,
            bodies,
            elapsed_milliseconds(started_at),
        )?;
        Ok(EmbeddingBatchCollectedResult {
            task_id: task.task_id.clone(),
            response,
            cleanup_warning: None,
        })
    }

    /// Deletes one file-backed Batch input after the caller confirms durable result ingestion.
    /// 在调用方确认结果已持久入库后删除一个文件型 Batch 输入。
    pub(crate) async fn cleanup_embedding_batch_input(
        &self,
        route: &ProviderExecutionRoute,
        model: &ConnectionModel,
        submission: &EmbeddingBatchSubmission,
        secret: &CredentialExecutionSecret,
    ) -> Result<(), ProviderExecutionFailure> {
        submission.validate().map_err(pre_dispatch_failure)?;
        let binding = embedding_batch_binding(route, model.id.as_str())?;
        if !binding.capabilities.deletes_input_file_after_ingestion {
            return Err(pre_dispatch_failure(
                "Selected embedding Batch provider does not permit input-file cleanup.",
            ));
        }
        let file_id = submission
            .provider_input_file_id
            .as_deref()
            .ok_or_else(|| {
                pre_dispatch_failure("File-backed embedding Batch submission has no input file id.")
            })?;
        self.delete_batch_input_file(route, binding, file_id, secret)
            .await
    }

    /// Requests cancellation through the exact manifest endpoint and returns the resulting state.
    /// 通过精确 Manifest 端点请求取消并返回结果状态。
    pub(crate) async fn cancel_embedding_batch(
        &self,
        route: &ProviderExecutionRoute,
        model: &ConnectionModel,
        submission: &EmbeddingBatchSubmission,
        secret: &CredentialExecutionSecret,
    ) -> Result<EmbeddingBatchSnapshot, ProviderExecutionFailure> {
        submission.validate().map_err(pre_dispatch_failure)?;
        let binding = embedding_batch_binding(route, model.id.as_str())?;
        let template = binding
            .cancel_endpoint_path_template
            .as_deref()
            .ok_or_else(|| {
                pre_dispatch_failure(
                    "Selected embedding Batch provider does not support cancellation.",
                )
            })?;
        let endpoint = expand_endpoint_template(
            template,
            "{batch_id}",
            submission.provider_batch_id.as_str(),
        )?;
        let body = self
            .send_batch_request(
                BatchHttpRequest {
                    route,
                    binding,
                    method: Method::POST,
                    endpoint: endpoint.as_str(),
                    secret,
                    maximum_response_bytes: 16 * 1024 * 1024,
                    require_json: true,
                },
                |builder| builder,
            )
            .await?;
        let batch = parse_batch_object(body.as_str())?;
        if binding.capabilities.transport == InferenceBatchTransport::JsonlFile {
            validate_alibaba_submission_ownership(&batch, submission)?;
        }
        batch_snapshot(submission, batch)
    }

    /// Sends one bounded authenticated Batch HTTP operation.
    /// 发送一个有界且经过鉴权的 Batch HTTP 操作。
    async fn send_batch_request<F>(
        &self,
        request: BatchHttpRequest<'_>,
        configure: F,
    ) -> Result<String, ProviderExecutionFailure>
    where
        F: FnOnce(RequestBuilder) -> RequestBuilder,
    {
        let BatchHttpRequest {
            route,
            binding,
            method,
            endpoint,
            secret,
            maximum_response_bytes,
            require_json,
        } = request;
        if maximum_response_bytes > EMBEDDING_BATCH_ABSOLUTE_MAXIMUM_RESULT_BYTES {
            return Err(pre_dispatch_failure(
                "Embedding Batch response limit exceeds the process safety ceiling.",
            ));
        }
        let maximum_response_bytes = maximum_response_bytes
            .try_into()
            .map_err(|_| pre_dispatch_failure("Embedding Batch response limit exceeds usize."))?;
        let transport = self
            .protocols
            .transport_profile(route.protocol_route.protocol_profile_id.as_str())
            .map_err(|error| {
                pre_dispatch_failure(format!(
                    "Embedding Batch transport profile is unavailable: {error}"
                ))
            })?;
        // Body encoders own Content-Type, including the exact multipart boundary for file uploads.
        // 正文编码器负责 Content-Type，包含文件上传的精确 multipart 分隔边界。
        let mut headers = execution_headers(route, &BTreeMap::new(), secret, None)?;
        headers.remove(CONTENT_TYPE);
        let url = batch_request_url(route, binding, endpoint)?;
        let request = configure(self.http.request(method, url.as_str()).headers(headers));
        let response = tokio::time::timeout(
            Duration::from_millis(transport.response_headers_timeout_ms),
            request.send(),
        )
        .await
        .map_err(|_| {
            transport_failure(format!(
                "Embedding Batch request to `{url}` exceeded the registered {} ms response-header timeout.",
                transport.response_headers_timeout_ms
            ))
        })?
        .map_err(|error| {
            http_send_transport_failure(
                &error,
                format!("Embedding Batch request to `{url}` failed: {error}"),
            )
        })?;
        let status = response.status().as_u16();
        let retry_after = parse_retry_after_headers(response.headers(), SystemTime::now());
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = read_bounded_response_text(
            response,
            maximum_response_bytes,
            url.as_str(),
            status,
            retry_after.clone(),
            Duration::from_millis(transport.stream_idle_timeout_ms),
        )
        .await?;
        if !(200..300).contains(&status) {
            return Err(crate::execution::sanitize_provider_execution_failure(
                http_failure(status, retry_after, body.as_str()),
                secret,
            ));
        }
        if require_json
            && !content_type
                .as_deref()
                .is_some_and(|value| value.to_ascii_lowercase().starts_with("application/json"))
        {
            return Err(response_headers_protocol_failure(format!(
                "Embedding Batch response from `{url}` used unsupported Content-Type `{}`.",
                content_type.as_deref().unwrap_or("<missing>")
            )));
        }
        Ok(body)
    }

    /// Deletes one temporary provider input file after durable vector ingestion.
    /// 在向量持久入库后删除一个临时供应商输入文件。
    async fn delete_batch_input_file(
        &self,
        route: &ProviderExecutionRoute,
        binding: &NativeInferenceBatchBinding,
        file_id: &str,
        secret: &CredentialExecutionSecret,
    ) -> Result<(), ProviderExecutionFailure> {
        let template = binding
            .file_delete_endpoint_path_template
            .as_deref()
            .ok_or_else(|| {
                pre_dispatch_failure("Embedding Batch file-delete endpoint is absent.")
            })?;
        let endpoint = expand_endpoint_template(template, "{file_id}", file_id)?;
        self.send_batch_request(
            BatchHttpRequest {
                route,
                binding,
                method: Method::DELETE,
                endpoint: endpoint.as_str(),
                secret,
                maximum_response_bytes: 4 * 1024 * 1024,
                require_json: false,
            },
            |builder| builder,
        )
        .await?;
        Ok(())
    }
}

/// Resolves an exact model-scoped embedding binding with a verified Batch declaration.
/// 解析一个带已验证 Batch 声明的精确模型作用域向量绑定。
fn embedding_batch_binding<'a>(
    route: &'a ProviderExecutionRoute,
    model_id: &str,
) -> Result<&'a NativeInferenceBatchBinding, ProviderExecutionFailure> {
    select_native_batch_binding(
        &route.entry.native_inference_bindings,
        AiOperation::EmbeddingCreate,
        model_id,
    )
    .and_then(|binding| binding.batch.as_ref())
    .ok_or_else(|| {
        pre_dispatch_failure(format!(
            "Model `{model_id}` does not expose a verified asynchronous embedding Batch binding."
        ))
    })
}

/// Returns the stable action binding selected beside the Batch declaration.
/// 返回在 Batch 声明旁选择的稳定动作绑定标识。
fn binding_id<'a>(
    route: &'a ProviderExecutionRoute,
    model_id: &str,
) -> Result<&'a str, ProviderExecutionFailure> {
    select_native_batch_binding(
        &route.entry.native_inference_bindings,
        AiOperation::EmbeddingCreate,
        model_id,
    )
    .map(|binding| binding.binding_id.as_str())
    .ok_or_else(|| pre_dispatch_failure("Native embedding Batch action binding is unavailable."))
}

/// Prepares the complete deterministic task partition without provider I/O.
/// 在不执行供应商 I/O 的情况下准备完整确定性任务分区。
fn prepare_embedding_batch_tasks(
    model: &ConnectionModel,
    request: &EmbeddingRequest,
    binding: &NativeInferenceBatchBinding,
) -> Result<Vec<EmbeddingBatchTaskPlan>, ProviderExecutionFailure> {
    if request.encoding != EmbeddingEncoding::Float {
        return Err(pre_dispatch_failure(
            "Asynchronous embedding Batch execution currently requires float output.",
        ));
    }
    let maximum_requests = usize::try_from(binding.capabilities.effective_maximum_requests())
        .map_err(|_| pre_dispatch_failure("Embedding Batch request limit exceeds usize."))?;
    let maximum_tasks = usize::try_from(binding.capabilities.effective_maximum_tasks_per_run())
        .map_err(|_| pre_dispatch_failure("Embedding Batch task limit exceeds usize."))?;
    let maximum_payload = binding.capabilities.effective_maximum_payload_bytes();
    let mut tasks = Vec::new();
    let mut encoded_task_items = Vec::new();
    let mut task_first_input_index = 0_usize;
    let mut payload_bytes = base_payload_bytes(binding, model.id.as_str())?;
    for (input_index, input) in request.inputs.iter().enumerate() {
        let encoded = encode_batch_item(model, request, binding, input_index, input)?;
        let item_bytes = u64::try_from(encoded.len())
            .map_err(|_| pre_dispatch_failure("Embedding Batch item length exceeds u64."))?;
        if let Some(maximum) = binding.capabilities.provider_maximum_item_bytes
            && item_bytes > maximum
        {
            return Err(pre_dispatch_failure(format!(
                "Embedding Batch item {input_index} is {item_bytes} bytes and exceeds the provider item-size limit of {maximum} bytes."
            )));
        }
        let delimiter_bytes = match binding.capabilities.transport {
            InferenceBatchTransport::InlineJson if !encoded_task_items.is_empty() => 1,
            InferenceBatchTransport::InlineJson => 0,
            InferenceBatchTransport::JsonlFile => 1,
        };
        let candidate_bytes = payload_bytes
            .checked_add(item_bytes)
            .and_then(|value| value.checked_add(delimiter_bytes))
            .ok_or_else(|| pre_dispatch_failure("Embedding Batch payload length overflowed."))?;
        if encoded_task_items.len() == maximum_requests || candidate_bytes > maximum_payload {
            if encoded_task_items.is_empty() {
                return Err(pre_dispatch_failure(format!(
                    "Embedding Batch item {input_index} produces a {candidate_bytes}-byte payload and exceeds the effective {maximum_payload}-byte {} limit.",
                    effective_payload_limit_source(&binding.capabilities)
                )));
            }
            tasks.push(
                build_prepared_task(
                    request,
                    model.id.as_str(),
                    binding,
                    &encoded_task_items,
                    task_first_input_index,
                )?
                .plan,
            );
            if tasks.len() >= maximum_tasks {
                return Err(pre_dispatch_failure(format!(
                    "Embedding Batch plan requires more than {maximum_tasks} provider tasks."
                )));
            }
            encoded_task_items.clear();
            task_first_input_index = input_index;
            payload_bytes = base_payload_bytes(binding, model.id.as_str())?;
            let first_delimiter_bytes = match binding.capabilities.transport {
                InferenceBatchTransport::InlineJson => 0,
                InferenceBatchTransport::JsonlFile => 1,
            };
            payload_bytes = payload_bytes
                .checked_add(item_bytes)
                .and_then(|value| value.checked_add(first_delimiter_bytes))
                .ok_or_else(|| {
                    pre_dispatch_failure("Embedding Batch payload length overflowed.")
                })?;
            if payload_bytes > maximum_payload {
                return Err(pre_dispatch_failure(format!(
                    "Embedding Batch item {input_index} produces a {payload_bytes}-byte payload and exceeds the effective {maximum_payload}-byte {} limit.",
                    effective_payload_limit_source(&binding.capabilities)
                )));
            }
        } else {
            payload_bytes = candidate_bytes;
        }
        encoded_task_items.push(encoded);
    }
    if !encoded_task_items.is_empty() {
        tasks.push(
            build_prepared_task(
                request,
                model.id.as_str(),
                binding,
                &encoded_task_items,
                task_first_input_index,
            )?
            .plan,
        );
    }
    if tasks.len() > maximum_tasks {
        return Err(pre_dispatch_failure(format!(
            "Embedding Batch plan requires more than {maximum_tasks} provider tasks."
        )));
    }
    Ok(tasks)
}

/// Recreates and verifies exactly one planned task before dispatch.
/// 在分派前重新创建并校验一个精确规划任务。
fn prepare_exact_embedding_batch_task(
    model: &ConnectionModel,
    request: &EmbeddingRequest,
    binding: &NativeInferenceBatchBinding,
    expected: &EmbeddingBatchTaskPlan,
) -> Result<PreparedEmbeddingBatchTask, ProviderExecutionFailure> {
    expected.validate().map_err(pre_dispatch_failure)?;
    if expected.input_count > binding.capabilities.effective_maximum_requests() {
        return Err(pre_dispatch_failure(
            "Embedding Batch task exceeds the effective request-count limit.",
        ));
    }
    let start = usize::try_from(expected.first_input_index)
        .map_err(|_| pre_dispatch_failure("Embedding Batch task offset exceeds usize."))?;
    let end = usize::try_from(expected.end_input_index().map_err(pre_dispatch_failure)?)
        .map_err(|_| pre_dispatch_failure("Embedding Batch task end exceeds usize."))?;
    let inputs = request.inputs.get(start..end).ok_or_else(|| {
        pre_dispatch_failure("Embedding Batch task range exceeds the frozen request inputs.")
    })?;
    let encoded_items = inputs
        .iter()
        .enumerate()
        .map(|(relative_index, input)| {
            encode_batch_item(model, request, binding, start + relative_index, input)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(maximum_item_bytes) = binding.capabilities.provider_maximum_item_bytes {
        for (relative_index, item) in encoded_items.iter().enumerate() {
            let item_bytes = u64::try_from(item.len())
                .map_err(|_| pre_dispatch_failure("Embedding Batch item length exceeds u64."))?;
            if item_bytes > maximum_item_bytes {
                return Err(pre_dispatch_failure(format!(
                    "Embedding Batch item {} is {item_bytes} bytes and exceeds the provider item-size limit of {maximum_item_bytes} bytes.",
                    start + relative_index,
                )));
            }
        }
    }
    let prepared = build_prepared_task(request, model.id.as_str(), binding, &encoded_items, start)?;
    if prepared.plan.encoded_payload_bytes > binding.capabilities.effective_maximum_payload_bytes()
    {
        let maximum_payload = binding.capabilities.effective_maximum_payload_bytes();
        return Err(pre_dispatch_failure(format!(
            "Embedding Batch task payload is {} bytes and exceeds the effective {maximum_payload}-byte {} limit.",
            prepared.plan.encoded_payload_bytes,
            effective_payload_limit_source(&binding.capabilities)
        )));
    }
    if prepared.plan != *expected {
        return Err(pre_dispatch_failure(
            "Embedding Batch payload reconstruction differs from its deterministic plan.",
        ));
    }
    Ok(prepared)
}

/// Describes which declared bound determines the effective complete-payload limit.
/// 描述哪一项声明边界决定有效完整负载上限。
///
/// # Parameters
/// 参数
/// `capabilities` contains the frozen provider and host-policy payload limits.
/// `capabilities` 包含冻结的供应商与宿主策略负载上限。
///
/// # Returns
/// 返回值
/// A stable diagnostic label naming the active limit source.
/// 返回命名当前生效限制来源的稳定诊断标签。
fn effective_payload_limit_source(capabilities: &EmbeddingBatchCapabilities) -> &'static str {
    match capabilities.provider_maximum_payload_bytes {
        Some(provider) if provider < capabilities.host_policy_maximum_payload_bytes => "provider",
        Some(provider) if provider == capabilities.host_policy_maximum_payload_bytes => {
            "provider and host-policy"
        }
        _ => "host-policy",
    }
}

/// Encodes one provider-specific single-input request item.
/// 编码一个供应商特定的单输入请求项。
fn encode_batch_item(
    model: &ConnectionModel,
    request: &EmbeddingRequest,
    binding: &NativeInferenceBatchBinding,
    input_index: usize,
    input: &vulcan_ai_contract::EmbeddingInput,
) -> Result<Vec<u8>, ProviderExecutionFailure> {
    let text = match &input.content {
        EmbeddingInputContent::Text { text } => text.as_str(),
        EmbeddingInputContent::Resource { .. } => {
            return Err(pre_dispatch_failure(
                "Asynchronous embedding Batch execution accepts text inputs only.",
            ));
        }
    };
    let custom_id = batch_custom_id(
        request.request_id.as_str(),
        input.input_id.as_str(),
        input_index,
    );
    match binding.protocol {
        NativeInferenceBatchProtocolKind::OpenRouterBetaV1 => {
            serde_json::to_vec(&OpenRouterBatchRequestItem {
                custom_id,
                body: CompatibleEmbeddingBatchBody {
                    model: model.id.as_str(),
                    input: text,
                    dimensions: request.dimensions,
                    encoding_format: "float",
                },
            })
        }
        NativeInferenceBatchProtocolKind::AlibabaOpenAiCompatibleV1 => {
            serde_json::to_vec(&AlibabaBatchRequestItem {
                custom_id,
                method: "POST",
                url: binding.item_endpoint_path.as_str(),
                body: AlibabaEmbeddingBatchBody {
                    model: model.id.as_str(),
                    input: text,
                    dimensions: request.dimensions,
                    encoding_format: "float",
                },
            })
        }
    }
    .map_err(|error| {
        pre_dispatch_failure(format!(
            "Embedding Batch request item could not be encoded: {error}"
        ))
    })
}

/// Returns the exact fixed payload bytes outside serialized request items.
/// 返回序列化请求项之外的精确固定负载字节数。
fn base_payload_bytes(
    binding: &NativeInferenceBatchBinding,
    model_id: &str,
) -> Result<u64, ProviderExecutionFailure> {
    match binding.protocol {
        NativeInferenceBatchProtocolKind::OpenRouterBetaV1 => {
            let prefix = openrouter_payload_prefix(binding.item_endpoint_path.as_str(), model_id)?;
            u64::try_from(prefix.len() + 2)
                .map_err(|_| pre_dispatch_failure("Embedding Batch payload base exceeds u64."))
        }
        NativeInferenceBatchProtocolKind::AlibabaOpenAiCompatibleV1 => Ok(0),
    }
}

/// Builds one exact task payload and its stable content-addressed plan.
/// 构建一个精确任务负载及其稳定内容寻址计划。
fn build_prepared_task(
    request: &EmbeddingRequest,
    model_id: &str,
    binding: &NativeInferenceBatchBinding,
    encoded_items: &[Vec<u8>],
    first_input_index: usize,
) -> Result<PreparedEmbeddingBatchTask, ProviderExecutionFailure> {
    let mut payload = Vec::new();
    match binding.protocol {
        NativeInferenceBatchProtocolKind::OpenRouterBetaV1 => {
            payload.extend_from_slice(
                openrouter_payload_prefix(binding.item_endpoint_path.as_str(), model_id)?
                    .as_bytes(),
            );
            for (relative_index, item) in encoded_items.iter().enumerate() {
                if relative_index != 0 {
                    payload.push(b',');
                }
                payload.extend_from_slice(item);
            }
            payload.extend_from_slice(b"]}");
        }
        NativeInferenceBatchProtocolKind::AlibabaOpenAiCompatibleV1 => {
            for item in encoded_items {
                payload.extend_from_slice(item);
                payload.push(b'\n');
            }
        }
    }
    let payload_sha256 = hex::encode(Sha256::digest(&payload));
    let input_count = u32::try_from(encoded_items.len())
        .map_err(|_| pre_dispatch_failure("Embedding Batch task count exceeds u32."))?;
    let first_input_index = u32::try_from(first_input_index)
        .map_err(|_| pre_dispatch_failure("Embedding Batch task offset exceeds u32."))?;
    let task_id = format!(
        "batch-{}",
        hex::encode(Sha256::digest(format!(
            "{}\n{}\n{}\n{}",
            request.request_id, first_input_index, input_count, payload_sha256
        )))
    );
    let encoded_payload_bytes = u64::try_from(payload.len())
        .map_err(|_| pre_dispatch_failure("Embedding Batch payload length exceeds u64."))?;
    Ok(PreparedEmbeddingBatchTask {
        plan: EmbeddingBatchTaskPlan {
            task_id,
            first_input_index,
            input_count,
            encoded_payload_bytes,
            payload_sha256,
        },
        payload,
    })
}

/// Creates the exact ordered OpenRouter JSON prefix required by its streaming parser.
/// 创建 OpenRouter 流式解析器要求的精确有序 JSON 前缀。
fn openrouter_payload_prefix(
    endpoint: &str,
    model_id: &str,
) -> Result<String, ProviderExecutionFailure> {
    let endpoint = serde_json::to_string(endpoint).map_err(|error| {
        pre_dispatch_failure(format!("Embedding Batch endpoint encoding failed: {error}"))
    })?;
    let model = serde_json::to_string(model_id).map_err(|error| {
        pre_dispatch_failure(format!("Embedding Batch model encoding failed: {error}"))
    })?;
    Ok(format!(
        "{{\"endpoint\":{endpoint},\"model\":{model},\"requests\":["
    ))
}

/// Derives one collision-resistant custom id from immutable input identity.
/// 从不可变输入身份派生一个抗碰撞 custom id。
fn batch_custom_id(request_id: &str, input_id: &str, input_index: usize) -> String {
    format!(
        "vce-{}",
        hex::encode(Sha256::digest(format!(
            "{request_id}\n{input_index}\n{input_id}"
        )))
    )
}

/// Resolves the immutable Batch route identity without materializing provider task payloads.
/// 在不实例化供应商任务负载的情况下解析不可变 Batch 路由身份。
///
/// # Parameters
/// 参数
/// `route`, `model`, `request`, `usage_scope`, and `revisions` are the exact current dispatch facts.
/// `route`、`model`、`request`、`usage_scope` 与 `revisions` 是当前精确分派事实。
///
/// # Returns
/// 返回值
/// The selected route snapshot, frozen capabilities, and deterministic request fingerprint.
/// 返回所选路由快照、冻结能力及确定性请求指纹。
fn embedding_batch_identity(
    route: &ProviderExecutionRoute,
    model: &ConnectionModel,
    request: &EmbeddingRequest,
    usage_scope: InferenceUsageScope,
    revisions: ExecutionRevisionRefs<'_>,
) -> Result<
    (
        vulcan_ai_contract::ExecutionRouteSnapshot,
        EmbeddingBatchCapabilities,
        String,
    ),
    ProviderExecutionFailure,
> {
    request.validate().map_err(pre_dispatch_failure)?;
    model
        .capabilities
        .embedding
        .as_ref()
        .ok_or_else(|| {
            pre_dispatch_failure(format!(
                "Model `{}` does not expose an embedding contract.",
                model.id
            ))
        })?
        .validate_batch_request(request)
        .map_err(pre_dispatch_failure)?;
    let binding = embedding_batch_binding(route, model.id.as_str())?;
    let route_snapshot = super::native_inference::execution_route_snapshot(
        route,
        model,
        usage_scope,
        revisions.catalog,
        revisions.route,
        Some(binding_id(route, model.id.as_str())?),
    );
    let request_fingerprint = embedding_batch_request_fingerprint(request, &route_snapshot)?;
    Ok((
        route_snapshot,
        binding.capabilities.clone(),
        request_fingerprint,
    ))
}

/// Hashes the immutable typed request beside its exact selected route.
/// 对不可变类型化请求及其精确所选路由进行哈希。
fn embedding_batch_request_fingerprint(
    request: &EmbeddingRequest,
    route: &vulcan_ai_contract::ExecutionRouteSnapshot,
) -> Result<String, ProviderExecutionFailure> {
    let mut writer = Sha256Writer(Sha256::new());
    serde_json::to_writer(&mut writer, &(request, route)).map_err(|error| {
        pre_dispatch_failure(format!(
            "Embedding Batch request fingerprint encoding failed: {error}"
        ))
    })?;
    Ok(hex::encode(writer.0.finalize()))
}

/// Builds a provider URL from the manifest-owned Batch API root and endpoint.
/// 从 Manifest 持有的 Batch API 根和端点构建供应商 URL。
fn batch_request_url(
    route: &ProviderExecutionRoute,
    binding: &NativeInferenceBatchBinding,
    endpoint: &str,
) -> Result<String, ProviderExecutionFailure> {
    let endpoint_path = endpoint.split_once('?').map_or(endpoint, |(path, _)| path);
    if !endpoint_path.starts_with('/')
        || endpoint_path.starts_with("//")
        || endpoint.contains('#')
        || endpoint_path.contains('\\')
        || endpoint_path.chars().any(char::is_whitespace)
        || endpoint_path
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
    {
        return Err(pre_dispatch_failure(
            "Embedding Batch endpoint must stay within the declared service root.",
        ));
    }
    let base_url = binding.base_url_override.clone().unwrap_or_else(|| {
        crate::connection_endpoint::resolved_connection_protocol_base_url(
            &route.connection,
            &route.entry,
            &route.protocol_route,
        )
    });
    let url = format!("{}{}", base_url.trim_end_matches('/'), endpoint);
    let parsed = reqwest::Url::parse(url.as_str()).map_err(|error| {
        pre_dispatch_failure(format!("Embedding Batch URL is invalid: {error}"))
    })?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err(pre_dispatch_failure(
            "Embedding Batch URL must be an absolute HTTPS URL.",
        ));
    }
    Ok(url)
}

/// Expands one exact manifest endpoint template with a provider identifier.
/// 使用供应商标识展开一个精确 Manifest 端点模板。
fn expand_endpoint_template(
    template: &str,
    placeholder: &str,
    value: &str,
) -> Result<String, ProviderExecutionFailure> {
    if value.trim().is_empty() || value.contains('/') || !template.contains(placeholder) {
        return Err(pre_dispatch_failure(
            "Embedding Batch endpoint template or provider identifier is invalid.",
        ));
    }
    Ok(template.replace(placeholder, value))
}

/// Parses and validates one common provider Batch object.
/// 解析并校验一个通用供应商 Batch 对象。
fn parse_batch_object(body: &str) -> Result<WireBatchObject, ProviderExecutionFailure> {
    let batch: WireBatchObject = serde_json::from_str(body).map_err(|error| {
        response_headers_protocol_failure(format!(
            "Embedding Batch response is invalid JSON: {error}"
        ))
    })?;
    if batch.id.trim().is_empty() {
        return Err(response_headers_protocol_failure(
            "Embedding Batch response has an empty provider id.",
        ));
    }
    normalize_batch_state(batch.status.as_str())?;
    Ok(batch)
}

/// Validates the Alibaba file and caller metadata echoed by one Batch object.
/// 校验阿里云 Batch 对象回显的文件身份与调用方元数据。
///
/// # Parameters
/// 参数
/// `batch` is the provider response, while `task_id` and `input_file_id` are the frozen local identities.
/// `batch` 是供应商响应，`task_id` 与 `input_file_id` 是冻结的本地身份。
///
/// # Returns
/// 返回值
/// Unit only when the provider response belongs to the exact submitted task and input file.
/// 仅当供应商响应精确属于已提交任务及输入文件时返回空值。
fn validate_alibaba_batch_ownership(
    batch: &WireBatchObject,
    task_id: &str,
    input_file_id: &str,
) -> Result<(), ProviderExecutionFailure> {
    if batch.input_file_id.as_deref() != Some(input_file_id) {
        return Err(response_headers_protocol_failure(
            "Alibaba embedding Batch response belongs to a different input file.",
        ));
    }
    if batch
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("ds_name"))
        .and_then(serde_json::Value::as_str)
        != Some(task_id)
    {
        return Err(response_headers_protocol_failure(
            "Alibaba embedding Batch response belongs to a different task.",
        ));
    }
    Ok(())
}

/// Validates one Alibaba Batch object against a durable local submission.
/// 根据持久化本地提交记录校验一个阿里云 Batch 对象。
///
/// # Parameters
/// 参数
/// `batch` is the current provider response and `submission` is the frozen local identity.
/// `batch` 是当前供应商响应，`submission` 是冻结的本地身份。
///
/// # Returns
/// 返回值
/// Unit when the provider echoes the exact file and task identities.
/// 当供应商精确回显文件及任务身份时返回空值。
fn validate_alibaba_submission_ownership(
    batch: &WireBatchObject,
    submission: &EmbeddingBatchSubmission,
) -> Result<(), ProviderExecutionFailure> {
    let input_file_id = submission
        .provider_input_file_id
        .as_deref()
        .ok_or_else(|| {
            pre_dispatch_failure(
                "File-backed embedding Batch submission has no durable input file id.",
            )
        })?;
    validate_alibaba_batch_ownership(batch, submission.task_id.as_str(), input_file_id)
}

/// Projects one provider Batch object into the durable provider-neutral snapshot.
/// 将一个供应商 Batch 对象投影为持久且供应商无关的快照。
fn batch_snapshot(
    submission: &EmbeddingBatchSubmission,
    batch: WireBatchObject,
) -> Result<EmbeddingBatchSnapshot, ProviderExecutionFailure> {
    if batch.id != submission.provider_batch_id {
        return Err(response_headers_protocol_failure(
            "Embedding Batch status belongs to a different provider id.",
        ));
    }
    let state = normalize_batch_state(batch.status.as_str())?;
    let (total_requests, completed_requests, failed_requests) =
        batch.request_counts.map_or((None, None, None), |counts| {
            (
                Some(counts.total),
                Some(counts.completed),
                Some(counts.failed),
            )
        });
    let snapshot = EmbeddingBatchSnapshot {
        submission: submission.clone(),
        state,
        total_requests,
        completed_requests,
        failed_requests,
        output_file_id: batch.output_file_id,
        error_file_id: batch.error_file_id,
        error: batch
            .errors
            .map(|_| String::from("provider Batch reported terminal errors")),
    };
    snapshot
        .validate()
        .map_err(response_headers_protocol_failure)?;
    Ok(snapshot)
}

/// Parses inline results only after revalidating the exact provider task identity and terminal state.
/// 仅在重新校验精确供应商任务身份及终态后解析内嵌结果。
///
/// # Parameters
/// 参数
/// `submission` is the durable expected task identity and `body` is the bounded status response.
/// `submission` 是持久化的预期任务身份，`body` 是有界状态响应。
///
/// # Returns
/// 返回值
/// The complete inline item set, or a closed protocol failure before result projection.
/// 返回完整内嵌条目集合；若协议不闭合则在结果投影前失败。
fn parse_inline_batch_result_items(
    submission: &EmbeddingBatchSubmission,
    body: &str,
) -> Result<Vec<WireBatchResultItem>, ProviderExecutionFailure> {
    let mut batch = parse_batch_object(body)?;
    let results = batch.results.take();
    let snapshot = batch_snapshot(submission, batch)?;
    if snapshot.state != EmbeddingBatchProviderState::Completed {
        return Err(response_headers_protocol_failure(
            "Embedding Batch left completed state before collection.",
        ));
    }
    results.ok_or_else(|| {
        response_headers_protocol_failure("Completed inline embedding Batch has no result items.")
    })
}

/// Maps a provider status string into the closed shared lifecycle.
/// 将供应商状态字符串映射到封闭共享生命周期。
fn normalize_batch_state(
    status: &str,
) -> Result<EmbeddingBatchProviderState, ProviderExecutionFailure> {
    match status {
        "validating" => Ok(EmbeddingBatchProviderState::Validating),
        "in_progress" => Ok(EmbeddingBatchProviderState::InProgress),
        "finalizing" => Ok(EmbeddingBatchProviderState::Finalizing),
        "completed" => Ok(EmbeddingBatchProviderState::Completed),
        "failed" => Ok(EmbeddingBatchProviderState::Failed),
        "expired" => Ok(EmbeddingBatchProviderState::Expired),
        "cancelling" => Ok(EmbeddingBatchProviderState::Cancelling),
        "cancelled" => Ok(EmbeddingBatchProviderState::Cancelled),
        _ => Err(response_headers_protocol_failure(format!(
            "Embedding Batch returned unknown status `{status}`."
        ))),
    }
}

/// Parses a bounded Alibaba output file as one JSON object per non-empty line.
/// 将有界阿里云输出文件按每个非空行一个 JSON 对象解析。
fn parse_jsonl_result_items(
    body: &str,
) -> Result<Vec<WireBatchResultItem>, ProviderExecutionFailure> {
    body.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(line_index, line)| {
            serde_json::from_str(line).map_err(|error| {
                response_headers_protocol_failure(format!(
                    "Embedding Batch result JSONL line {} is invalid: {error}",
                    line_index + 1
                ))
            })
        })
        .collect()
}

/// Maps exact custom ids back to original indexes and rejects partial or duplicate output.
/// 将精确 custom id 映射回原始索引，并拒绝部分或重复输出。
fn map_batch_result_items(
    request: &EmbeddingRequest,
    task: &EmbeddingBatchTaskPlan,
    wire_items: Vec<WireBatchResultItem>,
) -> Result<Vec<(u32, serde_json::Value)>, ProviderExecutionFailure> {
    let start = usize::try_from(task.first_input_index)
        .map_err(|_| response_headers_protocol_failure("Batch task offset exceeds usize."))?;
    let end = usize::try_from(task.end_input_index().map_err(pre_dispatch_failure)?)
        .map_err(|_| response_headers_protocol_failure("Batch task end exceeds usize."))?;
    let mut expected = BTreeMap::new();
    let task_inputs = request.inputs.get(start..end).ok_or_else(|| {
        response_headers_protocol_failure(
            "Embedding Batch task range exceeds the immutable typed request.",
        )
    })?;
    for (input_index, input) in task_inputs.iter().enumerate() {
        let original_index = start + input_index;
        expected.insert(
            batch_custom_id(
                request.request_id.as_str(),
                input.input_id.as_str(),
                original_index,
            ),
            u32::try_from(original_index).map_err(|_| {
                response_headers_protocol_failure("Embedding Batch index exceeds u32.")
            })?,
        );
    }
    if wire_items.len() != expected.len() {
        return Err(response_headers_protocol_failure(format!(
            "Embedding Batch returned {} result items for {} inputs.",
            wire_items.len(),
            expected.len()
        )));
    }
    let mut bodies = Vec::with_capacity(wire_items.len());
    for item in wire_items {
        let original_index = expected.remove(item.custom_id.as_str()).ok_or_else(|| {
            response_headers_protocol_failure(format!(
                "Embedding Batch returned unknown or duplicate custom id `{}`.",
                item.custom_id
            ))
        })?;
        if item.error.is_some() {
            return Err(response_headers_protocol_failure(format!(
                "Embedding Batch item `{}` reported a provider error.",
                item.custom_id
            )));
        }
        let response = item.response.ok_or_else(|| {
            response_headers_protocol_failure(format!(
                "Embedding Batch item `{}` has neither response nor error.",
                item.custom_id
            ))
        })?;
        if !(200..300).contains(&response.status_code) {
            return Err(response_headers_protocol_failure(format!(
                "Embedding Batch item `{}` returned HTTP {}.",
                item.custom_id, response.status_code
            )));
        }
        bodies.push((original_index, response.body));
    }
    if !expected.is_empty() {
        return Err(response_headers_protocol_failure(
            "Embedding Batch result omitted one or more custom ids.",
        ));
    }
    bodies.sort_by_key(|(index, _)| *index);
    Ok(bodies)
}

/// Extracts one bounded provider error example without retaining a custom id or request input.
/// 提取一条有界供应商错误示例，不保留自定义标识或请求输入。
/// `body` is the provider error JSONL; returns its first structured error code and message.
/// `body` 是供应商错误 JSONL；返回首条结构化错误码和消息。
fn first_batch_error_file_diagnostic(body: &str) -> Option<BatchErrorExample> {
    let error = body
        .lines()
        .filter(|line| !line.trim().is_empty())
        .find_map(|line| {
            serde_json::from_str::<WireBatchResultItem>(line)
                .ok()
                .and_then(|item| item.error)
        })?;
    let code = error.get("code").and_then(serde_json::Value::as_str);
    let message = error.get("message").and_then(serde_json::Value::as_str);
    if code.is_none() && message.is_none() {
        return None;
    }
    Some(BatchErrorExample {
        code: code.map(bounded_batch_error_text),
        message: message.map(bounded_batch_error_text),
    })
}

impl BatchErrorExample {
    /// Format one provider-owned example for a bounded user-facing diagnostic.
    /// 将一条供应商错误示例格式化为有界的用户可见诊断。
    /// Returns the exact available code and message without item identity or request text.
    /// 返回可用的错误码和消息，不包含条目标识或请求正文。
    fn display(&self) -> String {
        match (&self.code, &self.message) {
            (Some(code), Some(message)) => format!("{code}: {message}"),
            (Some(code), None) => code.clone(),
            (None, Some(message)) => message.clone(),
            (None, None) => String::new(),
        }
    }
}

/// Classify an incomplete provider Batch without inventing a 200 status or blaming user input.
/// 对不完整的供应商批次分类，不虚构 200 状态或归咎于用户输入。
/// Partial successful items make replaying the whole Batch unsafe even when one item has a 500 error.
/// 即使单项报 500，部分条目已成功也使整个批次无法安全重放。
fn batch_failed_items_failure(
    message: String,
    example: Option<&BatchErrorExample>,
) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::Unclassified,
            subcategory: Some(String::from("batch_item_failure")),
            stage: String::from("batch_collect"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: example.and_then(|item| item.code.clone()),
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: example.and_then(|item| item.message.clone()),
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message,
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Bounds and flattens one provider diagnostic before persisting it in a VCE job error.
/// 在持久化到 VCE 作业错误前限制并压平一条供应商诊断。
/// `value` may come from a provider error file or a failed error-file download.
/// `value` 可来自供应商错误文件或失败的错误文件下载。
fn bounded_batch_error_text(value: &str) -> String {
    value
        .chars()
        .take(320)
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// Describes a completed provider Batch whose item failures prevent complete vector ingestion.
/// 描述已完成但存在失败条目、因而无法完整写入向量的供应商批次。
/// Counts are provider-reported and may be absent when the provider omits metadata.
/// 计数来自供应商；供应商省略元数据时计数可能缺失。
fn failed_batch_request_diagnostic(failed: Option<u32>, total: Option<u32>) -> String {
    match (failed, total) {
        (Some(failed), Some(total)) => {
            format!("Completed embedding Batch contains {failed}/{total} failed request items.")
        }
        (Some(failed), None) => {
            format!("Completed embedding Batch contains {failed} failed request items.")
        }
        _ => String::from("Completed embedding Batch contains failed request items."),
    }
}

/// Returns current Unix time in milliseconds for durable submission ordering.
/// 返回当前 Unix 毫秒时间用于持久提交排序。
fn current_unix_millis() -> Result<u64, ProviderExecutionFailure> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            pre_dispatch_failure(format!("System clock precedes Unix epoch: {error}"))
        })?;
    u64::try_from(duration.as_millis())
        .map_err(|_| pre_dispatch_failure("Unix millisecond timestamp exceeds u64."))
}

/// Converts elapsed monotonic duration to a saturated public millisecond count.
/// 将单调时钟耗时转换为饱和的公开毫秒计数。
fn elapsed_milliseconds(started_at: Instant) -> u64 {
    u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModelCapabilityEvidence;
    use std::collections::BTreeSet;
    use vulcan_ai_contract::{
        DistanceMetric, EmbeddingCapabilities, EmbeddingInput, EmbeddingInvalidInputPolicy,
        EmbeddingNormalization, EmbeddingOutputKind, EmbeddingTask, ModelCapabilities,
    };

    /// Verify the production Batch sender uses one encoder-owned Content-Type for multipart and JSON.
    /// 验证生产 Batch 发送器对 multipart 及 JSON 使用唯一的编码器所属 Content-Type。
    /// No parameters; a loopback server checks headers, boundaries and payload bytes without provider calls.
    /// 无参数；回环服务校验请求头、分隔符及正文，不调用供应商。
    #[tokio::test]
    async fn batch_sender_preserves_multipart_boundary_and_json_content_type() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        // Synthetic secret and manifest-backed route exercise the real shared header construction.
        // 模拟密钥及清单路由经过真实共享请求头构造。
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("fixture-key"))
            .unwrap()
            .execution_secret()
            .unwrap();
        let route = crate::execution::test_support::bundled_manifest_route(
            "alibaba",
            "model-studio-singapore",
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
        );
        let binding = test_binding(InferenceBatchTransport::JsonlFile, 10, 1_048_576, None);
        let http = reqwest::Client::builder().no_proxy().build().unwrap();
        let client = ProviderExecutionClient::new(
            http.clone(),
            std::sync::Arc::new(vulcan_ai_protocols::ProtocolRegistry::first_batch().unwrap()),
        );
        for multipart in [true, false] {
            // Bind an ephemeral loopback endpoint and retain its exact address for this request only.
            // 绑定临时回环端点，仅为本次请求保留精确地址。
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                // Read bounded raw wire bytes so duplicate headers cannot be hidden by a HeaderMap.
                // 读取有界线缆字节，避免 HeaderMap 隐藏重复请求头。
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut chunk = [0_u8; 4096];
                loop {
                    let count = stream.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&chunk[..count]);
                    assert!(bytes.len() < 16_384);
                    if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                        let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().unwrap())
                            })
                            .unwrap();
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                // Capture before replying; assertions below verify the actual emitted MIME contract.
                // 回复前捕获字节；后续断言校验实际发出的 MIME 契约。
                let captured = String::from_utf8(bytes).unwrap();
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
                captured
            });
            client
                .send_batch_request(
                    BatchHttpRequest {
                        route: &route,
                        binding: &binding,
                        method: Method::POST,
                        endpoint: "/v1/files",
                        secret: &secret,
                        maximum_response_bytes: 1024,
                        require_json: true,
                    },
                    |builder| {
                        // Both encoders use the real sender's authenticated headers; only the test destination changes.
                        // 两个编码器使用真实发送器的鉴权请求头；仅测试目标地址被替换。
                        let builder = if multipart {
                            builder.multipart(
                                reqwest::multipart::Form::new()
                                    .text("purpose", "batch")
                                    .part(
                                        "file",
                                        reqwest::multipart::Part::bytes(
                                            b"{\"input\":\"fixture\"}\n".to_vec(),
                                        )
                                        .file_name("fixture.jsonl")
                                        .mime_str("application/jsonl")
                                        .unwrap(),
                                    ),
                            )
                        } else {
                            builder.json(&serde_json::json!({"input_file_id": "file-fixture"}))
                        };
                        let mut request = builder.build().unwrap();
                        *request.url_mut() = format!("http://{address}/v1/files").parse().unwrap();
                        RequestBuilder::from_parts(http.clone(), request)
                    },
                )
                .await
                .unwrap();
            // Enforce exactly one MIME header and a boundary matching the multipart body.
            // 强制只有一个 MIME 请求头，且分隔符与 multipart 正文一致。
            let captured = tokio::time::timeout(Duration::from_secs(5), server)
                .await
                .unwrap()
                .unwrap();
            let (headers, body) = captured.split_once("\r\n\r\n").unwrap();
            let types = headers
                .lines()
                .filter_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-type")
                        .then_some(value.trim())
                })
                .collect::<Vec<_>>();
            assert_eq!(types.len(), 1);
            if multipart {
                let boundary = types[0]
                    .strip_prefix("multipart/form-data; boundary=")
                    .unwrap();
                assert!(body.starts_with(&format!("--{boundary}\r\n")));
                assert!(body.ends_with(&format!("--{boundary}--\r\n")));
                assert!(body.contains("{\"input\":\"fixture\"}\n"));
                assert!(body.contains("name=\"purpose\"\r\n\r\nbatch"));
            } else {
                assert_eq!(types[0], "application/json");
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(body).unwrap()["input_file_id"],
                    "file-fixture"
                );
            }
        }
    }

    /// Builds one minimal model used by deterministic Batch planner tests.
    /// 构建确定性 Batch 规划器测试使用的最小模型。
    fn test_model() -> ConnectionModel {
        ConnectionModel {
            id: String::from("embedding-model"),
            display_name: String::from("Embedding Model"),
            capabilities: ModelCapabilities::default(),
            capability_evidence: ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        }
    }

    /// Builds the OpenRouter embedding model contract used by route-identity tests.
    /// 构建路由身份测试使用的 OpenRouter 向量模型契约。
    fn test_openrouter_embedding_model() -> ConnectionModel {
        let mut model = test_model();
        model.id = String::from("openai/text-embedding-3-small");
        model.capabilities.embedding = Some(EmbeddingCapabilities {
            tasks: BTreeSet::from([EmbeddingTask::Document]),
            input_resources: BTreeSet::new(),
            output_kinds: BTreeSet::from([EmbeddingOutputKind::Dense]),
            encodings: BTreeSet::from([EmbeddingEncoding::Float]),
            dimensions: BTreeSet::from([8]),
            minimum_dimensions: None,
            maximum_dimensions: None,
            default_dimensions: Some(8),
            maximum_inputs: Some(1),
            maximum_tokens_per_input: None,
            normalization: EmbeddingNormalization::Unknown,
            distance_metric: DistanceMetric::Unknown,
        });
        model
    }

    /// Builds one ordered text request with stable caller-owned identities.
    /// 构建一份带稳定调用方身份的有序文本请求。
    fn test_request(contents: &[&str]) -> EmbeddingRequest {
        EmbeddingRequest {
            contract_version: 1,
            request_id: String::from("request-1"),
            idempotency_key: None,
            consumer_profile_id: String::from("vce"),
            inputs: contents
                .iter()
                .enumerate()
                .map(|(index, content)| EmbeddingInput {
                    input_id: format!("input-{index}"),
                    content: EmbeddingInputContent::Text {
                        text: (*content).to_string(),
                    },
                })
                .collect(),
            task: EmbeddingTask::Document,
            dimensions: Some(8),
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            invalid_input_policy: EmbeddingInvalidInputPolicy::FailBatch,
        }
    }

    /// Builds one bounded provider binding for either supported transport.
    /// 为任一支持的传输方式构建一份有界供应商绑定。
    fn test_binding(
        transport: InferenceBatchTransport,
        maximum_requests: u32,
        maximum_payload_bytes: u64,
        maximum_item_bytes: Option<u64>,
    ) -> NativeInferenceBatchBinding {
        let file_backed = transport == InferenceBatchTransport::JsonlFile;
        NativeInferenceBatchBinding {
            protocol: match transport {
                InferenceBatchTransport::InlineJson => {
                    NativeInferenceBatchProtocolKind::OpenRouterBetaV1
                }
                InferenceBatchTransport::JsonlFile => {
                    NativeInferenceBatchProtocolKind::AlibabaOpenAiCompatibleV1
                }
            },
            capabilities: EmbeddingBatchCapabilities {
                transport,
                result_delivery: if file_backed {
                    InferenceBatchResultDelivery::JsonlFiles
                } else {
                    InferenceBatchResultDelivery::InlineJson
                },
                provider_maximum_requests: Some(maximum_requests),
                provider_maximum_payload_bytes: Some(maximum_payload_bytes),
                provider_maximum_item_bytes: maximum_item_bytes,
                provider_maximum_files: file_backed.then_some(10_000),
                provider_maximum_total_file_bytes: file_backed.then_some(100 * 1024 * 1024),
                provider_file_upload_qps: file_backed.then_some(3),
                provider_file_operation_qps: file_backed.then_some(10),
                provider_maximum_parallel_tasks: file_backed.then_some(1_000),
                provider_task_create_rpm: file_backed.then_some(1_000),
                provider_task_status_rpm: file_backed.then_some(1_000),
                provider_task_list_rpm: file_backed.then_some(100),
                provider_task_cancel_rpm: file_backed.then_some(1_000),
                host_policy_maximum_tasks_per_run: 100,
                host_policy_maximum_requests: maximum_requests,
                host_policy_maximum_payload_bytes: maximum_payload_bytes,
                host_policy_maximum_result_bytes: 16 * 1024 * 1024,
                provider_completion_window_seconds: match transport {
                    InferenceBatchTransport::InlineJson => None,
                    InferenceBatchTransport::JsonlFile => Some(86_400),
                },
                minimum_poll_interval_ms: 1_000,
                supports_cancel: true,
                deletes_input_file_after_ingestion: file_backed,
            },
            base_url_override: None,
            submit_endpoint_path: String::from("/v1/batches"),
            status_endpoint_path_template: String::from("/v1/batches/{batch_id}"),
            cancel_endpoint_path_template: Some(String::from("/v1/batches/{batch_id}/cancel")),
            item_endpoint_path: String::from("/v1/embeddings"),
            file_upload_endpoint_path: file_backed.then(|| String::from("/v1/files")),
            file_content_endpoint_path_template: file_backed
                .then(|| String::from("/v1/files/{file_id}/content")),
            file_delete_endpoint_path_template: file_backed
                .then(|| String::from("/v1/files/{file_id}")),
        }
    }

    /// Builds one synchronous or supplemental Batch action binding for selector tests.
    /// 为选择器测试构建一个同步或补充 Batch 动作绑定。
    fn test_action_binding(
        binding_id: &str,
        model_ids: Vec<String>,
        batch: Option<NativeInferenceBatchBinding>,
    ) -> crate::NativeInferenceBinding {
        crate::NativeInferenceBinding {
            operation: AiOperation::EmbeddingCreate,
            model_ids,
            binding_id: binding_id.to_string(),
            endpoint_path: String::from("/embeddings"),
            projection: crate::NativeInferenceProjectionKind::Compatible,
            service_origin_base_path: None,
            requires_service_origin_override: false,
            rerank_truncation: None,
            response_model_aliases: BTreeMap::new(),
            batch,
        }
    }

    /// Verifies an exact Batch declaration augments rather than replaces the synchronous route.
    /// 验证精确 Batch 声明只增强而不会替换同步路由。
    #[test]
    fn batch_binding_selection_is_independent_from_synchronous_selection() {
        let bindings = vec![
            test_action_binding(
                "embedding.batch.v1",
                vec![String::from("embedding-model")],
                Some(test_binding(
                    InferenceBatchTransport::InlineJson,
                    10,
                    1_048_576,
                    None,
                )),
            ),
            test_action_binding("embedding.sync.v1", Vec::new(), None),
        ];

        let synchronous = crate::native_inference::select_native_binding(
            &bindings,
            AiOperation::EmbeddingCreate,
            "embedding-model",
        )
        .expect("synchronous binding");
        let asynchronous =
            select_native_batch_binding(&bindings, AiOperation::EmbeddingCreate, "embedding-model")
                .expect("Batch binding");

        assert_eq!(synchronous.binding_id, "embedding.sync.v1");
        assert_eq!(asynchronous.binding_id, "embedding.batch.v1");
    }

    /// Verifies lifecycle identity exposes the supplemental Batch action rather than the sync route.
    /// 验证生命周期身份公开补充 Batch 动作，而不是同步路由。
    #[test]
    fn batch_request_identity_uses_exact_supplemental_action_binding() {
        let route = crate::execution::test_support::bundled_manifest_route(
            "openrouter",
            "public-api",
            "openai-chat-completions.v1",
        );
        let model = test_openrouter_embedding_model();
        let request = test_request(&["alpha", "beta"]);

        let (snapshot, _, fingerprint) = embedding_batch_identity(
            &route,
            &model,
            &request,
            InferenceUsageScope::InternalBackground,
            ExecutionRevisionRefs {
                catalog: "catalog-revision",
                route: "route-revision",
            },
        )
        .expect("Batch identity");

        assert_eq!(
            snapshot.action_binding_id.as_deref(),
            Some("openrouter.embeddings.batch.v1")
        );
        assert_eq!(snapshot.protocol_binding_id, None);
        assert!(!fingerprint.is_empty());
    }

    /// Verifies provider status normalization remains closed and terminal-aware.
    /// 验证供应商状态归一化保持封闭且终态明确。
    #[test]
    fn batch_status_mapping_rejects_unknown_values() {
        assert_eq!(
            normalize_batch_state("completed").expect("known state"),
            EmbeddingBatchProviderState::Completed
        );
        assert!(normalize_batch_state("queued").is_err());
    }

    /// Verify the declared wire protocol cannot disagree with transport and result delivery.
    /// 验证声明的线协议不能与传输方式及结果交付方式不一致。
    #[test]
    fn batch_binding_rejects_protocol_transport_mismatch() {
        let mut binding = test_binding(InferenceBatchTransport::InlineJson, 10, 1_024, None);
        binding.protocol = NativeInferenceBatchProtocolKind::AlibabaOpenAiCompatibleV1;
        assert_eq!(
            binding
                .validate()
                .expect_err("mismatched Batch wire protocol must fail"),
            "Native inference Batch protocol, transport, and result delivery are inconsistent"
        );
    }

    /// Verifies each closed wire protocol enforces its documented completion-window range.
    /// 验证每个封闭线协议都会执行其文档规定的完成窗口范围。
    #[test]
    fn batch_binding_enforces_protocol_completion_windows() {
        let mut openrouter = test_binding(InferenceBatchTransport::InlineJson, 10, 1_024, None);
        openrouter.capabilities.provider_completion_window_seconds = Some(86_400);
        assert!(openrouter.validate().is_err());

        let mut alibaba = test_binding(InferenceBatchTransport::JsonlFile, 10, 1_024, Some(512));
        alibaba.capabilities.provider_completion_window_seconds = Some(1_213_200);
        assert!(alibaba.validate().is_err());
        alibaba.capabilities.provider_completion_window_seconds = Some(1_209_600);
        assert!(alibaba.validate().is_ok());
    }

    /// Verifies publication rejects a result limit the runtime would otherwise need to truncate.
    /// 验证发布阶段会拒绝运行时原本只能截断的结果上限。
    #[test]
    fn batch_binding_rejects_result_limit_above_process_ceiling() {
        let mut binding = test_binding(InferenceBatchTransport::InlineJson, 10, 1_024, None);
        binding.capabilities.host_policy_maximum_result_bytes =
            EMBEDDING_BATCH_ABSOLUTE_MAXIMUM_RESULT_BYTES + 1;

        let error = binding
            .validate()
            .expect_err("oversized result policy must fail before publication");
        assert!(error.contains("process safety ceiling"));
    }

    /// Verifies compatible Batch responses retain the documented plural diagnostics field.
    /// 验证兼容 Batch 响应会保留文档规定的复数诊断字段。
    #[test]
    fn batch_object_parses_plural_errors_and_null_metadata() {
        let batch = parse_batch_object(
            r#"{"id":"batch-1","status":"failed","errors":{"message":"invalid"},"metadata":null}"#,
        )
        .expect("compatible Batch object");

        assert_eq!(
            batch.errors,
            Some(serde_json::json!({"message": "invalid"}))
        );
        assert_eq!(batch.metadata, None);
    }

    /// Verify a completed file-backed Batch exposes provider item counts and one bounded cause.
    /// 验证已完成的文件型批次展示供应商条目计数与一条有界原因。
    /// No parameters; synthetic JSONL must not expose the per-item custom id in the diagnostic.
    /// 无参数；模拟 JSONL 的诊断不得暴露单条自定义标识。
    #[test]
    fn failed_batch_error_file_reports_count_and_provider_cause() {
        let body = r#"{"custom_id":"private-item-id","response":null,"error":{"code":"InvalidParameter","message":"input is too long"}}
{"custom_id":"another-item","response":null,"error":{"code":"RateLimit","message":"later error"}}"#;
        let example = first_batch_error_file_diagnostic(body).expect("structured error example");
        let diagnostic = failed_batch_request_diagnostic(Some(197), Some(298));

        assert!(diagnostic.contains("197/298 failed request items"));
        assert_eq!(example.display(), "InvalidParameter: input is too long");
        assert!(!example.display().contains("private-item-id"));
        assert!(!example.display().contains("later error"));
        let failure = batch_failed_items_failure(diagnostic, Some(&example));
        assert_eq!(failure.error.category, AiErrorCategory::Unclassified);
        assert_eq!(failure.error.stage, "batch_collect");
        assert_eq!(failure.error.http_status, None);
        assert_eq!(
            failure.error.upstream_code.as_deref(),
            Some("InvalidParameter")
        );
        assert!(!failure.error.retryable);
    }

    /// Verify a provider-side vLLM item error is not labeled as a malformed user request.
    /// 验证供应商侧 vLLM 条目错误不会被标记为用户请求格式错误。
    /// No parameters; the observed error code remains typed without fabricating an HTTP 200 item status.
    /// 无参数；保留已观察到的错误码，不虚构条目的 HTTP 200 状态。
    #[test]
    fn failed_batch_vllm_item_keeps_upstream_code_without_user_blame() {
        let body = r#"{"custom_id":"private-item-id","response":null,"error":{"code":"InternalError.Algo","message":"<5000405> InternalError.Algo.ForwardingTransportError: vLLM Error: "}}"#;
        let example = first_batch_error_file_diagnostic(body).expect("provider item error");
        let failure = batch_failed_items_failure(
            failed_batch_request_diagnostic(Some(197), Some(298)),
            Some(&example),
        );

        assert_eq!(failure.error.category, AiErrorCategory::Unclassified);
        assert_eq!(
            failure.error.subcategory.as_deref(),
            Some("batch_item_failure")
        );
        assert_eq!(
            failure.error.upstream_code.as_deref(),
            Some("InternalError.Algo")
        );
        assert_eq!(failure.error.http_status, None);
        assert!(!failure.error.retryable);
    }

    /// Verifies Alibaba lifecycle responses must echo the exact durable task and input file.
    /// 验证阿里云生命周期响应必须精确回显持久任务及输入文件身份。
    #[test]
    fn alibaba_batch_ownership_rejects_missing_or_mismatched_echoes() {
        let exact = parse_batch_object(
            r#"{"id":"batch-1","status":"validating","input_file_id":"file-1","metadata":{"ds_name":"task-1"}}"#,
        )
        .expect("exact Alibaba Batch object");
        validate_alibaba_batch_ownership(&exact, "task-1", "file-1")
            .expect("exact ownership echoes");

        let wrong_file = parse_batch_object(
            r#"{"id":"batch-1","status":"validating","input_file_id":"file-2","metadata":{"ds_name":"task-1"}}"#,
        )
        .expect("wrong-file Alibaba Batch object");
        assert!(validate_alibaba_batch_ownership(&wrong_file, "task-1", "file-1").is_err());

        let missing_metadata = parse_batch_object(
            r#"{"id":"batch-1","status":"validating","input_file_id":"file-1","metadata":null}"#,
        )
        .expect("metadata-free Alibaba Batch object");
        assert!(validate_alibaba_batch_ownership(&missing_metadata, "task-1", "file-1").is_err());
    }

    /// Verify inline result collection rejects a completed object owned by another Batch id.
    /// 验证内嵌结果收集会拒绝属于另一 Batch 身份的完成对象。
    #[test]
    fn inline_result_collection_rejects_wrong_provider_batch_id() {
        let submission = EmbeddingBatchSubmission {
            task_id: String::from("task-one"),
            provider_batch_id: String::from("batch-expected"),
            provider_input_file_id: None,
            submitted_at_unix_ms: 1,
        };
        let body = r#"{"id":"batch-other","status":"completed","request_counts":{"total":1,"completed":1,"failed":0},"results":[]}"#;

        assert!(parse_inline_batch_result_items(&submission, body).is_err());
    }

    /// Verifies OpenRouter's stream-sensitive top-level field order.
    /// 验证 OpenRouter 对流式解析敏感的顶层字段顺序。
    #[test]
    fn openrouter_prefix_orders_endpoint_and_model_before_requests() {
        let prefix = openrouter_payload_prefix("/v1/embeddings", "model-a").expect("encode prefix");
        assert_eq!(
            prefix,
            "{\"endpoint\":\"/v1/embeddings\",\"model\":\"model-a\",\"requests\":["
        );
    }

    /// Verifies stable custom ids change for different original positions.
    /// 验证稳定 custom id 会随不同原始位置变化。
    #[test]
    fn custom_ids_are_stable_and_position_specific() {
        assert_eq!(
            batch_custom_id("request", "input", 1),
            batch_custom_id("request", "input", 1)
        );
        assert_ne!(
            batch_custom_id("request", "input", 1),
            batch_custom_id("request", "input", 2)
        );
    }

    /// Verifies request-count partitioning and exact payload fingerprints are deterministic.
    /// 验证按请求数分片及精确负载指纹保持确定性。
    #[test]
    fn planner_partitions_deterministically_and_hashes_exact_payload() {
        let request = test_request(&["one", "two", "three"]);
        let binding = test_binding(InferenceBatchTransport::InlineJson, 2, 1_048_576, None);
        let first =
            prepare_embedding_batch_tasks(&test_model(), &request, &binding).expect("first plan");
        let second =
            prepare_embedding_batch_tasks(&test_model(), &request, &binding).expect("second plan");

        assert_eq!(first.len(), 2);
        assert_eq!(first[0].first_input_index, 0);
        assert_eq!(first[0].input_count, 2);
        assert_eq!(first[1].first_input_index, 2);
        assert_eq!(first[1].input_count, 1);
        assert_eq!(first[0], second[0]);
        let prepared =
            prepare_exact_embedding_batch_task(&test_model(), &request, &binding, &first[0])
                .expect("rebuild first payload");
        assert_eq!(
            first[0].payload_sha256,
            hex::encode(Sha256::digest(&prepared.payload))
        );
        assert_eq!(
            first[0].encoded_payload_bytes,
            u64::try_from(prepared.payload.len()).expect("payload length")
        );
    }

    /// Verifies a larger run stays contiguous while every exact task can be rebuilt independently.
    /// 验证较大运行保持连续分区，并且每个精确任务均可独立重建。
    #[test]
    fn planner_rebuilds_every_bounded_task_from_a_larger_run() {
        let contents = vec!["bounded-input"; 97];
        let request = test_request(&contents);
        let binding = test_binding(InferenceBatchTransport::InlineJson, 7, 1_048_576, None);
        let tasks =
            prepare_embedding_batch_tasks(&test_model(), &request, &binding).expect("large plan");

        assert_eq!(tasks.len(), 14);
        let mut next_input_index = 0_u32;
        for task in &tasks {
            assert_eq!(task.first_input_index, next_input_index);
            assert!(task.input_count <= 7);
            let prepared =
                prepare_exact_embedding_batch_task(&test_model(), &request, &binding, task)
                    .expect("rebuild bounded payload");
            assert_eq!(
                task.payload_sha256,
                hex::encode(Sha256::digest(&prepared.payload))
            );
            next_input_index = task.end_input_index().expect("task end");
        }
        assert_eq!(next_input_index, 97);
    }

    /// Verifies every OpenRouter Batch item keeps the complete embedding endpoint body.
    /// 验证每个 OpenRouter Batch 请求项均保留完整的向量端点正文。
    #[test]
    fn openrouter_planner_repeats_model_inside_each_request_body() {
        let request = test_request(&["one", "two"]);
        let binding = test_binding(InferenceBatchTransport::InlineJson, 10, 1_048_576, None);
        let tasks =
            prepare_embedding_batch_tasks(&test_model(), &request, &binding).expect("inline plan");
        let prepared =
            prepare_exact_embedding_batch_task(&test_model(), &request, &binding, &tasks[0])
                .expect("rebuild inline payload");
        let payload: serde_json::Value =
            serde_json::from_slice(&prepared.payload).expect("inline Batch payload");
        let requests = payload["requests"]
            .as_array()
            .expect("inline Batch requests");

        assert_eq!(payload["model"], "embedding-model");
        assert_eq!(requests.len(), 2);
        for item in requests {
            assert_eq!(item["body"]["model"], "embedding-model");
            assert_eq!(item["body"]["encoding_format"], "float");
        }
    }

    /// Verifies Alibaba payloads remain one complete request object per JSONL line.
    /// 验证阿里云负载保持每条 JSONL 行一个完整请求对象。
    #[test]
    fn alibaba_planner_emits_exact_jsonl_lines() {
        let request = test_request(&["one", "two"]);
        let binding = test_binding(
            InferenceBatchTransport::JsonlFile,
            10,
            1_048_576,
            Some(6 * 1024 * 1024),
        );
        let tasks =
            prepare_embedding_batch_tasks(&test_model(), &request, &binding).expect("jsonl plan");

        assert_eq!(tasks.len(), 1);
        let prepared =
            prepare_exact_embedding_batch_task(&test_model(), &request, &binding, &tasks[0])
                .expect("rebuild JSONL payload");
        let lines = prepared
            .payload
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        for line in lines {
            let value: serde_json::Value = serde_json::from_slice(line).expect("JSONL item");
            assert_eq!(value["method"], "POST");
            assert_eq!(value["url"], "/v1/embeddings");
            assert_eq!(value["body"]["model"], "embedding-model");
        }
    }

    /// Verifies an oversized provider item is rejected before any network dispatch.
    /// 验证超过供应商单项上限的请求会在任何网络分派前被拒绝。
    #[test]
    fn planner_rejects_item_larger_than_provider_limit() {
        let request = test_request(&["content larger than the configured item bound"]);
        let binding = test_binding(InferenceBatchTransport::JsonlFile, 10, 1_048_576, Some(8));

        let error = prepare_embedding_batch_tasks(&test_model(), &request, &binding)
            .expect_err("oversized provider item must fail planning");
        assert!(error.error.message.contains("bytes"));
        assert!(
            error
                .error
                .message
                .contains("provider item-size limit of 8")
        );
    }

    /// Verifies the exact payload boundary is accepted while one byte less fails before dispatch.
    /// 验证精确负载边界可被接受，而少一个字节的限制会在分派前失败。
    #[test]
    fn planner_enforces_exact_payload_boundary() {
        let request = test_request(&["one"]);
        let generous = test_binding(InferenceBatchTransport::InlineJson, 10, 1_048_576, None);
        let exact_bytes = prepare_embedding_batch_tasks(&test_model(), &request, &generous)
            .expect("measure payload")
            .remove(0)
            .encoded_payload_bytes;
        let exact = test_binding(InferenceBatchTransport::InlineJson, 10, exact_bytes, None);
        let too_small = test_binding(
            InferenceBatchTransport::InlineJson,
            10,
            exact_bytes.saturating_sub(1),
            None,
        );

        assert!(prepare_embedding_batch_tasks(&test_model(), &request, &exact).is_ok());
        let error = prepare_embedding_batch_tasks(&test_model(), &request, &too_small)
            .expect_err("undersized host policy must fail planning");
        assert!(error.error.message.contains(&format!("{exact_bytes}-byte")));
        assert!(error.error.message.contains("host-policy"));
    }

    /// Verifies one logical run cannot expand beyond the host task-count policy.
    /// 验证一次逻辑运行不能扩张到超过宿主任务数量策略。
    #[test]
    fn planner_rejects_too_many_provider_tasks() {
        let request = test_request(&["one", "two", "three"]);
        let mut binding = test_binding(InferenceBatchTransport::InlineJson, 1, 1_048_576, None);
        binding.capabilities.host_policy_maximum_tasks_per_run = 2;

        let error = match prepare_embedding_batch_tasks(&test_model(), &request, &binding) {
            Ok(_) => panic!("three tasks must exceed the two-task host limit"),
            Err(error) => error,
        };
        assert!(error.error.message.contains("more than 2"));
    }

    /// Verifies provider result order cannot change caller input order.
    /// 验证供应商结果顺序无法改变调用方输入顺序。
    #[test]
    fn result_mapping_restores_original_order() {
        let request = test_request(&["one", "two"]);
        let binding = test_binding(InferenceBatchTransport::InlineJson, 10, 1_048_576, None);
        let task = prepare_embedding_batch_tasks(&test_model(), &request, &binding)
            .expect("plan")
            .remove(0);
        let first_id = batch_custom_id("request-1", "input-0", 0);
        let second_id = batch_custom_id("request-1", "input-1", 1);
        let mapped = map_batch_result_items(
            &request,
            &task,
            vec![
                WireBatchResultItem {
                    custom_id: second_id,
                    response: Some(WireBatchItemResponse {
                        status_code: 200,
                        body: serde_json::json!({"marker": 2}),
                    }),
                    error: None,
                },
                WireBatchResultItem {
                    custom_id: first_id,
                    response: Some(WireBatchItemResponse {
                        status_code: 200,
                        body: serde_json::json!({"marker": 1}),
                    }),
                    error: None,
                },
            ],
        )
        .expect("mapped results");

        assert_eq!(mapped[0].0, 0);
        assert_eq!(mapped[0].1["marker"], 1);
        assert_eq!(mapped[1].0, 1);
        assert_eq!(mapped[1].1["marker"], 2);
    }

    /// Verifies duplicate custom ids are rejected instead of silently losing one result.
    /// 验证重复 custom id 会被拒绝而不是静默丢失一项结果。
    #[test]
    fn result_mapping_rejects_duplicate_custom_ids() {
        let request = test_request(&["one", "two"]);
        let binding = test_binding(InferenceBatchTransport::InlineJson, 10, 1_048_576, None);
        let task = prepare_embedding_batch_tasks(&test_model(), &request, &binding)
            .expect("plan")
            .remove(0);
        let repeated_id = batch_custom_id("request-1", "input-0", 0);
        let items = (0..2)
            .map(|_| WireBatchResultItem {
                custom_id: repeated_id.clone(),
                response: Some(WireBatchItemResponse {
                    status_code: 200,
                    body: serde_json::json!({"marker": 1}),
                }),
                error: None,
            })
            .collect();

        assert!(map_batch_result_items(&request, &task, items).is_err());
    }

    /// Verifies an unknown custom id cannot impersonate one expected result item.
    /// 验证未知 custom id 不能冒充一项预期结果。
    #[test]
    fn result_mapping_rejects_unknown_custom_id() {
        let request = test_request(&["one"]);
        let binding = test_binding(InferenceBatchTransport::InlineJson, 10, 1_048_576, None);
        let task = prepare_embedding_batch_tasks(&test_model(), &request, &binding)
            .expect("plan")
            .remove(0);
        let items = vec![WireBatchResultItem {
            custom_id: String::from("unknown"),
            response: Some(WireBatchItemResponse {
                status_code: 200,
                body: serde_json::json!({"marker": 1}),
            }),
            error: None,
        }];

        assert!(map_batch_result_items(&request, &task, items).is_err());
    }

    /// Verifies an explicit provider item error fails the complete VCE Batch result.
    /// 验证明确的供应商单项错误会使完整 VCE Batch 结果失败。
    #[test]
    fn result_mapping_rejects_provider_item_error() {
        let request = test_request(&["one"]);
        let binding = test_binding(InferenceBatchTransport::InlineJson, 10, 1_048_576, None);
        let task = prepare_embedding_batch_tasks(&test_model(), &request, &binding)
            .expect("plan")
            .remove(0);
        let items = vec![WireBatchResultItem {
            custom_id: batch_custom_id("request-1", "input-0", 0),
            response: None,
            error: Some(serde_json::json!({"message": "invalid input"})),
        }];

        assert!(map_batch_result_items(&request, &task, items).is_err());
    }

    /// Verifies a non-success per-item HTTP status cannot be accepted as an embedding result.
    /// 验证单项非成功 HTTP 状态不能被接受为向量结果。
    #[test]
    fn result_mapping_rejects_non_success_item_status() {
        let request = test_request(&["one"]);
        let binding = test_binding(InferenceBatchTransport::InlineJson, 10, 1_048_576, None);
        let task = prepare_embedding_batch_tasks(&test_model(), &request, &binding)
            .expect("plan")
            .remove(0);
        let items = vec![WireBatchResultItem {
            custom_id: batch_custom_id("request-1", "input-0", 0),
            response: Some(WireBatchItemResponse {
                status_code: 429,
                body: serde_json::json!({"error": "rate limited"}),
            }),
            error: None,
        }];

        assert!(map_batch_result_items(&request, &task, items).is_err());
    }

    /// Verifies a structurally valid task cannot index beyond its immutable typed request.
    /// 验证结构有效的任务不能索引到不可变类型化请求范围之外。
    #[test]
    fn result_mapping_rejects_task_range_outside_request() {
        let request = test_request(&["one"]);
        let task = EmbeddingBatchTaskPlan {
            task_id: String::from("foreign-range"),
            first_input_index: 1,
            input_count: 1,
            encoded_payload_bytes: 1,
            payload_sha256: "a".repeat(64),
        };

        assert!(map_batch_result_items(&request, &task, Vec::new()).is_err());
    }

    /// Verifies a Batch base URL with invisible surrounding whitespace is rejected before dispatch.
    /// 验证带不可见首尾空白的 Batch 基础 URL 会在分派前被拒绝。
    #[test]
    fn batch_binding_rejects_untrimmed_base_url_override() {
        let mut binding = test_binding(InferenceBatchTransport::InlineJson, 10, 1_048_576, None);
        binding.base_url_override = Some(String::from(" https://openrouter.ai/api/beta"));

        assert!(binding.validate().is_err());
    }

    /// Verifies a Batch API root cannot conceal credentials inside its URL authority.
    /// 验证 Batch API 根地址不能在 URL 权威部分隐藏凭据。
    #[test]
    fn batch_binding_rejects_credential_bearing_base_url_override() {
        let mut binding = test_binding(InferenceBatchTransport::InlineJson, 10, 1_048_576, None);
        binding.base_url_override = Some(String::from(
            "https://embedded-secret@openrouter.ai/api/beta",
        ));

        assert!(binding.validate().is_err());
    }

    /// Verifies every manifest endpoint is canonical before the binding can be published.
    /// 验证每个 Manifest 端点在绑定发布前均为规范路径。
    #[test]
    fn batch_binding_rejects_noncanonical_endpoint_paths() {
        let mut whitespace = test_binding(InferenceBatchTransport::InlineJson, 10, 1_048_576, None);
        whitespace.submit_endpoint_path = String::from(" /batches");
        assert!(whitespace.validate().is_err());

        let mut traversal = test_binding(InferenceBatchTransport::InlineJson, 10, 1_048_576, None);
        traversal.item_endpoint_path = String::from("/v1/../embeddings");
        assert!(traversal.validate().is_err());

        let mut duplicate = test_binding(InferenceBatchTransport::InlineJson, 10, 1_048_576, None);
        duplicate.status_endpoint_path_template = String::from("/batches/{batch_id}/{batch_id}");
        assert!(duplicate.validate().is_err());

        let mut query = test_binding(InferenceBatchTransport::InlineJson, 10, 1_048_576, None);
        query.status_endpoint_path_template = String::from("/batches/{batch_id}?expand=true");
        assert!(query.validate().is_err());
    }
}
