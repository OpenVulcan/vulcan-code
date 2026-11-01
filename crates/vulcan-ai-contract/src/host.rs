//! Host-registered internal AI capability contracts.
//! 宿主注册的内部 AI 能力契约。

// HostAiError intentionally retains stable correlation fields; its large optional payloads are boxed.
// HostAiError 有意保留稳定关联字段；其中较大的可选载荷已经盒装。
#![allow(
    clippy::result_large_err,
    reason = "HostAiError preserves the frozen host-boundary diagnostic contract"
)]

use async_trait::async_trait;
use futures_core::Stream;
use std::{fmt, future::Future, pin::Pin, sync::Arc};

use crate::{
    AiErrorCategory, AiExecutionError, AiOperation, AiStreamEvent, DecisionRequest,
    DecisionResponse, DispatchOutcome, EmbeddingBatchCollectedResult, EmbeddingBatchPlan,
    EmbeddingBatchSnapshot, EmbeddingBatchSubmission, EmbeddingBatchTaskPlan,
    EmbeddingBatchUploadedInput, EmbeddingRequest, EmbeddingResponse, ExecutionRouteSnapshot,
    InferenceCallerContext, InferenceCapabilitySnapshot, RerankRequest, RerankResponse,
    ResponsesExecutionRequest, ResponsesExecutionResult, RetryAfterHint,
};

/// Provides a process-local cancellation signal without coupling the contract crate to one async runtime.
/// 提供进程内取消信号，且不让契约 crate 耦合到某一种异步运行时。
pub trait HostAiCancellation: Send + Sync {
    /// Returns whether the owning operation has already been cancelled.
    /// 返回所属操作是否已经被取消。
    fn is_cancelled(&self) -> bool;

    /// Resolves when the owning operation is cancelled.
    /// 在所属操作被取消时完成。
    fn cancelled<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

/// Represents an operation that has no cancellation source.
/// 表示没有取消来源的一次操作。
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopHostAiCancellation;

impl HostAiCancellation for NoopHostAiCancellation {
    /// Always reports that the operation is still active.
    /// 始终报告操作仍处于活动状态。
    fn is_cancelled(&self) -> bool {
        false
    }

    /// Never resolves because this cancellation source cannot be triggered.
    /// 永不完成，因为该取消来源无法被触发。
    fn cancelled<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(std::future::pending())
    }
}

/// Identifies why a second attempt was explicitly requested.
/// 标识显式发起第二次尝试的原因。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum HostAiRetryReason {
    /// The host retried a transient provider failure on the same route.
    /// 宿主在同一路由上重试了瞬时供应商失败。
    ProviderTransient,
    /// The consumer requested a schema-repair attempt after structured output validation failed.
    /// 结构化输出校验失败后，消费者请求了 Schema 修复尝试。
    SchemaRepair,
    /// The consumer explicitly requested another attempt under its own policy.
    /// 消费者根据自身策略显式请求了再次尝试。
    ConsumerRequested,
}

/// Carries operation correlation and lifecycle context beside an existing typed AI request.
/// 在现有类型化 AI 请求之外承载操作关联和生命周期上下文。
#[derive(Clone)]
pub struct HostAiCallContext {
    /// Stable logical operation identifier shared by all attempts of one operation.
    /// 同一逻辑操作的所有尝试共享的稳定操作标识。
    pub operation_id: String,
    /// Optional VMM-Core effect identifier that caused this AI call.
    /// 触发本次 AI 调用的可选 VMM-Core effect 标识。
    pub effect_id: Option<String>,
    /// Optional VMM-Core command identifier that owns the effect.
    /// 持有该 effect 的可选 VMM-Core command 标识。
    pub command_id: Option<String>,
    /// Optional host-proven conversation session that owns this AI call.
    /// 由宿主证明、持有本次 AI 调用的可选会话标识。
    pub owner_session_id: Option<String>,
    /// Optional operation-level Unix deadline in milliseconds.
    /// 可选的操作级 Unix 毫秒截止时间。
    pub deadline_unix_ms: Option<u64>,
    /// One-based physical attempt number; the first attempt is `1`.
    /// 从一开始计数的真实尝试次数；第一次尝试为 `1`。
    pub attempt: u32,
    /// Explicit reason for an attempt after the first one.
    /// 第一次尝试之后再次尝试时使用的显式原因。
    pub retry_reason: Option<HostAiRetryReason>,
    /// Cooperative cancellation source owned by the host operation.
    /// 由宿主操作持有的协作式取消来源。
    pub cancellation: Arc<dyn HostAiCancellation>,
}

impl fmt::Debug for HostAiCallContext {
    /// Formats correlation fields without exposing the cancellation implementation.
    /// 格式化关联字段但不暴露取消实现细节。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostAiCallContext")
            .field("operation_id", &self.operation_id)
            .field("effect_id", &self.effect_id)
            .field("command_id", &self.command_id)
            .field("owner_session_id", &self.owner_session_id)
            .field("deadline_unix_ms", &self.deadline_unix_ms)
            .field("attempt", &self.attempt)
            .field("retry_reason", &self.retry_reason)
            .field("cancelled", &self.cancellation.is_cancelled())
            .finish()
    }
}

