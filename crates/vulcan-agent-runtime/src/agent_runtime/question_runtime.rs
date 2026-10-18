//! Host-managed question runtime and model-visible ask tool surface.
//! 宿主管理的问题运行时与模型可见 ask 工具表面。

use crate::model::SessionId;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::oneshot;
use uuid::Uuid;
use vulcan_core::types::{
    ToolInvocationRequest as CoreToolInvocationRequest,
    ToolInvocationResult as CoreToolInvocationResult,
};
use vulcan_session_core::v1::ids::{ApiCallId, RequestStreamId, ToolCallId};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
    VulcanToolInvocationContext, VulcanToolInvocationResult, VulcanToolOrigin,
    VulcanToolSourceKind,
};

use crate::agent_runtime::tool_runtime_bridge::{
    build_tool_failure_invocation_result, canonicalize_tool_invocation_result,
};

mod approval;
pub use approval::{APPROVAL_ALLOW_LABEL, APPROVAL_DENY_LABEL, ApprovalAnswer, ToolApprovalPrompt};

/// Fallible callback invoked whenever pending-question visibility changes.
/// 待回答问题可见性变化时触发的可失败回调。
pub type PendingQuestionChangeCallback =
    Arc<dyn Fn() -> Result<(), String> + Send + Sync + 'static>;

/// Fallible host callback that makes each Ask lifecycle boundary durable before it becomes observable.
/// 在每个 Ask 生命周期边界可观察之前使其可靠落盘的可失败宿主回调。
pub type PendingQuestionDurabilityCallback =
    Arc<dyn Fn(PendingQuestionDurabilityAction) -> Result<(), String> + Send + Sync + 'static>;

/// One exact Ask lifecycle action submitted to the V1 session owner.
/// 提交给 V1 会话所有者的一条精确 Ask 生命周期动作。
#[derive(Debug, Clone)]
pub enum PendingQuestionDurabilityAction {
    /// The exact local tool must become durably WaitingUser before WebUI publication.
    /// 精确本地工具必须在 WebUI 发布前可靠进入 WaitingUser。
    Waiting {
        /// Canonical pending request and exact local invocation identity.
        /// 规范待回答请求与精确本地调用身份。
        request: QuestionRequest,
    },
    /// The exact successful or failed answer result must become durable before the future resumes.
    /// 精确回答成功或失败结果必须在 future 恢复前可靠落盘。
    Answered {
        /// Canonical pending request and exact local invocation identity.
        /// 规范待回答请求与精确本地调用身份。
        request: QuestionRequest,
        /// Canonical controller result that the running tool will return unchanged.
        /// 运行中工具随后将原样返回的规范控制器结果。
        result: CoreToolInvocationResult,
    },
    /// An approved or denied permission request returns its tool to Running before the future
    /// resumes; the tool body, or the denial, completes it afterwards.
    /// 已批准或已拒绝的权限请求在 future 恢复前把工具恢复为 Running；之后由工具主体或拒绝结果完成它。
    Resumed {
        /// Canonical pending approval request and exact local invocation identity.
        /// 规范待审批请求与精确本地调用身份。
        request: QuestionRequest,
    },
    /// The exact cancellation result must become durable before the future resumes with rejection.
    /// 精确取消结果必须在 future 以拒绝恢复前可靠落盘。
    Cancelled {
        /// Canonical pending request and exact local invocation identity.
        /// 规范待回答请求与精确本地调用身份。
        request: QuestionRequest,
        /// Canonical controller failure result emitted by the rejected tool invocation.
        /// 被拒绝工具调用随后产生的规范控制器失败结果。
        result: CoreToolInvocationResult,
    },
}

/// Stable host-result kind emitted by the ask tool so transcript and future WebUI enrichments can identify structured question replies.
/// ask 工具输出的稳定宿主结果类型，供转录与后续WebUI增强识别结构化问答回复。
const QUESTION_TOOL_HOST_RESULT_KIND: &str = "ask";

/// Stable rejection text shared by durable cancellation and the resumed tool future.
/// 由可靠取消与恢复后的工具 future 共享的稳定拒绝文本。
const QUESTION_REJECTED_ERROR: &str = "The user abandoned the current questionnaire request. The current conversation run has been interrupted.";

/// Structured question-tool arguments accepted from the model-visible `ask` tool surface.
/// 模型可见 `ask` 工具表面接受的结构化问题工具参数。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct QuestionToolInvocationArgs {
    /// Ordered follow-up questions that should be presented to the user.
    /// 应按顺序展示给用户的跟进问题集合。
    questions: Vec<QuestionPrompt>,
}

/// One selectable option shown inside one structured question prompt.
/// 单条结构化问题提示中展示的一项可选答案。
#[derive(Debug, Clone, Deserialize)]
pub struct QuestionOption {
    /// Short user-facing label returned back to the model when this option is selected.
    /// 该选项被选中时回传给模型的简短用户可见标签。
    pub label: String,
    /// Optional explanatory helper text rendered under the option label in the WebUI.
    /// 在WebUI中显示于选项标签下方的可选说明文字。
    pub description: String,
}

/// One structured prompt presented to the user while a model tool call is waiting for follow-up input.
/// 模型工具调用等待跟进输入期间呈现给用户的一条结构化提示。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct QuestionPrompt {
    /// Full natural-language question shown to the user.
    /// 展示给用户的完整自然语言问题。
    pub question: String,
    /// Very short header rendered as the field label inside the modal.
    /// 作为弹层字段标题渲染的极短表头文本。
    pub header: String,
    /// Available selectable choices rendered for this question.
    /// 针对该问题渲染的可选选项集合。
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    /// Whether multiple option labels may be selected simultaneously.
    /// 是否允许同时选中多个选项标签。
    pub multiple: Option<bool>,
}

impl QuestionPrompt {
    /// Returns whether this prompt should allow selecting more than one option at a time.
    /// 返回该提示当前是否允许一次选择多个选项。
    pub fn allows_multiple(&self) -> bool {
        self.multiple.unwrap_or(false)
    }
}

/// Canonical pending question request stored by the host runtime until the user replies or rejects it.
/// 宿主运行时在用户回复或拒绝前持久保存的规范待回答问题请求。
#[derive(Debug, Clone)]
pub struct QuestionRequest {
    /// Stable request identifier used by WebUI actions to route replies back into the runtime.
    /// WebUI动作用于把回复路由回运行时的稳定请求标识。
    pub id: String,
    /// Controller session that owns the current blocking tool call.
    /// 拥有当前阻塞式工具调用的控制器会话标识。
    pub session_id: SessionId,
    /// Exact Provider stream item that owns this durable local tool invocation.
    /// 拥有该可靠本地工具调用的精确 Provider 流项目。
    pub request_stream_id: RequestStreamId,
    /// Exact local physical API identity that owns the waiting Ask.
    /// 拥有该等待中 Ask 的精确本地物理 API 身份。
    pub api_call_id: ApiCallId,
    /// Exact local tool-call identity of the waiting Ask.
    /// 该等待中 Ask 的精确本地工具调用身份。
    pub tool_call_id: ToolCallId,
    /// Exact controller-owned tool-call identity used to reconstruct the canonical result.
    /// 用于重建规范结果的精确控制器所有工具调用身份。
    pub controller_tool_call_id: String,
    /// Optional Provider-native identity retained only for diagnostics and correlation.
    /// 仅用于诊断与关联而保留的可选 Provider 原生身份。
    pub provider_tool_call_id: Option<String>,
    /// Exact public tool name used by the controller request.
    /// 控制器请求使用的精确公开工具名。
    pub tool_name: String,
    /// Exact structured arguments retained for canonical result reconstruction.
    /// 为规范结果重建保留的精确结构化参数。
    pub tool_arguments: Value,
    /// Monotonic sequence used to present older pending questions before newer ones.
    /// 用于让较早的问题请求先于较新的请求展示的单调序号。
    pub sequence: u64,
    /// Ordered structured prompts that still need user answers.
    /// 仍然需要用户作答的有序结构化提示集合。
    pub questions: Vec<QuestionPrompt>,
    /// Permission facts when this request approves a gated tool call instead of asking questions.
    /// 当该请求审批受闸门管控的工具调用而非提问时，携带的权限事实。
    pub approval: Option<ToolApprovalPrompt>,
}

/// Canonical reply payload returned to the model once the user has answered every pending question.
/// 用户完成所有待回答问题后返回给模型的规范回复载荷。
#[derive(Debug, Clone)]
pub struct QuestionReply {
    /// Ordered answers aligned with the original question array, where each inner array contains one or more selected labels.
    /// 与原始问题数组一一对齐的有序答案集合，其中每个内部数组包含一个或多个选中的标签。
    pub answers: Vec<Vec<String>>,
}

/// Terminal completion sent over the pending request channel once the user has replied or rejected the modal.
/// 用户回复或拒绝弹层后，通过待处理请求通道发送的终态完成消息。
#[derive(Debug, Clone)]
enum PendingQuestionCompletion {
    /// Concrete structured answers supplied by the user.
    /// 用户提供的具体结构化答案。
    Reply(QuestionReply),
    /// Explicit rejection emitted when the user closes the dialog without answering.
    /// 用户未作答直接关闭对话框时发出的显式拒绝信号。
    Reject,
}

/// One pending question entry retained inside the runtime registry while the tool invocation is suspended.
/// 在工具调用挂起期间保存在运行时注册表中的单条待回答问题记录。
struct PendingQuestionEntry {
    /// Canonical request metadata mirrored into the WebUI poll surface.
    /// 镜像到WebUI轮询表面的规范请求元数据。
    request: QuestionRequest,
    /// One-shot sender used to resume the suspended tool invocation.
    /// 用于恢复挂起工具调用的一次性发送端。
    sender: oneshot::Sender<PendingQuestionCompletion>,
}

/// Drop guard that removes one pending question request if the waiting tool future is cancelled before the user responds.
/// 若等待中的工具 Future 在用户响应前被取消，则负责移除对应待回答问题请求的 Drop 守卫。
struct PendingQuestionGuard {
    /// Shared runtime registry that owns the pending-question map.
    /// 持有待回答问题映射表的共享运行时注册表。
    runtime: Arc<QuestionRuntime>,
    /// Stable identifier of the pending request that should be cleaned up on drop.
    /// 在 Drop 时需要清理的待回答请求稳定标识。
    request_id: String,
    /// Stable host callback retained only after the registration notification succeeds.
    /// 仅在注册通知成功后保留的稳定宿主回调。
    change_callback: Option<PendingQuestionChangeCallback>,
}

