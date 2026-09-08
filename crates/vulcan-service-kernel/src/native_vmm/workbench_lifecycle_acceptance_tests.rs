//! Real browser lifecycle assertions over normal intake, direct writes and both profile roles.
//! 基于普通入库、主动写入和两类画像的真实浏览器生命周期断言。

use super::*;
use std::collections::BTreeSet;
use vulcan_memorymesh_core::{
    StorageManualRecycleSelection, StorageSessionStatus, StorageSnapshot,
};
use vulcan_vmm::{ApplyProfileInstructionRequest, PostActionRequest, ProfileTarget};

/// The exact synthetic source selected by every browser management operation.
/// 每次浏览器管理操作选择的准确合成来源。
const SESSION: &str = "browser-native-parity-seed";
const BATCH_SESSION: &str = "browser-native-parity-batch";
const PRESERVED_SESSION: &str = "browser-unselected";

/// Creates seven primary turns, two batch-session turns, one preserved turn, and two profiles.
/// 创建七个主会话回合、两个批量会话回合、一个保留回合和两个画像。
/// Takes the private acceptance and stable project ID; returns after normal public writes complete.
/// 接收私有验收及稳定项目身份；普通公开写入完成后返回。
pub(super) async fn prepare(test: &Acceptance, project: u64) {
    // Match the existing HTTP intake fixture: isolate semantic prototypes during seed preparation only.
    // 与既有 HTTP 入库夹具一致：仅在种子准备期间隔离语义原型。
    let original_settings = test
        .runtime
        .resource_catalog()
        .await
        .expect("browser original resources")
        .settings;
    // Normal sanitation, durable intake and lifecycle processing remain production behavior.
    // 正常清理、持久入库及生命周期处理仍使用生产行为。
    let mut settings = original_settings.clone();
    settings.noise_semantic_enabled = false;
    test.runtime
        .update_resource_settings(settings)
        .await
        .expect("browser seed semantic isolation");
    for index in 1..=10 {
        // The eighth and ninth sources form the second exact batch-selection target.
        // 第八和第九个来源组成第二个准确批量选择目标。
        let session = match index {
            8 | 9 => BATCH_SESSION,
            10 => PRESERVED_SESSION,
            _ => SESSION,
        };
        // Every operation has a distinct durable queue identity, independent of generated turn IDs.
        // 每个操作具有不同持久队列身份，独立于生成回合身份。
        let response = test
            .client
            .post_action(PostActionRequest {
                host_project_id: "replacement-host-b".to_owned(),
                project_id: project,
                user_id: 1,
                session_id: session.to_owned(),
                operation_id: format!("browser-source-{index}"),
                user_content: format!("浏览器验收第 {index} 项：使用 PostgreSQL 保存业务数据。"),
                assistant_content: format!("Confirmed database decision {index}."),
                ..Default::default()
            })
            .await
            .expect("browser real source intake");
        assert!(response.accepted);
    }
    for (target, role) in [
        (ProfileTarget::ProfileTargetUser, "user"),
        (ProfileTarget::ProfileTargetProject, "project"),
    ] {
        // The deterministic provider interprets the declared instruction, not a fabricated stored node.
        // 确定性供应商解释声明指令，而非伪造存储节点。
        let response = test
            .client
            .apply_profile_instruction(ApplyProfileInstructionRequest {
                host_project_id: "replacement-host-b".to_owned(),
                project_id: project,
                user_id: 1,
                target,
                instruction: format!("HTTP profile browser-lifecycle {role} L2"),
            })
            .await
            .expect("browser real profile instruction");
        assert_eq!(response.accepted_nodes.len(), 1);
    }
    test.runtime
        .update_resource_settings(original_settings)
        .await
        .expect("browser restore resource settings");
}

/// Selects one known synthetic source metadata row without guessing project ownership.
/// 选择一个已知合成来源元数据行，不猜测项目归属。
/// Takes a snapshot; returns its exact source metadata index, asserting uniqueness.
/// 接收快照；返回准确来源元数据下标，并断言唯一性。
fn session_index_for(snapshot: &StorageSnapshot, source_id: &str) -> usize {
    // This fixture creates exactly one session with each source identity across all projects.
    // 本夹具在全部项目中为每个来源身份只创建一个会话。
    let indices = snapshot
        .memory_sessions
        .iter()
        .enumerate()
        .filter(|(_, session)| session.session_source_id.as_str() == source_id)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(indices.len(), 1);
    indices[0]
}

