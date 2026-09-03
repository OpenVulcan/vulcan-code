//! Optional adapter from VCE plans to the frozen VulcanCode HostAiPort.
//! 从 VCE 计划到已冻结 VulcanCode HostAiPort 的可选适配器。
//!
//! This module is feature-gated so the domain engine remains independently testable.
//! 本模块使用特性门控，以便领域引擎仍可独立测试。

#![allow(clippy::result_large_err)]
// Keep the shared host error type intact so callers receive the exact HostAiPort contract error.
// 保持共享宿主错误类型不变，以便调用方接收精确的 HostAiPort 契约错误。

use crate::embedding::{EmbeddingBatch, SegmentVector};
use vulcan_ai_contract::{
    EmbeddingEncoding, EmbeddingInput as HostEmbeddingInput, EmbeddingInputContent,
    EmbeddingInvalidInputPolicy, EmbeddingOutputKind, EmbeddingPayload, EmbeddingRequest,
    EmbeddingTask, HostAiCallContext, HostAiCapabilityHandle, HostAiEmbeddingRequest,
    HostAiEmbeddingResponse, HostAiError, HostAiErrorCode, HostAiEventStream,
    HostAiExecutionHandle, HostAiExecutionSnapshot, HostAiPort, INFERENCE_CONTRACT_VERSION,
    RerankRequest, RerankResponse, ResponsesExecutionRequest, ResponsesExecutionResult,
};

/// Configuration needed to project one VCE Embedding batch into the host contract.
/// 将一个 VCE 向量批次投影到宿主契约所需的配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEmbeddingRequestConfig {
    /// Logical vector-space identity selected by the storage owner.
    /// 由存储所有者选择的逻辑向量空间标识。
    pub vector_space_id: String,
    /// Stable request identity for this physical batch attempt.
    /// 当前物理批次尝试使用的稳定请求标识。
    pub request_id: String,
    /// Optional idempotency key used only for in-flight coalescing.
    /// 仅用于合并运行中请求的可选幂等键。
    pub idempotency_key: Option<String>,
    /// Authorized host consumer profile identity.
    /// 已授权的宿主消费者配置档标识。
    pub consumer_profile_id: String,
    /// Provider-neutral semantic task for this batch.
    /// 当前批次使用的供应商无关语义任务。
    pub task: EmbeddingTask,
    /// Optional exact target vector dimension.
    /// 可选的精确目标向量维度。
    pub dimensions: Option<u32>,
}

/// Project one VCE batch into a dense-float HostAiPort request.
/// 将一个 VCE 批次投影为稠密浮点 HostAiPort 请求。
pub fn build_embedding_request(
    batch: &EmbeddingBatch,
    config: HostEmbeddingRequestConfig,
) -> Result<HostAiEmbeddingRequest, HostAiError> {
    batch
        .validate()
        .map_err(|error| contract_error(&config.request_id, error.to_string()))?;
    let request = HostAiEmbeddingRequest {
        vector_space_id: config.vector_space_id,
        request: EmbeddingRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: config.request_id,
            idempotency_key: config.idempotency_key,
            consumer_profile_id: config.consumer_profile_id,
            inputs: batch
                .segments
                .iter()
                .map(|segment| HostEmbeddingInput {
                    input_id: segment.request_input_id(),
                    content: EmbeddingInputContent::Text {
                        text: segment.text.clone(),
                    },
                })
                .collect(),
            task: config.task,
            dimensions: config.dimensions,
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            invalid_input_policy: EmbeddingInvalidInputPolicy::FailBatch,
        },
    };
    request.validate()?;
    Ok(request)
}

/// Retain the validated host response beside VCE-aligned segment vectors.
/// 在 VCE 对齐的分段向量旁保留已校验的宿主响应。
#[derive(Debug, Clone, PartialEq)]
pub struct HostEmbeddingBatchResult {
    /// Original host response containing route, fingerprint, usage, and timing metadata.
    /// 保留路由、指纹、用量和耗时元数据的原始宿主响应。
    pub response: HostAiEmbeddingResponse,
    /// Dense vectors aligned to the exact VCE batch segment order.
    /// 按 VCE 批次分段精确顺序对齐的稠密向量。
    pub vectors: Vec<SegmentVector>,
}

