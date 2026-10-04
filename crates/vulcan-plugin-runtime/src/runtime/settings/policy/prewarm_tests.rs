//! Real file replacement and restart tests keep automatic policy publication distinct from Lua execution.
//! 真实文件替换及重启测试将自动策略发布与 Lua 执行分离。

use super::*;

/// Build one exact request using the store's observed revision; return an explicit nonempty rule collection.
/// 使用存储已观测修订构建单个精确请求；返回明确非空规则集合。
fn request(store: &PolicyStore, runtime_id: &str) -> VulcanPluginPrewarmPolicyUpdate {
    VulcanPluginPrewarmPolicyUpdate {
        update_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: runtime_id.into(),
        expected_revision: store.prewarm_revision.clone(),
        rules: vec![VulcanPluginAutomaticPrewarmRule {
            plugin_id: "sample".into(),
            tool_id: "sample_read".into(),
            target_idle: 1,
        }],
    }
}

/// Every uncertain publication boundary preserves old live settings until the exact final image is acknowledged.
/// 每个不确定发布边界都保留旧活动设置，直至精确最终映像确认。
#[test]
fn automatic_policy_recovers_before_and_after_each_actual_file_publication() {
    for phase in [
        VulcanPluginPrewarmPolicyPhase::Prepared,
        VulcanPluginPrewarmPolicyPhase::Committed,
    ] {
        for after in [false, true] {
            let root = tempfile::tempdir().expect("policy root");
            let settings = VulcanPluginRuntimeSettings::default();
            let mut store = PolicyStore::load(root.path(), &settings).expect("policy owner");
            let original = request(&store, "original");
            let old_revision = store.prewarm_revision.clone();
            store.prewarm_fault = Some((phase, after));
            let interrupted = store
                .begin_prewarm(root.path(), "original", original.clone())
                .expect("retained uncertainty");
            assert!(interrupted.persistence_error.is_some());
            assert_eq!(
                store.settings, settings,
                "unacknowledged publication cannot activate automatic execution"
            );
            assert_eq!(store.prewarm_revision, old_revision);
            let disk = load_plugin_runtime_settings(root.path()).expect("read committed settings");
            assert_eq!(
                !disk.automatic_prewarm.is_empty(),
                phase == VulcanPluginPrewarmPolicyPhase::Committed && after
            );
            assert_eq!(
                store
                    .begin_prewarm(root.path(), "original", original.clone())
                    .expect("duplicate receipt"),
                interrupted
            );
            assert!(store.forget_prewarm(root.path()).is_err());
            if phase == VulcanPluginPrewarmPolicyPhase::Committed {
                assert_eq!(
                    interrupted.phase,
                    VulcanPluginPrewarmPolicyPhase::Publishing
                );
                assert!(store.cancel_prewarm(root.path()).is_err());
            }
            let committed = store
                .resume_prewarm(root.path(), "original")
                .expect("original publication recovery");
            assert_eq!(committed.phase, VulcanPluginPrewarmPolicyPhase::Committed);
            assert_eq!(committed.result_revision, interrupted.result_revision);
            assert_eq!(store.settings.automatic_prewarm, original.rules);
            assert_eq!(store.prewarm_revision, committed.result_revision);
            assert_eq!(
                load_plugin_runtime_settings(root.path()).expect("durable current"),
                store.settings
            );
            let committed_bytes =
                std::fs::read(root.path().join(SETTINGS_FILE_NAME)).expect("committed image");
            assert_eq!(
                store
                    .resume_prewarm(root.path(), "original")
                    .expect("terminal repeat"),
                committed
            );
            assert_eq!(
                std::fs::read(root.path().join(SETTINGS_FILE_NAME)).expect("stable image"),
                committed_bytes
            );
        }
    }
}

/// A restarted owner exposes committed policy but interrupts a merely prepared original parent without replay.
/// 重启所有者暴露已提交策略，但中断仅准备的原父级意图，不重放。
#[test]
fn automatic_policy_restart_never_rebinds_uncommitted_intent() {
    for committed_on_disk in [false, true] {
        let root = tempfile::tempdir().expect("policy root");
        let settings = VulcanPluginRuntimeSettings::default();
        let mut store = PolicyStore::load(root.path(), &settings).expect("first owner");
        let original = request(&store, "original");
        store.prewarm_fault = Some((
            if committed_on_disk {
                VulcanPluginPrewarmPolicyPhase::Committed
            } else {
                VulcanPluginPrewarmPolicyPhase::Prepared
            },
            true,
        ));
        let receipt = store
            .begin_prewarm(root.path(), "original", original.clone())
            .expect("original retained");
        drop(store);
        let persisted = load_plugin_runtime_settings(root.path()).expect("restart settings");
        assert_eq!(!persisted.automatic_prewarm.is_empty(), committed_on_disk);
        let mut restarted = PolicyStore::load(root.path(), &persisted).expect("restart owner");
        let before = restarted.prewarm_revision.clone();
        let result = restarted
            .resume_prewarm(root.path(), "replacement")
            .expect("original receipt reconciliation");
        assert_eq!(
            result.phase,
            if committed_on_disk {
                VulcanPluginPrewarmPolicyPhase::Committed
            } else {
                VulcanPluginPrewarmPolicyPhase::Interrupted
            }
        );
        assert_eq!(result.result_revision, receipt.result_revision);
        assert_eq!(
            restarted.prewarm_revision, before,
            "historical receipt does not become this owner's live predecessor"
        );
        assert_eq!(restarted.settings, persisted);
        restarted
            .forget_prewarm(root.path())
            .expect("acknowledge historical terminal");
        assert!(
            restarted
                .begin_prewarm(root.path(), "replacement", original)
                .is_err(),
            "old parent authorization must never replay"
        );
    }
}

