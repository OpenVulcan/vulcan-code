// Mirror of `service/subagent-models-service.ts`.
// `service/subagent-models-service.ts` 的镜像。

use serde_json::{Value, json};

use super::{ServiceCall, Slot};

// A system list change also changes what a session that follows it shows.
// 系统清单变化也会改变跟随它的会话所显示的内容。
const SUBAGENT_MODELS: &[Slot] = &[Slot::SubagentModels, Slot::SessionSubagentModels];

/// Builds the request for `service_subagent_models`.
/// 构建 `service_subagent_models` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_subagent_models() -> ServiceCall {
    ServiceCall::load(
        "load_subagent_models",
        "service_subagent_models",
        json!({}),
        Slot::SubagentModels,
    )
}

/// Builds the request for `service_save_subagent_models`; sessions with their own list keep it.
/// 构建 `service_save_subagent_models` 的请求；拥有自有清单的会话保留其清单。
/// `models` is the complete `[{ providerId, modelId, note }]` list.
/// `models` 为完整的 `[{ providerId, modelId, note }]` 清单。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_subagent_models(models: Value, include_session_model: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_subagent_models",
        "service_save_subagent_models",
        json!({ "request": { "models": models, "includeSessionModel": include_session_model } }),
        SUBAGENT_MODELS,
    )
}

/// Builds the request for `service_session_subagent_models`.
/// 构建 `service_session_subagent_models` 的请求。
/// `session_id` is the saved session whose list is read.
/// `session_id` 为读取其清单的已保存会话。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_session_subagent_models(session_id: &str) -> ServiceCall {
    ServiceCall::load(
        "load_session_subagent_models",
        "service_session_subagent_models",
        json!({ "sessionId": session_id }),
        Slot::SessionSubagentModels,
    )
}

/// Builds the request for `service_save_session_subagent_models`.
/// 构建 `service_save_session_subagent_models` 的请求。
/// `models` is the session's complete list, or `Value::Null` to follow the system list.
/// `models` 为会话完整清单；为 `Value::Null` 时跟随系统清单。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_session_subagent_models(
    session_id: &str,
    models: Value,
    include_session_model: Value,
) -> ServiceCall {
    ServiceCall::mutate(
        "save_session_subagent_models",
        "service_save_session_subagent_models",
        json!({ "request": { "sessionId": session_id, "models": models, "includeSessionModel": include_session_model } }),
        &[Slot::SessionSubagentModels],
    )
}
