//! Answer mode visibility across failed and successful physical publications.
//! 回答模式跨失败与成功物理发布的可见性。

use super::*;
use vulcan_session_core::v1::config::{AnswerMode, PermissionMode};

/// Failed storage emits no confirmed preference; a successful retry publishes the exact candidate.
/// 存储失败不发送已确认偏好；成功重试发布精确候选。
#[test]
fn answer_mode_live_preference_waits_for_actual_durable_publication() {
    let recording = Arc::new(RecordingPersistenceBackend::default());
    let backend = Arc::new(FailThenPersistBackend {
        remaining_failures: AtomicUsize::new(1),
        delegate: Arc::clone(&recording),
    });
    let handle = published_handle(backend);
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .unwrap();
    let result = handle.mutate_durable_if_changed(
        SessionMutationAdmission::MetadataOnly,
        SessionFlushReason::MetadataChanged,
        |document| {
            // Start from an explicit standard mode so the switch is a real change.
            // 从显式标准模式开始，使切换成为真实变化。
            document.metadata.config.answer_mode.mode = AnswerMode::Standard;
            assert!(document.set_answer_mode(AnswerMode::Interactive, 0, 2)?);
            Ok(SessionMutationChange::Changed(()))
        },
    );
    assert!(result.is_err());
    assert_eq!(recording.flush_count(), 0);
    assert!(sink.events().iter().all(|event| matches!(event,
        ServiceEvent::ConversationEventDeltaV1(delta)
        if delta.durability == ConversationEventPatchDurabilityV1::Volatile && delta.answer_mode.is_none()
    )));
    sink.clear();
    handle.flush_all().unwrap();
    assert_eq!(recording.flush_count(), 1);
    let events = sink.events();
    let [ServiceEvent::ConversationEventDeltaV1(delta)] = events.as_slice() else {
        panic!("expected one successful durable confirmation");
    };
    assert_eq!(
        delta.durability,
        ConversationEventPatchDurabilityV1::Durable
    );
    let answer_mode = delta.answer_mode.as_ref().unwrap();
    assert_eq!(answer_mode.mode, AnswerMode::Interactive);
    assert_eq!(answer_mode.revision, 1);
    assert!(delta.events().is_empty());
}

/// A permission change publishes the exact session access with its durable confirmation.
/// 权限修改随可靠确认发布精确的会话授权。
#[test]
fn permission_mode_durable_confirmation_carries_session_access() {
    let recording = Arc::new(RecordingPersistenceBackend::default());
    let handle = published_handle(recording.clone());
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .unwrap();
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.metadata.mode.access.permission = PermissionMode::WorkspaceWrite;
                assert!(document.set_permission_mode(PermissionMode::AutoApprove, 2)?);
                Ok(SessionMutationChange::Changed(()))
            },
        )
        .unwrap();
    let durable_access: Vec<_> = sink
        .events()
        .into_iter()
        .filter_map(|event| match event {
            ServiceEvent::ConversationEventDeltaV1(delta)
                if delta.durability == ConversationEventPatchDurabilityV1::Durable =>
            {
                delta.access.clone()
            }
            _ => None,
        })
        .collect();
    let [access] = durable_access.as_slice() else {
        panic!("expected one durable confirmation carrying session access");
    };
    assert_eq!(access.permission_mode, PermissionMode::AutoApprove);
}
