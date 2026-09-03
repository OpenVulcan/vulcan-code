//! Host adapter that exposes configured inference through the native AI Port.
//! 通过原生 AI Port 暴露已配置推理能力的宿主适配器。

use async_trait::async_trait;
use futures_util::stream;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;
use vulcan_ai_contract::{
    AiOperation, AiStreamEvent, EmbeddingResponse, HostAiCallContext, HostAiCapabilityHandle,
    HostAiCapabilityRegistration, HostAiEmbeddingRequest, HostAiEmbeddingResponse, HostAiError,
    HostAiErrorCode, HostAiEventStream, HostAiExecutionHandle, HostAiExecutionPhase,
    HostAiExecutionSnapshot, HostAiPort, InferenceCallerContext, InferenceCapabilitySnapshot,
    InferencePermission, RerankRequest, RerankResponse, ResponsesExecutionRequest,
    ResponsesExecutionResult,
};

use crate::configured_inference::{
    ConfiguredInferenceError, ConfiguredInferenceErrorCategory, ConfiguredInferenceService,
    ConfiguredLlmExecutionPhase,
};

/// Implements the native HostAiPort over the host's configured inference service.
/// 基于宿主已配置推理服务实现原生 HostAiPort。
pub(crate) struct ConfiguredInferenceHostAiPort {
    /// The only concrete configured-inference executor owned by the service kernel.
    /// service-kernel 唯一持有的具体已配置推理执行器。
    service: Arc<ConfiguredInferenceService>,
    /// Process-local capability handles issued by the composition root.
    /// 由组合根签发的进程内能力句柄注册表。
    capabilities: Mutex<BTreeMap<String, HostAiCapabilityHandle>>,
}

impl ConfiguredInferenceHostAiPort {
    /// Creates a host AI Port backed by one configured inference service.
    /// 创建一个由已配置推理服务作为上游的宿主 AI Port。
    ///
    /// # Parameters
    /// 参数
    /// `service` is retained privately and is never exposed to internal consumers.
    /// `service` 仅在适配器内部保留，不向内部消费者暴露。
    ///
    /// # Returns
    /// 返回值
    /// A process-local native AI Port.
    /// 一个进程内原生 AI Port。
    pub(crate) fn new(service: Arc<ConfiguredInferenceService>) -> Self {
        Self {
            service,
            capabilities: Mutex::new(BTreeMap::new()),
        }
    }

    /// Validates a registration identity before querying configured capabilities.
    /// 在查询已配置能力前校验注册身份。
    fn validate_registration(
        registration: &HostAiCapabilityRegistration,
    ) -> Result<(), HostAiError> {
        for (field, value) in [
            ("registration_id", registration.registration_id.as_str()),
            ("owner_id", registration.owner_id.as_str()),
            (
                "consumer_profile_id",
                registration.consumer_profile_id.as_str(),
            ),
            ("caller_id", registration.caller.caller_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(HostAiError::new(
                    HostAiErrorCode::Contract,
                    format!("Host AI {field} must not be empty."),
                ));
            }
        }
        Ok(())
    }

    /// Ensures an operation is available in a registered capability snapshot.
    /// 确保注册能力快照明确提供指定操作。
    fn ensure_capability(
        handle: &HostAiCapabilityHandle,
        operation: AiOperation,
        purpose_id: Option<&str>,
        context: Option<&HostAiCallContext>,
        request_id: Option<&str>,
    ) -> Result<(), HostAiError> {
        if handle.supports(operation, purpose_id) {
            return Ok(());
        }
        let purpose = purpose_id.unwrap_or("<none>");
        let error = HostAiError::new(
            HostAiErrorCode::CapabilityUnavailable,
            format!(
                "Host AI capability does not expose operation {:?} for purpose `{purpose}`.",
                operation
            ),
        );
        Err(match context {
            Some(context) => error.with_request_context(context, request_id),
            None => match request_id {
                Some(request_id) => error.with_request_id(request_id),
                None => error,
            },
        })
    }

