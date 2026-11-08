//! Dedicated sessions (专职助手): memory lock transitions between turns and base prompt drafting.
//! 专职会话（专职助手）：回合之间的记忆锁定切换与基础提示词起草。

use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionMutationAdmission, SessionMutationChange, SessionRuntimeError,
    SessionRuntimeRegistry,
};
use crate::{SessionId, SharedServiceBridgeState, current_v1_unix_millis, parse_session_id};
use vulcan_agent_runtime::agent_runtime::provider_reasoning::{
    resolve_effective_public_reasoning_effort, validate_exact_wire_reasoning_effort,
};
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiOperation, ContentItem, MessageRole, ParallelToolCallPolicy,
    ReasoningOutputPolicy, StructuredOutputPolicy, ToolChoicePolicy,
};
use vulcan_service_api::{
    DedicatedMemoryLockRequest, DedicatedMemoryLockResponse, DedicatedPromptDraftProgressEvent,
    DedicatedPromptDraftRequest, DedicatedPromptDraftResponse, ServiceEvent, ServiceEventEmitter,
    WorkbenchSnapshot,
};
use vulcan_session_core::v1::{DEDICATED_BASE_PROMPT_MAX_CHARS, DomainError};
use vulcan_workspace_store::data_plane::HostRuntimeHandle;
use vulcan_workspace_store::workspace_store::{
    SESSION_ASSISTANT_NAME_MAX_CHARS, set_session_assistant_v1,
};

/// Locks or unlocks dedicated-session memory through the unique session writer.
/// 通过唯一会话写入器锁定或解锁专职会话记忆。
///
/// # Parameters
/// 参数
/// `state` supplies indexed session ownership; `request` names the session and target state.
/// `state` 提供索引会话归属；`request` 指定会话与目标状态。
///
/// # Returns
/// 返回值
/// Returns the durable profile and the appended note seq, or a stable error code when the
/// session is not dedicated or a turn is still open.
/// 返回可靠配置与追加的通知 seq；会话不是专职会话或仍有开放回合时返回稳定错误码。
pub(crate) async fn service_set_dedicated_memory_lock(
    state: SharedServiceBridgeState,
    request: DedicatedMemoryLockRequest,
) -> Result<DedicatedMemoryLockResponse, String> {
    let session_id = parse_session_id(&request.session_id)
        .ok_or_else(|| String::from("Invalid dedicated session identity."))?;
    let registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("Workspace is not ready."))?;
        if !crate::workspace_snapshot_contains_regular_session(workspace, session_id) {
            return Err(String::from(
                "Dedicated memory lock requires an active regular session.",
            ));
        }
        std::sync::Arc::clone(&guard.session_runtime_registry)
    };
    let _lifecycle = registry
        .acquire_lifecycle_transition(session_id)
        .await
        .map_err(|error| error.to_string())?;
    // Checked under the lifecycle transition so a concurrent conversion cannot slip between.
    // 在生命周期切换内检查，避免并发转换插入其间。
    if !request.locked && session_is_assistant(&state, session_id)? {
        return Err(String::from(
            "ERR_DEDICATED_ASSISTANT_UNLOCK_REFUSED: restore the assistant to a session before unlocking its memory.",
        ));
    }
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(&state, session_id).await?;
    let now_ms = current_v1_unix_millis()?;
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::DisplayBoundary,
            |document| {
                let note_seq = document.set_dedicated_memory_locked(request.locked, now_ms)?;
                let profile = document
                    .dedicated_profile()
                    .cloned()
                    .ok_or(DomainError::NotDedicatedSession)?;
                let value = (document.segment_generation(), note_seq, profile);
                Ok(if note_seq.is_some() {
                    SessionMutationChange::Changed(value)
                } else {
                    SessionMutationChange::Unchanged(value)
                })
            },
        )
        .map_err(|error| dedicated_lock_error_code(&error))?;
    Ok(DedicatedMemoryLockResponse {
        session_id: session_id.to_string(),
        window_generation: receipt.value.0,
        revision: receipt.revision,
        note_seq: receipt.value.1,
        dedicated: receipt.value.2,
    })
}

