//! `vulcan-core` controller configuration builders reused by runtime bootstrap and request snapshot assembly.
//! 供运行时 bootstrap 与请求快照组装复用的 `vulcan-core` 控制器配置构建模块。

use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::agent_runtime::provider_runtime_dispatch::DEFAULT_RUNTIME_MAX_TURNS_LIMIT;
use serde_json::Value;
use vulcan_core::types::ControllerConfig as CoreControllerConfig;

/// Builds one `vulcan-core` controller configuration from the current execution preparation.
/// 根据当前执行准备信息构建一份 `vulcan-core` 控制器配置。
pub(crate) fn build_core_controller_config(
    preparation: &ExecutionPreparation,
    system_preamble: &str,
    max_turns_override: Option<usize>,
    additional_params: Option<Value>,
) -> CoreControllerConfig {
    CoreControllerConfig {
        provider_id: preparation.model_blueprint.provider_id.clone(),
        model_id: preparation.model_blueprint.model_id.clone(),
        system_prompt: system_preamble.to_string(),
        temperature: preparation.temperature.map(f64::from),
        max_turns: max_turns_override.unwrap_or(DEFAULT_RUNTIME_MAX_TURNS_LIMIT),
        additional_params,
    }
}
