//! Provider tool change notices at actual request preparation boundaries.
//! 实际请求准备边界上的供应商工具变更通知。

use super::super::context_injection::{
    PROVIDER_TOOLS_CONTEXT_KEY, synchronize_request_dynamic_context,
};
use super::host_guidance::candidate_for;
use super::*;
use vulcan_session_core::v1::config::{
    ProviderExtensionModeConfig, ProviderExtensionModeKind, ProviderToolSelectionConfig,
    ProviderToolTargetConfig,
};

/// Persist one provider tool selection through the production document owner.
/// 通过生产文档所有者持久化一份供应商工具选择。
fn set_tools(handle: &SessionHandle, selection: ProviderToolSelectionConfig, now_ms: i64) {
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .set_provider_tool_selection(selection, now_ms)
                    .map(|changed| {
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
                .get(PROVIDER_TOOLS_CONTEXT_KEY)
                .map(|state| state.raw_content.clone())
        })
        .unwrap()
}

/// Tools chosen before the first request stay silent; a later change publishes one notice that
/// states the complete current configuration and is not duplicated by retries.
/// 首次请求前选择的工具保持静默；之后的修改发布一条陈述完整当前配置的通知，且重试不会重复。
#[test]
fn provider_tool_notice_appears_only_after_mid_session_change() {
    let (_directory, handle) = published_stored_open_turn();
    let fixed = |target_id: &str| ProviderToolSelectionConfig {
        initialized: true,
        overridden: true,
        extension_modes: vec![ProviderExtensionModeConfig {
            capability: String::from("image_generate"),
            mode: ProviderExtensionModeKind::Single,
            single_target: Some(ProviderToolTargetConfig {
                connection_id: String::from("connection-a"),
                target_id: target_id.to_owned(),
            }),
        }],
        ..Default::default()
    };
    set_tools(&handle, fixed("image-a"), 5);
    let candidate = candidate_for(&handle);
    let mut facts = request_facts();
    synchronize_request_dynamic_context(&handle, &facts, &candidate).unwrap();
    assert_eq!(
        notice(&handle),
        None,
        "tools chosen before the first request need no notice"
    );

    set_tools(&handle, fixed("image-b"), 10);
    facts.prepared_at_ms = 11;
    synchronize_request_dynamic_context(&handle, &facts, &candidate).unwrap();
    let body = notice(&handle).expect("a mid-session change must reach the request");
    assert!(body.contains("`image-b`"));
    assert!(!body.contains("`image-a`"));
    assert!(body.is_ascii(), "injected notices are English");

    let revision = handle.read(|document| document.memory_revision()).unwrap();
    synchronize_request_dynamic_context(&handle, &facts, &candidate).unwrap();
    assert_eq!(
        handle.read(|document| document.memory_revision()).unwrap(),
        revision,
        "retries must not duplicate the notice"
    );
}