impl HostAiCallContext {
    /// Creates the first-attempt context for one logical operation.
    /// 创建一次逻辑操作的第一次尝试上下文。
    ///
    /// # Parameters
    /// 参数
    /// `operation_id` is the stable correlation identity retained across retries.
    /// `operation_id` 是跨重试保留的稳定关联标识。
    ///
    /// # Returns
    /// 返回值
    /// A context with attempt `1` and a no-op cancellation source.
    /// 返回尝试次数为 `1` 且使用空取消来源的上下文。
    pub fn new(operation_id: impl Into<String>) -> Self {
        Self {
            operation_id: operation_id.into(),
            effect_id: None,
            command_id: None,
            owner_session_id: None,
            deadline_unix_ms: None,
            attempt: 1,
            retry_reason: None,
            cancellation: Arc::new(NoopHostAiCancellation),
        }
    }

    /// Replaces the operation-level deadline with an earlier host-owned deadline.
    /// 使用更早的宿主操作截止时间替换当前截止时间。
    ///
    /// # Parameters
    /// 参数
    /// `deadline_unix_ms` is an absolute Unix deadline; the caller budget remains an upper bound.
    /// `deadline_unix_ms` 是绝对 Unix 截止时间；调用方预算仍然是更高层上限。
    ///
    /// # Returns
    /// 返回值
    /// The same context with the requested deadline.
    /// 返回设置了请求截止时间的同一上下文。
    pub fn with_deadline(mut self, deadline_unix_ms: Option<u64>) -> Self {
        self.deadline_unix_ms = deadline_unix_ms;
        self
    }

    /// Attaches a host-owned cooperative cancellation source.
    /// 附加由宿主持有的协作式取消来源。
    ///
    /// # Parameters
    /// 参数
    /// `cancellation` is shared by all host work observing this operation.
    /// `cancellation` 由观察该操作的全部宿主工作共享。
    ///
    /// # Returns
    /// 返回值
    /// The same context with the supplied cancellation source.
    /// 返回使用指定取消来源的同一上下文。
    pub fn with_cancellation(mut self, cancellation: Arc<dyn HostAiCancellation>) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Marks this context as a later attempt with an explicit retry reason.
    /// 将上下文标记为带有显式重试原因的后续尝试。
    ///
    /// # Parameters
    /// 参数
    /// `attempt` is one-based and must be greater than one.
    /// `attempt` 从一开始计数且必须大于一。
    /// `retry_reason` explains the consumer or host retry policy.
    /// `retry_reason` 说明消费者或宿主的重试策略。
    ///
    /// # Returns
    /// 返回值
    /// The same context with later-attempt metadata.
    /// 返回带有后续尝试元数据的同一上下文。
    pub fn with_attempt(mut self, attempt: u32, retry_reason: HostAiRetryReason) -> Self {
        self.attempt = attempt;
        self.retry_reason = Some(retry_reason);
        self
    }

    /// Adds VMM-Core correlation identifiers to the operation context.
    /// 为操作上下文添加 VMM-Core 关联标识。
    ///
    /// # Parameters
    /// 参数
    /// `effect_id` and `command_id` are stable identifiers from the owning Core command.
    /// `effect_id` 与 `command_id` 是所属 Core 命令的稳定标识。
    ///
    /// # Returns
    /// 返回值
    /// The same context with effect and command correlation.
    /// 返回带有 effect 与 command 关联信息的同一上下文。
    pub fn with_core_correlation(
        mut self,
        effect_id: Option<String>,
        command_id: Option<String>,
    ) -> Self {
        self.effect_id = effect_id;
        self.command_id = command_id;
        self
    }

    /// Binds this operation to the conversation session that owns it.
    /// 将本次操作绑定到持有它的会话。
    ///
    /// # Parameters
    /// 参数
    /// `owner_session_id` is the host-proven session identity, or `None` when the work has no session.
    /// `owner_session_id` 是宿主证明的会话标识；工作不属于任何会话时为 `None`。
    ///
    /// # Returns
    /// 返回值
    /// The same context with the owning session.
    /// 返回带有所属会话的同一上下文。
    pub fn with_owner_session(mut self, owner_session_id: Option<String>) -> Self {
        self.owner_session_id = owner_session_id;
        self
    }

    /// Computes the effective deadline without allowing a request to exceed the caller budget.
    /// 计算有效截止时间，确保请求不能超过调用方预算。
    ///
    /// # Parameters
    /// 参数
    /// `caller_deadline_unix_ms` is the trusted caller-level deadline.
    /// `caller_deadline_unix_ms` 是可信调用方级截止时间。
    ///
    /// # Returns
    /// 返回值
    /// The earlier configured deadline, or the only deadline that exists.
    /// 返回两个截止时间中更早的一个，或唯一存在的截止时间。
    pub fn effective_deadline(&self, caller_deadline_unix_ms: Option<u64>) -> Option<u64> {
        match (self.deadline_unix_ms, caller_deadline_unix_ms) {
            (Some(operation), Some(caller)) => Some(operation.min(caller)),
            (Some(operation), None) => Some(operation),
            (None, Some(caller)) => Some(caller),
            (None, None) => None,
        }
    }

