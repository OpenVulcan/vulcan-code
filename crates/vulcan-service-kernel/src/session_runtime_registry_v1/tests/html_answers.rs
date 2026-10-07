//! HTML preference visibility across failed and successful physical publications.
//! HTML 偏好跨失败与成功物理发布的可见性。

use super::*;

/// Hydration and reconnect authorize the stored source, never a newer resident semantic revision.
/// 加载与重连只授权已存储源码，绝不授权更新的驻留语义修订。
#[tokio::test]
async fn html_answers_window_requires_exact_physically_persisted_source() {
    for (source, expected) in [
        (
            "```vulcan-html {\"version\":1,\"title\":\"saved\"}\n<p>content</p>\n```\n",
            "<p>content</p>\n",
        ),
        (
            "````vulcan-html {\"version\":2,\"title\":\"saved\",\"template\":\"document\"}\n```ui-kv\nPool | Shared\n```\n````\n",
            "```ui-kv\nPool | Shared\n```\n",
        ),
    ] {
        assert_persisted_html_source(source, expected).await;
    }
}

/// Run the same real-storage lifecycle assertions for raw HTML and compact draft `source`.
/// 对原始 HTML 及精简草稿 `source` 执行同一组真实存储生命周期断言。
/// `expected` is the original inner source, never generated markup; returns after cleanup succeeds.
/// `expected` 为原始内部源码，绝非生成标记；清理成功后返回。
async fn assert_persisted_html_source(source: &str, expected: &str) {
    use crate::conversation_events_v1::{
        service_session_event_hydration_v1, service_session_event_reconnect_v1,
    };
    use crate::tests::test_support::{TestHostRuntimeRoot, test_service_state_with_parent_session};
    use vulcan_service_api::{
        ConversationEventHydrationV1Request, ConversationEventReconnectV1Request,
    };
    use vulcan_session_core::v1::AssistantSemanticStatus;

    // The isolated indexed root exercises production storage without touching real sessions.
    // 隔离索引根覆盖生产存储路径，不接触真实会话。
    let session_id = SessionId::new_v4();
    let root = TestHostRuntimeRoot::new(session_id);
    let mut service = test_service_state_with_parent_session(session_id);
    service.controller_runtime = Some(root.attached_host_runtime_handle().await);
    let handle = SessionHandle::new(
        session_id,
        root.v1_storage_locator(),
        load_session_v1_full(&root.v1_storage_locator())
            .unwrap()
            .document,
    );
    service
        .session_runtime_registry
        .register(handle.clone())
        .unwrap();
    let api = start_initialized_test_api(&handle);
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.append_assistant_text(api, source, AssistantSemanticStatus::Streaming, 5)
            },
        )
        .unwrap();
    handle
        .mutate_in_memory(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.append_assistant_text(api, source, AssistantSemanticStatus::Completed, 6)
            },
        )
        .unwrap();
    // A low event counter can already be below the document frontier while its bytes remain unsaved.
    // 较小的事件计数可早已低于文档前沿，但对应字节仍未保存。
    let (event_revision, durable_revision) = handle
        .read(|document| {
            let event = document
                .events()
                .iter()
                .find(|event| matches!(event.event, EventBody::AssistantText(_)))
                .unwrap();
            (event.event_revision, document.durable_revision())
        })
        .unwrap();
    assert!(
        event_revision <= durable_revision,
        "fixture must distinguish independent revision counters"
    );
    let state = Arc::new(Mutex::new(service));
    let hydrate = ConversationEventHydrationV1Request {
        session_id: session_id.to_string(),
        parent_session_id: None,
    };
    let before = service_session_event_hydration_v1(state.clone(), hydrate.clone())
        .await
        .unwrap();
    assert!(before.html_answer_projections.is_empty());
    let reconnect = ConversationEventReconnectV1Request {
        html_answers_from_seq: None,
        session_id: session_id.to_string(),
        window_generation: before.batch.window_generation,
        last_confirmed_seq: None,
        service_epoch: state
            .lock()
            .unwrap()
            .session_runtime_registry
            .service_epoch()
            .to_owned(),
        last_confirmed_memory_revision: 0,
    };
    assert!(
        service_session_event_reconnect_v1(state.clone(), reconnect.clone())
            .await
            .unwrap()
            .html_answer_projections
            .is_empty()
    );
    handle.flush_all().unwrap();
    let saved = service_session_event_hydration_v1(state.clone(), hydrate)
        .await
        .unwrap();
    assert_eq!(saved.html_answer_projections.len(), 1);
    assert_eq!(
        saved.html_answer_projections[0].event_revision,
        event_revision
    );
    assert_eq!(
        service_session_event_reconnect_v1(state.clone(), reconnect)
            .await
            .unwrap()
            .html_answer_projections,
        saved.html_answer_projections
    );
    // The viewer reads exact stored bytes and rejects altered ownership facts without consulting a cache.
    // 查看器读取精确持久字节，拒绝被改动的归属事实，不查询缓存。
    use crate::session_html_answers::viewing::resolve_html_answer_source;
    use vulcan_service_api::{HtmlAnswerSourceIdentity, HtmlAnswerViewError};
    use vulcan_session_core::v1::html_answers::HtmlAnswerAvailability;
    let projection = &saved.html_answer_projections[0];
    let HtmlAnswerAvailability::Ready { artifact_id, .. } = &projection.answers[0].availability
    else {
        panic!("expected saved source")
    };
    let identity = HtmlAnswerSourceIdentity {
        session_id,
        parent_session_id: None,
        window_generation: saved.batch.window_generation,
        event_seq: projection.seq.value(),
        event_revision: projection.event_revision,
        artifact_id: artifact_id.clone(),
    };
    let content = resolve_html_answer_source(state.clone(), identity.clone())
        .await
        .unwrap();
    assert_eq!(content.source, expected);
    for field in ["generation", "revision", "artifact"] {
        let mut invalid = identity.clone();
        match field {
            "generation" => invalid.window_generation += 1,
            "revision" => invalid.event_revision += 1,
            "artifact" => {
                invalid.artifact_id =
                    "0".repeat(vulcan_session_core::v1::html_answers::HTML_ANSWER_ID_HEX_LENGTH)
            }
            _ => unreachable!(),
        }
        assert!(matches!(
            resolve_html_answer_source(state.clone(), invalid).await,
            Err(HtmlAnswerViewError::StaleSource)
        ));
    }
    let (controller, registry) = {
        let service = state.lock().unwrap();
        (
            service.controller_runtime.clone().unwrap(),
            service.session_runtime_registry.clone(),
        )
    };
    crate::session_v1_lifecycle::delete_session_lifecycle_v1(&controller, &registry, session_id)
        .await
        .unwrap();
    assert!(matches!(
        resolve_html_answer_source(state, identity).await,
        Err(HtmlAnswerViewError::Unavailable)
    ));
}

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

