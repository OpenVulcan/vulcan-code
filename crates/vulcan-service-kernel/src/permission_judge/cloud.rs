//! Cloud judge: one decision request per escalated call through the `permission_judge` route.
//! 云端判定：每次升级调用经由 `permission_judge` 路由发起一次判定请求。
//!
//! The route names the hosted decision model, such as jev on TypeSafe or OpenRouter; the call
//! leaves this machine, which the settings page states before the user enables it.
//! 路由指定托管判定模型，例如 TypeSafe 或 OpenRouter 上的 jev；调用会离开本机，设置页在用户启用
//! 前说明这一点。

use std::sync::Arc;
use std::time::{Duration, Instant};

use vulcan_ai_contract::{
    AiOperation, DecisionAnswer, DecisionCriterion, DecisionQuestion, DecisionQuestionKind,
    DecisionRequest, HostAiCallContext, HostAiCapabilityHandle, HostAiCapabilityRegistration,
    HostAiCapabilityRequirement, HostAiPort, INFERENCE_CONTRACT_VERSION, InferenceCallerBudget,
    InferenceCallerContext, InferencePermission, InferenceSchedulingClass, InferenceUsageScope,
};
use vulcan_permission_judge::{JudgeLabel, JudgeSettings, cloud_label, cloud_policy, cloud_state};

use super::review::unix_ms_after;
use crate::SharedServiceBridgeState;

/// Consumer profile that owns the cloud judge's decision route.
/// 持有云端判定判定路由的消费者配置档。
pub(crate) const PERMISSION_JUDGE_CONSUMER_PROFILE_ID: &str = "permission_judge";

/// Stable capability registration identity.
/// 稳定能力注册标识。
const JUDGE_REGISTRATION_ID: &str = "vulcan-code-permission-judge";

/// Owner and caller identity of the registration.
/// 注册的持有者与调用方标识。
const JUDGE_CALLER_ID: &str = "vulcan-code-permission-judge";

/// Longest wait for one cloud judgement; the call then goes to the next layer.
/// 一次云端判定的最长等待时间；超时后调用交给下一层。
const CLOUD_JUDGE_TIMEOUT: Duration = Duration::from_secs(30);

/// Concurrent judgements one process may run; parallel tool calls and the ten-call speed test
/// each need one.
/// 一个进程可并发运行的判定数；并行工具调用与十条速度测试各需一次。
const CLOUD_JUDGE_MAX_CONCURRENT: u32 = 10;

/// The only question of a judgement request.
/// 判定请求中唯一的问题。
const JUDGE_QUESTION_ID: &str = "decision";

/// One call the cloud judge reads.
/// 云端判定读取的一次调用。
pub(super) struct CloudCall<'a> {
    /// Project root, when known.
    /// 已知时的项目根目录。
    pub(super) project_root: Option<&'a str>,
    /// Tool name.
    /// 工具名称。
    pub(super) tool: &'a str,
    /// Argument text exactly as the agent model sent it.
    /// 代理模型发送的原样参数文本。
    pub(super) raw_arguments: &'a str,
}

/// The configured route's model, or `None` when no decision route is saved.
/// 已配置路由的模型；未保存判定路由时为 `None`。
pub(super) fn configured_model(state: &SharedServiceBridgeState) -> Result<Option<String>, String> {
    let (_, configuration) = crate::public_ai_settings::public_ai_runtime_and_configuration(state)?;
    Ok(configuration
        .inference_profiles
        .get(PERMISSION_JUDGE_CONSUMER_PROFILE_ID)
        .and_then(|profile| profile.decision.as_ref())
        .map(|route| route.model_id.clone()))
}

/// Judges one call through the configured decision route; returns the label and the probability
/// of allow.
/// 经由已配置的判定路由判定一次调用；返回标签与 allow 的概率。
pub(super) async fn judge(
    state: &SharedServiceBridgeState,
    settings: &JudgeSettings,
    call: CloudCall<'_>,
) -> Result<(JudgeLabel, f32), String> {
    let port = internal_ai_port(state)?;
    let handle = capability(port.as_ref())?;
    let configured = handle
        .snapshot
        .operations
        .iter()
        .any(|operation| operation.operation == AiOperation::DecisionEvaluate);
    if !configured {
        return Err(String::from(
            "Choose a decision model for the cloud judge in the permission judge settings.",
        ));
    }
    let request = judge_request(settings, &call);
    let context = HostAiCallContext::new(request.request_id.clone())
        .with_deadline(Some(unix_ms_after(CLOUD_JUDGE_TIMEOUT)));
    let response = tokio::time::timeout(
        CLOUD_JUDGE_TIMEOUT,
        port.evaluate_decision(&handle, context, request),
    )
    .await
    .map_err(|_| String::from("The cloud judge did not answer in time."))?
    .map_err(|error| error.message)?;
    match response.answers.get(JUDGE_QUESTION_ID) {
        Some(DecisionAnswer::Choice {
            choice,
            probabilities,
            ..
        }) => {
            let allow = probabilities
                .get(JudgeLabel::Allow.key())
                .copied()
                .unwrap_or_default();
            cloud_label(choice, allow).map(|label| (label, allow as f32))
        }
        None => Err(String::from("The cloud judge returned no answer.")),
    }
}

