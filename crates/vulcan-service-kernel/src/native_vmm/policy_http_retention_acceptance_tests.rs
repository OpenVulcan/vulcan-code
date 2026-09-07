//! Real cleanup effects after live global and memory-project policy saves.
//! 联网全局及记忆项目策略保存后的真实清洗效果。

use super::*;
#[path = "policy_http_schedule_acceptance_tests.rs"]
mod schedule;
use vulcan_memorymesh_core::{
    MemoryLevel, MemoryScopeClass, MemorySourceKind, SessionSourceId, SubjectId, Turn, TurnId,
    TurnStatus,
};
use vulcan_service_api::{
    VmmNativeProjectRetentionEdit, VmmNativeProjectRetentionRules,
    VmmNativeRetentionSettingsSummary,
};

/// One exact day expressed in milliseconds for independently declared duration expectations.
/// 以毫秒表示的准确一天，用于独立声明的时长期望。
const DAY: u64 = 86_400_000;
/// Stable owner tags belong only to this private acceptance environment.
/// 稳定归属标签仅属于本私有验收环境。
const HOSTS: [&str; 2] = ["http-retention-a", "http-retention-b"];

/// Returns one of three complete global generations; indexes are explicit fixture phases.
/// 返回三代完整全局值之一；索引是明确夹具阶段。
pub(super) fn global(generation: usize) -> VmmNativeRetentionSettingsSummary {
    match generation {
        0 => VmmNativeRetentionSettingsSummary {
            enabled: false,
            scan_interval_minutes: 7,
            profile_expiry_interval_seconds: 11,
            turn_hot_window_size: 3,
            session_idle_ms: 3 * DAY,
            trash_retention_ms: 2 * DAY,
            protected_priority_floor: 50,
            protected_memory_level_floor: 3,
            skip_protected_shared_memories: true,
        },
        1 => VmmNativeRetentionSettingsSummary {
            enabled: true,
            scan_interval_minutes: 2,
            profile_expiry_interval_seconds: 3,
            turn_hot_window_size: 1,
            session_idle_ms: DAY,
            trash_retention_ms: DAY,
            protected_priority_floor: 100,
            protected_memory_level_floor: 4,
            skip_protected_shared_memories: true,
        },
        2 => VmmNativeRetentionSettingsSummary {
            enabled: false,
            scan_interval_minutes: 3,
            profile_expiry_interval_seconds: 5,
            turn_hot_window_size: 4,
            session_idle_ms: 3 * DAY,
            trash_retention_ms: 4 * DAY,
            protected_priority_floor: 0,
            protected_memory_level_floor: 1,
            skip_protected_shared_memories: false,
        },
        _ => panic!("unexpected HTTP retention generation"),
    }
}

/// Returns the exact six-field project override without including global worker controls.
/// 返回准确六字段项目覆盖，不包含全局工作器控件。
fn owner() -> VmmNativeProjectRetentionRules {
    VmmNativeProjectRetentionRules {
        turn_hot_window_size: 2,
        session_idle_ms: DAY,
        trash_retention_ms: 3 * DAY,
        protected_priority_floor: 75,
        protected_memory_level_floor: 4,
        skip_protected_shared_memories: false,
    }
}

/// Holds the exact physical fixture identities and independently expected scope-local cleanup targets.
/// 持有准确物理夹具身份以及独立预期的作用域内清洗目标。
struct Sample {
    /// Exact stable memory-project identity from the creation receipt.
    /// 来自创建回执的准确稳定记忆项目身份。
    project: u64,
    /// Seven known records: six terminal protections and one expired session memory.
    /// 七条已知记录：六条终态保护样本及一条到期会话记忆。
    memories: Vec<MemoryRecord>,
    /// Five processed sources expose the configured hot-window suffix.
    /// 五条已处理来源暴露所配置的热窗口后缀。
    turns: Vec<Turn>,
}

