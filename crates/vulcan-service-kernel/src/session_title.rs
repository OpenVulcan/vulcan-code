//! Model-generated titles for new sessions, written from the first user prompt.
//! 根据首个用户提示词为新会话生成的模型标题。
//!
//! Generation runs in the background after the submission is accepted. It renames the session
//! only while the session still carries the truncated title it was created with, so a manual
//! rename always wins. A failure is never retried.
//! 生成在提交被接纳后于后台运行；只有会话仍保留创建时的截断标题才会重命名，因此手动重命名始终
//! 优先。失败永不重试。

use crate::workspace_persistence_versions::SESSION_TITLE_MAX_CHARS;
use crate::{SessionId, SharedServiceBridgeState, emit_workbench_event};
use vulcan_agent_runtime::agent_runtime::provider_reasoning::{
    resolve_effective_public_reasoning_effort, validate_exact_wire_reasoning_effort,
};
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiOperation, ContentItem, MessageRole, ModelCapabilities,
    ParallelToolCallPolicy, ReasoningEffort, ReasoningOutputPolicy, StructuredOutputPolicy,
    ToolChoicePolicy,
};
use vulcan_provider_adapters::ResolvedPublicAiSelection;
use vulcan_provider_config::config::ReasoningEffortLevel;
use vulcan_service_api::{
    ServiceEvent, ServiceEventEmitter, SessionTitleGeneratedEvent, SessionTitleGenerationOutcome,
};

/// Inference consumer profile that holds the optional title route and its reasoning effort.
/// 保存可选标题路由及其思考档位的推理消费者配置档。
pub(crate) const SESSION_TITLE_CONSUMER_PROFILE_ID: &str = "session_title";

/// Restricted LLM purpose of the title route inside its profile.
/// 标题路由在其配置档中的受限 LLM 用途。
pub(crate) const SESSION_TITLE_PURPOSE_ID: &str = "session_title_generation";

/// Longest time one title request may run.
/// 单次标题请求的最长运行时间。
pub(crate) const SESSION_TITLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Longest part of the first prompt sent to the model; the start of a prompt names its task.
/// 发送给模型的首个提示词最长部分；提示词开头即可说明其任务。
const SESSION_TITLE_PROMPT_MAX_CHARS: usize = 4_000;

/// Everything title generation needs from the submission that created the session.
/// 标题生成从创建会话的提交中需要的全部信息。
pub(crate) struct SessionTitleGenerationRequest {
    /// The new session.
    /// 新会话。
    pub(crate) session_id: SessionId,
    /// The truncated title the session was created with.
    /// 会话创建时的截断标题。
    pub(crate) initial_title: String,
    /// The first user prompt.
    /// 首个用户提示词。
    pub(crate) prompt: String,
    /// The model the session runs on, used when no title route is saved.
    /// 会话运行所用的模型，未保存标题路由时使用。
    pub(crate) session_model: Option<ResolvedPublicAiSelection>,
}

/// Starts background title generation for one new session.
/// 为一个新会话启动后台标题生成。
///
/// # Parameters
/// 参数
/// `app` receives the workbench and outcome events; `state` owns settings and the AI runtime.
/// `app` 接收工作台与结果事件；`state` 持有设置与 AI 运行时。
pub(crate) fn spawn_session_title_generation(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionTitleGenerationRequest,
) {
    crate::spawn_service_task(async move {
        generate_session_title(app, state, request).await;
    });
}

/// Generates, applies and reports one session title; every skip and failure ends here.
/// 生成、应用并报告一个会话标题；所有跳过与失败都在此结束。
async fn generate_session_title(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionTitleGenerationRequest,
) {
    let session_id = request.session_id;
    let (model, outcome) = match prepare_generation(&state, request) {
        Ok(Preparation::Skip) => return,
        Ok(Preparation::Ready(prepared)) => {
            let model = prepared.selection.model.display_name.clone();
            match run_generation(&state, *prepared).await {
                Ok(Some(title)) => (model, SessionTitleGenerationOutcome::Renamed { title }),
                // The user renamed the session meanwhile; their title stays and nothing is reported.
                // 用户在此期间重命名了会话；保留其标题且不报告。
                Ok(None) => return,
                Err(error) => {
                    tracing::warn!(
                        "[session-title] title generation failed for {session_id}: {error}"
                    );
                    (model, SessionTitleGenerationOutcome::Failed)
                }
            }
        }
        Ok(Preparation::RouteUnavailable { model, error }) => {
            tracing::warn!("[session-title] title model unavailable for {session_id}: {error}");
            (model, SessionTitleGenerationOutcome::Failed)
        }
        Err(error) => {
            tracing::warn!("[session-title] skipped title generation for {session_id}: {error}");
            return;
        }
    };
    if let SessionTitleGenerationOutcome::Renamed { .. } = &outcome {
        match crate::refresh_workbench_snapshot(state.clone()).await {
            Ok(workbench) => emit_workbench_event(&app, &workbench),
            Err(error) => {
                tracing::warn!("[session-title] workbench refresh failed after rename: {error}")
            }
        }
    }
    app.emit(ServiceEvent::SessionTitleGenerated(Box::new(
        SessionTitleGeneratedEvent {
            session_id: session_id.to_string(),
            model,
            outcome,
        },
    )));
}

