//! Host-managed question runtime and model-visible ask tool surface.
//! 宿主管理的问题运行时与模型可见 ask 工具表面。

use crate::model::SessionId;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::oneshot;
use uuid::Uuid;
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
    VulcanToolInvocationResult, VulcanToolSourceKind,
};

/// Callback invoked whenever pending-question visibility changes.
/// 待回答问题可见性变化时触发的回调。
pub type PendingQuestionChangeCallback = Arc<dyn Fn() + Send + Sync + 'static>;

/// Stable host-result kind emitted by the ask tool so transcript and future WebUI enrichments can identify structured question replies.
/// ask 工具输出的稳定宿主结果类型，供转录与后续WebUI增强识别结构化问答回复。
const QUESTION_TOOL_HOST_RESULT_KIND: &str = "ask";

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
    /// Monotonic sequence used to present older pending questions before newer ones.
    /// 用于让较早的问题请求先于较新的请求展示的单调序号。
    pub sequence: u64,
    /// Ordered structured prompts that still need user answers.
    /// 仍然需要用户作答的有序结构化提示集合。
    pub questions: Vec<QuestionPrompt>,
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
}

impl Drop for PendingQuestionGuard {
    /// Cleans up the pending request eagerly when the suspended tool call goes away unexpectedly.
    /// 当挂起中的工具调用异常结束时，主动清理对应的待回答请求。
    fn drop(&mut self) {
        self.runtime.remove_pending_request(&self.request_id);
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
    /// Monotonic sequence allocator used to preserve display order for concurrently pending questions.
    /// 用于在并发待回答问题之间保持展示顺序的单调序号分配器。
    next_sequence: AtomicU64,
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

    /// Suspends the current tool invocation, registers the pending structured questions, and waits until the WebUI replies or rejects them.
    /// 挂起当前工具调用、注册待回答结构化问题，并等待WebUI回复或拒绝。
    pub async fn ask(
        self: &Arc<Self>,
        session_id: SessionId,
        questions: Vec<QuestionPrompt>,
    ) -> Result<QuestionReply, String> {
        validate_question_prompts(&questions)?;
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        let request_id = format!("question-{}", Uuid::new_v4());
        let request = QuestionRequest {
            id: request_id.clone(),
            session_id,
            sequence,
            questions,
        };
        let (sender, receiver) = oneshot::channel::<PendingQuestionCompletion>();
        self.pending
            .write()
            .map_err(|_| String::from("The question runtime registry is locked and cannot register pending questions."))?
            .insert(request_id.clone(), PendingQuestionEntry { request, sender });
        self.notify_pending_question_changed();
        let _pending_guard = PendingQuestionGuard {
            runtime: Arc::clone(self),
            request_id,
        };
        match receiver.await {
            Ok(PendingQuestionCompletion::Reply(reply)) => Ok(reply),
            Ok(PendingQuestionCompletion::Reject) => Err(String::from(
                "The user abandoned the current questionnaire request. The current conversation run has been interrupted.",
            )),
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
    pub fn reply(&self, request_id: &str, answers: Vec<Vec<String>>) -> Result<bool, String> {
        let entry = self
            .pending
            .write()
            .map_err(|_| {
                String::from("The question runtime registry is locked and cannot submit answers.")
            })?
            .remove(request_id);
        let Some(entry) = entry else {
            return Ok(false);
        };
        self.notify_pending_question_changed();
        let _ = entry
            .sender
            .send(PendingQuestionCompletion::Reply(QuestionReply { answers }));
        Ok(true)
    }

    /// Rejects one pending request without answers and resumes the blocked tool invocation with a cancellation error when the request still exists.
    /// 在请求仍存在时无答案地拒绝一条待回答请求，并以取消错误恢复被阻塞的工具调用。
    pub fn reject(&self, request_id: &str) -> Result<bool, String> {
        let entry = self
            .pending
            .write()
            .map_err(|_| String::from("The question runtime registry is locked and cannot cancel the pending question."))?
            .remove(request_id);
        let Some(entry) = entry else {
            return Ok(false);
        };
        self.notify_pending_question_changed();
        let _ = entry.sender.send(PendingQuestionCompletion::Reject);
        Ok(true)
    }

    /// Executes one model-visible ask tool call for the current controller session.
    /// 为当前控制器会话执行一次模型可见 ask 工具调用。
    pub async fn invoke_tool(
        self: &Arc<Self>,
        current_session_id: SessionId,
        args: Value,
        tool_name: &str,
    ) -> Result<VulcanToolInvocationResult, String> {
        let parsed_args = parse_question_tool_arguments(args)?;
        let reply = self
            .ask(current_session_id, parsed_args.questions.clone())
            .await?;
        Ok(build_question_invocation_result(
            tool_name,
            &parsed_args.questions,
            &reply,
        ))
    }

    /// Removes one pending request from the registry when it still exists.
    /// 当一条待回答请求仍存在时，将其从注册表中移除。
    fn remove_pending_request(&self, request_id: &str) {
        if let Ok(mut pending) = self.pending.write()
            && pending.remove(request_id).is_some()
        {
            drop(pending);
            self.notify_pending_question_changed();
        }
    }

    /// Notifies the optional host callback after pending-question state changed.
    /// 待回答问题状态变化后通知可选宿主回调。
    fn notify_pending_question_changed(&self) {
        let callback = self
            .change_callback
            .read()
            .ok()
            .and_then(|guard| guard.clone());
        if let Some(callback) = callback {
            callback();
        }
    }
}

/// Builds one model-visible tool candidate backed by the shared question runtime registry.
/// 根据共享问题运行时注册表构建一条模型可见工具候选项。
pub fn build_question_tool_candidate(
    question_runtime: Arc<QuestionRuntime>,
    public_tool_name: impl Into<String>,
) -> VulcanToolCandidate {
    let tool_name = public_tool_name.into();
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: tool_name.clone(),
            name: tool_name.clone(),
            description: String::from(
                "Ask the user one or more structured questions and wait for the answers before continuing. Use this tool whenever progress depends on a user decision, preference, missing requirement, or explicit confirmation. Prefer this tool over guessing when choices are ambiguous, risky, or approval-sensitive.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: question_tool_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        invoke: Arc::new(move |args, session_id| {
            let question_runtime = Arc::clone(&question_runtime);
            let tool_name = tool_name.clone();
            Box::pin(async move {
                let Some(session_id) = session_id.as_deref() else {
                    return Err(format!(
                        "The `{}` tool requires a session identifier and cannot run without it.",
                        tool_name
                    ));
                };
                let session_id = Uuid::parse_str(session_id).map_err(|error| {
                    format!(
                        "Failed to parse the session identifier for the `{}` tool: {error}",
                        tool_name
                    )
                })?;
                question_runtime
                    .invoke_tool(session_id, args, &tool_name)
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

/// Builds one compact Markdown summary of the final ask answers that will be returned back to the model.
/// 构建一份紧凑 Markdown 形式的 ask 最终答案摘要，并将其回传给模型。
pub fn build_question_reply_markdown(
    questions: &[QuestionPrompt],
    answers: &[Vec<String>],
) -> String {
    let mut lines = vec![String::from("## User Answers"), String::new()];

    for (index, prompt) in questions.iter().enumerate() {
        let rendered_answer = answers
            .get(index)
            .map(|answer| answer.join(", "))
            .filter(|answer| !answer.trim().is_empty())
            .unwrap_or_else(|| String::from("Unanswered"));
        lines.push(format!("- {}: {}", prompt.header, rendered_answer));
    }

    lines.push(String::new());
    lines.push(String::from("Continue with these answers."));
    lines.join("\n")
}

/// Builds a structured display payload for WebUI-only ask presentation without changing the model-facing answer text.
/// 构建仅供WebUI展示使用的 ask 结构化载荷，同时不改变面向模型的答案文本。
fn build_question_display_payload(questions: &[QuestionPrompt], answers: &[Vec<String>]) -> Value {
    let display_questions = questions
        .iter()
        .enumerate()
        .map(|(index, prompt)| {
            let answer_values = answers.get(index).cloned().unwrap_or_default();
            json!({
                "header": prompt.header,
                "question": prompt.question,
                "options": prompt.options.iter().map(|option| json!({
                    "label": option.label,
                    "description": option.description,
                })).collect::<Vec<_>>(),
                "multiple": prompt.allows_multiple(),
                "answers": answer_values,
                "answered": !answer_values.is_empty(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "kind": "ask_question",
        "title": "向用户提问",
        "ignored_answer_text": "用户忽略回答",
        "questions": display_questions,
    })
}

/// Builds the final tool invocation result surfaced back to the model after the user has answered every structured question.
/// 在用户完成全部结构化问题作答后，构建回传给模型的最终工具调用结果。
fn build_question_invocation_result(
    tool_name: &str,
    questions: &[QuestionPrompt],
    reply: &QuestionReply,
) -> VulcanToolInvocationResult {
    let output = build_question_reply_markdown(questions, &reply.answers);
    VulcanToolInvocationResult::from_model_output(Value::String(output)).with_host_result(
        VulcanToolHostResult {
            kind: String::from(QUESTION_TOOL_HOST_RESULT_KIND),
            payload: json!({
                "tool_name": tool_name,
                "headers": questions.iter().map(|question| question.header.clone()).collect::<Vec<_>>(),
                "answers": reply.answers,
                "display": build_question_display_payload(questions, &reply.answers),
            }),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let ask_runtime = Arc::clone(&runtime);
        let ask_task = tokio::spawn(async move {
            ask_runtime
                .ask(
                    Uuid::nil(),
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