impl Drop for PendingQuestionGuard {
    /// Cleans up the pending request eagerly when the suspended tool call goes away unexpectedly.
    /// 当挂起中的工具调用异常结束时，主动清理对应的待回答请求。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; cleanup failures are reported through an explicit runtime diagnostic.
    /// 不返回值；清理失败会通过明确的运行时诊断上报。
    fn drop(&mut self) {
        if let Err(error) = self
            .runtime
            .remove_pending_request(&self.request_id, self.change_callback.as_ref())
        {
            eprintln!(
                "Question runtime failed to clean up pending request `{}` during cancellation: {error}",
                self.request_id
            );
        }
    }
}

/// Host-managed registry that mirrors pending structured questions into the WebUI and resumes tool calls once the user answers them.
/// 把待回答结构化问题镜像到WebUI并在用户作答后恢复工具调用的宿主管理注册表。
pub struct QuestionRuntime {
    /// Pending structured question requests keyed by their stable request identifiers.
    /// 按稳定请求标识索引的待回答结构化问题请求集合。
    pending: RwLock<HashMap<String, PendingQuestionEntry>>,
    /// Optional host callback used to notify WebUI projections when pending requests change.
    /// 待回答请求变化时用于通知WebUI投影的可选宿主回调。
    change_callback: RwLock<Option<PendingQuestionChangeCallback>>,
    /// Optional host callback that commits Ask lifecycle changes into the V1 ledger.
    /// 把 Ask 生命周期变化提交到 V1 账本的可选宿主回调。
    durability_callback: RwLock<Option<PendingQuestionDurabilityCallback>>,
    /// Monotonic sequence allocator used to preserve display order for concurrently pending questions.
    /// 用于在并发待回答问题之间保持展示顺序的单调序号分配器。
    next_sequence: AtomicU64,
}

/// Route identity of the pending request of one tool call: a transitional UI projection of the
/// authoritative V1 local identity. Provider-native ids never become keys.
/// 一次工具调用的待处理请求路由标识：权威 V1 本地身份的过渡 UI 投影。Provider 原生 id 绝不成为主键。
fn pending_request_id(
    session_id: SessionId,
    api_call_id: ApiCallId,
    tool_call_id: ToolCallId,
) -> String {
    format!(
        "{}:{}:{}",
        session_id,
        api_call_id.value(),
        tool_call_id.value()
    )
}

impl Default for QuestionRuntime {
    /// Builds one empty question runtime with a fresh monotonic sequence allocator.
    /// 构建一份带全新单调序号分配器的空问题运行时。
    fn default() -> Self {
        Self::new()
    }
}