/// Cancellation, external edit conflicts and another writer cannot overwrite original or independently owned bytes.
/// 取消、外部编辑冲突及其他写入者不能覆盖原始或独立持有的字节。
#[test]
fn automatic_policy_preserves_writer_fences_and_explicit_cancellation() {
    let root = tempfile::tempdir().expect("policy root");
    let settings = VulcanPluginRuntimeSettings::default();
    let mut first = PolicyStore::load(root.path(), &settings).expect("first");
    let mut second = PolicyStore::load(root.path(), &settings).expect("second");
    let original = request(&first, "original");
    first.prewarm_fault = Some((VulcanPluginPrewarmPolicyPhase::Prepared, false));
    first
        .begin_prewarm(root.path(), "original", original.clone())
        .expect("retain failed preparation");
    assert!(
        second
            .begin_prewarm(root.path(), "second", request(&second, "second"))
            .is_err()
    );
    let cancelled = first
        .cancel_prewarm(root.path())
        .expect("explicit cancellation");
    assert_eq!(cancelled.phase, VulcanPluginPrewarmPolicyPhase::Cancelled);
    assert_eq!(
        load_plugin_runtime_settings(root.path()).expect("unchanged policy"),
        settings
    );
    first
        .forget_prewarm(root.path())
        .expect("acknowledge cancellation");
    let external = serde_json::to_vec_pretty(&settings).expect("external image");
    std::fs::write(root.path().join(SETTINGS_FILE_NAME), &external).expect("external writer");
    let conflict = first
        .begin_prewarm(root.path(), "original", request(&first, "original"))
        .expect("retain external conflict");
    assert!(
        conflict
            .persistence_error
            .expect("publication diagnostic")
            .contains("another writer")
    );
    assert_eq!(
        std::fs::read(root.path().join(SETTINGS_FILE_NAME)).expect("retained external bytes"),
        external
    );
    assert_eq!(first.settings, settings);
}

/// Strict persisted shape rejects contradictory transactions, mutable revisions and transient memory checkpoints.
/// 严格持久形状拒绝冲突事务、可变修订及瞬时内存检查点。
#[test]
fn automatic_policy_document_rejects_incoherent_or_incomplete_evidence() {
    let root = tempfile::tempdir().expect("policy root");
    let settings = VulcanPluginRuntimeSettings::default();
    let mut store = PolicyStore::load(root.path(), &settings).expect("owner");
    let record = store
        .begin_prewarm(root.path(), "original", request(&store, "original"))
        .expect("commit");
    let mut document = PolicyDocument::new(store.settings.clone(), None);
    document.prewarm_update = Some(record);
    let original = serde_json::to_value(&document).expect("complete document");
    for case in [
        "missing",
        "publishing",
        "uncertain",
        "candidate",
        "revision",
        "both",
    ] {
        let mut invalid = original.clone();
        match case {
            "missing" => {
                invalid
                    .as_object_mut()
                    .expect("document")
                    .remove("prewarmUpdate");
            }
            "publishing" => invalid["prewarmUpdate"]["phase"] = serde_json::json!("publishing"),
            "uncertain" => {
                invalid["prewarmUpdate"]["persistenceError"] = serde_json::json!("unknown")
            }
            "candidate" => invalid["settings"]["automaticPrewarm"] = serde_json::json!([]),
            "revision" => {
                invalid["prewarmUpdate"]["resultRevision"] =
                    invalid["prewarmUpdate"]["request"]["expectedRevision"].clone()
            }
            "both" => {
                invalid["update"] = serde_json::to_value(super::tests::prepared(&settings).update)
                    .expect("capacity receipt")
            }
            _ => unreachable!("declared cases"),
        }
        assert!(
            version::decode_container(&invalid.to_string()).is_err(),
            "{case}"
        );
    }
}
