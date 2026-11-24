//! Answer mode visibility across failed and successful physical publications.
//! 回答模式跨失败与成功物理发布的可见性。

use super::*;
use vulcan_session_core::v1::config::{AnswerMode, PendingRequestConfig, PermissionMode};

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

/// A model change appears on the durable acknowledgement only after its own failed flush is retried.
/// 模型变更的首次落盘失败后，只有重试成功的持久确认才会携带该变更。
#[test]
fn pending_model_is_published_only_after_its_flush_succeeds() {
    // Reject the initial write to separate the in-memory request from the persisted route.
    // 首次写入拒绝，以区分内存中的请求和持久化路线。
    let handle = published_handle(Arc::new(FailThenPersistBackend {
        remaining_failures: AtomicUsize::new(1),
        delegate: Arc::new(RecordingPersistenceBackend::default()),
    }));
    // Capture the same events dispatched to subscribed clients.
    // 捕获分派给订阅客户端的实际事件。
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .unwrap();
    let result = handle.mutate_durable_if_changed(
        SessionMutationAdmission::MetadataOnly,
        SessionFlushReason::MetadataChanged,
        |document| {
            assert!(document.set_pending_request(
                PendingRequestConfig {
                    provider: Some("provider".into()),
                    connection_id: Some("account".into()),
                    model: Some("next-model".into()),
                    thinking_level: None,
                    temperature: None,
                    set_at_ms: 2,
                },
                2
            )?);
            Ok(SessionMutationChange::Changed(()))
        },
    );
    assert!(result.is_err());
    assert!(sink.events().iter().all(|event| matches!(event,
        ServiceEvent::ConversationEventDeltaV1(delta)
        if delta.durability == ConversationEventPatchDurabilityV1::Volatile && delta.request_configs.is_none()
    )));
    sink.clear();
    handle.flush_all().unwrap();
    // The retry confirms the same credentials and model as the pending configuration.
    // 重试确认与待生效配置一致的账号标识和模型。
    let events = sink.events();
    let [ServiceEvent::ConversationEventDeltaV1(delta)] = events.as_slice() else {
        panic!("expected a durable request configuration acknowledgement");
    };
    assert_eq!(
        delta.durability,
        ConversationEventPatchDurabilityV1::Durable
    );
    let pending = delta
        .request_configs
        .as_ref()
        .and_then(|requests| requests.next_request.as_ref())
        .unwrap();
    assert_eq!(pending.provider.as_deref(), Some("provider"));
    assert_eq!(pending.connection_id.as_deref(), Some("account"));
    assert_eq!(pending.model.as_deref(), Some("next-model"));
}