    /// Validates operation correlation before a typed request is dispatched.
    /// 在分派类型化请求前校验操作关联信息。
    ///
    /// # Parameters
    /// 参数
    /// `request_id` is the request-specific identity carried by the typed request.
    /// `request_id` 是类型化请求携带的请求级标识。
    ///
    /// # Returns
    /// 返回值
    /// Success when all lifecycle identities are explicit and internally consistent.
    /// 生命周期标识均明确且内部一致时返回成功。
    pub fn validate(&self, request_id: &str) -> Result<(), HostAiError> {
        if self.operation_id.trim().is_empty() {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Host AI operation_id must not be empty.",
            )
            .with_request_context(self, Some(request_id)));
        }
        if request_id.trim().is_empty() {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Host AI request_id must not be empty.",
            )
            .with_request_context(self, None));
        }
        if self.attempt == 0 {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Host AI attempt must be greater than zero.",
            )
            .with_request_context(self, Some(request_id)));
        }
        for (field, value) in [
            ("effect_id", self.effect_id.as_deref()),
            ("command_id", self.command_id.as_deref()),
            ("owner_session_id", self.owner_session_id.as_deref()),
        ] {
            if value.is_some_and(|value| value.trim().is_empty()) {
                return Err(HostAiError::new(
                    HostAiErrorCode::Contract,
                    format!("Host AI {field} must not be empty when present."),
                )
                .with_request_context(self, Some(request_id)));
            }
        }
        Ok(())
    }
}

/// Identifies a stable failure code at the host Port boundary.
/// 标识宿主 Port 边界上的稳定失败代码。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum HostAiErrorCode {
    /// The typed request or operation context is invalid.
    /// 类型化请求或操作上下文无效。
    Contract,
    /// Provider authentication failed.
    /// 供应商鉴权失败。
    Authentication,
    /// The host caller lacks the required permission.
    /// 宿主调用方缺少所需权限。
    PermissionDenied,
    /// The upstream account has insufficient payment or quota.
    /// 上游账号支付能力或额度不足。
    PaymentRequired,
    /// The host or provider rate limit was reached.
    /// 宿主或供应商的请求频率上限已达到。
    RateLimited,
    /// The provider is temporarily overloaded.
    /// 供应商暂时过载。
    ProviderOverloaded,
    /// The provider is temporarily unavailable.
    /// 供应商暂时不可用。
    ProviderUnavailable,
    /// The host profile or route configuration is invalid.
    /// 宿主 Profile 或路由配置无效。
    Configuration,
    /// The requested operation is not available in the selected capability.
    /// 所选能力不提供请求的操作。
    CapabilityUnavailable,
    /// A host or provider input limit was exceeded.
    /// 超过宿主或供应商输入限制。
    LimitExceeded,
    /// The request is invalid for the selected operation.
    /// 请求对所选操作无效。
    InvalidRequest,
    /// Returned embedding dimensions do not match the requested space.
    /// 返回向量维度与请求空间不匹配。
    DimensionMismatch,
    /// Returned embedding compatibility identity does not match the requested space.
    /// 返回向量兼容身份与请求空间不匹配。
    FingerprintMismatch,
    /// The execution exceeded a timeout or deadline.
    /// 执行超过超时或截止时间。
    Timeout,
    /// The owning operation was cancelled.
    /// 所属操作已被取消。
    Cancelled,
    /// The requested execution handle no longer exists or is not owned by the caller.
    /// 请求的执行句柄不存在或不属于当前调用方。
    OperationNotFound,
    /// The upstream provider returned a classified execution failure.
    /// 上游供应商返回了已分类的执行失败。
    Provider,
    /// The host encountered an unclassified internal failure.
    /// 宿主遇到未分类的内部失败。
    Internal,
}

/// Carries a stable host-level error without exposing a concrete service implementation.
/// 承载稳定的宿主级错误且不暴露具体服务实现。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HostAiError {
    /// Stable host-level failure code.
    /// 稳定的宿主级失败代码。
    pub code: HostAiErrorCode,
    /// Controlled diagnostic safe for the consuming library.
    /// 可供调用库使用的受控诊断信息。
    pub message: String,
    /// Request identity when a typed request was available.
    /// 存在类型化请求时的请求标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Logical operation identity shared by attempts.
    /// 跨尝试共享的逻辑操作标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    /// Optional VMM-Core effect identity.
    /// 可选的 VMM-Core effect 标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_id: Option<String>,
    /// Optional VMM-Core command identity.
    /// 可选的 VMM-Core command 标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
    /// One-based physical attempt number.
    /// 从一开始计数的真实尝试次数。
    pub attempt: u32,
    /// Whether a new attempt is permitted by the host policy.
    /// 宿主策略是否允许发起新的尝试。
    pub retryable: bool,
    /// Whether the upstream boundary was reached.
    /// 是否已经到达上游边界。
    pub dispatch_outcome: DispatchOutcome,
    /// Optional normalized retry delay and provenance.
    /// 可选的归一化重试延迟及其来源。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<RetryAfterHint>,
    /// Boxed resolved route when one was available, keeping host errors inexpensive to return.
    /// 已解析路由可用时的盒装快照，使宿主错误保持较低返回成本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<Box<ExecutionRouteSnapshot>>,
    /// Boxed normalized provider error retained for diagnostics without inflating every host result.
    /// 盒装保存的归一化供应商错误，用于诊断且避免放大每个宿主结果。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_error: Option<Box<AiExecutionError>>,
}

