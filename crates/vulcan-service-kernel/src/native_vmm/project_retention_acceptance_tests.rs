//! Real storage verification for owner-specific automatic and confirmed cleanup.
//! 按归属方自动清洗和确认清洗的真实存储验证。

use super::*;
use vulcan_service_api::VmmNativeProjectRetentionEdit;

/// Seeds terminal shared memories with real vectors through the production snapshot transaction.
/// 通过生产快照事务放入带真实向量的共享终态记忆。
/// Project IDs and numeric offsets belong to this isolated acceptance controller only.
/// 项目 ID 和数字偏移仅属于当前隔离验收控制器。
async fn seed(runtime: &NativeVmmCoreRuntime, projects: &[u64], offset: usize) {
    let version = runtime
        .storage_snapshot()
        .expect("before seed")
        .storage_version;
    let memories: Vec<_> = projects
        .iter()
        .enumerate()
        .map(|(index, project)| {
            let mut memory = indexed_memory(offset + index, &format!("vmm-project:{project}"));
            memory.embedding = Some(vec![0.6, 0.8]);
            memory.source_subject_id = Some(SubjectId::new("vmm-subject:1").expect("subject"));
            memory.access_class = MemoryAccessClass::ScopeShared;
            memory.state = MemoryState::Deleted;
            memory
        })
        .collect();
    runtime
        .transform_storage_snapshot(
            &format!("project-retention-seed-{offset}"),
            "test",
            version,
            move |snapshot| {
                snapshot.dataset.memories.extend(memories);
                Ok(())
            },
        )
        .await
        .expect("seed");
}

/// Produces the complete allowed owner group with a distinct frozen recycle duration.
/// 生成完整获准归属方配置组，具有独立的冻结回收期限。
fn rules(duration: u64) -> vulcan_service_api::VmmNativeProjectRetentionRules {
    vulcan_service_api::VmmNativeProjectRetentionRules {
        turn_hot_window_size: 2,
        session_idle_ms: 86_400_000,
        trash_retention_ms: duration,
        protected_priority_floor: 100,
        protected_memory_level_floor: 4,
        skip_protected_shared_memories: true,
    }
}

