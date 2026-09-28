//! Persistent transaction tests exercise exact bytes, writer leases and uncertain file publication.
//! 持久事务测试验证精确字节、写入租约及不确定文件发布。

use super::*;

/// Create a complete valid request over settings; return a fixed semantic owner with fresh caller identity.
/// 在 settings 上创建完整合法请求；返回具有新调用方身份的固定语义所有者。
fn request(settings: &VulcanPluginRuntimeSettings) -> VulcanPluginCapacityPolicyUpdate {
    // The native fields are opaque in the document codec and are checked by live control on application.
    // 原生字段在文档编码器中为不透明值，应用时由活动控制检查。
    let mut config = settings
        .capacity_budget("sample", VulcanPluginCapacityPurpose::ToolCall)
        .0;
    config.max_queued_calls -= 1;
    VulcanPluginCapacityPolicyUpdate {
        update_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: "original-runtime".into(),
        capacity_id: "original-capacity".into(),
        expected_revision: "1".into(),
        plugin_id: "sample".into(),
        purpose: VulcanPluginCapacityPurpose::ToolCall,
        config: Some(config),
    }
}

/// Return one prepared durable container without allocating a native runtime.
/// 返回一个预备持久容器，不分配原生运行时。
fn prepared(settings: &VulcanPluginRuntimeSettings) -> PolicyDocument {
    PolicyDocument::new(
        settings.clone(),
        Some(VulcanPluginCapacityPolicyRecord {
            request: request(settings),
            phase: VulcanPluginCapacityPolicyPhase::Prepared,
            applied_revision: None,
            error: None,
            persistence_error: None,
        }),
    )
}

/// Loading intent never applies its candidate; strict nested decoding preserves invalid bytes for diagnosis.
/// 加载意图绝不应用其候选；严格嵌套解码保留无效原字节供诊断。
#[test]
fn policy_document_loads_committed_settings_and_rejects_invalid_checkpoints() {
    // Start with the sole host default and an independently explicit pending candidate.
    // 从唯一宿主默认值及独立明确待处理候选开始。
    let settings = VulcanPluginRuntimeSettings::default();
    // The complete original document is the source for every malformed variation.
    // 完整原文档作为每个畸形变体的来源。
    let document = prepared(&settings);
    // Serialization does not commit or execute a pending transaction.
    // 序列化不提交或执行待处理事务。
    let bytes = serde_json::to_string(&document).expect("document");
    assert_eq!(
        version::decode(&bytes).expect("committed settings"),
        settings
    );
    for invalid in [
        bytes.replacen("\"schemaVersion\":4", "\"schemaVersion\":5", 1),
        bytes.replacen("\"settings\":{", "\"settings\":{\"schemaVersion\":3,", 1),
        bytes.replace("\"phase\":\"prepared\"", "\"phase\":\"applied\""),
        bytes.replace("\"appliedRevision\":null", "\"appliedRevision\":\"2\""),
        bytes.replace("\"phase\":\"prepared\"", "\"phase\":\"committed\""),
        bytes.replacen("\"updateId\":", "\"unknown\":true,\"updateId\":", 1),
    ] {
        assert_ne!(invalid, bytes, "mutation must select an actual field");
        assert!(
            version::decode(&invalid).is_err(),
            "invalid checkpoint accepted"
        );
    }
}

