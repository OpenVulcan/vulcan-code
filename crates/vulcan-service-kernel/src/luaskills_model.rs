//! LuaSkills internal model capability bridge.
//! LuaSkills 内部模型能力桥。
//!
//! LuaSkills exposes exactly two model entry points to Lua:
//! `vulcan.models.llm(system, user)` and `vulcan.models.embed(text)`. Both are
//! one-shot: the request carries only the caller-supplied text plus an
//! automatically attached `RuntimeModelCaller`, and the response carries a
//! single assistant text or one vector. This module maps those exact shapes onto
//! the shared host AI boundary (`HostAiPort`) under its own consumer identity.
//!
//! LuaSkills 只向 Lua 暴露两个模型入口：`vulcan.models.llm(system, user)` 与
//! `vulcan.models.embed(text)`。两者都是一次性的：请求只携带调用方给出的文本以及
//! 自动附加的 `RuntimeModelCaller`，响应只携带一段 assistant 文本或一个向量。
//! 本模块把这两个精确形状映射到共享的宿主 AI 边界（`HostAiPort`），并使用独立的
//! 消费者身份。
//!
//! Boundaries enforced here:
//! - Lua cannot select a model, provider, tool policy, history, or generation
//!   parameter, so this bridge never forwards any such input.
//! - No tool is ever exposed: `tools` and `native_tools` stay empty and the
//!   existing no-tool internal-call policies are reused unchanged.
//! - A tool-call item or a non-`completed` termination is a contract error, not
//!   a silently truncated answer.
//! - Provider detail never crosses back into Lua; errors are sanitized.
//!
//! 本模块强制的边界：
//! - Lua 无法选择模型、供应商、工具策略、历史或生成参数，因此本桥不转发任何此类输入。
//! - 永不暴露工具：`tools` 与 `native_tools` 保持为空，并沿用既有"无工具内部调用"策略。
//! - 出现工具调用项或非 `completed` 终态属于契约错误，而不是被静默截断的回答。
//! - 供应商细节绝不回传进 Lua；错误经过脱敏。

// The bridge returns the frozen diagnostic-rich HostAiError contract directly.
// 该桥直接返回已冻结且富含诊断信息的 HostAiError 契约。
#![allow(
    clippy::result_large_err,
    reason = "the bridge must preserve the frozen HostAiError boundary contract"
)]

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use luaskills::{
    RuntimeModelAvailabilityCallback, RuntimeModelEmbedCallback, RuntimeModelEmbedRequest,
    RuntimeModelEmbedResponse, RuntimeModelError, RuntimeModelErrorCode, RuntimeModelLlmCallback,
    RuntimeModelLlmRequest, RuntimeModelLlmResponse, RuntimeModelUsage,
    clear_model_embed_availability_callback_if, clear_model_embed_callback_if,
    clear_model_llm_availability_callback_if, clear_model_llm_callback_if,
    set_model_embed_availability_callback, set_model_embed_callback,
    set_model_llm_availability_callback, set_model_llm_callback,
};
use vulcan_ai_contract::{
    AiMessage, AiOperation, ContentItem, EmbeddingEncoding, EmbeddingInput, EmbeddingInputContent,
    EmbeddingInvalidInputPolicy, EmbeddingOutputKind, EmbeddingPayload, EmbeddingRequest,
    EmbeddingTask, HostAiCallContext, HostAiCancellation, HostAiCapabilityHandle,
    HostAiCapabilityRegistration, HostAiCapabilityRequirement, HostAiEmbeddingRequest, HostAiError,
    HostAiErrorCode, HostAiPort, INFERENCE_CONTRACT_VERSION, InferenceCallerBudget,
    InferenceCallerContext, InferenceCapabilitySnapshot, InferencePermission,
    InferenceSchedulingClass, InferenceUsageScope, MessageRole, ParallelToolCallPolicy,
    ReasoningOutputPolicy, ResponsesExecutionRequest, ToolChoicePolicy, UsageValue,
};

/// Configured inference consumer profile owned by LuaSkills.
/// LuaSkills 拥有的已配置推理消费者 Profile。
pub(crate) const LUASKILLS_CONSUMER_PROFILE_ID: &str = "luaskills";

/// New LuaSkills-only LLM purpose key inside its own consumer profile.
/// LuaSkills 专属消费者 Profile 内新增的 LLM 用途键。
pub(crate) const LUASKILLS_TEMPORARY_LLM_PURPOSE_ID: &str = "temporary_llm";

/// Stable registration identity used for replacement and removal.
/// 用于替换与移除的稳定注册标识。
const LUASKILLS_REGISTRATION_ID: &str = "vulcan-code-luaskills-model";

/// Stable owner identity reported to the host AI boundary.
/// 向宿主 AI 边界报告的稳定所有者标识。
const LUASKILLS_OWNER_ID: &str = "vulcan-code-service-luaskills";

/// Trusted caller identity issued by the composition root.
/// 由组合根签发的可信调用方标识。
const LUASKILLS_CALLER_ID: &str = "vulcan-code-luaskills";

/// Stable vector-space identity for the single LuaSkills embedding route.
/// LuaSkills 单条向量路由使用的稳定向量空间标识。
const LUASKILLS_VECTOR_SPACE_ID: &str = "luaskills.embedding";

/// Embedding task requested for LuaSkills callers.
/// LuaSkills 调用方请求的向量任务。
const LUASKILLS_EMBEDDING_TASK: EmbeddingTask = EmbeddingTask::ProviderDefault;

/// Upper bound one synchronous Lua model callback may block its caller thread.
/// 同步 Lua 模型回调允许阻塞其调用线程的上限。
///
/// The bound keeps a hostile or slow provider from pinning an engine read guard,
/// a per-call OS thread, and a main-pool VM lease indefinitely. Expiry is
/// reported as an unknown outcome and is never retried blindly.
///
/// 该上限阻止敌意或缓慢的供应商无限期占住引擎读守卫、按调用创建的 OS 线程和主池 VM
/// 租约。超时按"结果未知"上报，且绝不盲目重试。
const LUASKILLS_MODEL_CALL_TIMEOUT: Duration = Duration::from_secs(120);

/// Host-side ceiling on concurrent LuaSkills model calls.
/// 宿主侧对并发 LuaSkills 模型调用的上限。
///
/// Concurrent model calls can only arise from concurrent Lua executions, which are
/// bounded by the configured VM pools and plugin leases rather than by request
/// volume, so this is a safety ceiling and not a throughput target.
///
/// 并发模型调用只可能来自并发 Lua 执行，而后者由已配置的 VM 池与插件租约限制，
/// 而不是由请求量限制；因此这是一个安全上限，不是吞吐目标。
const LUASKILLS_MAX_CONCURRENT_MODEL_CALLS: u32 = 16;

/// Host-side ceiling on input units accepted for one LuaSkills model call.
/// 宿主侧对单次 LuaSkills 模型调用可接受输入单元的上限。
const LUASKILLS_MAX_INPUT_UNITS_PER_REQUEST: u64 = 262_144;

/// Process-wide sequence that prevents model-operation identity reuse after owner replacement.
/// 防止模型所有者替换后复用操作身份的进程级序号。
static NEXT_LUASKILLS_MODEL_OPERATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Host budget duration, derived from the synchronous bound plus a grace period.
/// 宿主预算时长，由同步上限加宽限期推导。
const LUASKILLS_MODEL_CALL_BUDGET_MS: u64 =
    LUASKILLS_MODEL_CALL_TIMEOUT.as_secs() * 1_000 + LUASKILLS_MODEL_CALL_BUDGET_GRACE_MS;

/// Grace before the host cancels work orphaned by the synchronous bound.
/// 宿主取消被同步上限遗弃的工作之前的宽限期。
const LUASKILLS_MODEL_CALL_BUDGET_GRACE_MS: u64 = 10_000;

/// Unix millisecond deadline helper used for one bounded call.
/// 单次有界调用使用的 Unix 毫秒截止时间辅助函数。
fn unix_deadline_ms(timeout: Duration) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from((now + timeout).as_millis()).unwrap_or(u64::MAX)
}

/// Host-owned cooperative cancellation backed by one atomic flag.
/// 以一个原子标志实现的宿主持有协作式取消。
#[derive(Default)]
struct FlagCancellation {
    /// Set once the bridge abandons waiting for this call.
    /// 当本桥放弃等待该调用时置位。
    cancelled: AtomicBool,
    /// Wakes a waiter that is already blocked.
    /// 唤醒已经阻塞的等待方。
    notify: tokio::sync::Notify,
}

impl FlagCancellation {
    /// Marks the operation cancelled and wakes any waiting implementation.
    /// 将操作标记为已取消，并唤醒任何正在等待的实现。
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }
}

impl HostAiCancellation for FlagCancellation {
    /// Reports whether the host bridge has cancelled this operation.
    /// 返回宿主桥是否已取消本次操作。
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Waits until the host bridge cancels this operation.
    /// 等待宿主桥取消本次操作。
    fn cancelled<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            loop {
                let notified = self.notify.notified();
                tokio::pin!(notified);
                // Register before observing the atomic flag because `notify_waiters`
                // does not retain a permit for a waiter created after notification.
                // 先注册等待方再观察原子标志，因为 `notify_waiters` 不会为通知后才创建的
                // 等待方保留许可。
                notified.as_mut().enable();
                if self.is_cancelled() {
                    return;
                }
                notified.await;
            }
        })
    }
}

/// Builds one sanitized LuaSkills model error.
/// 构造一个经过脱敏的 LuaSkills 模型错误。
///
/// Provider-supplied messages and codes are deliberately dropped: they describe
/// upstream infrastructure and must not cross back into Lua.
///
/// 供应商给出的消息与代码被刻意丢弃：它们描述上游基础设施，不得回传进 Lua。
fn model_error(code: RuntimeModelErrorCode, message: impl Into<String>) -> RuntimeModelError {
    RuntimeModelError {
        code,
        message: message.into(),
        provider_message: None,
        provider_code: None,
        provider_status: None,
    }
}

/// Converts one host usage counter into the optional LuaSkills token count.
/// 把宿主用量计数转换为可选的 LuaSkills token 计数。
///
/// `Unknown` stays `None` rather than becoming a fabricated zero.
/// `Unknown` 保持为 `None`，而不是伪造成 0。
fn usage_tokens(value: &UsageValue) -> Option<u64> {
    match value {
        UsageValue::Known { value, .. } => Some(*value),
        UsageValue::Unknown => None,
    }
}

/// Projects host token counters without inventing an empty usage observation.
/// 投影宿主 token 计数，同时避免凭空构造空的用量观测。
///
/// The outer `Option` records whether the host observed any token counter at all.
/// 外层 `Option` 表示宿主是否确实观测到至少一个 token 计数。
///
/// `output` is absent for embedding operations because that upstream response has no
/// output-token dimension; an unknown LLM output counter is passed as `Some(Unknown)`.
/// 向量操作没有输出 token 维度，因此 `output` 缺省；LLM 输出计数未知时传入
/// `Some(Unknown)`，以保留两类语义之间的区别。
fn runtime_model_usage(
    input: &UsageValue,
    output: Option<&UsageValue>,
) -> Option<RuntimeModelUsage> {
    let input_tokens = usage_tokens(input);
    let output_tokens = output.and_then(usage_tokens);
    if input_tokens.is_none() && output_tokens.is_none() {
        return None;
    }
    Some(RuntimeModelUsage {
        input_tokens,
        output_tokens,
    })
}