/// Persists a bounded synthetic retention-time fixture through the real native snapshot transaction.
/// 通过真实原生快照事务持久化有界保留时间夹具。
/// Takes exact project, unique phase offset and a real vector-bearing template; returns the inserted originals.
/// 接收准确项目、唯一阶段偏移及真实带向量模板；返回插入的原始记录。
async fn seed(test: &Acceptance, project: u64, offset: usize, template: &MemoryRecord) -> Sample {
    // Real wall-clock time is used only to place samples far from the one-day/three-day boundaries.
    // 真实墙钟仅用于将样本置于远离一天／三天边界的位置。
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("HTTP clock")
            .as_nanos(),
    )
    .expect("HTTP nanoseconds");
    // This scope comes from a real catalog ID, never a display name.
    // 此作用域来自真实目录 ID，绝不使用显示名称。
    let scope = project_scope_id(project).expect("HTTP retention scope");
    // Samples independently isolate priority OR level protection at exact threshold edges.
    // 样本在准确阈值边缘独立区分优先级或等级保护。
    let cases = [
        (49, MemoryLevel::Phase),
        (50, MemoryLevel::Phase),
        (99, MemoryLevel::Session),
        (49, MemoryLevel::Stable),
        (50, MemoryLevel::Stable),
        (99, MemoryLevel::Persistent),
        (0, MemoryLevel::Session),
    ];
    // Every inserted record retains a real validated vector but receives explicit independent ownership.
    // 每条插入记录保留真实已校验向量，但获得明确独立归属。
    let memories = cases
        .into_iter()
        .enumerate()
        .map(|(index, (priority, level))| {
            // This fixture is not an inference result; all time and lifecycle fields are declared explicitly.
            // 本夹具不是推理结果；全部时间及生命周期字段显式声明。
            let mut memory = template.clone();
            memory.id =
                MemoryId::new(format!("vmm-memory-{}", offset + index)).expect("HTTP retention ID");
            memory.scope_id = scope.clone();
            memory.scope_class = if index == 6 {
                MemoryScopeClass::Session
            } else {
                MemoryScopeClass::Environment
            };
            memory.source_subject_id =
                Some(SubjectId::new("vmm-subject:1").expect("HTTP fixed subject"));
            memory.source_turn_id = None;
            memory.source_kind = MemorySourceKind::Direct;
            memory.origin_session_source_id = if index == 6 {
                Some(
                    SessionSourceId::new(format!("http-idle-{offset}")).expect("HTTP idle session"),
                )
            } else {
                None
            };
            memory.priority = priority;
            memory.memory_level = level;
            memory.state = if index == 6 {
                MemoryState::Active
            } else {
                MemoryState::Deleted
            };
            memory.created_at_ns = now - 4 * 86_400_000_000_000;
            memory.updated_at_ns = now - 2 * 86_400_000_000_000;
            memory.expires_at_ns = if index == 6 {
                Some(now - 2 * 86_400_000_000_000)
            } else {
                None
            };
            memory.last_recalled_at_ns = None;
            memory.last_adopted_at_ns = None;
            memory.last_reinforced_at_ns = None;
            memory.dedupe_hash = None;
            memory.evidence.clear();
            memory.summary = format!("HTTP retention sample {offset} {index}");
            memory.content = memory.summary.clone();
            memory
        })
        .collect::<Vec<_>>();
    // Processed-unknown is an existing legitimate cold-turn state, requiring no model inference during cleanup.
    // 已处理但未知是既有合法冷回合状态，清洗时不要求模型推理。
    let turns = (0..5)
        .map(|index| Turn {
            id: TurnId::new(format!("vmm-turn-{}", offset + index))
                .expect("HTTP retention turn ID"),
            subject_id: SubjectId::new("vmm-subject:1").expect("HTTP fixed turn subject"),
            scope_id: scope.clone(),
            session_source_id: SessionSourceId::new(format!("http-hot-{offset}"))
                .expect("HTTP hot session"),
            // Nonempty canonical endpoints make truncation or source loss observable in the recycled record.
            // 非空规范端点使回收记录中的截断或来源丢失可观察。
            timeline: vec![
                vulcan_memorymesh_core::TimelineItem {
                    role: vulcan_memorymesh_core::Role::User,
                    content: format!("HTTP retention source {offset} {index}\n保留此原始说明。"),
                    occurred_at_ns: now - 20 + index as i64,
                },
                vulcan_memorymesh_core::TimelineItem {
                    role: vulcan_memorymesh_core::Role::Assistant,
                    content:
                        "The source remains available in the recycle batch.\n回收批次保留来源。"
                            .to_owned(),
                    occurred_at_ns: now - 10 + index as i64,
                },
            ],
            previous_turn_ids: Vec::new(),
            next_turn_ids: Vec::new(),
            created_at_ns: now - 10 + index as i64,
            status: TurnStatus::ProcessedUnknown,
            user_input_kind: None,
            analysis_details: None,
        })
        .collect::<Vec<_>>();
    // The returned originals remain independent of all later preview/result responses.
    // 返回的原始记录独立于全部后续预览／结果响应。
    let sample = Sample {
        project,
        memories,
        turns,
    };
    // Validate collision-free fixture identity before touching the private snapshot.
    // 修改私有快照前校验夹具身份无冲突。
    let before = test.runtime.storage_snapshot().expect("HTTP seed snapshot");
    assert!(
        sample.memories.iter().all(|new| before
            .dataset
            .memories
            .iter()
            .all(|old| old.id != new.id))
    );
    assert!(
        sample
            .turns
            .iter()
            .all(|new| before.dataset.turns.iter().all(|old| old.id != new.id))
    );
    test.runtime
        .transform_storage_snapshot(
            &format!("http-retention-seed-{offset}"),
            "test",
            before.storage_version,
            |snapshot| {
                snapshot.dataset.memories.extend(sample.memories.clone());
                snapshot.dataset.turns.extend(sample.turns.clone());
                Ok(())
            },
        )
        .await
        .expect("HTTP persisted retention fixture");
    sample
}