/// Missing nullable fields cannot erase an intent, infer a reset, or silently remove checkpoint evidence.
/// 可空字段缺失不能抹除意图、推断重置或静默移除检查点证据。
#[test]
fn policy_document_requires_explicit_nullable_fields() {
    // Every malformed input derives from one valid durable transaction.
    // 每个畸形输入均从一个合法持久事务派生。
    let settings = VulcanPluginRuntimeSettings::default();
    let original = serde_json::to_value(prepared(&settings)).expect("complete document");
    for (pointer, field) in [
        ("", "update"),
        ("/update/request", "config"),
        ("/update", "appliedRevision"),
        ("/update", "error"),
    ] {
        // Remove only the selected real field so the exact omission is rejected.
        // 仅移除所选真实字段，验证精确缺失被拒绝。
        let mut missing = original.clone();
        assert!(
            missing
                .pointer_mut(pointer)
                .expect("field owner")
                .as_object_mut()
                .expect("object")
                .remove(field)
                .is_some()
        );
        assert!(
            version::decode(&missing.to_string())
                .expect_err("explicit nullable field required")
                .contains(field)
        );
    }
    // Explicit null remains the supported reset and acknowledgement representation.
    // 明确空值继续作为支持的重置及确认表示。
    let mut reset = original.clone();
    reset["update"]["request"]["config"] = serde_json::Value::Null;
    assert_eq!(
        version::decode(&reset.to_string()).expect("explicit reset"),
        settings
    );
    reset["update"] = serde_json::Value::Null;
    assert_eq!(
        version::decode(&reset.to_string()).expect("explicit empty receipt"),
        settings
    );
}

/// Actual OS writer locks and exact comparison refuse another owner or an external configuration rewrite.
/// 实际操作系统写入锁及精确比较拒绝另一所有者或外部配置重写。
#[test]
fn policy_store_fences_live_writer_and_external_edits() {
    // Both memory owners begin at the same explicitly absent file.
    // 两个内存所有者从同一明确缺失文件开始。
    let root = tempfile::tempdir().expect("policy root");
    let settings = VulcanPluginRuntimeSettings::default();
    let mut first = PolicyStore::load(root.path(), &settings).expect("first owner");
    let mut second = PolicyStore::load(root.path(), &settings).expect("second owner");
    assert!(!root.path().join(SETTINGS_FILE_NAME).exists());
    // A held native file lease is independent of repeated settings-file replacement.
    // 持有的原生文件租约独立于反复设置文件替换。
    let document = prepared(&settings);
    first.persist(root.path(), &document).expect("first intent");
    assert!(second.persist(root.path(), &document).is_err());
    // Preserve a valid but independently edited image without silently normalizing or overwriting it.
    // 保留合法但被独立编辑的映像，不静默规范化或覆盖它。
    let external = serde_json::to_vec(&settings).expect("external image");
    std::fs::write(root.path().join(SETTINGS_FILE_NAME), &external).expect("external writer");
    assert!(
        first
            .persist(root.path(), &document)
            .expect_err("external conflict")
            .contains("another writer")
    );
    assert_eq!(
        std::fs::read(root.path().join(SETTINGS_FILE_NAME)).expect("original external bytes"),
        external
    );
}

/// Recovery preserves an observed uncertain image even if the next replacement fails before publishing.
/// 即使下一次替换在发布前失败，恢复仍保留已观测的不确定映像。
#[test]
fn policy_store_recovers_successive_uncertain_publications_without_losing_its_own_image() {
    // Real filesystem replacement is used; injection selects only which acknowledgement is lost.
    // 使用真实文件系统替换；注入仅选择丢失哪个确认。
    let root = tempfile::tempdir().expect("policy root");
    let settings = VulcanPluginRuntimeSettings::default();
    let mut store = PolicyStore::load(root.path(), &settings).expect("owner");
    let document = prepared(&settings);
    store.fault = Some((VulcanPluginCapacityPolicyPhase::Prepared, true));
    assert!(store.persist(root.path(), &document).is_err());
    assert_eq!(
        load_plugin_runtime_settings(root.path()).expect("original settings"),
        settings
    );
    // The rejected image differs from the published prepared image and initially fails before replacement.
    // 拒绝映像不同于已发布预备映像，初次在替换前失败。
    let mut rejected = document.clone();
    rejected.update.as_mut().expect("receipt").phase = VulcanPluginCapacityPolicyPhase::Rejected;
    store.fault = Some((VulcanPluginCapacityPolicyPhase::Rejected, false));
    assert!(store.persist(root.path(), &rejected).is_err());
    store
        .persist(root.path(), &rejected)
        .expect("same original transaction recovers");
    assert_eq!(
        decode_document(
            &std::fs::read_to_string(root.path().join(SETTINGS_FILE_NAME)).expect("file")
        )
        .expect("strict stored document"),
        rejected
    );
}
