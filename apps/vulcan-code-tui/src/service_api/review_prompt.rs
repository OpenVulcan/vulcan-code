// Mirror of `service/review-prompt-service.ts`.
// `service/review-prompt-service.ts` 的镜像。

use serde_json::json;

use super::{ServiceCall, Slot};

// A system default change also changes what a session that follows it shows.
// 系统默认值变化也会改变跟随它的会话所显示的内容。
const REVIEW_PROMPT: &[Slot] = &[Slot::ReviewPromptSystemDefault, Slot::SessionReviewPrompt];

/// Builds the request for `service_review_prompt_system_default`.
/// 构建 `service_review_prompt_system_default` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_review_prompt_system_default() -> ServiceCall {
    ServiceCall::load(
        "load_review_prompt_system_default",
        "service_review_prompt_system_default",
        json!({}),
        Slot::ReviewPromptSystemDefault,
    )
}

/// Builds the request for `service_save_review_prompt_system_default`; sessions with their own
/// prompt keep it.
/// 构建 `service_save_review_prompt_system_default` 的请求；拥有自有提示词的会话保留其提示词。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_review_prompt_system_default(prompt: &str) -> ServiceCall {
    ServiceCall::mutate(
        "save_review_prompt_system_default",
        "service_save_review_prompt_system_default",
        json!({ "request": { "prompt": prompt } }),
        REVIEW_PROMPT,
    )
}

/// Builds the request for `service_session_review_prompt`.
/// 构建 `service_session_review_prompt` 的请求。
/// `session_id` is the saved session whose prompt is read.
/// `session_id` 为读取其提示词的已保存会话。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_session_review_prompt(session_id: &str) -> ServiceCall {
    ServiceCall::load(
        "load_session_review_prompt",
        "service_session_review_prompt",
        json!({ "sessionId": session_id }),
        Slot::SessionReviewPrompt,
    )
}

/// Builds the request for `service_save_session_review_prompt`.
/// 构建 `service_save_session_review_prompt` 的请求。
/// `prompt` is the session's own text, or `None` to follow the system default.
/// `prompt` 为会话自有文本；为 `None` 时跟随系统默认值。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_session_review_prompt(session_id: &str, prompt: Option<&str>) -> ServiceCall {
    ServiceCall::mutate(
        "save_session_review_prompt",
        "service_save_session_review_prompt",
        json!({ "request": { "sessionId": session_id, "prompt": prompt } }),
        &[Slot::SessionReviewPrompt],
    )
}