    /// Verifies that a handle was issued by this process-local Port registry.
    /// 校验能力句柄确实由当前进程内 Port 注册表签发。
    fn ensure_registered_handle(&self, handle: &HostAiCapabilityHandle) -> Result<(), HostAiError> {
        let capabilities = self.capabilities.lock().map_err(|_| {
            HostAiError::new(
                HostAiErrorCode::Internal,
                "Host AI capability registry is unavailable while locked.",
            )
        })?;
        let registered = capabilities
            .get(handle.registration_id.as_str())
            .ok_or_else(|| {
                HostAiError::new(
                    HostAiErrorCode::OperationNotFound,
                    format!(
                        "Host AI capability registration `{}` was not found.",
                        handle.registration_id
                    ),
                )
            })?;
        if registered.owner_id != handle.owner_id
            || registered.consumer_profile_id != handle.consumer_profile_id
            || registered.caller != handle.caller
        {
            return Err(HostAiError::new(
                HostAiErrorCode::PermissionDenied,
                "Host AI capability handle does not match its registered owner or caller.",
            ));
        }
        Ok(())
    }

    /// Builds a caller context whose deadline is bounded by the operation context.
    /// 构造一个 deadline 不超过操作上下文的调用方上下文。
    fn caller_for_context(
        handle: &HostAiCapabilityHandle,
        context: &HostAiCallContext,
    ) -> InferenceCallerContext {
        let mut caller = handle.caller.clone();
        caller.deadline_unix_ms = context.effective_deadline(caller.deadline_unix_ms);
        caller
    }

    /// Validates one operation context, caller permission, and registered capability.
    /// 校验操作上下文、调用方权限和注册能力。
    fn validate_call(
        &self,
        handle: &HostAiCapabilityHandle,
        context: &HostAiCallContext,
        request_id: &str,
        operation: AiOperation,
        purpose_id: Option<&str>,
        permission: InferencePermission,
    ) -> Result<(), HostAiError> {
        context.validate(request_id)?;
        self.ensure_registered_handle(handle)?;
        if let Err(message) = handle
            .caller
            .authorize(handle.consumer_profile_id.as_str(), permission)
        {
            return Err(HostAiError::new(HostAiErrorCode::PermissionDenied, message)
                .with_request_context(context, Some(request_id)));
        }
        if let Err(message) = handle.caller.authorize(
            handle.consumer_profile_id.as_str(),
            InferencePermission::Status,
        ) {
            return Err(HostAiError::new(HostAiErrorCode::PermissionDenied, message)
                .with_request_context(context, Some(request_id)));
        }
        Self::ensure_capability(
            handle,
            operation,
            purpose_id,
            Some(context),
            Some(request_id),
        )
    }

    /// Produces a cancellation error carrying the current operation identity.
    /// 生成携带当前操作标识的取消错误。
    fn cancelled(context: &HostAiCallContext, request_id: &str) -> HostAiError {
        HostAiError::new(
            HostAiErrorCode::Cancelled,
            "Host AI operation was cancelled.",
        )
        .with_request_context(context, Some(request_id))
    }

    /// Maps a configured-inference error into the provider-neutral HostAiError shape.
    /// 将已配置推理错误映射为供应商无关的 HostAiError 结构。
    fn map_error(
        error: ConfiguredInferenceError,
        context: Option<&HostAiCallContext>,
        request_id: Option<&str>,
        handle: Option<&HostAiExecutionHandle>,
    ) -> HostAiError {
        let mut mapped = match error.provider_error {
            Some(provider_error) => HostAiError::from_execution_error(*provider_error),
            None => HostAiError::new(
                match error.category {
                    ConfiguredInferenceErrorCategory::Contract => HostAiErrorCode::Contract,
                    ConfiguredInferenceErrorCategory::Authorization => {
                        HostAiErrorCode::PermissionDenied
                    }
                    ConfiguredInferenceErrorCategory::Configuration => {
                        HostAiErrorCode::Configuration
                    }
                    ConfiguredInferenceErrorCategory::Limit => HostAiErrorCode::LimitExceeded,
                    ConfiguredInferenceErrorCategory::Execution => HostAiErrorCode::Provider,
                    ConfiguredInferenceErrorCategory::Cancelled => HostAiErrorCode::Cancelled,
                },
                error.message,
            ),
        };
        if let Some(context) = context {
            mapped = mapped.with_request_context(context, request_id);
        } else if let Some(request_id) = request_id {
            mapped = mapped.with_request_id(request_id);
        }
        if let Some(handle) = handle {
            mapped = mapped.with_execution_handle(handle);
        }
        mapped
    }