/// Completed HTML cannot acquire executable annotations until a failed physical write recovers.
/// 已完成 HTML 必须等待失败物理写入恢复后，才能获得可执行标注。
#[test]
fn html_answers_live_descriptor_waits_for_source_publication() {
    let recording = Arc::new(RecordingPersistenceBackend::default());
    let handle = published_handle(Arc::new(FailThenPersistBackend {
        remaining_failures: AtomicUsize::new(1),
        delegate: recording.clone(),
    }));
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .unwrap();
    handle
        .mutate_in_memory(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| document.submit_root_input(test_client_input("html-live", "make a page"), 2),
        )
        .unwrap();
    let api = handle
        .mutate_in_memory(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiStarted,
            |document| {
                document.prepare_and_start_api(test_prepare_api_input(
                    document,
                    ApiPurposeKind::MainGeneration,
                    ApiRequestSnapshot::new("test", "test"),
                    3,
                ))
            },
        )
        .unwrap()
        .value
        .into_value()
        .api_call_id;
    let result = handle.mutate_durable(
        SessionMutationAdmission::ExistingTurn,
        SessionFlushReason::MetadataChanged,
        |document| {
            document.append_assistant_text(
                api,
                "```vulcan-html {\"version\":1,\"title\":\"test\"}\n<p>hello</p>\n```\n",
                vulcan_session_core::v1::AssistantSemanticStatus::Completed,
                4,
            )
        },
    );
    assert!(result.is_err());
    assert!(sink.events().iter().all(|event| match event {
        ServiceEvent::ConversationEventDeltaV1(delta) => delta.html_answer_projections.is_none(),
        _ => true,
    }));
    sink.clear();
    handle.flush_all().unwrap();
    let events = sink.events();
    let [ServiceEvent::ConversationEventDeltaV1(delta)] = events.as_slice() else {
        panic!("expected one durable acknowledgement with derived annotations");
    };
    assert!(delta.events().is_empty());
    let [projection] = delta.html_answer_projections.as_ref().unwrap().as_slice() else {
        panic!("expected exact source annotation")
    };
    assert!(matches!(
        projection.answers[0].availability,
        vulcan_session_core::v1::html_answers::HtmlAnswerAvailability::Ready { .. }
    ));
    let persisted = recording.persisted_events();
    let source = persisted
        .iter()
        .find(|event| event.seq == projection.seq)
        .unwrap();
    assert_eq!(source.event_revision, projection.event_revision);
}
