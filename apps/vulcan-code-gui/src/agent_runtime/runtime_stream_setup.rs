//! Runtime stream startup-event projection helpers for the outer execution shell.
//! 面向外层执行壳的运行时流启动事件投影辅助模块。

use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::model::SessionId;

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
                openrouter_endpoint_route: None,
                base_url: Some(String::from("https://api.example.test/v1")),
                supports_remote_model_refresh: true,
                credential_source: String::from("环境变量"),
                user_agent: None,
                extra_headers: BTreeMap::new(),
                extra_body: json!({}),
                reasoning: Default::default(),
                capabilities: Default::default(),
                interleaved_reasoning: None,
            },
            prompt_preview: String::from("请分析代码"),
            workspace_path: String::from(r"D:\workspace"),
            conversation_attachments: Vec::new(),
            context_history_count: 2,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            prompt_configuration: crate::provider_config::PersistedPromptConfiguration::default(),
            prompt_purpose: crate::agent_runtime::ExecutionPromptPurpose::UserTurn,
            prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            implicit_luaskill_prompt_entries: Vec::new(),
            prompt_skill_developer_segment: None,
            skill_names: Vec::new(),
            tool_descriptors: Vec::new(),
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            provider_wire_request_json: None,
            context_budget: None,
            context_assembly: None,
            context_baseline: None,
            context_request_mode: None,
            context_injection: None,
            project_vmm: Default::default(),
            session_vmm: Default::default(),
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
