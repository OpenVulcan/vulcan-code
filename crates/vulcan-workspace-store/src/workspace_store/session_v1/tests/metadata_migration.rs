//! Metadata evolution regressions through production storage readers and writers.
//! 通过生产存储读写入口验证元数据演进。

use super::*;
use vulcan_session_core::v1::business_hook::BusinessHookBinding;
use vulcan_session_core::v1::collaboration::*;
use vulcan_session_core::v1::config::METADATA_SCHEMA_VERSION;

/// Schema 2 gains only default-off HTML state, with byte-exact backup and repeatable full loading.
/// 模式 2 仅增加默认关闭的 HTML 状态，保留精确原字节备份并支持重复完整加载。
#[test]
fn metadata_migration_html_answers_v2_to_v3_preserves_original_and_unrelated_state() {
    let (_root, layout, metadata) = fixture(false);
    let mut raw = serde_json::to_value(&metadata).unwrap();
    raw["schema_version"] = 2.into();
    raw["config"]
        .as_object_mut()
        .unwrap()
        .remove("html_answers");
    let original = serde_json::to_vec_pretty(&raw).unwrap();
    std::fs::write(layout.metadata_path(), &original).unwrap();
    let migrated = read_metadata(&layout.metadata_path()).unwrap();
    assert_eq!(migrated, metadata);
    assert_eq!(migrated.config.html_answers, Default::default());
    assert_eq!(
        std::fs::read(layout.session_dir().join("metadata.schema-2.backup.json")).unwrap(),
        original
    );
    let saved = std::fs::read(layout.metadata_path()).unwrap();
    assert_eq!(load_session(&layout).unwrap().metadata, migrated);
    assert_eq!(read_metadata(&layout.metadata_path()).unwrap(), migrated);
    assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), saved);
}

/// A corrupt current document cannot borrow old defaults, and an old schema cannot smuggle new intent.
/// 损坏的当前文档不能借用旧版本默认值，旧模式不能夹带新偏好。
#[test]
fn metadata_migration_html_answers_rejects_missing_current_or_foreign_old_state() {
    for current in [false, true] {
        let (_root, layout, metadata) = fixture(false);
        let mut raw = serde_json::to_value(&metadata).unwrap();
        if current {
            raw["config"]
                .as_object_mut()
                .unwrap()
                .remove("html_answers");
        } else {
            raw["schema_version"] = 2.into();
            raw["config"]["html_answers"]["enabled"] = true.into();
        }
        let original = serde_json::to_vec(&raw).unwrap();
        std::fs::write(layout.metadata_path(), &original).unwrap();
        assert!(read_metadata(&layout.metadata_path()).is_err());
        assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), original);
        assert!(
            !layout
                .session_dir()
                .join("metadata.schema-2.backup.json")
                .exists()
        );
    }
}

/// Publishes a real human or agent task turn with its correctly captured Hook policy.
/// 发布真实用户或代理任务回合，捕获正确的 Hook 策略。
/// `agent` chooses the origin; returns isolated storage and the published expected metadata.
/// `agent` 选择来源；返回隔离存储及已发布的预期元数据。
fn fixture(agent: bool) -> (tempfile::TempDir, SessionV1Layout, MetadataDocument) {
    let (root, layout) = fresh_layout("migration");
    let mut document = SessionDocument::new(&layout.session_id, "project-1", "migration", 1);
    if agent {
        let message = CollaborationTaskMessage {
            delivery_id: CollaborationDeliveryId::try_new("migration-delivery").unwrap(),
            operation_id: CollaborationOperationId::try_new("migration-send").unwrap(),
            source: CollaborationParticipant::new(
                "source",
                "source",
                "project-1",
                1,
                Some(TurnId::try_new(2).unwrap()),
                None,
            )
            .unwrap(),
            target: CollaborationParticipant::new(
                &layout.session_id,
                &layout.session_id,
                "project-1",
                1,
                None,
                None,
            )
            .unwrap(),
            kind: CollaborationTaskMessageKind::TaskStart,
            body: "Agent work, not a human prompt".into(),
        };
        document
            .accept_task_message(
                CollaborationOperationId::try_new("accept").unwrap(),
                message.clone(),
                2,
            )
            .unwrap();
        document
            .claim_task_message_before_api(
                CollaborationOperationId::try_new("claim").unwrap(),
                message.delivery_id,
                3,
            )
            .unwrap();
    } else {
        submit_test_root(&mut document, "human", "Human work", 2);
    }
    let turn = document.active_turn().unwrap();
    let binding = BusinessHookBinding {
        session_id: layout.session_id.clone(),
        segment_generation: 1,
        turn_id: turn.turn_id,
        start_seq: turn.start_seq,
        hook_turn_id: "original-hook".into(),
        user_content: "Original transformed input".into(),
        vmm_runtime_ready: true,
        observes_user: !agent,
    };
    document.capture_business_hook_binding(binding, 4).unwrap();
    let metadata = publish_initial(&layout, &document);
    (root, layout, metadata)
}

