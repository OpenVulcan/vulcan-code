//! Shared session-context assembly and its read-only inspector entry.
//! 共享会话上下文组装及其只读检查入口。

use super::{
    CompactionNode, DedicatedSessionProfile, LockedBaselineProjection, ProjectedItem, Seq,
    SessionRuntimeError, SourcedProjectedItem, TurnId, compaction_continuation,
    dedicated_provider_events, project_context_generation_items_with_sources,
};
use serde_json::{Value, json};
use vulcan_agent_runtime::agent_runtime::{
    config::AgentRuntimeConfig, provider_runtime_context::compose_conversation_prompt_preview,
};
use vulcan_session_core::v1::event::{AssistantSemanticStatus, ConversationEvent, EventBody};
use vulcan_workspace_store::workspace_store::{
    RequestContextAssistantResponse, SessionV1StorageLocator, load_session_v1_full,
    render_simulated_request_context,
};

mod tools;
pub(crate) use tools::preview_session_tool_definitions;

/// Returns the finalized visible assistant text belonging to `api_call_id` in the resident event tail.
/// 返回常驻事件尾段中属于 `api_call_id` 的已完成可见 assistant 正文。
/// Stops at the request boundary to avoid opening sealed history or borrowing text from another call.
/// 在请求边界停止，不打开已封存历史或借用其他调用的正文。
pub(crate) fn latest_context_assistant_response(
    events: &[ConversationEvent],
    api_call_id: u64,
) -> Option<RequestContextAssistantResponse> {
    // Only completed reasoning before the selected answer belongs to its prefix.
    // 只有所选回答之前已经完成的思考属于其前缀。
    let mut response = None;
    // The V1 API group stores one accumulated reasoning event before the final answer.
    // V1 API 组在最终回答之前存储一条累积思考事件。
    let mut reasoning_prefix = None;
    for event in events.iter().rev() {
        match &event.event {
            EventBody::AssistantText(text)
                if text.api_call_id.value() == api_call_id
                    && text.status == AssistantSemanticStatus::Completed
                    && !text.text.is_empty()
                    && response.is_none() =>
            {
                response = Some(text.text.as_str());
            }
            EventBody::AssistantReasoning(reasoning)
                if response.is_some()
                    && reasoning.api_call_id.value() == api_call_id
                    && reasoning.status == AssistantSemanticStatus::Completed
                    && !reasoning.text.trim().is_empty() =>
            {
                reasoning_prefix = Some(reasoning.text.as_str());
                break;
            }
            EventBody::ApiStarted(started) if started.api_call_id.value() == api_call_id => break,
            _ => {}
        }
    }
    response
        .map(|text| RequestContextAssistantResponse::from_text(text, reasoning_prefix.as_slice()))
}

/// Assembles the history used for both physical dispatch and simulation.
/// 组装物理派发与模拟共用的历史。
/// The supplied frontier determines retention and system-state backfill; a summary adds its exact continuation.
/// 给定前沿决定保留范围与系统状态回填；摘要会追加其精确续接内容。
pub(super) fn assemble_sourced_request_history(
    events: &[ConversationEvent],
    dedicated: Option<&DedicatedSessionProfile>,
    open_turn: Option<TurnId>,
    retained_from: Seq,
    instruction_frontier: Option<Seq>,
    summary: Option<&CompactionNode>,
) -> Result<Vec<SourcedProjectedItem>, SessionRuntimeError> {
    // Dedicated memory rules own which events are eligible for model projection.
    // 专职记忆规则决定哪些事件可进入模型投影。
    let dedicated_events =
        dedicated.map(|profile| dedicated_provider_events(profile, events, open_turn));
    let mut items = project_context_generation_items_with_sources(
        dedicated_events.as_deref().unwrap_or(events),
        retained_from,
        instruction_frontier,
    )?;
    if let Some(summary) = summary {
        compaction_continuation::insert_summary_continuation(&mut items, summary)?;
    }
    Ok(items)
}

