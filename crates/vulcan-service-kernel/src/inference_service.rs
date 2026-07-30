//! Authorized transport dispatch for the loopback configured-inference service.
//! 回环已配置推理服务的授权传输分发。

use crate::{
    configured_inference::{
        ConfiguredInferenceError, ConfiguredInferenceErrorCategory, ConfiguredInferenceService,
    },
    inference_access::InferenceAccessRegistry,
    state::SharedServiceBridgeState,
};
use serde::Serialize;
use std::sync::Arc;
use vulcan_ai_contract::{
    EmbeddingInputContent, EmbeddingRequest, InferencePermission, InferenceUsageScope,
};
use vulcan_service_api::{InferenceServiceHttpError, InferenceServiceRequest};
use vulcan_service_host::{
    InferenceServiceEventSubscriber, InferenceServiceEventSubscription,
    InferenceServiceRpcDispatcher,
};

/// Builds the transport-independent inference request dispatcher.
/// 构建传输无关的推理请求分发器。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative configured inference runtime.
/// `state` 持有权威已配置推理运行时。
/// `access_registry` owns process-local least-privilege grants.
/// `access_registry` 持有进程内最小权限授权。
///
/// # Returns
/// 返回值
/// A cloneable asynchronous dispatcher suitable for the loopback HTTP host.
/// 返回适用于回环 HTTP 宿主的可克隆异步分发器。
pub(crate) fn inference_service_dispatcher(
    state: SharedServiceBridgeState,
    access_registry: Arc<InferenceAccessRegistry>,
    consumer_profile_id: String,
) -> InferenceServiceRpcDispatcher {
    Arc::new(move |access_token, request| {
        let state = state.clone();
        let access_registry = Arc::clone(&access_registry);
        let consumer_profile_id = consumer_profile_id.clone();
        Box::pin(async move {
            dispatch_inference_service_request(
                state,
                access_registry,
                consumer_profile_id,
                access_token,
                request,
            )
            .await
        })
    })
}

/// Builds the authorized restricted-LLM event subscriber.
/// 构建经过授权的受限 LLM 事件订阅器。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative configured inference runtime.
/// `state` 持有权威已配置推理运行时。
/// `access_registry` owns process-local least-privilege grants.
/// `access_registry` 持有进程内最小权限授权。
/// `consumer_profile_id` is the exact profile represented by the discovery grant.
/// `consumer_profile_id` 是发现授权代表的精确配置档。
///
/// # Returns
/// 返回值
/// A cloneable subscriber that checks token scope and execution ownership.
/// 返回会校验令牌范围与执行所有权的可克隆订阅器。
pub(crate) fn inference_service_event_subscriber(
    state: SharedServiceBridgeState,
    access_registry: Arc<InferenceAccessRegistry>,
    consumer_profile_id: String,
) -> InferenceServiceEventSubscriber {
    Arc::new(move |access_token, execution_id| {
        let state = state.clone();
        let access_registry = Arc::clone(&access_registry);
        let consumer_profile_id = consumer_profile_id.clone();
        Box::pin(async move {
            let caller = access_registry
                .authorize(
                    access_token.as_str(),
                    consumer_profile_id.as_str(),
                    InferencePermission::Status,
                    None,
                    InferenceUsageScope::ExternalService,
                )
                .map_err(access_error)?;
            let service = configured_inference_service(&state)?;
            let (replay, receiver) = service
                .stream_llm(caller.caller_id.as_str(), execution_id.as_str())
                .map_err(configured_inference_http_error)?;
            Ok(InferenceServiceEventSubscription { replay, receiver })
        })
    })
}