/// Converts one memory-locked dedicated session into a named floating assistant.
/// 将一个记忆已锁定的专职会话转换为具名浮动助手。
///
/// # Parameters
/// 参数
/// `conversation_id` names an active regular session; `name` is the requested display name.
/// `conversation_id` 指定一个活动常规会话；`name` 是请求的显示名称。
///
/// # Returns
/// 返回值
/// Returns the reloaded workbench, or a stable error code when the name is invalid, the session
/// is not dedicated, or its memory is not locked.
/// 返回重新加载的工作台；名称无效、会话不是专职会话或记忆未锁定时返回稳定错误码。
pub(crate) async fn service_convert_conversation_to_assistant(
    state: SharedServiceBridgeState,
    conversation_id: String,
    name: String,
) -> Result<WorkbenchSnapshot, String> {
    let name = normalized_assistant_name(&name)?;
    let owner = assistant_session_owner(&state, &conversation_id)?;
    let lifecycle = owner
        .registry
        .acquire_lifecycle_transition(owner.session_id)
        .await
        .map_err(|error| error.to_string())?;
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(&state, owner.session_id)
            .await?;
    let locked = handle
        .read(|document| {
            document
                .dedicated_profile()
                .map(|profile| profile.is_locked())
        })
        .map_err(|error| error.to_string())?;
    match locked {
        None => {
            return Err(String::from(
                "ERR_DEDICATED_SESSION_REQUIRED: only dedicated sessions can become assistants.",
            ));
        }
        Some(false) => {
            return Err(String::from(
                "ERR_DEDICATED_ASSISTANT_REQUIRES_LOCK: lock the memory before converting the session into an assistant.",
            ));
        }
        Some(true) => {}
    }
    set_session_assistant_v1(
        &owner.controller_runtime,
        &owner.session_id.to_string(),
        Some(&name),
    )
    .await
    .map_err(|error| format!("Failed to convert the session into an assistant: {error}"))?;
    // The refresh takes the selected session's lifecycle guard, which is usually this session.
    // 刷新会获取选中会话的生命周期守卫，而选中会话通常就是本会话。
    drop(lifecycle);
    crate::refresh_workbench_snapshot(state).await
}

/// Returns one floating assistant to the ordinary session list.
/// 将一个浮动助手还原到普通会话列表。
///
/// # Parameters
/// 参数
/// `conversation_id` names the active regular session behind the assistant.
/// `conversation_id` 指定助手背后的活动常规会话。
///
/// # Returns
/// 返回值
/// Returns the reloaded workbench; restoring a session that is not an assistant changes nothing.
/// 返回重新加载的工作台；还原非助手会话不产生变化。
pub(crate) async fn service_restore_assistant_conversation(
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let owner = assistant_session_owner(&state, &conversation_id)?;
    let lifecycle = owner
        .registry
        .acquire_lifecycle_transition(owner.session_id)
        .await
        .map_err(|error| error.to_string())?;
    set_session_assistant_v1(
        &owner.controller_runtime,
        &owner.session_id.to_string(),
        None,
    )
    .await
    .map_err(|error| format!("Failed to restore the assistant to a session: {error}"))?;
    // Released before the refresh, which takes the selected session's lifecycle guard.
    // 在刷新前释放，因为刷新会获取选中会话的生命周期守卫。
    drop(lifecycle);
    crate::refresh_workbench_snapshot(state).await
}

/// Trims one assistant name and enforces the stored length bound.
/// 去除助手名称首尾空白并执行存储长度上限。
fn normalized_assistant_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(String::from(
            "ERR_DEDICATED_ASSISTANT_NAME_REQUIRED: the assistant name is empty.",
        ));
    }
    if trimmed.chars().count() > SESSION_ASSISTANT_NAME_MAX_CHARS {
        return Err(format!(
            "ERR_DEDICATED_ASSISTANT_NAME_TOO_LONG: the assistant name exceeds {SESSION_ASSISTANT_NAME_MAX_CHARS} characters."
        ));
    }
    Ok(trimmed.to_string())
}

