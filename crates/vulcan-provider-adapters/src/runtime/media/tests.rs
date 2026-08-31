//! Media admission, scalar cache, and cancelled-upload progress regressions.
//! 媒体准入、标量缓存与上传取消进度回归测试。

use super::*;
use std::sync::Mutex as StdMutex;

/// Verifies the shared inline threshold applies to every upload-capable media modality without inventing channels.
/// 验证共享内联阈值适用于每种具备上传能力的媒体模态，且不会虚构上传通道。
#[test]
fn media_understanding_upload_policy_is_modality_and_threshold_exact() {
    assert!(media_understanding_uses_provider_upload(
        ResourceKind::Video,
        true,
        1
    ));
    assert!(!media_understanding_uses_provider_upload(
        ResourceKind::Video,
        false,
        u64::MAX
    ));
    assert!(!media_understanding_uses_provider_upload(
        ResourceKind::Image,
        true,
        crate::PREFERRED_INLINE_MEDIA_MAX_BYTES
    ));
    assert!(media_understanding_uses_provider_upload(
        ResourceKind::Image,
        true,
        crate::PREFERRED_INLINE_MEDIA_MAX_BYTES + 1
    ));
    assert!(media_understanding_uses_provider_upload(
        ResourceKind::Audio,
        true,
        crate::PREFERRED_INLINE_MEDIA_MAX_BYTES + 1
    ));
}

/// Verifies collection media bindings bypass only the scalar cache and remain valid for generic upload materialization.
/// 验证集合媒体绑定仅绕过标量缓存，且仍可由通用上传物化器正常处理。
#[test]
fn media_understanding_collection_upload_binding_bypasses_scalar_cache() {
    let scalar = crate::OperationUploadBinding {
        source_path: String::from("/video_path"),
        target_path: String::from("/video"),
        item_source_path: None,
        item_target_path: None,
        item_match_path: None,
        item_match_value: None,
        upload_channel_refs: vec![String::from("provider-files")],
        mime_type: Some(String::from("video/mp4")),
        mime_types_by_extension: Default::default(),
        required: true,
        remove_source: true,
        target_encoding: Default::default(),
    };
    let collection = crate::OperationUploadBinding {
        source_path: String::from("/references"),
        target_path: String::from("/references"),
        item_source_path: Some(String::from("/path")),
        item_target_path: Some(String::from("/resource")),
        ..scalar.clone()
    };

    assert!(provider_tool_media_upload_binding_is_cacheable(&scalar));
    assert!(!provider_tool_media_upload_binding_is_cacheable(
        &collection
    ));
}

/// Verifies dropping an active upload emits cancellation with the latest byte count.
/// 验证丢弃进行中的上传会使用最近字节数发出取消终态。
#[test]
fn media_upload_terminal_guard_emits_cancelled_on_drop() {
    // observed owns the complete ordered event sequence emitted by the test context.
    // observed 持有测试上下文发出的完整有序事件序列。
    let observed = Arc::new(StdMutex::new(Vec::new()));
    let observed_for_callback = Arc::clone(&observed);
    let context = RuntimeMediaUploadProgressContext {
        observer: Arc::new(move |event| {
            observed_for_callback
                .lock()
                .expect("progress event lock")
                .push(event);
        }),
        base: crate::RuntimeMediaUploadProgressEvent {
            transfer_id: String::from("transfer-one"),
            owner_session_id: String::from("session-one"),
            provider_id: String::from("provider-one"),
            service_entry_id: String::from("entry-one"),
            connection_id: String::from("connection-one"),
            model_id: String::from("model-one"),
            protocol_profile_id: String::from("provider-tool"),
            manifest_revision: 1,
            manifest_generation: 2,
            route_revision: String::from("route-one"),
            operation_id: String::from("video-understanding"),
            upload_channel_id: String::from("video-file"),
            file_name: String::from("clip.mp4"),
            media_kind: String::from("video"),
            phase: crate::RuntimeMediaUploadPhase::Preparing,
            bytes_sent: 0,
            total_bytes: Some(10),
            error: None,
        },
        last_bytes_sent: Arc::new(AtomicU64::new(0)),
    };
    {
        // guard remains armed while one uploading event advances the retained byte counter.
        // guard 在一条上传事件推进保留字节计数时保持启用。
        let _guard = RuntimeMediaUploadTerminalGuard::new(Some(&context));
        context.emit(crate::RuntimeMediaUploadPhase::Uploading, 7, Some(10), None);
    }

    let events = observed.lock().expect("progress event lock");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].phase, crate::RuntimeMediaUploadPhase::Uploading);
    assert_eq!(events[1].phase, crate::RuntimeMediaUploadPhase::Cancelled);
    assert_eq!(events[1].bytes_sent, 7);
}
