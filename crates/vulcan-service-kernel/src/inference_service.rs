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
use std::{collections::BTreeMap, sync::Arc};
use vulcan_ai_contract::{
    AiMessage, AiOperation, ContentItem, EmbeddingInputContent, EmbeddingRequest, ImageInputDetail,
    InferencePermission, InferenceUsageScope, MessageRole, NativeToolKind, ParallelToolCallPolicy,
    PromptCachePolicy, ReasoningEffort, ReasoningOutputPolicy, RequestUsage, ResourceKind,
    ResourceReference, ResourceSourceKind, ResponsesExecutionRequest, StructuredOutputPolicy,
    ToolCall, ToolChoicePolicy, ToolDefinition, ToolResult, UsageValue, derive_usage_sum,
};
use vulcan_service_api::{
    InferenceServiceHttpError, InferenceServiceRequest, OpenAiModel, OpenAiModelList,
    ResponsesCreateRequest,
};
use vulcan_service_host::{
    InferenceServiceCancellationGuard, InferenceServiceEventSubscriber,
    InferenceServiceEventSubscription, InferenceServiceRpcDispatcher,
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
                .stream_response(caller.caller_id.as_str(), execution_id.as_str())
                .map_err(configured_inference_http_error)?;
            let cancellation_service = Arc::clone(&service);
            let cancellation_caller_id = caller.caller_id;
            let cancellation_execution_id = execution_id;
            Ok(InferenceServiceEventSubscription {
                replay,
                receiver,
                cancellation_guard: InferenceServiceCancellationGuard::new(move || {
                    let _ = cancellation_service.cancel_execution(
                        cancellation_caller_id.as_str(),
                        cancellation_execution_id.as_str(),
                    );
                }),
            })
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
        InferenceServiceRequest::ListModels => {
            let caller = authorize_request(
                &access_registry,
                access_token.as_str(),
                discovery_consumer_profile_id.as_str(),
                InferencePermission::Status,
                None,
            )?;
            let snapshot = service
                .capability_snapshot(&caller, discovery_consumer_profile_id.as_str())
                .map_err(configured_inference_http_error)?;
            let mut public_models = BTreeMap::new();
            let mut physical_models = BTreeMap::new();
            for operation in snapshot.operations.into_iter().filter(|operation| {
                operation.available && operation.operation == AiOperation::ConversationRespond
            }) {
                let Some(route) = operation.route else {
                    continue;
                };
                if let Some(purpose_id) = operation.purpose_id {
                    public_models.insert(
                        purpose_id.clone(),
                        OpenAiModel {
                            id: purpose_id,
                            object: "model",
                            created: 0,
                            owned_by: route.provider_id.clone(),
                        },
                    );
                }
                physical_models
                    .entry(route.model_id.clone())
                    .and_modify(|entry| *entry = None)
                    .or_insert(Some(route));
            }
            for route in physical_models.into_values().flatten() {
                public_models
                    .entry(route.model_id.clone())
                    .or_insert(OpenAiModel {
                        id: route.model_id,
                        object: "model",
                        created: 0,
                        owned_by: route.provider_id,
                    });
            }
            let models = public_models.into_values().collect::<Vec<_>>();
            serialize_result(OpenAiModelList {
                object: "list",
                data: models,
            })
        }
        InferenceServiceRequest::StartResponse { request } => {
            let (caller, restricted_request) = prepare_standard_response_request(
                &service,
                &access_registry,
                access_token.as_str(),
                discovery_consumer_profile_id.as_str(),
                request,
            )?;
            let execution_id = service
                .start_response_execution(caller, restricted_request)
                .await
                .map_err(configured_inference_http_error)?;
            Ok(serde_json::json!({ "execution_id": execution_id }))
        }
        InferenceServiceRequest::CreateResponse { request } => {
            let (caller, restricted_request) = prepare_standard_response_request(
                &service,
                &access_registry,
                access_token.as_str(),
                discovery_consumer_profile_id.as_str(),
                request,
            )?;
            let response = service
                .execute_response(caller, restricted_request)
                .await
                .map_err(configured_inference_http_error)?;
            standard_response_json(response)
        }
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

/// Resolves one standard Responses request to the sole authorized configured model purpose.
/// 将一个标准 Responses 请求解析到唯一已授权的配置模型用途。
fn prepare_standard_response_request(
    service: &ConfiguredInferenceService,
    access_registry: &InferenceAccessRegistry,
    access_token: &str,
    consumer_profile_id: &str,
    request: ResponsesCreateRequest,
) -> Result<
    (
        vulcan_ai_contract::InferenceCallerContext,
        ResponsesExecutionRequest,
    ),
    InferenceServiceHttpError,
> {
    if request.store == Some(true) {
        return Err(responses_parameter_error(
            "store",
            "Stored northbound Responses are not implemented.",
        ));
    }
    if request.metadata.is_some() {
        return Err(responses_parameter_error(
            "metadata",
            "Northbound response metadata persistence is not implemented.",
        ));
    }
    if request
        .include
        .iter()
        .any(|field| field != "reasoning.encrypted_content")
    {
        return Err(responses_parameter_error(
            "include",
            "Only `reasoning.encrypted_content` is supported in `include`.",
        ));
    }
    if request
        .include
        .iter()
        .filter(|field| field.as_str() == "reasoning.encrypted_content")
        .count()
        > 1
    {
        return Err(responses_parameter_error(
            "include",
            "The `reasoning.encrypted_content` include value must not be duplicated.",
        ));
    }
    if request.service_tier.is_some() {
        return Err(responses_parameter_error(
            "service_tier",
            "Caller-selected service tiers are not registered by the selected model capability contract.",
        ));
    }
    // Codex transport metadata is intentionally terminated at this authenticated host boundary;
    // upstream tracing and client metadata are host-owned and cannot be caller-injected.
    // Codex 传输元数据刻意终止于这一已认证宿主边界；上游追踪与客户端元数据由宿主持有，
    // 不允许调用方注入。
    let _validated_client_metadata = request.client_metadata.as_ref();
    let inspection_caller = authorize_request(
        access_registry,
        access_token,
        consumer_profile_id,
        InferencePermission::Status,
        None,
    )?;
    let capability = service
        .capability_snapshot(&inspection_caller, consumer_profile_id)
        .map_err(configured_inference_http_error)?;
    let purpose_aliases = capability
        .operations
        .iter()
        .filter(|operation| {
            operation.available
                && operation.operation == AiOperation::ConversationRespond
                && operation.purpose_id.as_deref() == Some(request.model.as_str())
        })
        .filter_map(|operation| operation.purpose_id.clone())
        .collect::<Vec<_>>();
    let matching_purposes = capability
        .operations
        .into_iter()
        .filter(|operation| {
            operation.available
                && operation.operation == AiOperation::ConversationRespond
                && operation
                    .route
                    .as_ref()
                    .is_some_and(|route| route.model_id == request.model)
        })
        .filter_map(|operation| operation.purpose_id)
        .collect::<Vec<_>>();
    let resolved_purposes = if purpose_aliases.is_empty() {
        matching_purposes
    } else {
        purpose_aliases
    };
    let purpose_id = match resolved_purposes.as_slice() {
        [purpose_id] => purpose_id.clone(),
        [] => {
            return Err(InferenceServiceHttpError {
                status: 404,
                code: String::from("model_not_found"),
                message: format!(
                    "Model `{}` is not authorized and routable for this consumer.",
                    request.model
                ),
            });
        }
        _ => {
            return Err(InferenceServiceHttpError {
                status: 409,
                code: String::from("ambiguous_model_route"),
                message: format!(
                    "Model `{}` maps to multiple configured purposes and is not public-routable.",
                    request.model
                ),
            });
        }
    };
    let caller = authorize_request(
        access_registry,
        access_token,
        consumer_profile_id,
        InferencePermission::Llm,
        Some(purpose_id.as_str()),
    )?;
    let (tools, native_tools) = responses_tools(request.tools.as_slice())?;
    let tool_choice = responses_tool_choice(request.tool_choice.as_ref())?;
    let parallel_tool_calls = match request.parallel_tool_calls {
        Some(true) => ParallelToolCallPolicy::Allowed,
        Some(false) => ParallelToolCallPolicy::Disabled,
        None => ParallelToolCallPolicy::ProviderDefault,
    };
    let mut resources = Vec::new();
    let messages = responses_input_messages(
        request.instructions.as_deref(),
        &request.input,
        &mut resources,
    )?;
    let (reasoning_effort, reasoning_output) =
        responses_reasoning_policy(request.reasoning.as_ref())?;
    let structured_output = responses_text_output_policy(request.text.as_ref())?;
    Ok((
        caller,
        ResponsesExecutionRequest {
            contract_version: vulcan_ai_contract::INFERENCE_CONTRACT_VERSION,
            request_id: uuid::Uuid::new_v4().to_string(),
            idempotency_key: None,
            consumer_profile_id: consumer_profile_id.to_string(),
            purpose_id,
            messages,
            tools,
            native_tools,
            tool_choice,
            parallel_tool_calls,
            structured_output,
            prompt_cache: PromptCachePolicy::ProviderDefault,
            prompt_cache_key: request.prompt_cache_key,
            include_reasoning_encrypted_content: request
                .include
                .iter()
                .any(|field| field == "reasoning.encrypted_content"),
            reasoning_effort,
            reasoning_output,
            model_parameters: BTreeMap::new(),
            stop_sequences: Vec::new(),
            resources,
            output_token_limit: request.max_output_tokens,
        },
    ))
}

/// Converts standard Responses string/message input into the restricted transitional message shape.
/// 将标准 Responses 字符串或消息输入转换为过渡期受限消息形态。
fn responses_input_messages(
    instructions: Option<&str>,
    input: &serde_json::Value,
    resources: &mut Vec<ResourceReference>,
) -> Result<Vec<AiMessage>, InferenceServiceHttpError> {
    let mut messages = Vec::new();
    if let Some(instructions) = instructions.filter(|value| !value.is_empty()) {
        messages.push(AiMessage {
            role: MessageRole::System,
            content: vec![ContentItem::Text {
                text: instructions.to_string(),
            }],
            provider_context_items: Vec::new(),
        });
    }
    if let Some(text) = input.as_str() {
        if text.is_empty() {
            return Err(responses_parameter_error(
                "input",
                "Responses string input must not be empty.",
            ));
        }
        messages.push(AiMessage {
            role: MessageRole::User,
            content: vec![ContentItem::Text {
                text: text.to_string(),
            }],
            provider_context_items: Vec::new(),
        });
        return Ok(messages);
    }
    let items = input.as_array().ok_or_else(|| {
        responses_parameter_error("input", "Responses input must be a string or item array.")
    })?;
    for (index, item) in items.iter().enumerate() {
        let object = item.as_object().ok_or_else(|| {
            responses_parameter_error(
                "input",
                format!("Responses input item {index} must be an object."),
            )
        })?;
        let item_type = object
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("message");
        if item_type == "function_call" {
            messages.push(responses_function_call_message(object, index)?);
            continue;
        }
        if item_type == "function_call_output" {
            messages.push(responses_function_output_message(object, index)?);
            continue;
        }
        if item_type == "reasoning" {
            messages.push(responses_reasoning_message(object, index)?);
            continue;
        }
        if matches!(
            item_type,
            "compaction" | "context_compaction" | "web_search_call" | "image_generation_call"
        ) {
            messages.push(responses_provider_context_message(item.clone(), index)?);
            continue;
        }
        if item_type != "message" {
            return Err(responses_parameter_error(
                "input",
                format!("Responses input item type `{item_type}` is not supported."),
            ));
        }
        reject_unknown_responses_keys(
            object,
            &["type", "id", "role", "content", "status"],
            "input",
            format!("Responses message item {index}"),
        )?;
        let role = match object
            .get("role")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                responses_parameter_error(
                    "input",
                    format!("Responses message item {index} is missing `role`."),
                )
            })? {
            "system" => MessageRole::System,
            "developer" => MessageRole::Developer,
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            role => {
                return Err(responses_parameter_error(
                    "input",
                    format!("Responses message role `{role}` is not supported."),
                ));
            }
        };
        validate_optional_responses_item_identity(object, index, "message")?;
        let content = responses_message_content(object.get("content"), index, role, resources)?;
        messages.push(AiMessage {
            role,
            content,
            provider_context_items: Vec::new(),
        });
    }
    if messages.is_empty() {
        return Err(responses_parameter_error(
            "input",
            "Responses input must contain at least one message.",
        ));
    }
    Ok(messages)
}