/// Owners an assistant transition needs for one active regular session.
/// 助手切换针对一个活动常规会话所需的归属对象。
struct AssistantSessionOwner {
    /// Exact session identity.
    /// 精确会话身份。
    session_id: SessionId,
    /// Runtime owning the application SQLite binding.
    /// 持有应用 SQLite 绑定的运行时。
    controller_runtime: HostRuntimeHandle,
    /// Registry serializing lifecycle transitions of the session.
    /// 串行化会话生命周期切换的注册表。
    registry: std::sync::Arc<SessionRuntimeRegistry>,
}

/// Resolves the owners of an active regular session for an assistant transition.
/// 为助手切换解析活动常规会话的归属对象。
fn assistant_session_owner(
    state: &SharedServiceBridgeState,
    conversation_id: &str,
) -> Result<AssistantSessionOwner, String> {
    let session_id = parse_session_id(conversation_id)
        .ok_or_else(|| String::from("Invalid assistant session identity."))?;
    let guard = state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?;
    let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
        String::from("Service is still starting; controller runtime is not ready.")
    })?;
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("Workspace is not ready."))?;
    if !crate::workspace_snapshot_contains_regular_session(workspace, session_id) {
        return Err(String::from(
            "Assistant conversion requires an active regular session.",
        ));
    }
    Ok(AssistantSessionOwner {
        session_id,
        controller_runtime,
        registry: std::sync::Arc::clone(&guard.session_runtime_registry),
    })
}

/// Returns whether the retained workspace lists the session as a floating assistant.
/// 返回保留的工作区是否将该会话列为浮动助手。
fn session_is_assistant(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<bool, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?;
    Ok(guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| crate::workspace_session_summary_for_session(workspace, session_id))
        .is_some_and(|summary| summary.assistant_name.is_some()))
}

/// Maps a lock failure onto a stable code the clients can localize, keeping the detail.
/// 将锁定失败映射为客户端可本地化的稳定错误码，并保留细节。
fn dedicated_lock_error_code(error: &SessionRuntimeError) -> String {
    let code = match error {
        SessionRuntimeError::Domain(domain) => match domain.as_ref() {
            DomainError::NotDedicatedSession => "ERR_DEDICATED_SESSION_REQUIRED",
            DomainError::DedicatedMemoryLockWhileTurnOpen { .. } => {
                "ERR_DEDICATED_MEMORY_LOCK_TURN_OPEN"
            }
            _ => "ERR_DEDICATED_MEMORY_LOCK_FAILED",
        },
        _ => "ERR_DEDICATED_MEMORY_LOCK_FAILED",
    };
    format!("{code}: {error}")
}