impl HostAiError {
    /// Creates a deterministic host error without request context.
    /// 创建一项不带请求上下文的确定性宿主错误。
    ///
    /// # Parameters
    /// 参数
    /// `code` and `message` are the stable host-selected failure identity and diagnostic.
    /// `code` 与 `message` 是宿主选择的稳定失败身份和诊断信息。
    ///
    /// # Returns
    /// 返回值
    /// A non-retryable error with no dispatch evidence.
    /// 返回一项不可重试且没有分派证据的错误。
    pub fn new(code: HostAiErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            request_id: None,
            operation_id: None,
            effect_id: None,
            command_id: None,
            attempt: 1,
            retryable: false,
            dispatch_outcome: DispatchOutcome::NotDispatched,
            retry_after: None,
            route: None,
            provider_error: None,
        }
    }

    /// Attaches request and operation correlation to this error.
    /// 为错误附加请求和操作关联信息。
    ///
    /// # Parameters
    /// 参数
    /// `context` carries operation/effect/command/attempt metadata; `request_id` is optional for handle errors.
    /// `context` 承载操作、effect、command、尝试次数；`request_id` 对执行句柄错误可以为空。
    ///
    /// # Returns
    /// 返回值
    /// The same error enriched with stable correlation fields.
    /// 返回补充稳定关联字段的同一错误。
    pub fn with_request_context(
        mut self,
        context: &HostAiCallContext,
        request_id: Option<&str>,
    ) -> Self {
        self.request_id = request_id.map(str::to_owned);
        self.operation_id = Some(context.operation_id.clone());
        self.effect_id = context.effect_id.clone();
        self.command_id = context.command_id.clone();
        self.attempt = context.attempt;
        self
    }

    /// Attaches only a request identity when no operation context is available.
    /// 在没有操作上下文时仅为错误附加请求标识。
    ///
    /// # Parameters
    /// 参数
    /// `request_id` is the typed request identity.
    /// `request_id` 是类型化请求的标识。
    ///
    /// # Returns
    /// 返回值
    /// The same error with the request identity attached.
    /// 返回附加请求标识的同一错误。
    pub fn with_request_id(mut self, request_id: &str) -> Self {
        if !request_id.trim().is_empty() {
            self.request_id = Some(request_id.to_owned());
        }
        self
    }

    /// Attaches the identity fields retained by an execution handle.
    /// 为错误附加执行句柄保留的身份字段。
    ///
    /// # Parameters
    /// 参数
    /// `handle` is the host-issued handle that owns the failed operation.
    /// `handle` 是持有失败操作的宿主签发句柄。
    ///
    /// # Returns
    /// 返回值
    /// The same error enriched with handle correlation.
    /// 返回补充句柄关联信息的同一错误。
    pub fn with_execution_handle(mut self, handle: &HostAiExecutionHandle) -> Self {
        self.request_id = Some(handle.request_id.clone());
        self.operation_id = Some(handle.operation_id.clone());
        self.effect_id = handle.effect_id.clone();
        self.command_id = handle.command_id.clone();
        self.attempt = handle.attempt;
        self
    }

    /// Converts an existing normalized provider error to the host Port error shape.
    /// 将已有归一化供应商错误转换为宿主 Port 错误形态。
    ///
    /// # Parameters
    /// 参数
    /// `error` is the provider-neutral error already normalized by the host runtime.
    /// `error` 是宿主运行时已经归一化的供应商无关错误。
    ///
    /// # Returns
    /// 返回值
    /// A host error preserving provider retry and dispatch semantics.
    /// 返回保留供应商重试与分派语义的宿主错误。
    pub fn from_execution_error(error: AiExecutionError) -> Self {
        let code = match error.category {
            AiErrorCategory::Authentication => HostAiErrorCode::Authentication,
            AiErrorCategory::PermissionDenied => HostAiErrorCode::PermissionDenied,
            AiErrorCategory::PaymentRequired => HostAiErrorCode::PaymentRequired,
            AiErrorCategory::RateLimited => HostAiErrorCode::RateLimited,
            AiErrorCategory::ProviderOverloaded => HostAiErrorCode::ProviderOverloaded,
            AiErrorCategory::ProviderUnavailable => HostAiErrorCode::ProviderUnavailable,
            AiErrorCategory::Timeout => HostAiErrorCode::Timeout,
            AiErrorCategory::Cancelled => HostAiErrorCode::Cancelled,
            AiErrorCategory::InvalidRequest => HostAiErrorCode::InvalidRequest,
            AiErrorCategory::ContextLengthExceeded | AiErrorCategory::ContentPolicy => {
                HostAiErrorCode::Provider
            }
            AiErrorCategory::Unclassified => HostAiErrorCode::Internal,
        };
        Self {
            code,
            message: error.message.clone(),
            request_id: error.request_id.clone(),
            operation_id: None,
            effect_id: None,
            command_id: None,
            attempt: 1,
            retryable: error.retryable,
            dispatch_outcome: error.dispatch_outcome,
            retry_after: error.retry_after.clone(),
            route: None,
            provider_error: Some(Box::new(error)),
        }
    }

    /// Marks a host error as retryable under an explicit host policy.
    /// 按显式宿主策略将错误标记为可重试。
    ///
    /// # Parameters
    /// 参数
    /// `retryable` is the final host decision, not a provider suggestion.
    /// `retryable` 是最终宿主决定，而不是供应商建议。
    ///
    /// # Returns
    /// 返回值
    /// The same error with the final retry decision.
    /// 返回带有最终重试决定的同一错误。
    pub fn with_retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }
}

impl fmt::Display for HostAiError {
    /// Formats the controlled host diagnostic.
    /// 格式化受控宿主诊断。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message.as_str())
    }
}

impl std::error::Error for HostAiError {}

/// Describes one operation that a registered capability must provide.
/// 描述一项已注册能力必须提供的操作。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HostAiCapabilityRequirement {
    /// Provider-neutral operation identity.
    /// 供应商无关的操作标识。
    pub operation: AiOperation,
    /// Consumer-scoped LLM purpose when the operation is purpose-bound.
    /// 操作按用途绑定时使用的消费者作用域 LLM 用途。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose_id: Option<String>,
    /// Whether the operation must be available before registration succeeds.
    /// 注册成功前该操作是否必须可用。
    pub required: bool,
}