/// Converts one standard message content value without guessing unsupported media shapes.
/// 转换一个标准消息内容值，且不猜测不支持的媒体形态。
fn responses_message_content(
    content: Option<&serde_json::Value>,
    message_index: usize,
    role: MessageRole,
    resources: &mut Vec<ResourceReference>,
) -> Result<Vec<ContentItem>, InferenceServiceHttpError> {
    let content = content.ok_or_else(|| {
        responses_parameter_error(
            "input",
            format!("Responses message item {message_index} is missing `content`."),
        )
    })?;
    if let Some(text) = content.as_str() {
        return Ok(vec![ContentItem::Text {
            text: text.to_string(),
        }]);
    }
    let parts = content.as_array().ok_or_else(|| {
        responses_parameter_error(
            "input",
            format!("Responses message item {message_index} content must be text or an array."),
        )
    })?;
    let mut converted = Vec::new();
    for (part_index, part) in parts.iter().enumerate() {
        let part_object = part.as_object().ok_or_else(|| {
            responses_parameter_error(
                "input",
                format!(
                    "Responses message item {message_index} content part {part_index} must be an object."
                ),
            )
        })?;
        let part_type = part
            .get("type")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                responses_parameter_error(
                    "input",
                    format!(
                        "Responses message item {message_index} content part {part_index} is missing `type`."
                    ),
                )
            })?;
        match part_type {
            "input_text" | "output_text" | "text" => {
                let role_allows_part = match role {
                    MessageRole::System | MessageRole::Developer | MessageRole::User => {
                        part_type != "output_text"
                    }
                    MessageRole::Assistant => part_type != "input_text",
                    MessageRole::Tool => false,
                };
                if !role_allows_part {
                    return Err(responses_parameter_error(
                        "input",
                        format!(
                            "Responses message item {message_index} role `{role:?}` cannot contain `{part_type}`."
                        ),
                    ));
                }
                reject_unknown_responses_keys(
                    part_object,
                    &["type", "text", "annotations", "logprobs"],
                    "input",
                    format!("Responses message item {message_index} content part {part_index}"),
                )?;
                let text = part
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        responses_parameter_error(
                            "input",
                            format!(
                                "Responses message item {message_index} content part {part_index} is missing text."
                            ),
                        )
                    })?;
                converted.push(ContentItem::Text {
                    text: text.to_string(),
                });
                validate_ignored_output_text_metadata(part_object, message_index, part_index)?;
            }
            "input_image" => {
                if role != MessageRole::User {
                    return Err(responses_parameter_error(
                        "input",
                        format!(
                            "Responses message item {message_index} non-user role cannot contain input images."
                        ),
                    ));
                }
                let resource = responses_image_resource(part, message_index, part_index)?;
                resources.push(resource.clone());
                converted.push(ContentItem::Resource { resource });
            }
            "input_file" => {
                if role != MessageRole::User {
                    return Err(responses_parameter_error(
                        "input",
                        format!(
                            "Responses message item {message_index} non-user role cannot contain input files."
                        ),
                    ));
                }
                let resource = responses_file_resource(part, message_index, part_index)?;
                resources.push(resource.clone());
                converted.push(ContentItem::Resource { resource });
            }
            "refusal" => {
                if role != MessageRole::Assistant {
                    return Err(responses_parameter_error(
                        "input",
                        format!(
                            "Responses message item {message_index} non-assistant role cannot contain refusal output."
                        ),
                    ));
                }
                reject_unknown_responses_keys(
                    part_object,
                    &["type", "refusal"],
                    "input",
                    format!("Responses message item {message_index} content part {part_index}"),
                )?;
                let text = part_object
                    .get("refusal")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        responses_parameter_error(
                            "input",
                            format!(
                                "Responses message item {message_index} refusal part {part_index} is missing refusal text."
                            ),
                        )
                    })?;
                converted.push(ContentItem::Refusal {
                    text: text.to_string(),
                });
            }
            _ => {
                return Err(responses_parameter_error(
                    "input",
                    format!("Responses content part type `{part_type}` is not supported."),
                ));
            }
        }
    }
    if converted.is_empty() {
        return Err(responses_parameter_error(
            "input",
            format!("Responses message item {message_index} content must not be empty."),
        ));
    }
    Ok(converted)
}

