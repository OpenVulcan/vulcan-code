//! HTML preference coverage through the actual durable request-context publication path.
//! 通过实际持久请求上下文发布路径覆盖 HTML 偏好行为。

use super::super::context_injection::synchronize_request_dynamic_context;
use super::host_guidance::{candidate_for, install_baseline};
use super::*;
use vulcan_session_core::v1::document::HTML_ANSWER_CONTEXT_KEY;

/// Render the captured producer through the authoritative host envelope for physical-message assertions.
/// 通过权威宿主信封渲染捕获的生产者，以断言物理请求消息。
/// `state` supplies the durable sequence and unescaped body; returns the actual wire encoding.
/// `state` 提供可靠序号和未转义正文；返回实际线协议编码。
fn producer_envelope(state: &vulcan_session_core::v1::DynamicContextState) -> String {
    vulcan_ai_contract::HostMessageEnvelope {
        event_id: format!(
            "v1-context-update-{}-{HTML_ANSWER_CONTEXT_KEY}",
            state.source_seq.value()
        ),
        kind: vulcan_ai_contract::HostMessageKind::ContextUpdate,
        source: "host".into(),
        target: "model".into(),
        task_id: None,
        question_id: None,
        scope: "session".into(),
        body: state.raw_content.clone(),
    }
    .render()
    .unwrap()
}

/// Persist an exact synthetic desired toggle through the production owner, returning no user data.
/// 通过生产所有者持久化精确的合成期望开关，不返回用户数据。
/// `handle` owns isolated storage; revision checks and publication failures stay on the real path.
/// `handle` 拥有隔离存储；修订校验和发布失败均沿真实路径执行。
fn set_mode(handle: &SessionHandle, enabled: bool, revision: u64, now_ms: i64) {
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .set_html_answer_mode(enabled, revision, now_ms)
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

/// On/off changes reach physical request messages, survive reload and do not rewrite locked history.
/// 开关变化到达物理请求消息，重载后保留，且不重写已锁定历史。
#[test]
fn html_answers_request_guidance_is_durable_idempotent_and_compaction_independent() {
    let (_directory, handle) = published_stored_open_turn();
    let candidate = candidate_for(&handle);
    synchronize_request_dynamic_context(&handle, &request_facts(), &candidate).unwrap();
    assert!(
        !handle
            .read(|document| document
                .metadata
                .dynamic_context
                .contains_key(HTML_ANSWER_CONTEXT_KEY))
            .unwrap()
    );
    install_baseline(
        &handle,
        initial_locked_baseline_with_host_guidance(&candidate.locked_baseline).unwrap(),
    );
    let prefix = candidate_for(&handle).exact_request.messages;
    set_mode(&handle, true, 0, 5);
    let mut facts = request_facts();
    facts.prepared_at_ms = 6;
    synchronize_request_dynamic_context(&handle, &facts, &candidate).unwrap();
    let enabled_state = handle
        .read(|document| document.metadata.dynamic_context[HTML_ANSWER_CONTEXT_KEY].clone())
        .unwrap();
    let messages = candidate_for(&handle).exact_request.messages;
    assert!(messages.starts_with(&prefix));
    assert!(
        messages
            .iter()
            .any(|message| message.content.iter().any(|item| matches!(item,
                ContentItem::Text { text } if text == &producer_envelope(&enabled_state)
            ))),
        "the stored producer body must reach actual request messages"
    );
    let before = handle.read(|document| document.memory_revision()).unwrap();
    synchronize_request_dynamic_context(&handle, &facts, &candidate).unwrap();
    assert_eq!(
        handle.read(|document| document.memory_revision()).unwrap(),
        before
    );

    let locator = handle.storage_locator().unwrap();
    let loaded = vulcan_workspace_store::workspace_store::load_session_v1_full(&locator).unwrap();
    let cold = SessionHandle::new(handle.session_id(), locator, loaded.document);
    synchronize_request_dynamic_context(&cold, &facts, &candidate).unwrap();
    assert_eq!(
        cold.read(|document| document.memory_revision()).unwrap(),
        before
    );
    set_mode(&cold, false, 1, 7);
    let desired = cold
        .read(|document| document.metadata.config.html_answers.clone())
        .unwrap();
    assert!(!desired.enabled);
    assert!(desired.prepared.unwrap().enabled);
    facts.purpose = ApiPurposeKind::Compaction;
    facts.prepared_at_ms = 8;
    synchronize_request_dynamic_context(&cold, &facts, &candidate).unwrap();
    assert_eq!(
        cold.read(|document| document.metadata.config.html_answers.clone())
            .unwrap(),
        desired
    );
    facts.purpose = ApiPurposeKind::MainGeneration;
    synchronize_request_dynamic_context(&cold, &facts, &candidate).unwrap();
    let disabled_state = cold
        .read(|document| document.metadata.dynamic_context[HTML_ANSWER_CONTEXT_KEY].clone())
        .unwrap();
    assert_ne!(disabled_state.source_seq, enabled_state.source_seq);
    assert_ne!(disabled_state.fingerprint, enabled_state.fingerprint);
    assert!(
        !cold
            .read(|document| document
                .metadata
                .config
                .html_answers
                .prepared
                .unwrap()
                .enabled)
            .unwrap()
    );
    assert!(
        candidate_for(&cold)
            .exact_request
            .messages
            .iter()
            .any(|message| message.content.iter().any(|item| matches!(item,
                ContentItem::Text { text } if text == &producer_envelope(&disabled_state)
            ))),
        "the explicit exit producer body must reach actual request messages"
    );
}