/// Dispatches one closed inference operation after exact grant authorization.
/// 在精确授权校验后分发一项封闭推理操作。
///
/// # Parameters
/// 参数
/// `state`, `access_registry`, `access_token`, and `request` are the complete trusted dispatch inputs.
/// `state`、`access_registry`、`access_token` 与 `request` 是完整的可信分发输入。
///
/// # Returns
/// 返回值
/// A serialized operation result or controlled HTTP failure.
/// 返回序列化操作结果或受控 HTTP 失败。
async fn dispatch_inference_service_request(
    state: SharedServiceBridgeState,
    access_registry: Arc<InferenceAccessRegistry>,
    discovery_consumer_profile_id: String,
    access_token: String,
    request: InferenceServiceRequest,
) -> Result<serde_json::Value, InferenceServiceHttpError> {
    let service = configured_inference_service(&state)?;
    match request {
        InferenceServiceRequest::Capabilities {
            consumer_profile_id,
        } => {
            let caller = authorize_request(
                &access_registry,
                access_token.as_str(),
                consumer_profile_id.as_str(),
                InferencePermission::Status,
                None,
            )?;
            serialize_result(
                service
                    .capability_snapshot(&caller, consumer_profile_id.as_str())
                    .map_err(configured_inference_http_error)?,
            )
        }
        InferenceServiceRequest::StartLlm { request } => {
            let caller = authorize_request(
                &access_registry,
                access_token.as_str(),
                request.consumer_profile_id.as_str(),
                InferencePermission::Llm,
                Some(request.purpose_id.as_str()),
            )?;
            let execution_id = service
                .start_llm(caller, request)
                .await
                .map_err(configured_inference_http_error)?;
            Ok(serde_json::json!({ "execution_id": execution_id }))
        }
        InferenceServiceRequest::ExecuteLlm { request } => {
            let caller = authorize_request(
                &access_registry,
                access_token.as_str(),
                request.consumer_profile_id.as_str(),
                InferencePermission::Llm,
                Some(request.purpose_id.as_str()),
            )?;
            serialize_result(
                service
                    .execute_llm(caller, request)
                    .await
                    .map_err(configured_inference_http_error)?,
            )
        }
        InferenceServiceRequest::LlmStatus { execution_id } => {
            let caller = authorize_request(
                &access_registry,
                access_token.as_str(),
                discovery_consumer_profile_id.as_str(),
                InferencePermission::Status,
                None,
            )?;
            serialize_result(
                service
                    .execution_status(caller.caller_id.as_str(), execution_id.as_str())
                    .map_err(configured_inference_http_error)?,
            )
        }
        InferenceServiceRequest::CancelLlm { execution_id } => {
            let caller = authorize_request(
                &access_registry,
                access_token.as_str(),
                discovery_consumer_profile_id.as_str(),
                InferencePermission::Cancel,
                None,
            )?;
            service
                .cancel_execution(caller.caller_id.as_str(), execution_id.as_str())
                .map_err(configured_inference_http_error)?;
            Ok(serde_json::json!({ "cancelled": true }))
        }
        InferenceServiceRequest::CreateEmbeddings { request } => {
            let caller = authorize_request(
                &access_registry,
                access_token.as_str(),
                request.consumer_profile_id.as_str(),
                InferencePermission::Embedding,
                None,
            )?;
            validate_external_embedding_request(&request)?;
            serialize_result(
                service
                    .create_embeddings(&caller, request)
                    .await
                    .map_err(configured_inference_http_error)?,
            )
        }
        InferenceServiceRequest::Rerank { request } => {
            let caller = authorize_request(
                &access_registry,
                access_token.as_str(),
                request.consumer_profile_id.as_str(),
                InferencePermission::Rerank,
                None,
            )?;
            serialize_result(
                service
                    .rerank_documents(&caller, request)
                    .await
                    .map_err(configured_inference_http_error)?,
            )
        }
    }
}

/// Rejects resource-bearing embedding input at the external loopback trust boundary.
/// 在外部回环信任边界拒绝携带资源的向量输入。
///
/// # Parameters
/// 参数
/// `request` is already authenticated but still contains caller-controlled content.
/// `request` 已完成鉴权，但仍包含调用方控制的内容。
///
/// # Returns
/// 返回值
/// Success only when every embedding input is plain text.
/// 仅当每项向量输入都是纯文本时返回成功。
fn validate_external_embedding_request(
    request: &EmbeddingRequest,
) -> Result<(), InferenceServiceHttpError> {
    if request
        .inputs
        .iter()
        .any(|input| !matches!(&input.content, EmbeddingInputContent::Text { .. }))
    {
        return Err(InferenceServiceHttpError {
            status: 400,
            code: String::from("inference_contract_invalid"),
            message: String::from(
                "External embedding requests may contain plain text only; resource references are not accepted.",
            ),
        });
    }
    Ok(())
}

/// Authorizes one exact external-service operation.
/// 授权一项精确的外部服务操作。
///
/// # Parameters
/// 参数
/// All parameters are exact request identities and closed permission values.
/// 所有参数均为精确请求身份与封闭权限值。
///
/// # Returns
/// 返回值
/// A host-injected caller context that cannot select its own usage scope.
/// 返回无法自行选择用途范围的宿主注入调用方上下文。
fn authorize_request(
    access_registry: &InferenceAccessRegistry,
    access_token: &str,
    consumer_profile_id: &str,
    permission: InferencePermission,
    purpose_id: Option<&str>,
) -> Result<vulcan_ai_contract::InferenceCallerContext, InferenceServiceHttpError> {
    access_registry
        .authorize(
            access_token,
            consumer_profile_id,
            permission,
            purpose_id,
            InferenceUsageScope::ExternalService,
        )
        .map_err(access_error)
}