/// Converts standard Responses tool declarations into provider-neutral function and native tools.
/// 将标准 Responses 工具声明转换为供应商无关函数工具与原生工具。
fn responses_tools(
    tools: &[serde_json::Value],
) -> Result<(Vec<ToolDefinition>, Vec<NativeToolKind>), InferenceServiceHttpError> {
    let mut functions = Vec::new();
    let mut native = Vec::new();
    for (index, tool) in tools.iter().enumerate() {
        let object = tool.as_object().ok_or_else(|| {
            responses_parameter_error("tools", format!("Tool {index} must be an object."))
        })?;
        let tool_type = object
            .get("type")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                responses_parameter_error("tools", format!("Tool {index} is missing `type`."))
            })?;
        match tool_type {
            "function" => {
                reject_unknown_responses_keys(
                    object,
                    &["type", "name", "description", "parameters", "strict"],
                    "tools",
                    format!("Function tool {index}"),
                )?;
                let strict = object
                    .get("strict")
                    .map(|value| {
                        value.as_bool().ok_or_else(|| {
                            responses_parameter_error(
                                "tools",
                                format!("Function tool {index} `strict` must be a boolean."),
                            )
                        })
                    })
                    .transpose()?
                    .unwrap_or(false);
                let name = required_responses_string(object, "name", "tools", index)?;
                let description = object
                    .get("description")
                    .map(|value| {
                        value.as_str().map(str::to_string).ok_or_else(|| {
                            responses_parameter_error(
                                "tools",
                                format!("Function tool {index} `description` must be a string."),
                            )
                        })
                    })
                    .transpose()?
                    .unwrap_or_default();
                let input_schema = object
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));
                if !input_schema.is_object() {
                    return Err(responses_parameter_error(
                        "tools",
                        format!("Function tool {index} `parameters` must be an object schema."),
                    ));
                }
                if functions
                    .iter()
                    .any(|tool: &ToolDefinition| tool.name == name)
                {
                    return Err(responses_parameter_error(
                        "tools",
                        format!("Function tool name `{name}` is duplicated."),
                    ));
                }
                functions.push(ToolDefinition {
                    name,
                    description,
                    input_schema,
                    strict,
                });
            }
            "web_search" | "web_search_preview" => {
                validate_native_tool_keys(object, index)?;
                if native.contains(&NativeToolKind::WebSearch) {
                    return Err(responses_parameter_error(
                        "tools",
                        "Web search tool is duplicated.",
                    ));
                }
                native.push(NativeToolKind::WebSearch);
            }
            "code_interpreter" => {
                validate_native_tool_keys(object, index)?;
                if native.contains(&NativeToolKind::CodeInterpreter) {
                    return Err(responses_parameter_error(
                        "tools",
                        "Code interpreter tool is duplicated.",
                    ));
                }
                native.push(NativeToolKind::CodeInterpreter);
            }
            "image_generation" => {
                validate_native_tool_keys(object, index)?;
                if native.contains(&NativeToolKind::ImageGeneration) {
                    return Err(responses_parameter_error(
                        "tools",
                        "Image generation tool is duplicated.",
                    ));
                }
                native.push(NativeToolKind::ImageGeneration);
            }
            _ => {
                return Err(responses_parameter_error(
                    "tools",
                    format!("Responses tool type `{tool_type}` is not supported."),
                ));
            }
        }
    }
    Ok((functions, native))
}

/// Rejects native-tool configuration fields that cannot be represented losslessly.
/// 拒绝无法无损表达的原生工具配置字段。
fn validate_native_tool_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    index: usize,
) -> Result<(), InferenceServiceHttpError> {
    if let Some(field) = object.keys().find(|field| field.as_str() != "type") {
        return Err(responses_parameter_error(
            "tools",
            format!(
                "Native tool {index} field `{field}` is not supported by this compatibility level."
            ),
        ));
    }
    Ok(())
}