/// Checks actual preview and confirmed physical movement for the two current samples.
/// 检查两个当前样本的实际预览与确认后的物理移动。
/// Takes independently expected terminal indexes, hot window, idle eligibility and admitted batch duration.
/// 接收独立预期终态索引、热窗口、闲置资格及准入批次时长。
async fn clean(
    test: &Acceptance,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    samples: &[(Sample, Vec<usize>, usize, bool, u64)],
) {
    // Snapshot the complete prior recycle batches so later saves cannot alter their deadlines or contents.
    // 对全部既有回收批次取快照，使后续保存不能改变其期限或内容。
    let before = test
        .runtime
        .storage_snapshot()
        .expect("HTTP cleanup before");
    // Cleanup must not invoke the inference fixture at all.
    // 清洗绝不能调用推理夹具。
    let calls = test
        .port
        .requests
        .lock()
        .expect("HTTP cleanup inference count")
        .len();
    // The actual production preview is read-only and freezes exact stable IDs.
    // 实际生产预览只读并冻结准确稳定 ID。
    let preview = maintenance
        .preview_retention_cleanup()
        .await
        .expect("HTTP actual cleanup preview");
    assert_eq!(
        test.runtime
            .storage_snapshot()
            .expect("HTTP preview unchanged"),
        before
    );
    for (sample, terminal, hot, idle, _) in samples {
        // Only this exact project receipt owns the preview's effect list.
        // 仅此准确项目回执持有预览效果列表。
        let project = preview
            .projects
            .iter()
            .find(|item| item.project_id == sample.project)
            .expect("HTTP preview project");
        for (index, memory) in sample.memories.iter().enumerate() {
            assert_eq!(
                project
                    .recycled_memory_ids
                    .contains(&memory.id.as_str().to_owned()),
                terminal.contains(&index) || (index == 6 && *idle)
            );
        }
        for (index, turn) in sample.turns.iter().enumerate() {
            assert_eq!(
                project
                    .recycled_turn_ids
                    .contains(&turn.id.as_str().to_owned()),
                index < 5 - hot
            );
        }
    }
    // Confirmation consumes exactly the production-issued preview token.
    // 确认准确消费生产发出的预览令牌。
    let result = maintenance
        .execute_retention_cleanup(&uuid::Uuid::new_v4().to_string(), &preview.preview_id)
        .await
        .expect("HTTP confirmed cleanup");
    assert!(
        result.project_failures.is_empty(),
        "{:?}",
        result.project_failures
    );
    assert_eq!(result.purged_trash_batches, 0);
    // The runtime snapshot materializes vectors from their real physical owner.
    // 运行时快照从真实物理归属方物化向量。
    let after = test.runtime.storage_snapshot().expect("HTTP cleanup after");
    assert_eq!(
        test.port
            .requests
            .lock()
            .expect("HTTP cleanup no inference")
            .len(),
        calls
    );
    assert_eq!(after.dataset.profile_nodes, before.dataset.profile_nodes);
    for batch in &before.retention_trash_batches {
        assert_eq!(
            after
                .retention_trash_batches
                .iter()
                .find(|next| next.batch_id == batch.batch_id),
            Some(batch)
        );
    }
    for (sample, terminal, hot, idle, duration) in samples {
        for (index, memory) in sample.memories.iter().enumerate() {
            // Exact independently selected identities must disappear from active storage and retain full vector-bearing records in trash.
            // 独立选出的准确身份必须从活动存储消失，并在回收站保留完整带向量记录。
            let recycled = terminal.contains(&index) || (index == 6 && *idle);
            assert_eq!(
                after
                    .dataset
                    .memories
                    .iter()
                    .find(|item| item.id == memory.id),
                if recycled { None } else { Some(memory) }
            );
            if recycled {
                // A newly created batch freezes the effective owner duration at confirmation.
                // 新建批次在确认时冻结有效归属方时长。
                let batch = after
                    .retention_trash_batches
                    .iter()
                    .find(|batch| {
                        batch
                            .dataset
                            .memories
                            .iter()
                            .any(|item| item.id == memory.id)
                    })
                    .expect("HTTP actual retained memory");
                assert_eq!(
                    batch
                        .dataset
                        .memories
                        .iter()
                        .find(|item| item.id == memory.id),
                    Some(memory)
                );
                assert_eq!(
                    batch.expires_at_ns - batch.recycled_at_ns,
                    i64::try_from(*duration).expect("HTTP trash duration") * 1_000_000
                );
            }
        }
        for (index, turn) in sample.turns.iter().enumerate() {
            assert_eq!(
                after.dataset.turns.iter().find(|item| item.id == turn.id),
                if index < 5 - hot { None } else { Some(turn) }
            );
            if index < 5 - hot {
                // Disappearance from active storage is insufficient: the complete source must be durably retained.
                // 仅从活动存储消失并不足够：完整来源必须耐久保留。
                let batch = after
                    .retention_trash_batches
                    .iter()
                    .find(|batch| batch.dataset.turns.iter().any(|item| item.id == turn.id))
                    .expect("HTTP actual retained turn");
                assert_eq!(
                    batch.dataset.turns.iter().find(|item| item.id == turn.id),
                    Some(turn)
                );
                assert_eq!(
                    batch.expires_at_ns - batch.recycled_at_ns,
                    i64::try_from(*duration).expect("HTTP turn trash duration") * 1_000_000
                );
            }
        }
    }
}

