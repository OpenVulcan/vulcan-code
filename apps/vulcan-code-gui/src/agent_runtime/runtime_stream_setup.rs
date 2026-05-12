//! Runtime streaming bootstrap helpers for preparation, tool resolution, and request snapshot seeding.
//! 面向准备阶段、工具解析与请求快照播种的运行时流式启动辅助模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::execution_models::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionRunReport,
};
use crate::agent_runtime::execution_preparation::prepare_execution;
use crate::agent_runtime::mcp_runtime_projection::collect_enabled_mcp_runtime_events;
use crate::agent_runtime::provider_request_bridge::build_core_conversation_snapshot;
use crate::agent_runtime::provider_request_snapshot::{
    build_core_controller_config, build_core_provider_request_snapshot,
    build_provider_request_preview_markdown,
};
use crate::agent_runtime::provider_runtime_context::{
    build_runtime_system_preamble, resolve_runtime_max_turns,
};
use crate::agent_runtime::provider_runtime_inputs::resolve_provider_additional_params;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::agent_runtime::runtime_stream_delta::RuntimeStreamDeltaState;
use crate::conversation_protocol::conversation_messages_to_rig_history;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeHandle;
use crate::model::SessionId;
use crate::provider_config::ProviderConfigurationFile;
use crate::vulcan_tools_bridge::build_vulcan_tools_manager;
use rig::message::Message as RigMessage;
use std::path::PathBuf;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::types::ConversationSnapshot as CoreConversationSnapshot;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Bootstrap payload that captures the fully prepared runtime turn before provider streaming begins.
/// 在 Provider 流式执行开始前捕获完整运行时回合准备结果的启动载荷。
pub(crate) struct RuntimeStreamBootstrap {
    /// Updated execution preparation after request-snapshot seeding.
    /// 完成请求快照播种之后的更新执行准备对象。
    pub preparation: ExecutionPreparation,
    /// Active agent definition resolved from the runtime foundation for this turn.
    /// 为当前回合从运行时基础架构解析得到的激活代理定义。
    pub active_agent: Option<RuntimeAgentDefinition>,
    /// Host-owned unified tool manager used by the transport layer.
    /// 供传输层使用的宿主自有统一工具管理器。
    pub tools_manager: VulcanToolsManager,
    /// Final resolved tool definitions exposed to the provider for this turn.
    /// 当前回合向 Provider 暴露的最终已解析工具定义集合。
    pub resolved_tool_definitions: Vec<ResolvedToolDefinition>,
    /// Discovery events projected from enabled MCP runtime servers.
    /// 从已启用 MCP 运行时服务器投影得到的发现事件集合。
    pub discovery_events: Vec<RuntimeEvent>,
    /// Human-readable MCP discovery rows captured for the final execution report.
    /// 为最终执行报告捕获的人类可读 MCP 发现摘要行集合。
    pub discovery_rows: Vec<String>,
    /// MCP discovery errors captured for the final execution report.
    /// 为最终执行报告捕获的 MCP 发现错误集合。
    pub discovery_errors: Vec<String>,
    /// Mutable warning list accumulated before provider streaming begins.
    /// 在 Provider 流式执行开始前累计得到的可变警告列表。
    pub warnings: Vec<String>,
    /// Effective system preamble sent to the provider transport layer.
    /// 发送给 Provider 传输层的生效系统前导语。
    pub system_preamble: String,
    /// Provider-visible history reconstructed from canonical conversation messages.
    /// 从规范会话消息重建得到的 Provider 可见历史记录。
    pub request_history: Vec<RigMessage>,
    /// Canonical conversation snapshot handed to `vulcan-core`.
    /// 交给 `vulcan-core` 的规范会话快照。
    pub core_snapshot: CoreConversationSnapshot,
    /// Host-owned controller that tracks streaming deltas and execution events.
    /// 负责跟踪流式增量与执行事件的宿主自有控制器。
    pub core_controller: VulcanCoreController,
}

