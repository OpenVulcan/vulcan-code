//! Additional review prompt: text the user writes for the permission reviewer while they are
//! present. The system default lives in the global judge settings; a session may keep its own
//! text beside its V1 document, and child sessions follow their root session.
//! 附加复核提示词：用户为在场时的权限复核器编写的文本。系统默认值保存在全局判定设置中；会话可以在
//! 其 V1 文档旁保存自有文本，子会话跟随其根会话。

use serde::{Deserialize, Serialize};
use vulcan_permission_judge::system::MAX_REVIEW_PROMPT_CHARS;
use vulcan_service_api::{
    ReviewPromptSystemDefaultSaveRequest, ReviewPromptSystemDefaultSnapshot,
    SessionReviewPromptSaveRequest, SessionReviewPromptSnapshot,
};
use vulcan_workspace_store::workspace_store::{
    load_session_review_prompt, save_session_review_prompt,
};

use super::services::{read_state, save_settings};
use crate::SharedServiceBridgeState;
use crate::session_runtime_registry_v1::SessionHandle;

/// Version of the session review prompt file; other versions are refused.
/// 会话复核提示词文件的版本；其他版本一律拒绝。
const SESSION_REVIEW_PROMPT_VERSION: u32 = 1;

/// Session review prompt file stored beside the session document.
/// 保存在会话文档旁的会话复核提示词文件。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionReviewPromptDocument {
    /// File format version.
    /// 文件格式版本。
    version: u32,
    /// The session's own prompt; empty means the session uses none.
    /// 会话自有提示词；为空表示本会话不使用提示词。
    prompt: String,
}

/// The prompt as saved, or why it is too long.
/// 保存形态的提示词；或其过长的原因。
fn normalized(prompt: &str) -> Result<String, String> {
    let prompt = prompt.trim();
    if prompt.chars().count() > MAX_REVIEW_PROMPT_CHARS {
        return Err(format!(
            "The review prompt is longer than {MAX_REVIEW_PROMPT_CHARS} characters."
        ));
    }
    Ok(prompt.to_string())
}

/// Reads one session's own prompt, or `None` when it follows the system default.
/// 读取一个会话的自有提示词；跟随系统默认时返回 `None`。
pub(crate) fn session_review_prompt(handle: &SessionHandle) -> Result<Option<String>, String> {
    let locator = handle
        .storage_locator()
        .map_err(|error| error.to_string())?;
    load_session_review_prompt::<SessionReviewPromptDocument>(&locator)?
        .map(prompt_from_document)
        .transpose()
}

/// Accepts only the current file version.
/// 只接受当前文件版本。
fn prompt_from_document(document: SessionReviewPromptDocument) -> Result<String, String> {
    if document.version != SESSION_REVIEW_PROMPT_VERSION {
        return Err(format!(
            "Session review prompt version {} is not supported.",
            document.version
        ));
    }
    Ok(document.prompt)
}

/// Returns the saved system default prompt.
/// 返回已保存的系统默认提示词。
pub(crate) fn service_review_prompt_system_default(
    state: SharedServiceBridgeState,
) -> Result<ReviewPromptSystemDefaultSnapshot, String> {
    let (_, settings) = read_state(&state)?;
    Ok(ReviewPromptSystemDefaultSnapshot {
        prompt: settings.review_prompt,
        max_chars: MAX_REVIEW_PROMPT_CHARS,
    })
}

/// Saves the system default prompt; sessions without their own prompt read it on the next review.
/// 保存系统默认提示词；没有自有提示词的会话在下一次复核时读取它。
pub(crate) async fn service_save_review_prompt_system_default(
    state: SharedServiceBridgeState,
    request: ReviewPromptSystemDefaultSaveRequest,
) -> Result<ReviewPromptSystemDefaultSnapshot, String> {
    let prompt = normalized(&request.prompt)?;
    let settings = save_settings(&state, |settings| {
        settings.review_prompt = prompt;
        Ok(())
    })
    .await?;
    Ok(ReviewPromptSystemDefaultSnapshot {
        prompt: settings.review_prompt,
        max_chars: MAX_REVIEW_PROMPT_CHARS,
    })
}

