//! Environment value requests carried by the shared question runtime.
//! 由共享问题运行时承载的环境变量取值请求。
//!
//! A value request reuses the Ask lifecycle (durable WaitingUser, refresh retention, rejection),
//! but the values never pass through it: the client stores them through its own service call and
//! then replies with a fixed label, so the reply returns the `env` tool to Running and the tool
//! reads the stored values back. The prompt text is a default-language fallback.
//! 取值请求复用 Ask 生命周期（可靠 WaitingUser、刷新保留、拒绝），但值从不经过它：客户端通过自身的
//! 服务调用存储这些值，再以固定标签回复，回复使 `env` 工具恢复为 Running，工具随后从存储读回这些值。
//! 提示文本只是默认语言后备。

use super::*;
use crate::tooling::native::{EnvironmentValuesAnswer, EnvironmentValuesPrompt};

/// Canonical label replied after the client stored the values the user entered.
/// 客户端存储用户输入的值之后回复的规范标签。
pub const ENVIRONMENT_SAVED_LABEL: &str = "Saved";

/// Canonical label replied when the user declines to provide values.
/// 用户拒绝提供值时回复的规范标签。
pub const ENVIRONMENT_DECLINED_LABEL: &str = "Declined";

impl QuestionRuntime {
    /// Asks the user for environment values and waits until the client stored them or the user
    /// declined. `tool_name` and `tool_arguments` describe the asking `env` call itself.
    /// 向用户索取环境变量值，并等待客户端存储完成或用户拒绝。`tool_name` 与 `tool_arguments`
    /// 描述发起请求的 `env` 调用本身。
    pub async fn request_environment_values(
        self: &Arc<Self>,
        session_id: SessionId,
        context: VulcanToolInvocationContext,
        tool_name: String,
        tool_arguments: Value,
        prompt: EnvironmentValuesPrompt,
    ) -> Result<EnvironmentValuesAnswer, String> {
        if prompt.variables.is_empty() {
            return Err(String::from(
                "An environment value request must name at least one variable.",
            ));
        }
        let question = environment_question(&prompt);
        let reply = self
            .suspend(
                session_id,
                context.request_stream_id,
                context.api_call_id,
                context.tool_call_id,
                context.controller_tool_call_id,
                context.provider_tool_call_id,
                tool_name,
                tool_arguments,
                vec![question],
                PendingRequestKind::Environment(prompt),
            )
            .await?;
        validate_environment_answers(&reply.answers)?;
        Ok(if reply.answers[0][0] == ENVIRONMENT_SAVED_LABEL {
            EnvironmentValuesAnswer::Saved
        } else {
            EnvironmentValuesAnswer::Declined
        })
    }
}

/// Accepts exactly one canonical value-request label; anything else leaves the request pending.
/// 只接受恰好一个规范取值请求标签；其他回复会让请求保持待处理。
pub(super) fn validate_environment_answers(answers: &[Vec<String>]) -> Result<(), String> {
    match answers {
        [choice] if matches!(choice.as_slice(), [label] if label == ENVIRONMENT_SAVED_LABEL || label == ENVIRONMENT_DECLINED_LABEL) => {
            Ok(())
        }
        _ => Err(format!(
            "An environment value request must be answered with exactly `{ENVIRONMENT_SAVED_LABEL}` or `{ENVIRONMENT_DECLINED_LABEL}`."
        )),
    }
}

/// Builds the default-language prompt shown by clients that cannot render value requests.
/// 构建无法渲染取值请求的客户端所展示的默认语言提示。
fn environment_question(prompt: &EnvironmentValuesPrompt) -> QuestionPrompt {
    let names = prompt
        .variables
        .iter()
        .map(|variable| variable.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    QuestionPrompt {
        question: format!(
            "The agent asks for environment variable values: {names}\n\n{}",
            prompt.purpose
        ),
        header: String::from("Environment"),
        options: vec![
            QuestionOption {
                label: String::from(ENVIRONMENT_SAVED_LABEL),
                description: String::from("The values were stored."),
            },
            QuestionOption {
                label: String::from(ENVIRONMENT_DECLINED_LABEL),
                description: String::from("Do not provide values; the agent is told."),
            },
        ],
        multiple: Some(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tooling::native::{
        EnvironmentValueRequest, EnvironmentVariableKind, EnvironmentVariableLayer,
    };
    use std::sync::Mutex;
    use vulcan_session_core::v1::ids::Seq;

    /// Exact coordinates of one synthetic `env` call.
    /// 一次合成 `env` 调用的精确坐标。
    fn context() -> VulcanToolInvocationContext {
        VulcanToolInvocationContext {
            turn_id: vulcan_session_core::v1::ids::TurnId::try_new(1).expect("synthetic turn"),
            session_id: Uuid::nil().to_string(),
            request_stream_id: RequestStreamId::try_new("environment-stream").unwrap(),
            api_call_id: ApiCallId::try_new(1).unwrap(),
            tool_call_id: ToolCallId::try_new(2).unwrap(),
            tool_seq: Seq::try_new(3).unwrap(),
            controller_tool_call_id: String::from("environment-call"),
            provider_tool_call_id: None,
        }
    }

    #[tokio::test]
    async fn value_request_replies_resume_the_tool_instead_of_completing_it() {
        for (label, expected) in [
            (ENVIRONMENT_SAVED_LABEL, EnvironmentValuesAnswer::Saved),
            (
                ENVIRONMENT_DECLINED_LABEL,
                EnvironmentValuesAnswer::Declined,
            ),
        ] {
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
            let requesting = Arc::clone(&runtime);
            let task = tokio::spawn(async move {
                requesting
                    .request_environment_values(
                        Uuid::nil(),
                        context(),
                        String::from("env"),
                        json!({"action": "request"}),
                        EnvironmentValuesPrompt {
                            purpose: String::from("Deploy needs a token."),
                            variables: vec![EnvironmentValueRequest {
                                name: String::from("VULCAN_ENV_TOKEN"),
                                description: String::from("Deploy token"),
                                kind: EnvironmentVariableKind::Secret,
                                layer: EnvironmentVariableLayer::Project,
                            }],
                            project_available: true,
                        },
                    )
                    .await
            });
            let request = loop {
                if let Some(request) = runtime.list_pending().unwrap().into_iter().next() {
                    break request;
                }
                tokio::task::yield_now().await;
            };
            assert!(request.environment.is_some());
            assert!(!request.is_model_question());
            assert!(
                runtime
                    .reply(&request.id, vec![vec![String::from("secret-value")]])
                    .is_err(),
                "a value must never be accepted as a reply"
            );
            assert!(
                runtime
                    .reply(&request.id, vec![vec![String::from(label)]])
                    .unwrap()
            );
            assert_eq!(task.await.unwrap(), Ok(expected));
            assert_eq!(*actions.lock().unwrap(), vec!["waiting", "resumed"]);
        }
    }
}
