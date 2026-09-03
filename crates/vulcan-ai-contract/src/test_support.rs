//! Runtime-free mock implementation of the native HostAiPort.
//! 原生 HostAiPort 的无运行时 mock 实现。

// The public mock intentionally mirrors the frozen diagnostic-rich HostAiError signatures.
// 公共 mock 有意镜像已冻结且富含诊断信息的 HostAiError 签名。
#![allow(
    clippy::result_large_err,
    reason = "the mock must preserve the frozen HostAiError boundary contract"
)]

use futures_core::Stream;
use std::{
    collections::{BTreeMap, VecDeque},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use crate::{
    AiOperation, AiStreamEvent, EmbeddingResponse, HostAiCallContext, HostAiCapabilityHandle,
    HostAiCapabilityRegistration, HostAiEmbeddingRequest, HostAiEmbeddingResponse, HostAiError,
    HostAiErrorCode, HostAiEventStream, HostAiExecutionHandle, HostAiExecutionPhase,
    HostAiExecutionSnapshot, HostAiPort, InferenceCapabilitySnapshot, InferencePermission,
    RerankRequest, RerankResponse, ResponsesExecutionRequest, ResponsesExecutionResult,
};

/// Provides deterministic host capability and response fixtures to downstream unit tests.
/// 为下游单元测试提供确定性的宿主能力和响应夹具。
#[derive(Clone, Default)]
pub struct MockHostAiPort {
    /// Shared mutable fixture state so the Port can be cloned into async test tasks.
    /// 可在异步测试任务之间克隆的共享可变夹具状态。
    state: Arc<Mutex<MockHostAiState>>,
}

/// Stores mock capability snapshots, typed responses, and execution records.
/// 存储 mock 能力快照、类型化响应和执行记录。
#[derive(Default)]
struct MockHostAiState {
    /// Capability snapshots keyed by consumer profile identity.
    /// 按消费 Profile 标识索引的能力快照。
    snapshots: BTreeMap<String, InferenceCapabilitySnapshot>,
    /// Registered host-issued capability handles.
    /// 已注册的宿主签发能力句柄。
    capabilities: BTreeMap<String, HostAiCapabilityHandle>,
    /// Embedding fixtures keyed by request identity.
    /// 按请求标识索引的 Embedding 夹具。
    embeddings: BTreeMap<String, EmbeddingResponse>,
    /// Rerank fixtures keyed by request identity.
    /// 按请求标识索引的重排夹具。
    reranks: BTreeMap<String, RerankResponse>,
    /// LLM fixtures keyed by request identity.
    /// 按请求标识索引的 LLM 夹具。
    responses: BTreeMap<String, MockResponseFixture>,
    /// Owned asynchronous executions keyed by generated execution identity.
    /// 按生成的执行标识索引的所属异步执行。
    executions: BTreeMap<String, MockExecution>,
    /// Monotonic identifier used to make mock execution handles deterministic.
    /// 用于生成确定性 mock 执行句柄的单调标识。
    next_execution: u64,
}

/// Retains one aggregate LLM response and its normalized stream events.
/// 保存一项聚合 LLM 响应及其规范化事件流。
#[derive(Clone)]
struct MockResponseFixture {
    /// Aggregate response returned by wait_response.
    /// 由 wait_response 返回的聚合响应。
    response: ResponsesExecutionResult,
    /// Replay-plus-live events returned by stream_response.
    /// 由 stream_response 返回的历史重放加实时事件。
    events: Vec<AiStreamEvent>,
}

/// Retains the lifecycle state of one mock execution.
/// 保存一项 mock 执行的生命周期状态。
#[derive(Clone)]
struct MockExecution {
    /// Aggregate response available after successful completion.
    /// 成功完成后可取得的聚合响应。
    response: Option<ResponsesExecutionResult>,
    /// Normalized events associated with the execution.
    /// 与执行关联的规范化事件。
    events: Vec<AiStreamEvent>,
    /// Current host lifecycle phase.
    /// 当前宿主生命周期阶段。
    phase: HostAiExecutionPhase,
    /// Terminal error when the mock execution fails or is cancelled.
    /// mock 执行失败或取消时的终态错误。
    error: Option<HostAiError>,
}

/// Implements a one-shot stream over deterministic mock events.
/// 基于确定性 mock 事件实现一次性事件流。
struct MockEventStream {
    /// Remaining normalized events.
    /// 尚未返回的规范化事件。
    events: VecDeque<Result<AiStreamEvent, HostAiError>>,
}

impl Stream for MockEventStream {
    /// One normalized event or host-boundary error.
    /// 一项规范化事件或宿主边界错误。
    type Item = Result<AiStreamEvent, HostAiError>;

    /// Returns the next fixture event without blocking a runtime.
    /// 不阻塞运行时地返回下一项夹具事件。
    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.events.pop_front())
    }
}