/// Recreates the exact old schema shape while retaining every unrelated field and value.
/// 重建精确旧模式结构，同时保留所有无关字段和值。
/// `omit_policy` simulates the pre-field version; returns exact bytes for backup comparisons.
/// `omit_policy` 模拟字段新增前版本；返回用于备份对比的精确字节。
fn write_legacy(
    layout: &SessionV1Layout,
    metadata: &MetadataDocument,
    omit_policy: bool,
) -> Vec<u8> {
    let mut raw = serde_json::to_value(metadata).unwrap();
    raw["schema_version"] = 1.into();
    raw["config"]
        .as_object_mut()
        .unwrap()
        .remove("html_answers");
    if omit_policy {
        raw["business_hook"]["binding"]
            .as_object_mut()
            .unwrap()
            .remove("observes_user");
    }
    let bytes = serde_json::to_vec_pretty(&raw).unwrap();
    std::fs::write(layout.metadata_path(), &bytes).unwrap();
    bytes
}

/// Both genuine origins migrate from their exact event and retain configuration, revision and history.
/// 两种真实来源均根据精确事件迁移，保留配置、修订及历史。
/// Also verifies byte-exact backup, full loading and idempotent reads without another write.
/// 同时验证原字节备份、完整加载及重读不再写入。
#[test]
fn metadata_migration_preserves_human_and_agent_hook_origins() {
    for agent in [false, true] {
        let (_root, layout, metadata) = fixture(agent);
        let original = write_legacy(&layout, &metadata, true);
        let migrated = read_metadata(&layout.metadata_path()).unwrap();
        assert_eq!(migrated, metadata);
        assert_eq!(
            migrated
                .business_hook
                .binding
                .as_ref()
                .unwrap()
                .observes_user,
            !agent
        );
        assert_eq!(
            std::fs::read(layout.session_dir().join("metadata.schema-1.backup.json")).unwrap(),
            original
        );
        let bytes = std::fs::read(layout.metadata_path()).unwrap();
        assert_eq!(load_session(&layout).unwrap().metadata, migrated);
        assert_eq!(read_metadata(&layout.metadata_path()).unwrap(), migrated);
        assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), bytes);
    }
}