/// Declares one host-owned caller and its required configured capability.
/// 声明一个宿主持有的调用方及其所需的已配置能力。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostAiCapabilityRegistration {
    /// Stable registration identity used for replacement and removal.
    /// 用于替换和移除的稳定注册标识。
    pub registration_id: String,
    /// Stable owner identity such as a knowledge library or native VMM runtime.
    /// 稳定的所有者标识，例如资料库或内置 VMM 运行时。
    pub owner_id: String,
    /// Configured inference consumer profile selected by the owner.
    /// 所有者选择的已配置推理消费者 Profile。
    pub consumer_profile_id: String,
    /// Trusted caller context that the host will use for execution.
    /// 宿主用于执行的可信调用方上下文。
    pub caller: InferenceCallerContext,
    /// Required and optional operations validated at registration time.
    /// 注册时校验的必需和可选操作。
    pub requirements: Vec<HostAiCapabilityRequirement>,
}

/// Carries a validated host-issued capability and its trusted caller context.
/// 承载经过校验的宿主签发能力及其可信调用方上下文。
#[derive(Debug, Clone)]
pub struct HostAiCapabilityHandle {
    /// Stable registration identity.
    /// 稳定注册标识。
    pub registration_id: String,
    /// Stable owner identity.
    /// 稳定所有者标识。
    pub owner_id: String,
    /// Configured consumer profile identity.
    /// 已配置消费者 Profile 标识。
    pub consumer_profile_id: String,
    /// Trusted caller context injected by the host.
    /// 宿主注入的可信调用方上下文。
    pub caller: InferenceCallerContext,
    /// Capability snapshot captured when this handle was registered or refreshed.
    /// 注册或刷新句柄时捕获的能力快照。
    pub snapshot: InferenceCapabilitySnapshot,
}

impl HostAiCapabilityHandle {
    /// Creates a validated capability handle for host adapters and test doubles.
    /// 为宿主适配器和测试替身创建经过校验的能力句柄。
    ///
    /// # Parameters
    /// 参数
    /// All identities must be non-empty and `snapshot` must represent `consumer_profile_id`.
    /// 所有标识必须非空，且 `snapshot` 必须表示 `consumer_profile_id`。
    ///
    /// # Returns
    /// 返回值
    /// A capability handle that can be passed to every HostAiPort operation.
    /// 可传递给全部 HostAiPort 操作的能力句柄。
    pub fn new(
        registration_id: impl Into<String>,
        owner_id: impl Into<String>,
        consumer_profile_id: impl Into<String>,
        caller: InferenceCallerContext,
        snapshot: InferenceCapabilitySnapshot,
    ) -> Result<Self, HostAiError> {
        let registration_id = registration_id.into();
        let owner_id = owner_id.into();
        let consumer_profile_id = consumer_profile_id.into();
        if registration_id.trim().is_empty()
            || owner_id.trim().is_empty()
            || consumer_profile_id.trim().is_empty()
        {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Host AI capability identities must not be empty.",
            ));
        }
        if snapshot.consumer_profile_id != consumer_profile_id {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Host AI capability snapshot profile does not match its handle.",
            ));
        }
        Ok(Self {
            registration_id,
            owner_id,
            consumer_profile_id,
            caller,
            snapshot,
        })
    }

    /// Returns whether the captured snapshot exposes an executable operation.
    /// 返回捕获的能力快照是否暴露了可执行操作。
    ///
    /// # Parameters
    /// 参数
    /// `operation` and `purpose_id` identify the requested capability.
    /// `operation` 与 `purpose_id` 标识所请求的能力。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when a matching operation is explicitly available.
    /// 仅当匹配操作被明确标记为可用时返回 `true`。
    pub fn supports(&self, operation: AiOperation, purpose_id: Option<&str>) -> bool {
        self.snapshot.operations.iter().any(|capability| {
            capability.operation == operation
                && capability.purpose_id.as_deref() == purpose_id
                && capability.available
        })
    }
}

/// Carries one embedding request together with its logical storage space identity.
/// 承载一次向量请求及其逻辑存储空间标识。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HostAiEmbeddingRequest {
    /// Stable logical vector-space identity selected by the storage owner.
    /// 由存储所有者选择的稳定逻辑向量空间标识。
    pub vector_space_id: String,
    /// Existing provider-neutral embedding request.
    /// 现有供应商无关向量请求。
    pub request: EmbeddingRequest,
}

impl HostAiEmbeddingRequest {
    /// Validates the logical space identity and the nested embedding request.
    /// 校验逻辑空间标识及嵌套向量请求。
    ///
    /// # Returns
    /// 返回值
    /// Success when both the space and nested request are explicit and valid.
    /// 空间与嵌套请求均明确且有效时返回成功。
    pub fn validate(&self) -> Result<(), HostAiError> {
        if self.vector_space_id.trim().is_empty() {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Host AI vector_space_id must not be empty.",
            ));
        }
        self.request.validate().map_err(|message| {
            HostAiError::new(HostAiErrorCode::Contract, message)
                .with_request_id(self.request.request_id.as_str())
        })
    }
}

