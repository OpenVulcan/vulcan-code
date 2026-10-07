//! HTML preference visibility across failed and successful physical publications.
//! HTML 偏好跨失败与成功物理发布的可见性。

use super::*;

/// Failed storage emits no confirmed preference; a successful retry publishes the exact candidate.
/// 存储失败不发送已确认偏好；成功重试发布精确候选。
#[test]
fn html_answers_live_preference_waits_for_actual_durable_publication() {
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
            assert!(document.set_html_answer_mode(true, 0, 2)?);
            Ok(SessionMutationChange::Changed(()))
        },
    );
    assert!(result.is_err());
    assert_eq!(recording.flush_count(), 0);
    assert!(sink.events().iter().all(|event| matches!(event,
        ServiceEvent::ConversationEventDeltaV1(delta)
        if delta.durability == ConversationEventPatchDurabilityV1::Volatile && delta.html_answers.is_none()
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
    assert!(delta.html_answers.as_ref().unwrap().enabled);
    assert_eq!(delta.html_answers.as_ref().unwrap().revision, 1);
    assert!(delta.events().is_empty());
}