/// Drafts or revises a dedicated-session base prompt on the exact model the user selected.
/// 在用户所选的精确模型上起草或修订专职会话基础提示词。
///
/// # Parameters
/// 参数
/// `state` owns the AI runtime; `request` freezes the route and the user's material.
/// `state` 持有 AI 运行时；`request` 冻结路由与用户提供的材料。
///
/// # Returns
/// 返回值
/// Returns the trimmed prompt, or a stable error code when the input is empty or too long, or the
/// output exceeds the base prompt limit. The route is never substituted.
/// 返回去除首尾空白的提示词；输入为空或过长、输出超过基础提示词上限时返回稳定错误码。绝不替换路由。
pub(crate) async fn service_draft_dedicated_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: DedicatedPromptDraftRequest,
) -> Result<DedicatedPromptDraftResponse, String> {
    let user_message = dedicated_prompt_draft_message(&request)?;
    let progress_id = dedicated_prompt_draft_progress_id(request.progress_id.as_deref())?;
    let (runtime, configuration) =
        crate::public_ai_settings::public_ai_runtime_and_configuration(&state)?;
    let selection = runtime.resolve_connection_model_selection(
        &configuration,
        request.connection_id.as_str(),
        request.provider_id.as_str(),
        request.model_id.as_str(),
    )?;
    let reasoning_effort = dedicated_prompt_draft_reasoning_effort(
        &selection.model.capabilities,
        request.reasoning_effort.as_deref(),
    )?;
    let service = state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|runtime| std::sync::Arc::clone(&runtime.configured_inference_service))
        .ok_or_else(|| String::from("Configured inference runtime is still starting."))?;
    let execution_id = uuid::Uuid::new_v4().to_string();
    let execution_request = AiExecutionRequest {
        operation: AiOperation::ConversationRespond,
        model_id: selection.model.id.clone(),
        // The draft belongs to no session, so it is its own scope: providers that route by a
        // session header or derive prefix identity from the cache scope reject a request without one.
        // 起草不属于任何会话，因此以自身为作用域：按会话请求头路由或从缓存作用域派生前缀标识的
        // 供应商会拒绝没有作用域的请求。
        cache_scope_id: Some(execution_id.clone()),
        continuation: None,
        messages: vec![
            text_message(
                MessageRole::System,
                vulcan_runtime_assets::DEDICATED_PROMPT_GENERATOR_MD,
            ),
            text_message(MessageRole::User, &user_message),
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
        output_token_limit: Some(
            crate::configured_inference::DEDICATED_PROMPT_DRAFT_OUTPUT_TOKEN_LIMIT,
        ),
    };
    let progress = progress_id.map(|progress_id| DraftProgressEmitter::new(app, progress_id));
    let report = progress.as_ref().map(|progress| {
        move |reasoning: &str, output: &str| progress.observe(reasoning, output, false)
    });
    let result = service
        .execute_exact_selection_text(
            crate::configured_inference::ExactSelectionTextPurpose::DedicatedPromptDraft,
            &selection,
            execution_request,
            execution_id,
            report.as_ref().map(|report| {
                report as &crate::configured_inference::ExactSelectionTextProgress<'_>
            }),
        )
        .await;
    if let Some(progress) = &progress {
        progress.flush();
    }
    let prompt = result.map_err(|error| format!("ERR_DEDICATED_PROMPT_DRAFT_FAILED: {error}"))?;
    if prompt.chars().count() > DEDICATED_BASE_PROMPT_MAX_CHARS {
        return Err(String::from("ERR_DEDICATED_PROMPT_DRAFT_TOO_LONG"));
    }
    Ok(DedicatedPromptDraftResponse {
        prompt,
        provider_id: request.provider_id,
        model_id: request.model_id,
    })
}

/// Longest accepted client progress id.
/// 可接受的客户端进度 id 最大长度。
const DEDICATED_PROMPT_DRAFT_PROGRESS_ID_MAX_CHARS: usize = 128;

/// Minimum spacing between two progress events of one draft request.
/// 同一起草请求两次进度事件之间的最小间隔。
const DEDICATED_PROMPT_DRAFT_PROGRESS_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(150);

/// Validates the optional client progress id; absent or blank disables progress events.
/// 校验可选的客户端进度 id；为空或空白时不推送进度事件。
fn dedicated_prompt_draft_progress_id(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.chars().count() > DEDICATED_PROMPT_DRAFT_PROGRESS_ID_MAX_CHARS
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(String::from(
            "ERR_DEDICATED_PROMPT_DRAFT_PROGRESS_ID_INVALID",
        ));
    }
    Ok(Some(value.to_string()))
}