/// Carries an embedding response while preserving the caller-selected logical space.
/// 承载向量响应并保留调用方选择的逻辑空间。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HostAiEmbeddingResponse {
    /// Logical vector-space identity from the originating request.
    /// 来源请求中的逻辑向量空间标识。
    pub vector_space_id: String,
    /// Existing provider-neutral embedding response and physical fingerprint.
    /// 现有供应商无关向量响应及其物理指纹。
    pub response: EmbeddingResponse,
}

/// Identifies the lifecycle phase of one host-owned LLM execution.
/// 标识一项宿主持有 LLM 执行的生命周期阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostAiExecutionPhase {
    /// The provider execution is still running.
    /// 供应商执行仍在运行。
    Running,
    /// The aggregate response completed successfully.
    /// 聚合响应已成功完成。
    Completed,
    /// The execution ended with a failure.
    /// 执行以失败结束。
    Failed,
    /// The owning operation cancelled the execution.
    /// 所属操作取消了执行。
    Cancelled,
}

/// Carries a stable LLM execution snapshot without exposing service internals.
/// 承载稳定的 LLM 执行快照且不暴露服务内部结构。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HostAiExecutionSnapshot {
    /// Stable execution identifier.
    /// 稳定执行标识。
    pub execution_id: String,
    /// Logical operation identity.
    /// 逻辑操作标识。
    pub operation_id: String,
    /// Original request identity.
    /// 原始请求标识。
    pub request_id: String,
    /// Current execution phase.
    /// 当前执行阶段。
    pub phase: HostAiExecutionPhase,
    /// Resolved non-secret route when dispatch has started.
    /// 已开始分派时的非秘密路由快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<ExecutionRouteSnapshot>,
    /// Terminal host error when the phase is failed or cancelled.
    /// 阶段为失败或取消时的终态宿主错误。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<HostAiError>,
}

/// Identifies one host-owned asynchronous LLM execution.
/// 标识一项宿主持有的异步 LLM 执行。
#[derive(Clone)]
pub struct HostAiExecutionHandle {
    /// Stable logical operation identifier.
    /// 稳定逻辑操作标识。
    pub operation_id: String,
    /// Optional VMM-Core effect identifier.
    /// 可选的 VMM-Core effect 标识。
    pub effect_id: Option<String>,
    /// Optional VMM-Core command identifier.
    /// 可选的 VMM-Core command 标识。
    pub command_id: Option<String>,
    /// Stable host execution identifier.
    /// 稳定宿主执行标识。
    pub execution_id: String,
    /// Original request identifier.
    /// 原始请求标识。
    pub request_id: String,
    /// Trusted caller identifier.
    /// 可信调用方标识。
    pub caller_id: String,
    /// Authorized consumer profile identifier.
    /// 已授权消费者 Profile 标识。
    pub consumer_profile_id: String,
    /// One-based physical attempt number.
    /// 从一开始计数的真实尝试次数。
    pub attempt: u32,
    /// Cooperative cancellation source retained for wait operations.
    /// 为等待操作保留的协作式取消来源。
    cancellation: Arc<dyn HostAiCancellation>,
}

impl fmt::Debug for HostAiExecutionHandle {
    /// Formats stable identities without exposing the cancellation implementation.
    /// 格式化稳定标识但不暴露取消实现细节。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostAiExecutionHandle")
            .field("operation_id", &self.operation_id)
            .field("effect_id", &self.effect_id)
            .field("command_id", &self.command_id)
            .field("execution_id", &self.execution_id)
            .field("request_id", &self.request_id)
            .field("caller_id", &self.caller_id)
            .field("consumer_profile_id", &self.consumer_profile_id)
            .field("attempt", &self.attempt)
            .finish()
    }
}

impl HostAiExecutionHandle {
    /// Creates a host execution handle after validating all stable identities.
    /// 在校验全部稳定标识后创建宿主执行句柄。
    ///
    /// # Returns
    /// 返回值
    /// A handle suitable for stream, wait, status, and cancellation operations.
    /// 可用于流式读取、等待、状态查询和取消操作的句柄。
    pub fn new(
        context: &HostAiCallContext,
        execution_id: impl Into<String>,
        request: &ResponsesExecutionRequest,
        caller_id: impl Into<String>,
    ) -> Result<Self, HostAiError> {
        context.validate(request.request_id.as_str())?;
        let execution_id = execution_id.into();
        let caller_id = caller_id.into();
        if execution_id.trim().is_empty() || caller_id.trim().is_empty() {
            return Err(HostAiError::new(
                HostAiErrorCode::Contract,
                "Host AI execution_id and caller_id must not be empty.",
            )
            .with_request_context(context, Some(request.request_id.as_str())));
        }
        Ok(Self {
            operation_id: context.operation_id.clone(),
            effect_id: context.effect_id.clone(),
            command_id: context.command_id.clone(),
            execution_id,
            request_id: request.request_id.clone(),
            caller_id,
            consumer_profile_id: request.consumer_profile_id.clone(),
            attempt: context.attempt,
            cancellation: Arc::clone(&context.cancellation),
        })
    }

    /// Returns the cancellation source retained for this execution.
    /// 返回该执行保留的取消来源。
    pub fn cancellation(&self) -> Arc<dyn HostAiCancellation> {
        Arc::clone(&self.cancellation)
    }
}

/// Provides a process-local stream of normalized AI events.
/// 提供进程内归一化 AI 事件流。
pub type HostAiEventStream =
    Pin<Box<dyn Stream<Item = Result<AiStreamEvent, HostAiError>> + Send + 'static>>;