/// Selects the primary session row used by the existing lifecycle matrix.
/// 选择既有生命周期矩阵使用的主会话行。
fn session_index(snapshot: &StorageSnapshot) -> usize {
    session_index_for(snapshot, SESSION)
}

/// Returns one canonical numeric turn identity from a known session and position.
/// 从已知会话及位置返回一个规范数字回合身份。
fn turn_id_for(snapshot: &StorageSnapshot, source_id: &str, position: usize) -> String {
    // The browser project overview orders these Core numeric turn identities chronologically;
    // the durable snapshot itself is not the UI ordering contract.
    // 浏览器项目总览按 Core 数字回合身份按时间顺序展示；持久快照本身不是 UI 排序契约。
    let mut turn_ids = snapshot
        .dataset
        .turns
        .iter()
        .filter(|turn| turn.session_source_id.as_str() == source_id)
        .map(|turn| {
            (
                crate::native_vmm::client::parse_core_numeric_id(&turn.id, "vmm-turn-")
                    .expect("numeric browser turn"),
                turn.id.to_string(),
            )
        })
        .collect::<Vec<_>>();
    turn_ids.sort_by_key(|(numeric_id, _)| *numeric_id);
    turn_ids
        .into_iter()
        .nth(position)
        .map(|(_, turn_id)| turn_id)
        .expect("known browser turn")
}

/// Confirms both relational durability and the absence of unsolicited model or embedding work.
/// 确认关系持久化及没有未经请求的模型或嵌入工作。
/// Takes the exact expected snapshot and inference counts; returns after independent physical reads.
/// 接收准确预期快照和推理计数；独立物理读取后返回。
async fn persisted(test: &Acceptance, expected: &StorageSnapshot, inference: (usize, usize)) {
    assert_eq!(
        test.runtime
            .storage_snapshot()
            .expect("browser current state"),
        *expected
    );
    // Read the actual committed SQLite snapshot instead of treating memory projection as durable proof.
    // 读取实际提交的关系快照，不把内存投影视为耐久证据。
    let mut durable: StorageSnapshot = serde_json::from_str(
        &test
            .data_plane
            .load_core_snapshot_json()
            .await
            .expect("browser durable read")
            .expect("browser durable JSON"),
    )
    .expect("browser durable snapshot");
    // Restore appends physical turns; Core exports the same full rows in TurnId map order.
    // 恢复追加物理回合；Core 按回合身份映射顺序导出相同完整行。
    durable
        .dataset
        .turns
        .sort_by(|left, right| left.id.cmp(&right.id));
    assert_eq!(durable, expected.without_embeddings());
    assert_eq!(
        test.port
            .requests
            .lock()
            .expect("browser inference count")
            .len(),
        inference.0
    );
    assert_eq!(
        test.port
            .embedding_requests
            .lock()
            .expect("browser embedding count")
            .len(),
        inference.1
    );
}