/// Converts the standard Responses tool-choice value into the shared policy.
/// 将标准 Responses 工具选择值转换为共享策略。
fn responses_tool_choice(
    choice: Option<&serde_json::Value>,
) -> Result<ToolChoicePolicy, InferenceServiceHttpError> {
    let Some(choice) = choice else {
        return Ok(ToolChoicePolicy::Auto);
    };
    if let Some(choice) = choice.as_str() {
        return match choice {
            "auto" => Ok(ToolChoicePolicy::Auto),
            "required" => Ok(ToolChoicePolicy::Required),
            "none" => Ok(ToolChoicePolicy::Disabled),
            _ => Err(responses_parameter_error(
                "tool_choice",
                format!("Tool choice `{choice}` is not supported."),
            )),
        };
    }
    let object = choice.as_object().ok_or_else(|| {
        responses_parameter_error("tool_choice", "Tool choice must be a string or object.")
    })?;
    reject_unknown_responses_keys(
        object,
        &["type", "name"],
        "tool_choice",
        "Function tool choice",
    )?;
    if object.get("type").and_then(serde_json::Value::as_str) != Some("function") {
        return Err(responses_parameter_error(
            "tool_choice",
            "Object tool choice must have `type: function`.",
        ));
    }
    let name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| {
            responses_parameter_error("tool_choice", "Function tool choice is missing `name`.")
        })?;
    Ok(ToolChoicePolicy::Specific {
        tool_name: name.to_string(),
    })
}

/// Converts one standard function-call input item into an assistant message.
/// 将一项标准函数调用输入项目转换为助手消息。
fn responses_function_call_message(
    object: &serde_json::Map<String, serde_json::Value>,
    index: usize,
) -> Result<AiMessage, InferenceServiceHttpError> {
    reject_unknown_responses_keys(
        object,
        &["type", "id", "status", "call_id", "name", "arguments"],
        "input",
        format!("Function call item {index}"),
    )?;
    validate_optional_responses_item_identity(object, index, "function call")?;
    let call_id = required_responses_string(object, "call_id", "input", index)?;
    let name = required_responses_string(object, "name", "input", index)?;
    let arguments_text = required_responses_string(object, "arguments", "input", index)?;
    let arguments = serde_json::from_str(arguments_text.as_str()).map_err(|error| {
        responses_parameter_error(
            "input",
            format!("Function call item {index} has invalid JSON arguments: {error}"),
        )
    })?;
    Ok(AiMessage {
        role: MessageRole::Assistant,
        content: vec![ContentItem::ToolCall {
            call: ToolCall {
                id: call_id,
                name,
                arguments,
                signature: None,
            },
        }],
        provider_context_items: Vec::new(),
    })
}

/// Converts one standard function-call output item into a tool message.
/// 将一项标准函数调用输出项目转换为工具消息。
fn responses_function_output_message(
    object: &serde_json::Map<String, serde_json::Value>,
    index: usize,
) -> Result<AiMessage, InferenceServiceHttpError> {
    reject_unknown_responses_keys(
        object,
        &["type", "id", "call_id", "output"],
        "input",
        format!("Function output item {index}"),
    )?;
    validate_optional_responses_item_identity(object, index, "function output")?;
    let call_id = required_responses_string(object, "call_id", "input", index)?;
    let output = object.get("output").cloned().ok_or_else(|| {
        responses_parameter_error(
            "input",
            format!("Function output item {index} is missing `output`."),
        )
    })?;
    validate_responses_function_output(&output, index)?;
    Ok(AiMessage {
        role: MessageRole::Tool,
        content: vec![ContentItem::ToolResult {
            result: ToolResult {
                call_id,
                is_error: false,
                content: output,
            },
        }],
        provider_context_items: Vec::new(),
    })
}

/// Converts one standard reasoning replay item into provider-neutral assistant context.
/// 将一项标准推理重放项目转换为供应商无关的助手上下文。
///
/// # Parameters
/// 参数
/// `object` is the validated input object and `index` identifies its request position.
/// `object` 是已校验的输入对象，`index` 标识其请求位置。
///
/// # Returns
/// 返回值
/// An assistant message retaining public summary text and provenance-wrapped encrypted content.
/// 返回保留公共摘要文本和带来源封装加密内容的助手消息。
fn responses_reasoning_message(
    object: &serde_json::Map<String, serde_json::Value>,
    index: usize,
) -> Result<AiMessage, InferenceServiceHttpError> {
    reject_unknown_responses_keys(
        object,
        &[
            "type",
            "id",
            "summary",
            "content",
            "encrypted_content",
            "status",
        ],
        "input",
        format!("Reasoning item {index}"),
    )?;
    validate_optional_responses_item_identity(object, index, "reasoning")?;
    let mut text = String::new();
    for field in ["summary", "content"] {
        let Some(parts) = object.get(field) else {
            continue;
        };
        let parts = parts.as_array().ok_or_else(|| {
            responses_parameter_error(
                "input",
                format!("Reasoning item {index} `{field}` must be an array."),
            )
        })?;
        for (part_index, part) in parts.iter().enumerate() {
            let part = part.as_object().ok_or_else(|| {
                responses_parameter_error(
                    "input",
                    format!(
                        "Reasoning item {index} `{field}` part {part_index} must be an object."
                    ),
                )
            })?;
            reject_unknown_responses_keys(
                part,
                &["type", "text"],
                "input",
                format!("Reasoning item {index} `{field}` part {part_index}"),
            )?;
            let part_type = part.get("type").and_then(serde_json::Value::as_str);
            if !matches!(part_type, Some("summary_text" | "reasoning_text")) {
                return Err(responses_parameter_error(
                    "input",
                    format!(
                        "Reasoning item {index} `{field}` part {part_index} has an unsupported type."
                    ),
                ));
            }
            text.push_str(
                part.get("text")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        responses_parameter_error(
                            "input",
                            format!(
                                "Reasoning item {index} `{field}` part {part_index} is missing text."
                            ),
                        )
                    })?,
            );
        }
    }
    let signature = object
        .get("encrypted_content")
        .filter(|value| !value.is_null())
        .map(|value| {
            let signature = value.as_str().ok_or_else(|| {
                responses_parameter_error(
                    "input",
                    format!("Reasoning item {index} `encrypted_content` must be a string."),
                )
            })?;
            vulcan_ai_protocols::wrap_openai_reasoning_signature(signature).ok_or_else(|| {
                responses_parameter_error(
                    "input",
                    format!("Reasoning item {index} has invalid encrypted content."),
                )
            })
        })
        .transpose()?;
    if text.is_empty() && signature.is_none() {
        return Err(responses_parameter_error(
            "input",
            format!("Reasoning item {index} must contain summary, content, or encrypted content."),
        ));
    }
    Ok(AiMessage {
        role: MessageRole::Assistant,
        content: vec![ContentItem::Reasoning { text, signature }],
        provider_context_items: Vec::new(),
    })
}

