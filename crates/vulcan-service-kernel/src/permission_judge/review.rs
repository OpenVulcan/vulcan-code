//! Permission reviewer: one restricted LLM call per escalated tool call, routed through the
//! `permission_review` inference profile with its own reasoning effort.
//! 权限复核器：每次升级工具调用发起一次受限 LLM 调用，经由 `permission_review` 推理配置档路由，
//! 并使用其独立思考档位。

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{Value, json};
use vulcan_agent_runtime::agent_runtime::tool_permission::{
    JudgeFinding, LlmReview, ReviewCall, ReviewVerdict,
};
use vulcan_ai_contract::{
    AiMessage, AiOperation, ContentItem, HostAiCallContext, HostAiCapabilityHandle,
    HostAiCapabilityRegistration, HostAiCapabilityRequirement, HostAiPort,
    INFERENCE_CONTRACT_VERSION, InferenceCallerBudget, InferenceCallerContext, InferencePermission,
    InferenceSchedulingClass, InferenceUsageScope, MessageRole, ParallelToolCallPolicy,
    ReasoningOutputPolicy, ResponsesExecutionRequest, StructuredOutputPolicy, ToolChoicePolicy,
};
use vulcan_provider_config::provider_config::JudgeState;
use vulcan_runtime_assets::PERMISSION_REVIEW_MD;

use super::services::{backend_policy, load_project_options, read_state};
use crate::SharedServiceBridgeState;

/// Consumer profile that owns the reviewer's inference route and reasoning effort.
/// 持有复核器推理路由与思考档位的消费者配置档。
pub(crate) const PERMISSION_REVIEW_CONSUMER_PROFILE_ID: &str = "permission_review";

/// The reviewer's only LLM purpose within its consumer profile.
/// 复核器在其消费者配置档内唯一的 LLM 用途。
pub(crate) const PERMISSION_REVIEW_PURPOSE_ID: &str = "tool_call_review";

/// Stable capability registration identity.
/// 稳定能力注册标识。
const REVIEW_REGISTRATION_ID: &str = "vulcan-code-permission-review";

/// Owner and caller identity of the registration.
/// 注册的持有者与调用方标识。
const REVIEW_CALLER_ID: &str = "vulcan-code-permission-review";

/// Longest wait for one review; the call then goes to the user.
/// 一次复核的最长等待时间；超时后调用交给用户。
const REVIEW_TIMEOUT: Duration = Duration::from_secs(120);

/// Concurrent reviews one process may run; parallel tool calls each need one.
/// 一个进程可并发运行的复核数；并行工具调用各需一次。
const REVIEW_MAX_CONCURRENT: u32 = 8;

/// Characters of the latest user request the reviewer reads; longer requests keep their start.
/// 复核器读取的最近用户请求字符数；更长的请求保留开头部分。
const REVIEW_USER_REQUEST_CHARS: usize = 4_000;

/// Structured answer of the reviewer.
/// 复核器的结构化回答。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewAnswer {
    /// Decision key.
    /// 决定键。
    verdict: String,
    /// Reason for the user.
    /// 给用户的理由。
    reason: String,
}