/// Launch payload that captures everything the outer runtime stream needs after preparation and bootstrap have both succeeded.
/// 捕获准备阶段与启动阶段都成功后外层运行时流所需全部内容的启动载荷。
pub(crate) struct RuntimeStreamLaunch {
    /// Session identifier owned by the current runtime run.
    /// 当前运行时执行所属的会话标识。
    pub session_id: SessionId,
    /// Trimmed effective user prompt that will be sent to the provider transport layer.
    /// 将发送给 Provider 传输层的裁剪后生效用户提示词。
    pub effective_prompt: String,
    /// Updated execution preparation after request-snapshot seeding.
    /// 完成请求快照播种之后的更新执行准备对象。
    pub preparation: ExecutionPreparation,
    /// Active agent definition resolved from the runtime foundation for this turn.
    /// 为当前回合从运行时基础架构解析得到的激活代理定义。
    pub active_agent: Option<RuntimeAgentDefinition>,
    /// Host-owned unified tool manager used by the transport layer.
    /// 供传输层使用的宿主自有统一工具管理器。
    pub tools_manager: VulcanToolsManager,
    /// Final resolved tool definitions exposed to the provider for this turn.
    /// 当前回合向 Provider 暴露的最终已解析工具定义集合。
    pub resolved_tool_definitions: Vec<ResolvedToolDefinition>,
    /// Discovery events projected from enabled MCP runtime servers.
    /// 从已启用 MCP 运行时服务器投影得到的发现事件集合。
    pub discovery_events: Vec<RuntimeEvent>,
    /// Human-readable MCP discovery rows captured for the final execution report.
    /// 为最终执行报告捕获的人类可读 MCP 发现摘要行集合。
    pub discovery_rows: Vec<String>,
    /// MCP discovery errors captured for the final execution report.
    /// 为最终执行报告捕获的 MCP 发现错误集合。
    pub discovery_errors: Vec<String>,
    /// Mutable warning list accumulated before provider streaming begins.
    /// 在 Provider 流式执行开始前累计得到的可变警告列表。
    pub warnings: Vec<String>,
    /// Effective system preamble sent to the provider transport layer.
    /// 发送给 Provider 传输层的生效系统前导语。
    pub system_preamble: String,
    /// Provider-visible history reconstructed from canonical conversation messages.
    /// 从规范会话消息重建得到的 Provider 可见历史记录。
    pub request_history: Vec<RigMessage>,
    /// Canonical conversation snapshot handed to `vulcan-core`.
    /// 交给 `vulcan-core` 的规范会话快照。
    pub core_snapshot: CoreConversationSnapshot,
    /// Mutable streaming-loop state seeded from the bootstrapped core controller.
    /// 由已启动核心控制器播种得到的可变流式循环状态。
    pub stream_state: RuntimeStreamDeltaState,
}

/// Builds one fully prepared runtime-stream launch payload or one terminal preparation-failure report.
/// 构建一份完全就绪的运行时流启动载荷，或者生成一份终态准备失败报告。
pub(crate) async fn prepare_runtime_stream_launch(
    config: &ProviderConfigurationFile,
    agent_profile_catalog: &AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeHandle>,
    request: ExecutionPreparationRequest,
) -> Result<RuntimeStreamLaunch, ExecutionRunReport> {
    let foundation = RuntimeFoundation::build(
        config,
        agent_profile_catalog,
        luaskills_runtime.clone(),
        mcp_runtime.clone(),
    );
    let preparation = match prepare_execution(
        &foundation,
        luaskills_runtime.clone(),
        mcp_runtime.clone(),
        config,
        &request,
    ) {
        Ok(preparation) => preparation,
        Err(error) => {
            return Err(ExecutionRunReport::failure(
                request.session_id,
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                error,
            ));
        }
    };

    let bootstrap = bootstrap_runtime_stream(
        &foundation,
        luaskills_runtime,
        mcp_runtime,
        &request,
        preparation,
    )
    .await;

    Ok(RuntimeStreamLaunch {
        session_id: request.session_id,
        effective_prompt: request.prompt.trim().to_string(),
        preparation: bootstrap.preparation,
        active_agent: bootstrap.active_agent,
        tools_manager: bootstrap.tools_manager,
        resolved_tool_definitions: bootstrap.resolved_tool_definitions,
        discovery_events: bootstrap.discovery_events,
        discovery_rows: bootstrap.discovery_rows,
        discovery_errors: bootstrap.discovery_errors,
        warnings: bootstrap.warnings,
        system_preamble: bootstrap.system_preamble,
        request_history: bootstrap.request_history,
        core_snapshot: bootstrap.core_snapshot,
        stream_state: RuntimeStreamDeltaState::new(bootstrap.core_controller),
    })
}