/// Projects one session's own prompt next to the system default it would follow.
/// 将一个会话的自有提示词与其将跟随的系统默认值一并投影。
fn session_snapshot(
    state: &SharedServiceBridgeState,
    session_id: String,
    prompt: Option<String>,
) -> Result<SessionReviewPromptSnapshot, String> {
    let (_, settings) = read_state(state)?;
    Ok(SessionReviewPromptSnapshot {
        session_id,
        prompt,
        system_prompt: settings.review_prompt,
        max_chars: MAX_REVIEW_PROMPT_CHARS,
    })
}

/// Returns one active regular session's prompt.
/// 返回一个活动普通会话的提示词。
pub(crate) async fn service_session_review_prompt(
    state: SharedServiceBridgeState,
    session_id: String,
) -> Result<SessionReviewPromptSnapshot, String> {
    let (id, _lifecycle, handle) =
        crate::permission_mode::session_access_writer(&state, &session_id).await?;
    let prompt = session_review_prompt(handle.as_ref())?;
    session_snapshot(&state, id.to_string(), prompt)
}

/// Saves one session's prompt; `None` returns it to the system default. The next review reads it.
/// 保存一个会话的提示词；`None` 使其恢复跟随系统默认值。下一次复核即读取它。
pub(crate) async fn service_save_session_review_prompt(
    state: SharedServiceBridgeState,
    request: SessionReviewPromptSaveRequest,
) -> Result<SessionReviewPromptSnapshot, String> {
    let prompt = request.prompt.as_deref().map(normalized).transpose()?;
    let (id, _lifecycle, handle) =
        crate::permission_mode::session_access_writer(&state, &request.session_id).await?;
    let locator = handle
        .storage_locator()
        .map_err(|error| error.to_string())?;
    let document = prompt.clone().map(|prompt| SessionReviewPromptDocument {
        version: SESSION_REVIEW_PROMPT_VERSION,
        prompt,
    });
    save_session_review_prompt(&locator, document.as_ref())?;
    session_snapshot(&state, id.to_string(), prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the current file version and known fields are accepted.
    /// 只接受当前文件版本与已知字段。
    #[test]
    fn session_review_prompt_accepts_only_the_current_version() {
        let parse =
            |value: serde_json::Value| serde_json::from_value::<SessionReviewPromptDocument>(value);
        let current = parse(serde_json::json!({
            "version": SESSION_REVIEW_PROMPT_VERSION,
            "prompt": "p",
        }))
        .expect("document shape");
        assert_eq!(prompt_from_document(current).as_deref(), Ok("p"));
        let newer = parse(serde_json::json!({
            "version": SESSION_REVIEW_PROMPT_VERSION + 1,
            "prompt": "p",
        }))
        .expect("document shape");
        assert!(prompt_from_document(newer).is_err());
        assert!(
            parse(serde_json::json!({
                "version": SESSION_REVIEW_PROMPT_VERSION,
                "prompt": "p",
                "extra": true,
            }))
            .is_err()
        );
    }

    /// Saved prompts are trimmed and refused past the limit, counted in Unicode scalar values.
    /// 保存的提示词去除首尾空白，超过按 Unicode 标量值计的上限即拒绝。
    #[test]
    fn review_prompt_is_trimmed_and_bounded() {
        assert_eq!(normalized("  p  ").as_deref(), Ok("p"));
        assert!(normalized(&"é".repeat(MAX_REVIEW_PROMPT_CHARS)).is_ok());
        assert!(normalized(&"é".repeat(MAX_REVIEW_PROMPT_CHARS + 1)).is_err());
    }
}
