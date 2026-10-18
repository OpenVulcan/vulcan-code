//! Permission approvals carried by the shared question runtime.
//! 由共享问题运行时承载的权限审批。
//!
//! An approval reuses the Ask lifecycle (durable WaitingUser, refresh retention, rejection), but a
//! reply never completes the tool: it returns the tool to Running and the gate either runs the body
//! or returns the denial. The prompt text is a default-language fallback; clients render the
//! structured permission facts in their own language.
//! 审批复用 Ask 生命周期（可靠 WaitingUser、刷新保留、拒绝），但回复从不完成工具：它把工具恢复为
//! Running，闸门随后执行主体或返回拒绝结果。提示文本只是默认语言后备；客户端以自身语言渲染结构化
//! 权限事实。

use super::*;
use crate::agent_runtime::tool_permission::{
    HighRiskCategory, JudgeFinding, PermissionSubject, ReviewFinding,
};
use vulcan_session_core::v1::config::PermissionMode;

/// Canonical label of the option that runs the gated call once.
/// 执行一次受管控调用的选项的规范标签。
pub const APPROVAL_ALLOW_LABEL: &str = "Allow once";

/// Canonical label of the option that refuses the gated call.
/// 拒绝受管控调用的选项的规范标签。
pub const APPROVAL_DENY_LABEL: &str = "Deny";

/// Permission facts of one pending approval, projected to clients for rendering.
/// 一条待处理审批的权限事实，投影给客户端渲染。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolApprovalPrompt {
    /// Session permission mode that escalated the call.
    /// 使调用升级的会话权限模式。
    pub mode: PermissionMode,
    /// What the call would affect.
    /// 调用会影响的对象。
    pub subject: PermissionSubject,
    /// Matched high-risk category, when any.
    /// 命中的高风险类别（如有）。
    pub high_risk: Option<HighRiskCategory>,
    /// Judge layer finding, when a judge is configured for the call.
    /// 判定层结论（当调用配置了判定器时）。
    pub judge: Option<JudgeFinding>,
    /// Review layer finding, when a reviewing LLM is configured for the call.
    /// 复核层结论（当调用配置了复核 LLM 时）。
    pub review: Option<ReviewFinding>,
    /// When the approval is denied unanswered, in Unix milliseconds; absent waits for the user.
    /// Set by the question runtime from the approval wait.
    /// 审批未获回答而被拒绝的时间（Unix 毫秒）；缺省表示一直等待用户。由问题运行时按审批等待时间设置。
    pub deadline_unix_ms: Option<u64>,
}

/// How one approval ended.
/// 一次审批的结束方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalAnswer {
    /// The user allowed the call once.
    /// 用户允许本次调用。
    Allowed,
    /// The user denied the call.
    /// 用户拒绝该调用。
    Denied,
    /// Nobody answered before the approval wait ran out, so the call was denied.
    /// 审批等待时间耗尽前无人回答，因此调用被拒绝。
    Expired,
}

impl QuestionRuntime {
    /// Asks the user whether one gated call may run once. With a `timeout`, an unanswered approval
    /// is denied through the same durable reply a user's denial takes, and reported as expired.
    /// 询问用户受管控调用是否可以执行一次。给出 `timeout` 时，未获回答的审批经由与用户拒绝相同的
    /// 可靠回复被拒绝，并报告为已过期。
    /// `tool_name` and `tool_arguments` describe the gated call itself, so a rejection cancels it.
    /// `tool_name` 与 `tool_arguments` 描述受管控调用本身，因此拒绝时会取消该调用。
    pub async fn request_approval(
        self: &Arc<Self>,
        session_id: SessionId,
        context: VulcanToolInvocationContext,
        tool_name: String,
        tool_arguments: Value,
        mut prompt: ToolApprovalPrompt,
        timeout: Option<Duration>,
    ) -> Result<ApprovalAnswer, String> {
        prompt.deadline_unix_ms = timeout.map(unix_ms_after);
        let question = approval_question(&tool_name, &prompt);
        let request_id = pending_request_id(session_id, context.api_call_id, context.tool_call_id);
        let waiting = self.suspend(
            session_id,
            context.request_stream_id,
            context.api_call_id,
            context.tool_call_id,
            context.controller_tool_call_id,
            context.provider_tool_call_id,
            tool_name,
            tool_arguments,
            vec![question],
            Some(prompt),
        );
        tokio::pin!(waiting);
        let mut expired = false;
        let reply = match timeout {
            None => waiting.await?,
            Some(timeout) => tokio::select! {
                // The wait registers the approval on its first poll, before the timer can fire.
                // 等待在首次轮询时登记审批，早于计时器触发。
                biased;
                reply = &mut waiting => reply?,
                () = tokio::time::sleep(timeout) => {
                    // A user answer that won the race leaves nothing to deny.
                    // 抢先到达的用户回答使这里无可拒绝。
                    expired = self.reply(&request_id, vec![vec![String::from(APPROVAL_DENY_LABEL)]])?;
                    waiting.await?
                }
            },
        };
        validate_approval_answers(&reply.answers)?;
        Ok(if reply.answers[0][0] == APPROVAL_ALLOW_LABEL {
            ApprovalAnswer::Allowed
        } else if expired {
            ApprovalAnswer::Expired
        } else {
            ApprovalAnswer::Denied
        })
    }
}

