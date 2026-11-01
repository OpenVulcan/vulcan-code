//! API judge: one decision request per escalated call through the `permission_judge` route.
//! API 判定：每次升级调用经由 `permission_judge` 路由发起一次判定请求。
//!
//! The route names a decision model the user reaches with their own provider account, such as jev
//! on TypeSafe or OpenRouter; the call leaves this machine, which the settings page states before
//! the user enables it. Routes of the OpenVulcan account are refused: OpenVulcan takes part in
//! judging only as the cloud backend.
//! 路由指定用户以自己的供应商账号访问的判定模型，例如 TypeSafe 或 OpenRouter 上的 jev；调用会离开
//! 本机，设置页在用户启用前说明这一点。拒绝 OpenVulcan 账户的路由：OpenVulcan 只以云端后端参与判定。

use std::sync::Arc;
use std::time::{Duration, Instant};

use vulcan_ai_contract::{
    AiOperation, DecisionAnswer, DecisionCriterion, DecisionQuestion, DecisionQuestionKind,
    DecisionRequest, HostAiCallContext, HostAiCapabilityHandle, HostAiCapabilityRegistration,
    HostAiCapabilityRequirement, HostAiPort, INFERENCE_CONTRACT_VERSION, InferenceCallerBudget,
    InferenceCallerContext, InferencePermission, InferenceSchedulingClass, InferenceUsageScope,
};
use vulcan_permission_judge::{JudgeLabel, JudgeSettings, api_label, api_state, hosted_policy};
use vulcan_provider_adapters::{
    InferenceRouteConfiguration, PersistedAiConfiguration, PublicAiRuntime,
};

use super::review::unix_ms_after;
use crate::SharedServiceBridgeState;
use crate::public_ai_settings::openvulcan_account::is_core_account_provider;

/// Consumer profile that owns the API judge's decision route.
/// 持有 API 判定判定路由的消费者配置档。
pub(crate) const PERMISSION_JUDGE_CONSUMER_PROFILE_ID: &str = "permission_judge";

/// Stable capability registration identity.
/// 稳定能力注册标识。
const JUDGE_REGISTRATION_ID: &str = "vulcan-code-permission-judge";

/// Owner and caller identity of the registration, and the usage-ledger consumer of every judge
/// backend.
/// 注册的持有者与调用方标识，也是所有判定后端的用量账本消费者。
pub(super) const JUDGE_CALLER_ID: &str = "vulcan-code-permission-judge";

/// Longest wait for one API judgement; the call then goes to the next layer.
/// 一次 API 判定的最长等待时间；超时后调用交给下一层。
const API_JUDGE_TIMEOUT: Duration = Duration::from_secs(30);

/// Concurrent judgements one process may run; parallel tool calls and the ten-call speed test
/// each need one.
/// 一个进程可并发运行的判定数；并行工具调用与十条速度测试各需一次。
const API_JUDGE_MAX_CONCURRENT: u32 = 10;

/// The only question of a judgement request.
/// 判定请求中唯一的问题。
const JUDGE_QUESTION_ID: &str = "decision";

/// One call the API judge reads.
/// API 判定读取的一次调用。
pub(super) struct ApiCall<'a> {
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

/// The configured route's model, or `None` when no decision route is saved; a saved route of the
/// OpenVulcan account is an error.
/// 已配置路由的模型；未保存判定路由时为 `None`；已保存的 OpenVulcan 账户路由为错误。
pub(super) fn configured_model(state: &SharedServiceBridgeState) -> Result<Option<String>, String> {
    let (runtime, configuration) =
        crate::public_ai_settings::public_ai_runtime_and_configuration(state)?;
    let Some(route) = configuration
        .inference_profiles
        .get(PERMISSION_JUDGE_CONSUMER_PROFILE_ID)
        .and_then(|profile| profile.decision.as_ref())
    else {
        return Ok(None);
    };
    reject_openvulcan_route(&runtime, &configuration, route)?;
    Ok(Some(route.model_id.clone()))
}

/// Refuses a decision route that runs on the OpenVulcan account, by its provider or by the
/// provider of its connection. OpenVulcan takes part in judging only as the cloud backend, so the
/// decision models its account lists never judge calls as the API backend.
/// 拒绝运行在 OpenVulcan 账户上的判定路由，按其供应商或其连接的供应商判断。OpenVulcan 只以云端后端
/// 参与判定，因此其账户列出的判定模型绝不作为 API 后端判定调用。
pub(crate) fn reject_openvulcan_route(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    route: &InferenceRouteConfiguration,
) -> Result<(), String> {
    let connection_provider = configuration
        .connection_graph
        .connections
        .get(&route.connection_id)
        .map(|connection| connection.provider_id.as_str());
    for provider_id in std::iter::once(route.provider_id.as_str()).chain(connection_provider) {
        if is_core_account_provider(runtime, configuration, provider_id)? {
            return Err(format!(
                "The API judge accepts only a provider account you connect yourself; the OpenVulcan account model `{}` cannot judge calls here. Choose the cloud judge to use OpenVulcan's judge.",
                route.model_id
            ));
        }
    }
    Ok(())
}