impl QuestionRuntime {
    /// Builds one empty question runtime with no pending structured question requests.
    /// 构建一份不含任何待回答结构化问题请求的空问题运行时。
    pub fn new() -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
            change_callback: RwLock::new(None),
            durability_callback: RwLock::new(None),
            next_sequence: AtomicU64::new(1),
        }
    }

    /// Installs or clears the host callback invoked when pending-question requests are added or removed.
    /// 安装或清除待回答问题请求增删时触发的宿主回调。
    pub fn set_change_callback(
        &self,
        callback: Option<PendingQuestionChangeCallback>,
    ) -> Result<(), String> {
        *self.change_callback.write().map_err(|_| {
            String::from("The question runtime registry is locked and cannot update its callback.")
        })? = callback;
        Ok(())
    }

    /// Installs or clears the host callback that durably commits Ask lifecycle transitions.
    /// 安装或清除可靠提交 Ask 生命周期转换的宿主回调。
    ///
    /// # Parameters
    /// 参数
    /// `callback` receives exact local identities and canonical terminal results before visibility or resumption.
    /// `callback` 在可见或恢复前接收精确本地身份与规范终态结果。
    ///
    /// # Returns
    /// 返回值
    /// Success after replacing the callback, or an explicit callback-registry lock error.
    /// 替换回调后返回成功；回调注册表锁失败时返回显式错误。
    pub fn set_durability_callback(
        &self,
        callback: Option<PendingQuestionDurabilityCallback>,
    ) -> Result<(), String> {
        *self.durability_callback.write().map_err(|_| {
            String::from(
                "The question runtime durability registry is locked and cannot update its callback.",
            )
        })? = callback;
        Ok(())
    }

    /// Suspends the current tool invocation, registers the pending structured questions, and waits until the WebUI replies or rejects them.
    /// 挂起当前工具调用、注册待回答结构化问题，并等待WebUI回复或拒绝。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the controller session that owns the question request.
    /// `session_id` 标识拥有该提问请求的控制器会话。
    /// `questions` contains the validated ordered prompts presented to the user.
    /// `questions` 包含向用户展示的有序待校验问题。
    ///
    /// # Returns
    /// 返回值
    /// Returns the structured reply, or a validation, registry, cancellation, or channel error.
    /// 返回结构化答案，失败时返回校验、注册表、取消或通道错误。
    pub async fn ask(
        self: &Arc<Self>,
        session_id: SessionId,
        request_stream_id: RequestStreamId,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
        controller_tool_call_id: String,
        provider_tool_call_id: Option<String>,
        tool_name: String,
        tool_arguments: Value,
        questions: Vec<QuestionPrompt>,
    ) -> Result<QuestionReply, String> {
        validate_question_prompts(&questions)?;
        self.suspend(
            session_id,
            request_stream_id,
            api_call_id,
            tool_call_id,
            controller_tool_call_id,
            provider_tool_call_id,
            tool_name,
            tool_arguments,
            questions,
            None,
        )
        .await
    }

    /// Registers one pending request, commits it as WaitingUser, and waits for the user.
    /// 注册一条待处理请求，可靠提交为 WaitingUser，并等待用户处理。
    /// Shared by Ask and permission approvals; `approval` selects how replies are committed.
    /// 由 Ask 与权限审批共用；`approval` 决定回复的提交方式。
    async fn suspend(
        self: &Arc<Self>,
        session_id: SessionId,
        request_stream_id: RequestStreamId,
        api_call_id: ApiCallId,
        tool_call_id: ToolCallId,
        controller_tool_call_id: String,
        provider_tool_call_id: Option<String>,
        tool_name: String,
        tool_arguments: Value,
        questions: Vec<QuestionPrompt>,
        approval: Option<ToolApprovalPrompt>,
    ) -> Result<QuestionReply, String> {
        if tool_name.trim().is_empty() || tool_name.trim() != tool_name {
            return Err(String::from(
                "The Ask tool name must be exact, non-empty, and free of surrounding whitespace.",
            ));
        }
        if controller_tool_call_id.trim().is_empty()
            || controller_tool_call_id.trim() != controller_tool_call_id
        {
            return Err(String::from(
                "The controller tool-call identifier must be exact, non-empty, and free of surrounding whitespace.",
            ));
        }
        if provider_tool_call_id
            .as_ref()
            .is_some_and(|identity| identity.trim().is_empty() || identity.trim() != identity)
        {
            return Err(String::from(
                "The optional Provider tool-call identifier must be exact and non-empty when present.",
            ));
        }
        // changeCallback is acquired before pending mutation so callback-lock failure cannot hide a request.
        // changeCallback 在修改 pending 前获取，确保回调锁失败不会隐藏请求。
        let change_callback = self.change_callback_snapshot()?;
        // durabilityCallback is acquired before registration so a callback-lock failure cannot create a process-only Ask.
        // durabilityCallback 在注册前获取，确保回调锁失败不会创建仅存在于进程内的 Ask。
        let durability_callback = self.durability_callback_snapshot()?;
        let request_id = pending_request_id(session_id, api_call_id, tool_call_id);
        let (sender, receiver) = oneshot::channel::<PendingQuestionCompletion>();
        {
            let mut pending = self.pending.write().map_err(|_| {
                String::from(
                    "The question runtime registry is locked and cannot register pending questions.",
                )
            })?;
            if pending.contains_key(&request_id) {
                return Err(format!(
                    "The V1 Ask identity `{request_id}` is already waiting for an answer."
                ));
            }
            let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
            let request = QuestionRequest {
                id: request_id.clone(),
                session_id,
                request_stream_id,
                api_call_id,
                tool_call_id,
                controller_tool_call_id,
                provider_tool_call_id,
                tool_name,
                tool_arguments,
                sequence,
                questions,
                approval,
            };
            pending.insert(request_id.clone(), PendingQuestionEntry { request, sender });
        }
        // pendingGuard owns cleanup before the host callback can observe the new request.
        // pendingGuard 在宿主回调观察新请求前即拥有清理责任。
        let mut pending_guard = PendingQuestionGuard {
            runtime: Arc::clone(self),
            request_id,
            change_callback: None,
        };
        let request = self.require_pending_request(&pending_guard.request_id)?;
        Self::commit_question_durability(
            durability_callback.as_ref(),
            PendingQuestionDurabilityAction::Waiting { request },
        )?;
        // Durable WaitingUser is the commit point; a projection failure is diagnostic-only and cannot roll it back.
        // 可靠 WaitingUser 是提交点；投影失败仅用于诊断，不能回滚该事实。
        if let Err(error) = Self::notify_pending_question_changed(change_callback.as_ref()) {
            eprintln!(
                "Question runtime failed to publish a durably waiting request `{}`: {error}",
                pending_guard.request_id
            );
        }
        pending_guard.change_callback = change_callback;
        match receiver.await {
            Ok(PendingQuestionCompletion::Reply(reply)) => Ok(reply),
            Ok(PendingQuestionCompletion::Reject) => Err(String::from(QUESTION_REJECTED_ERROR)),
            Err(_) => Err(String::from(
                "The pending question request lost its host connection, so the ask tool cannot continue.",
            )),
        }
    }

    /// Returns every currently pending structured question request ordered by creation sequence so the WebUI can present the oldest blocking prompt first.
    /// 返回按创建序号排序的全部当前待回答结构化问题请求，供WebUI优先展示最早阻塞的提示。
    pub fn list_pending(&self) -> Result<Vec<QuestionRequest>, String> {
        let mut requests = self
            .pending
            .read()
            .map_err(|_| {
                String::from(
                    "The question runtime registry is locked and cannot read pending questions.",
                )
            })?
            .values()
            .map(|entry| entry.request.clone())
            .collect::<Vec<_>>();
        requests.sort_by_key(|request| request.sequence);
        Ok(requests)
    }

    /// Resolves one pending request with concrete structured answers and resumes the blocked tool invocation when the request still exists.
    /// 使用具体结构化答案完成一条待回答请求，并在请求仍存在时恢复被阻塞的工具调用。
    ///
    /// # Parameters
    /// 参数
    /// `request_id` identifies the exact pending request to complete.
    /// `request_id` 标识需要完成的精确待回答请求。
    /// `answers` contains ordered structured answers aligned with the original prompts.
    /// `answers` 包含与原始问题对齐的有序结构化答案。
    ///
    /// # Returns
    /// 返回值
    /// Returns `Ok(true)` after delivery, `Ok(false)` when absent, or a lock or channel error.
    /// 投递成功后返回 `Ok(true)`，请求不存在时返回 `Ok(false)`，失败时返回锁或通道错误。
    pub fn reply(&self, request_id: &str, answers: Vec<Vec<String>>) -> Result<bool, String> {
        self.reply_then(request_id, answers, |_, _| {})
    }

    /// Submits answers like [`Self::reply`], running `before_resume` once the reply is committed
    /// and before the blocked tool resumes, so observers record it inside the asking turn.
    /// 与 [`Self::reply`] 相同地提交答案，并在回答提交后、被阻塞的工具恢复前运行 `before_resume`，
    /// 使观察者在提问所在回合内记录它。
    ///
    /// `before_resume` receives the committed request and answers; it runs outside the pending
    /// lock and only when this call commits.
    /// `before_resume` 接收已提交的请求与答案；它在 pending 锁之外运行，且仅在本次调用提交时运行。
    pub fn reply_then(
        &self,
        request_id: &str,
        answers: Vec<Vec<String>>,
        before_resume: impl FnOnce(&QuestionRequest, &[Vec<String>]),
    ) -> Result<bool, String> {
        // changeCallback must be readable before the pending request is consumed.
        // changeCallback 必须在消费 pending 请求前可读。
        let change_callback = self.change_callback_snapshot()?;
        let durability_callback = self.durability_callback_snapshot()?;
        let reply = QuestionReply { answers };
        let entry = {
            // pending serializes one exact request through durable commit and removal. The lock is
            // released before process delivery or WebUI projection so callbacks may read the new state.
            // pending 串行化一条精确请求的可靠提交与移除；进程投递或 WebUI 投影前释放该锁，
            // 使回调能够读取新状态。
            let mut pending = self.pending.write().map_err(|_| {
                String::from("The question runtime registry is locked and cannot submit answers.")
            })?;
            let Some(request) = pending.get(request_id).map(|entry| entry.request.clone()) else {
                return Ok(false);
            };
            let action = if request.approval.is_some() {
                approval::validate_approval_answers(&reply.answers)?;
                PendingQuestionDurabilityAction::Resumed { request }
            } else {
                let result = build_question_core_result(&request, &reply)?;
                PendingQuestionDurabilityAction::Answered { request, result }
            };
            Self::commit_question_durability(durability_callback.as_ref(), action)?;
            pending.remove(request_id).ok_or_else(|| {
                format!(
                    "The pending question request `{request_id}` disappeared during reply commit."
                )
            })?
        };
        before_resume(&entry.request, &reply.answers);
        // deliveryResult distinguishes a real tool resumption from a vanished receiver.
        // deliveryResult 区分真实工具恢复与接收方已经消失。
        let delivery_result = entry.sender.send(PendingQuestionCompletion::Reply(reply));
        // notificationResult records host-projection failure without allowing callback panic to escape.
        // notificationResult 记录宿主投影失败，同时禁止回调 panic 逃逸。
        let notification_result = Self::notify_pending_question_changed(change_callback.as_ref());
        if delivery_result.is_err() {
            eprintln!(
                "Question runtime committed reply for pending request `{request_id}`, but its process-local tool receiver was already gone."
            );
        }
        // Durable completion is the irreversible commit point; process delivery and projection failures are diagnostic-only afterward.
        // 可靠完成是不可逆提交点；其后的进程投递与投影失败都只能作为诊断。
        if let Err(error) = notification_result {
            eprintln!(
                "Question runtime failed to publish committed reply removal for pending request `{request_id}`: {error}"
            );
        }
        Ok(true)
    }

    /// Rejects one pending request without answers and resumes the blocked tool invocation with a cancellation error when the request still exists.
    /// 在请求仍存在时无答案地拒绝一条待回答请求，并以取消错误恢复被阻塞的工具调用。
    ///
    /// # Parameters
    /// 参数
    /// `request_id` identifies the exact pending request to reject.
    /// `request_id` 标识需要拒绝的精确待回答请求。
    ///
    /// # Returns
    /// 返回值
    /// Returns `Ok(true)` after delivery, `Ok(false)` when absent, or a lock or channel error.
    /// 投递成功后返回 `Ok(true)`，请求不存在时返回 `Ok(false)`，失败时返回锁或通道错误。
    pub fn reject(&self, request_id: &str) -> Result<bool, String> {
        self.reject_then(request_id, |_| {})
    }

    /// Rejects like [`Self::reject`], running `before_resume` once the rejection is committed and
    /// before the blocked tool resumes, so observers record it inside the asking turn.
    /// 与 [`Self::reject`] 相同地拒绝，并在拒绝提交后、被阻塞的工具恢复前运行 `before_resume`，
    /// 使观察者在提问所在回合内记录它。
    ///
    /// `before_resume` receives the committed request; it runs outside the pending lock and only
    /// when this call commits.
    /// `before_resume` 接收已提交的请求；它在 pending 锁之外运行，且仅在本次调用提交时运行。
    pub fn reject_then(
        &self,
        request_id: &str,
        before_resume: impl FnOnce(&QuestionRequest),
    ) -> Result<bool, String> {
        // changeCallback must be readable before the pending request is consumed.
        // changeCallback 必须在消费 pending 请求前可读。
        let change_callback = self.change_callback_snapshot()?;
        let durability_callback = self.durability_callback_snapshot()?;
        let entry = {
            // pending serializes one exact request through durable cancellation and removal. The
            // lock is released before process delivery or WebUI projection to avoid lock reentry.
            // pending 串行化一条精确请求的可靠取消与移除；进程投递或 WebUI 投影前释放该锁，
            // 避免锁重入。
            let mut pending = self.pending.write().map_err(|_| {
                String::from(
                    "The question runtime registry is locked and cannot cancel the pending question.",
                )
            })?;
            let Some(request) = pending.get(request_id).map(|entry| entry.request.clone()) else {
                return Ok(false);
            };
            let result = build_question_rejection_result(&request)?;
            Self::commit_question_durability(
                durability_callback.as_ref(),
                PendingQuestionDurabilityAction::Cancelled { request, result },
            )?;
            pending.remove(request_id).ok_or_else(|| {
                format!(
                    "The pending question request `{request_id}` disappeared during rejection commit."
                )
            })?
        };
        before_resume(&entry.request);
        // deliveryResult distinguishes a real cancellation from a vanished receiver.
        // deliveryResult 区分真实取消投递与接收方已经消失。
        let delivery_result = entry.sender.send(PendingQuestionCompletion::Reject);
        // notificationResult records host-projection failure without allowing callback panic to escape.
        // notificationResult 记录宿主投影失败，同时禁止回调 panic 逃逸。
        let notification_result = Self::notify_pending_question_changed(change_callback.as_ref());
        if delivery_result.is_err() {
            eprintln!(
                "Question runtime committed rejection for pending request `{request_id}`, but its process-local tool receiver was already gone."
            );
        }
        // Durable cancellation is the irreversible commit point; process delivery and projection failures are diagnostic-only afterward.
        // 可靠取消是不可逆提交点；其后的进程投递与投影失败都只能作为诊断。
        if let Err(error) = notification_result {
            eprintln!(
                "Question runtime failed to publish committed rejection removal for pending request `{request_id}`: {error}"
            );
        }
        Ok(true)
    }

    /// Executes one model-visible ask tool call for the current controller session.
    /// 为当前控制器会话执行一次模型可见 ask 工具调用。
    pub async fn invoke_tool(
        self: &Arc<Self>,
        context: VulcanToolInvocationContext,
        args: Value,
        tool_name: &str,
    ) -> Result<VulcanToolInvocationResult, String> {
        let current_session_id = Uuid::parse_str(&context.session_id).map_err(|error| {
            format!("Failed to parse the session identifier for the `{tool_name}` tool: {error}")
        })?;
        let tool_arguments = args.clone();
        let parsed_args = parse_question_tool_arguments(args)?;
        let reply = self
            .ask(
                current_session_id,
                context.request_stream_id,
                context.api_call_id,
                context.tool_call_id,
                context.controller_tool_call_id,
                context.provider_tool_call_id,
                tool_name.to_string(),
                tool_arguments,
                parsed_args.questions.clone(),
            )
            .await?;
        Ok(build_question_invocation_result(
            tool_name,
            &parsed_args.questions,
            &reply,
        ))
    }

    /// Removes one pending request during cancellation and notifies its captured host callback.
    /// 在取消期间移除一条待回答请求，并通知其捕获的宿主回调。
    ///
    /// # Parameters
    /// 参数
    /// `request_id` identifies the exact pending request owned by the dropping guard.
    /// `request_id` 标识由正在销毁的守卫拥有的精确待回答请求。
    /// `change_callback` is the stable callback captured before request registration.
    /// `change_callback` 是请求注册前捕获的稳定回调。
    ///
    /// # Returns
    /// 返回值
    /// Returns whether the request was removed, or an explicit pending-registry lock error.
    /// 返回请求是否已移除，失败时返回明确的 pending 注册表锁错误。
    fn remove_pending_request(
        &self,
        request_id: &str,
        change_callback: Option<&PendingQuestionChangeCallback>,
    ) -> Result<bool, String> {
        // removed records whether this guard still owned a live registry entry.
        // removed 记录该守卫是否仍拥有有效注册项。
        let removed = self
            .pending
            .write()
            .map_err(|_| {
                format!(
                    "The question runtime registry is locked and cannot clean up pending request `{request_id}`."
                )
            })?
            .remove(request_id)
            .is_some();
        if removed {
            Self::notify_pending_question_changed(change_callback)?;
        }
        Ok(removed)
    }

    /// Returns the stable host callback required before mutating pending-question state.
    /// 返回修改待回答问题状态前所需的稳定宿主回调。
    ///
    /// # Returns
    /// 返回值
    /// Returns the optional callback snapshot, or an explicit callback-registry lock error.
    /// 返回可选回调快照，失败时返回明确的回调注册表锁错误。
    fn change_callback_snapshot(&self) -> Result<Option<PendingQuestionChangeCallback>, String> {
        self
            .change_callback
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| {
                String::from(
                    "The question runtime callback registry is locked and cannot prepare a pending-question change.",
                )
            })
    }

    /// Returns the stable durability callback required before publishing or resolving an Ask.
    /// 返回发布或解决 Ask 前所需的稳定可靠性回调。
    ///
    /// # Returns
    /// 返回值
    /// Returns the optional callback snapshot, or an explicit callback-registry lock error.
    /// 返回可选回调快照；回调注册表锁失败时返回显式错误。
    fn durability_callback_snapshot(
        &self,
    ) -> Result<Option<PendingQuestionDurabilityCallback>, String> {
        self.durability_callback
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| {
                String::from(
                    "The question runtime durability registry is locked and cannot prepare an Ask lifecycle change.",
                )
            })
    }

    /// Clones one exact pending request after its process-local reservation is installed.
    /// 在进程内预留安装后克隆一条精确待回答请求。
    ///
    /// # Parameters
    /// 参数
    /// `request_id` identifies the exact registered request.
    /// `request_id` 标识精确的已注册请求。
    ///
    /// # Returns
    /// 返回值
    /// Returns the canonical request or an explicit registry/identity failure.
    /// 返回规范请求；注册表或身份失败时返回显式错误。
    fn require_pending_request(&self, request_id: &str) -> Result<QuestionRequest, String> {
        self.pending
            .read()
            .map_err(|_| {
                String::from(
                    "The question runtime registry is locked and cannot read the pending Ask reservation.",
                )
            })?
            .get(request_id)
            .map(|entry| entry.request.clone())
            .ok_or_else(|| {
                format!(
                    "The pending question request `{request_id}` disappeared before its durable waiting checkpoint."
                )
            })
    }

    /// Commits one Ask lifecycle action through the installed V1 durability bridge.
    /// 通过已安装的 V1 可靠性桥提交一条 Ask 生命周期动作。
    ///
    /// # Parameters
    /// 参数
    /// `callback` is the stable optional bridge and `action` owns the exact lifecycle facts.
    /// `callback` 是稳定可选桥；`action` 持有精确生命周期事实。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the optional durable commit, or its explicit failure.
    /// 可选可靠提交完成后返回成功；失败时返回其显式错误。
    fn commit_question_durability(
        callback: Option<&PendingQuestionDurabilityCallback>,
        action: PendingQuestionDurabilityAction,
    ) -> Result<(), String> {
        match callback {
            Some(callback) => callback(action),
            None => Err(String::from(
                "The Ask tool cannot run before its V1 durability bridge is installed.",
            )),
        }
    }

    /// Invokes one already-owned host callback after pending-question state changed.
    /// 待回答问题状态变化后调用一个已取得所有权的宿主回调。
    ///
    /// # Parameters
    /// 参数
    /// `change_callback` is the stable optional callback acquired before state mutation.
    /// `change_callback` 是状态修改前获取的稳定可选回调。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the optional callback completes, or its explicit error or a contained-panic error.
    /// 可选回调完成后返回成功；失败时返回回调的明确错误或已收容的 panic 错误。
    fn notify_pending_question_changed(
        change_callback: Option<&PendingQuestionChangeCallback>,
    ) -> Result<(), String> {
        let Some(callback) = change_callback else {
            return Ok(());
        };
        // notificationResult contains arbitrary host callback panics at the runtime boundary.
        // notificationResult 在运行时边界内收容任意宿主回调 panic。
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback())).map_err(|_| {
            String::from(
                "The pending-question change callback panicked while updating the host projection.",
            )
        })?
    }
}