/// Absolute Unix milliseconds `after` from now.
/// 从现在起 `after` 之后的绝对 Unix 毫秒数。
fn unix_ms_after(after: Duration) -> u64 {
    std::time::SystemTime::now()
        .checked_add(after)
        .and_then(|deadline| deadline.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |deadline| deadline.as_millis() as u64)
}

/// Accepts exactly one canonical approval label; anything else leaves the request pending.
/// 只接受恰好一个规范审批标签；其他回复会让请求保持待处理。
pub(super) fn validate_approval_answers(answers: &[Vec<String>]) -> Result<(), String> {
    match answers {
        [choice] if matches!(choice.as_slice(), [label] if label == APPROVAL_ALLOW_LABEL || label == APPROVAL_DENY_LABEL) => {
            Ok(())
        }
        _ => Err(format!(
            "A permission approval must be answered with exactly `{APPROVAL_ALLOW_LABEL}` or `{APPROVAL_DENY_LABEL}`."
        )),
    }
}

/// Builds the default-language prompt shown by clients that cannot render approvals.
/// 构建无法渲染审批的客户端所展示的默认语言提示。
fn approval_question(tool_name: &str, prompt: &ToolApprovalPrompt) -> QuestionPrompt {
    let mut question = match &prompt.subject {
        PermissionSubject::Command { text } if text.trim().is_empty() => {
            format!("Allow `{tool_name}` to start an interactive shell?")
        }
        PermissionSubject::Command { text } => {
            format!("Allow `{tool_name}` to run this command?\n\n{text}")
        }
        PermissionSubject::FileWrite { paths } => {
            format!(
                "Allow `{tool_name}` to write these paths?\n\n{}",
                paths.join("\n")
            )
        }
        PermissionSubject::Opaque | PermissionSubject::Ungoverned => {
            format!("Allow `{tool_name}` to run? It is not known to be read-only.")
        }
    };
    if let Some(category) = prompt.high_risk {
        question.push_str(&format!("\n\nHigh risk: {}", category.key()));
    }
    match &prompt.judge {
        Some(JudgeFinding::Scored(label)) => {
            question.push_str(&format!("\n\nPermission judge: {}", label.key()));
        }
        Some(JudgeFinding::Unavailable(reason)) => {
            question.push_str(&format!("\n\nPermission judge unavailable: {reason}"));
        }
        None => {}
    }
    match &prompt.review {
        Some(ReviewFinding::Reviewed(review)) => {
            question.push_str(&format!(
                "\n\nPermission reviewer ({}): {}",
                review.verdict.key(),
                review.reason
            ));
        }
        Some(ReviewFinding::Unavailable(reason)) => {
            question.push_str(&format!("\n\nPermission reviewer unavailable: {reason}"));
        }
        None => {}
    }
    QuestionPrompt {
        question,
        header: String::from("Permission"),
        options: vec![
            QuestionOption {
                label: String::from(APPROVAL_ALLOW_LABEL),
                description: String::from("Run this call once."),
            },
            QuestionOption {
                label: String::from(APPROVAL_DENY_LABEL),
                description: String::from("Do not run it; the agent is told it was denied."),
            },
        ],
        multiple: Some(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::tool_permission::JudgeLabel;
    use std::sync::Mutex;
    use vulcan_session_core::v1::ids::Seq;

    /// Exact coordinates of one synthetic gated call.
    /// 一次合成受管控调用的精确坐标。
    fn context() -> VulcanToolInvocationContext {
        VulcanToolInvocationContext {
            turn_id: vulcan_session_core::v1::ids::TurnId::try_new(1).expect("synthetic turn"),
            session_id: Uuid::nil().to_string(),
            request_stream_id: RequestStreamId::try_new("approval-stream").unwrap(),
            api_call_id: ApiCallId::try_new(1).unwrap(),
            tool_call_id: ToolCallId::try_new(2).unwrap(),
            tool_seq: Seq::try_new(3).unwrap(),
            controller_tool_call_id: String::from("approval-call"),
            provider_tool_call_id: None,
        }
    }

    /// Runtime, recorded durability action names, and the waiting approval task.
    /// 运行时、已记录的可靠性动作名称以及等待中的审批任务。
    type ApprovalFixture = (
        Arc<QuestionRuntime>,
        Arc<Mutex<Vec<&'static str>>>,
        tokio::task::JoinHandle<Result<ApprovalAnswer, String>>,
    );

    /// Starts one approval for a shell command that waits for the user.
    /// 为一条 shell 命令发起一直等待用户的审批。
    fn start_approval() -> ApprovalFixture {
        start_timed_approval(None)
    }

    /// Starts one approval for a shell command and records every durability action by name.
    /// 为一条 shell 命令发起审批，并按名称记录每个可靠性动作。
    fn start_timed_approval(timeout: Option<Duration>) -> ApprovalFixture {
        let runtime = Arc::new(QuestionRuntime::new());
        let actions = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&actions);
        runtime
            .set_durability_callback(Some(Arc::new(move |action| {
                recorded.lock().unwrap().push(match action {
                    PendingQuestionDurabilityAction::Waiting { .. } => "waiting",
                    PendingQuestionDurabilityAction::Answered { .. } => "answered",
                    PendingQuestionDurabilityAction::Resumed { .. } => "resumed",
                    PendingQuestionDurabilityAction::Cancelled { .. } => "cancelled",
                });
                Ok(())
            })))
            .unwrap();
        let approving = Arc::clone(&runtime);
        let task = tokio::spawn(async move {
            approving
                .request_approval(
                    Uuid::nil(),
                    context(),
                    String::from("shell_command"),
                    json!({"command": "rm -rf build"}),
                    ToolApprovalPrompt {
                        mode: PermissionMode::AutoApprove,
                        subject: PermissionSubject::Command {
                            text: String::from("rm -rf build"),
                        },
                        high_risk: Some(HighRiskCategory::RecursiveDelete),
                        judge: Some(JudgeFinding::Scored(JudgeLabel::Block)),
                        review: None,
                        deadline_unix_ms: None,
                    },
                    timeout,
                )
                .await
        });
        (runtime, actions, task)
    }

    /// Waits until the approval is registered and returns its request.
    /// 等待审批注册完成并返回其请求。
    async fn pending(runtime: &QuestionRuntime) -> QuestionRequest {
        loop {
            if let Some(request) = runtime.list_pending().unwrap().into_iter().next() {
                return request;
            }
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn approval_answers_resume_the_tool_instead_of_completing_it() {
        for (label, approved) in [
            (APPROVAL_ALLOW_LABEL, ApprovalAnswer::Allowed),
            (APPROVAL_DENY_LABEL, ApprovalAnswer::Denied),
        ] {
            let (runtime, actions, task) = start_approval();
            let request = pending(&runtime).await;
            assert_eq!(request.tool_name, "shell_command");
            assert_eq!(
                request
                    .approval
                    .as_ref()
                    .map(|prompt| prompt.deadline_unix_ms),
                Some(None)
            );
            assert!(
                runtime
                    .reply(&request.id, vec![vec![String::from("Maybe")]])
                    .is_err(),
                "a non-canonical answer must be refused"
            );
            assert_eq!(runtime.list_pending().unwrap().len(), 1);
            assert!(
                runtime
                    .reply(&request.id, vec![vec![String::from(label)]])
                    .unwrap()
            );
            assert_eq!(task.await.unwrap(), Ok(approved));
            assert_eq!(*actions.lock().unwrap(), vec!["waiting", "resumed"]);
        }
    }

    /// An unanswered approval is denied durably when its wait runs out, and the projected deadline
    /// tells clients when that happens.
    /// 未获回答的审批在等待时间耗尽时被可靠地拒绝，投影的截止时间告知客户端何时发生。
    #[tokio::test]
    async fn an_unanswered_approval_expires_into_a_durable_denial() {
        let (runtime, actions, task) = start_timed_approval(Some(Duration::from_millis(30)));
        let request = pending(&runtime).await;
        assert!(
            request
                .approval
                .as_ref()
                .and_then(|prompt| prompt.deadline_unix_ms)
                .is_some()
        );
        assert_eq!(task.await.unwrap(), Ok(ApprovalAnswer::Expired));
        assert!(runtime.list_pending().unwrap().is_empty());
        assert_eq!(*actions.lock().unwrap(), vec!["waiting", "resumed"]);
    }

    /// An answer given within the wait is the user's, not an expiry.
    /// 在等待时间内给出的回答属于用户，而非过期。
    #[tokio::test]
    async fn an_answer_within_the_wait_is_the_users() {
        let (runtime, _, task) = start_timed_approval(Some(Duration::from_secs(60)));
        let request = pending(&runtime).await;
        assert!(
            runtime
                .reply(&request.id, vec![vec![String::from(APPROVAL_DENY_LABEL)]])
                .unwrap()
        );
        assert_eq!(task.await.unwrap(), Ok(ApprovalAnswer::Denied));
    }

    #[tokio::test]
    async fn abandoning_an_approval_cancels_the_call() {
        let (runtime, actions, task) = start_approval();
        let request = pending(&runtime).await;
        assert!(runtime.reject(&request.id).unwrap());
        assert!(task.await.unwrap().is_err());
        assert_eq!(*actions.lock().unwrap(), vec!["waiting", "cancelled"]);
    }
}