/// Reads the initialized configured-inference service without holding the state lock.
/// 在不跨调用持有状态锁的情况下读取已初始化的已配置推理服务。
///
/// # Parameters
/// 参数
/// `state` is the authoritative service bridge state.
/// `state` 是权威服务桥状态。
///
/// # Returns
/// 返回值
/// The shared runtime service or a retryable service-unavailable response.
/// 返回共享运行时服务或可重试的服务不可用响应。
fn configured_inference_service(
    state: &SharedServiceBridgeState,
) -> Result<Arc<ConfiguredInferenceService>, InferenceServiceHttpError> {
    state
        .lock()
        .map_err(|_| InferenceServiceHttpError {
            status: 503,
            code: String::from("service_state_unavailable"),
            message: String::from("Vulcan Code inference state is temporarily unavailable."),
        })?
        .agent_runtime
        .as_ref()
        .map(|runtime| Arc::clone(&runtime.configured_inference_service))
        .ok_or_else(|| InferenceServiceHttpError {
            status: 503,
            code: String::from("inference_runtime_starting"),
            message: String::from("Vulcan Code inference runtime is still starting."),
        })
}

/// Serializes one typed successful result using the versioned service envelope boundary.
/// 通过版本化服务信封边界序列化一个类型化成功结果。
///
/// # Parameters
/// 参数
/// `result` is a contract result without credential material.
/// `result` 是不含凭据材料的契约结果。
///
/// # Returns
/// 返回值
/// A JSON value or a controlled internal serialization failure.
/// 返回 JSON 值或受控内部序列化失败。
fn serialize_result<T: Serialize>(
    result: T,
) -> Result<serde_json::Value, InferenceServiceHttpError> {
    serde_json::to_value(result).map_err(|error| InferenceServiceHttpError {
        status: 500,
        code: String::from("response_serialization_failed"),
        message: format!("Inference response serialization failed: {error}"),
    })
}

/// Converts access-registry diagnostics into an unauthorized response.
/// 将访问注册表诊断转换为未授权响应。
///
/// # Parameters
/// 参数
/// `message` is a controlled registry diagnostic.
/// `message` 是受控注册表诊断。
///
/// # Returns
/// 返回值
/// A stable unauthorized transport failure.
/// 返回稳定的未授权传输失败。
fn access_error(message: String) -> InferenceServiceHttpError {
    InferenceServiceHttpError {
        status: 401,
        code: String::from("inference_access_denied"),
        message,
    }
}

/// Converts configured-inference failures into stable HTTP semantics.
/// 将已配置推理失败转换为稳定 HTTP 语义。
///
/// # Parameters
/// 参数
/// `error` retains normalized provider detail only inside the response payload.
/// `error` 仅在响应载荷内保留归一化供应商详情。
///
/// # Returns
/// 返回值
/// A controlled status, code, and diagnostic.
/// 返回受控状态码、错误码与诊断。
fn configured_inference_http_error(error: ConfiguredInferenceError) -> InferenceServiceHttpError {
    let (status, code) = match error.category {
        ConfiguredInferenceErrorCategory::Contract => (400, "inference_contract_invalid"),
        ConfiguredInferenceErrorCategory::Authorization => (403, "inference_forbidden"),
        ConfiguredInferenceErrorCategory::Configuration => {
            (409, "inference_configuration_unavailable")
        }
        ConfiguredInferenceErrorCategory::Limit => (429, "inference_limit_exceeded"),
        ConfiguredInferenceErrorCategory::Execution => (502, "inference_execution_failed"),
        ConfiguredInferenceErrorCategory::Cancelled => (409, "inference_cancelled"),
    };
    InferenceServiceHttpError {
        status,
        code: String::from(code),
        message: error.message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_ai_contract::{
        EmbeddingEncoding, EmbeddingInput, EmbeddingOutputKind, EmbeddingTask,
        INFERENCE_CONTRACT_VERSION, ResourceKind, ResourceReference,
    };

    /// Verifies the loopback API cannot be used to submit a local resource path.
    /// 验证回环 API 无法用于提交本地资源路径。
    #[test]
    fn external_embedding_rejects_resource_references() {
        let request = EmbeddingRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("request-1"),
            idempotency_key: None,
            consumer_profile_id: String::from("vmm"),
            inputs: vec![EmbeddingInput {
                input_id: String::from("resource-1"),
                content: EmbeddingInputContent::Resource {
                    resource: ResourceReference {
                        id: String::from("local-resource"),
                        kind: ResourceKind::File,
                        media_type: Some(String::from("text/plain")),
                        local_path: Some(String::from("C:\\private\\memory.txt")),
                        url: None,
                    },
                },
            }],
            task: EmbeddingTask::Document,
            dimensions: None,
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
        };

        let error = validate_external_embedding_request(&request)
            .expect_err("resource-bearing external request must fail");
        assert_eq!(error.status, 400);
        assert_eq!(error.code, "inference_contract_invalid");
    }
}