/// Retains one explicitly replayable Codex Responses item without interpreting provider payloads.
/// 保留一项显式可重放的 Codex Responses 项目，且不解释供应商载荷。
///
/// # Parameters
/// 参数
/// `item` is one public input item and `index` identifies its request position.
/// `item` 是一项公共输入项目，`index` 标识其请求位置。
///
/// # Returns
/// 返回值
/// A route-bound assistant context message accepted only by a compatible protocol encoder.
/// 返回仅由兼容协议编码器接受的路由绑定助手上下文消息。
fn responses_provider_context_message(
    item: serde_json::Value,
    index: usize,
) -> Result<AiMessage, InferenceServiceHttpError> {
    if !vulcan_ai_contract::is_replayable_codex_responses_item(&item) {
        return Err(responses_parameter_error(
            "input",
            format!("Responses provider context item {index} is not replayable."),
        ));
    }
    Ok(AiMessage {
        role: MessageRole::Assistant,
        content: vec![ContentItem::Reasoning {
            text: String::new(),
            signature: None,
        }],
        provider_context_items: vec![item],
    })
}

/// Validates a standard function output string or rich content array.
/// 校验标准函数输出字符串或富内容数组。
///
/// # Parameters
/// 参数
/// `output` is the exact wire value and `item_index` identifies its parent input item.
/// `output` 是精确 Wire 值，`item_index` 标识其父输入项目。
///
/// # Returns
/// 返回值
/// Success only for a string or an array of supported input-text and input-image parts.
/// 仅对字符串或由受支持输入文本与输入图片分片组成的数组返回成功。
fn validate_responses_function_output(
    output: &serde_json::Value,
    item_index: usize,
) -> Result<(), InferenceServiceHttpError> {
    if output.is_string() {
        return Ok(());
    }
    let parts = output.as_array().ok_or_else(|| {
        responses_parameter_error(
            "input",
            format!("Function output item {item_index} output must be a string or content array."),
        )
    })?;
    for (part_index, part) in parts.iter().enumerate() {
        let object = part.as_object().ok_or_else(|| {
            responses_parameter_error(
                "input",
                format!("Function output item {item_index} part {part_index} must be an object."),
            )
        })?;
        match object.get("type").and_then(serde_json::Value::as_str) {
            Some("input_text") => {
                reject_unknown_responses_keys(
                    object,
                    &["type", "text"],
                    "input",
                    format!("Function output item {item_index} part {part_index}"),
                )?;
                if !object.get("text").is_some_and(serde_json::Value::is_string) {
                    return Err(responses_parameter_error(
                        "input",
                        format!(
                            "Function output item {item_index} part {part_index} is missing text."
                        ),
                    ));
                }
            }
            Some("input_image") => {
                responses_image_resource(part, item_index, part_index)?;
            }
            _ => {
                return Err(responses_parameter_error(
                    "input",
                    format!(
                        "Function output item {item_index} part {part_index} has an unsupported type."
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Converts one standard input-image part into a provider-neutral resource.
/// 将一项标准输入图片分片转换为供应商无关资源。
fn responses_image_resource(
    part: &serde_json::Value,
    message_index: usize,
    part_index: usize,
) -> Result<ResourceReference, InferenceServiceHttpError> {
    let object = part
        .as_object()
        .expect("content part was validated as an object");
    let image_url = object
        .get("image_url")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let file_id = object
        .get("file_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty());
    if let Some(field) = object
        .keys()
        .find(|field| !matches!(field.as_str(), "type" | "image_url" | "file_id" | "detail"))
    {
        return Err(responses_parameter_error(
            "input",
            format!("Input image {message_index}:{part_index} field `{field}` is not supported."),
        ));
    }
    if object.contains_key("image_url") && image_url.is_none()
        || object.contains_key("file_id") && file_id.is_none()
    {
        return Err(responses_parameter_error(
            "input",
            format!("Input image {message_index}:{part_index} sources must be non-empty strings."),
        ));
    }
    if image_url.is_some() == file_id.is_some() {
        return Err(responses_parameter_error(
            "input",
            format!(
                "Input image {message_index}:{part_index} must contain exactly one of `image_url` or `file_id`."
            ),
        ));
    }
    let image_detail = object
        .get("detail")
        .map(|value| {
            serde_json::from_value::<ImageInputDetail>(value.clone()).map_err(|_| {
                responses_parameter_error(
                    "input",
                    format!(
                        "Input image {message_index}:{part_index} `detail` must be `auto`, `low`, or `high`."
                    ),
                )
            })
        })
        .transpose()?;
    let source_kind = if file_id.is_some() {
        ResourceSourceKind::ProviderFileId
    } else if image_url.is_some_and(|value| value.starts_with("data:")) {
        ResourceSourceKind::InlineData
    } else {
        ResourceSourceKind::RemoteUrl
    };
    Ok(ResourceReference {
        id: file_id.map_or_else(
            || format!("responses-image-{message_index}-{part_index}"),
            str::to_string,
        ),
        kind: ResourceKind::Image,
        media_type: None,
        source_kind,
        file_name: None,
        image_detail,
        local_path: None,
        url: image_url.map(str::to_string),
    })
}

/// Converts one standard input-file part into a provider-neutral resource.
/// 将一项标准输入文件分片转换为供应商无关资源。
fn responses_file_resource(
    part: &serde_json::Value,
    message_index: usize,
    part_index: usize,
) -> Result<ResourceReference, InferenceServiceHttpError> {
    let object = part
        .as_object()
        .expect("content part was validated as an object");
    if let Some(field) = object.keys().find(|field| {
        !matches!(
            field.as_str(),
            "type" | "file_id" | "file_url" | "file_data" | "filename"
        )
    }) {
        return Err(responses_parameter_error(
            "input",
            format!("Input file {message_index}:{part_index} field `{field}` is not supported."),
        ));
    }
    for field in ["file_id", "file_url", "file_data"] {
        if object.contains_key(field)
            && !object
                .get(field)
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(responses_parameter_error(
                "input",
                format!(
                    "Input file {message_index}:{part_index} `{field}` must be a non-empty string."
                ),
            ));
        }
    }
    let sources = ["file_id", "file_url", "file_data"]
        .into_iter()
        .filter_map(|field| {
            object
                .get(field)
                .and_then(serde_json::Value::as_str)
                .map(|value| (field, value))
        })
        .collect::<Vec<_>>();
    let [(field, value)] = sources.as_slice() else {
        return Err(responses_parameter_error(
            "input",
            format!(
                "Input file {message_index}:{part_index} must contain exactly one of `file_id`, `file_url`, or `file_data`."
            ),
        ));
    };
    let file_name = object
        .get("filename")
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    responses_parameter_error(
                        "input",
                        format!(
                            "Input file {message_index}:{part_index} `filename` must be a non-empty string."
                        ),
                    )
                })
        })
        .transpose()?;
    if *field == "file_data" && file_name.is_none() {
        return Err(responses_parameter_error(
            "input",
            format!(
                "Input file {message_index}:{part_index} with `file_data` requires `filename`."
            ),
        ));
    }
    let source_kind = match *field {
        "file_id" => ResourceSourceKind::ProviderFileId,
        "file_url" => ResourceSourceKind::RemoteUrl,
        "file_data" => ResourceSourceKind::InlineData,
        _ => unreachable!("file source field was validated above"),
    };
    Ok(ResourceReference {
        id: if *field == "file_id" {
            (*value).to_string()
        } else {
            format!("responses-file-{message_index}-{part_index}")
        },
        kind: ResourceKind::File,
        media_type: None,
        source_kind,
        file_name,
        image_detail: None,
        local_path: None,
        url: (*field != "file_id").then(|| (*value).to_string()),
    })
}

/// Reads one required non-empty standard string field from an indexed object.
/// 从带索引对象读取一个必需且非空的标准字符串字段。
fn required_responses_string(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    param: &str,
    index: usize,
) -> Result<String, InferenceServiceHttpError> {
    object
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            responses_parameter_error(
                param,
                format!("Item {index} is missing non-empty string field `{field}`."),
            )
        })
}

/// Rejects object fields outside one explicitly supported Responses compatibility surface.
/// 拒绝超出显式支持 Responses 兼容界面的对象字段。
///
/// # Parameters
/// 参数
/// `object` is the input object, `allowed` is the closed key set, and diagnostics identify scope.
/// `object` 是输入对象，`allowed` 是封闭键集合，诊断参数用于标识作用域。
///
/// # Returns
/// 返回值
/// Success when every key is represented, otherwise one parameter-scoped public error.
/// 每个键均可表达时返回成功，否则返回一项参数作用域公共错误。
fn reject_unknown_responses_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
    param: &str,
    label: impl AsRef<str>,
) -> Result<(), InferenceServiceHttpError> {
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        return Err(responses_parameter_error(
            param,
            format!(
                "{} field `{field}` is not supported by this compatibility level.",
                label.as_ref()
            ),
        ));
    }
    Ok(())
}