/// Maps one host AI failure onto a sanitized LuaSkills model error.
/// 把一个宿主 AI 失败映射为脱敏的 LuaSkills 模型错误。
fn map_host_error(error: &HostAiError) -> RuntimeModelError {
    let code = match error.code {
        HostAiErrorCode::CapabilityUnavailable | HostAiErrorCode::Configuration => {
            RuntimeModelErrorCode::ModelUnavailable
        }
        // LuaSkills has no authorization identity of its own, so a rejected
        // registration or permission is reported as unavailability rather than
        // as something the Lua caller could retry differently.
        // LuaSkills 没有自己的授权身份，因此被拒绝的注册或权限按"不可用"上报，
        // 而不是上报成 Lua 调用方可以换个方式重试的错误。
        HostAiErrorCode::PermissionDenied | HostAiErrorCode::Authentication => {
            RuntimeModelErrorCode::ModelUnavailable
        }
        HostAiErrorCode::InvalidRequest
        | HostAiErrorCode::Contract
        | HostAiErrorCode::DimensionMismatch
        | HostAiErrorCode::FingerprintMismatch => RuntimeModelErrorCode::InvalidArgument,
        HostAiErrorCode::LimitExceeded | HostAiErrorCode::PaymentRequired => {
            RuntimeModelErrorCode::BudgetExceeded
        }
        // Provider is a classified provider-side execution failure, so it reads as a
        // provider error rather than as an unclassified host failure: the retry guidance
        // the Lua caller derives from the code differs between the two.
        // Provider 是已分类的供应商侧执行失败，因此按供应商错误上报，而不是按未分类的宿主
        // 失败上报：Lua 调用方从代码得出的重试指引在两者间不同。
        HostAiErrorCode::RateLimited
        | HostAiErrorCode::ProviderOverloaded
        | HostAiErrorCode::Provider => RuntimeModelErrorCode::ProviderError,
        HostAiErrorCode::ProviderUnavailable => RuntimeModelErrorCode::ModelUnavailable,
        HostAiErrorCode::Timeout => RuntimeModelErrorCode::Timeout,
        // Cancellation, missing executions, unclassified internal failures, and any future
        // code are host-internal conditions with no Lua-actionable meaning.
        // 取消、执行缺失、未分类的内部失败以及任何未来新增代码都是宿主内部条件，对 Lua
        // 没有可操作含义。
        _ => RuntimeModelErrorCode::InternalError,
    };
    // The host message is not forwarded: it can embed provider details.
    // 宿主消息不转发：它可能包含供应商细节。
    model_error(code, String::from("LuaSkills host model call failed."))
}

/// Process-local bridge holding the shared port and the LuaSkills capability.
/// 持有共享 Port 与 LuaSkills 能力的进程内桥。
///
/// The capability handle is registered lazily on first use and refreshed by
/// snapshot afterwards, mirroring the native VMM precedent. Re-registering on
/// every dispatch would churn the port registry and its replacement semantics.
///
/// 能力句柄在首次使用时惰性注册，之后只刷新快照，与内置 VMM 的先例一致。
/// 每次派发都重新注册会搅动 Port 注册表及其替换语义。
pub(crate) struct LuaSkillsModelBridge {
    /// Shared host AI boundary owned by the composition root.
    /// 由组合根持有的共享宿主 AI 边界。
    port: Arc<dyn HostAiPort>,
    /// Runtime used to drive the asynchronous port from a synchronous callback.
    /// 用于从同步回调驱动异步 Port 的运行时。
    runtime: tokio::runtime::Handle,
    /// Lazily registered capability handle.
    /// 惰性注册的能力句柄。
    capability: Mutex<Option<HostAiCapabilityHandle>>,
    /// Upper bound one synchronous callback blocks its caller thread.
    /// 同步回调阻塞其调用线程的上限。
    ///
    /// Held per bridge so tests can exercise bounded exit without waiting the
    /// production bound; production always uses [`LUASKILLS_MODEL_CALL_TIMEOUT`].
    /// 按桥持有，使测试无需等待生产上限即可验证有界退出；生产始终使用
    /// [`LUASKILLS_MODEL_CALL_TIMEOUT`]。
    call_timeout: Duration,
}

impl LuaSkillsModelBridge {
    /// Creates one bridge over the shared port and runtime handle.
    /// 在共享 Port 与运行时句柄之上创建一个桥。
    pub(crate) fn new(port: Arc<dyn HostAiPort>, runtime: tokio::runtime::Handle) -> Arc<Self> {
        Self::new_with_timeout(port, runtime, LUASKILLS_MODEL_CALL_TIMEOUT)
    }

    /// Creates one bridge with an explicit synchronous bound.
    /// 以显式同步上限创建一个桥。
    fn new_with_timeout(
        port: Arc<dyn HostAiPort>,
        runtime: tokio::runtime::Handle,
        call_timeout: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            port,
            runtime,
            capability: Mutex::new(None),
            call_timeout,
        })
    }

    /// Builds the LuaSkills capability registration.
    /// 构造 LuaSkills 能力注册。
    ///
    /// Both requirements are optional on purpose: an operator may configure only
    /// the LLM route or only the embedding route, and a missing half must not
    /// disable the other.
    ///
    /// 两项要求刻意都声明为可选：运维方可能只配置 LLM 路由或只配置向量路由，缺失其一
    /// 不得关闭另一项。
    fn registration() -> HostAiCapabilityRegistration {
        HostAiCapabilityRegistration {
            registration_id: String::from(LUASKILLS_REGISTRATION_ID),
            owner_id: String::from(LUASKILLS_OWNER_ID),
            consumer_profile_id: String::from(LUASKILLS_CONSUMER_PROFILE_ID),
            caller: InferenceCallerContext {
                caller_id: String::from(LUASKILLS_CALLER_ID),
                allowed_consumer_profile_ids: std::collections::BTreeSet::from([String::from(
                    LUASKILLS_CONSUMER_PROFILE_ID,
                )]),
                permissions: std::collections::BTreeSet::from([
                    InferencePermission::Status,
                    InferencePermission::Llm,
                    InferencePermission::Embedding,
                ]),
                // LuaSkills model work is Vulcan-owned background work requested
                // by skill code, not user-driven interactive agent work.
                // LuaSkills 模型工作是由技能代码发起的 Vulcan 自有后台工作，
                // 不是用户驱动的交互式智能体工作。
                usage_scope: InferenceUsageScope::InternalBackground,
                scheduling_class: InferenceSchedulingClass::ByUsageScope,
                interactive_session_id: None,
                owner_session_id: None,
                deadline_unix_ms: None,
                budget: InferenceCallerBudget {
                    // Bounded by the number of concurrent Lua executions rather
                    // than by request volume.
                    // 由并发 Lua 执行数限制，而不是由请求量限制。
                    max_concurrent_executions: LUASKILLS_MAX_CONCURRENT_MODEL_CALLS,
                    max_input_units_per_request: Some(LUASKILLS_MAX_INPUT_UNITS_PER_REQUEST),
                    // Generation limits come from the exact configured route, not
                    // from Lua, so no extra host-side token cap is imposed here.
                    // 生成限制来自精确配置的路由而非 Lua，因此此处不额外施加宿主侧 token 上限。
                    max_output_tokens_per_request: None,
                    max_requests_per_minute: None,
                    max_input_units_per_minute: None,
                    // Outlives the synchronous bound by a grace period so a call
                    // that finishes just after the bound still completes cleanly,
                    // while work orphaned by a timeout is cancelled instead of
                    // running unbounded in the background.
                    // 比同步上限多出一个宽限期，使刚好在超时后完成的调用仍能正常收敛；
                    // 同时让被超时遗弃的工作被取消，而不是在后台无界运行。
                    max_execution_duration_ms: Some(LUASKILLS_MODEL_CALL_BUDGET_MS),
                },
            },
            requirements: vec![
                HostAiCapabilityRequirement {
                    operation: AiOperation::ConversationRespond,
                    purpose_id: Some(String::from(LUASKILLS_TEMPORARY_LLM_PURPOSE_ID)),
                    required: false,
                },
                HostAiCapabilityRequirement {
                    operation: AiOperation::EmbeddingCreate,
                    purpose_id: None,
                    required: false,
                },
            ],
        }
    }

    /// Returns the LuaSkills capability, registering it on first use.
    /// 返回 LuaSkills 能力，并在首次使用时注册。
    ///
    /// An unconfigured consumer profile is not an error condition for startup:
    /// the caller decides how to surface it. Here it is reported so the callback
    /// can return a structured `model_unavailable` to Lua.
    ///
    /// 未配置的消费者 Profile 不是启动错误条件：由调用方决定如何呈现。此处按错误上报，
    /// 使回调能向 Lua 返回结构化的 `model_unavailable`。
    fn capability(&self) -> Result<HostAiCapabilityHandle, HostAiError> {
        let mut guard = match self.capability.lock() {
            Ok(guard) => guard,
            // A poisoned lock only means some earlier holder panicked; the handle
            // behind it is still the authoritative registration.
            // 锁中毒只说明先前某个持有者 panic 了；其中的句柄仍是权威注册。
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(existing) = guard.as_ref() {
            // Existing handle: refresh the snapshot so has/status reflects live
            // routes without changing handle identity.
            // 已有句柄：刷新快照，使 has/status 反映实时路由而不改变句柄身份。
            let mut refreshed = existing.clone();
            refreshed.snapshot = self.port.capability_snapshot(existing)?;
            *guard = Some(refreshed.clone());
            return Ok(refreshed);
        }
        let handle = self.port.register_capability(Self::registration())?;
        *guard = Some(handle.clone());
        Ok(handle)
    }

    /// Drops the cached handle and removes the registration from the port.
    /// 丢弃缓存的句柄并从 Port 移除注册。
    pub(crate) fn unregister(&self) {
        if let Ok(mut guard) = self.capability.lock() {
            *guard = None;
        }
        let _ = self.port.unregister_capability(LUASKILLS_REGISTRATION_ID);
    }

    /// Allocates the next process-wide stable per-call identity.
    /// 分配下一个进程级稳定逐调用标识。
    fn next_operation_id(&self, prefix: &str) -> String {
        let value = NEXT_LUASKILLS_MODEL_OPERATION_SEQUENCE.fetch_add(1, Ordering::SeqCst);
        format!("luaskills-{prefix}-{value}")
    }

    /// Runs one asynchronous port call with a bounded synchronous wait.
    /// 以有界同步等待运行一次异步 Port 调用。
    ///
    /// The work is spawned onto the shared runtime rather than onto a freshly
    /// created thread, so a slow provider cannot cause unbounded thread growth.
    /// When the bound expires or delivery disconnects, the callback signals
    /// cancellation and returns without retrying the unknown outcome.
    ///
    /// 工作被投递到共享运行时，而不是新建线程，因此缓慢供应商不会导致线程无界增长。
    /// 超出上限或结果投递断开时，回调发出取消信号并返回，且不会重试未知终态。
    fn call_bounded<T, F>(
        &self,
        cancellation: &Arc<FlagCancellation>,
        future: F,
    ) -> Result<T, RuntimeModelError>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, HostAiError>> + Send + 'static,
    {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<Result<T, HostAiError>>(1);
        self.runtime.spawn(async move {
            let outcome = future.await;
            // The receiver may already be gone after a bound expiry; dropping the
            // late result is correct because a timeout is an unknown outcome.
            // 上限到期后接收端可能已经消失；丢弃迟到结果是正确的，因为超时属于未知结果。
            let _ = sender.send(outcome);
        });
        match receiver.recv_timeout(self.call_timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(map_host_error(&error)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                cancellation.cancel();
                Err(model_error(
                    RuntimeModelErrorCode::Timeout,
                    "LuaSkills model call exceeded its host deadline; the outcome is unknown.",
                ))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                cancellation.cancel();
                Err(model_error(
                    RuntimeModelErrorCode::InternalError,
                    "LuaSkills model call channel closed before returning a result.",
                ))
            }
        }
    }

    /// Builds one host call context for a LuaSkills model call.
    /// 为一次 LuaSkills 模型调用构造宿主持有调用上下文。
    ///
    /// LuaSkills is not a VMM-Core effect, so the Core correlation identifiers
    /// stay unset.
    ///
    /// LuaSkills 不是 VMM-Core effect，因此 Core 关联标识保持未设置。
    fn call_context(
        &self,
        operation_id: &str,
        cancellation: Arc<FlagCancellation>,
    ) -> HostAiCallContext {
        HostAiCallContext::new(String::from(operation_id))
            .with_deadline(Some(unix_deadline_ms(self.call_timeout)))
            .with_cancellation(cancellation)
    }

    /// Dispatches one one-shot LuaSkills LLM question.
    /// 派发一次性的 LuaSkills LLM 问答。
    pub(crate) fn dispatch_llm(
        &self,
        request: &RuntimeModelLlmRequest,
    ) -> Result<RuntimeModelLlmResponse, RuntimeModelError> {
        let handle = self.capability().map_err(|error| map_host_error(&error))?;
        let operation_id = self.next_operation_id("llm");
        let cancellation = Arc::new(FlagCancellation::default());
        let context = self.call_context(&operation_id, Arc::clone(&cancellation));
        let execution_request = ResponsesExecutionRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: operation_id,
            idempotency_key: None,
            consumer_profile_id: String::from(LUASKILLS_CONSUMER_PROFILE_ID),
            purpose_id: String::from(LUASKILLS_TEMPORARY_LLM_PURPOSE_ID),
            // Exactly the caller-supplied system and user turns; no history.
            // 仅调用方给出的 system 与 user 两轮；没有历史。
            messages: vec![
                AiMessage {
                    role: MessageRole::System,
                    content: vec![ContentItem::Text {
                        text: request.system.clone(),
                    }],
                    provider_context_items: Vec::new(),
                },
                AiMessage {
                    role: MessageRole::User,
                    content: vec![ContentItem::Text {
                        text: request.user.clone(),
                    }],
                    provider_context_items: Vec::new(),
                },
            ],
            // Lua has no tool surface, so both tool selection axes are explicitly
            // disabled instead of inheriting provider defaults.
            // Lua 没有工具面，因此显式关闭两个工具选择维度，而不是继承供应商默认值。
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: ToolChoicePolicy::Disabled,
            parallel_tool_calls: ParallelToolCallPolicy::Disabled,
            structured_output: Default::default(),
            prompt_cache: Default::default(),
            prompt_cache_key: None,
            // Reasoning continuation payloads must never reach Lua.
            // 推理续跑载荷绝不进入 Lua。
            include_reasoning_encrypted_content: false,
            reasoning_effort: None,
            reasoning_output: ReasoningOutputPolicy::Hidden,
            model_parameters: BTreeMap::new(),
            stop_sequences: Vec::new(),
            resources: Vec::new(),
            output_token_limit: None,
        };
        let port = Arc::clone(&self.port);
        let result = self.call_bounded(&cancellation, async move {
            let execution = port
                .start_response_execution(&handle, context, execution_request)
                .await?;
            port.wait_response(&execution).await
        })?;
        // A non-completed termination or output that the Lua one-shot response
        // cannot represent is a contract error, never a partially valid answer.
        // 非 completed 终态或 Lua 一次性响应无法表达的输出属于契约错误，绝不是部分有效的回答。
        if result.termination_reason != "completed" {
            return Err(model_error(
                RuntimeModelErrorCode::InternalError,
                format!(
                    "LuaSkills model call ended with an unusable termination state `{}`.",
                    result.termination_reason
                ),
            ));
        }
        if result.structured_output.is_some() {
            return Err(model_error(
                RuntimeModelErrorCode::InternalError,
                "LuaSkills model call returned structured output, which this one-shot text API cannot represent.",
            ));
        }
        if let Some(kind) = result
            .output_items
            .iter()
            .find_map(|item| match &item.content {
                // Reasoning is intentionally hidden and does not change the one-shot assistant payload.
                // 推理内容按约定隐藏，不改变一次性 assistant 载荷。
                ContentItem::Reasoning { .. } => None,
                ContentItem::Text { .. } => Some("duplicate text item"),
                ContentItem::StructuredJson { .. } => Some("structured output item"),
                ContentItem::Resource { .. } => Some("resource item"),
                ContentItem::ToolCall { .. } => Some("tool call"),
                ContentItem::ToolResult { .. } => Some("tool result"),
                ContentItem::Refusal { .. } => Some("refusal"),
                ContentItem::Citation { .. } => Some("citation"),
            })
        {
            return Err(model_error(
                RuntimeModelErrorCode::InternalError,
                format!(
                    "LuaSkills model call returned a {kind}, which this one-shot text API cannot represent."
                ),
            ));
        }
        Ok(RuntimeModelLlmResponse {
            assistant: result.output_text,
            usage: runtime_model_usage(
                &result.usage.input_tokens,
                Some(&result.usage.output_tokens),
            ),
        })
    }

    /// Dispatches one one-shot LuaSkills embedding request.
    /// 派发一次性的 LuaSkills 向量请求。
    pub(crate) fn dispatch_embed(
        &self,
        request: &RuntimeModelEmbedRequest,
    ) -> Result<RuntimeModelEmbedResponse, RuntimeModelError> {
        let handle = self.capability().map_err(|error| map_host_error(&error))?;
        let operation_id = self.next_operation_id("embed");
        let cancellation = Arc::new(FlagCancellation::default());
        let context = self.call_context(&operation_id, Arc::clone(&cancellation));
        let host_request = HostAiEmbeddingRequest {
            vector_space_id: String::from(LUASKILLS_VECTOR_SPACE_ID),
            request: EmbeddingRequest {
                contract_version: INFERENCE_CONTRACT_VERSION,
                request_id: operation_id,
                idempotency_key: None,
                consumer_profile_id: String::from(LUASKILLS_CONSUMER_PROFILE_ID),
                inputs: vec![EmbeddingInput {
                    input_id: String::from("input-0"),
                    content: EmbeddingInputContent::Text {
                        text: request.text.clone(),
                    },
                }],
                task: LUASKILLS_EMBEDDING_TASK,
                dimensions: None,
                output_kind: EmbeddingOutputKind::Dense,
                encoding: EmbeddingEncoding::Float,
                invalid_input_policy: EmbeddingInvalidInputPolicy::FailBatch,
            },
        };
        let port = Arc::clone(&self.port);
        let response = self.call_bounded(&cancellation, async move {
            port.create_embeddings(&handle, context, host_request).await
        })?;
        let first = response.response.results.first().ok_or_else(|| {
            model_error(
                RuntimeModelErrorCode::InternalError,
                "LuaSkills embedding call returned no vector for its single input.",
            )
        })?;
        let vector = match &first.embedding {
            EmbeddingPayload::Dense { values } => values.clone(),
            _ => {
                return Err(model_error(
                    RuntimeModelErrorCode::InternalError,
                    "LuaSkills embedding call returned a non-dense payload.",
                ));
            }
        };
        Ok(RuntimeModelEmbedResponse {
            dimensions: vector.len(),
            vector,
            usage: runtime_model_usage(&response.response.usage.input_tokens, None),
        })
    }
}