/// Builds the full bootstrap payload required before provider streaming starts.
/// 构建 Provider 流式执行开始前所需的完整启动载荷。
pub(crate) async fn bootstrap_runtime_stream(
    foundation: &RuntimeFoundation,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeHandle>,
    request: &ExecutionPreparationRequest,
    mut preparation: ExecutionPreparation,
) -> RuntimeStreamBootstrap {
    let active_agent = foundation.active_agent_definition().cloned();
    let active_agent_tool_id = active_agent
        .as_ref()
        .map(|agent| agent.id.clone())
        .unwrap_or_else(|| preparation.agent_id.clone());
    let tools_manager = build_vulcan_tools_manager(
        foundation.native_tool_catalog().clone(),
        luaskills_runtime,
        mcp_runtime.clone(),
        (!preparation.workspace_path.is_empty())
            .then(|| PathBuf::from(preparation.workspace_path.clone())),
        foundation.resolved_native_tools_for_agent(&active_agent_tool_id),
        foundation.resolved_luaskills_tools_for_agent(&active_agent_tool_id),
        foundation.resolved_mcp_tools_for_agent(&active_agent_tool_id),
    );
    let resolved_tools = tools_manager.resolve_for_execution_mode(&preparation.execution_mode);
    let (discovery_events, discovery_rows, discovery_errors, discovery_warnings) =
        collect_enabled_mcp_runtime_events(mcp_runtime.as_ref());
    let mut warnings = preparation.warnings.clone();
    warnings.extend(discovery_warnings);

    let system_preamble = build_runtime_system_preamble(&preparation);
    let request_history =
        conversation_messages_to_rig_history(&request.conversation_state.messages);
    let core_snapshot = build_core_conversation_snapshot(&request.conversation_state);
    let core_controller = VulcanCoreController::new(
        build_core_controller_config(
            &preparation,
            &system_preamble,
            resolve_runtime_max_turns(active_agent.as_ref()),
            resolve_provider_additional_params(&preparation.model_blueprint),
        ),
        core_snapshot.clone(),
    );

    match build_core_provider_request_snapshot(
        &preparation,
        &request.conversation_state,
        &system_preamble,
        request.prompt.trim(),
        &resolved_tools.definitions,
        resolve_runtime_max_turns(active_agent.as_ref()),
    )
    .await
    {
        Ok(request_snapshot) => {
            preparation.provider_request_preview_markdown =
                build_provider_request_preview_markdown(&preparation, &request_snapshot);
            preparation.provider_request_snapshot_json =
                serde_json::to_value(&request_snapshot).ok();
        }
        Err(error) => {
            preparation.provider_request_preview_markdown =
                format!("# 最后一次真实 API 请求快照\n\n请求快照构建失败：{}", error);
            preparation.provider_request_snapshot_json = None;
            warnings.push(format!("请求快照构建失败：{error}"));
        }
    }

    RuntimeStreamBootstrap {
        preparation,
        active_agent,
        tools_manager,
        resolved_tool_definitions: resolved_tools.definitions,
        discovery_events,
        discovery_rows,
        discovery_errors,
        warnings,
        system_preamble,
        request_history,
        core_snapshot,
        core_controller,
    }
}