/// Runs five page-save phases and real confirmed cleanups; returns the stable overridden project for reload checks.
/// 运行五个页面保存阶段及真实确认清洗；返回稳定覆盖项目以检查重载。
/// Prior scenario projects receive explicit test-only protection so their records do not interfere with this matrix.
/// 前序场景项目获得明确仅用于测试的保护，避免其记录干扰本矩阵。
pub(super) async fn verify(test: &Acceptance, root: &Path) -> u64 {
    // This is the same production maintenance handle exposed by the private HTTP service.
    // 这是私有 HTTP 服务暴露的同一个生产维护句柄。
    let maintenance = test
        .service
        .lock()
        .expect("HTTP retention service")
        .native_vmm_maintenance
        .clone()
        .expect("HTTP retention maintenance");
    maintenance.set_native_backend_enabled(true);
    for project in test
        .data_plane
        .list_projects()
        .await
        .expect("HTTP prior projects")
    {
        maintenance
            .update_project_retention(VmmNativeProjectRetentionEdit::Override {
                project_id: project.id,
                rules: VmmNativeProjectRetentionRules {
                    turn_hot_window_size: u32::MAX,
                    session_idle_ms: 3650 * DAY,
                    trash_retention_ms: DAY,
                    protected_priority_floor: 0,
                    protected_memory_level_floor: 1,
                    skip_protected_shared_memories: true,
                },
            })
            .await
            .expect("HTTP protect prior samples");
    }
    // New scope identities are obtained only from successful creation/binding receipts.
    // 新作用域身份仅来自成功创建／绑定回执。
    let a = maintenance
        .create_and_bind_project(HOSTS[0], "HTTP retention A")
        .await
        .expect("HTTP retention A");
    // The sibling stays inherited across A's override and restoration.
    // A 覆盖及恢复期间，同级保持继承。
    let b = maintenance
        .create_and_bind_project(HOSTS[1], "HTTP retention B")
        .await
        .expect("HTTP retention B");
    // Preserve preexisting records and unrelated policy groups independently of frontend results.
    // 独立于前端结果保留既有记录及无关配置组。
    let original = test
        .runtime
        .storage_snapshot()
        .expect("HTTP retention original");
    // Use one physically materialized vector record only as a structural fixture template.
    // 仅将一条已物化物理向量记录用作结构夹具模板。
    let template = original
        .dataset
        .memories
        .first()
        .expect("HTTP existing memory template")
        .clone();
    assert!(template.embedding.is_some());
    // Global non-retention fields must remain exact throughout all page saves.
    // 全部页面保存期间，全局非保留字段必须保持准确。
    let initial = test
        .runtime
        .runtime_policy()
        .await
        .expect("HTTP retention initial policy");
    wait_phase(root, "retention-setup").await;
    acknowledge(root, "retention-setup");
    // Each tuple declares independent expected terminal indexes, hot windows and idle eligibility.
    // 每个元组声明独立预期终态索引、热窗口及闲置资格。
    let phases = [
        ("retention-global", 0, false, vec![0], 3, false),
        ("retention-project", 0, true, vec![0], 3, false),
        ("retention-changed", 1, true, vec![0, 1, 2, 3, 4], 1, true),
        ("retention-inherit", 1, false, vec![0, 1, 2, 3, 4], 1, true),
        (
            "retention-unprotected",
            2,
            false,
            vec![0, 1, 2, 3, 4, 5],
            4,
            false,
        ),
    ];
    for (index, (phase, generation, overridden, indexes, hot, idle)) in
        phases.into_iter().enumerate()
    {
        // Configuration-only work must not modify a complete physical snapshot.
        // 仅配置操作不得修改完整物理快照。
        let before_save = test
            .runtime
            .storage_snapshot()
            .expect("HTTP before retention save");
        wait_phase(root, phase).await;
        assert_eq!(
            test.runtime
                .storage_snapshot()
                .expect("HTTP retention save no mutation"),
            before_save
        );
        // The independent expected whole group must equal the actual runtime state.
        // 独立预期整组必须等于实际运行时状态。
        let expected = global(generation);
        // Real status consumes global scheduler settings even though this phase uses explicit confirmed cleanup.
        // 即使本阶段使用显式确认清洗，真实状态仍消费全局调度设置。
        let status = maintenance
            .retention_status()
            .await
            .expect("HTTP retention status");
        assert_eq!(status.enabled, expected.enabled);
        assert_eq!(
            status.interval_minutes,
            u64::from(expected.scan_interval_minutes)
        );
        assert_eq!(
            status.profile_expiry_interval_seconds,
            u64::from(expected.profile_expiry_interval_seconds)
        );
        // No actual worker clock is claimed by this status check; cadence acceptance remains separate.
        // 本状态检查不宣称覆盖实际工作器时钟；周期验收保持独立。
        let saved = test
            .runtime
            .runtime_policy()
            .await
            .expect("HTTP saved retention");
        assert_eq!(saved.retention, expected);
        assert_eq!(saved.profiles, initial.profiles);
        assert_eq!(saved.text, initial.text);
        assert_eq!(saved.recall, initial.recall);
        assert_eq!(saved.retrieval, initial.retrieval);
        // Project groups have no worker control keys and keep their stable memory ownership.
        // 项目组没有工作器控制键，并保持稳定记忆归属。
        let local = test
            .runtime
            .project_retention_summary(a.id)
            .await
            .expect("HTTP A retention");
        assert_eq!(
            local.overrides,
            if overridden { Some(owner()) } else { None }
        );
        assert_eq!(
            local.effective,
            if overridden { owner() } else { local.global }
        );
        assert!(
            test.runtime
                .project_retention_summary(b.id)
                .await
                .expect("HTTP B retention")
                .overrides
                .is_none()
        );
        // New samples use collision-free exact numeric identities; earlier successful batches remain untouched.
        // 新样本使用无冲突准确数字身份；之前成功批次保持不变。
        let sample_a = seed(test, a.id, 9_000_000 + index * 100, &template).await;
        // The second project has the same data but independent policy ownership.
        // 第二个项目具有相同数据及独立策略归属。
        let sample_b = seed(test, b.id, 9_000_020 + index * 100, &template).await;
        clean(
            test,
            &maintenance,
            &[
                (
                    sample_a,
                    if overridden {
                        vec![0, 1, 2, 3, 4, 5]
                    } else {
                        indexes.clone()
                    },
                    if overridden { 2 } else { hot },
                    overridden || idle,
                    if overridden {
                        3 * DAY
                    } else {
                        expected.trash_retention_ms
                    },
                ),
                (sample_b, indexes, hot, idle, expected.trash_retention_ms),
            ],
        )
        .await;
        // Explicit fixture protection preserves every record from the prior HTTP scenarios.
        // 明确夹具保护保留前序 HTTP 场景的每条记录。
        let after = test
            .runtime
            .storage_snapshot()
            .expect("HTTP earlier scenarios unchanged");
        for old in &original.dataset.memories {
            assert_eq!(
                after.dataset.memories.iter().find(|item| item.id == old.id),
                Some(old)
            );
        }
        for old in &original.dataset.turns {
            assert_eq!(
                after.dataset.turns.iter().find(|item| item.id == old.id),
                Some(old)
            );
        }
        acknowledge(root, phase);
    }
    schedule::verify(test, root, &maintenance, a.id).await;
    a.id
}