/// Reviews one escalated call; `Ok(None)` when no reviewer route is configured.
/// 复核一次升级调用；未配置复核路由时返回 `Ok(None)`。
pub(crate) async fn review_call(
    state: &SharedServiceBridgeState,
    call: ReviewCall,
) -> Result<Option<LlmReview>, String> {
    let (port, may_allow_blocked) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let runtime = guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| String::from("The service is still starting."))?;
        (
            Arc::clone(&runtime.internal_ai_port),
            runtime
                .provider_configuration
                .permission_judge
                .llm_may_allow_blocked,
        )
    };
    let handle = capability(port.as_ref())?;
    let configured = handle.snapshot.operations.iter().any(|operation| {
        operation.operation == AiOperation::ConversationRespond
            && operation.purpose_id.as_deref() == Some(PERMISSION_REVIEW_PURPOSE_ID)
    });
    if !configured {
        return Ok(None);
    }
    let policy = project_policy(state, call.workspace_root.as_deref()).await;
    let request = review_request(&call, &policy);
    let context = HostAiCallContext::new(request.request_id.clone())
        .with_deadline(Some(unix_ms_after(REVIEW_TIMEOUT)));
    let result = tokio::time::timeout(REVIEW_TIMEOUT, async {
        let execution = port
            .start_response_execution(&handle, context, request)
            .await?;
        port.wait_response(&execution).await
    })
    .await
    .map_err(|_| String::from("The permission reviewer did not answer in time."))?
    .map_err(|error| error.message)?;
    if result.termination_reason != "completed" {
        return Err(format!(
            "The permission reviewer stopped with `{}`.",
            result.termination_reason
        ));
    }
    let value = result
        .structured_output
        .ok_or_else(|| String::from("The permission reviewer returned no structured answer."))?;
    parse_answer(value, may_allow_blocked).map(Some)
}