/// A title request whose model and reasoning effort are settled.
/// 已确定模型与思考档位的标题请求。
struct PreparedGeneration {
    request: SessionTitleGenerationRequest,
    selection: ResolvedPublicAiSelection,
    reasoning_effort: Option<ReasoningEffort>,
}

/// What the switch and the saved route allow for one new session.
/// 开关与已保存路由对一个新会话所允许的处理。
enum Preparation {
    /// The feature is off or no model is available; nothing is reported.
    /// 功能关闭或没有可用模型；不报告。
    Skip,
    /// The model and reasoning effort are settled.
    /// 模型与思考档位已确定。
    Ready(Box<PreparedGeneration>),
    /// The saved title model no longer resolves; reported as a failure of that model.
    /// 已保存的标题模型无法解析；作为该模型的失败报告。
    RouteUnavailable { model: String, error: String },
}

/// Reads the switch and the saved route, then settles the model and reasoning effort.
/// 读取开关与已保存路由，然后确定模型与思考档位。
///
/// # Returns
/// 返回值
/// The preparation outcome, or an error when settings cannot be read.
/// 返回准备结果；无法读取设置时返回错误。
fn prepare_generation(
    state: &SharedServiceBridgeState,
    request: SessionTitleGenerationRequest,
) -> Result<Preparation, String> {
    let enabled = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workbench
        .session_title_generation_enabled
        .unwrap_or(true);
    if !enabled {
        return Ok(Preparation::Skip);
    }
    let (runtime, configuration) =
        crate::public_ai_settings::public_ai_runtime_and_configuration(state)?;
    let profile = configuration
        .inference_profiles
        .get(SESSION_TITLE_CONSUMER_PROFILE_ID);
    let saved_route = profile.and_then(|profile| profile.llm_routes.get(SESSION_TITLE_PURPOSE_ID));
    let (selection, reasoning_effort) = match saved_route {
        Some(route) => {
            let selection =
                match runtime.resolve_internal_inference_selection(&configuration, route) {
                    Ok(selection) => selection,
                    Err(error) => {
                        return Ok(Preparation::RouteUnavailable {
                            model: route.model_id.clone(),
                            error,
                        });
                    }
                };
            let saved_effort = profile
                .and_then(|profile| profile.llm_reasoning_efforts.get(SESSION_TITLE_PURPOSE_ID))
                .copied();
            let effort = resolve_effective_public_reasoning_effort(
                &selection.model.capabilities,
                saved_effort,
            );
            (selection, effort)
        }
        None => {
            let Some(selection) = request.session_model.clone() else {
                return Ok(Preparation::Skip);
            };
            let effort = session_model_reasoning_effort(&selection.model.capabilities);
            (selection, effort)
        }
    };
    Ok(Preparation::Ready(Box::new(PreparedGeneration {
        request,
        selection,
        reasoning_effort,
    })))
}

/// Turns thinking off on the session's model when it allows that, otherwise keeps its default.
/// 会话模型允许时关闭思考，否则保留其默认值。
fn session_model_reasoning_effort(capabilities: &ModelCapabilities) -> Option<ReasoningEffort> {
    validate_exact_wire_reasoning_effort(capabilities, ReasoningEffortLevel::None)
        .unwrap_or_else(|_| resolve_effective_public_reasoning_effort(capabilities, None))
}

/// Asks the model for a title and applies it while the initial title is still in place.
/// 向模型请求标题，并在初始标题仍在时应用它。
///
/// # Returns
/// 返回值
/// The applied title, `None` when the session was renamed meanwhile, or the failure.
/// 返回已应用的标题；会话在此期间被重命名时返回 `None`；否则返回失败。
async fn run_generation(
    state: &SharedServiceBridgeState,
    prepared: PreparedGeneration,
) -> Result<Option<String>, String> {
    let PreparedGeneration {
        request,
        selection,
        reasoning_effort,
    } = prepared;
    let service = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|runtime| std::sync::Arc::clone(&runtime.configured_inference_service))
        .ok_or_else(|| String::from("Configured inference runtime is still starting."))?;
    let execution_request = AiExecutionRequest {
        operation: AiOperation::ConversationRespond,
        model_id: selection.model.id.clone(),
        cache_scope_id: None,
        continuation: None,
        messages: vec![
            text_message(
                MessageRole::System,
                vulcan_runtime_assets::SESSION_TITLE_GENERATOR_MD,
            ),
            text_message(MessageRole::User, &title_request_message(&request.prompt)),
        ],
        tools: Vec::new(),
        native_tools: Vec::new(),
        tool_choice: ToolChoicePolicy::Disabled,
        parallel_tool_calls: ParallelToolCallPolicy::Disabled,
        structured_output: StructuredOutputPolicy::None,
        stop_sequences: Vec::new(),
        prompt_cache: selection.overrides.prompt_cache.clone().unwrap_or_default(),
        reasoning_effort,
        reasoning_output: ReasoningOutputPolicy::ProviderDefault,
        model_parameters: selection.overrides.model_parameters.clone(),
        resources: Vec::new(),
        output_token_limit: Some(crate::configured_inference::SESSION_TITLE_OUTPUT_TOKEN_LIMIT),
    };
    let output = service
        .execute_exact_selection_text(
            crate::configured_inference::ExactSelectionTextPurpose::SessionTitle,
            &selection,
            execution_request,
            uuid::Uuid::new_v4().to_string(),
            None,
        )
        .await
        .map_err(|error| error.to_string())?;
    let title = sanitize_generated_title(&output)
        .ok_or_else(|| String::from("The model returned an empty title."))?;
    if title == request.initial_title {
        return Ok(Some(title));
    }
    let renamed = crate::project_conversation_services::rename_session_to_generated_title(
        state.clone(),
        request.session_id,
        request.initial_title,
        title.clone(),
    )
    .await?;
    Ok(renamed.map(|_| title))
}