/// Resolves the drafting thinking level: an explicit level must be one the model declares,
/// and an absent level uses the model's own default, exactly as a conversation turn would.
/// 解析起草所用思考档位：显式档位必须是模型声明的档位；未指定时与会话回合一样使用模型自身默认档位。
fn dedicated_prompt_draft_reasoning_effort(
    capabilities: &vulcan_ai_contract::ModelCapabilities,
    requested: Option<&str>,
) -> Result<Option<vulcan_ai_contract::ReasoningEffort>, String> {
    let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(resolve_effective_public_reasoning_effort(
            capabilities,
            None,
        ));
    };
    let level = crate::workspace_persistence_versions::parse_reasoning_effort(requested)
        .ok_or_else(|| format!("ERR_DEDICATED_PROMPT_DRAFT_REASONING_INVALID: {requested}"))?;
    validate_exact_wire_reasoning_effort(capabilities, level)
        .map_err(|error| format!("ERR_DEDICATED_PROMPT_DRAFT_REASONING_INVALID: {error}"))
}

/// Turns accumulated draft text into throttled progress deltas for one client request.
/// 将累计的起草文本转换为单个客户端请求的节流进度增量。
struct DraftProgressEmitter {
    app: ServiceEventEmitter,
    progress_id: String,
    state: std::sync::Mutex<DraftProgressState>,
}

/// Byte offsets already sent and the time of the last event.
/// 已发送的字节偏移与上次事件时间。
struct DraftProgressState {
    reasoning_sent: usize,
    output_sent: usize,
    last_emit: Option<std::time::Instant>,
    reasoning: String,
    output: String,
}

impl DraftProgressEmitter {
    fn new(app: ServiceEventEmitter, progress_id: String) -> Self {
        Self {
            app,
            progress_id,
            state: std::sync::Mutex::new(DraftProgressState {
                reasoning_sent: 0,
                output_sent: 0,
                last_emit: None,
                reasoning: String::new(),
                output: String::new(),
            }),
        }
    }

    /// Records the accumulated text and emits the unsent tail when due or when `force` is set.
    /// 记录累计文本；到期或 `force` 为真时发送尚未发送的尾部。
    fn observe(&self, reasoning: &str, output: &str, force: bool) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.reasoning.len() != reasoning.len() {
            state.reasoning = reasoning.to_string();
        }
        if state.output.len() != output.len() {
            state.output = output.to_string();
        }
        self.emit_due(&mut state, force);
    }

    /// Sends whatever has not been sent yet.
    /// 发送全部尚未发送的内容。
    fn flush(&self) {
        if let Ok(mut state) = self.state.lock() {
            self.emit_due(&mut state, true);
        }
    }

    fn emit_due(&self, state: &mut DraftProgressState, force: bool) {
        let pending =
            state.reasoning.len() > state.reasoning_sent || state.output.len() > state.output_sent;
        let due = state
            .last_emit
            .is_none_or(|last| last.elapsed() >= DEDICATED_PROMPT_DRAFT_PROGRESS_INTERVAL);
        if !pending || !(force || due) {
            return;
        }
        let reasoning_delta = state
            .reasoning
            .get(state.reasoning_sent..)
            .unwrap_or_default()
            .to_string();
        let output_delta = state
            .output
            .get(state.output_sent..)
            .unwrap_or_default()
            .to_string();
        state.reasoning_sent = state.reasoning.len();
        state.output_sent = state.output.len();
        state.last_emit = Some(std::time::Instant::now());
        self.app
            .emit(ServiceEvent::DedicatedPromptDraftProgress(Box::new(
                DedicatedPromptDraftProgressEvent {
                    progress_id: self.progress_id.clone(),
                    reasoning_delta,
                    output_delta,
                },
            )));
    }
}