/// Checks a whole-session move against every original record and the exact retained selection.
/// 对照每条原记录和准确保留选择检查完整会话移动。
/// Takes pre-move and actual snapshots plus the frozen retention duration; returns the verified new snapshot.
/// 接收移动前及实际快照和冻结保留时长；返回已验证的新快照。
fn recycled(
    before: &StorageSnapshot,
    actual: StorageSnapshot,
    retention_ms: u64,
) -> StorageSnapshot {
    // Exact session metadata supplies both owner identities and the original selection state.
    // 准确会话元数据提供两个归属身份及原选择状态。
    let index = session_index(before);
    // Compare the sole new batch separately before accepting its generated identity and timestamp.
    // 接受生成身份和时间戳前，独立比较唯一新增批次。
    assert!(before.retention_trash_batches.is_empty());
    assert_eq!(
        actual.retention_trash_batches.len(),
        1,
        "browser session recycle snapshot: version={}, sessions={:?}, active_turns={}, active_memories={}, batches={:?}",
        actual.storage_version,
        actual
            .memory_sessions
            .iter()
            .map(|session| (session.session_source_id.as_str(), session.status))
            .collect::<Vec<_>>(),
        actual.dataset.turns.len(),
        actual.dataset.memories.len(),
        actual
            .retention_trash_batches
            .iter()
            .map(|batch| (
                batch.batch_id.as_str(),
                batch.reason,
                batch.dataset.turns.len(),
                batch.dataset.memories.len()
            ))
            .collect::<Vec<_>>(),
    );
    let batch = &actual.retention_trash_batches[0];
    assert!(batch.batch_id.starts_with("manual:"));
    assert_eq!(batch.subject_id, before.memory_sessions[index].subject_id);
    assert_eq!(batch.scope_id, before.memory_sessions[index].scope_id);
    assert_eq!(
        batch.reason,
        vulcan_memorymesh_core::RetentionReasonCode::ManualSessionRecycle
    );
    assert!(batch.recycled_at_ns > 0);
    assert_eq!(
        batch.expires_at_ns - batch.recycled_at_ns,
        i64::try_from(retention_ms).expect("browser finite retention") * 1_000_000
    );
    assert_eq!(
        batch.manual_selection,
        Some(StorageManualRecycleSelection::Sessions {
            sessions: vec![before.memory_sessions[index].clone()]
        })
    );
    assert!(batch.analysis_attempts.is_empty());
    // Normal intake deliberately leaves no extracted facts beyond the three direct-write memories.
    // 普通入库刻意不生成三个主动写入记忆之外的已提炼事实。
    let mut moved = before.dataset.clone();
    moved
        .turns
        .retain(|turn| turn.session_source_id.as_str() == SESSION);
    moved.profile_nodes.clear();
    assert_eq!(moved.turns.len(), 7);
    assert_eq!(moved.memories.len(), 3);
    assert!(moved.chat_compact_boundaries.is_empty());
    assert_eq!(batch.dataset, moved);
    // Every other field remains independently frozen; both profile roles and the eighth turn survive.
    // 其他每个字段均独立冻结；两类画像及第八个回合保持。
    let mut expected = before.clone();
    expected.storage_version += 1;
    expected.dataset.memories.clear();
    expected
        .dataset
        .turns
        .retain(|turn| turn.session_source_id.as_str() != SESSION);
    expected.memory_sessions[index].status = StorageSessionStatus::Recycled;
    expected.memory_sessions[index].last_extraction_observed_at_ns = 0;
    expected.memory_sessions[index].last_extraction_completed_at_ns = None;
    expected.retention_trash_batches.push(batch.clone());
    assert_eq!(actual, expected);
    actual
}