    /// Validates the response identity and requested dimensions at the host boundary.
    /// 在宿主边界校验响应身份和请求维度。
    fn validate_embedding_response(
        request: &HostAiEmbeddingRequest,
        response: EmbeddingResponse,
        context: &HostAiCallContext,
    ) -> Result<HostAiEmbeddingResponse, HostAiError> {
        response
            .validate_for_request(&request.request)
            .map_err(|message| {
                HostAiError::new(HostAiErrorCode::InvalidRequest, message)
                    .with_request_context(context, Some(request.request.request_id.as_str()))
            })?;
        if request
            .request
            .dimensions
            .is_some_and(|dimensions| response.fingerprint.dimensions != dimensions)
        {
            return Err(HostAiError::new(
                HostAiErrorCode::DimensionMismatch,
                format!(
                    "Embedding response dimensions {} do not match the requested dimensions {:?}.",
                    response.fingerprint.dimensions, request.request.dimensions
                ),
            )
            .with_request_context(context, Some(request.request.request_id.as_str())));
        }
        Ok(HostAiEmbeddingResponse {
            vector_space_id: request.vector_space_id.clone(),
            response,
        })
    }

    /// Validates the minimum identity invariant for a rerank response.
    /// 校验重排响应的最低身份不变量。
    fn validate_rerank_response(
        request: &RerankRequest,
        response: RerankResponse,
        context: &HostAiCallContext,
    ) -> Result<RerankResponse, HostAiError> {
        if response.request_id != request.request_id {
            return Err(HostAiError::new(
                HostAiErrorCode::InvalidRequest,
                "Rerank response request_id does not match the originating request.",
            )
            .with_request_context(context, Some(request.request_id.as_str())));
        }
        Ok(response)
    }

    /// Returns whether a normalized event ends one LLM execution.
    /// 返回规范化事件是否结束一项 LLM 执行。
    fn is_terminal_event(event: &AiStreamEvent) -> bool {
        matches!(
            event,
            AiStreamEvent::ResponseCompleted
                | AiStreamEvent::ResponseFailed { .. }
                | AiStreamEvent::ResponseCancelled
        )
    }
}

#[async_trait]
impl HostAiPort for ConfiguredInferenceHostAiPort {
    /// Registers or replaces one caller-owned capability after host validation.
    /// 宿主校验后注册或替换一项调用方能力。
    fn register_capability(
        &self,
        registration: HostAiCapabilityRegistration,
    ) -> Result<HostAiCapabilityHandle, HostAiError> {
        Self::validate_registration(&registration)?;
        let snapshot = self
            .service
            .capability_snapshot(
                &registration.caller,
                registration.consumer_profile_id.as_str(),
            )
            .map_err(|error| Self::map_error(error, None, None, None))?;
        for requirement in &registration.requirements {
            if requirement.required
                && !snapshot.operations.iter().any(|capability| {
                    capability.operation == requirement.operation
                        && capability.purpose_id.as_deref() == requirement.purpose_id.as_deref()
                        && capability.available
                })
            {
                return Err(HostAiError::new(
                    HostAiErrorCode::CapabilityUnavailable,
                    format!(
                        "Required Host AI operation {:?} for purpose {:?} is unavailable.",
                        requirement.operation, requirement.purpose_id
                    ),
                ));
            }
        }
        let handle = HostAiCapabilityHandle::new(
            registration.registration_id.clone(),
            registration.owner_id.clone(),
            registration.consumer_profile_id.clone(),
            registration.caller,
            snapshot,
        )?;
        let mut capabilities = self.capabilities.lock().map_err(|_| {
            HostAiError::new(
                HostAiErrorCode::Internal,
                "Host AI capability registry is unavailable while locked.",
            )
        })?;
        if let Some(previous) = capabilities.get(handle.registration_id.as_str())
            && (previous.owner_id != handle.owner_id
                || previous.consumer_profile_id != handle.consumer_profile_id)
        {
            return Err(HostAiError::new(
                HostAiErrorCode::PermissionDenied,
                format!(
                    "Host AI capability registration `{}` belongs to another owner.",
                    handle.registration_id
                ),
            ));
        }
        capabilities.insert(handle.registration_id.clone(), handle.clone());
        Ok(handle)
    }