/// Wraps the user's material in the data tags the generator prompt names.
/// 用生成器提示词约定的数据标签包裹用户材料。
fn dedicated_prompt_draft_message(request: &DedicatedPromptDraftRequest) -> Result<String, String> {
    let requirement = request.requirement.trim();
    let current_prompt = request
        .current_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let revision_request = request
        .revision_request
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if requirement.is_empty() && current_prompt.is_none() {
        return Err(String::from("ERR_DEDICATED_PROMPT_DRAFT_EMPTY"));
    }
    if [Some(requirement), current_prompt, revision_request]
        .into_iter()
        .flatten()
        .any(|text| text.chars().count() > DEDICATED_BASE_PROMPT_MAX_CHARS)
    {
        return Err(String::from("ERR_DEDICATED_PROMPT_DRAFT_INPUT_TOO_LONG"));
    }
    let mut message = format!("<requirement>\n{requirement}\n</requirement>");
    if let Some(current_prompt) = current_prompt {
        message.push_str(&format!(
            "\n\n<current_prompt>\n{current_prompt}\n</current_prompt>"
        ));
    }
    if let Some(revision_request) = revision_request {
        message.push_str(&format!(
            "\n\n<revision_request>\n{revision_request}\n</revision_request>"
        ));
    }
    Ok(message)
}

/// Builds one text-only message.
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

    /// Builds a draft request with the given material.
    /// 以给定材料构建起草请求。
    fn draft_request(
        requirement: &str,
        current_prompt: Option<&str>,
        revision_request: Option<&str>,
    ) -> DedicatedPromptDraftRequest {
        DedicatedPromptDraftRequest {
            provider_id: String::from("provider"),
            connection_id: String::from("connection"),
            model_id: String::from("model"),
            requirement: requirement.to_string(),
            current_prompt: current_prompt.map(str::to_string),
            revision_request: revision_request.map(str::to_string),
            reasoning_effort: None,
            progress_id: None,
        }
    }

    /// Verifies the draft message carries only present material and rejects empty or oversized input.
    /// 验证起草消息只携带存在的材料，并拒绝空输入或超长输入。
    #[test]
    fn draft_message_tags_present_material() {
        let fresh =
            dedicated_prompt_draft_message(&draft_request("translate", None, None)).unwrap();
        assert!(fresh.contains("<requirement>"));
        assert!(!fresh.contains("<current_prompt>"));
        let revision = dedicated_prompt_draft_message(&draft_request(
            "",
            Some("You translate."),
            Some("also Italian"),
        ))
        .unwrap();
        assert!(revision.contains("<current_prompt>"));
        assert!(revision.contains("<revision_request>"));
        assert_eq!(
            dedicated_prompt_draft_message(&draft_request("  ", Some(" "), None)).unwrap_err(),
            "ERR_DEDICATED_PROMPT_DRAFT_EMPTY"
        );
        let oversized = "x".repeat(DEDICATED_BASE_PROMPT_MAX_CHARS + 1);
        assert_eq!(
            dedicated_prompt_draft_message(&draft_request(&oversized, None, None)).unwrap_err(),
            "ERR_DEDICATED_PROMPT_DRAFT_INPUT_TOO_LONG"
        );
    }

    /// Verifies progress ids are optional and only accept short identifier characters.
    /// 验证进度 id 可省略，且只接受短标识符字符。
    #[test]
    fn draft_progress_id_accepts_only_identifier_characters() {
        assert_eq!(dedicated_prompt_draft_progress_id(None), Ok(None));
        assert_eq!(dedicated_prompt_draft_progress_id(Some("  ")), Ok(None));
        assert_eq!(
            dedicated_prompt_draft_progress_id(Some("draft-1_a")),
            Ok(Some(String::from("draft-1_a")))
        );
        for invalid in ["draft 1", "draft/1"] {
            assert_eq!(
                dedicated_prompt_draft_progress_id(Some(invalid)),
                Err(String::from(
                    "ERR_DEDICATED_PROMPT_DRAFT_PROGRESS_ID_INVALID"
                ))
            );
        }
        let oversized = "a".repeat(DEDICATED_PROMPT_DRAFT_PROGRESS_ID_MAX_CHARS + 1);
        assert!(dedicated_prompt_draft_progress_id(Some(&oversized)).is_err());
    }
}