/// Loads the latest published V1 generation at `locator` and returns its simulated logical context.
/// 读取 `locator` 的最新已发布 V1 代际，返回逻辑上下文模拟结果。
/// `workspace_path` and `agent_config` resolve the local prompt sources used for a conversation.
/// `workspace_path` 与 `agent_config` 解析会话使用的本地提示词来源。
/// This entry stops before dispatch preparation, which would acquire credentials and commit an API admission.
/// 此入口止于派发准备之前；后者会取得凭据并提交 API 准入。
pub(crate) fn simulate_request_context(
    locator: &SessionV1StorageLocator,
    workspace_path: &str,
    agent_config: &AgentRuntimeConfig,
    preview_tools: impl FnOnce(
        &vulcan_workspace_store::workspace_store::LoadedSessionV1,
    ) -> Result<Vec<Value>, String>,
) -> Result<Value, String> {
    // A single storage load binds event history, generation and frozen settings even when no live session exists.
    // 单次存储加载绑定事件历史、代际和冻结的设置，即使会话没有实时所有者。
    let loaded = load_session_v1_full(locator).map_err(|error| error.to_string())?;
    let document = &loaded.document;
    // The current compaction pointer is the sole authority for the already published context boundary.
    // 当前压缩指针是已发布上下文边界的唯一权威。
    let summary = document
        .runtime_lock
        .current_compaction_id
        .map(|id| {
            document
                .compactions
                .iter()
                .find(|node| node.compaction_id == id)
                .ok_or_else(|| {
                    String::from("Runtime compaction pointer has no matching durable node.")
                })
        })
        .transpose()?;
    let retained_from = match summary {
        Some(node) => node.retained_from_seq,
        None => Seq::try_new(1).map_err(|error| error.to_string())?,
    };
    let assembled = assemble_sourced_request_history(
        document.events(),
        document.dedicated_profile(),
        document
            .active_turn()
            .filter(|turn| !turn.closed)
            .map(|turn| turn.turn_id),
        retained_from,
        summary.map(|node| node.api_stats_seq),
        summary,
    )
    .map_err(|error| error.to_string())?;

    // Editable foundations resolve at inspection time and precede the published baseline by role.
    // 可编辑的基础提示词在检查时解析，按角色放在已发布的锁定基线前。
    let saved_baseline =
        LockedBaselineProjection::from_initialized_runtime_lock(&document.runtime_lock)
            .map_err(|error| error.to_string())?;
    let (fixed, baseline) = compose_conversation_prompt_preview(
        workspace_path,
        agent_config,
        document.dedicated_profile(),
        loaded.metadata.child_execution_profile.as_ref(),
        saved_baseline,
    )?;
    let (messages, origins) =
        crate::agent_stream::provider_v1_sink::preview_request_prefix(&fixed, &baseline)
            .map_err(|error| error.to_string())?;
    // Private replay envelopes are transport inputs, not logical context display nodes.
    // 私有重放封套属于传输输入，不属于逻辑上下文展示节点。
    let conversation = assembled
        .into_iter()
        .map(|entry| {
            let mut item = entry.item;
            if let ProjectedItem::AssistantText {
                provider_source, ..
            } = &mut item
            {
                *provider_source = None;
            }
            item
        })
        .collect::<Vec<_>>();
    // An unresolved model or tool route still leaves the saved conversation inspectable.
    // 模型或工具路由无法解析时，已保存的会话仍然可以检查。
    let tool_definitions = preview_tools(&loaded);
    // Keep the setup rows and tool schemas together under the common inspector bounds.
    // 将初始提示词与工具 Schema 放在同一个检查面板预算下。
    let body = json!({
        "baseline": messages,
        "conversation": conversation,
        "tools": tool_definitions.as_ref().ok(),
    });
    // Tool preparation failure is distinct from an intentionally empty tool list.
    // 工具准备失败与按配置禁用所有工具是不同的状态。
    let mut rows = serde_json::to_value(render_simulated_request_context(&body, &origins))
        .map_err(|error| error.to_string())?;
    if let Err(error) = tool_definitions {
        tracing::warn!("Request context tool preview failed: {error}");
        rows["toolsUnavailable"] = Value::Bool(true);
    }
    Ok(rows)
}