    /// Returns one previously issued capability handle.
    /// 返回此前签发的一项能力句柄。
    fn capability_handle(
        &self,
        registration_id: &str,
    ) -> Result<HostAiCapabilityHandle, HostAiError> {
        let registration_id = registration_id.trim();
        if registration_id.is_empty() {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Host AI registration_id must not be empty.",
            ));
        }
        let capabilities = self.capabilities.lock().map_err(|_| {
            HostAiError::new(
                HostAiErrorCode::Internal,
                "Host AI capability registry is unavailable while locked.",
            )
        })?;
        capabilities.get(registration_id).cloned().ok_or_else(|| {
            HostAiError::new(
                HostAiErrorCode::OperationNotFound,
                format!("Host AI capability registration `{registration_id}` was not found."),
            )
        })
    }

    /// Removes a capability registration; repeated removal is idempotent for cleanup paths.
    /// 移除能力注册；重复移除对清理路径保持幂等。
    fn unregister_capability(&self, registration_id: &str) -> Result<(), HostAiError> {
        let registration_id = registration_id.trim();
        if registration_id.is_empty() {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Host AI registration_id must not be empty.",
            ));
        }
        let mut capabilities = self.capabilities.lock().map_err(|_| {
            HostAiError::new(
                HostAiErrorCode::Internal,
                "Host AI capability registry is unavailable while locked.",
            )
        })?;
        capabilities.remove(registration_id);
        Ok(())
    }

    /// Refreshes a registered capability snapshot after configuration changes.
    /// 配置变化后刷新已注册能力快照。
    fn capability_snapshot(
        &self,
        handle: &HostAiCapabilityHandle,
    ) -> Result<InferenceCapabilitySnapshot, HostAiError> {
        self.ensure_registered_handle(handle)?;
        let snapshot = self
            .service
            .capability_snapshot(&handle.caller, handle.consumer_profile_id.as_str())
            .map_err(|error| Self::map_error(error, None, None, None))?;
        let mut capabilities = self.capabilities.lock().map_err(|_| {
            HostAiError::new(
                HostAiErrorCode::Internal,
                "Host AI capability registry is unavailable while locked.",
            )
        })?;
        if let Some(registered) = capabilities.get_mut(handle.registration_id.as_str()) {
            registered.snapshot = snapshot.clone();
        }
        Ok(snapshot)
    }

    /// Executes one embedding batch with host cancellation and deadline context.
    /// 使用宿主取消和 deadline 上下文执行一个 Embedding 批次。
    async fn create_embeddings(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: HostAiEmbeddingRequest,
    ) -> Result<HostAiEmbeddingResponse, HostAiError> {
        request.validate()?;
        let request_id = request.request.request_id.clone();
        if request.request.consumer_profile_id != handle.consumer_profile_id {
            return Err(HostAiError::new(
                HostAiErrorCode::PermissionDenied,
                "Embedding request profile does not match the capability handle.",
            )
            .with_request_context(&context, Some(request_id.as_str())));
        }
        self.validate_call(
            handle,
            &context,
            request_id.as_str(),
            AiOperation::EmbeddingCreate,
            None,
            InferencePermission::Embedding,
        )?;
        if context.cancellation.is_cancelled() {
            return Err(Self::cancelled(&context, request_id.as_str()));
        }
        let caller = Self::caller_for_context(handle, &context);
        let service = Arc::clone(&self.service);
        let service_request = request.request.clone();
        let cancellation = Arc::clone(&context.cancellation);
        let operation = service.create_embeddings(&caller, service_request);
        tokio::pin!(operation);
        let response = tokio::select! {
            biased;
            result = &mut operation => result.map_err(|error| Self::map_error(error, Some(&context), Some(request_id.as_str()), None))?,
            _ = cancellation.cancelled() => return Err(Self::cancelled(&context, request_id.as_str())),
        };
        Self::validate_embedding_response(&request, response, &context)
    }

    /// Executes one rerank request with host cancellation and deadline context.
    /// 使用宿主取消和 deadline 上下文执行一个重排请求。
    async fn rerank_documents(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: RerankRequest,
    ) -> Result<RerankResponse, HostAiError> {
        request.validate().map_err(|message| {
            HostAiError::new(HostAiErrorCode::Contract, message)
                .with_request_context(&context, Some(request.request_id.as_str()))
        })?;
        if request.consumer_profile_id != handle.consumer_profile_id {
            return Err(HostAiError::new(
                HostAiErrorCode::PermissionDenied,
                "Rerank request profile does not match the capability handle.",
            )
            .with_request_context(&context, Some(request.request_id.as_str())));
        }
        self.validate_call(
            handle,
            &context,
            request.request_id.as_str(),
            AiOperation::RerankDocuments,
            None,
            InferencePermission::Rerank,
        )?;
        if context.cancellation.is_cancelled() {
            return Err(Self::cancelled(&context, request.request_id.as_str()));
        }
        let caller = Self::caller_for_context(handle, &context);
        let service = Arc::clone(&self.service);
        let cancellation = Arc::clone(&context.cancellation);
        let operation = service.rerank_documents(&caller, request.clone());
        tokio::pin!(operation);
        let response = tokio::select! {
            biased;
            result = &mut operation => result.map_err(|error| Self::map_error(error, Some(&context), Some(request.request_id.as_str()), None))?,
            _ = cancellation.cancelled() => return Err(Self::cancelled(&context, request.request_id.as_str())),
        };
        Self::validate_rerank_response(&request, response, &context)
    }

    /// Starts one LLM execution and returns a host-owned lifecycle handle.
    /// 启动一项 LLM 执行并返回宿主持有的生命周期句柄。
    async fn start_response_execution(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: ResponsesExecutionRequest,
    ) -> Result<HostAiExecutionHandle, HostAiError> {
        request.validate().map_err(|message| {
            HostAiError::new(HostAiErrorCode::Contract, message)
                .with_request_context(&context, Some(request.request_id.as_str()))
        })?;
        if request.consumer_profile_id != handle.consumer_profile_id {
            return Err(HostAiError::new(
                HostAiErrorCode::PermissionDenied,
                "LLM request profile does not match the capability handle.",
            )
            .with_request_context(&context, Some(request.request_id.as_str())));
        }
        self.validate_call(
            handle,
            &context,
            request.request_id.as_str(),
            AiOperation::ConversationRespond,
            Some(request.purpose_id.as_str()),
            InferencePermission::Llm,
        )?;
        if context.cancellation.is_cancelled() {
            return Err(Self::cancelled(&context, request.request_id.as_str()));
        }
        let caller = Self::caller_for_context(handle, &context);
        let execution_id = self
            .service
            .start_response_execution(caller, request.clone())
            .await
            .map_err(|error| {
                Self::map_error(
                    error,
                    Some(&context),
                    Some(request.request_id.as_str()),
                    None,
                )
            })?;
        HostAiExecutionHandle::new(
            &context,
            execution_id,
            &request,
            handle.caller.caller_id.clone(),
        )
    }

    /// Returns one host-normalized status snapshot for an owned execution.
    /// 返回一项所属执行的宿主规范化状态快照。
    fn execution_status(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<HostAiExecutionSnapshot, HostAiError> {
        let snapshot = self
            .service
            .execution_status(handle.caller_id.as_str(), handle.execution_id.as_str())
            .map_err(|error| {
                Self::map_error(error, None, Some(handle.request_id.as_str()), Some(handle))
            })?;
        let phase = match snapshot.phase {
            ConfiguredLlmExecutionPhase::Running => HostAiExecutionPhase::Running,
            ConfiguredLlmExecutionPhase::Completed => HostAiExecutionPhase::Completed,
            ConfiguredLlmExecutionPhase::Failed => HostAiExecutionPhase::Failed,
            ConfiguredLlmExecutionPhase::Cancelled => HostAiExecutionPhase::Cancelled,
        };
        let error = snapshot.error.map(|error| {
            Self::map_error(error, None, Some(handle.request_id.as_str()), Some(handle))
        });
        Ok(HostAiExecutionSnapshot {
            execution_id: snapshot.execution_id,
            operation_id: handle.operation_id.clone(),
            request_id: handle.request_id.clone(),
            phase,
            route: Some(snapshot.route),
            error,
        })
    }

    /// Opens a bounded replay-plus-live stream and observes host cancellation.
    /// 打开有界的历史重放加实时事件流并观察宿主取消。
    fn stream_response(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<HostAiEventStream, HostAiError> {
        let (replay, receiver) = self
            .service
            .stream_response(handle.caller_id.as_str(), handle.execution_id.as_str())
            .map_err(|error| {
                Self::map_error(error, None, Some(handle.request_id.as_str()), Some(handle))
            })?;
        let service = Arc::clone(&self.service);
        let handle = handle.clone();
        let state = (VecDeque::from(replay), Some(receiver), handle);
        let stream = stream::unfold(state, move |(mut replay, mut receiver, handle)| {
            let service = Arc::clone(&service);
            async move {
                if let Some(event) = replay.pop_front() {
                    let terminal = Self::is_terminal_event(&event);
                    return Some((
                        Ok(event),
                        (replay, if terminal { None } else { receiver }, handle),
                    ));
                }
                let Some(mut receiver) = receiver.take() else {
                    return None;
                };
                if handle.cancellation().is_cancelled() {
                    let _ = service
                        .cancel_execution(handle.caller_id.as_str(), handle.execution_id.as_str());
                    return Some((
                        Err(HostAiError::new(
                            HostAiErrorCode::Cancelled,
                            "Host AI operation was cancelled.",
                        )
                        .with_execution_handle(&handle)),
                        (VecDeque::new(), None, handle),
                    ));
                }
                let cancellation = handle.cancellation();
                let (next, cancelled) = {
                    let receive = receiver.recv();
                    tokio::pin!(receive);
                    tokio::select! {
                        biased;
                        event = &mut receive => (Some(event), false),
                        _ = cancellation.cancelled() => (None, true),
                    }
                };
                if cancelled {
                    let _ = service
                        .cancel_execution(handle.caller_id.as_str(), handle.execution_id.as_str());
                    return Some((
                        Err(HostAiError::new(
                            HostAiErrorCode::Cancelled,
                            "Host AI operation was cancelled.",
                        )
                        .with_execution_handle(&handle)),
                        (VecDeque::new(), None, handle),
                    ));
                }
                let next = next.expect("non-cancelled Host AI stream poll must have a result");
                match next {
                    Ok(event) => {
                        let terminal = Self::is_terminal_event(&event);
                        Some((
                            Ok(event),
                            (
                                VecDeque::new(),
                                if terminal { None } else { Some(receiver) },
                                handle,
                            ),
                        ))
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => Some((
                        Err(HostAiError::new(
                            HostAiErrorCode::LimitExceeded,
                            format!("Host AI event stream lagged and skipped {skipped} events."),
                        )
                        .with_execution_handle(&handle)),
                        (VecDeque::new(), None, handle),
                    )),
                    Err(broadcast::error::RecvError::Closed) => Some((
                        Err(HostAiError::new(
                            HostAiErrorCode::Provider,
                            "Host AI event stream ended before a terminal event.",
                        )
                        .with_execution_handle(&handle)),
                        (VecDeque::new(), None, handle),
                    )),
                }
            }
        });
        Ok(Box::pin(stream))
    }

    /// Waits for one owned execution and converts terminal errors at the Port boundary.
    /// 等待一项所属执行并在 Port 边界转换终态错误。
    async fn wait_response(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<ResponsesExecutionResult, HostAiError> {
        if handle.cancellation().is_cancelled() {
            let _ = self.cancel_execution(handle);
            return Err(HostAiError::new(
                HostAiErrorCode::Cancelled,
                "Host AI operation was cancelled.",
            )
            .with_execution_handle(handle));
        }
        let service = Arc::clone(&self.service);
        let cancellation = handle.cancellation();
        let wait = service.wait_response(handle.caller_id.as_str(), handle.execution_id.as_str());
        tokio::pin!(wait);
        tokio::select! {
            biased;
            result = &mut wait => result
                .map_err(|error| Self::map_error(error, None, Some(handle.request_id.as_str()), Some(handle))),
            _ = cancellation.cancelled() => {
                let _ = self.cancel_execution(handle);
                Err(HostAiError::new(
                    HostAiErrorCode::Cancelled,
                    "Host AI operation was cancelled.",
                )
                .with_execution_handle(handle))
            }
        }
    }

    /// Requests cooperative cancellation for one owned execution.
    /// 请求协作式取消一项所属执行。
    fn cancel_execution(&self, handle: &HostAiExecutionHandle) -> Result<(), HostAiError> {
        self.service
            .cancel_execution(handle.caller_id.as_str(), handle.execution_id.as_str())
            .map_err(|error| {
                Self::map_error(error, None, Some(handle.request_id.as_str()), Some(handle))
            })
    }
}