/// Times `batch_size` concurrent judgements of one ordinary call; returns the wall time.
/// 为一次普通调用的 `batch_size` 个并发判定计时；返回总耗时。
pub(super) async fn time_batch(
    state: &SharedServiceBridgeState,
    batch_size: u32,
) -> Result<Duration, String> {
    let settings = cloud_policy()
        .resolve_settings(&Default::default())
        .map_err(|error| error.to_string())?;
    // Registering once up front keeps the concurrent calls from racing to register.
    // 预先注册一次，避免并发调用争相注册。
    capability(internal_ai_port(state)?.as_ref())?;
    let started = Instant::now();
    let judgements = (0..batch_size).map(|_| {
        judge(
            state,
            &settings,
            CloudCall {
                project_root: None,
                tool: "shell_command",
                raw_arguments: r#"{"command":"git status"}"#,
            },
        )
    });
    for result in futures_util::future::join_all(judgements).await {
        result?;
    }
    Ok(started.elapsed())
}

/// The service's internal AI port.
/// 服务的内部 AI 端口。
fn internal_ai_port(state: &SharedServiceBridgeState) -> Result<Arc<dyn HostAiPort>, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let runtime = guard
        .agent_runtime
        .as_ref()
        .ok_or_else(|| String::from("The service is still starting."))?;
    Ok(Arc::clone(&runtime.internal_ai_port))
}

/// Returns the judge capability with a fresh snapshot, registering it on first use.
/// 返回带最新快照的判定能力，首次使用时注册。
fn capability(port: &dyn HostAiPort) -> Result<HostAiCapabilityHandle, String> {
    let mut handle = match port.capability_handle(JUDGE_REGISTRATION_ID) {
        Ok(handle) => handle,
        Err(_) => {
            return port
                .register_capability(registration())
                .map_err(|error| error.message);
        }
    };
    handle.snapshot = port
        .capability_snapshot(&handle)
        .map_err(|error| error.message)?;
    Ok(handle)
}

/// Declares the cloud judge's optional decision requirement.
/// 声明云端判定可选的判定要求。
fn registration() -> HostAiCapabilityRegistration {
    HostAiCapabilityRegistration {
        registration_id: String::from(JUDGE_REGISTRATION_ID),
        owner_id: String::from(JUDGE_CALLER_ID),
        consumer_profile_id: String::from(PERMISSION_JUDGE_CONSUMER_PROFILE_ID),
        caller: InferenceCallerContext {
            caller_id: String::from(JUDGE_CALLER_ID),
            allowed_consumer_profile_ids: [String::from(PERMISSION_JUDGE_CONSUMER_PROFILE_ID)]
                .into(),
            permissions: [InferencePermission::Status, InferencePermission::Decision].into(),
            usage_scope: InferenceUsageScope::InternalBackground,
            // An agent turn waits on every judgement, so it runs in foreground capacity.
            // 智能体回合等待每次判定，因此使用前台容量。
            scheduling_class: InferenceSchedulingClass::InternalQuery,
            interactive_session_id: None,
            deadline_unix_ms: None,
            budget: InferenceCallerBudget {
                max_concurrent_executions: CLOUD_JUDGE_MAX_CONCURRENT,
                max_input_units_per_request: None,
                max_output_tokens_per_request: None,
                max_requests_per_minute: None,
                max_input_units_per_minute: None,
                max_execution_duration_ms: Some(CLOUD_JUDGE_TIMEOUT.as_millis() as u64),
            },
        },
        requirements: vec![HostAiCapabilityRequirement {
            operation: AiOperation::DecisionEvaluate,
            purpose_id: None,
            required: false,
        }],
    }
}

/// Builds the decision request: the policy renders the instructions and its criteria become the
/// options, in label order.
/// 构建判定请求：策略渲染指令，其判据按标签顺序成为选项。
fn judge_request(settings: &JudgeSettings, call: &CloudCall<'_>) -> DecisionRequest {
    let policy = cloud_policy();
    DecisionRequest {
        contract_version: INFERENCE_CONTRACT_VERSION,
        request_id: format!("permission-judge-{}", uuid::Uuid::new_v4()),
        idempotency_key: None,
        consumer_profile_id: String::from(PERMISSION_JUDGE_CONSUMER_PROFILE_ID),
        state: cloud_state(call.project_root, call.tool, call.raw_arguments),
        questions: vec![DecisionQuestion {
            id: String::from(JUDGE_QUESTION_ID),
            kind: DecisionQuestionKind::Choice {
                instructions: policy.instructions(settings),
                criteria: policy
                    .criteria
                    .iter()
                    .map(|(option, criterion)| DecisionCriterion {
                        option: option.clone(),
                        criterion: criterion.clone(),
                    })
                    .collect(),
            },
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_offers_the_labels_and_passes_its_own_validation() {
        let settings = cloud_policy()
            .resolve_settings(&Default::default())
            .unwrap();
        let request = judge_request(
            &settings,
            &CloudCall {
                project_root: Some("/w/demo"),
                tool: "shell_command",
                raw_arguments: r#"{"command":"ls"}"#,
            },
        );
        request.validate().unwrap();
        let DecisionQuestionKind::Choice { criteria, .. } = &request.questions[0].kind;
        assert_eq!(
            criteria
                .iter()
                .map(|criterion| criterion.option.as_str())
                .collect::<Vec<_>>(),
            JudgeLabel::ORDER.map(JudgeLabel::key)
        );
        assert_eq!(request.state["call"]["args"], r#"{"command":"ls"}"#);
    }
}