/// Verifies real workbench selection of multiple sessions and turns before the single-session matrix.
/// 在单会话矩阵前验证真实工作台的多会话及多回合选择。
/// Takes the baseline snapshot and requires every browser phase to return to that exact baseline.
/// 接收基线快照，并要求每个浏览器阶段都恢复到该准确基线。
pub(super) async fn verify_selection_matrix(
    test: &Acceptance,
    root: &Path,
    before: &StorageSnapshot,
) {
    // Three independent source sessions make cross-session selection observable.
    // 三个独立来源会话使跨会话选择可被准确观察。
    assert_eq!(before.memory_sessions.len(), 3);
    assert_eq!(before.dataset.turns.len(), 10);
    assert_eq!(before.dataset.memories.len(), 3);
    assert!(before.retention_trash_batches.is_empty());
    let jobs = test
        .data_plane
        .list_active_pending_analysis_jobs()
        .await
        .expect("browser selection baseline queue");
    assert_eq!(jobs.len(), 10);
    let inference = (
        test.port
            .requests
            .lock()
            .expect("browser selection inference baseline")
            .len(),
        test.port
            .embedding_requests
            .lock()
            .expect("browser selection embedding baseline")
            .len(),
    );

    browser_phase(root, "browser-batch-recycled").await;
    let batch_recycled = test
        .runtime
        .storage_snapshot()
        .expect("browser batch recycled");
    assert_eq!(batch_recycled.storage_version, before.storage_version + 1);
    assert_eq!(batch_recycled.retention_trash_batches.len(), 1);
    let batch = &batch_recycled.retention_trash_batches[0];
    assert_eq!(
        batch.reason,
        vulcan_memorymesh_core::RetentionReasonCode::ManualSessionRecycle
    );
    let selected_session_rows = before
        .memory_sessions
        .iter()
        .filter(|session| [SESSION, BATCH_SESSION].contains(&session.session_source_id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        batch.manual_selection,
        Some(StorageManualRecycleSelection::Sessions {
            sessions: selected_session_rows,
        })
    );
    let selected_turn_ids = before
        .dataset
        .turns
        .iter()
        .filter(|turn| [SESSION, BATCH_SESSION].contains(&turn.session_source_id.as_str()))
        .map(|turn| turn.id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        batch
            .dataset
            .turns
            .iter()
            .map(|turn| turn.id.clone())
            .collect::<BTreeSet<_>>(),
        selected_turn_ids
    );
    assert_eq!(batch.dataset.memories.len(), 3);
    assert_eq!(batch_recycled.dataset.turns.len(), 1);
    assert!(batch_recycled.dataset.memories.is_empty());
    for source_id in [SESSION, BATCH_SESSION] {
        assert_eq!(
            batch_recycled.memory_sessions[session_index_for(before, source_id)].status,
            StorageSessionStatus::Recycled
        );
    }
    assert_eq!(
        batch_recycled.memory_sessions[session_index_for(before, PRESERVED_SESSION)].status,
        StorageSessionStatus::Active
    );
    assert_eq!(
        test.data_plane
            .list_active_pending_analysis_jobs()
            .await
            .expect("browser batch queue"),
        jobs
    );
    assert_eq!(
        (
            test.port.requests.lock().expect("batch LLM count").len(),
            test.port
                .embedding_requests
                .lock()
                .expect("batch embedding count")
                .len(),
        ),
        inference
    );
    acknowledge(root, "browser-batch-recycled");

    browser_phase(root, "browser-batch-restored").await;
    let mut expected = before.clone();
    expected.storage_version += 2;
    assert_eq!(
        test.runtime
            .storage_snapshot()
            .expect("browser batch restored"),
        expected
    );
    assert_eq!(
        test.data_plane
            .list_active_pending_analysis_jobs()
            .await
            .expect("browser restored batch queue"),
        jobs
    );
    acknowledge(root, "browser-batch-restored");

    let mut selected_turn_ids = vec![
        turn_id_for(before, SESSION, 0),
        turn_id_for(before, BATCH_SESSION, 0),
    ];
    selected_turn_ids.sort();
    let selected_turn_id_values = selected_turn_ids
        .iter()
        .cloned()
        .map(|id| vulcan_memorymesh_core::TurnId::new(id).expect("browser selected turn"))
        .collect::<Vec<_>>();
    browser_phase(root, "browser-turns-recycled").await;
    let turns_recycled = test
        .runtime
        .storage_snapshot()
        .expect("browser turns recycled");
    // The batch recycle/restore pair already committed two versions before this turn recycle.
    // 批量回收/恢复已经在本回合回收前提交两个版本。
    assert_eq!(turns_recycled.storage_version, before.storage_version + 3);
    assert_eq!(turns_recycled.retention_trash_batches.len(), 1);
    let turn_batch = &turns_recycled.retention_trash_batches[0];
    assert_eq!(
        turn_batch.reason,
        vulcan_memorymesh_core::RetentionReasonCode::ManualTurnRecycle
    );
    assert_eq!(
        turn_batch.manual_selection,
        Some(StorageManualRecycleSelection::Turns {
            turn_ids: selected_turn_id_values,
        })
    );
    assert_eq!(
        turn_batch
            .dataset
            .turns
            .iter()
            .map(|turn| turn.id.to_string())
            .collect::<Vec<_>>(),
        selected_turn_ids
    );
    assert!(turn_batch.dataset.memories.is_empty());
    assert_eq!(turns_recycled.dataset.turns.len(), 8);
    assert_eq!(turns_recycled.dataset.memories.len(), 3);
    assert!(
        turns_recycled
            .memory_sessions
            .iter()
            .all(|session| session.status == StorageSessionStatus::Active)
    );
    assert_eq!(
        test.data_plane
            .list_active_pending_analysis_jobs()
            .await
            .expect("browser turn queue"),
        jobs
    );
    acknowledge(root, "browser-turns-recycled");

    browser_phase(root, "browser-turns-restored").await;
    expected = before.clone();
    expected.storage_version += 4;
    assert_eq!(
        test.runtime
            .storage_snapshot()
            .expect("browser turns restored"),
        expected
    );
    assert_eq!(
        test.data_plane
            .list_active_pending_analysis_jobs()
            .await
            .expect("browser restored turn queue"),
        jobs
    );
    acknowledge(root, "browser-turns-restored");
}

/// Verifies browser-driven archive, unarchive, recycle, restore, purge and explicit restart.
/// 验证浏览器驱动的归档、取消归档、回收、恢复、永久清除及明确重新启用。
/// Takes private state and the prepared baseline; every phase checks durable whole-state changes and queues.
/// 接收私有状态及准备基线；每阶段检查耐久完整状态变化和队列。
pub(super) async fn verify(test: &Acceptance, root: &Path, before: &StorageSnapshot) {
    // Both profile roles must be nonempty before source removal begins.
    // 来源移除开始前，两类画像必须非空。
    assert_eq!(before.dataset.profile_nodes.len(), 2);
    assert_eq!(before.dataset.turns.len(), 10);
    assert_eq!(before.memory_sessions.len(), 3);
    assert!(before.dataset.context_evidence.is_empty());
    assert!(before.analysis_attempts.is_empty());
    // Resolve the actual physical vector owner and retain exact values before any source move.
    // 解析实际物理向量归属，并在任何来源移动前保留准确值。
    let binding = test
        .data_plane
        .resolve_vector_space(
            before.dataset.memories[0]
                .vector_space_id
                .as_ref()
                .expect("browser memory vector space")
                .as_str(),
        )
        .await
        .expect("browser vector binding")
        .expect("browser physical binding");
    // Stable memory IDs remain the lookup authority even after a record leaves active storage.
    // 稳定记忆身份在记录离开活动存储后仍是查询权威。
    let memory_ids = before
        .dataset
        .memories
        .iter()
        .map(|memory| memory.id.to_string())
        .collect::<Vec<_>>();
    let vectors = test
        .data_plane
        .load_vectors(&binding, &memory_ids)
        .await
        .expect("browser original physical vectors");
    assert_eq!(vectors.len(), 3);
    // All ten operations came from real intake, and management may not replay their content.
    // 十个操作均来自真实入库，管理不得重放其正文。
    let jobs = test
        .data_plane
        .list_active_pending_analysis_jobs()
        .await
        .expect("browser prepared queue");
    assert_eq!(jobs.len(), 10);
    // Release capture locks before any browser wait or asynchronous storage call.
    // 在任何浏览器等待或异步存储调用前释放捕获锁。
    let inference = (
        test.port
            .requests
            .lock()
            .expect("browser inference baseline")
            .len(),
        test.port
            .embedding_requests
            .lock()
            .expect("browser embeddings baseline")
            .len(),
    );
    // Lifecycle classification changes only status and the committed transform version.
    // 生命周期分类只改变状态及已提交转换版本。
    // The selection matrix has already completed two recycle/restore round trips before this lifecycle begins.
    // 前面的选择矩阵已经完成两次回收/恢复往返，因此本生命周期必须从累计版本基线开始。
    let mut expected = before.clone();
    expected.storage_version += 4;
    let index = session_index(before);
    for (phase, status) in [
        ("browser-archived", StorageSessionStatus::Archived),
        ("browser-unarchived", StorageSessionStatus::Active),
    ] {
        browser_phase(root, phase).await;
        expected.storage_version += 1;
        expected.memory_sessions[index].status = status;
        persisted(test, &expected, inference).await;
        assert_eq!(
            test.data_plane
                .list_active_pending_analysis_jobs()
                .await
                .expect("browser classification queue"),
            jobs
        );
        acknowledge(root, phase);
    }
    // The browser may only restore the exact batch created by the first source recycle.
    // 浏览器只能恢复首次来源回收创建的准确批次。
    let roundtrip = expected.clone();
    let retention_ms = test
        .runtime
        .runtime_policy()
        .await
        .expect("browser retention")
        .retention
        .trash_retention_ms;
    browser_phase(root, "browser-recycled").await;
    expected = recycled(
        &expected,
        test.runtime.storage_snapshot().expect("browser recycled"),
        retention_ms,
    );
    persisted(test, &expected, inference).await;
    assert_eq!(
        test.data_plane
            .list_active_pending_analysis_jobs()
            .await
            .expect("browser recoverable queue"),
        jobs
    );
    acknowledge(root, "browser-recycled");
    browser_phase(root, "browser-sources-restored").await;
    expected = roundtrip;
    expected.storage_version += 2;
    persisted(test, &expected, inference).await;
    assert_eq!(
        test.data_plane
            .list_active_pending_analysis_jobs()
            .await
            .expect("browser restored queue"),
        jobs
    );
    acknowledge(root, "browser-sources-restored");
    // Restore must reuse the exact original physical vectors, never regenerate approximations.
    // 恢复必须复用准确原物理向量，绝不重新生成近似值。
    let restored_vectors = test
        .data_plane
        .load_vectors(&binding, &memory_ids)
        .await
        .expect("browser restored physical vectors");
    assert_eq!(restored_vectors.len(), vectors.len());
    for original in &vectors {
        let restored = restored_vectors
            .iter()
            .find(|row| row.memory_id == original.memory_id)
            .expect("browser restored stable vector identity");
        assert_eq!(restored.vector, original.vector);
    }
    browser_phase(root, "browser-recycled-again").await;
    expected = recycled(
        &expected,
        test.runtime
            .storage_snapshot()
            .expect("browser recycled again"),
        retention_ms,
    );
    persisted(test, &expected, inference).await;
    acknowledge(root, "browser-recycled-again");
    browser_phase(root, "browser-purged").await;
    expected.storage_version += 1;
    // Permanent deletion also removes old successful receipts that refer to these exact deleted entities.
    // 永久删除同时移除引用这些准确已删除实体的旧成功回执。
    let purged_ids = before
        .dataset
        .memories
        .iter()
        .map(|memory| memory.id.to_string())
        .chain(
            before
                .dataset
                .turns
                .iter()
                .filter(|turn| turn.session_source_id.as_str() == SESSION)
                .map(|turn| turn.id.to_string()),
        )
        .collect::<BTreeSet<_>>();
    expected.outcomes.retain(|entry| {
        let vulcan_memorymesh_core::MutationOutcome::Committed { receipt, .. } = &entry.outcome
        else {
            panic!("browser seed operations must all be committed");
        };
        !receipt
            .entity_bindings
            .iter()
            .any(|binding| purged_ids.contains(&binding.domain_id))
            && !receipt
                .committed_versions
                .iter()
                .any(|version| purged_ids.contains(&version.domain_id))
    });
    // The seven seed-turn receipts are removed, while the two profile instruction receipts remain because profile nodes are retained.
    // 七条种子回合回执会被移除，但两条画像指令回执因画像节点保留而继续存在。
    assert_eq!(expected.outcomes.len(), 5);
    expected.retention_trash_batches.clear();
    expected.memory_sessions[index].status = StorageSessionStatus::Forgotten;
    persisted(test, &expected, inference).await;
    for job_index in 1..=7 {
        assert!(
            test.data_plane
                .load_pending_analysis_job(&format!("browser-source-{job_index}"))
                .await
                .expect("browser purged source job")
                .is_none()
        );
    }
    // This strict hydration API errors on missing identities; query each ID so all three absences are proven.
    // 此严格回填接口对缺失身份报错；逐个查询身份，以证明三个向量均已消失。
    for memory_id in &memory_ids {
        assert_eq!(
            test.data_plane
                .load_vectors(&binding, std::slice::from_ref(memory_id))
                .await
                .expect_err("purged physical vector must be absent"),
            "VMM LanceDB 缺少 Core 快照引用的记忆向量"
        );
    }
    // The three unselected queue rows remain exact, while restart must create no replacement jobs.
    // 三个未选择队列行保持准确，而重新启用不得创建替代作业。
    let remaining = test
        .data_plane
        .list_active_pending_analysis_jobs()
        .await
        .expect("browser remaining queue");
    assert_eq!(
        remaining.len(),
        3,
        "browser remaining queue ids: {:?}",
        remaining
            .iter()
            .map(|job| (
                job.operation_id.as_str(),
                job.turn_id.as_deref(),
                job.state.as_str()
            ))
            .collect::<Vec<_>>()
    );
    assert!(remaining.iter().all(|job| jobs.contains(job)));
    acknowledge(root, "browser-purged");
    browser_phase(root, "browser-restarted").await;
    expected.storage_version += 1;
    expected.memory_sessions[index].status = StorageSessionStatus::Active;
    persisted(test, &expected, inference).await;
    assert_eq!(
        test.data_plane
            .list_active_pending_analysis_jobs()
            .await
            .expect("browser no replay queue"),
        remaining
    );
    acknowledge(root, "browser-restarted");
}
