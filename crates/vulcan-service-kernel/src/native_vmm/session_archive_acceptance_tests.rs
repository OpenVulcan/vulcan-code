//! Actual archive maintenance validation over records created by native extraction.
//! 在原生提炼创建的记录上验证实际归档维护。

use super::*;
use vulcan_service_api::VmmNativeSessionArchiveSelection;

/// Verifies preview, archive, replay, unarchive and stale rejection while preserving all content.
/// 验证预览、归档、重放、取消归档和过期拒绝，同时保留全部内容。
/// Takes isolated runtime/maintenance/AI capture and owner ID; leaves the session archived for migration.
/// 接收隔离运行时、维护门面、AI 捕获及归属 ID；最终保持归档以供迁移验收。
pub(super) async fn verify(
    runtime: &NativeVmmCoreRuntime,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    data_plane: &VmmDataPlaneHandle,
    port: &CapturingAnalysisPort,
    project_id: u64,
) {
    maintenance.set_native_backend_enabled(true);
    // Only the existing exact session may be classified; project names never participate.
    // 只能分类准确已有会话，项目名称不参与。
    let selection = VmmNativeSessionArchiveSelection {
        vmm_project_id: project_id,
        session_source_ids: vec!["nonempty-session".to_owned()],
        archived: true,
    };
    let before = runtime.storage_snapshot().expect("archive before");
    // Observe physical vectors independently from the in-memory storage projection.
    // 独立于内存存储投影观察物理向量。
    let binding = data_plane
        .resolve_vector_space(
            before.dataset.memories[0]
                .vector_space_id
                .as_ref()
                .expect("space")
                .as_str(),
        )
        .await
        .expect("resolve vector binding")
        .expect("binding");
    // Stable memory identities must retain exactly the same physical vector values.
    // 稳定记忆身份必须保留完全相同的物理向量值。
    let memory_ids = vec![before.dataset.memories[0].id.to_string()];
    let vectors_before = data_plane
        .load_vectors(&binding, &memory_ids)
        .await
        .expect("vectors before");
    assert_eq!(vectors_before.len(), 1);
    let llm_count = port.requests.lock().expect("LLM count").len();
    let embedding_count = port
        .embedding_requests
        .lock()
        .expect("embedding count")
        .len();
    let replaced = maintenance
        .preview_session_archive(selection.clone())
        .await
        .expect("first preview");
    let preview = maintenance
        .preview_session_archive(selection.clone())
        .await
        .expect("replacement preview");
    assert_ne!(preview.preview_id, replaced.preview_id);
    assert_eq!(preview.selection, selection);
    assert_eq!(
        runtime.storage_snapshot().expect("read-only preview"),
        before
    );
    assert!(
        maintenance
            .execute_session_archive("replaced-archive", &replaced.preview_id)
            .await
            .is_err()
    );
    let result = maintenance
        .execute_session_archive("actual-archive", &preview.preview_id)
        .await
        .expect("archive");
    // Compare the entire snapshot: only lifecycle and the host transform version may change.
    // 比较整份快照：只允许生命周期及宿主转换版本改变。
    let mut expected = before.clone();
    expected.storage_version += 1;
    expected.memory_sessions[0].status = vulcan_memorymesh_core::StorageSessionStatus::Archived;
    assert_eq!(result.selection, selection);
    assert_eq!(result.storage_version, expected.storage_version);
    assert_eq!(
        maintenance
            .memory_records(project_id, Some("nonempty-session"))
            .await
            .expect("archived projection")
            .sessions[0]
            .status,
        Some(vulcan_memorymesh_core::StorageSessionStatus::Archived),
    );
    assert_eq!(
        runtime.storage_snapshot().expect("archived snapshot"),
        expected
    );
    assert_eq!(
        maintenance
            .execute_session_archive("actual-archive", &preview.preview_id)
            .await
            .expect("idempotent retry"),
        result
    );
    assert_eq!(
        runtime.storage_snapshot().expect("replay unchanged"),
        expected
    );
    assert!(
        maintenance
            .execute_session_archive("actual-archive", "different-preview")
            .await
            .is_err()
    );
    assert!(
        maintenance
            .preview_session_archive(selection.clone())
            .await
            .is_err()
    );
    // Reverse only the classification through a fresh preview, not a synthetic state edit.
    // 通过全新预览只逆转分类，不使用人工状态修改。
    let unarchive = maintenance
        .preview_session_archive(VmmNativeSessionArchiveSelection {
            archived: false,
            ..selection.clone()
        })
        .await
        .expect("unarchive preview");
    maintenance
        .execute_session_archive("actual-unarchive", &unarchive.preview_id)
        .await
        .expect("unarchive");
    expected.storage_version += 1;
    expected.memory_sessions[0].status = vulcan_memorymesh_core::StorageSessionStatus::Active;
    assert_eq!(
        runtime.storage_snapshot().expect("unarchived snapshot"),
        expected
    );
    let stale = maintenance
        .preview_session_archive(selection.clone())
        .await
        .expect("stale candidate");
    // Simulate an intervening committed host operation through the actual versioned transform.
    // 通过实际带版本转换模拟插入的一次已提交宿主操作。
    runtime
        .transform_storage_snapshot(
            "archive-concurrent-change",
            "fixture-version-change",
            expected.storage_version,
            |_| Ok(()),
        )
        .await
        .expect("intervening commit");
    expected.storage_version += 1;
    assert!(
        maintenance
            .execute_session_archive("stale-archive", &stale.preview_id)
            .await
            .is_err()
    );
    assert_eq!(
        runtime.storage_snapshot().expect("stale unchanged"),
        expected
    );
    let final_preview = maintenance
        .preview_session_archive(selection)
        .await
        .expect("final archive preview");
    maintenance
        .execute_session_archive("archive-before-migration", &final_preview.preview_id)
        .await
        .expect("final archive");
    assert_eq!(port.requests.lock().expect("LLM count").len(), llm_count);
    let vectors_after = data_plane
        .load_vectors(&binding, &memory_ids)
        .await
        .expect("vectors after");
    assert_eq!(vectors_after.len(), 1);
    assert_eq!(vectors_after[0].memory_id, vectors_before[0].memory_id);
    assert_eq!(vectors_after[0].vector, vectors_before[0].vector);
    assert_eq!(
        port.embedding_requests
            .lock()
            .expect("embedding count")
            .len(),
        embedding_count
    );
}