/// Validates real planning, policy-change rejection, independent batch deadlines and vector preservation.
/// 验证真实规划、策略变化拒绝、独立批次截止时间及向量保留。
/// No AI route is registered; all fixture configuration resides under the isolated VMM user root.
/// 不注册 AI 路由；所有夹具配置位于隔离 VMM 用户根目录下。
pub(super) async fn verify(
    runtime: &NativeVmmCoreRuntime,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
) {
    let a = runtime
        .host
        .data_plane
        .create_project("Same", "retention-a")
        .await
        .expect("a");
    let b = runtime
        .host
        .data_plane
        .create_project("Same", "retention-b")
        .await
        .expect("b");
    let mut global = runtime.retention_settings().await.expect("global");
    global.enabled = false;
    global.protected_priority_floor = 100;
    global.protected_memory_level_floor = 3;
    global.skip_protected_shared_memories = true;
    global.trash_retention_ms = 86_400_000;
    maintenance
        .update_retention_settings(global.clone())
        .await
        .expect("global rules");
    let path = runtime
        .host
        .data_plane
        .data_root
        .join("config/memory-project-retention.json");
    let a_saved = maintenance
        .update_project_retention(VmmNativeProjectRetentionEdit::Override {
            project_id: a.id,
            rules: rules(172_800_000),
        })
        .await
        .expect("save through production maintenance");
    assert_eq!(a_saved.project_id, a.id);
    assert_eq!(a_saved.overrides, Some(rules(172_800_000)));
    assert_eq!(a_saved.effective, rules(172_800_000));
    assert!(
        maintenance
            .project_retention_summary(b.id)
            .await
            .expect("b inherits")
            .overrides
            .is_none()
    );
    let saved_bytes = std::fs::read(&path).expect("persisted owner rule");
    assert!(
        maintenance
            .project_retention_summary(u64::MAX)
            .await
            .is_err()
    );
    assert!(
        maintenance
            .update_project_retention(VmmNativeProjectRetentionEdit::Override {
                project_id: u64::MAX,
                rules: rules(1),
            })
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(&path).expect("invalid project unchanged"),
        saved_bytes
    );
    seed(runtime, &[a.id, b.id], 9100).await;
    let preview = maintenance
        .preview_retention_cleanup()
        .await
        .expect("owner preview");
    assert_eq!(
        preview
            .projects
            .iter()
            .find(|project| project.project_id == a.id)
            .expect("a impact")
            .recycled_memory_ids,
        vec!["vmm-memory-9100"]
    );
    assert!(
        preview
            .projects
            .iter()
            .find(|project| project.project_id == b.id)
            .expect("b impact")
            .recycled_memory_ids
            .is_empty()
    );
    let before = runtime.storage_snapshot().expect("preview unchanged");
    maintenance
        .update_project_retention(VmmNativeProjectRetentionEdit::Override {
            project_id: b.id,
            rules: rules(259_200_000),
        })
        .await
        .expect("save b through production maintenance");
    let valid_bytes = std::fs::read(&path).expect("valid saved rules");
    assert!(
        maintenance
            .execute_retention_cleanup("stale-owner-cleanup", &preview.preview_id)
            .await
            .is_err()
    );
    assert_eq!(
        runtime.storage_snapshot().expect("rejected unchanged"),
        before
    );
    let confirmed = maintenance
        .preview_retention_cleanup()
        .await
        .expect("new preview");
    maintenance
        .execute_retention_cleanup("owner-confirmed-cleanup", &confirmed.preview_id)
        .await
        .expect("confirmed cleanup");
    assert_batches(runtime, 9100).await;
    // The same policies must govern the production automatic pass, not only preview simulation.
    // 同一策略必须作用于生产自动轮次，而非仅作用于预览模拟。
    seed(runtime, &[a.id, b.id], 9200).await;
    // A malformed owner policy aborts the automatic pass before any project is partially changed.
    // 损坏的归属方策略在任何项目被部分更改前中止自动轮次。
    let before_invalid = runtime.storage_snapshot().expect("before invalid policy");
    std::fs::write(&path, b"{").expect("invalid isolated user edit");
    assert!(maintenance.run_retention_once().await.is_err());
    assert_eq!(
        runtime
            .storage_snapshot()
            .expect("invalid policy unchanged"),
        before_invalid
    );
    std::fs::write(&path, valid_bytes).expect("restore valid user policy");
    let result = maintenance
        .run_retention_once()
        .await
        .expect("scheduled path");
    assert!(
        result.project_failures.is_empty(),
        "{:?}",
        result.project_failures
    );
    assert_eq!(result.recycled_memories, 2);
    assert_batches(runtime, 9200).await;
    let before_inherit = runtime.storage_snapshot().expect("before inherit");
    let inherited = maintenance
        .update_project_retention(VmmNativeProjectRetentionEdit::Inherit { project_id: a.id })
        .await
        .expect("restore a inheritance");
    assert!(inherited.overrides.is_none());
    assert_eq!(inherited.effective, inherited.global);
    assert_eq!(
        maintenance
            .project_retention_summary(b.id)
            .await
            .expect("b preserved")
            .overrides,
        Some(rules(259_200_000))
    );
    assert_eq!(
        runtime.storage_snapshot().expect("inherit changes no data"),
        before_inherit
    );
    assert_eq!(
        runtime
            .retention_settings()
            .await
            .expect("global unchanged"),
        global
    );
    assert!(!maintenance.host_ai_status().checked);
}

/// Checks persisted batches by their exact member IDs, independent of incidental batch ordering.
/// 按准确成员 ID 检查持久批次，不依赖偶然的批次顺序。
async fn assert_batches(runtime: &NativeVmmCoreRuntime, offset: usize) {
    let stored: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &runtime
            .host
            .data_plane
            .load_core_snapshot_json()
            .await
            .expect("SQLite read")
            .expect("SQLite snapshot"),
    )
    .expect("typed SQLite snapshot");
    for (index, duration) in [172_800_000_000_000_i64, 259_200_000_000_000]
        .iter()
        .enumerate()
    {
        let id = format!("vmm-memory-{}", offset + index);
        let batch = stored
            .retention_trash_batches
            .iter()
            .find(|batch| {
                batch
                    .dataset
                    .memories
                    .iter()
                    .any(|memory| memory.id.as_str() == id)
            })
            .expect("persisted owner batch");
        assert_eq!(batch.expires_at_ns - batch.recycled_at_ns, *duration);
        assert!(
            !stored
                .dataset
                .memories
                .iter()
                .any(|memory| memory.id.as_str() == id)
        );
        let vector = runtime
            .storage_snapshot()
            .expect("vector-bearing snapshot")
            .retention_trash_batches
            .into_iter()
            .flat_map(|batch| batch.dataset.memories)
            .find(|memory| memory.id.as_str() == id)
            .expect("retained vector");
        assert_eq!(vector.embedding, Some(vec![0.6, 0.8]));
    }
}