/// Builds one model-visible tool candidate backed by the shared question runtime registry.
/// 根据共享问题运行时注册表构建一条模型可见工具候选项。
pub fn build_question_tool_candidate(
    question_runtime: Arc<QuestionRuntime>,
    public_tool_name: impl Into<String>,
) -> VulcanToolCandidate {
    let tool_name = public_tool_name.into();
    let origin = VulcanToolOrigin::new(VulcanToolSourceKind::Native, "native", tool_name.clone());
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor::new(
            tool_name.clone(),
            tool_name.clone(),
            String::from(
                "Ask the user one or more structured questions and wait for the answers before continuing. Use this tool whenever progress depends on a user decision, preference, missing requirement, or explicit confirmation. Prefer this tool over guessing when choices are ambiguous, risky, or approval-sensitive.",
            ),
            None,
            origin,
        ),
        parameters_schema: question_tool_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(move |args, context| {
            let question_runtime = Arc::clone(&question_runtime);
            let tool_name = tool_name.clone();
            Box::pin(async move {
                question_runtime
                    .invoke_tool(context, args, &tool_name)
                    .await
            })
        }),
    }
}

/// Builds the JSON Schema advertised to the model for the `ask` tool surface.
/// 为模型端 `ask` 工具表面构建对外声明的 JSON Schema。
fn question_tool_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "questions": {
                "type": "array",
                "description": "Structured questions to present to the user in order. Use this when you need the user's decision, preference, approval, or missing information before you can continue safely.",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "Full question text shown to the user. Ask for exactly the decision or confirmation you need."
                        },
                        "header": {
                            "type": "string",
                            "description": "Very short field label shown above the question, ideally 1 to 3 words."
                        },
                        "options": {
                            "type": "array",
                            "description": "Selectable options. Prefer options when the user is choosing between concrete paths. Leave empty when free-text input is needed.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "label": {
                                        "type": "string",
                                        "description": "Short option label returned back to the model when selected."
                                    },
                                    "description": {
                                        "type": "string",
                                        "description": "Helpful explanation shown under the option label so the user can compare tradeoffs quickly."
                                    }
                                },
                                "required": ["label", "description"],
                                "additionalProperties": false
                            }
                        },
                        "multiple": {
                            "type": "boolean",
                            "description": "Whether the user may select more than one option label. Leave false for ordinary single-path decisions."
                        }
                    },
                    "required": ["question", "header", "options"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["questions"],
        "additionalProperties": false
    })
}

/// Parses one raw JSON payload into strongly typed question tool arguments.
/// 将一份原始 JSON 载荷解析为强类型问题工具参数。
fn parse_question_tool_arguments(args: Value) -> Result<QuestionToolInvocationArgs, String> {
    serde_json::from_value(args)
        .map_err(|error| format!("Failed to parse ask tool arguments: {error}"))
}

/// Validates a structured question array before it is registered in the runtime so obviously broken prompts fail fast.
/// 在结构化问题数组写入运行时前执行校验，从而让明显损坏的提示尽早失败。
fn validate_question_prompts(questions: &[QuestionPrompt]) -> Result<(), String> {
    if questions.is_empty() {
        return Err(String::from(
            "At least one question must be provided to the ask tool.",
        ));
    }
    for (question_index, question) in questions.iter().enumerate() {
        let question_number = question_index + 1;
        if question.question.trim().is_empty() {
            return Err(format!(
                "Question #{question_number} is missing the `question` text."
            ));
        }
        if question.header.trim().is_empty() {
            return Err(format!(
                "Question #{question_number} is missing the `header` text."
            ));
        }
        let mut seen_labels = std::collections::BTreeSet::new();
        for option in &question.options {
            if option.label.trim().is_empty() {
                return Err(format!(
                    "Question #{question_number} contains an option with an empty label."
                ));
            }
            if option.description.trim().is_empty() {
                return Err(format!(
                    "Option `{}` in question #{question_number} is missing a description.",
                    option.label
                ));
            }
            if !seen_labels.insert(option.label.trim().to_string()) {
                return Err(format!(
                    "Question #{question_number} contains a duplicate option label `{}`.",
                    option.label
                ));
            }
        }
    }
    Ok(())
}

/// Produces one shared answer body from accepted prompts and ordered answers.
/// 根据已接受题目与有序答案生成唯一共享正文。
/// Returns Markdown consumed directly after any host redaction or post-processing.
/// 返回经宿主脱敏或后置处理后直接消费的 Markdown。
fn build_question_reply_projection(
    questions: &[QuestionPrompt],
    answers: &[Vec<String>],
) -> String {
    let mut body = String::from("## User Answers\n\n");
    for (index, prompt) in questions.iter().enumerate() {
        let prefix = format!("- {}: ", prompt.header);
        body.push_str(&prefix);
        if let Some(values) = answers
            .get(index)
            .filter(|values| values.iter().any(|value| !value.trim().is_empty()))
        {
            for (answer_index, answer) in values.iter().enumerate() {
                if answer_index > 0 {
                    body.push_str(", ");
                }
                body.push_str(answer);
            }
        } else {
            body.push_str("Unanswered");
        }
        body.push('\n');
    }
    body.push_str("\nContinue with these answers.");
    body
}

/// Reconstructs the exact controller-owned request facts retained by one pending Ask.
/// 重建一条待回答 Ask 保留的精确控制器请求事实。
///
/// # Parameters
/// 参数
/// `request` owns the exact stream, tool, name, and argument facts captured before suspension.
/// `request` 持有挂起前捕获的精确流、工具、名称与参数事实。
///
/// # Returns
/// 返回值
/// Returns the canonical controller request, or an explicit missing-identity error.
/// 返回规范控制器请求；缺少身份时返回显式错误。
fn core_tool_request_from_question(
    request: &QuestionRequest,
) -> Result<CoreToolInvocationRequest, String> {
    Ok(CoreToolInvocationRequest {
        request_stream_id: request.request_stream_id.as_str().to_string(),
        tool_call_id: request.controller_tool_call_id.clone(),
        tool_name: request.tool_name.clone(),
        arguments: request.tool_arguments.clone(),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    })
}

/// Builds the exact canonical controller result for one structured Ask reply.
/// 为一条结构化 Ask 回答构建精确规范控制器结果。
///
/// # Parameters
/// 参数
/// `request` owns the original invocation and prompts; `reply` owns the ordered answers.
/// `request` 持有原始调用与问题；`reply` 持有有序答案。
///
/// # Returns
/// 返回值
/// Returns the same result later emitted by the resumed tool invocation.
/// 返回恢复后的工具调用随后产生的完全相同结果。
fn build_question_core_result(
    request: &QuestionRequest,
    reply: &QuestionReply,
) -> Result<CoreToolInvocationResult, String> {
    let core_request = core_tool_request_from_question(request)?;
    Ok(canonicalize_tool_invocation_result(
        &core_request,
        build_question_invocation_result(&request.tool_name, &request.questions, reply),
    ))
}