/// Validates optional standard item identity fields that are safe to normalize away.
/// 校验可安全归一化移除的可选标准项目身份字段。
///
/// # Parameters
/// 参数
/// `object`, `index`, and `kind` identify one input item and its diagnostic scope.
/// `object`、`index` 与 `kind` 标识一项输入项目及其诊断作用域。
///
/// # Returns
/// 返回值
/// Success for a non-empty string ID and completed status when present.
/// 可选 ID 为非空字符串且可选状态为已完成时返回成功。
fn validate_optional_responses_item_identity(
    object: &serde_json::Map<String, serde_json::Value>,
    index: usize,
    kind: &str,
) -> Result<(), InferenceServiceHttpError> {
    if let Some(id) = object.get("id")
        && !id.as_str().is_some_and(|value| !value.trim().is_empty())
    {
        return Err(responses_parameter_error(
            "input",
            format!("Responses {kind} item {index} `id` must be a non-empty string."),
        ));
    }
    if let Some(status) = object.get("status")
        && status.as_str() != Some("completed")
    {
        return Err(responses_parameter_error(
            "input",
            format!("Responses {kind} item {index} must have status `completed` for replay."),
        ));
    }
    Ok(())
}

/// Validates output-text metadata that is semantically empty and therefore safe to omit.
/// 校验语义为空、因而可安全省略的输出文本元数据。
///
/// # Parameters
/// 参数
/// `part`, `message_index`, and `part_index` identify one standard text content part.
/// `part`、`message_index` 与 `part_index` 标识一个标准文本内容分片。
///
/// # Returns
/// 返回值
/// Success only when annotations and log probabilities are absent, null, or empty arrays.
/// 仅当注解与对数概率缺失、为空值或为空数组时返回成功。
fn validate_ignored_output_text_metadata(
    part: &serde_json::Map<String, serde_json::Value>,
    message_index: usize,
    part_index: usize,
) -> Result<(), InferenceServiceHttpError> {
    for field in ["annotations", "logprobs"] {
        let Some(value) = part.get(field) else {
            continue;
        };
        if value.is_null() || value.as_array().is_some_and(Vec::is_empty) {
            continue;
        }
        return Err(responses_parameter_error(
            "input",
            format!(
                "Responses message item {message_index} content part {part_index} `{field}` cannot be represented losslessly."
            ),
        ));
    }
    Ok(())
}

/// Resolves standard reasoning fields into the existing provider-neutral policy.
/// 将标准 reasoning 字段解析为既有供应商无关策略。
fn responses_reasoning_policy(
    reasoning: Option<&serde_json::Value>,
) -> Result<(Option<ReasoningEffort>, ReasoningOutputPolicy), InferenceServiceHttpError> {
    let Some(reasoning) = reasoning else {
        return Ok((None, ReasoningOutputPolicy::ProviderDefault));
    };
    let object = reasoning.as_object().ok_or_else(|| {
        responses_parameter_error("reasoning", "The `reasoning` field must be an object.")
    })?;
    reject_unknown_responses_keys(
        object,
        &["effort", "summary"],
        "reasoning",
        "Reasoning configuration",
    )?;
    let effort = object
        .get("effort")
        .map(|value| {
            let label = value.as_str().ok_or_else(|| {
                responses_parameter_error("reasoning.effort", "Reasoning effort must be a string.")
            })?;
            serde_json::from_value::<ReasoningEffort>(serde_json::Value::String(label.to_string()))
                .map_err(|_| {
                    responses_parameter_error(
                        "reasoning.effort",
                        format!("Reasoning effort `{label}` is not supported."),
                    )
                })
        })
        .transpose()?;
    if object.contains_key("summary") && !object["summary"].is_string() {
        return Err(responses_parameter_error(
            "reasoning.summary",
            "Reasoning summary mode must be a string.",
        ));
    }
    let output = match object.get("summary").and_then(serde_json::Value::as_str) {
        None => ReasoningOutputPolicy::ProviderDefault,
        Some("auto") => ReasoningOutputPolicy::SummaryAuto,
        Some("detailed") => ReasoningOutputPolicy::Detailed,
        Some(value) => {
            return Err(responses_parameter_error(
                "reasoning.summary",
                format!("Reasoning summary mode `{value}` is not supported."),
            ));
        }
    };
    Ok((effort, output))
}