/// Defines the only internal AI boundary that VulcanCode libraries may consume.
/// 定义 VulcanCode 内部库唯一允许使用的 AI 边界。
#[async_trait]
pub trait HostAiPort: Send + Sync {
    /// Registers or replaces one host-owned capability registration.
    /// 注册或替换一项宿主持有的能力注册。
    ///
    /// The composition root owns this mutation; consumers receive the returned handle and do not construct Providers.
    /// 该变更由组合根负责；消费者只接收返回句柄，不创建 Provider。
    fn register_capability(
        &self,
        registration: HostAiCapabilityRegistration,
    ) -> Result<HostAiCapabilityHandle, HostAiError>;

    /// Returns a previously registered capability handle by stable registration identity.
    /// 按稳定注册标识返回此前注册的能力句柄。
    fn capability_handle(
        &self,
        registration_id: &str,
    ) -> Result<HostAiCapabilityHandle, HostAiError>;

    /// Removes one registration from the process-local capability registry.
    /// 从进程内能力注册表移除一项注册。
    fn unregister_capability(&self, registration_id: &str) -> Result<(), HostAiError>;

    /// Refreshes the capability snapshot while preserving the handle identity.
    /// 在保持句柄身份不变的情况下刷新能力快照。
    fn capability_snapshot(
        &self,
        handle: &HostAiCapabilityHandle,
    ) -> Result<InferenceCapabilitySnapshot, HostAiError>;

    /// Builds a deterministic bounded asynchronous embedding Batch plan without provider I/O.
    /// 在不执行供应商 I/O 的情况下构建确定且有界的异步向量 Batch 计划。
    fn plan_embedding_batch(
        &self,
        _handle: &HostAiCapabilityHandle,
        request: &HostAiEmbeddingRequest,
    ) -> Result<EmbeddingBatchPlan, HostAiError> {
        Err(HostAiError::new(
            HostAiErrorCode::CapabilityUnavailable,
            "The Host AI implementation does not support asynchronous embedding Batch execution.",
        )
        .with_request_id(request.request.request_id.as_str()))
    }

    /// Uploads one exact file-backed embedding Batch input before task creation.
    /// 在创建任务前上传一个精确的文件型向量 Batch 输入。
    async fn upload_embedding_batch_input(
        &self,
        _handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: HostAiEmbeddingRequest,
        _request_fingerprint: String,
        _task: EmbeddingBatchTaskPlan,
    ) -> Result<EmbeddingBatchUploadedInput, HostAiError> {
        Err(HostAiError::new(
            HostAiErrorCode::CapabilityUnavailable,
            "The Host AI implementation does not support embedding Batch input upload.",
        )
        .with_request_context(&context, Some(request.request.request_id.as_str())))
    }

    /// Submits one previously planned embedding Batch task exactly once.
    /// 精确一次提交一个此前已规划的向量 Batch 任务。
    async fn submit_embedding_batch(
        &self,
        _handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: HostAiEmbeddingRequest,
        _request_fingerprint: String,
        _task: EmbeddingBatchTaskPlan,
        _uploaded_input: Option<EmbeddingBatchUploadedInput>,
    ) -> Result<EmbeddingBatchSubmission, HostAiError> {
        Err(HostAiError::new(
            HostAiErrorCode::CapabilityUnavailable,
            "The Host AI implementation does not support asynchronous embedding Batch submission.",
        )
        .with_request_context(&context, Some(request.request.request_id.as_str())))
    }

    /// Reconciles a file-backed task whose creation response was not durably observed.
    /// 对账一项创建响应未被持久观测的文件型任务。
    async fn reconcile_embedding_batch_submission(
        &self,
        _handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: HostAiEmbeddingRequest,
        _request_fingerprint: String,
        _task: EmbeddingBatchTaskPlan,
        _uploaded_input: EmbeddingBatchUploadedInput,
    ) -> Result<Option<EmbeddingBatchSubmission>, HostAiError> {
        Err(HostAiError::new(
            HostAiErrorCode::CapabilityUnavailable,
            "The Host AI implementation does not support embedding Batch submission reconciliation.",
        )
        .with_request_context(&context, Some(request.request.request_id.as_str())))
    }

    /// Polls one durable asynchronous embedding Batch submission without replaying it.
    /// 在不重放任务的情况下轮询一个持久异步向量 Batch 提交。
    async fn embedding_batch_status(
        &self,
        _handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: HostAiEmbeddingRequest,
        _request_fingerprint: String,
        _submission: EmbeddingBatchSubmission,
    ) -> Result<EmbeddingBatchSnapshot, HostAiError> {
        Err(HostAiError::new(
            HostAiErrorCode::CapabilityUnavailable,
            "The Host AI implementation does not support asynchronous embedding Batch polling.",
        )
        .with_request_context(&context, Some(request.request.request_id.as_str())))
    }

    /// Collects and normalizes one completed asynchronous embedding Batch task.
    /// 收集并归一化一个已完成异步向量 Batch 任务。
    async fn collect_embedding_batch(
        &self,
        _handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: HostAiEmbeddingRequest,
        _request_fingerprint: String,
        _task: EmbeddingBatchTaskPlan,
        _snapshot: EmbeddingBatchSnapshot,
    ) -> Result<EmbeddingBatchCollectedResult, HostAiError> {
        Err(HostAiError::new(
            HostAiErrorCode::CapabilityUnavailable,
            "The Host AI implementation does not support asynchronous embedding Batch collection.",
        )
        .with_request_context(&context, Some(request.request.request_id.as_str())))
    }