/// Builds the exact canonical controller failure for one rejected Ask.
/// 为一条被拒绝 Ask 构建精确规范控制器失败结果。
///
/// # Parameters
/// 参数
/// `request` owns the original invocation identity and exact arguments.
/// `request` 持有原始调用身份与精确参数。
///
/// # Returns
/// 返回值
/// Returns the same failure later emitted when the blocked tool future resumes.
/// 返回阻塞工具 future 恢复后产生的完全相同失败结果。
fn build_question_rejection_result(
    request: &QuestionRequest,
) -> Result<CoreToolInvocationResult, String> {
    let core_request = core_tool_request_from_question(request)?;
    Ok(build_tool_failure_invocation_result(
        &core_request,
        QUESTION_REJECTED_ERROR,
    ))
}

/// Builds the final tool invocation result surfaced back to the model after the user has answered every structured question.
/// 在用户完成全部结构化问题作答后，构建回传给模型的最终工具调用结果。
fn build_question_invocation_result(
    _tool_name: &str,
    questions: &[QuestionPrompt],
    reply: &QuestionReply,
) -> VulcanToolInvocationResult {
    let output = build_question_reply_projection(questions, &reply.answers);
    VulcanToolInvocationResult::from_model_output(Value::String(output)).with_host_result(
        VulcanToolHostResult {
            kind: String::from(QUESTION_TOOL_HOST_RESULT_KIND),
            payload: json!({
                "format": "markdown",
            }),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Measures the old three-copy answer envelope against the actual shared Markdown producer.
    /// 对照旧的三份答案封套，计量实际共享 Markdown 生产者。
    #[test]
    fn ask_payload_cost_sample() {
        let prompt = test_question_prompt();
        let answer = format!("UNIQUE_ASK_BODY {}", "accepted answer ".repeat(128));
        let reply = QuestionReply {
            answers: vec![vec![answer.clone()]],
        };
        let after = build_question_invocation_result("ask", std::slice::from_ref(&prompt), &reply);
        let before = VulcanToolInvocationResult::from_model_output(after.model_output.clone()).with_host_result(VulcanToolHostResult {
            kind:"ask".into(), payload:json!({
                "tool_name":"ask", "headers":[prompt.header], "answers":reply.answers,
                "display":{"kind":"ask_question","title":"向用户提问","ignored_answer_text":"用户忽略回答",
                    "questions":[{"header":prompt.header,"question":prompt.question,"options":[],"multiple":false,
                        "answers":[answer],"answered":true}]}
            })
        });
        crate::agent_runtime::tool_runtime_bridge::assert_result_payload_cost(
            "ask",
            before,
            after,
            "UNIQUE_ASK_BODY",
        );
    }

    /// Verifies exact Unicode answers survive canonical serialization without storing duplicate text.
    /// 验证精确 Unicode 答案经过规范序列化仍准确，且不保存重复正文。
    #[test]
    fn markdown_answers_survive_canonical_roundtrip() {
        let prompts = vec![test_question_prompt(), test_question_prompt()];
        let reply = QuestionReply {
            answers: vec![
                vec!["唯一答案🦀, one\ntwo".into(), " second ".into()],
                vec![],
            ],
        };
        let result = build_question_invocation_result("ask", &prompts, &reply);
        let request = CoreToolInvocationRequest {
            request_stream_id: "ask-test".into(),
            tool_call_id: "ask-call".into(),
            tool_name: "ask".into(),
            arguments: test_tool_arguments(),
            argument_parse_error: None,
            raw_arguments: Default::default(),
            user_request: Default::default(),
        };
        let canonical = canonicalize_tool_invocation_result(&request, result);
        let stored = serde_json::to_string(&canonical).expect("serialized result");
        assert_eq!(stored.matches("唯一答案").count(), 1);
        let restored: CoreToolInvocationResult =
            serde_json::from_str(&stored).expect("restored result");
        assert_eq!(
            restored.host_result.unwrap()["payload"],
            json!({"format":"markdown"})
        );
        for answer in reply.answers.iter().flatten() {
            assert!(restored.model_output.contains(answer));
        }
    }

    /// Builds one deterministic request-stream identity for Ask runtime tests.
    /// 为 Ask 运行时测试构建一个确定性的请求流身份。
    fn test_request_stream_id() -> RequestStreamId {
        RequestStreamId::try_new("ask-request-stream-1").expect("test request stream id")
    }

    /// Builds one deterministic controller tool-call identity for Ask runtime tests.
    /// 为 Ask 运行时测试构建一个确定性的控制器工具调用身份。
    fn test_controller_tool_call_id() -> String {
        String::from("controller-ask-tool-call-1")
    }

    /// Builds one exact structured argument payload for direct Ask runtime tests.
    /// 为直接 Ask 运行时测试构建一份精确结构化参数载荷。
    fn test_tool_arguments() -> Value {
        json!({"questions": []})
    }

    /// Builds one deterministic structured prompt for Ask durability tests.
    /// 为 Ask 可靠性测试构建一个确定性的结构化问题。
    fn test_question_prompt() -> QuestionPrompt {
        QuestionPrompt {
            question: String::from("Confirm the durable Ask transition"),
            header: String::from("Confirm"),
            options: Vec::new(),
            multiple: Some(false),
        }
    }

    /// Installs one successful V1 durability bridge for focused runtime tests.
    /// 为聚焦运行时测试安装一个成功的 V1 可靠落盘桥。
    fn install_test_durability_callback(runtime: &QuestionRuntime) {
        runtime
            .set_durability_callback(Some(Arc::new(|_| Ok(()))))
            .expect("test Ask durability callback should install");
    }

    /// Verifies ask stays executable in Plan mode because planning depends on user decisions.
    /// 验证 ask 在计划模式下保持可执行，因为计划协作依赖用户决策。
    #[test]
    fn question_tool_is_allowed_in_plan_mode() {
        let candidate = build_question_tool_candidate(Arc::new(QuestionRuntime::new()), "ask");

        assert!(matches!(
            candidate.plan_policy,
            VulcanPlanToolPolicy::Visible
        ));
    }

    /// Verifies one pending question request can suspend the tool flow and later resume successfully with structured answers.
    /// 验证单条待回答问题请求能够挂起工具流程，并在稍后以结构化答案成功恢复。
    #[tokio::test]
    async fn question_runtime_round_trips_one_reply() {
        let runtime = Arc::new(QuestionRuntime::new());
        install_test_durability_callback(&runtime);
        let ask_runtime = Arc::clone(&runtime);
        let ask_task = tokio::spawn(async move {
            ask_runtime
                .ask(
                    Uuid::nil(),
                    test_request_stream_id(),
                    ApiCallId::try_new(1).expect("test API id"),
                    ToolCallId::try_new(2).expect("test tool id"),
                    test_controller_tool_call_id(),
                    Some(String::from("provider-ask-1")),
                    String::from("ask"),
                    test_tool_arguments(),
                    vec![QuestionPrompt {
                        question: String::from("Choose one path"),
                        header: String::from("Path"),
                        options: vec![QuestionOption {
                            label: String::from("A"),
                            description: String::from("Take route A"),
                        }],
                        multiple: Some(false),
                    }],
                )
                .await
        });
        tokio::task::yield_now().await;
        let pending = runtime.list_pending().expect("pending question list");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].api_call_id.value(), 1);
        assert_eq!(pending[0].tool_call_id.value(), 2);
        assert_eq!(
            pending[0].provider_tool_call_id.as_deref(),
            Some("provider-ask-1")
        );
        assert_eq!(pending[0].questions[0].question, "Choose one path");
        runtime
            .reply(&pending[0].id, vec![vec![String::from("A")]])
            .expect("reply should succeed");
        let reply = ask_task
            .await
            .expect("ask task should join")
            .expect("reply should resolve");
        assert_eq!(reply.answers, vec![vec![String::from("A")]]);
    }

    /// Verifies the committed-reply and committed-rejection observers run before the asking tool
    /// resumes, so a rejection that ends the run cannot close the turn before they record it.
    /// 验证已提交回答与已提交拒绝的观察者在提问工具恢复前运行，使结束本次运行的拒绝无法在其记录前
    /// 关闭回合。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn commit_observers_run_before_the_asking_tool_resumes() {
        for reject in [false, true] {
            let runtime = Arc::new(QuestionRuntime::new());
            install_test_durability_callback(&runtime);
            let ask_runtime = Arc::clone(&runtime);
            let ask_task = tokio::spawn(async move {
                ask_runtime
                    .ask(
                        Uuid::nil(),
                        test_request_stream_id(),
                        ApiCallId::try_new(1).expect("test API id"),
                        ToolCallId::try_new(2).expect("test tool id"),
                        test_controller_tool_call_id(),
                        None,
                        String::from("ask"),
                        test_tool_arguments(),
                        vec![test_question_prompt()],
                    )
                    .await
            });
            let request_id = loop {
                if let Some(request) = runtime.list_pending().expect("pending list").pop() {
                    break request.id;
                }
                tokio::task::yield_now().await;
            };
            // A tool resumed before the observer would finish on the other worker meanwhile.
            // 若工具先于观察者恢复，它会在此期间于另一工作线程上完成。
            let observed = |request: &QuestionRequest| {
                assert_eq!(request.id, request_id);
                std::thread::sleep(std::time::Duration::from_millis(100));
                assert!(
                    !ask_task.is_finished(),
                    "the tool resumed before the observer ran"
                );
            };
            let mut calls = 0;
            let committed = if reject {
                runtime.reject_then(&request_id, |request| {
                    calls += 1;
                    observed(request);
                })
            } else {
                runtime.reply_then(
                    &request_id,
                    vec![vec![String::from("A")]],
                    |request, answers| {
                        calls += 1;
                        assert_eq!(answers, [vec![String::from("A")]]);
                        observed(request);
                    },
                )
            };
            assert!(committed.expect("commit should succeed"));
            assert_eq!(calls, 1);
            assert_eq!(
                ask_task.await.expect("ask task should join").is_err(),
                reject
            );
            assert!(
                !runtime
                    .reject_then(&request_id, |_| panic!(
                        "an absent request is never observed"
                    ))
                    .expect("absent rejection")
            );
        }
    }

    /// Verifies Ask refuses a process-only registration when no V1 durability bridge is installed.
    /// 验证未安装 V1 可靠性桥时 Ask 会拒绝仅存在于进程内的注册。
    #[tokio::test]
    async fn question_runtime_requires_v1_durability_before_visibility() {
        let runtime = Arc::new(QuestionRuntime::new());
        let projection_count = Arc::new(AtomicU64::new(0));
        let projection_counter = Arc::clone(&projection_count);
        runtime
            .set_change_callback(Some(Arc::new(move || {
                projection_counter.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })))
            .expect("projection callback should install");

        let error = runtime
            .ask(
                Uuid::nil(),
                test_request_stream_id(),
                ApiCallId::try_new(1).expect("test API id"),
                ToolCallId::try_new(2).expect("test tool id"),
                test_controller_tool_call_id(),
                None,
                String::from("ask"),
                test_tool_arguments(),
                vec![test_question_prompt()],
            )
            .await
            .expect_err("Ask without a durability bridge must fail");

        assert!(error.contains("V1 durability bridge"));
        assert_eq!(projection_count.load(Ordering::Relaxed), 0);
        assert!(runtime.list_pending().expect("pending registry").is_empty());
    }

    /// Verifies a failed WaitingUser commit removes the private reservation before any WebUI projection.
    /// 验证 WaitingUser 提交失败时会在任何 WebUI 投影前移除私有预留。
    #[tokio::test]
    async fn question_runtime_waiting_commit_failure_never_becomes_visible() {
        let runtime = Arc::new(QuestionRuntime::new());
        runtime
            .set_durability_callback(Some(Arc::new(|action| match action {
                PendingQuestionDurabilityAction::Waiting { .. } => {
                    Err(String::from("simulated WaitingUser persistence failure"))
                }
                _ => panic!("waiting admission must fail before terminal Ask actions"),
            })))
            .expect("failing durability callback should install");
        let projection_count = Arc::new(AtomicU64::new(0));
        let projection_counter = Arc::clone(&projection_count);
        runtime
            .set_change_callback(Some(Arc::new(move || {
                projection_counter.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })))
            .expect("projection callback should install");

        let error = runtime
            .ask(
                Uuid::nil(),
                test_request_stream_id(),
                ApiCallId::try_new(1).expect("test API id"),
                ToolCallId::try_new(2).expect("test tool id"),
                test_controller_tool_call_id(),
                None,
                String::from("ask"),
                test_tool_arguments(),
                vec![test_question_prompt()],
            )
            .await
            .expect_err("failed WaitingUser persistence must reject Ask");

        assert!(error.contains("simulated WaitingUser persistence failure"));
        assert_eq!(projection_count.load(Ordering::Relaxed), 0);
        assert!(runtime.list_pending().expect("pending registry").is_empty());
    }

    /// Verifies a failed durable answer leaves the exact pending Ask suspended and retryable.
    /// 验证可靠回答提交失败时精确 pending Ask 会保持挂起并可重试。
    #[tokio::test]
    async fn question_runtime_answer_commit_failure_preserves_pending_for_retry() {
        let runtime = Arc::new(QuestionRuntime::new());
        let fail_answer = Arc::new(AtomicBool::new(true));
        let answer_failure = Arc::clone(&fail_answer);
        runtime
            .set_durability_callback(Some(Arc::new(move |action| match action {
                PendingQuestionDurabilityAction::Waiting { .. } => Ok(()),
                PendingQuestionDurabilityAction::Answered { .. }
                    if answer_failure.load(Ordering::Relaxed) =>
                {
                    Err(String::from("simulated answer persistence failure"))
                }
                PendingQuestionDurabilityAction::Answered { .. } => Ok(()),
                PendingQuestionDurabilityAction::Cancelled { .. }
                | PendingQuestionDurabilityAction::Resumed { .. } => {
                    panic!("answer retry test must not cancel or resume the Ask")
                }
            })))
            .expect("conditional durability callback should install");
        let ask_runtime = Arc::clone(&runtime);
        let ask_task = tokio::spawn(async move {
            ask_runtime
                .ask(
                    Uuid::nil(),
                    test_request_stream_id(),
                    ApiCallId::try_new(1).expect("test API id"),
                    ToolCallId::try_new(2).expect("test tool id"),
                    test_controller_tool_call_id(),
                    None,
                    String::from("ask"),
                    test_tool_arguments(),
                    vec![test_question_prompt()],
                )
                .await
        });
        tokio::task::yield_now().await;
        let request_id = runtime
            .list_pending()
            .expect("pending Ask should be readable")
            .first()
            .expect("one pending Ask")
            .id
            .clone();

        let error = runtime
            .reply(&request_id, vec![vec![String::from("confirmed")]])
            .expect_err("failed durable answer must remain retryable");
        assert!(error.contains("simulated answer persistence failure"));
        assert!(!ask_task.is_finished());
        assert_eq!(runtime.list_pending().expect("pending registry").len(), 1);

        fail_answer.store(false, Ordering::Relaxed);
        assert!(
            runtime
                .reply(&request_id, vec![vec![String::from("confirmed")]])
                .expect("exact durable answer retry should succeed")
        );
        let reply = ask_task
            .await
            .expect("Ask task should join")
            .expect("durable answer should resume Ask");
        assert_eq!(reply.answers, vec![vec![String::from("confirmed")]]);
    }

    /// Verifies a failed durable cancellation leaves the exact pending Ask suspended and retryable.
    /// 验证可靠取消提交失败时精确 pending Ask 会保持挂起并可重试。
    #[tokio::test]
    async fn question_runtime_cancel_commit_failure_preserves_pending_for_retry() {
        let runtime = Arc::new(QuestionRuntime::new());
        let fail_cancel = Arc::new(AtomicBool::new(true));
        let cancel_failure = Arc::clone(&fail_cancel);
        runtime
            .set_durability_callback(Some(Arc::new(move |action| match action {
                PendingQuestionDurabilityAction::Waiting { .. } => Ok(()),
                PendingQuestionDurabilityAction::Cancelled { .. }
                    if cancel_failure.load(Ordering::Relaxed) =>
                {
                    Err(String::from("simulated cancellation persistence failure"))
                }
                PendingQuestionDurabilityAction::Cancelled { .. } => Ok(()),
                PendingQuestionDurabilityAction::Answered { .. }
                | PendingQuestionDurabilityAction::Resumed { .. } => {
                    panic!("cancellation retry test must not answer or resume the Ask")
                }
            })))
            .expect("conditional durability callback should install");
        let ask_runtime = Arc::clone(&runtime);
        let ask_task = tokio::spawn(async move {
            ask_runtime
                .ask(
                    Uuid::nil(),
                    test_request_stream_id(),
                    ApiCallId::try_new(1).expect("test API id"),
                    ToolCallId::try_new(2).expect("test tool id"),
                    test_controller_tool_call_id(),
                    None,
                    String::from("ask"),
                    test_tool_arguments(),
                    vec![test_question_prompt()],
                )
                .await
        });
        tokio::task::yield_now().await;
        let request_id = runtime
            .list_pending()
            .expect("pending Ask should be readable")
            .first()
            .expect("one pending Ask")
            .id
            .clone();

        let error = runtime
            .reject(&request_id)
            .expect_err("failed durable cancellation must remain retryable");
        assert!(error.contains("simulated cancellation persistence failure"));
        assert!(!ask_task.is_finished());
        assert_eq!(runtime.list_pending().expect("pending registry").len(), 1);

        fail_cancel.store(false, Ordering::Relaxed);
        assert!(
            runtime
                .reject(&request_id)
                .expect("exact durable cancellation retry should succeed")
        );
        let error = ask_task
            .await
            .expect("Ask task should join")
            .expect_err("durable cancellation should reject Ask");
        assert!(error.contains("abandoned"));
    }

    /// Verifies a duplicate V1 local Ask identity cannot replace an already pending invocation.
    /// 验证重复的 V1 本地 Ask 身份不能覆盖已经挂起的调用。
    #[tokio::test]
    async fn question_runtime_rejects_duplicate_v1_local_identity() {
        let runtime = Arc::new(QuestionRuntime::new());
        install_test_durability_callback(&runtime);
        let first_runtime = Arc::clone(&runtime);
        let first_task = tokio::spawn(async move {
            first_runtime
                .ask(
                    Uuid::nil(),
                    test_request_stream_id(),
                    ApiCallId::try_new(1).expect("test API id"),
                    ToolCallId::try_new(2).expect("test tool id"),
                    test_controller_tool_call_id(),
                    Some(String::from("provider-ask-duplicate")),
                    String::from("ask"),
                    test_tool_arguments(),
                    vec![QuestionPrompt {
                        question: String::from("First question"),
                        header: String::from("First"),
                        options: Vec::new(),
                        multiple: Some(false),
                    }],
                )
                .await
        });
        tokio::task::yield_now().await;

        let duplicate_error = runtime
            .ask(
                Uuid::nil(),
                test_request_stream_id(),
                ApiCallId::try_new(1).expect("test API id"),
                ToolCallId::try_new(2).expect("test tool id"),
                test_controller_tool_call_id(),
                Some(String::from("provider-ask-duplicate")),
                String::from("ask"),
                test_tool_arguments(),
                vec![QuestionPrompt {
                    question: String::from("Second question"),
                    header: String::from("Second"),
                    options: Vec::new(),
                    multiple: Some(false),
                }],
            )
            .await
            .expect_err("duplicate V1 local identity must be rejected");
        assert!(duplicate_error.contains("already waiting"));

        let pending = runtime.list_pending().expect("pending question list");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].questions[0].question, "First question");
        runtime
            .reject(&pending[0].id)
            .expect("first ask should remain addressable");
        let first_error = first_task
            .await
            .expect("first ask task should join")
            .expect_err("rejected first ask should stop");
        assert!(first_error.contains("abandoned"));
    }

    /// Verifies callback-registry poisoning rejects ask before a hidden pending request is installed.
    /// 验证回调注册表中毒时会在安装隐藏 pending 请求前拒绝 ask。
    #[tokio::test]
    async fn question_runtime_callback_lock_failure_rejects_ask_before_registration() {
        // runtime owns the callback registry deliberately poisoned before ask registration.
        // runtime 持有在 ask 注册前刻意中毒的回调注册表。
        let runtime = Arc::new(QuestionRuntime::new());
        // poisonResult proves the host notification capability cannot be acquired.
        // poisonResult 证明无法取得宿主通知能力。
        let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // guard keeps the callback write lock held until the deliberate panic poisons it.
            // guard 将回调写锁保持到刻意 panic，使其进入中毒状态。
            let _guard = runtime
                .change_callback
                .write()
                .expect("question callback registry should lock before poisoning");
            panic!("poison question callback registry before ask registration");
        }));
        assert!(poison_result.is_err());
        // error is returned before the pending registry can receive the request.
        // error 会在 pending 注册表接收请求前返回。
        let error = runtime
            .ask(
                Uuid::nil(),
                test_request_stream_id(),
                ApiCallId::try_new(1).expect("test API id"),
                ToolCallId::try_new(2).expect("test tool id"),
                test_controller_tool_call_id(),
                Some(String::from("provider-ask-poisoned")),
                String::from("ask"),
                test_tool_arguments(),
                vec![QuestionPrompt {
                    question: String::from("Choose one path"),
                    header: String::from("Path"),
                    options: Vec::new(),
                    multiple: Some(false),
                }],
            )
            .await
            .expect_err("poisoned callback registry should reject ask");

        assert!(error.contains("question runtime callback registry is locked"));
        assert!(
            runtime
                .list_pending()
                .expect("pending registry should remain readable")
                .is_empty()
        );
    }

    /// Verifies callback preflight failure leaves an existing pending request available for retry.
    /// 验证回调前置检查失败时会保留现有 pending 请求供重试。
    #[tokio::test]
    async fn question_runtime_callback_lock_failure_preserves_pending_reply() {
        // runtime owns the pending request and callback registry used by the race test.
        // runtime 持有竞态测试使用的 pending 请求与回调注册表。
        let runtime = Arc::new(QuestionRuntime::new());
        install_test_durability_callback(&runtime);
        // askRuntime lets the suspended tool future share the runtime under test.
        // askRuntime 让挂起的工具 future 共享被测运行时。
        let ask_runtime = Arc::clone(&runtime);
        // askTask owns the receiver and its cancellation cleanup guard.
        // askTask 持有接收端及其取消清理守卫。
        let ask_task = tokio::spawn(async move {
            ask_runtime
                .ask(
                    Uuid::nil(),
                    test_request_stream_id(),
                    ApiCallId::try_new(1).expect("test API id"),
                    ToolCallId::try_new(2).expect("test tool id"),
                    test_controller_tool_call_id(),
                    Some(String::from("provider-ask-callback")),
                    String::from("ask"),
                    test_tool_arguments(),
                    vec![QuestionPrompt {
                        question: String::from("Confirm"),
                        header: String::from("Confirm"),
                        options: Vec::new(),
                        multiple: Some(false),
                    }],
                )
                .await
        });
        tokio::task::yield_now().await;
        // pending captures the exact request that must survive callback preflight failure.
        // pending 捕获必须在回调前置检查失败后保留的精确请求。
        let pending = runtime
            .list_pending()
            .expect("pending question should be readable before poisoning");
        assert_eq!(pending.len(), 1);
        // requestId identifies the pending entry used for the failed reply attempt.
        // requestId 标识失败回答尝试使用的 pending 项。
        let request_id = pending[0].id.clone();
        // poisonResult makes callback ownership unavailable after ask registration.
        // poisonResult 在 ask 注册后使回调所有权不可用。
        let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // guard keeps the callback write lock held until the deliberate panic poisons it.
            // guard 将回调写锁保持到刻意 panic，使其进入中毒状态。
            let _guard = runtime
                .change_callback
                .write()
                .expect("question callback registry should lock before poisoning");
            panic!("poison question callback registry before reply preflight");
        }));
        assert!(poison_result.is_err());

        // error proves reply failed before consuming the pending request.
        // error 证明回答在消费 pending 请求前失败。
        let error = runtime
            .reply(&request_id, vec![vec![String::from("yes")]])
            .expect_err("poisoned callback registry should reject reply preflight");
        assert!(error.contains("question runtime callback registry is locked"));
        assert_eq!(
            runtime
                .list_pending()
                .expect("pending request should remain readable")
                .len(),
            1
        );

        ask_task.abort();
        let _ = ask_task.await;
        assert!(
            runtime
                .list_pending()
                .expect("cancelled ask should clean up its pending request")
                .is_empty()
        );
    }

    /// Verifies a panicking projection cannot roll back a durably waiting Ask.
    /// 验证投影回调 panic 不能回滚已经可靠进入等待态的 Ask。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if callback panic escapes, repeats during Drop, or leaves pending state.
    /// 此测试无返回值；若回调 panic 逃逸、在 Drop 中重复发生或遗留 pending 状态则失败。
    #[tokio::test]
    async fn question_runtime_callback_panic_is_contained_without_drop_reentry() {
        // runtime owns the durable Ask and deliberately panicking projection callback.
        // runtime 持有可靠 Ask 与刻意 panic 的投影回调。
        let runtime = Arc::new(QuestionRuntime::new());
        install_test_durability_callback(&runtime);
        // callbackCount proves both projection attempts remain diagnostic-only.
        // callbackCount 证明两次投影尝试都只具有诊断意义。
        let callback_count = Arc::new(AtomicU64::new(0));
        // callbackCounter shares the invocation count with the callback closure.
        // callbackCounter 与回调闭包共享调用计数。
        let callback_counter = Arc::clone(&callback_count);
        runtime
            .set_change_callback(Some(Arc::new(move || {
                callback_counter.fetch_add(1, Ordering::Relaxed);
                panic!("panic from pending-question projection callback");
            })))
            .expect("panicking callback should install before invocation");

        // askTask remains suspended because durable WaitingUser survives projection failure.
        // askTask 保持挂起，因为可靠 WaitingUser 不受投影失败影响。
        let ask_runtime = Arc::clone(&runtime);
        let ask_task = tokio::spawn(async move {
            ask_runtime
                .ask(
                    Uuid::nil(),
                    test_request_stream_id(),
                    ApiCallId::try_new(1).expect("test API id"),
                    ToolCallId::try_new(2).expect("test tool id"),
                    test_controller_tool_call_id(),
                    Some(String::from("provider-ask-panic")),
                    String::from("ask"),
                    test_tool_arguments(),
                    vec![QuestionPrompt {
                        question: String::from("Confirm callback containment"),
                        header: String::from("Contain"),
                        options: Vec::new(),
                        multiple: Some(false),
                    }],
                )
                .await
        });
        tokio::task::yield_now().await;
        let pending = runtime
            .list_pending()
            .expect("durably waiting Ask should remain addressable");
        assert_eq!(pending.len(), 1);
        assert_eq!(callback_count.load(Ordering::Relaxed), 1);

        assert!(
            runtime
                .reject(&pending[0].id)
                .expect("durable rejection should ignore projection panic")
        );
        let error = ask_task
            .await
            .expect("Ask task should join")
            .expect_err("rejected Ask should stop");
        assert!(error.contains("abandoned"));
        assert_eq!(callback_count.load(Ordering::Relaxed), 2);
        assert!(
            runtime
                .list_pending()
                .expect("pending registry should remain readable after durable rejection")
                .is_empty()
        );
    }

    /// Verifies an explicit projection error cannot erase a durably waiting Ask.
    /// 验证明确的投影错误不能抹除已经可靠进入等待态的 Ask。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails if the callback error is hidden, repeated, or leaves pending state.
    /// 此测试无返回值；若回调错误被隐藏、重复发生或遗留 pending 状态则失败。
    #[tokio::test]
    async fn question_runtime_callback_error_rejects_unpublished_registration_once() {
        // runtime owns the durable Ask and deliberately failing projection callback.
        // runtime 持有可靠 Ask 与刻意失败的投影回调。
        let runtime = Arc::new(QuestionRuntime::new());
        install_test_durability_callback(&runtime);
        // callbackCount proves both projection attempts remain diagnostic-only.
        // callbackCount 证明两次投影尝试都只具有诊断意义。
        let callback_count = Arc::new(AtomicU64::new(0));
        // callbackCounter shares the invocation count with the callback closure.
        // callbackCounter 与回调闭包共享调用计数。
        let callback_counter = Arc::clone(&callback_count);
        runtime
            .set_change_callback(Some(Arc::new(move || {
                callback_counter.fetch_add(1, Ordering::Relaxed);
                Err(String::from("projection unavailable"))
            })))
            .expect("failing callback should install before invocation");

        // askTask remains suspended because durable WaitingUser survives projection failure.
        // askTask 保持挂起，因为可靠 WaitingUser 不受投影失败影响。
        let ask_runtime = Arc::clone(&runtime);
        let ask_task = tokio::spawn(async move {
            ask_runtime
                .ask(
                    Uuid::nil(),
                    test_request_stream_id(),
                    ApiCallId::try_new(1).expect("test API id"),
                    ToolCallId::try_new(2).expect("test tool id"),
                    test_controller_tool_call_id(),
                    Some(String::from("provider-ask-error")),
                    String::from("ask"),
                    test_tool_arguments(),
                    vec![QuestionPrompt {
                        question: String::from("Confirm explicit error propagation"),
                        header: String::from("Confirm"),
                        options: Vec::new(),
                        multiple: Some(false),
                    }],
                )
                .await
        });
        tokio::task::yield_now().await;
        let pending = runtime
            .list_pending()
            .expect("durably waiting Ask should remain addressable");
        assert_eq!(pending.len(), 1);
        assert_eq!(callback_count.load(Ordering::Relaxed), 1);

        assert!(
            runtime
                .reject(&pending[0].id)
                .expect("durable rejection should ignore projection error")
        );
        let error = ask_task
            .await
            .expect("Ask task should join")
            .expect_err("rejected Ask should stop");
        assert!(error.contains("abandoned"));
        assert_eq!(callback_count.load(Ordering::Relaxed), 2);
        assert!(
            runtime
                .list_pending()
                .expect("pending registry should remain readable after durable rejection")
                .is_empty()
        );
    }

    /// Verifies committed replies and rejections remain successful when removal notification panics.
    /// 验证移除通知 panic 时，已提交的回答与拒绝仍保持成功结果。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if a post-commit notification panic creates a false operation failure.
    /// 此测试无返回值；若提交后的通知 panic 制造操作假失败则测试失败。
    #[tokio::test]
    async fn question_runtime_committed_completions_survive_notification_panic() {
        // runtime owns two pending completions and the deliberately panicking projection callback.
        // runtime 持有两条 pending 完成项与刻意 panic 的投影回调。
        let runtime = QuestionRuntime::new();
        install_test_durability_callback(&runtime);
        // callbackCount proves both committed removals attempted projection notification.
        // callbackCount 证明两次已提交移除均尝试了投影通知。
        let callback_count = Arc::new(AtomicU64::new(0));
        // callbackCounter shares the invocation count with the callback closure.
        // callbackCounter 与回调闭包共享调用计数。
        let callback_counter = Arc::clone(&callback_count);
        runtime
            .set_change_callback(Some(Arc::new(move || {
                callback_counter.fetch_add(1, Ordering::Relaxed);
                panic!("panic from committed pending-question removal callback");
            })))
            .expect("panicking completion callback should install");
        // replyRequestId identifies the answer completion expected to commit successfully.
        // replyRequestId 标识预期成功提交的回答完成项。
        let reply_request_id = String::from("question-committed-reply");
        // rejectRequestId identifies the rejection completion expected to commit successfully.
        // rejectRequestId 标识预期成功提交的拒绝完成项。
        let reject_request_id = String::from("question-committed-reject");
        // replyChannel observes the exact answer delivered before notification fails.
        // replyChannel 观察通知失败前已投递的精确答案。
        let (reply_sender, reply_receiver) = oneshot::channel();
        // rejectChannel observes the exact rejection delivered before notification fails.
        // rejectChannel 观察通知失败前已投递的精确拒绝。
        let (reject_sender, reject_receiver) = oneshot::channel();
        {
            // pending installs the two exact entries consumed by reply and reject.
            // pending 安装由回答与拒绝消费的两条精确项。
            let mut pending = runtime
                .pending
                .write()
                .expect("pending question registry should be writable");
            pending.insert(
                reply_request_id.clone(),
                PendingQuestionEntry {
                    request: QuestionRequest {
                        id: reply_request_id.clone(),
                        session_id: Uuid::nil(),
                        request_stream_id: test_request_stream_id(),
                        api_call_id: ApiCallId::try_new(1).expect("test API id"),
                        tool_call_id: ToolCallId::try_new(2).expect("test tool id"),
                        controller_tool_call_id: test_controller_tool_call_id(),
                        provider_tool_call_id: None,
                        tool_name: String::from("ask"),
                        tool_arguments: test_tool_arguments(),
                        sequence: 1,
                        questions: Vec::new(),
                        approval: None,
                    },
                    sender: reply_sender,
                },
            );
            pending.insert(
                reject_request_id.clone(),
                PendingQuestionEntry {
                    request: QuestionRequest {
                        id: reject_request_id.clone(),
                        session_id: Uuid::nil(),
                        request_stream_id: test_request_stream_id(),
                        api_call_id: ApiCallId::try_new(3).expect("test API id"),
                        tool_call_id: ToolCallId::try_new(4).expect("test tool id"),
                        controller_tool_call_id: String::from("controller-ask-tool-call-2"),
                        provider_tool_call_id: None,
                        tool_name: String::from("ask"),
                        tool_arguments: test_tool_arguments(),
                        sequence: 2,
                        questions: Vec::new(),
                        approval: None,
                    },
                    sender: reject_sender,
                },
            );
        }

        assert!(
            runtime
                .reply(&reply_request_id, vec![vec![String::from("confirmed")]])
                .expect("committed reply should ignore projection panic")
        );
        assert!(
            runtime
                .reject(&reject_request_id)
                .expect("committed rejection should ignore projection panic")
        );
        // replyCompletion is the concrete answer received by the waiting tool invocation.
        // replyCompletion 是等待中的工具调用收到的具体答案。
        let reply_completion = reply_receiver
            .await
            .expect("committed reply receiver should remain connected");
        // rejectCompletion is the concrete rejection received by the waiting tool invocation.
        // rejectCompletion 是等待中的工具调用收到的具体拒绝。
        let reject_completion = reject_receiver
            .await
            .expect("committed rejection receiver should remain connected");
        // reply contains the structured answers extracted from the committed completion.
        // reply 包含从已提交完成项中提取的结构化答案。
        let PendingQuestionCompletion::Reply(reply) = reply_completion else {
            panic!("reply completion should contain structured answers");
        };
        assert_eq!(reply.answers, vec![vec![String::from("confirmed")]]);
        assert!(matches!(
            reject_completion,
            PendingQuestionCompletion::Reject
        ));
        assert_eq!(callback_count.load(Ordering::Relaxed), 2);
        assert!(
            runtime
                .list_pending()
                .expect("pending registry should remain readable after completion callbacks")
                .is_empty()
        );
    }

    /// Verifies durable answers and rejections survive vanished process-local receivers.
    /// 验证可靠回答与拒绝可以在进程内接收端消失后保持成功。
    #[test]
    fn question_runtime_completion_reports_disconnected_receivers() {
        // runtime owns two synthetic pending entries whose receivers are deliberately dropped.
        // runtime 持有两条接收端被刻意丢弃的合成 pending 项。
        let runtime = QuestionRuntime::new();
        install_test_durability_callback(&runtime);
        // replyRequestId identifies the disconnected answer-delivery entry.
        // replyRequestId 标识断开连接的答案投递项。
        let reply_request_id = String::from("question-disconnected-reply");
        // rejectRequestId identifies the disconnected rejection-delivery entry.
        // rejectRequestId 标识断开连接的拒绝投递项。
        let reject_request_id = String::from("question-disconnected-reject");
        // replySender is retained in the registry after its paired receiver is dropped.
        // replySender 在配对接收端被丢弃后保留于注册表。
        let (reply_sender, reply_receiver) = oneshot::channel();
        drop(reply_receiver);
        // rejectSender is retained in the registry after its paired receiver is dropped.
        // rejectSender 在配对接收端被丢弃后保留于注册表。
        let (reject_sender, reject_receiver) = oneshot::channel();
        drop(reject_receiver);
        {
            // pending installs the exact disconnected entries consumed by reply and reject.
            // pending 安装由回答与拒绝消费的精确断开连接项。
            let mut pending = runtime
                .pending
                .write()
                .expect("pending question registry should be writable");
            pending.insert(
                reply_request_id.clone(),
                PendingQuestionEntry {
                    request: QuestionRequest {
                        id: reply_request_id.clone(),
                        session_id: Uuid::nil(),
                        request_stream_id: test_request_stream_id(),
                        api_call_id: ApiCallId::try_new(1).expect("test API id"),
                        tool_call_id: ToolCallId::try_new(2).expect("test tool id"),
                        controller_tool_call_id: test_controller_tool_call_id(),
                        provider_tool_call_id: None,
                        tool_name: String::from("ask"),
                        tool_arguments: test_tool_arguments(),
                        sequence: 1,
                        questions: Vec::new(),
                        approval: None,
                    },
                    sender: reply_sender,
                },
            );
            pending.insert(
                reject_request_id.clone(),
                PendingQuestionEntry {
                    request: QuestionRequest {
                        id: reject_request_id.clone(),
                        session_id: Uuid::nil(),
                        request_stream_id: test_request_stream_id(),
                        api_call_id: ApiCallId::try_new(3).expect("test API id"),
                        tool_call_id: ToolCallId::try_new(4).expect("test tool id"),
                        controller_tool_call_id: String::from("controller-ask-tool-call-2"),
                        provider_tool_call_id: None,
                        tool_name: String::from("ask"),
                        tool_arguments: test_tool_arguments(),
                        sequence: 2,
                        questions: Vec::new(),
                        approval: None,
                    },
                    sender: reject_sender,
                },
            );
        }

        assert!(
            runtime
                .reply(&reply_request_id, Vec::new())
                .expect("durable answer should ignore a vanished process receiver")
        );
        assert!(
            runtime
                .reject(&reject_request_id)
                .expect("durable rejection should ignore a vanished process receiver")
        );
        assert!(
            runtime
                .list_pending()
                .expect("completed disconnected entries should be removed")
                .is_empty()
        );
    }

    /// Verifies cancellation cleanup uses the callback captured when the request was registered.
    /// 验证取消清理会使用请求注册时捕获的回调。
    #[tokio::test]
    async fn question_runtime_cancellation_uses_registered_callback_snapshot() {
        // runtime owns the pending request and the stable callback installed before registration.
        // runtime 持有 pending 请求及注册前安装的稳定回调。
        let runtime = Arc::new(QuestionRuntime::new());
        install_test_durability_callback(&runtime);
        // callbackCount records registration and cancellation notifications.
        // callbackCount 记录注册与取消通知次数。
        let callback_count = Arc::new(AtomicU64::new(0));
        // callbackCounter shares the notification counter with the callback closure.
        // callbackCounter 与回调闭包共享通知计数器。
        let callback_counter = Arc::clone(&callback_count);
        runtime
            .set_change_callback(Some(Arc::new(move || {
                callback_counter.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })))
            .expect("question callback should install");
        // askRuntime lets the cancellable tool future share the runtime under test.
        // askRuntime 让可取消工具 future 共享被测运行时。
        let ask_runtime = Arc::clone(&runtime);
        // askTask owns the pending guard whose Drop path performs cancellation cleanup.
        // askTask 持有通过 Drop 路径执行取消清理的 pending 守卫。
        let ask_task = tokio::spawn(async move {
            ask_runtime
                .ask(
                    Uuid::nil(),
                    test_request_stream_id(),
                    ApiCallId::try_new(1).expect("test API id"),
                    ToolCallId::try_new(2).expect("test tool id"),
                    test_controller_tool_call_id(),
                    Some(String::from("provider-ask-cancel")),
                    String::from("ask"),
                    test_tool_arguments(),
                    vec![QuestionPrompt {
                        question: String::from("Wait for cancellation"),
                        header: String::from("Cancel"),
                        options: Vec::new(),
                        multiple: Some(false),
                    }],
                )
                .await
        });
        tokio::task::yield_now().await;
        assert_eq!(
            runtime
                .list_pending()
                .expect("registered request should be visible")
                .len(),
            1
        );

        ask_task.abort();
        let _ = ask_task.await;

        assert!(
            runtime
                .list_pending()
                .expect("cancelled request should be removed")
                .is_empty()
        );
        assert_eq!(callback_count.load(Ordering::Relaxed), 2);
    }

    /// Verifies the prompt validator rejects duplicate option labels so the WebUI never has to disambiguate ambiguous selections.
    /// 验证问题校验器会拒绝重复选项标签，从而避免WebUI处理歧义选择。
    #[test]
    fn validate_question_prompts_rejects_duplicate_option_labels() {
        let prompts = vec![QuestionPrompt {
            question: String::from("Pick"),
            header: String::from("Pick"),
            options: vec![
                QuestionOption {
                    label: String::from("A"),
                    description: String::from("First"),
                },
                QuestionOption {
                    label: String::from("A"),
                    description: String::from("Second"),
                },
            ],
            multiple: Some(false),
        }];
        let error = validate_question_prompts(&prompts).expect_err("duplicate label should fail");
        assert!(error.contains("duplicate option label"));
    }
}
