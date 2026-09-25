//! Append-only model identity notices at actual request preparation boundaries.
//! 实际请求准备边界上只追加的模型身份通知。

use super::super::context_injection::{
    MODEL_SELECTION_CONTEXT_KEY, synchronize_request_dynamic_context,
};
use super::host_guidance::{candidate_for, install_baseline};
use super::*;

/// A switch informs the model through durable User context and leaves the historical prefix intact.
/// 模型切换通过持久 User 上下文通知模型，并保持历史前缀不变。
#[test]
fn model_selection_notice_is_durable_idempotent_and_corrects_failed_switch_back() {
    // Handle contains a completed main request on provider-1/model-1, without touching user files.
    // handle 含 provider-1/model-1 上已完成的主请求，不接触用户文件。
    let (_directory, handle) = published_stored_open_turn();
    let candidate = candidate_for(&handle);
    synchronize_request_dynamic_context(&handle, &request_facts(), &candidate).unwrap();
    assert!(
        !handle
            .read(|doc| doc
                .metadata
                .dynamic_context
                .contains_key(MODEL_SELECTION_CONTEXT_KEY))
            .unwrap()
    );
    install_baseline(
        &handle,
        initial_locked_baseline_with_host_guidance(&candidate.locked_baseline).unwrap(),
    );
    // Prefix is the physical request history before the switch context is published.
    // prefix 是切换上下文发布前的物理请求历史。
    let prefix = candidate_for(&handle).exact_request.messages;
    let mut facts = request_facts();
    facts.request = ApiRequestSnapshot::new("new-provider", "new-model");
    facts.request.profile = Some("openai-responses.v1".into());
    facts.prepared_at_ms = 10;
    synchronize_request_dynamic_context(&handle, &facts, &candidate).unwrap();
    let after = candidate_for(&handle).exact_request.messages;
    assert!(
        after.starts_with(&prefix),
        "a model switch must never rewrite the cached prefix"
    );
    let notice = after.iter().find(|message| message.content.iter().any(|item| matches!(item, ContentItem::Text {text} if text.contains(MODEL_SELECTION_CONTEXT_KEY)))).expect("model identity must reach the actual request");
    assert_eq!(notice.role, MessageRole::User);
    assert!(notice.content.iter().any(|item| matches!(item, ContentItem::Text {text} if text.contains("new-provider") && text.contains("new-model") && text.contains("<vulcan_context_update"))));
    let revision = handle.read(|doc| doc.memory_revision()).unwrap();
    facts.request.thinking_level = Some("max".into());
    synchronize_request_dynamic_context(&handle, &facts, &candidate).unwrap();
    assert_eq!(
        handle.read(|doc| doc.memory_revision()).unwrap(),
        revision,
        "effort changes and retries must not duplicate identity notices"
    );

    // Cold reload retains the notice even if preparation failed before any new api_started event.
    // 即使准备在任何新 api_started 事件之前失败，冷加载仍保留通知。
    let locator = handle.storage_locator().unwrap();
    let loaded = vulcan_workspace_store::workspace_store::load_session_v1_full(&locator).unwrap();
    let cold = SessionHandle::new(handle.session_id(), locator, loaded.document);
    synchronize_request_dynamic_context(&cold, &facts, &candidate).unwrap();
    assert_eq!(cold.read(|doc| doc.memory_revision()).unwrap(), revision);
    facts.request = ApiRequestSnapshot::new("provider-1", "model-1");
    synchronize_request_dynamic_context(&cold, &facts, &candidate).unwrap();
    let body = cold
        .read(|doc| {
            doc.metadata.dynamic_context[MODEL_SELECTION_CONTEXT_KEY]
                .raw_content
                .clone()
        })
        .unwrap();
    assert!(
        body.contains("model-1") && !body.contains("new-model"),
        "switching back before dispatch must correct the last published identity"
    );
    let before_compaction = cold.read(|doc| doc.memory_revision()).unwrap();
    facts.purpose = ApiPurposeKind::Compaction;
    facts.request = ApiRequestSnapshot::new("summary-provider", "summary-model");
    synchronize_request_dynamic_context(&cold, &facts, &candidate).unwrap();
    assert_eq!(
        cold.read(|doc| doc.memory_revision()).unwrap(),
        before_compaction,
        "summary routing must not change conversation identity"
    );
}