/// Judges one call through the configured decision route; returns the label and the probability
/// of allow.
/// 经由已配置的判定路由判定一次调用；返回标签与 allow 的概率。
pub(super) async fn judge(
    state: &SharedServiceBridgeState,
    settings: &JudgeSettings,
    call: ApiCall<'_>,
) -> Result<(JudgeLabel, f32), String> {
    configured_model(state)?;
    let port = internal_ai_port(state)?;
    let handle = capability(port.as_ref())?;
    let configured = handle
        .snapshot
        .operations
        .iter()
        .any(|operation| operation.operation == AiOperation::DecisionEvaluate);
    if !configured {
        return Err(String::from(
            "Choose a decision model for the API judge in the permission judge settings.",
        ));
    }
    let request = judge_request(settings, &call);
    let context = HostAiCallContext::new(request.request_id.clone())
        .with_deadline(Some(unix_ms_after(API_JUDGE_TIMEOUT)));
    let response = tokio::time::timeout(
        API_JUDGE_TIMEOUT,
        port.evaluate_decision(&handle, context, request),
    )
    .await
    .map_err(|_| String::from("The API judge did not answer in time."))?
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
            api_label(choice, allow).map(|label| (label, allow as f32))
        }
        None => Err(String::from("The API judge returned no answer.")),
    }
}

/// Times `batch_size` concurrent judgements of one ordinary call; returns the wall time.
/// 为一次普通调用的 `batch_size` 个并发判定计时；返回总耗时。
pub(super) async fn time_batch(
    state: &SharedServiceBridgeState,
    batch_size: u32,
) -> Result<Duration, String> {
    let settings = hosted_policy()
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
            ApiCall {
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

/// Declares the API judge's optional decision requirement.
/// 声明 API 判定可选的判定要求。
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
            owner_session_id: None,
            deadline_unix_ms: None,
            budget: InferenceCallerBudget {
                max_concurrent_executions: API_JUDGE_MAX_CONCURRENT,
                max_input_units_per_request: None,
                max_output_tokens_per_request: None,
                max_requests_per_minute: None,
                max_input_units_per_minute: None,
                max_execution_duration_ms: Some(API_JUDGE_TIMEOUT.as_millis() as u64),
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
fn judge_request(settings: &JudgeSettings, call: &ApiCall<'_>) -> DecisionRequest {
    let policy = hosted_policy();
    DecisionRequest {
        contract_version: INFERENCE_CONTRACT_VERSION,
        request_id: format!("permission-judge-{}", uuid::Uuid::new_v4()),
        idempotency_key: None,
        consumer_profile_id: String::from(PERMISSION_JUDGE_CONSUMER_PROFILE_ID),
        state: api_state(call.project_root, call.tool, call.raw_arguments),
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
    use vulcan_provider_adapters::{CredentialConnection, CredentialMode, InferenceRouteBinding};

    /// A decision route of `provider_id` on `connection_id`.
    /// `provider_id` 在 `connection_id` 上的判定路由。
    fn decision_route(provider_id: &str, connection_id: &str) -> InferenceRouteConfiguration {
        InferenceRouteConfiguration {
            provider_id: provider_id.into(),
            service_entry_id: String::from("entry"),
            connection_id: connection_id.into(),
            model_id: String::from("jev"),
            binding: InferenceRouteBinding::Action {
                action_binding_id: String::from("decision"),
            },
            operation: AiOperation::DecisionEvaluate,
            usage_scope: InferenceUsageScope::InternalBackground,
        }
    }

    /// A connection of `provider_id` stored under `id`, without any secret.
    /// 以 `id` 保存、不含任何密钥的 `provider_id` 连接。
    fn connection(id: &str, provider_id: &str) -> CredentialConnection {
        CredentialConnection {
            id: id.into(),
            provider_id: provider_id.into(),
            service_entry_id: String::from("entry"),
            credential_profile_id: String::from("account"),
            credential_mode: CredentialMode::Auth,
            credential_id: id.into(),
            display_name: id.into(),
            secret_reference: id.into(),
            management_secret_reference: None,
            service_origin_override: None,
        }
    }

    #[test]
    fn openvulcan_account_routes_cannot_judge_as_the_api_backend() {
        let runtime = PublicAiRuntime::unavailable().unwrap();
        let mut configuration = PersistedAiConfiguration::default();
        for (id, provider_id) in [("own", "typesafe"), ("account", "openvulcan")] {
            configuration
                .connection_graph
                .connections
                .insert(id.into(), connection(id, provider_id));
        }
        // A route the user connects through their own provider account stands.
        // 用户经自己的供应商账号接入的路由成立。
        assert!(
            reject_openvulcan_route(&runtime, &configuration, &decision_route("typesafe", "own"))
                .is_ok()
        );
        // The OpenVulcan account is refused by the route's provider or by its connection's.
        // 按路由的供应商或其连接的供应商拒绝 OpenVulcan 账户。
        for route in [
            decision_route("openvulcan", "account"),
            decision_route("openvulcan", "missing"),
            decision_route("typesafe", "account"),
        ] {
            assert!(
                reject_openvulcan_route(&runtime, &configuration, &route).is_err(),
                "{route:?}"
            );
        }
    }

    #[test]
    fn request_offers_the_labels_and_passes_its_own_validation() {
        let settings = hosted_policy()
            .resolve_settings(&Default::default())
            .unwrap();
        let request = judge_request(
            &settings,
            &ApiCall {
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