/// Builds the standard runtime-context event emitted right after preparation succeeds.
/// 构建在准备成功后立即发出的标准运行时上下文事件。
pub(crate) fn build_runtime_context_event(preparation: &ExecutionPreparation) -> RuntimeEvent {
    RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "执行上下文",
        format!(
            "已进入真实执行阶段，目标模型为 `{}`，工作区为 `{}`。",
            preparation.model_blueprint.model_name, preparation.workspace_path
        ),
    )
}

/// Builds the initial execution event batch emitted immediately after bootstrap succeeds.
/// 构建在启动阶段成功后立即发出的初始执行事件批次。
pub(crate) fn build_runtime_start_events(
    session_id: SessionId,
    preparation: &ExecutionPreparation,
    discovery_events: Vec<RuntimeEvent>,
) -> Vec<ExecutionStreamEvent> {
    let mut events = Vec::with_capacity(discovery_events.len() + 2);
    events.push(ExecutionStreamEvent::Prepared(preparation.clone()));
    events.push(ExecutionStreamEvent::RuntimeEvent {
        session_id,
        event: build_runtime_context_event(preparation),
    });
    events.extend(
        discovery_events
            .into_iter()
            .map(|event| ExecutionStreamEvent::RuntimeEvent { session_id, event }),
    );
    events
}

#[cfg(test)]
mod tests {
    //! Startup-batch tests for runtime stream bootstrap event projection.
    //! 面向运行时流式启动事件投影的启动批次测试。

    use super::*;
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use crate::provider_config::ProviderRuntimeKind;
    use serde_json::json;
    use std::collections::BTreeMap;
    use uuid::Uuid;

    /// Builds one minimal execution preparation used by startup-batch tests.
    /// 构建一份供启动批次测试使用的最小执行准备对象。
    fn build_test_preparation() -> ExecutionPreparation {
        ExecutionPreparation {
            session_id: Uuid::nil(),
            session_title: String::from("测试会话"),
            agent_id: String::from("default-agent"),
            agent_name: String::from("默认代理"),
            agent_kind: String::from("内置"),
            model_blueprint: ProviderRuntimeBlueprint {
                provider_id: String::from("openai"),
                provider_name: String::from("OpenAI"),
                runtime_kind: ProviderRuntimeKind::OpenAi,
                runtime_label: String::from("OpenAI"),
                model_id: String::from("gpt-test"),
                model_name: String::from("GPT Test"),
                base_url: Some(String::from("https://api.example.test/v1")),
                supports_remote_model_refresh: true,
                credential_source: String::from("环境变量"),
                user_agent: None,
                extra_headers: BTreeMap::new(),
                extra_body: json!({}),
            },
            prompt_preview: String::from("请分析代码"),
            workspace_path: String::from("D:/workspace"),
            context_history_count: 2,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            skill_names: Vec::new(),
            tool_descriptors: Vec::new(),
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
        }
    }

    /// Preserves the startup event order as prepared, runtime-context, and discovery events.
    /// 保持启动事件顺序为准备完成、运行时上下文与发现事件。
    #[test]
    fn runtime_start_events_preserve_prepared_context_and_discovery_order() {
        let preparation = build_test_preparation();
        let session_id = Uuid::nil();
        let discovery_event = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "MCP 发现",
            "发现 1 个 MCP 服务",
        );

        let events =
            build_runtime_start_events(session_id, &preparation, vec![discovery_event.clone()]);

        assert!(matches!(
            events.as_slice(),
            [
                ExecutionStreamEvent::Prepared(prepared),
                ExecutionStreamEvent::RuntimeEvent { event: context_event, .. },
                ExecutionStreamEvent::RuntimeEvent { event: projected_discovery, .. },
            ] if prepared.session_title == "测试会话"
                && context_event.title == "执行上下文"
                && projected_discovery.title == discovery_event.title
        ));
    }
}