/// Sole owner of the two process-wide LuaSkills model callbacks.
/// 两个进程级 LuaSkills 模型回调的唯一所有者。
///
/// Upstream installs callbacks into one process-wide slot per capability and a
/// later install silently replaces the previous one. Concentrating ownership in
/// one value keeps repeated bootstrap idempotent and guarantees that shutdown
/// clears exactly what this service installed.
///
/// 上游把回调安装进每个能力一个的进程级槽，后一次安装会静默替换前一次。把所有权集中在
/// 单个值中，可让重复引导保持幂等，并保证关闭时精确清理本服务安装的内容。
pub(crate) struct LuaSkillsModelCallbackOwner {
    /// Bridge shared by both installed callbacks.
    /// 两个已安装回调共享的桥。
    bridge: Arc<LuaSkillsModelBridge>,
    /// Callback installed into the upstream LLM dispatch slot.
    /// 安装进上游 LLM 派发槽的回调。
    llm_callback: RuntimeModelLlmCallback,
    /// Callback installed into the upstream embedding dispatch slot.
    /// 安装进上游向量派发槽的回调。
    embed_callback: RuntimeModelEmbedCallback,
    /// Live capability probe installed for Lua-facing LLM discovery.
    /// 为面向 Lua 的 LLM 能力发现安装的实时探针。
    llm_availability_callback: RuntimeModelAvailabilityCallback,
    /// Live capability probe installed for Lua-facing embedding discovery.
    /// 为面向 Lua 的向量能力发现安装的实时探针。
    embed_availability_callback: RuntimeModelAvailabilityCallback,
    /// Whether this owner may still dispatch or unregister its Host AI capability.
    /// 本所有者是否仍可派发或注销其宿主 AI 能力。
    active: Arc<AtomicBool>,
    /// Lifecycle gate held for the complete synchronous callback or shutdown.
    /// 在整次同步回调或关闭期间持有的生命周期门。
    lifecycle: Arc<RwLock<()>>,
}

impl LuaSkillsModelCallbackOwner {
    /// Installs both LuaSkills model callbacks over the shared host AI port.
    /// 在共享宿主 AI Port 之上安装两个 LuaSkills 模型回调。
    ///
    /// Building the owner does not touch the upstream slots; the caller decides
    /// when to align them, because `sync_availability` needs this owner to be the
    /// process-wide owner already.
    /// 构造所有者不触碰上游槽；由调用方决定何时对齐，因为 `sync_availability` 需要本
    /// 所有者已经是进程级所有者。
    pub(crate) fn install(port: Arc<dyn HostAiPort>, runtime: tokio::runtime::Handle) -> Arc<Self> {
        let bridge = LuaSkillsModelBridge::new(port, runtime);
        let active = Arc::new(AtomicBool::new(true));
        let lifecycle = Arc::new(RwLock::new(()));
        let llm_bridge = Arc::clone(&bridge);
        let llm_active = Arc::clone(&active);
        let llm_lifecycle = Arc::clone(&lifecycle);
        let llm_callback: RuntimeModelLlmCallback = Arc::new(
            move |request: &RuntimeModelLlmRequest| -> Result<_, RuntimeModelError> {
                let _lifecycle_guard = match llm_lifecycle.read() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if !llm_active.load(Ordering::Acquire) {
                    return Err(model_error(
                        RuntimeModelErrorCode::ModelUnavailable,
                        "LuaSkills host model bridge is no longer active.",
                    ));
                }
                llm_bridge.dispatch_llm(request)
            },
        );
        let embed_bridge = Arc::clone(&bridge);
        let embed_active = Arc::clone(&active);
        let embed_lifecycle = Arc::clone(&lifecycle);
        let embed_callback: RuntimeModelEmbedCallback = Arc::new(
            move |request: &RuntimeModelEmbedRequest| -> Result<_, RuntimeModelError> {
                let _lifecycle_guard = match embed_lifecycle.read() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if !embed_active.load(Ordering::Acquire) {
                    return Err(model_error(
                        RuntimeModelErrorCode::ModelUnavailable,
                        "LuaSkills host model bridge is no longer active.",
                    ));
                }
                embed_bridge.dispatch_embed(request)
            },
        );
        let llm_availability_bridge = Arc::clone(&bridge);
        let llm_availability_active = Arc::clone(&active);
        let llm_availability_lifecycle = Arc::clone(&lifecycle);
        let llm_availability_callback: RuntimeModelAvailabilityCallback = Arc::new(move || {
            let _lifecycle_guard = match llm_availability_lifecycle.read() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            llm_availability_active.load(Ordering::Acquire)
                && bridge_availability(&llm_availability_bridge).llm
        });
        let embed_availability_bridge = Arc::clone(&bridge);
        let embed_availability_active = Arc::clone(&active);
        let embed_availability_lifecycle = Arc::clone(&lifecycle);
        let embed_availability_callback: RuntimeModelAvailabilityCallback = Arc::new(move || {
            let _lifecycle_guard = match embed_availability_lifecycle.read() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            embed_availability_active.load(Ordering::Acquire)
                && bridge_availability(&embed_availability_bridge).embedding
        });
        Arc::new(Self {
            bridge,
            llm_callback,
            embed_callback,
            llm_availability_callback,
            embed_availability_callback,
            active,
            lifecycle,
        })
    }