impl MockHostAiPort {
    /// Creates an empty mock Port whose fixtures must be configured explicitly.
    /// 创建一个空 mock Port，所有夹具都必须显式配置。
    ///
    /// # Returns
    /// 返回值
    /// A cloneable runtime-free mock Port.
    /// 一个可克隆且不依赖运行时的 mock Port。
    pub fn new() -> Self {
        Self::default()
    }

    /// Installs one capability snapshot used by subsequent registrations.
    /// 安装一项供后续注册使用的能力快照。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` is the explicit capability evidence for one consumer profile.
    /// `snapshot` 是一个消费 Profile 的显式能力证据。
    ///
    /// # Returns
    /// 返回值
    /// Success or a deterministic contract error when the profile identity is empty.
    /// 成功，或 Profile 标识为空时返回确定性的契约错误。
    pub fn set_capability_snapshot(
        &self,
        snapshot: InferenceCapabilitySnapshot,
    ) -> Result<(), HostAiError> {
        if snapshot.consumer_profile_id.trim().is_empty() {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Mock Host AI capability profile must not be empty.",
            ));
        }
        let mut state = self.lock_state()?;
        state
            .snapshots
            .insert(snapshot.consumer_profile_id.clone(), snapshot);
        Ok(())
    }

    /// Installs one embedding response fixture keyed by request identity.
    /// 按请求标识安装一项 Embedding 响应夹具。
    ///
    /// # Parameters
    /// 参数
    /// `response` is returned after the Port validates it against the incoming request.
    /// `response` 会在 Port 按传入请求校验后返回。
    pub fn set_embedding_response(&self, response: EmbeddingResponse) -> Result<(), HostAiError> {
        if response.request_id.trim().is_empty() {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Mock embedding response request_id must not be empty.",
            ));
        }
        let mut state = self.lock_state()?;
        state
            .embeddings
            .insert(response.request_id.clone(), response);
        Ok(())
    }

    /// Installs one rerank response fixture keyed by request identity.
    /// 按请求标识安装一项重排响应夹具。
    ///
    /// # Parameters
    /// 参数
    /// `response` is returned after request identity validation.
    /// `response` 会在请求标识校验后返回。
    pub fn set_rerank_response(&self, response: RerankResponse) -> Result<(), HostAiError> {
        if response.request_id.trim().is_empty() {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Mock rerank response request_id must not be empty.",
            ));
        }
        let mut state = self.lock_state()?;
        state.reranks.insert(response.request_id.clone(), response);
        Ok(())
    }

    /// Installs one LLM aggregate response and normalized stream fixture.
    /// 安装一项 LLM 聚合响应及规范化事件流夹具。
    ///
    /// # Parameters
    /// 参数
    /// `response` is returned by wait_response; `events` are replayed by stream_response.
    /// `response` 由 wait_response 返回；`events` 由 stream_response 重放。
    pub fn set_response(
        &self,
        response: ResponsesExecutionResult,
        events: Vec<AiStreamEvent>,
    ) -> Result<(), HostAiError> {
        if response.request_id.trim().is_empty() {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Mock LLM response request_id must not be empty.",
            ));
        }
        let mut state = self.lock_state()?;
        state.responses.insert(
            response.request_id.clone(),
            MockResponseFixture { response, events },
        );
        Ok(())
    }

    /// Locks fixture state and converts poisoning into a stable host error.
    /// 锁定夹具状态并将 poisoned 锁转换为稳定宿主错误。
    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, MockHostAiState>, HostAiError> {
        self.state.lock().map_err(|_| {
            HostAiError::new(
                HostAiErrorCode::Internal,
                "Mock Host AI fixture state is unavailable while locked.",
            )
        })
    }

    /// Ensures a handle is still registered with the same owner and caller.
    /// 确保句柄仍以相同所有者和调用方注册。
    fn ensure_registered_handle(&self, handle: &HostAiCapabilityHandle) -> Result<(), HostAiError> {
        let state = self.lock_state()?;
        let registered = state
            .capabilities
            .get(handle.registration_id.as_str())
            .ok_or_else(|| {
                HostAiError::new(
                    HostAiErrorCode::OperationNotFound,
                    format!(
                        "Mock Host AI capability registration `{}` was not found.",
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
                "Mock Host AI capability handle does not match its registration.",
            ));
        }
        Ok(())
    }

    /// Validates one request against handle ownership and operation capability.
    /// 按句柄所有权和操作能力校验一项请求。
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
        handle
            .caller
            .authorize(handle.consumer_profile_id.as_str(), permission)
            .map_err(|message| {
                HostAiError::new(HostAiErrorCode::PermissionDenied, message)
                    .with_request_context(context, Some(request_id))
            })?;
        if !handle.supports(operation, purpose_id) {
            return Err(HostAiError::new(
                HostAiErrorCode::CapabilityUnavailable,
                format!(
                    "Mock Host AI capability does not expose operation {:?} for purpose {:?}.",
                    operation, purpose_id
                ),
            )
            .with_request_context(context, Some(request_id)));
        }
        Ok(())
    }

    /// Returns a cancellation error with operation correlation.
    /// 返回携带操作关联信息的取消错误。
    fn cancelled(context: &HostAiCallContext, request_id: &str) -> HostAiError {
        HostAiError::new(
            HostAiErrorCode::Cancelled,
            "Mock Host AI operation was cancelled.",
        )
        .with_request_context(context, Some(request_id))
    }
}