/// Wraps the start of the first prompt and the title limit for the model.
/// 为模型包装首个提示词的开头与标题长度上限。
fn title_request_message(prompt: &str) -> String {
    let excerpt = prompt
        .trim()
        .chars()
        .take(SESSION_TITLE_PROMPT_MAX_CHARS)
        .collect::<String>();
    format!(
        "<first_message>\n{excerpt}\n</first_message>\n\nTitle limit: {SESSION_TITLE_MAX_CHARS} characters."
    )
}

/// Reduces model output to one title line within the session title limit.
/// 将模型输出收敛为不超过会话标题上限的单行标题。
///
/// # Returns
/// 返回值
/// The title, or `None` when nothing usable remains.
/// 返回标题；没有可用内容时返回 `None`。
fn sanitize_generated_title(output: &str) -> Option<String> {
    const QUOTES: &[char] = &[
        '"', '\'', '`', '“', '”', '‘', '’', '「', '」', '『', '』', '《', '》',
    ];
    const TRAILING: &[char] = &[
        '.', '。', '!', '！', '?', '？', ',', '，', ';', '；', ':', '：', '、',
    ];
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let line = line.trim_start_matches(['#', '*', '-', ' ']).trim();
    let line = line
        .split_once(':')
        .filter(|(prefix, _)| prefix.trim().eq_ignore_ascii_case("title"))
        .map_or(line, |(_, rest)| rest.trim());
    let line = line.trim_matches(QUOTES).trim_matches('*').trim();
    let line = line.trim_end_matches(TRAILING).trim_matches(QUOTES).trim();
    let title = line
        .chars()
        .take(SESSION_TITLE_MAX_CHARS)
        .collect::<String>()
        .trim()
        .to_string();
    (!title.is_empty()).then_some(title)
}

/// Builds one plain-text message.
/// 构建一条纯文本消息。
fn text_message(role: MessageRole, text: &str) -> AiMessage {
    AiMessage {
        role,
        content: vec![ContentItem::Text {
            text: text.to_string(),
        }],
        provider_context_items: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies output is reduced to its first non-empty line without wrapping or end punctuation.
    /// 验证输出收敛为首个非空行，并去掉包裹符号与结尾标点。
    #[test]
    fn sanitized_title_keeps_one_bare_line() {
        assert_eq!(
            sanitize_generated_title("\n  \"Fix login redirect.\"\nextra").as_deref(),
            Some("Fix login redirect")
        );
        assert_eq!(
            sanitize_generated_title("Title: **Refactor parser**").as_deref(),
            Some("Refactor parser")
        );
        assert_eq!(
            sanitize_generated_title("「Fix login redirect」。").as_deref(),
            Some("Fix login redirect")
        );
    }

    /// Verifies empty output yields no title and long output stays within the limit.
    /// 验证空输出不产生标题，过长输出保持在上限以内。
    #[test]
    fn sanitized_title_rejects_empty_and_caps_length() {
        assert_eq!(sanitize_generated_title("  \n\"\"\n"), None);
        let long = "a".repeat(SESSION_TITLE_MAX_CHARS * 2);
        assert_eq!(
            sanitize_generated_title(&long).map(|title| title.chars().count()),
            Some(SESSION_TITLE_MAX_CHARS)
        );
    }

    /// Verifies the request carries the prompt as tagged data and caps its length.
    /// 验证请求以标签包裹提示词数据并限制其长度。
    #[test]
    fn request_message_wraps_and_caps_the_prompt() {
        let message = title_request_message(&"x".repeat(SESSION_TITLE_PROMPT_MAX_CHARS + 10));
        assert!(message.starts_with("<first_message>\n"));
        assert_eq!(
            message.matches('x').count(),
            SESSION_TITLE_PROMPT_MAX_CHARS,
            "the excerpt should stop at the prompt cap"
        );
    }
}