    /// Installs dispatch callbacks and their live availability probes into upstream slots.
    /// 把派发回调及其实时可用性探针安装进上游槽。
    fn install_callbacks(&self) {
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        // Probes are installed first so callback discovery cannot briefly expose an
        // unusable route through the legacy dispatch-slot fallback.
        // 先安装探针，避免能力发现短暂通过旧派发槽回退暴露不可用路由。
        set_model_llm_availability_callback(Some(Arc::clone(&self.llm_availability_callback)));
        set_model_embed_availability_callback(Some(Arc::clone(&self.embed_availability_callback)));
        set_model_llm_callback(Some(Arc::clone(&self.llm_callback)));
        set_model_embed_callback(Some(Arc::clone(&self.embed_callback)));
    }

    /// Returns the installed bridge for status projection and tests.
    /// 返回已安装的桥，供状态投影与测试使用。
    #[cfg(test)]
    pub(crate) fn bridge(&self) -> &Arc<LuaSkillsModelBridge> {
        &self.bridge
    }

    /// Reads the live capability snapshot used by Lua discovery and settings projection.
    /// 读取 Lua 能力发现与设置投影共同使用的实时能力快照。
    ///
    /// Upstream invokes the same owner-held probes on every `has`/`status` call,
    /// while actual dispatch refreshes the capability again before execution.
    /// 上游在每次 `has`/`status` 调用时执行同一所有者持有的探针，实际派发前还会再次
    /// 刷新能力。
    pub(crate) fn sync_availability(&self) -> LuaSkillsModelAvailability {
        // Direct settings projections use this method as well as the installed
        // availability callbacks. Hold the lifecycle read gate for both paths so
        // shutdown cannot unregister between the active check and a lazy register
        // or snapshot refresh.
        // 设置投影会直接调用本方法，已安装的可用性回调也会调用它。两条路径都持有生命周期
        // 读门，防止关闭在活动检查与惰性注册或快照刷新之间注销能力。
        let _lifecycle_guard = match self.lifecycle.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !self.active.load(Ordering::Acquire) {
            return LuaSkillsModelAvailability {
                installed: false,
                llm: false,
                embedding: false,
            };
        }
        bridge_availability(&self.bridge)
    }

    /// Clears both callbacks and removes the LuaSkills capability registration.
    /// 清除两个回调并移除 LuaSkills 能力注册。
    pub(crate) fn shutdown(&self) {
        // A writer waits for callbacks that passed admission and excludes new ones.
        // 写者等待已通过入场的回调结束，并排除新的回调进入。
        let _lifecycle_guard = match self.lifecycle.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let was_active = self.active.swap(false, Ordering::AcqRel);
        // Dispatch slots are cleared first so legacy fallback can never report a
        // capability after this owner starts shutting down. Conditional removal
        // prevents an obsolete owner from clearing a newer bootstrap generation.
        // 先清理派发槽，确保本所有者开始关闭后旧回退绝不会继续报告能力。条件移除还可
        // 防止过期所有者清掉更新的引导代次。
        clear_model_llm_callback_if(&self.llm_callback);
        clear_model_embed_callback_if(&self.embed_callback);
        clear_model_llm_availability_callback_if(&self.llm_availability_callback);
        clear_model_embed_availability_callback_if(&self.embed_availability_callback);
        if was_active {
            self.bridge.unregister();
        }
    }
}

impl Drop for LuaSkillsModelCallbackOwner {
    /// Shuts down the exact callbacks and capability registration owned by this value.
    /// 清理本值精确持有的回调与能力注册。
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Process-wide slot holding the single owner of the LuaSkills model callbacks.
/// 持有 LuaSkills 模型回调唯一所有者的进程级槽。
///
/// Upstream callbacks are process-wide, so the owner must be too; otherwise two
/// bootstraps in one process could each believe they own the slots.
///
/// 上游回调是进程级的，因此所有者也是进程级的；否则同一进程内的两次引导会各自以为
/// 自己拥有这些槽。
static GLOBAL_OWNER: Mutex<Option<Arc<LuaSkillsModelCallbackOwner>>> = Mutex::new(None);

/// Locks the process-wide owner slot, recovering from a poisoned mutex.
/// 锁定进程级所有者槽，并在互斥锁中毒时恢复。
fn lock_global_owner() -> std::sync::MutexGuard<'static, Option<Arc<LuaSkillsModelCallbackOwner>>> {
    match GLOBAL_OWNER.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Installs the LuaSkills model callbacks as the process-wide owner.
/// 以进程级所有者身份安装 LuaSkills 模型回调。
///
/// Replacement and installation are serialized by the owner slot. Conditional
/// upstream cleanup also makes a delayed obsolete owner harmless.
///
/// 所有者槽串行化替换与安装；上游条件清理也使延迟析构的过期所有者无害。
pub(crate) fn install_global(
    port: Arc<dyn HostAiPort>,
    runtime: tokio::runtime::Handle,
) -> Arc<LuaSkillsModelCallbackOwner> {
    // The owner lock serializes callback replacement. Installing probes before
    // dispatch callbacks keeps Lua discovery fail-closed throughout replacement.
    // 所有者锁串行化回调替换。先安装探针再安装派发回调，使 Lua 能力发现全程失败关闭。
    let owner = {
        let mut guard = lock_global_owner();
        if let Some(previous) = guard.take() {
            previous.shutdown();
        }
        let owner = LuaSkillsModelCallbackOwner::install(port, runtime);
        owner.install_callbacks();
        *guard = Some(Arc::clone(&owner));
        owner
    };
    owner.sync_availability();
    owner
}

/// Re-aligns the installed callbacks with the live capability snapshot.
/// 使已安装回调与实时能力快照重新一致。
///
/// Configuration changes call this so `has`/`status` track reality; a missing
/// owner is not an error because model support is optional.
/// 配置变更调用它以让 `has`/`status` 跟踪真实状态；缺少所有者不是错误，因为模型支持是可选的。
pub(crate) fn sync_global_availability() -> LuaSkillsModelAvailability {
    // Clone the owner under the process slot and release that lock before the
    // capability snapshot reaches the Host AI port.
    // 在进程槽锁内克隆所有者，并在能力快照进入宿主 AI Port 前释放该锁。
    let owner = {
        let guard = lock_global_owner();
        guard.as_ref().map(Arc::clone)
    };
    match owner {
        Some(owner) => owner.sync_availability(),
        None => LuaSkillsModelAvailability {
            installed: false,
            llm: false,
            embedding: false,
        },
    }
}

/// Clears the process-wide owner and both callbacks.
/// 清除进程级所有者与两个回调。
///
/// Production shutdown and bootstrap rollback call this explicitly; tests use
/// the same entry point to return the process to its pristine state.
/// 生产关闭与引导回滚会显式调用本入口；测试也用同一入口把进程恢复到初始状态。
pub(crate) fn shutdown_global() {
    let owner = {
        let mut guard = lock_global_owner();
        guard.take()
    };
    if let Some(owner) = owner {
        owner.shutdown();
    }
}

/// Returns the installed bridge when one is present.
/// 在已安装时返回桥。
#[cfg(test)]
pub(crate) fn global_bridge() -> Option<Arc<LuaSkillsModelBridge>> {
    let guard = lock_global_owner();
    guard.as_ref().map(|owner| Arc::clone(owner.bridge()))
}

/// Reports which LuaSkills model capabilities are currently usable.
/// 报告当前可用的 LuaSkills 模型能力。
///
/// Settings warnings must agree with the capability state Lua observes, so this
/// is the single source both projections read.
/// 设置警示必须与 Lua 观察到的能力状态一致，因此这是两种投影共同读取的唯一来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LuaSkillsModelAvailability {
    /// Whether this process installed the LuaSkills model callbacks.
    /// 本进程是否安装了 LuaSkills 模型回调。
    pub(crate) installed: bool,
    /// Whether the configured route exposes the one-shot LLM operation.
    /// 已配置路由是否暴露一次性 LLM 操作。
    pub(crate) llm: bool,
    /// Whether the configured route exposes the embedding operation.
    /// 已配置路由是否暴露向量操作。
    pub(crate) embedding: bool,
}

/// Returns whether one snapshot exposes an exact, currently available operation and purpose.
/// 返回某个快照是否暴露精确且当前可用的操作与用途。
fn snapshot_exposes(
    snapshot: &InferenceCapabilitySnapshot,
    operation: AiOperation,
    purpose_id: Option<&str>,
) -> bool {
    snapshot.operations.iter().any(|capability| {
        capability.available
            && capability.operation == operation
            && capability.purpose_id.as_deref() == purpose_id
    })
}

/// Resolves one bridge's current LuaSkills model capability availability.
/// 解析某个桥当前的 LuaSkills 模型能力可用性。
///
/// An unconfigured profile is reported as unavailable rather than as an error:
/// the service must still start, and the settings page owns the repair entry.
/// 未配置 Profile 按"不可用"上报而不是错误：服务必须仍能启动，修复入口属于设置页。
fn bridge_availability(bridge: &LuaSkillsModelBridge) -> LuaSkillsModelAvailability {
    match bridge.capability() {
        Ok(handle) => LuaSkillsModelAvailability {
            installed: true,
            llm: snapshot_exposes(
                &handle.snapshot,
                AiOperation::ConversationRespond,
                Some(LUASKILLS_TEMPORARY_LLM_PURPOSE_ID),
            ),
            embedding: snapshot_exposes(&handle.snapshot, AiOperation::EmbeddingCreate, None),
        },
        Err(_) => LuaSkillsModelAvailability {
            installed: true,
            llm: false,
            embedding: false,
        },
    }
}

