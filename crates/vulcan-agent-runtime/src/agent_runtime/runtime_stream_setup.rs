//! Runtime stream startup-event projection helpers for the outer execution shell.
//! 面向外层执行壳的运行时流启动事件投影辅助模块。

use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::model::SessionId;
use vulcan_context::RuntimeProviderLoweringRepairEvent;

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
    lowering_repair_events: Vec<RuntimeProviderLoweringRepairEvent>,
) -> Vec<ExecutionStreamEvent> {
    let mut events = Vec::with_capacity(discovery_events.len() + lowering_repair_events.len() + 2);
    events.push(ExecutionStreamEvent::Prepared(preparation.clone()));
    events.push(ExecutionStreamEvent::RuntimeEvent {
        session_id,
        event: build_runtime_context_event(preparation),
    });
    events.extend(
        lowering_repair_events
            .into_iter()
            .map(|event| ExecutionStreamEvent::ProviderLoweringRepair { session_id, event }),
    );
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
    use uuid::Uuid;

    /// Builds one minimal execution preparation used by startup-batch tests.
    /// 构建一份供启动批次测试使用的最小执行准备对象。
    fn build_test_preparation() -> ExecutionPreparation {
        ExecutionPreparation {
            session_id: Uuid::nil(),
            conversation_hook_turn_id: None,
            session_title: String::from("测试会话"),
            agent_id: String::from("default-agent"),
            agent_name: String::from("默认代理"),
            agent_kind: String::from("内置"),
            agent_prompt_snapshot: None,
            subagent_prompt_context: None,
            model_blueprint: ProviderRuntimeBlueprint {
                provider_id: String::from("openai"),
                service_entry_id: String::from("public-api"),
                connection_id: String::from("test-connection"),
                provider_name: String::from("OpenAI"),
                runtime_label: String::from("OpenAI"),
                model_id: String::from("gpt-test"),
                model_name: String::from("GPT Test"),
                protocol_profile_id: String::from(vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID),
                manifest_revision: 0,
                frozen_selection: None,
                route_revision: String::from("sha256:test-route"),
                secondary_route: Default::default(),
                public_capabilities: vulcan_ai_contract::ModelCapabilities::default(),
                base_url: Some(String::from("https://api.example.test/v1")),
                supports_remote_model_refresh: true,
                credential_source: String::from("环境变量"),
                reasoning: Default::default(),
            },
            prompt_preview: String::from("请分析代码"),
            workspace_path: String::from(r"D:\workspace"),
            task_session_store_root: None,
            conversation_attachments: Vec::new(),
            context_history_count: 2,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            prompt_configuration: crate::provider_config::PersistedPromptConfiguration::default(),
            output_style_snapshot: None,
            prompt_purpose: crate::agent_runtime::ExecutionPromptPurpose::UserTurn,
            prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            implicit_luaskill_prompt_entries: Vec::new(),
            prompt_skill_developer_segment: None,
            skill_names: Vec::new(),
            tool_descriptors: Vec::new(),
            tool_request_rejection_reason: None,
            tool_request_rejection_all_tools: false,
            tool_request_rejection_tool_ids: Vec::new(),
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            provider_wire_request_json: None,
            context_budget: None,
            context_compaction_policy: None,
            context_budget_override: None,
            provider_usage_budget_baseline: None,
            provider_usage_context_generation_id: None,
            context_assembly: None,
            context_baseline: None,
            context_request_mode: None,
            context_injection: None,
            system_prompt_hook_blocks: Vec::new(),
            project_vmm: Default::default(),
            session_vmm: Default::default(),
            boundary_events: Vec::new(),
            runtime_system_updates: Vec::new(),
            session_goal_context: None,
            session_plan_context: None,
            embedded_turn_usage_segments: Vec::new(),
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

        let events = build_runtime_start_events(
            session_id,
            &preparation,
            vec![discovery_event.clone()],
            Vec::new(),
        );

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

    /// Verifies startup events include provider lowering repair audits discovered during bootstrap.
    /// 验证启动事件会包含 bootstrap 期间发现的 Provider 转换修复审计。
    #[test]
    fn start_events_include_lowering_repair_audits() {
        let preparation = build_test_preparation();
        let session_id = Uuid::nil();
        let repair = RuntimeProviderLoweringRepairEvent {
            turn_id: Some(String::from("turn-1")),
            repair_id: String::from("repair-1"),
            provider_id: String::from("minimax-cn-coding-plan"),
            model_id: String::from("MiniMax-M3"),
            provider_family: String::from("anthropic-messages"),
            action: String::from("fallback"),
            reason: String::from("system_update_lowered_to_user_scoped_wrapper"),
            affected_items: 1,
        };

        let events = build_runtime_start_events(session_id, &preparation, Vec::new(), vec![repair]);

        assert!(matches!(
            &events[2],
            ExecutionStreamEvent::ProviderLoweringRepair { event, .. }
                if event.reason == "system_update_lowered_to_user_scoped_wrapper"
        ));
    }
}