/// Returns the reviewer capability with a fresh snapshot, registering it on first use.
/// 返回带最新快照的复核能力，首次使用时注册。
fn capability(port: &dyn HostAiPort) -> Result<HostAiCapabilityHandle, String> {
    let mut handle = match port.capability_handle(REVIEW_REGISTRATION_ID) {
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

/// Declares the reviewer's optional LLM requirement.
/// 声明复核器可选的 LLM 要求。
fn registration() -> HostAiCapabilityRegistration {
    HostAiCapabilityRegistration {
        registration_id: String::from(REVIEW_REGISTRATION_ID),
        owner_id: String::from(REVIEW_CALLER_ID),
        consumer_profile_id: String::from(PERMISSION_REVIEW_CONSUMER_PROFILE_ID),
        caller: InferenceCallerContext {
            caller_id: String::from(REVIEW_CALLER_ID),
            allowed_consumer_profile_ids: [String::from(PERMISSION_REVIEW_CONSUMER_PROFILE_ID)]
                .into(),
            permissions: [InferencePermission::Status, InferencePermission::Llm].into(),
            usage_scope: InferenceUsageScope::InternalBackground,
            // An agent turn waits on every review, so it runs in foreground capacity.
            // 智能体回合等待每次复核，因此使用前台容量。
            scheduling_class: InferenceSchedulingClass::InternalQuery,
            interactive_session_id: None,
            deadline_unix_ms: None,
            budget: InferenceCallerBudget {
                max_concurrent_executions: REVIEW_MAX_CONCURRENT,
                max_input_units_per_request: None,
                max_output_tokens_per_request: None,
                max_requests_per_minute: None,
                max_input_units_per_minute: None,
                max_execution_duration_ms: Some(REVIEW_TIMEOUT.as_millis() as u64),
            },
        },
        requirements: vec![HostAiCapabilityRequirement {
            operation: AiOperation::ConversationRespond,
            purpose_id: Some(String::from(PERMISSION_REVIEW_PURPOSE_ID)),
            required: false,
        }],
    }
}

/// Project policy lines: resolved with sentences when the chosen backend's policy is available,
/// otherwise the project's raw choices.
/// 项目策略行：所选后端的策略可用时按策略解析并附带说明句，否则为项目的原始选择。
async fn project_policy(state: &SharedServiceBridgeState, root: Option<&Path>) -> Vec<String> {
    let chosen = match root {
        Some(root) => load_project_options(root).unwrap_or_default(),
        None => BTreeMap::new(),
    };
    if let Ok((runtime, settings)) = read_state(state)
        && settings.state == Some(JudgeState::Enabled)
        && let Ok(Some(policy)) = backend_policy(&runtime, &settings).await
        && let Ok(modes) = policy.resolve_settings(&chosen)
    {
        return modes
            .to_map()
            .into_iter()
            .map(|(key, mode)| {
                let sentence = policy
                    .options
                    .get(&key)
                    .and_then(|option| option.modes.get(&mode))
                    .cloned()
                    .unwrap_or_default();
                format!("{key} = {mode}: {sentence}")
            })
            .collect();
    }
    chosen
        .into_iter()
        .map(|(key, mode)| format!("{key} = {mode}"))
        .collect()
}

/// Builds the restricted request for one call.
/// 为一次调用构建受限请求。
fn review_request(call: &ReviewCall, policy: &[String]) -> ResponsesExecutionRequest {
    ResponsesExecutionRequest {
        contract_version: INFERENCE_CONTRACT_VERSION,
        request_id: format!("permission-review-{}", uuid::Uuid::new_v4()),
        idempotency_key: None,
        consumer_profile_id: String::from(PERMISSION_REVIEW_CONSUMER_PROFILE_ID),
        purpose_id: String::from(PERMISSION_REVIEW_PURPOSE_ID),
        messages: vec![
            AiMessage {
                role: MessageRole::System,
                content: vec![ContentItem::Text {
                    text: String::from(PERMISSION_REVIEW_MD),
                }],
                provider_context_items: Vec::new(),
            },
            AiMessage {
                role: MessageRole::User,
                content: vec![ContentItem::Text {
                    text: render_review_input(call, policy),
                }],
                provider_context_items: Vec::new(),
            },
        ],
        tools: Vec::new(),
        native_tools: Vec::new(),
        tool_choice: ToolChoicePolicy::Disabled,
        parallel_tool_calls: ParallelToolCallPolicy::Disabled,
        structured_output: StructuredOutputPolicy::JsonSchema {
            name: String::from("permission_review"),
            description: Some(String::from(
                "Decision on one tool call and the reason shown to the user.",
            )),
            schema: answer_schema(),
            strict: true,
        },
        prompt_cache: Default::default(),
        prompt_cache_key: None,
        include_reasoning_encrypted_content: false,
        // The profile's purpose-scoped effort applies when the request leaves it unset.
        // 请求未设置时适用配置档的用途级思考档位。
        reasoning_effort: None,
        reasoning_output: ReasoningOutputPolicy::Hidden,
        model_parameters: BTreeMap::new(),
        stop_sequences: Vec::new(),
        resources: Vec::new(),
        output_token_limit: None,
    }
}

/// Renders the reviewer's user turn; every section is marked as data.
/// 渲染复核器的用户轮；每个区段都标记为数据。
fn render_review_input(call: &ReviewCall, policy: &[String]) -> String {
    let project = call
        .workspace_root
        .as_ref()
        .map_or_else(|| String::from("(none)"), |root| root.display().to_string());
    let user_request = call
        .user_request
        .as_deref()
        .map(|text| text.chars().take(REVIEW_USER_REQUEST_CHARS).collect())
        .unwrap_or_else(|| String::from("(none)"));
    let judge = match &call.judge {
        Some(JudgeFinding::Scored(label)) => label.key().to_string(),
        Some(JudgeFinding::Unavailable(reason)) => format!("unavailable ({reason})"),
        None => String::from("not configured"),
    };
    let high_risk = call.high_risk.map_or_else(
        || String::from("none"),
        |category| category.key().to_string(),
    );
    let policy = if policy.is_empty() {
        String::from("(no choices)")
    } else {
        policy.join("\n")
    };
    format!(
        "<call>\nProject: {project}\nTool: {}\nArguments: {}\n</call>\n\n<user_request>\n{user_request}\n</user_request>\n\n<project_policy>\n{policy}\n</project_policy>\n\n<signals>\njudge: {judge}\nhigh_risk: {high_risk}\n</signals>",
        call.tool_name, call.arguments
    )
}

/// JSON Schema of the reviewer's answer.
/// 复核器回答的 JSON Schema。
fn answer_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "verdict": {"type": "string", "enum": ["allow", "ask_user", "deny"]},
            "reason": {"type": "string"}
        },
        "required": ["verdict", "reason"],
        "additionalProperties": false
    })
}