/// Resolves the current LuaSkills model capability availability.
/// 解析当前 LuaSkills 模型能力可用性。
pub(crate) fn luaskills_model_availability() -> LuaSkillsModelAvailability {
    sync_global_availability()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_ai_contract::HostAiErrorCode;
    use vulcan_ai_contract::test_support::MockHostAiPort;

    /// Serializes tests that mutate the process-wide callback owner.
    /// 串行化会改动进程级回调所有者的测试。
    ///
    /// The owner is inherently process-wide, so these tests must not overlap.
    /// 所有者本质上是进程级的，因此这些测试不得重叠。
    static OWNER_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Locks the owner test lock, recovering from poisoning.
    /// 锁定所有者测试锁，并在中毒时恢复。
    fn owner_test_guard() -> std::sync::MutexGuard<'static, ()> {
        match OWNER_TEST_LOCK.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Builds one runtime for bridge installation in tests.
    /// 为测试中的桥安装构造一个运行时。
    ///
    /// The runtime is returned by value because dropping it cancels every spawned
    /// bridge task; callers must keep it alive for the whole test.
    /// 运行时按值返回，因为丢弃它会取消所有已投递的桥任务；调用方必须在整个测试期间持有它。
    fn test_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("build test runtime")
    }

    /// Verifies both LuaSkills capabilities are declared optional.
    /// 验证 LuaSkills 两项能力都声明为可选。
    ///
    /// A deployment may configure only the LLM route or only the embedding
    /// route; neither may disable the other.
    /// 部署可能只配置 LLM 路由或只配置向量路由；两者不得相互关闭。
    #[test]
    fn registration_declares_both_capabilities_optional() {
        let registration = LuaSkillsModelBridge::registration();

        assert_eq!(
            registration.consumer_profile_id,
            LUASKILLS_CONSUMER_PROFILE_ID
        );
        assert_eq!(registration.registration_id, LUASKILLS_REGISTRATION_ID);
        assert_eq!(registration.owner_id, LUASKILLS_OWNER_ID);
        assert_eq!(registration.caller.caller_id, LUASKILLS_CALLER_ID);

        // The caller may only ever address its own consumer profile.
        // 调用方只能寻址自己的消费者 Profile。
        assert_eq!(
            registration.caller.allowed_consumer_profile_ids,
            std::collections::BTreeSet::from([String::from(LUASKILLS_CONSUMER_PROFILE_ID)])
        );
        assert_eq!(
            registration.caller.usage_scope,
            InferenceUsageScope::InternalBackground
        );

        assert_eq!(registration.requirements.len(), 2);
        for requirement in &registration.requirements {
            assert!(
                !requirement.required,
                "LuaSkills capabilities must stay independently optional: {requirement:?}"
            );
        }
        assert!(registration.requirements.iter().any(|requirement| {
            requirement.operation == AiOperation::ConversationRespond
                && requirement.purpose_id.as_deref() == Some(LUASKILLS_TEMPORARY_LLM_PURPOSE_ID)
        }));
        assert!(registration.requirements.iter().any(|requirement| {
            requirement.operation == AiOperation::EmbeddingCreate
                && requirement.purpose_id.is_none()
        }));
    }

    /// Verifies the process-wide owner is exclusive, replaceable, and clearable.
    /// 验证进程级所有者是独占的、可替换的且可清理。
    #[test]
    fn process_wide_owner_is_exclusive_and_clearable() {
        let _guard = owner_test_guard();
        shutdown_global();
        assert!(global_bridge().is_none());

        let runtime = test_runtime();
        let port = Arc::new(RecordingLuaSkillsPort::new());
        port.install_capability().expect("fixture capability");
        let erased: Arc<dyn HostAiPort> = Arc::clone(&port) as Arc<dyn HostAiPort>;
        let first = install_global(Arc::clone(&erased), runtime.handle().clone());
        assert!(global_bridge().is_some());

        // Keep the old Arc alive across replacement, then drop it afterwards. Its
        // delayed Drop must clear neither callbacks nor the stable Host AI registration
        // now owned by the successor.
        // 让旧 Arc 跨替换继续存活，随后再释放。它延迟执行的 Drop 既不得清除回调，也不得
        // 注销现在由替代者持有的稳定宿主 AI 注册。
        let second = install_global(Arc::clone(&erased), runtime.handle().clone());
        assert!(!Arc::ptr_eq(&first, &second));
        assert!(global_bridge().is_some());
        drop(first);
        (second.llm_callback)(&llm_request("system", "after replacement"))
            .expect("stale owner drop must preserve successor dispatch");

        // Explicit global shutdown must deactivate the current callback even while
        // this test deliberately retains an external Arc to its owner.
        // 即使测试刻意继续持有当前所有者的外部 Arc，显式全局关闭也必须停用其回调。
        shutdown_global();
        assert!(global_bridge().is_none());
        let error = (second.llm_callback)(&llm_request("system", "after shutdown"))
            .expect_err("shutdown must deactivate retained callback clones");
        assert_eq!(error.code, RuntimeModelErrorCode::ModelUnavailable);
    }

    /// Verifies replacement bridges cannot reuse a logical model-operation identity.
    /// 验证替代桥不能复用逻辑模型操作身份。
    #[test]
    fn operation_ids_remain_distinct_across_bridge_replacement() {
        let runtime = test_runtime();
        let first = LuaSkillsModelBridge::new(
            Arc::new(RecordingLuaSkillsPort::new()),
            runtime.handle().clone(),
        );
        let second = LuaSkillsModelBridge::new(
            Arc::new(RecordingLuaSkillsPort::new()),
            runtime.handle().clone(),
        );

        assert_ne!(
            first.next_operation_id("llm"),
            second.next_operation_id("llm"),
            "a replacement owner must not collide with a late operation from its predecessor"
        );
    }

    /// Verifies host failures are reported without provider detail.
    /// 验证宿主失败上报时不带供应商细节。
    #[test]
    fn host_failure_is_sanitized() {
        let error = HostAiError::new(
            HostAiErrorCode::ProviderUnavailable,
            "provider acme said: api-key sk-secret rejected at https://internal.example",
        );
        let mapped = map_host_error(&error);

        assert_eq!(mapped.code, RuntimeModelErrorCode::ModelUnavailable);
        assert!(mapped.provider_message.is_none());
        assert!(mapped.provider_code.is_none());
        assert!(mapped.provider_status.is_none());
        let rendered = mapped.message.to_lowercase();
        for leaked in ["sk-secret", "internal.example", "api-key", "acme"] {
            assert!(
                !rendered.contains(leaked),
                "host detail `{leaked}` leaked into the Lua-visible error"
            );
        }
    }

    /// Verifies unconfigured and permission failures both read as unavailable.
    /// 验证未配置与权限失败都按"不可用"上报。
    ///
    /// Lua has no retry lever for either condition, and an unconfigured profile is
    /// not an error the caller can act on.
    /// 两种情况 Lua 都没有重试手段，且未配置 Profile 不是调用方可处理的错误。
    #[test]
    fn unusable_route_reads_as_model_unavailable() {
        for code in [
            HostAiErrorCode::CapabilityUnavailable,
            HostAiErrorCode::Configuration,
            HostAiErrorCode::PermissionDenied,
            HostAiErrorCode::Authentication,
        ] {
            let mapped = map_host_error(&HostAiError::new(code, "detail"));
            assert_eq!(
                mapped.code,
                RuntimeModelErrorCode::ModelUnavailable,
                "unexpected mapping for {code:?}"
            );
        }
    }

    /// Verifies every host code maps to the Lua-visible code the caller can act on.
    /// 验证每个宿主代码都映射到调用方可据以行动的 Lua 可见代码。
    ///
    /// These codes cross into Lua, where a skill branches on them, so a wrong mapping is a
    /// wrong instruction rather than a cosmetic detail. Every variant is enumerated because
    /// a variant left to the catch-all is exactly the case no other test can reach.
    /// 这些代码会进入 Lua，技能据此分支，因此错误映射是错误指引而非细节。此处枚举每个变体，
    /// 因为落入兜底分支的变体正是其他测试无法触及的情形。
    #[test]
    fn every_host_code_maps_to_an_actionable_lua_code() {
        let cases: [(HostAiErrorCode, RuntimeModelErrorCode); 18] = [
            (
                HostAiErrorCode::Contract,
                RuntimeModelErrorCode::InvalidArgument,
            ),
            (
                HostAiErrorCode::Authentication,
                RuntimeModelErrorCode::ModelUnavailable,
            ),
            (
                HostAiErrorCode::PermissionDenied,
                RuntimeModelErrorCode::ModelUnavailable,
            ),
            (
                HostAiErrorCode::PaymentRequired,
                RuntimeModelErrorCode::BudgetExceeded,
            ),
            (
                HostAiErrorCode::RateLimited,
                RuntimeModelErrorCode::ProviderError,
            ),
            (
                HostAiErrorCode::ProviderOverloaded,
                RuntimeModelErrorCode::ProviderError,
            ),
            (
                HostAiErrorCode::ProviderUnavailable,
                RuntimeModelErrorCode::ModelUnavailable,
            ),
            (
                HostAiErrorCode::Configuration,
                RuntimeModelErrorCode::ModelUnavailable,
            ),
            (
                HostAiErrorCode::CapabilityUnavailable,
                RuntimeModelErrorCode::ModelUnavailable,
            ),
            (
                HostAiErrorCode::LimitExceeded,
                RuntimeModelErrorCode::BudgetExceeded,
            ),
            (
                HostAiErrorCode::InvalidRequest,
                RuntimeModelErrorCode::InvalidArgument,
            ),
            (
                HostAiErrorCode::DimensionMismatch,
                RuntimeModelErrorCode::InvalidArgument,
            ),
            (
                HostAiErrorCode::FingerprintMismatch,
                RuntimeModelErrorCode::InvalidArgument,
            ),
            (HostAiErrorCode::Timeout, RuntimeModelErrorCode::Timeout),
            // A classified provider failure must not read as an unclassified host failure.
            // 已分类的供应商失败绝不能按未分类的宿主失败上报。
            (
                HostAiErrorCode::Provider,
                RuntimeModelErrorCode::ProviderError,
            ),
            (
                HostAiErrorCode::Cancelled,
                RuntimeModelErrorCode::InternalError,
            ),
            (
                HostAiErrorCode::OperationNotFound,
                RuntimeModelErrorCode::InternalError,
            ),
            (
                HostAiErrorCode::Internal,
                RuntimeModelErrorCode::InternalError,
            ),
        ];
        for (code, expected) in cases {
            assert_eq!(
                map_host_error(&HostAiError::new(code, "detail")).code,
                expected,
                "unexpected mapping for {code:?}"
            );
        }
        // The host message never travels, because it can embed provider details.
        // 宿主消息绝不外传，因为它可能内嵌供应商细节。
        let mapped = map_host_error(&HostAiError::new(
            HostAiErrorCode::Provider,
            "secret-detail",
        ));
        assert!(
            !mapped.message.contains("secret-detail"),
            "host detail leaked: {}",
            mapped.message
        );
    }

    /// Verifies unknown usage stays unknown instead of becoming zero.
    /// 验证未知用量保持未知，而不是变成 0。
    #[test]
    fn unknown_usage_stays_unknown() {
        assert_eq!(usage_tokens(&UsageValue::Unknown), None);
        assert_eq!(runtime_model_usage(&UsageValue::Unknown, None), None);
        assert_eq!(
            runtime_model_usage(&UsageValue::Unknown, Some(&UsageValue::Unknown)),
            None
        );
        assert_eq!(
            usage_tokens(&UsageValue::Known {
                value: 7,
                source: vulcan_ai_contract::UsageValueSource::Reported,
            }),
            Some(7)
        );
    }

    /// Route snapshot used by deterministic fixtures.
    /// 确定性夹具使用的路由快照。
    fn fixture_route() -> vulcan_ai_contract::ExecutionRouteSnapshot {
        vulcan_ai_contract::ExecutionRouteSnapshot {
            provider_id: String::from("fixture"),
            service_entry_id: String::from("fixture"),
            connection_id: String::from("fixture"),
            model_id: String::from("fixture"),
            protocol_binding_id: None,
            action_binding_id: None,
            secondary_route_id: None,
            secondary_provider_name: None,
            secondary_endpoint_name: None,
            capability_revision: String::from("fixture-v1"),
            catalog_revision: String::from("fixture-v1"),
            route_revision: String::from("fixture-v1"),
            usage_scope: InferenceUsageScope::InternalBackground,
        }
    }

    /// Embedding space fingerprint used by deterministic fixtures.
    /// 确定性夹具使用的向量空间指纹。
    fn fixture_fingerprint(dimensions: u32) -> vulcan_ai_contract::EmbeddingSpaceFingerprint {
        vulcan_ai_contract::EmbeddingSpaceFingerprint {
            provider_id: String::from("fixture"),
            service_entry_id: String::from("fixture"),
            deployment_id: None,
            secondary_route_id: None,
            model_id: String::from("fixture"),
            binding_id: String::from("fixture-embedding"),
            capability_revision: String::from("fixture-v1"),
            task: EmbeddingTask::ProviderDefault,
            dimensions,
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            normalization: vulcan_ai_contract::EmbeddingNormalization::Unknown,
            distance_metric: vulcan_ai_contract::DistanceMetric::Unknown,
        }
    }

    /// Builds the capability handle that mirrors every declared LuaSkills requirement.
    /// 构造镜像每项已声明 LuaSkills 要求的能力句柄。
    fn luaskills_capability_handle() -> HostAiCapabilityHandle {
        let registration = LuaSkillsModelBridge::registration();
        let operations = registration
            .requirements
            .iter()
            .map(
                |requirement| vulcan_ai_contract::InferenceOperationCapability {
                    operation: requirement.operation,
                    purpose_id: requirement.purpose_id.clone(),
                    available: true,
                    unavailable_reason: None,
                    route: None,
                    embedding: None,
                    embedding_batch: None,
                    rerank: None,
                },
            )
            .collect();
        let snapshot = InferenceCapabilitySnapshot {
            contract_version: INFERENCE_CONTRACT_VERSION,
            consumer_profile_id: registration.consumer_profile_id.clone(),
            operations,
        };
        HostAiCapabilityHandle::new(
            registration.registration_id,
            registration.owner_id,
            registration.consumer_profile_id,
            registration.caller,
            snapshot,
        )
        .expect("luaskills capability handle")
    }

    /// Records exact host requests while delegating validation and execution to the mock port.
    /// 记录准确宿主请求，同时把校验与执行委托给模拟端口。
    ///
    /// The mock port validates the consumer profile, purpose, permission and call
    /// context, so a request that violates the Host AI contract fails here the way
    /// it would in production.
    /// 模拟端口会校验消费者 Profile、用途、权限与调用上下文，因此违反 Host AI 契约的请求
    /// 会像生产一样在此失败。
    struct RecordingLuaSkillsPort {
        /// Validating host registry and deterministic executor.
        /// 执行校验的宿主注册表与确定性执行器。
        inner: MockHostAiPort,
        /// Ordered LLM requests observed at the LuaSkills bridge boundary.
        /// 在 LuaSkills 桥边界观察到的有序 LLM 请求。
        llm_requests: Mutex<Vec<ResponsesExecutionRequest>>,
        /// Ordered embedding requests observed at the LuaSkills bridge boundary.
        /// 在 LuaSkills 桥边界观察到的有序向量请求。
        embed_requests: Mutex<Vec<HostAiEmbeddingRequest>>,
        /// Exact registrations received from the bridge.
        /// 从桥收到的准确注册。
        registrations: Mutex<Vec<HostAiCapabilityRegistration>>,
        /// Number of registration calls, used to prove handles are not re-registered.
        /// 注册调用次数，用于证明句柄不会被重复注册。
        register_calls: std::sync::atomic::AtomicUsize,
        /// Number of snapshot refreshes, used to prove live refresh happens.
        /// 快照刷新次数，用于证明确实发生实时刷新。
        snapshot_refreshes: std::sync::atomic::AtomicUsize,
        /// Number of explicit cancellations requested by the bridge.
        /// 桥请求的显式取消次数。
        cancel_calls: std::sync::atomic::AtomicUsize,
        /// User-visible text the LLM fixture returns.
        /// LLM 夹具返回的用户可见文本。
        output_text: Mutex<String>,
        /// Termination reason the LLM fixture reports.
        /// LLM 夹具报告的终止原因。
        termination_reason: Mutex<String>,
        /// Non-text output items the LLM fixture reports.
        /// LLM 夹具报告的非文本输出项。
        output_items: Mutex<Vec<vulcan_ai_contract::ResponsesExecutionOutputItem>>,
        /// Optional structured output the LLM fixture reports.
        /// LLM 夹具报告的可选结构化输出。
        structured_output: Mutex<Option<serde_json::Value>>,
        /// When set, the fixture never reaches a terminal result within the test bound.
        /// 置位时，夹具在测试上限内永不到达终态结果。
        stall: AtomicBool,
        /// Records that a stalled response has entered the provider wait.
        /// 记录停滞响应已经进入供应商等待阶段。
        stall_started: AtomicBool,
        /// Releases one deliberately stalled response without relying on wall-clock delay.
        /// 释放一个刻意停滞的响应，避免依赖墙钟延迟。
        stall_release: tokio::sync::Notify,
        /// Enables a deterministic block inside capability snapshot refresh.
        /// 启用能力快照刷新内部的确定性阻塞。
        snapshot_stall: AtomicBool,
        /// Announces entry into a deliberately blocked snapshot refresh.
        /// 宣布已经进入刻意阻塞的快照刷新。
        snapshot_entered: std::sync::Barrier,
        /// Releases a deliberately blocked snapshot refresh.
        /// 释放一个刻意阻塞的快照刷新。
        snapshot_release: std::sync::Barrier,
    }

    impl RecordingLuaSkillsPort {
        /// Creates one recording port with a completed single-text fixture.
        /// 创建一个带"已完成单文本"夹具的记录端口。
        fn new() -> Self {
            Self {
                inner: MockHostAiPort::new(),
                llm_requests: Mutex::new(Vec::new()),
                embed_requests: Mutex::new(Vec::new()),
                registrations: Mutex::new(Vec::new()),
                register_calls: std::sync::atomic::AtomicUsize::new(0),
                snapshot_refreshes: std::sync::atomic::AtomicUsize::new(0),
                cancel_calls: std::sync::atomic::AtomicUsize::new(0),
                output_text: Mutex::new(String::from("fixture answer")),
                termination_reason: Mutex::new(String::from("completed")),
                output_items: Mutex::new(Vec::new()),
                structured_output: Mutex::new(None),
                stall: AtomicBool::new(false),
                stall_started: AtomicBool::new(false),
                stall_release: tokio::sync::Notify::new(),
                snapshot_stall: AtomicBool::new(false),
                snapshot_entered: std::sync::Barrier::new(2),
                snapshot_release: std::sync::Barrier::new(2),
            }
        }

        /// Installs the mirror capability snapshot the bridge registers against.
        /// 安装桥注册所依据的镜像能力快照。
        fn install_capability(&self) -> Result<(), HostAiError> {
            self.inner
                .set_capability_snapshot(luaskills_capability_handle().snapshot)
        }

        /// Returns the ordered captured LLM requests.
        /// 返回已捕获的有序 LLM 请求。
        fn llm_requests(&self) -> Vec<ResponsesExecutionRequest> {
            self.llm_requests.lock().expect("llm capture lock").clone()
        }

        /// Returns the ordered captured embedding requests.
        /// 返回已捕获的有序向量请求。
        fn embed_requests(&self) -> Vec<HostAiEmbeddingRequest> {
            self.embed_requests
                .lock()
                .expect("embed capture lock")
                .clone()
        }
    }

    #[async_trait::async_trait]
    impl HostAiPort for RecordingLuaSkillsPort {
        fn register_capability(
            &self,
            registration: HostAiCapabilityRegistration,
        ) -> Result<HostAiCapabilityHandle, HostAiError> {
            self.register_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.registrations
                .lock()
                .expect("registration capture lock")
                .push(registration.clone());
            self.inner.register_capability(registration)
        }

        fn capability_handle(
            &self,
            registration_id: &str,
        ) -> Result<HostAiCapabilityHandle, HostAiError> {
            self.inner.capability_handle(registration_id)
        }

        fn unregister_capability(&self, registration_id: &str) -> Result<(), HostAiError> {
            self.inner.unregister_capability(registration_id)
        }

        fn capability_snapshot(
            &self,
            handle: &HostAiCapabilityHandle,
        ) -> Result<InferenceCapabilitySnapshot, HostAiError> {
            self.snapshot_refreshes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self
                .snapshot_stall
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                self.snapshot_entered.wait();
                self.snapshot_release.wait();
            }
            self.inner.capability_snapshot(handle)
        }

        async fn create_embeddings(
            &self,
            handle: &HostAiCapabilityHandle,
            context: HostAiCallContext,
            request: HostAiEmbeddingRequest,
        ) -> Result<vulcan_ai_contract::HostAiEmbeddingResponse, HostAiError> {
            self.embed_requests
                .lock()
                .expect("embed capture lock")
                .push(request.clone());
            let inner_request = &request.request;
            let dimension = 4_u32;
            let results = inner_request
                .inputs
                .iter()
                .enumerate()
                .map(|(index, input)| vulcan_ai_contract::EmbeddingResult {
                    input_id: input.input_id.clone(),
                    input_index: u32::try_from(index).expect("fixture index fits u32"),
                    embedding: EmbeddingPayload::Dense {
                        values: vec![0.25_f32; dimension as usize],
                    },
                })
                .collect();
            self.inner
                .set_embedding_response(vulcan_ai_contract::EmbeddingResponse {
                    request_id: inner_request.request_id.clone(),
                    route: fixture_route(),
                    fingerprint: fixture_fingerprint(dimension),
                    results,
                    dropped: Vec::new(),
                    usage: vulcan_ai_contract::RequestUsage::default(),
                    duration_ms: 0,
                })?;
            self.inner.create_embeddings(handle, context, request).await
        }

        async fn rerank_documents(
            &self,
            handle: &HostAiCapabilityHandle,
            context: HostAiCallContext,
            request: vulcan_ai_contract::RerankRequest,
        ) -> Result<vulcan_ai_contract::RerankResponse, HostAiError> {
            self.inner.rerank_documents(handle, context, request).await
        }

        async fn start_response_execution(
            &self,
            handle: &HostAiCapabilityHandle,
            context: HostAiCallContext,
            request: ResponsesExecutionRequest,
        ) -> Result<vulcan_ai_contract::HostAiExecutionHandle, HostAiError> {
            self.llm_requests
                .lock()
                .expect("llm capture lock")
                .push(request.clone());
            self.inner.set_response(
                vulcan_ai_contract::ResponsesExecutionResult {
                    request_id: request.request_id.clone(),
                    route: fixture_route(),
                    output_text: self.output_text.lock().expect("output lock").clone(),
                    reasoning_text: None,
                    include_reasoning_encrypted_content: false,
                    structured_output: self
                        .structured_output
                        .lock()
                        .expect("structured output lock")
                        .clone(),
                    output_items: self.output_items.lock().expect("items lock").clone(),
                    termination_reason: self
                        .termination_reason
                        .lock()
                        .expect("termination lock")
                        .clone(),
                    usage: vulcan_ai_contract::RequestUsage::default(),
                    duration_ms: 0,
                },
                Vec::new(),
            )?;
            self.inner
                .start_response_execution(handle, context, request)
                .await
        }

        fn execution_status(
            &self,
            handle: &vulcan_ai_contract::HostAiExecutionHandle,
        ) -> Result<vulcan_ai_contract::HostAiExecutionSnapshot, HostAiError> {
            self.inner.execution_status(handle)
        }

        fn stream_response(
            &self,
            handle: &vulcan_ai_contract::HostAiExecutionHandle,
        ) -> Result<vulcan_ai_contract::HostAiEventStream, HostAiError> {
            self.inner.stream_response(handle)
        }

        async fn wait_response(
            &self,
            handle: &vulcan_ai_contract::HostAiExecutionHandle,
        ) -> Result<vulcan_ai_contract::ResponsesExecutionResult, HostAiError> {
            if self.stall.load(std::sync::atomic::Ordering::SeqCst) {
                self.stall_started
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                // A notification makes lifecycle ordering deterministic while
                // remaining cancellable when the runtime is dropped.
                // 通知机制使生命周期顺序可确定，同时仍可在运行时丢弃时取消。
                self.stall_release.notified().await;
            }
            self.inner.wait_response(handle).await
        }

        fn cancel_execution(
            &self,
            handle: &vulcan_ai_contract::HostAiExecutionHandle,
        ) -> Result<(), HostAiError> {
            self.cancel_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.cancel_execution(handle)
        }
    }

    /// Builds a bridge over one recording port with an explicit bound.
    /// 以一个记录端口和显式上限构造桥。
    ///
    /// Returns the owning runtime alongside the bridge so the test keeps the
    /// spawned bridge tasks alive.
    /// 连同桥一起返回其所属运行时，使测试保持已投递的桥任务存活。
    fn bridge_over(
        port: Arc<RecordingLuaSkillsPort>,
        bound: Duration,
    ) -> (Arc<LuaSkillsModelBridge>, tokio::runtime::Runtime) {
        let runtime = test_runtime();
        let erased: Arc<dyn HostAiPort> = port;
        let bridge =
            LuaSkillsModelBridge::new_with_timeout(erased, runtime.handle().clone(), bound);
        (bridge, runtime)
    }

    /// Builds the caller context LuaSkills attaches automatically.
    /// 构造 LuaSkills 自动附加的调用方上下文。
    ///
    /// It is descriptive identity only; the bridge must never treat it as
    /// authorization.
    /// 它只是描述性身份；桥绝不把它当作授权。
    fn fixture_caller() -> luaskills::RuntimeModelCaller {
        luaskills::RuntimeModelCaller {
            skill_id: Some(String::from("fixture-skill")),
            entry_name: Some(String::from("main")),
            canonical_tool_name: None,
            root_name: Some(String::from("user")),
            skill_dir: Some(String::from("D:/fixture/skill")),
            client_name: Some(String::from("fixture-client")),
            request_id: Some(String::from("fixture-request")),
        }
    }

    /// Builds one plain LLM request as LuaSkills would produce it.
    /// 构造一个 LuaSkills 会产生的普通 LLM 请求。
    fn llm_request(system: &str, user: &str) -> RuntimeModelLlmRequest {
        RuntimeModelLlmRequest {
            system: String::from(system),
            user: String::from(user),
            caller: fixture_caller(),
        }
    }

    /// Verifies the encoded LLM request is a one-shot question with no tool surface.
    /// 验证编码后的 LLM 请求是一次性问答且没有工具面。
    #[test]
    fn llm_request_is_one_shot_without_tools_or_history() {
        let port = Arc::new(RecordingLuaSkillsPort::new());
        port.install_capability().expect("fixture capability");
        let (bridge, _runtime) = bridge_over(Arc::clone(&port), Duration::from_secs(5));

        let response = bridge
            .dispatch_llm(&llm_request("be terse", "what is 2+2?"))
            .expect("fixture llm call");
        assert_eq!(response.assistant, "fixture answer");
        assert_eq!(response.usage, None);

        let captured = port.llm_requests();
        assert_eq!(captured.len(), 1);
        let request = &captured[0];

        assert_eq!(request.consumer_profile_id, LUASKILLS_CONSUMER_PROFILE_ID);
        assert_eq!(request.purpose_id, LUASKILLS_TEMPORARY_LLM_PURPOSE_ID);
        assert!(request.request_id.starts_with("luaskills-llm-"));

        // Exactly the caller's two turns, in order, with no injected history.
        // 恰好是调用方的两轮，顺序一致，且没有注入历史。
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, MessageRole::System);
        assert_eq!(request.messages[1].role, MessageRole::User);
        for message in &request.messages {
            assert!(
                message.provider_context_items.is_empty(),
                "no provider context may be injected into a one-shot call"
            );
        }

        // No tool surface of any kind, and no extra tool-control protocol fields.
        // 没有任何工具面，也没有额外的工具控制协议字段。
        assert!(request.tools.is_empty());
        assert!(request.native_tools.is_empty());
        assert_eq!(request.tool_choice, ToolChoicePolicy::Disabled);
        assert_eq!(
            request.parallel_tool_calls,
            ParallelToolCallPolicy::Disabled
        );

        // Reasoning continuation must never be requested for Lua.
        // 绝不为 Lua 请求推理续跑内容。
        assert!(!request.include_reasoning_encrypted_content);
        assert_eq!(request.reasoning_output, ReasoningOutputPolicy::Hidden);
        assert_eq!(request.reasoning_effort, None);
        assert!(request.stop_sequences.is_empty());
        assert!(request.model_parameters.is_empty());
    }

    /// Verifies an unexpected tool call is a contract error, not a silent answer.
    /// 验证意外工具调用属于契约错误，而不是被静默当作回答。
    #[test]
    fn tool_call_output_is_rejected() {
        let port = Arc::new(RecordingLuaSkillsPort::new());
        port.install_capability().expect("fixture capability");
        port.output_items.lock().expect("items lock").push(
            vulcan_ai_contract::ResponsesExecutionOutputItem {
                item_id: String::from("item-0"),
                content: ContentItem::ToolCall {
                    call: vulcan_ai_contract::ToolCall {
                        id: String::from("call-0"),
                        name: String::from("vulcan-file-read"),
                        arguments: serde_json::json!({}),
                        signature: None,
                    },
                },
            },
        );
        let (bridge, _runtime) = bridge_over(Arc::clone(&port), Duration::from_secs(5));

        let error = bridge
            .dispatch_llm(&llm_request("system", "user"))
            .expect_err("a tool call must fail the one-shot contract");
        assert!(error.message.contains("tool call"), "{}", error.message);
        // The partial text must not leak as if it were a valid answer.
        // 部分文本不得作为有效回答泄露。
        assert!(!error.message.contains("fixture answer"));
    }

    /// Verifies refusal output is not silently converted into an empty assistant response.
    /// 验证拒绝输出不会被静默转换成空的 assistant 响应。
    #[test]
    fn refusal_output_is_rejected() {
        let port = Arc::new(RecordingLuaSkillsPort::new());
        port.install_capability().expect("fixture capability");
        port.output_items.lock().expect("items lock").push(
            vulcan_ai_contract::ResponsesExecutionOutputItem {
                item_id: String::from("item-refusal"),
                content: ContentItem::Refusal {
                    text: String::from("fixture refusal detail"),
                },
            },
        );
        let (bridge, _runtime) = bridge_over(Arc::clone(&port), Duration::from_secs(5));

        let error = bridge
            .dispatch_llm(&llm_request("system", "user"))
            .expect_err("a refusal cannot satisfy the one-shot assistant contract");
        assert!(error.message.contains("refusal"), "{}", error.message);
        // Neither the partial answer nor provider refusal detail may cross into Lua.
        // 部分回答和供应商拒绝详情都不得进入 Lua。
        assert!(!error.message.contains("fixture answer"));
        assert!(!error.message.contains("fixture refusal detail"));
    }

    /// Verifies structured output cannot be mistaken for the one assistant text response.
    /// 验证结构化输出不会被误当成唯一的 assistant 文本响应。
    #[test]
    fn structured_output_is_rejected() {
        let port = Arc::new(RecordingLuaSkillsPort::new());
        port.install_capability().expect("fixture capability");
        *port
            .structured_output
            .lock()
            .expect("structured output lock") = Some(serde_json::json!({"answer": 4}));
        let (bridge, _runtime) = bridge_over(Arc::clone(&port), Duration::from_secs(5));

        let error = bridge
            .dispatch_llm(&llm_request("system", "user"))
            .expect_err("structured output cannot satisfy the one-shot text contract");
        assert!(
            error.message.contains("structured output"),
            "{}",
            error.message
        );
        assert!(!error.message.contains("fixture answer"));
    }

    /// Verifies hidden reasoning metadata does not invalidate an otherwise complete text answer.
    /// 验证隐藏推理元数据不会使原本完整的文本回答失效。
    #[test]
    fn hidden_reasoning_output_is_accepted_without_exposure() {
        let port = Arc::new(RecordingLuaSkillsPort::new());
        port.install_capability().expect("fixture capability");
        port.output_items.lock().expect("items lock").push(
            vulcan_ai_contract::ResponsesExecutionOutputItem {
                item_id: String::from("item-reasoning"),
                content: ContentItem::Reasoning {
                    text: String::from("private fixture reasoning"),
                    signature: Some(String::from("private fixture signature")),
                },
            },
        );
        let (bridge, _runtime) = bridge_over(Arc::clone(&port), Duration::from_secs(5));

        let response = bridge
            .dispatch_llm(&llm_request("system", "user"))
            .expect("hidden reasoning must not break a complete text response");
        assert_eq!(response.assistant, "fixture answer");
        assert!(!response.assistant.contains("private fixture"));
    }

    /// Verifies a non-completed termination is rejected rather than returned.
    /// 验证非 completed 终态被拒绝，而不是被返回。
    #[test]
    fn non_completed_termination_is_rejected() {
        let port = Arc::new(RecordingLuaSkillsPort::new());
        port.install_capability().expect("fixture capability");
        *port.termination_reason.lock().expect("termination lock") = String::from("max_tokens");
        let (bridge, _runtime) = bridge_over(Arc::clone(&port), Duration::from_secs(5));

        let error = bridge
            .dispatch_llm(&llm_request("system", "user"))
            .expect_err("a truncated answer must not be returned as final");
        assert!(error.message.contains("max_tokens"), "{}", error.message);
    }

    /// Verifies the capability is registered once and only refreshed afterwards.
    /// 验证能力只注册一次，之后只刷新快照。
    #[test]
    fn capability_is_registered_once_then_refreshed() {
        let port = Arc::new(RecordingLuaSkillsPort::new());
        port.install_capability().expect("fixture capability");
        let (bridge, _runtime) = bridge_over(Arc::clone(&port), Duration::from_secs(5));

        for _ in 0..3 {
            bridge
                .dispatch_llm(&llm_request("system", "user"))
                .expect("fixture llm call");
        }

        assert_eq!(
            port.register_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1,
            "re-registering per dispatch would churn the port registry"
        );
        assert_eq!(
            port.snapshot_refreshes
                .load(std::sync::atomic::Ordering::SeqCst),
            2,
            "every call after the first must refresh the live snapshot"
        );
        let registrations = port.registrations.lock().expect("registration lock");
        assert_eq!(registrations.len(), 1);
        assert_eq!(registrations[0].caller.caller_id, LUASKILLS_CALLER_ID);
        drop(registrations);

        // Operation identities stay stable and distinct across calls.
        // 操作标识在多次调用间保持稳定且互不相同。
        let captured = port.llm_requests();
        let ids = captured
            .iter()
            .map(|request| request.request_id.as_str())
            .collect::<Vec<_>>();
        assert!(ids.iter().all(|id| id.starts_with("luaskills-llm-")));
        assert_eq!(
            ids.iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            3
        );
    }

    /// Verifies an unconfigured profile degrades instead of failing.
    /// 验证未配置 Profile 时降级而不是失败。
    ///
    /// Registration must fail closed into a structured `model_unavailable`, and the
    /// Lua-visible message must not expose host internals.
    /// 注册必须"失败关闭"为结构化的 `model_unavailable`，且 Lua 可见消息不得暴露宿主内部。
    #[test]
    fn unconfigured_profile_reports_model_unavailable() {
        // No capability snapshot installed: the port cannot resolve the profile.
        let port = Arc::new(RecordingLuaSkillsPort::new());
        let (bridge, _runtime) = bridge_over(Arc::clone(&port), Duration::from_secs(5));

        let error = bridge
            .dispatch_llm(&llm_request("system", "user"))
            .expect_err("an unconfigured profile must not execute");
        assert_eq!(error.code, RuntimeModelErrorCode::ModelUnavailable);
        assert!(error.provider_message.is_none());
        // A failed registration must not be cached as if it had succeeded.
        // 失败的注册不得被当作成功缓存下来。
        assert!(port.llm_requests().is_empty());
    }

    /// Verifies the embedding request carries exactly one dense text input.
    /// 验证向量请求恰好携带一个稠密文本输入。
    #[test]
    fn embedding_request_is_single_dense_text_input() {
        let port = Arc::new(RecordingLuaSkillsPort::new());
        port.install_capability().expect("fixture capability");
        let (bridge, _runtime) = bridge_over(Arc::clone(&port), Duration::from_secs(5));

        let response = bridge
            .dispatch_embed(&RuntimeModelEmbedRequest {
                text: String::from("embed me"),
                caller: fixture_caller(),
            })
            .expect("fixture embedding call");

        assert_eq!(response.vector, vec![0.25_f32; 4]);
        assert_eq!(response.dimensions, 4);
        assert_eq!(response.usage, None);

        let captured = port.embed_requests();
        assert_eq!(captured.len(), 1);
        let request = &captured[0].request;
        assert_eq!(request.consumer_profile_id, LUASKILLS_CONSUMER_PROFILE_ID);
        assert!(request.request_id.starts_with("luaskills-embed-"));
        assert_eq!(request.inputs.len(), 1);
        assert_eq!(request.task, LUASKILLS_EMBEDDING_TASK);
        assert_eq!(request.output_kind, EmbeddingOutputKind::Dense);
        // Lua cannot choose a dimension, so the exact route decides it.
        // Lua 无法选择维度，因此由精确路由决定。
        assert_eq!(request.dimensions, None);
    }

    /// Verifies a stalled provider exits the callback within the bound.
    /// 验证停滞的供应商会在上限内退出回调。
    ///
    /// The bound is what keeps a slow model call from pinning an engine read guard,
    /// a per-call OS thread and a main-pool VM lease indefinitely.
    /// 该上限阻止缓慢模型调用无限期占住引擎读守卫、按调用创建的 OS 线程和主池 VM 租约。
    #[test]
    fn stalled_provider_exits_within_the_bound() {
        let port = Arc::new(RecordingLuaSkillsPort::new());
        port.install_capability().expect("fixture capability");
        port.stall.store(true, std::sync::atomic::Ordering::SeqCst);
        let bound = Duration::from_millis(150);
        let (bridge, _runtime) = bridge_over(Arc::clone(&port), bound);

        let started = std::time::Instant::now();
        let error = bridge
            .dispatch_llm(&llm_request("system", "user"))
            .expect_err("a stalled call must not block forever");
        let elapsed = started.elapsed();

        assert_eq!(error.code, RuntimeModelErrorCode::Timeout);
        assert!(
            elapsed >= bound,
            "the bound must actually be waited out before giving up: {elapsed:?}"
        );
        assert!(
            elapsed < bound + Duration::from_secs(10),
            "the callback must exit shortly after the bound: {elapsed:?}"
        );
        // A timeout is an unknown outcome, so the text must not claim failure or success.
        // 超时属于未知结果，因此文案不得声称失败或成功。
        assert!(
            error.message.contains("unknown"),
            "a timeout must be reported as an unknown outcome: {}",
            error.message
        );
    }

    /// Verifies abnormal task loss still closes the cooperative cancellation edge.
    /// 验证异步任务异常丢失时仍会闭合协作取消边界。
    #[test]
    fn disconnected_result_channel_signals_cancellation() {
        let port = Arc::new(RecordingLuaSkillsPort::new());
        let (bridge, _runtime) = bridge_over(Arc::clone(&port), Duration::from_secs(5));
        let cancellation = Arc::new(FlagCancellation::default());

        let error = bridge
            .call_bounded(&cancellation, async move {
                panic!("simulated bridge task loss");
                #[allow(unreachable_code)]
                Ok::<(), HostAiError>(())
            })
            .expect_err("task loss must close the result channel");

        assert_eq!(error.code, RuntimeModelErrorCode::InternalError);
        assert!(
            cancellation.is_cancelled(),
            "a disconnected result channel must not leave child work uncancelled"
        );
    }

    /// Verifies shutdown drains admitted callbacks and rejects every later call.
    /// 验证关闭会排空已经入场的回调，并拒绝之后的所有调用。
    ///
    /// The lifecycle writer must wait until a callback that already crossed the
    /// admission gate returns. Once the writer completes, a retained callback
    /// value must fail before it can register or dispatch another capability.
    /// 生命周期写锁必须等待已经越过入场门的回调返回。写锁完成后，即使外部仍保留旧回调值，
    /// 该回调也必须在重新注册或派发能力之前失败。
    #[test]
    fn shutdown_waits_for_admitted_callback_and_blocks_late_dispatch() {
        let _guard = owner_test_guard();
        shutdown_global();

        let runtime = test_runtime();
        let port = Arc::new(RecordingLuaSkillsPort::new());
        port.install_capability().expect("fixture capability");
        port.stall.store(true, std::sync::atomic::Ordering::SeqCst);
        let erased: Arc<dyn HostAiPort> = Arc::clone(&port) as Arc<dyn HostAiPort>;
        let owner = install_global(erased, runtime.handle().clone());
        let retained_callback = Arc::clone(&owner.llm_callback);

        let (call_sender, call_receiver) = std::sync::mpsc::sync_channel(1);
        let active_callback = Arc::clone(&retained_callback);
        let call_thread = std::thread::spawn(move || {
            let outcome = active_callback(&llm_request("system", "user"));
            call_sender.send(outcome).expect("report callback outcome");
        });
        let admission_deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !port.stall_started.load(std::sync::atomic::Ordering::SeqCst) {
            assert!(
                std::time::Instant::now() < admission_deadline,
                "the callback must enter its provider wait"
            );
            std::thread::yield_now();
        }

        let (shutdown_started_sender, shutdown_started_receiver) = std::sync::mpsc::sync_channel(1);
        let (shutdown_done_sender, shutdown_done_receiver) = std::sync::mpsc::sync_channel(1);
        let shutdown_thread = std::thread::spawn(move || {
            shutdown_started_sender
                .send(())
                .expect("report shutdown admission");
            shutdown_global();
            shutdown_done_sender
                .send(())
                .expect("report shutdown completion");
        });
        shutdown_started_receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("shutdown thread must start");
        let shutdown_completed_early =
            match shutdown_done_receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(()) => true,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => false,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("shutdown completion channel disconnected")
                }
            };

        // Always release the callback before asserting so a failing assertion
        // cannot strand either worker thread behind the lifecycle lock.
        // 始终先释放回调再断言，避免失败断言把任一工作线程困在生命周期锁后。
        port.stall_release.notify_one();
        let call_outcome = call_receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("admitted callback must finish after release");
        if !shutdown_completed_early {
            shutdown_done_receiver
                .recv_timeout(Duration::from_secs(5))
                .expect("shutdown must finish after the admitted callback");
        }
        call_thread.join().expect("join callback thread");
        shutdown_thread.join().expect("join shutdown thread");

        assert!(
            !shutdown_completed_early,
            "shutdown must not pass an admitted callback"
        );
        assert!(call_outcome.is_ok(), "the admitted callback must finish");
        let register_calls = port
            .register_calls
            .load(std::sync::atomic::Ordering::SeqCst);
        let late_error = retained_callback(&llm_request("system", "late"))
            .expect_err("a retained callback must be inactive after shutdown");
        assert_eq!(late_error.code, RuntimeModelErrorCode::ModelUnavailable);
        assert_eq!(
            port.register_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            register_calls,
            "an inactive callback must not re-register the capability"
        );
    }

    /// Verifies shutdown also drains a direct availability refresh admitted before it.
    /// 验证关闭也会排空在它之前入场的直接可用性刷新。
    #[test]
    fn shutdown_orders_direct_availability_refresh_before_unregister() {
        let _guard = owner_test_guard();
        shutdown_global();

        let runtime = test_runtime();
        let port = Arc::new(RecordingLuaSkillsPort::new());
        port.install_capability().expect("fixture capability");
        let erased: Arc<dyn HostAiPort> = Arc::clone(&port) as Arc<dyn HostAiPort>;
        let owner = install_global(erased, runtime.handle().clone());
        port.snapshot_stall
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let refresh_owner = Arc::clone(&owner);
        let refresh_thread = std::thread::spawn(move || refresh_owner.sync_availability());
        port.snapshot_entered.wait();

        let (shutdown_done_sender, shutdown_done_receiver) = std::sync::mpsc::sync_channel(1);
        let shutdown_thread = std::thread::spawn(move || {
            shutdown_global();
            shutdown_done_sender
                .send(())
                .expect("report shutdown completion");
        });
        let shutdown_completed_early =
            match shutdown_done_receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(()) => true,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => false,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("shutdown completion channel disconnected")
                }
            };

        // Always release the snapshot call before asserting so a failing assertion
        // cannot strand the refresh or shutdown worker.
        // 始终先释放快照调用再断言，避免失败断言困住刷新或关闭工作线程。
        port.snapshot_release.wait();
        let admitted = refresh_thread.join().expect("join availability refresh");
        if !shutdown_completed_early {
            shutdown_done_receiver
                .recv_timeout(Duration::from_secs(5))
                .expect("shutdown must finish after the admitted refresh");
        }
        shutdown_thread.join().expect("join shutdown thread");

        assert!(admitted.llm && admitted.embedding);
        assert!(
            !shutdown_completed_early,
            "shutdown must not unregister during an admitted availability refresh"
        );
        let register_calls = port
            .register_calls
            .load(std::sync::atomic::Ordering::SeqCst);
        let inactive = owner.sync_availability();
        assert!(!inactive.installed);
        assert_eq!(
            port.register_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            register_calls,
            "an inactive direct refresh must not re-register the capability"
        );
    }

    /// Verifies availability tracks the live snapshot and survives a missing owner.
    /// 验证可用性跟随实时快照，且在缺少所有者时不报错。
    ///
    /// This is the host-side equivalent of an upstream capability query: `has`
    /// follows the slots, and the slots follow this result.
    /// 这是上游能力查询的宿主侧等价物：`has` 跟随槽位，槽位跟随本结果。
    #[test]
    fn availability_tracks_the_live_snapshot() {
        let _guard = owner_test_guard();
        shutdown_global();

        // No owner yet: model support is optional, so this is a plain answer.
        // 尚无所有者：模型支持是可选的，因此这是一个普通答案。
        let absent = sync_global_availability();
        assert!(!absent.installed);
        assert!(!absent.llm);
        assert!(!absent.embedding);

        let runtime = test_runtime();
        // Unconfigured profile: installed but nothing usable.
        // 未配置 Profile：已安装但无可用能力。
        let port = Arc::new(RecordingLuaSkillsPort::new());
        let erased: Arc<dyn HostAiPort> = Arc::clone(&port) as Arc<dyn HostAiPort>;
        let _owner = install_global(erased, runtime.handle().clone());

        let unavailable = sync_global_availability();
        assert!(unavailable.installed);
        assert!(!unavailable.llm);
        assert!(!unavailable.embedding);

        // A configured mirror snapshot makes both capabilities usable.
        // 配置镜像快照后两项能力都可用。
        port.install_capability().expect("fixture capability");
        let available = sync_global_availability();
        assert!(available.installed);
        assert!(available.llm);
        assert!(available.embedding);

        // The settings projection reads the same single source.
        // 设置投影读取同一唯一来源。
        assert_eq!(luaskills_model_availability(), available);

        shutdown_global();
        assert!(!sync_global_availability().installed);
    }
}