/// Resolves standard `text.format` into the provider-neutral structured-output policy.
/// 将标准 `text.format` 解析为供应商无关结构化输出策略。
fn responses_text_output_policy(
    text: Option<&serde_json::Value>,
) -> Result<StructuredOutputPolicy, InferenceServiceHttpError> {
    let Some(text) = text else {
        return Ok(StructuredOutputPolicy::None);
    };
    let text = text
        .as_object()
        .ok_or_else(|| responses_parameter_error("text", "The `text` field must be an object."))?;
    reject_unknown_responses_keys(text, &["format", "verbosity"], "text", "Text configuration")?;
    if text.contains_key("verbosity") {
        return Err(responses_parameter_error(
            "text.verbosity",
            "Text verbosity is not registered by the selected model capability contract.",
        ));
    }
    let format = text
        .get("format")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            responses_parameter_error("text.format", "The `text.format` field must be an object.")
        })?;
    match format.get("type").and_then(serde_json::Value::as_str) {
        Some("text") => {
            reject_unknown_responses_keys(format, &["type"], "text.format", "Text format")?;
            Ok(StructuredOutputPolicy::None)
        }
        Some("json_object") => {
            reject_unknown_responses_keys(format, &["type"], "text.format", "JSON object format")?;
            Ok(StructuredOutputPolicy::JsonObject)
        }
        Some("json_schema") => {
            reject_unknown_responses_keys(
                format,
                &["type", "name", "description", "schema", "strict"],
                "text.format",
                "JSON Schema format",
            )?;
            let name = format
                .get("name")
                .and_then(serde_json::Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| {
                    responses_parameter_error(
                        "text.format.name",
                        "JSON Schema output requires `name`.",
                    )
                })?
                .to_string();
            let description = format
                .get("description")
                .map(|value| {
                    value.as_str().map(str::to_string).ok_or_else(|| {
                        responses_parameter_error(
                            "text.format.description",
                            "JSON Schema output description must be a string.",
                        )
                    })
                })
                .transpose()?;
            let schema = format.get("schema").cloned().ok_or_else(|| {
                responses_parameter_error(
                    "text.format.schema",
                    "JSON Schema output requires `schema`.",
                )
            })?;
            if !schema.is_object() {
                return Err(responses_parameter_error(
                    "text.format.schema",
                    "JSON Schema output requires an object schema.",
                ));
            }
            let strict = format
                .get("strict")
                .map(|value| {
                    value.as_bool().ok_or_else(|| {
                        responses_parameter_error(
                            "text.format.strict",
                            "JSON Schema output strict mode must be a boolean.",
                        )
                    })
                })
                .transpose()?
                .unwrap_or(false);
            Ok(StructuredOutputPolicy::JsonSchema {
                name,
                description,
                schema,
                strict,
            })
        }
        Some(value) => Err(responses_parameter_error(
            "text.format.type",
            format!("Text format type `{value}` is not supported."),
        )),
        None => Err(responses_parameter_error(
            "text.format.type",
            "The `text.format.type` field is required.",
        )),
    }
}

/// Converts one completed restricted response into the standard Responses JSON shape.
/// 将一个已完成受限响应转换为标准 Responses JSON 形态。
fn standard_response_json(
    response: vulcan_ai_contract::ResponsesExecutionResult,
) -> Result<serde_json::Value, InferenceServiceHttpError> {
    let response_id = format!("resp_{}", response.request_id.replace('-', ""));
    let message_id = format!("msg_{}", response.request_id.replace('-', ""));
    let output_text = response
        .structured_output
        .as_ref()
        .map(serde_json::Value::to_string)
        .unwrap_or(response.output_text);
    let mut message_content = Vec::new();
    if !output_text.is_empty() {
        message_content.push(serde_json::json!({
            "type": "output_text",
            "text": output_text,
            "annotations": []
        }));
    }
    let mut output = Vec::new();
    if let Some(reasoning) = response.reasoning_text.filter(|value| {
        !value.is_empty()
            && !response
                .output_items
                .iter()
                .any(|item| matches!(item.content, ContentItem::Reasoning { .. }))
    }) {
        output.push(serde_json::json!({
            "id": format!("rs_{}", response.request_id.replace('-', "")),
            "type": "reasoning",
            "summary": [{"type": "summary_text", "text": reasoning}],
            "status": "completed"
        }));
    }
    for item in response.output_items {
        match item.content {
            ContentItem::ToolCall { call } => output.push(serde_json::json!({
                "id": item.item_id,
                "type": "function_call",
                "status": "completed",
                "call_id": call.id,
                "name": call.name,
                "arguments": call.arguments.to_string()
            })),
            ContentItem::Resource { resource }
                if resource.kind == vulcan_ai_contract::ResourceKind::Image =>
            {
                output.push(serde_json::json!({
                    "id": item.item_id,
                    "type": "image_generation_call",
                    "status": "completed",
                    "result": resource.url
                }));
            }
            ContentItem::Refusal { text } => message_content.push(serde_json::json!({
                "type": "refusal",
                "refusal": text
            })),
            ContentItem::Text { text } => message_content.push(serde_json::json!({
                "type": "output_text",
                "text": text,
                "annotations": []
            })),
            ContentItem::StructuredJson { value } => message_content.push(serde_json::json!({
                "type": "output_text",
                "text": value.to_string(),
                "annotations": []
            })),
            ContentItem::Reasoning { text, signature } => {
                let mut reasoning_item = serde_json::json!({
                    "id": item.item_id,
                    "type": "reasoning",
                    "summary": [{"type": "summary_text", "text": text}],
                    "status": "completed"
                });
                if response.include_reasoning_encrypted_content
                    && let Some(signature) = signature
                    && let Some(signature) =
                        vulcan_ai_contract::provider_continuation_payload("openai", &signature)
                {
                    reasoning_item["encrypted_content"] =
                        serde_json::Value::String(signature.to_string());
                }
                output.push(reasoning_item);
            }
            ContentItem::Resource { resource } => {
                return Err(unsupported_provider_output(format!(
                    "Responses execution produced unsupported `{:?}` resource output `{}`.",
                    resource.kind, resource.id
                )));
            }
            ContentItem::ToolResult { .. } => {
                return Err(unsupported_provider_output(
                    "Responses execution produced a tool-result output item.",
                ));
            }
            ContentItem::Citation { .. } => {
                return Err(unsupported_provider_output(
                    "Responses execution produced a citation without standard annotation offsets.",
                ));
            }
        }
    }
    if !message_content.is_empty() {
        output.push(serde_json::json!({
            "id": message_id,
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": message_content
        }));
    }
    Ok(serde_json::json!({
        "id": response_id,
        "object": "response",
        "created_at": current_unix_seconds(),
        "status": "completed",
        "background": false,
        "error": null,
        "incomplete_details": null,
        "instructions": null,
        "max_output_tokens": null,
        "model": response.route.model_id,
        "output": output,
        "parallel_tool_calls": false,
        "previous_response_id": null,
        "store": false,
        "temperature": null,
        "text": {"format": {"type": "text"}},
        "tool_choice": "auto",
        "tools": [],
        "top_p": null,
        "truncation": "disabled",
        "usage": standard_usage_json(&response.usage),
        "user": null,
        "metadata": {}
    }))
}

/// Builds one controlled upstream-output error when normalized content cannot be represented exactly.
/// 当归一化内容无法精确表示时构建一项受控上游输出错误。
fn unsupported_provider_output(message: impl Into<String>) -> InferenceServiceHttpError {
    InferenceServiceHttpError {
        status: 502,
        code: String::from("invalid_provider_output"),
        message: message.into(),
    }
}