/// Explicit old policies and empty bindings need no conversation hydration to advance the schema.
/// 明确旧策略和空绑定无需加载对话即可推进模式。
/// A current-version document never gains a backup or lock file merely from reading.
/// 当前版本文档不会仅因读取就新增备份或锁文件。
#[test]
fn metadata_migration_keeps_metadata_only_reads_and_current_fast_path() {
    for binding in [true, false] {
        let (_root, layout, mut metadata) = fixture(true);
        if !binding {
            metadata.business_hook.binding = None;
        }
        let original = write_legacy(&layout, &metadata, false);
        std::fs::remove_file(layout.segment_entry_path(1, &metadata.split_list[0].file_name))
            .unwrap();
        assert_eq!(read_metadata(&layout.metadata_path()).unwrap(), metadata);
        assert_eq!(
            std::fs::read(layout.session_dir().join("metadata.schema-1.backup.json")).unwrap(),
            original
        );
    }
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("metadata.json");
    let metadata = MetadataDocument::new("current", "project", "current", 1);
    std::fs::write(&path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    assert_eq!(read_metadata(&path).unwrap(), metadata);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

/// Unknown versions, malformed fields and unverifiable origins must leave the original file intact.
/// 未知版本、畸形字段和无法证明的来源必须保持原文件完整。
/// Fixtures alter only declared failure coordinates, never real user storage.
/// 测试仅修改明确失败坐标，绝不触碰真实用户存储。
#[test]
fn metadata_migration_rejects_unsafe_candidates_without_publication() {
    for failure in [
        "future",
        "current_missing",
        "unknown",
        "owner",
        "start",
        "segment",
        "malformed",
    ] {
        let (_root, layout, metadata) = fixture(false);
        write_legacy(&layout, &metadata, true);
        let mut raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(layout.metadata_path()).unwrap()).unwrap();
        match failure {
            "future" => raw["schema_version"] = (METADATA_SCHEMA_VERSION + 1).into(),
            "current_missing" => raw["schema_version"] = METADATA_SCHEMA_VERSION.into(),
            "unknown" => raw["unknown_state"] = true.into(),
            "owner" => raw["business_hook"]["binding"]["session_id"] = "other".into(),
            "start" => raw["business_hook"]["binding"]["start_seq"] = 999.into(),
            "segment" => raw["split_list"][0]["file_name"] = "../../metadata.json".into(),
            "malformed" => raw["business_hook"]["binding"]["observes_user"] = "true".into(),
            _ => unreachable!(),
        }
        let before = serde_json::to_vec(&raw).unwrap();
        std::fs::write(layout.metadata_path(), &before).unwrap();
        assert!(read_metadata(&layout.metadata_path()).is_err(), "{failure}");
        assert_eq!(
            std::fs::read(layout.metadata_path()).unwrap(),
            before,
            "{failure}"
        );
        assert!(
            !layout
                .session_dir()
                .join("metadata.schema-1.backup.json")
                .exists(),
            "{failure}"
        );
    }
}

/// Backup and publication failures preserve the source, and retry reuses an identical backup safely.
/// 备份和发布失败保留源文件，重试安全复用相同备份。
/// Exercises real atomic-write stages rather than bypassing production persistence.
/// 执行真实原子写阶段，而非绕过生产持久化。
#[test]
fn metadata_migration_write_failures_are_retryable() {
    for artifact in ["metadata migration backup", "metadata migration"] {
        for stage in [WriteStage::WriteAll, WriteStage::Rename] {
            let (_root, layout, metadata) = fixture(false);
            let original = write_legacy(&layout, &metadata, true);
            if artifact == "metadata migration" {
                std::fs::write(
                    layout.session_dir().join("metadata.schema-1.backup.json"),
                    &original,
                )
                .unwrap();
            }
            test_hooks::fail_next_artifact(stage, artifact);
            assert!(read_metadata(&layout.metadata_path()).is_err());
            assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), original);
            assert_eq!(read_metadata(&layout.metadata_path()).unwrap(), metadata);
        }
    }
}

/// Concurrent first readers serialize migration and all observe the same current document.
/// 并发首次读取串行迁移，全部观察到同一当前文档。
/// The original backup must remain the exact pre-upgrade bytes.
/// 原始备份必须保持升级前的精确字节。
#[test]
fn metadata_migration_concurrent_reads_publish_once() {
    let (_root, layout, metadata) = fixture(false);
    let original = write_legacy(&layout, &metadata, true);
    std::thread::scope(|scope| {
        let readers: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| read_metadata(&layout.metadata_path())))
            .collect();
        for reader in readers {
            assert_eq!(reader.join().unwrap().unwrap(), metadata);
        }
    });
    assert_eq!(
        std::fs::read(layout.session_dir().join("metadata.schema-1.backup.json")).unwrap(),
        original
    );
}

/// Capability rebuilding shares the versioned upgrade but never auto-publishes its detached candidate.
/// 能力重建共用版本升级，但绝不自动发布其独立候选。
/// The original backup includes the old capability bytes and the missing observer field.
/// 原始备份包含旧能力字节及缺失观察字段的结构。
#[test]
fn metadata_migration_capability_rebuild_preserves_original() {
    let (_root, layout, metadata) = fixture(false);
    let original = write_legacy(&layout, &metadata, true);
    let candidate = super::super::documents::read_metadata_for_provider_tool_rebuild(
        &layout.metadata_path(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(candidate.schema_version, METADATA_SCHEMA_VERSION);
    assert!(candidate.business_hook.binding.unwrap().observes_user);
    assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), original);
    assert_eq!(
        std::fs::read(layout.session_dir().join("metadata.schema-1.backup.json")).unwrap(),
        original
    );
}

