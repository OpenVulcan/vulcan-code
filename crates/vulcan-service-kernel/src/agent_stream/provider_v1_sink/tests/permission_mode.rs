//! Permission mode notices at actual request preparation boundaries.
//! 实际请求准备边界上的权限模式通知。

use super::super::context_injection::{
    PERMISSION_MODE_CONTEXT_KEY, synchronize_request_dynamic_context,
};
use super::host_guidance::candidate_for;
use super::*;
use vulcan_session_core::v1::config::PermissionMode;

/// Persist one permission mode through the production document owner.
/// 通过生产文档所有者持久化一个权限模式。
fn set_mode(handle: &SessionHandle, mode: PermissionMode, now_ms: i64) {
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.set_permission_mode(mode, now_ms).map(|changed| {
                    if changed {
                        SessionMutationChange::Changed(())
                    } else {
                        SessionMutationChange::Unchanged(())
                    }
                })
            },
        )
        .unwrap();
}

/// Reads the published notice body, when any.
/// 读取已发布的通知正文（如有）。
fn notice(handle: &SessionHandle) -> Option<String> {
    handle
        .read(|document| {
            document
                .metadata
                .dynamic_context
                .get(PERMISSION_MODE_CONTEXT_KEY)
                .map(|state| state.raw_content.clone())
        })
        .unwrap()
}

/// The default mode stays silent; other modes reach the request and later changes replace them.
/// 默认模式保持静默；其他模式进入请求，后续变化替换旧通知。
#[test]
fn permission_mode_notice_is_durable_idempotent_and_follows_changes() {
    let (_directory, handle) = published_stored_open_turn();
    let candidate = candidate_for(&handle);
    let mut facts = request_facts();
    synchronize_request_dynamic_context(&handle, &facts, &candidate).unwrap();
    assert_eq!(notice(&handle), None, "the default mode needs no notice");

    set_mode(&handle, PermissionMode::ReadOnly, 10);
    facts.prepared_at_ms = 11;
    synchronize_request_dynamic_context(&handle, &facts, &candidate).unwrap();
    let read_only = notice(&handle).expect("a non-default mode must reach the request");
    let messages = candidate_for(&handle).exact_request.messages;
    let message = messages
        .iter()
        .find(|message| message.content.iter().any(|item| matches!(item, ContentItem::Text { text } if text.contains(PERMISSION_MODE_CONTEXT_KEY))))
        .expect("the notice must reach the actual request");
    assert_eq!(message.role, MessageRole::User);

    let revision = handle.read(|document| document.memory_revision()).unwrap();
    synchronize_request_dynamic_context(&handle, &facts, &candidate).unwrap();
    assert_eq!(
        handle.read(|document| document.memory_revision()).unwrap(),
        revision,
        "retries must not duplicate the notice"
    );

    set_mode(&handle, PermissionMode::WorkspaceWrite, 12);
    facts.prepared_at_ms = 13;
    synchronize_request_dynamic_context(&handle, &facts, &candidate).unwrap();
    let restored = notice(&handle).expect("returning to the default must replace the notice");
    assert_ne!(restored, read_only);
}