/// Converts missing-aware usage into the standard Responses usage object without fabricating zeroes.
/// 将可表达缺失的用量转换为标准 Responses 用量对象，且不伪造零值。
fn standard_usage_json(usage: &RequestUsage) -> serde_json::Value {
    let total_tokens = match usage.total_tokens {
        UsageValue::Unknown => derive_usage_sum(usage.input_tokens, usage.output_tokens),
        known => known,
    };
    serde_json::json!({
        "input_tokens": known_usage_value(usage.input_tokens),
        "input_tokens_details": {
            "cached_tokens": known_usage_value(usage.cache_read_tokens)
        },
        "output_tokens": known_usage_value(usage.output_tokens),
        "output_tokens_details": {
            "reasoning_tokens": known_usage_value(usage.reasoning_tokens)
        },
        "total_tokens": known_usage_value(total_tokens)
    })
}

/// Returns a known usage count or JSON null when the provider omitted the dimension.
/// 返回已知用量计数；供应商省略该维度时返回 JSON null。
fn known_usage_value(value: UsageValue) -> serde_json::Value {
    match value {
        UsageValue::Known { value, .. } => serde_json::json!(value),
        UsageValue::Unknown => serde_json::Value::Null,
    }
}

/// Builds one parameter-scoped public Responses error at the service boundary.
/// 在服务边界构建一个参数作用域的公共 Responses 错误。
fn responses_parameter_error(param: &str, message: impl Into<String>) -> InferenceServiceHttpError {
    InferenceServiceHttpError {
        status: 400,
        code: format!("unsupported_parameter:{param}"),
        message: message.into(),
    }
}

/// Returns the current Unix timestamp in whole seconds for the standard response object.
/// 返回标准响应对象使用的当前 Unix 整秒时间戳。
fn current_unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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

    /// Verifies the standard response derives a total when only disjoint input and output counts exist.
    /// 验证仅存在互不重叠的输入与输出计数时标准响应会推导总量。
    #[test]
    fn standard_usage_derives_total_when_provider_omits_it() {
        let reported = |value| UsageValue::Known {
            value,
            source: vulcan_ai_contract::UsageValueSource::Reported,
        };
        let usage = RequestUsage {
            input_tokens: reported(8),
            output_tokens: reported(2),
            ..RequestUsage::default()
        };

        let value = standard_usage_json(&usage);
        assert_eq!(value["input_tokens"], 8);
        assert_eq!(value["output_tokens"], 2);
        assert_eq!(value["total_tokens"], 10);
    }

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
                        source_kind: ResourceSourceKind::HostResolved,
                        file_name: Some(String::from("memory.txt")),
                        image_detail: None,
                        local_path: Some(String::from("C:\\private\\memory.txt")),
                        url: None,
                    },
                },
            }],
            task: EmbeddingTask::Document,
            dimensions: None,
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            invalid_input_policy: Default::default(),
        };

        let error = validate_external_embedding_request(&request)
            .expect_err("resource-bearing external request must fail");
        assert_eq!(error.status, 400);
        assert_eq!(error.code, "inference_contract_invalid");
    }

    /// Verifies standard reasoning and compaction history retain exact Codex replay state.
    /// 验证标准推理与压缩历史会保留精确 Codex 重放状态。
    #[test]
    fn responses_input_retains_reasoning_and_compaction_replay() {
        let input = serde_json::json!([
            {
                "type": "reasoning",
                "id": "rs_1",
                "summary": [{"type": "summary_text", "text": "checked"}],
                "content": [],
                "encrypted_content": "opaque"
            },
            {
                "type": "compaction",
                "id": "cmp_1",
                "encrypted_content": "checkpoint"
            },
            {
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "continue"}]
            }
        ]);
        let mut resources = Vec::new();

        let messages = responses_input_messages(None, &input, &mut resources)
            .expect("Codex replay input should normalize");

        assert!(matches!(
            &messages[0].content[0],
            ContentItem::Reasoning { text, signature }
                if text == "checked" && signature.as_deref() == Some("openai#opaque")
        ));
        assert_eq!(messages[1].provider_context_items[0]["type"], "compaction");
        assert!(resources.is_empty());
    }

    /// Verifies rich function output arrays remain structured for native Responses forwarding.
    /// 验证富函数输出数组会为原生 Responses 转发保持结构化。
    #[test]
    fn responses_function_output_retains_rich_content_array() {
        let object = serde_json::json!({
            "type": "function_call_output",
            "call_id": "call_1",
            "output": [
                {"type": "input_text", "text": "done"},
                {"type": "input_image", "image_url": "data:image/png;base64,AA=="}
            ]
        });

        let message = responses_function_output_message(
            object.as_object().expect("test output should be an object"),
            0,
        )
        .expect("rich function output should normalize");

        let ContentItem::ToolResult { result } = &message.content[0] else {
            panic!("normalized output must be a tool result");
        };
        assert!(result.content.is_array());
        assert_eq!(result.content[1]["type"], "input_image");
    }

    /// Verifies public Responses input preserves developer precedence instead of collapsing it into system.
    /// 验证公开 Responses 输入保留开发者优先级，而不是将其压成系统角色。
    #[test]
    fn responses_input_preserves_developer_role() {
        let input = serde_json::json!([{
            "type": "message",
            "role": "developer",
            "content": [{"type": "input_text", "text": "follow repository rules"}]
        }]);
        let mut resources = Vec::new();

        let messages = responses_input_messages(None, &input, &mut resources)
            .expect("developer input should normalize");

        assert_eq!(messages[0].role, MessageRole::Developer);
        assert!(resources.is_empty());
    }

    /// Verifies function tools reject unknown fields instead of dropping provider semantics.
    /// 验证函数工具会拒绝未知字段而不是丢弃供应商语义。
    #[test]
    fn responses_tools_reject_unknown_function_fields() {
        let error = responses_tools(&[serde_json::json!({
            "type": "function",
            "name": "lookup",
            "parameters": {"type": "object"},
            "future": true
        })])
        .expect_err("unknown function field must fail");

        assert_eq!(error.status, 400);
        assert!(error.message.contains("future"));
    }

    /// Verifies assistant refusal replay is accepted while user refusal content is rejected.
    /// 验证助手拒绝重放会被接受，而用户拒绝内容会被拒绝。
    #[test]
    fn responses_message_content_enforces_role_specific_parts() {
        let content = serde_json::json!([{"type": "refusal", "refusal": "cannot"}]);
        let mut resources = Vec::new();
        let assistant =
            responses_message_content(Some(&content), 0, MessageRole::Assistant, &mut resources)
                .expect("assistant refusal should normalize");
        assert!(matches!(assistant[0], ContentItem::Refusal { .. }));

        let error = responses_message_content(Some(&content), 0, MessageRole::User, &mut resources)
            .expect_err("user refusal must fail");
        assert!(error.message.contains("non-assistant"));
    }
}