    /// Deletes a temporary provider input file only after the consumer durably stores all results.
    /// 仅在调用方持久保存全部结果后删除供应商临时输入文件。
    async fn cleanup_embedding_batch_input(
        &self,
        _handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: HostAiEmbeddingRequest,
        _request_fingerprint: String,
        _submission: EmbeddingBatchSubmission,
    ) -> Result<(), HostAiError> {
        Err(HostAiError::new(
            HostAiErrorCode::CapabilityUnavailable,
            "The Host AI implementation does not support embedding Batch input cleanup.",
        )
        .with_request_context(&context, Some(request.request.request_id.as_str())))
    }

    /// Requests cancellation of one durable asynchronous embedding Batch submission.
    /// 请求取消一个持久异步向量 Batch 提交。
    async fn cancel_embedding_batch(
        &self,
        _handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: HostAiEmbeddingRequest,
        _request_fingerprint: String,
        _submission: EmbeddingBatchSubmission,
    ) -> Result<EmbeddingBatchSnapshot, HostAiError> {
        Err(HostAiError::new(
            HostAiErrorCode::CapabilityUnavailable,
            "The Host AI implementation does not support asynchronous embedding Batch cancellation.",
        )
        .with_request_context(&context, Some(request.request.request_id.as_str())))
    }

    /// Executes one typed embedding request through the registered host route.
    /// 通过已注册宿主路由执行一次类型化向量请求。
    async fn create_embeddings(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: HostAiEmbeddingRequest,
    ) -> Result<HostAiEmbeddingResponse, HostAiError>;

    /// Executes one typed rerank request through the registered host route.
    /// 通过已注册宿主路由执行一次类型化重排请求。
    async fn rerank_documents(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: RerankRequest,
    ) -> Result<RerankResponse, HostAiError>;

    /// Executes one typed decision request through the registered host route.
    /// 通过已注册宿主路由执行一次类型化判定请求。
    async fn evaluate_decision(
        &self,
        _handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: DecisionRequest,
    ) -> Result<DecisionResponse, HostAiError> {
        Err(HostAiError::new(
            HostAiErrorCode::CapabilityUnavailable,
            "The Host AI implementation does not support decision evaluation.",
        )
        .with_request_context(&context, Some(request.request_id.as_str())))
    }

    /// Starts one typed Responses execution and returns its stable asynchronous handle.
    /// 启动一次类型化 Responses 执行并返回稳定的异步执行句柄。
    async fn start_response_execution(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: ResponsesExecutionRequest,
    ) -> Result<HostAiExecutionHandle, HostAiError>;

    /// Returns the current status and resolved route of one owned execution.
    /// 返回一项所属执行的当前状态及已解析路由。
    fn execution_status(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<HostAiExecutionSnapshot, HostAiError>;

    /// Opens a normalized replay-plus-live event stream for one owned execution.
    /// 为一项所属执行打开归一化重放加实时事件流。
    fn stream_response(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<HostAiEventStream, HostAiError>;

    /// Waits for one owned execution to reach a terminal result or cancellation.
    /// 等待一项所属执行达到终态结果或取消状态。
    async fn wait_response(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<ResponsesExecutionResult, HostAiError>;

    /// Requests cooperative cancellation of one owned execution.
    /// 请求协作式取消一项所属执行。
    fn cancel_execution(&self, handle: &HostAiExecutionHandle) -> Result<(), HostAiError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EmbeddingEncoding, EmbeddingInput, EmbeddingInputContent, EmbeddingInvalidInputPolicy,
        EmbeddingOutputKind, EmbeddingTask, INFERENCE_CONTRACT_VERSION,
    };

    /// Verifies that operation identities remain valid across an explicit retry context.
    /// 验证操作标识在显式重试上下文中保持有效。
    #[test]
    fn call_context_preserves_operation_identity_across_retry() {
        let context = HostAiCallContext::new("operation-1")
            .with_attempt(2, HostAiRetryReason::SchemaRepair)
            .with_core_correlation(
                Some(String::from("effect-1")),
                Some(String::from("command-1")),
            );

        assert_eq!(context.operation_id, "operation-1");
        assert_eq!(context.attempt, 2);
        assert_eq!(context.retry_reason, Some(HostAiRetryReason::SchemaRepair));
        assert!(context.validate("request-1").is_ok());
    }

    /// Verifies that logical vector-space identity is mandatory beside a typed embedding request.
    /// 验证逻辑向量空间标识必须与类型化 Embedding 请求同时存在。
    #[test]
    fn embedding_request_requires_logical_space_identity() {
        let request = EmbeddingRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("embedding-1"),
            idempotency_key: None,
            consumer_profile_id: String::from("profile-1"),
            inputs: vec![EmbeddingInput {
                input_id: String::from("input-1"),
                content: EmbeddingInputContent::Text {
                    text: String::from("text"),
                },
            }],
            task: EmbeddingTask::Document,
            dimensions: Some(2),
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            invalid_input_policy: EmbeddingInvalidInputPolicy::FailBatch,
        };
        let mut host_request = HostAiEmbeddingRequest {
            vector_space_id: String::from("knowledge_base:library-1"),
            request,
        };

        assert!(host_request.validate().is_ok());
        host_request.vector_space_id.clear();
        assert_eq!(
            host_request
                .validate()
                .expect_err("empty vector space must fail")
                .code,
            HostAiErrorCode::Contract
        );
    }
}