/// Decodes the structured answer.
/// 解码结构化回答。
fn parse_answer(value: Value, may_allow_blocked: bool) -> Result<LlmReview, String> {
    let answer: ReviewAnswer = serde_json::from_value(value)
        .map_err(|error| format!("The permission reviewer answer is malformed: {error}"))?;
    let verdict = [
        ReviewVerdict::Allow,
        ReviewVerdict::AskUser,
        ReviewVerdict::Deny,
    ]
    .into_iter()
    .find(|verdict| verdict.key() == answer.verdict)
    .ok_or_else(|| {
        format!(
            "The permission reviewer answered with an unknown verdict `{}`.",
            answer.verdict
        )
    })?;
    Ok(LlmReview {
        verdict,
        reason: answer.reason.trim().to_string(),
        may_allow_blocked,
    })
}

/// Absolute Unix milliseconds `after` from now.
/// 从现在起 `after` 之后的绝对 Unix 毫秒数。
pub(super) fn unix_ms_after(after: Duration) -> u64 {
    SystemTime::now()
        .checked_add(after)
        .and_then(|deadline| deadline.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |deadline| deadline.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use vulcan_agent_runtime::agent_runtime::tool_permission::{HighRiskCategory, JudgeLabel};

    #[test]
    fn answers_decode_to_verdicts_and_reject_unknown_shapes() {
        let review = parse_answer(json!({"verdict": "ask_user", "reason": " why "}), true).unwrap();
        assert_eq!(
            review,
            LlmReview {
                verdict: ReviewVerdict::AskUser,
                reason: String::from("why"),
                may_allow_blocked: true,
            }
        );
        assert!(parse_answer(json!({"verdict": "maybe", "reason": "x"}), false).is_err());
        assert!(
            parse_answer(
                json!({"verdict": "allow", "reason": "x", "extra": 1}),
                false
            )
            .is_err()
        );
    }

    #[test]
    fn request_carries_the_call_as_data_and_satisfies_its_schema() {
        let call = ReviewCall {
            workspace_root: Some(PathBuf::from("/work/app")),
            tool_name: String::from("shell_command"),
            arguments: String::from(r#"{"command":"rm -rf build"}"#),
            user_request: Some("x".repeat(REVIEW_USER_REQUEST_CHARS + 10)),
            high_risk: Some(HighRiskCategory::RecursiveDelete),
            judge: Some(JudgeFinding::Scored(JudgeLabel::Review)),
        };
        let request = review_request(&call, &[String::from("writes = allow")]);
        assert_eq!(
            request.consumer_profile_id,
            PERMISSION_REVIEW_CONSUMER_PROFILE_ID
        );
        assert_eq!(request.purpose_id, PERMISSION_REVIEW_PURPOSE_ID);
        assert_eq!(request.reasoning_effort, None);
        let ContentItem::Text { text } = &request.messages[1].content[0] else {
            panic!("the user turn must be text");
        };
        for section in ["<call>", "<user_request>", "<project_policy>", "<signals>"] {
            assert!(text.contains(section), "missing {section}");
        }
        assert!(text.contains(&call.arguments));
        assert!(!text.contains(&"x".repeat(REVIEW_USER_REQUEST_CHARS + 1)));
        let StructuredOutputPolicy::JsonSchema { schema, .. } = &request.structured_output else {
            panic!("the reviewer must request a JSON Schema answer");
        };
        let validator = jsonschema::draft202012::new(schema).unwrap();
        assert!(validator.is_valid(&json!({"verdict": "deny", "reason": "r"})));
        assert!(!validator.is_valid(&json!({"verdict": "deny"})));
    }
}