/// Execute one VCE Embedding batch through a host-issued capability handle.
/// 通过宿主发放的能力句柄执行一个 VCE 向量批次。
///
/// The function validates complete segment coverage because VCE aggregation cannot safely
/// continue when a provider silently drops one segment.
/// 该函数要求片段完整覆盖，因为供应商静默丢弃片段时 VCE 不能安全继续聚合。
pub async fn create_embedding_batch_with_response<P: HostAiPort + ?Sized>(
    port: &P,
    handle: &HostAiCapabilityHandle,
    context: HostAiCallContext,
    request: HostAiEmbeddingRequest,
) -> Result<HostEmbeddingBatchResult, HostAiError> {
    request.validate()?;
    let request_for_validation = request.request.clone();
    let request_id = request_for_validation.request_id.clone();
    context.validate(&request_id)?;
    let vector_space_id = request.vector_space_id.clone();
    let expected_ids = request_for_validation
        .inputs
        .iter()
        .map(|input| input.input_id.clone())
        .collect::<Vec<_>>();
    let expected_dimension = request_for_validation
        .dimensions
        .map(|value| value as usize);
    let response = port.create_embeddings(handle, context, request).await?;
    if response.vector_space_id != vector_space_id {
        return Err(contract_error(
            &request_id,
            format!(
                "Host AI vector_space_id mismatch: expected `{vector_space_id}`, received `{}`",
                response.vector_space_id
            ),
        ));
    }
    response
        .response
        .validate_for_request(&request_for_validation)
        .map_err(|message| contract_error(&request_id, message))?;
    let fingerprint = &response.response.fingerprint;
    if fingerprint.output_kind != EmbeddingOutputKind::Dense
        || fingerprint.encoding != EmbeddingEncoding::Float
        || fingerprint.task != request_for_validation.task
    {
        return Err(contract_error(
            &request_id,
            "Host AI Embedding fingerprint is incompatible with the VCE dense-float request.",
        ));
    }
    let fingerprint_dimension = fingerprint.dimensions as usize;
    if fingerprint_dimension == 0
        || expected_dimension.is_some_and(|dimension| dimension != fingerprint_dimension)
    {
        return Err(contract_error(
            &request_id,
            format!(
                "Host AI Embedding fingerprint dimension is incompatible: expected {:?}, received {}",
                expected_dimension, fingerprint_dimension
            ),
        ));
    }
    if !response.response.dropped.is_empty() {
        return Err(contract_error(
            &request_id,
            "VCE Embedding batches require complete segment coverage; provider dropped an input.",
        ));
    }

    let mut aligned = vec![None; expected_ids.len()];
    for result in &response.response.results {
        let index = result.input_index as usize;
        let Some(expected_id) = expected_ids.get(index) else {
            return Err(contract_error(
                &request_id,
                format!("Host AI Embedding result index is out of range: {index}"),
            ));
        };
        if result.input_id != *expected_id {
            return Err(contract_error(
                &request_id,
                format!(
                    "Host AI Embedding result identity mismatch at index {index}: expected `{expected_id}`, received `{}`",
                    result.input_id
                ),
            ));
        }
        let values = match &result.embedding {
            EmbeddingPayload::Dense { values } => values.clone(),
            _ => {
                return Err(contract_error(
                    &request_id,
                    format!("VCE requires dense float Embedding output for `{expected_id}`"),
                ));
            }
        };
        if values.len() != fingerprint_dimension {
            return Err(contract_error(
                &request_id,
                format!(
                    "Host AI Embedding dimension mismatch: expected {fingerprint_dimension}, received {}",
                    values.len()
                ),
            ));
        }
        validate_dense_values(&request_id, expected_id, &values)?;
        if aligned[index]
            .replace(SegmentVector {
                input_id: result.input_id.clone(),
                values,
            })
            .is_some()
        {
            return Err(contract_error(
                &request_id,
                format!("Host AI Embedding result index is duplicated: {index}"),
            ));
        }
    }

    let vectors = aligned
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            value.ok_or_else(|| {
                contract_error(
                    &request_id,
                    format!("Host AI Embedding result is missing at index {index}"),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(HostEmbeddingBatchResult { response, vectors })
}

/// Execute one VCE Embedding batch through a host-issued capability handle.
/// 通过宿主发放的能力句柄执行一个 VCE 向量批次。
pub async fn create_embedding_batch<P: HostAiPort + ?Sized>(
    port: &P,
    handle: &HostAiCapabilityHandle,
    context: HostAiCallContext,
    request: HostAiEmbeddingRequest,
) -> Result<Vec<SegmentVector>, HostAiError> {
    Ok(
        create_embedding_batch_with_response(port, handle, context, request)
            .await?
            .vectors,
    )
}

/// Execute one optional VCE Rerank request through the common host Port.
/// 通过公共宿主 Port 执行一次可选的 VCE 重排请求。
pub async fn rerank_documents<P: HostAiPort + ?Sized>(
    port: &P,
    handle: &HostAiCapabilityHandle,
    context: HostAiCallContext,
    request: RerankRequest,
) -> Result<RerankResponse, HostAiError> {
    request
        .validate()
        .map_err(|message| contract_error(&request.request_id, message))?;
    context.validate(&request.request_id)?;
    let request_for_validation = request.clone();
    let request_id = request.request_id.clone();
    let response = port.rerank_documents(handle, context, request).await?;
    response
        .validate_for_request(&request_for_validation)
        .map_err(|message| contract_error(&request_id, message))?;
    Ok(response)
}

/// Start one VCE LLM operation through the common host Port.
/// 通过公共宿主 Port 启动一次 VCE LLM 操作。
pub async fn start_response_execution<P: HostAiPort + ?Sized>(
    port: &P,
    handle: &HostAiCapabilityHandle,
    context: HostAiCallContext,
    request: ResponsesExecutionRequest,
) -> Result<HostAiExecutionHandle, HostAiError> {
    request
        .validate()
        .map_err(|message| contract_error(&request.request_id, message))?;
    context.validate(&request.request_id)?;
    port.start_response_execution(handle, context, request)
        .await
}

/// Read the current snapshot of one host-owned VCE LLM operation.
/// 读取一项宿主持有的 VCE LLM 操作当前快照。
pub fn execution_status<P: HostAiPort + ?Sized>(
    port: &P,
    handle: &HostAiExecutionHandle,
) -> Result<HostAiExecutionSnapshot, HostAiError> {
    port.execution_status(handle)
}

/// Open the normalized replay-plus-live stream of one VCE LLM operation.
/// 打开一项 VCE LLM 操作的归一化重放加实时事件流。
pub fn stream_response<P: HostAiPort + ?Sized>(
    port: &P,
    handle: &HostAiExecutionHandle,
) -> Result<HostAiEventStream, HostAiError> {
    port.stream_response(handle)
}

/// Wait for one VCE LLM operation to reach a terminal result.
/// 等待一项 VCE LLM 操作达到终态结果。
pub async fn wait_response<P: HostAiPort + ?Sized>(
    port: &P,
    handle: &HostAiExecutionHandle,
) -> Result<ResponsesExecutionResult, HostAiError> {
    port.wait_response(handle).await
}

/// Request cooperative cancellation for one VCE LLM operation.
/// 请求协作式取消一项 VCE LLM 操作。
pub fn cancel_execution<P: HostAiPort + ?Sized>(
    port: &P,
    handle: &HostAiExecutionHandle,
) -> Result<(), HostAiError> {
    port.cancel_execution(handle)
}

/// Construct a controlled contract error tied to one request.
/// 构建绑定请求身份的受控契约错误。
fn contract_error(request_id: &str, message: impl Into<String>) -> HostAiError {
    HostAiError::new(HostAiErrorCode::Contract, message).with_request_id(request_id)
}

/// Reject non-finite dense values before they enter VCE aggregation or host storage.
/// 在稠密向量进入 VCE 聚合或宿主存储前拒绝非有限值。
fn validate_dense_values(
    request_id: &str,
    input_id: &str,
    values: &[f32],
) -> Result<(), HostAiError> {
    if values.iter().any(|value| !value.is_finite()) {
        return Err(contract_error(
            request_id,
            format!("Host AI Embedding returned a non-finite value for `{input_id}`"),
        ));
    }
    Ok(())
}

#[cfg(test)]
// Test fixtures intentionally use panic-style extraction to keep invariant failures local.
// 测试夹具有意使用恐慌式提取，使不变量失败保持在本地。
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::embedding::{EmbeddingInput, EmbeddingPlan};
    use vulcan_ai_contract::{EmbeddingInputContent, EmbeddingTask};

    /// Keep a compile-time assertion that the VCE adapter accepts the frozen Port trait.
    /// 保留编译期断言，确保 VCE 适配器接受已冻结的 Port trait。
    #[allow(dead_code)]
    fn accepts_host_port<P: HostAiPort + ?Sized>(port: &P) {
        let _ = port;
    }

    /// Verify the local VCE Embedding input remains independent from host request construction.
    /// 验证 VCE 本地向量输入与宿主请求构造保持独立。
    #[test]
    fn local_embedding_input_is_host_independent() {
        let input = EmbeddingInput::new("chunk", "File: src/lib.rs\n\nfn main() {}").unwrap();
        assert_eq!(input.input_id, "chunk");
    }

    /// Verify VCE segment identities and text are projected into the frozen host request.
    /// 验证 VCE 分段标识和文本会投影到已冻结的宿主请求中。
    #[test]
    fn projects_embedding_batch_into_host_request() {
        let input = EmbeddingInput::new("chunk", "File: src/lib.rs\n\nfn main() {}").unwrap();
        let plan = EmbeddingPlan::new(vec![input], 8, 128, 128, 0).unwrap();
        let request = build_embedding_request(
            &plan.batches[0],
            HostEmbeddingRequestConfig {
                vector_space_id: String::from("code:workspace-1"),
                request_id: String::from("embedding-1"),
                idempotency_key: None,
                consumer_profile_id: String::from("vce"),
                task: EmbeddingTask::Document,
                dimensions: Some(2),
            },
        )
        .unwrap();

        assert_eq!(request.vector_space_id, "code:workspace-1");
        assert_eq!(request.request.request_id, "embedding-1");
        assert_eq!(request.request.inputs.len(), 1);
        assert_eq!(request.request.inputs[0].input_id, "chunk::segment:0");
        assert!(matches!(
            &request.request.inputs[0].content,
            EmbeddingInputContent::Text { text } if text == "File: src/lib.rs\n\nfn main() {}"
        ));
    }

    /// Verify an empty VCE batch fails before it can reach the host capability.
    /// 验证空 VCE 批次会在到达宿主能力之前显式失败。
    #[test]
    fn rejects_empty_embedding_batch() {
        let error = build_embedding_request(
            &EmbeddingBatch {
                segments: Vec::new(),
                total_chars: 0,
            },
            HostEmbeddingRequestConfig {
                vector_space_id: String::from("code:workspace-1"),
                request_id: String::from("embedding-empty"),
                idempotency_key: None,
                consumer_profile_id: String::from("vce"),
                task: EmbeddingTask::Document,
                dimensions: Some(2),
            },
        )
        .unwrap_err();
        assert_eq!(error.code, HostAiErrorCode::Contract);
    }

    /// Verify non-finite provider values are rejected at the VCE host adapter boundary.
    /// 验证非有限供应商向量值会在 VCE 宿主适配边界被拒绝。
    #[test]
    fn rejects_nonfinite_dense_values() {
        assert!(validate_dense_values("request", "input", &[f32::NAN]).is_err());
        assert!(validate_dense_values("request", "input", &[f32::INFINITY]).is_err());
        assert!(validate_dense_values("request", "input", &[1.0, -1.0]).is_ok());
    }
}
