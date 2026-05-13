//! Runtime stream launch payload model and bootstrap-to-launch assembly helpers.
//! 面向运行时流启动载荷模型与 bootstrap 到 launch 组装转换的辅助模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::events::RuntimeEvent;
use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionPreparationRequest};
use crate::agent_runtime::runtime_stream_bootstrap::RuntimeStreamBootstrap;
use crate::agent_runtime::runtime_stream_delta::RuntimeStreamDeltaState;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::model::SessionId;
use std::sync::Arc;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage,
    ConversationSnapshot as CoreConversationSnapshot,
};
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

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
    /// Shared resumable task runtime reused by the current execution turn.
    /// 被当前执行回合复用的共享可恢复任务运行时。
    pub task_runtime: Arc<TaskRuntime>,
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
    pub request_history: Vec<CoreConversationMessage>,
    /// Canonical conversation snapshot handed to `vulcan-core`.
    /// 交给 `vulcan-core` 的规范会话快照。
    pub core_snapshot: CoreConversationSnapshot,
    /// Mutable streaming-loop state seeded from the bootstrapped core controller.
    /// 由已启动核心控制器播种得到的可变流式循环状态。
    pub stream_state: RuntimeStreamDeltaState,
}

/// Builds the final runtime-stream launch payload from one execution request and one completed bootstrap payload.
/// 根据一份执行请求与一份完成后的 bootstrap 载荷构建最终运行时流启动载荷。
pub(crate) fn build_runtime_stream_launch_payload(
    request: &ExecutionPreparationRequest,
    bootstrap: RuntimeStreamBootstrap,
) -> RuntimeStreamLaunch {
    RuntimeStreamLaunch {
        session_id: request.session_id,
        effective_prompt: request.prompt.trim().to_string(),
        preparation: bootstrap.preparation,
        active_agent: bootstrap.active_agent,
        task_runtime: bootstrap.task_runtime,
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
    }
}

#[cfg(test)]
mod tests {
    //! Launch-payload assembly tests for runtime stream orchestration.
    //! 面向运行时流编排的启动载荷组装测试。

    use super::*;
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use crate::conversation_protocol::SessionConversationState;
    use crate::provider_config::ProviderRuntimeKind;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use uuid::Uuid;
    use vulcan_core::controller::VulcanCoreController;
    use vulcan_core::types::{
        ControllerConfig, ConversationMessage as CoreConversationMessage,
        ConversationPayload as CoreConversationPayload,
    };

    /// Builds one minimal execution preparation used by launch-payload tests.
    /// 构建一份供启动载荷测试使用的最小执行准备对象。
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
                reasoning: Default::default(),
            },
            prompt_preview: String::from("请分析代码"),
            workspace_path: String::from("D:/workspace"),
            context_history_count: 1,
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

    /// Builds one minimal execution request used by launch-payload tests.
    /// 构建一份供启动载荷测试使用的最小执行请求。
    fn build_test_request() -> ExecutionPreparationRequest {
        ExecutionPreparationRequest {
            session_id: Uuid::nil(),
            session_title: String::from("测试会话"),
            workspace_path: String::from("D:/workspace"),
            prompt: String::from("  请分析主入口文件  "),
            conversation_state: SessionConversationState::default(),
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            reasoning_effort: crate::provider_config::ReasoningEffortLevel::Off,
        }
    }

    /// Builds one minimal canonical snapshot used to verify controller seeding during launch assembly.
    /// 构建一份最小规范快照，用于验证启动组装期间的控制器播种行为。
    fn build_test_core_snapshot() -> CoreConversationSnapshot {
        CoreConversationSnapshot {
            messages: vec![CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("历史用户输入"),
                },
            }],
            ..CoreConversationSnapshot::default()
        }
    }

    /// Builds one minimal bootstrap payload used by launch-payload tests.
    /// 构建一份供启动载荷测试使用的最小 bootstrap 载荷。
    fn build_test_bootstrap() -> RuntimeStreamBootstrap {
        let core_snapshot = build_test_core_snapshot();
        let core_controller = VulcanCoreController::new(
            ControllerConfig {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-test"),
                system_prompt: String::from("system"),
                temperature: Some(0.2),
                max_turns: 4,
                additional_params: None,
            },
            core_snapshot.clone(),
        );

        RuntimeStreamBootstrap {
            preparation: build_test_preparation(),
            active_agent: None,
            task_runtime: Arc::new(TaskRuntime::new()),
            tools_manager: VulcanToolsManager::default(),
            resolved_tool_definitions: Vec::new(),
            discovery_events: Vec::new(),
            discovery_rows: vec![String::from("mcp-row")],
            discovery_errors: vec![String::from("mcp-error")],
            warnings: vec![String::from("warning")],
            system_preamble: String::from("system preamble"),
            request_history: Vec::new(),
            core_snapshot,
            core_controller,
        }
    }

    /// Trims the effective prompt and preserves the bootstrapped controller snapshot inside the launch delta state.
    /// 裁剪生效提示词，并在启动增量状态中保留已 bootstrap 的控制器快照。
    #[test]
    fn launch_payload_assembly_trims_prompt_and_preserves_controller_seed() {
        let request = build_test_request();
        let launch = build_runtime_stream_launch_payload(&request, build_test_bootstrap());

        assert_eq!(launch.session_id, request.session_id);
        assert_eq!(launch.effective_prompt, "请分析主入口文件");
        assert_eq!(launch.discovery_rows, vec![String::from("mcp-row")]);
        assert_eq!(launch.discovery_errors, vec![String::from("mcp-error")]);
        assert_eq!(launch.warnings, vec![String::from("warning")]);
        assert_eq!(launch.core_snapshot.messages.len(), 1);
        assert_eq!(
            launch.stream_state.controller().snapshot().messages.len(),
            1
        );
        assert!(launch.stream_state.controller().events().is_empty());
    }
}