/// Normal model-configuration access automatically upgrades metadata without loading conversation files.
/// 正常模型配置访问自动升级元数据，无需加载对话文件。
/// Returns no user data; verifies the production locator entry point rather than only an internal helper.
/// 不输出用户数据；验证生产定位器入口，而非仅验证内部辅助方法。
#[test]
fn metadata_migration_runs_through_configuration_read() {
    let root = tempfile::tempdir().unwrap();
    let locator = crate::workspace_store::session_v1_store::create_session_v1_storage_fixture(
        root.path(),
        "migration-model",
        1,
    )
    .unwrap();
    let path = locator.session_directory().join("metadata.json");
    let mut raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    raw["schema_version"] = 1.into();
    raw["config"]
        .as_object_mut()
        .unwrap()
        .remove("html_answers");
    let original = serde_json::to_vec(&raw).unwrap();
    std::fs::write(&path, &original).unwrap();
    locator.read_request_configuration().unwrap();
    assert_eq!(
        read_metadata(&path).unwrap().schema_version,
        METADATA_SCHEMA_VERSION
    );
    assert_eq!(
        std::fs::read(path.with_file_name("metadata.schema-1.backup.json")).unwrap(),
        original
    );
}

/// A preexisting different backup blocks migration rather than destroying the earlier recovery point.
/// 已有不同备份会阻止迁移，避免破坏此前恢复点。
/// Both backup and source bytes remain unchanged after rejection.
/// 拒绝后备份和源文件字节均保持不变。
#[test]
fn metadata_migration_backup_conflict_preserves_both_files() {
    let (_root, layout, metadata) = fixture(false);
    let original = write_legacy(&layout, &metadata, true);
    let backup = layout.session_dir().join("metadata.schema-1.backup.json");
    std::fs::write(&backup, b"earlier recovery point").unwrap();
    assert!(read_metadata(&layout.metadata_path()).is_err());
    assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), original);
    assert_eq!(std::fs::read(backup).unwrap(), b"earlier recovery point");
}

/// Copies one explicitly selected legacy session and validates migration without touching its source.
/// 复制明确选定的旧会话并验证迁移，不修改源目录。
/// Set VULCAN_METADATA_MIGRATION_FIXTURE to the session directory and opt in with --ignored.
/// 将 VULCAN_METADATA_MIGRATION_FIXTURE 设置为会话目录，并使用 --ignored 显式执行。
#[test]
#[ignore = "requires an explicitly selected local session fixture"]
fn metadata_migration_external_session_copy() {
    let source = PathBuf::from(
        std::env::var_os("VULCAN_METADATA_MIGRATION_FIXTURE").expect("explicit fixture directory"),
    );
    let before = std::fs::read(source.join("metadata.json")).unwrap();
    let raw: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let root = tempfile::tempdir().unwrap();
    let layout = SessionV1Layout::new(
        root.path(),
        "fixture",
        "2026-01-01",
        raw["session_id"]
            .as_str()
            .expect("fixture session identity"),
    );
    copy_fixture_tree(&source, &layout.session_dir());
    let migrated = read_metadata(&layout.metadata_path()).unwrap();
    assert_eq!(migrated.schema_version, METADATA_SCHEMA_VERSION);
    assert_eq!(load_session(&layout).unwrap().metadata, migrated);
    assert_eq!(std::fs::read(source.join("metadata.json")).unwrap(), before);
    if raw["schema_version"] != METADATA_SCHEMA_VERSION {
        let version = raw["schema_version"].as_u64().unwrap();
        assert_eq!(
            std::fs::read(
                layout
                    .session_dir()
                    .join(format!("metadata.schema-{version}.backup.json"))
            )
            .unwrap(),
            before
        );
    }
}

/// Copies a read-only fixture into a newly owned temporary directory, refusing filesystem links.
/// 将只读夹具复制到新建的专属临时目录，拒绝文件系统链接。
/// Source and destination are explicit paths; this helper never deletes or rewrites the source.
/// 源和目标均为明确路径；该辅助方法绝不删除或改写源文件。
fn copy_fixture_tree(source: &std::path::Path, destination: &std::path::Path) {
    std::fs::create_dir_all(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(!kind.is_symlink(), "fixture links are unsupported");
        if kind.is_dir() {
            copy_fixture_tree(&entry.path(), &destination.join(entry.file_name()));
        } else {
            assert!(kind.is_file(), "fixture must contain only regular files");
            std::fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}
