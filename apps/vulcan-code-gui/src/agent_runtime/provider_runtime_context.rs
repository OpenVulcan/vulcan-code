//! Provider runtime context helpers for system preamble and turn-budget resolution.
//! 面向系统前导语与轮次预算解析的 Provider 运行时上下文辅助模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::agent_runtime::provider_prompt_loader::load_provider_base_system_prompt;

/// Builds the effective runtime system prompt sent to the real provider agent.
/// 构建发送给真实 Provider 代理的生效运行时系统提示词。
pub(crate) fn build_runtime_system_preamble(preparation: &ExecutionPreparation) -> String {
    let mut sections = Vec::new();
    sections.push(
        load_provider_base_system_prompt(&preparation.model_blueprint)
            .unwrap_or_else(|| String::from("You are VulcanCode, an AI coding agent.")),
    );
    sections.push(format!("Current workspace: {}", preparation.workspace_path));
    sections.push(format!(
        "Current execution mode: {}",
        preparation.execution_mode
    ));

    sections.join("\n")
}

/// Resolves one optional max-turn override for multi-turn tool-calling providers.
/// 为支持多轮工具调用的 Provider 解析一份可选的最大轮次覆盖值。
pub(crate) fn resolve_runtime_max_turns(
    active_agent: Option<&RuntimeAgentDefinition>,
) -> Option<usize> {
    active_agent
        .and_then(|agent| agent.max_turns)
        .filter(|value| *value > 0)
        .map(|value| value as usize)
}