#[async_trait::async_trait]
impl HostAiPort for MockHostAiPort {
    /// Registers one explicit mock capability snapshot.
    /// 注册一项显式 mock 能力快照。
    fn register_capability(
        &self,
        registration: HostAiCapabilityRegistration,
    ) -> Result<HostAiCapabilityHandle, HostAiError> {
        if registration.registration_id.trim().is_empty()
            || registration.owner_id.trim().is_empty()
            || registration.consumer_profile_id.trim().is_empty()
        {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Mock Host AI registration identities must not be empty.",
            ));
        }
        let snapshot = {
            let state = self.lock_state()?;
            state
                .snapshots
                .get(registration.consumer_profile_id.as_str())
                .cloned()
                .ok_or_else(|| {
                    HostAiError::new(
                        HostAiErrorCode::Configuration,
                        format!(
                            "Mock Host AI capability snapshot for profile `{}` was not configured.",
                            registration.consumer_profile_id
                        ),
                    )
                })?
        };
        registration
            .caller
            .authorize(
                registration.consumer_profile_id.as_str(),
                InferencePermission::Status,
            )
            .map_err(|message| HostAiError::new(HostAiErrorCode::PermissionDenied, message))?;
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
                        "Required mock Host AI operation {:?} for purpose {:?} is unavailable.",
                        requirement.operation, requirement.purpose_id
                    ),
                ));
            }
        }
        let handle = HostAiCapabilityHandle::new(
            registration.registration_id,
            registration.owner_id,
            registration.consumer_profile_id,
            registration.caller,
            snapshot,
        )?;
        let mut state = self.lock_state()?;
        if let Some(previous) = state.capabilities.get(handle.registration_id.as_str())
            && (previous.owner_id != handle.owner_id
                || previous.consumer_profile_id != handle.consumer_profile_id)
        {
            return Err(HostAiError::new(
                HostAiErrorCode::PermissionDenied,
                "Mock Host AI registration belongs to another owner.",
            ));
        }
        state
            .capabilities
            .insert(handle.registration_id.clone(), handle.clone());
        Ok(handle)
    }

    /// Returns a previously registered mock capability handle.
    /// 返回此前注册的 mock 能力句柄。
    fn capability_handle(
        &self,
        registration_id: &str,
    ) -> Result<HostAiCapabilityHandle, HostAiError> {
        let state = self.lock_state()?;
        state
            .capabilities
            .get(registration_id)
            .cloned()
            .ok_or_else(|| {
                HostAiError::new(
                    HostAiErrorCode::OperationNotFound,
                    format!(
                        "Mock Host AI capability registration `{registration_id}` was not found."
                    ),
                )
            })
    }

    /// Removes one mock capability registration idempotently.
    /// 幂等地移除一项 mock 能力注册。
    fn unregister_capability(&self, registration_id: &str) -> Result<(), HostAiError> {
        if registration_id.trim().is_empty() {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Mock Host AI registration_id must not be empty.",
            ));
        }
        self.lock_state()?.capabilities.remove(registration_id);
        Ok(())
    }

    /// Refreshes one registered mock capability from its configured snapshot.
    /// 从已配置快照刷新一项已注册 mock 能力。
    fn capability_snapshot(
        &self,
        handle: &HostAiCapabilityHandle,
    ) -> Result<InferenceCapabilitySnapshot, HostAiError> {
        self.ensure_registered_handle(handle)?;
        let mut state = self.lock_state()?;
        let snapshot = state
            .snapshots
            .get(handle.consumer_profile_id.as_str())
            .cloned()
            .ok_or_else(|| {
                HostAiError::new(
                    HostAiErrorCode::Configuration,
                    format!(
                        "Mock Host AI capability snapshot for profile `{}` was not configured.",
                        handle.consumer_profile_id
                    ),
                )
            })?;
        if let Some(registered) = state.capabilities.get_mut(handle.registration_id.as_str()) {
            registered.snapshot = snapshot.clone();
        }
        Ok(snapshot)
    }

    /// Returns an explicitly configured embedding fixture.
    /// 返回显式配置的 Embedding 夹具。
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
                "Mock embedding request profile does not match the capability handle.",
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
        let response = self
            .lock_state()?
            .embeddings
            .get(request_id.as_str())
            .cloned()
            .ok_or_else(|| {
                HostAiError::new(
                    HostAiErrorCode::OperationNotFound,
                    format!("Mock embedding fixture `{request_id}` was not configured."),
                )
                .with_request_context(&context, Some(request_id.as_str()))
            })?;
        response
            .validate_for_request(&request.request)
            .map_err(|message| {
                HostAiError::new(HostAiErrorCode::InvalidRequest, message)
                    .with_request_context(&context, Some(request_id.as_str()))
            })?;
        if request
            .request
            .dimensions
            .is_some_and(|dimensions| response.fingerprint.dimensions != dimensions)
        {
            return Err(HostAiError::new(
                HostAiErrorCode::DimensionMismatch,
                "Mock embedding fixture dimensions do not match the request.",
            )
            .with_request_context(&context, Some(request_id.as_str())));
        }
        Ok(HostAiEmbeddingResponse {
            vector_space_id: request.vector_space_id,
            response,
        })
    }

    /// Returns an explicitly configured rerank fixture.
    /// 返回显式配置的重排夹具。
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
                "Mock rerank request profile does not match the capability handle.",
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
        let response = self
            .lock_state()?
            .reranks
            .get(request.request_id.as_str())
            .cloned()
            .ok_or_else(|| {
                HostAiError::new(
                    HostAiErrorCode::OperationNotFound,
                    format!(
                        "Mock rerank fixture `{}` was not configured.",
                        request.request_id
                    ),
                )
                .with_request_context(&context, Some(request.request_id.as_str()))
            })?;
        response.validate_for_request(&request).map_err(|message| {
            HostAiError::new(HostAiErrorCode::InvalidRequest, message)
                .with_request_context(&context, Some(request.request_id.as_str()))
        })?;
        Ok(response)
    }

    /// Starts one deterministic completed mock LLM execution.
    /// 启动一项确定性完成的 mock LLM 执行。
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
                "Mock LLM request profile does not match the capability handle.",
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
        let fixture = self
            .lock_state()?
            .responses
            .get(request.request_id.as_str())
            .cloned()
            .ok_or_else(|| {
                HostAiError::new(
                    HostAiErrorCode::OperationNotFound,
                    format!(
                        "Mock LLM fixture `{}` was not configured.",
                        request.request_id
                    ),
                )
                .with_request_context(&context, Some(request.request_id.as_str()))
            })?;
        let mut state = self.lock_state()?;
        state.next_execution = state.next_execution.saturating_add(1);
        let execution_id = format!("mock-execution-{}", state.next_execution);
        state.executions.insert(
            execution_id.clone(),
            MockExecution {
                response: Some(fixture.response),
                events: fixture.events,
                phase: HostAiExecutionPhase::Completed,
                error: None,
            },
        );
        HostAiExecutionHandle::new(
            &context,
            execution_id,
            &request,
            handle.caller.caller_id.clone(),
        )
    }

    /// Returns one deterministic mock execution snapshot.
    /// 返回一项确定性的 mock 执行快照。
    fn execution_status(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<HostAiExecutionSnapshot, HostAiError> {
        self.ensure_registered_handle_by_caller(handle)?;
        let state = self.lock_state()?;
        let execution = state
            .executions
            .get(handle.execution_id.as_str())
            .ok_or_else(|| {
                HostAiError::new(
                    HostAiErrorCode::OperationNotFound,
                    format!(
                        "Mock Host AI execution `{}` was not found.",
                        handle.execution_id
                    ),
                )
                .with_execution_handle(handle)
            })?;
        Ok(HostAiExecutionSnapshot {
            execution_id: handle.execution_id.clone(),
            operation_id: handle.operation_id.clone(),
            request_id: handle.request_id.clone(),
            phase: execution.phase,
            route: execution
                .response
                .as_ref()
                .map(|response| response.route.clone()),
            error: execution.error.clone(),
        })
    }

    /// Opens a runtime-free stream over one mock execution's events.
    /// 为一项 mock 执行事件打开不依赖运行时的事件流。
    fn stream_response(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<HostAiEventStream, HostAiError> {
        self.ensure_registered_handle_by_caller(handle)?;
        let state = self.lock_state()?;
        let execution = state
            .executions
            .get(handle.execution_id.as_str())
            .ok_or_else(|| {
                HostAiError::new(
                    HostAiErrorCode::OperationNotFound,
                    format!(
                        "Mock Host AI execution `{}` was not found.",
                        handle.execution_id
                    ),
                )
                .with_execution_handle(handle)
            })?;
        let events = execution.events.iter().cloned().map(Ok).collect();
        Ok(Box::pin(MockEventStream { events }))
    }

    /// Waits for one deterministic mock execution result.
    /// 等待一项确定性的 mock 执行结果。
    async fn wait_response(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<ResponsesExecutionResult, HostAiError> {
        self.ensure_registered_handle_by_caller(handle)?;
        if handle.cancellation().is_cancelled() {
            let _ = self.cancel_execution(handle);
            return Err(HostAiError::new(
                HostAiErrorCode::Cancelled,
                "Mock Host AI operation was cancelled.",
            )
            .with_execution_handle(handle));
        }
        let state = self.lock_state()?;
        let execution = state
            .executions
            .get(handle.execution_id.as_str())
            .ok_or_else(|| {
                HostAiError::new(
                    HostAiErrorCode::OperationNotFound,
                    format!(
                        "Mock Host AI execution `{}` was not found.",
                        handle.execution_id
                    ),
                )
                .with_execution_handle(handle)
            })?;
        match execution.phase {
            HostAiExecutionPhase::Completed => execution.response.clone().ok_or_else(|| {
                HostAiError::new(
                    HostAiErrorCode::Internal,
                    "Mock completed execution has no aggregate response.",
                )
                .with_execution_handle(handle)
            }),
            HostAiExecutionPhase::Failed | HostAiExecutionPhase::Cancelled => {
                Err(execution.error.clone().unwrap_or_else(|| {
                    HostAiError::new(
                        HostAiErrorCode::Provider,
                        "Mock execution ended without a terminal diagnostic.",
                    )
                    .with_execution_handle(handle)
                }))
            }
            HostAiExecutionPhase::Running => Err(HostAiError::new(
                HostAiErrorCode::Internal,
                "Mock execution is unexpectedly still running.",
            )
            .with_execution_handle(handle)),
        }
    }

    /// Cancels one mock execution and records its terminal state.
    /// 取消一项 mock 执行并记录其终态。
    fn cancel_execution(&self, handle: &HostAiExecutionHandle) -> Result<(), HostAiError> {
        self.ensure_registered_handle_by_caller(handle)?;
        let mut state = self.lock_state()?;
        let execution = state
            .executions
            .get_mut(handle.execution_id.as_str())
            .ok_or_else(|| {
                HostAiError::new(
                    HostAiErrorCode::OperationNotFound,
                    format!(
                        "Mock Host AI execution `{}` was not found.",
                        handle.execution_id
                    ),
                )
                .with_execution_handle(handle)
            })?;
        if execution.phase != HostAiExecutionPhase::Running {
            return Err(HostAiError::new(
                HostAiErrorCode::Configuration,
                "Mock Host AI execution is already terminal.",
            )
            .with_execution_handle(handle));
        }
        execution.phase = HostAiExecutionPhase::Cancelled;
        execution.error = Some(
            HostAiError::new(
                HostAiErrorCode::Cancelled,
                "Mock Host AI operation was cancelled.",
            )
            .with_execution_handle(handle),
        );
        Ok(())
    }
}

impl MockHostAiPort {
    /// Validates execution ownership using the caller identity retained by a handle.
    /// 使用句柄保留的调用方标识校验执行所有权。
    fn ensure_registered_handle_by_caller(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<(), HostAiError> {
        let state = self.lock_state()?;
        if !state
            .capabilities
            .values()
            .any(|capability| capability.caller.caller_id == handle.caller_id)
        {
            return Err(HostAiError::new(
                HostAiErrorCode::PermissionDenied,
                "Mock Host AI execution caller is not registered.",
            )
            .with_execution_handle(handle));
        }
        Ok(())
    }
}
