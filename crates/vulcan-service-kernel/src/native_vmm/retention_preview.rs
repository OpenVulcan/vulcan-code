//! Read-only simulation and exact confirmation of native manual cleanup.
//! 原生人工清洗的只读模拟与准确确认。

use super::*;
use vulcan_service_api::{VmmNativeRetentionPreview, VmmNativeRetentionProjectPreview};

/// Bounds confirmation lifetime without retaining an unbounded collection of snapshots.
/// 限制确认有效期，避免保留无界快照集合。
const PREVIEW_LIFETIME: Duration = Duration::from_secs(300);

/// Holds an immutable step evaluated by the ordinary Core planner.
/// 保存由常规 Core 规划器评估的不可变步骤。
struct PreparedRetentionStep {
    /// Exact planned identities and expected versions.
    /// 准确的计划标识与预期版本。
    plan: RetentionPlanSet,
    /// Policy for this stage, including bounded follow-up limits.
    /// 当前阶段的策略，包括有界后续阶段上限。
    policy: RetentionPolicy,
    /// Stable IDs for batches and jobs created by this step.
    /// 当前步骤创建的批次和作业的稳定标识。
    batch_prefix: String,
}

/// Stores only plans and counters after dropping the temporary simulated snapshot.
/// 丢弃临时模拟快照后只保存计划与计数。
pub(super) struct PreparedRetentionCleanup {
    /// Client-visible impact and opaque confirmation identity.
    /// 客户端可见的影响与不透明确认标识。
    summary: VmmNativeRetentionPreview,
    /// Monotonic creation instant, independent of wall-clock adjustment.
    /// 独立于系统时钟调整的单调创建时刻。
    created_at: std::time::Instant,
    /// Nanosecond evaluation time shared by every step.
    /// 全部步骤共用的纳秒评估时间。
    evaluated_at_ns: i64,
    /// Exact owner-policy map that must still match at confirmation.
    /// 确认时仍必须匹配的准确归属方策略映射。
    policies: BTreeMap<u64, RetentionPolicy>,
    /// Ordered stages, including newly enqueued cold jobs and subsequent idle cleanup.
    /// 有序阶段，包括新入队冷作业以及随后的闲置清洗。
    steps: Vec<PreparedRetentionStep>,
    /// Exact counters expected when applying these same stages.
    /// 应用相同阶段时预期的准确计数。
    expected: NativeVmmRetentionApplyResult,
}

impl NativeVmmMaintenance {
    /// Simulates one full manual pass on a copied snapshot and returns its non-mutating impact.
    /// 在复制快照上模拟一次完整人工轮次并返回其无写入影响。
    /// Rejects disabled native mode or any planning error; no partial preview is admitted.
    /// 拒绝未启用的原生模式或任何规划错误，不接受部分预览。
    pub(crate) async fn preview_retention_cleanup(
        &self,
    ) -> Result<VmmNativeRetentionPreview, String> {
        // Use the same lock order as project mutations; never recover or persist during a preview.
        // 使用与项目变更相同的锁顺序，预览期间绝不恢复或持久化。
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.read().await;
        if !self.native_backend_enabled.load(Ordering::Acquire) {
            return Err(String::from("请先启用内部原生记忆，再预览清洗。"));
        }
        *self.retention_preview.lock().await = None;
        let policy = retention_policy::resolve(&self.runtime.retention_settings().await?)?;
        let snapshot = self.runtime.storage_snapshot()?;
        let projects = self.data_plane.list_projects().await?;
        let policies = self
            .runtime
            .project_retention_policies(
                &policy,
                &projects
                    .iter()
                    .map(|project| project.id)
                    .collect::<Vec<_>>(),
            )
            .await?;
        let prepared = prepare_cleanup(
            &self.runtime,
            snapshot,
            &projects,
            policies,
            native_vmm_now_ns()?,
        )
        .await?;
        let summary = prepared.summary.clone();
        *self.retention_preview.lock().await = Some(prepared);
        Ok(summary)
    }

    /// Applies only the latest server-prepared preview, rejecting stale storage, policy or catalog.
    /// 仅应用服务端最近准备的预览，拒绝过期存储、策略或目录。
    /// Takes a request operation ID and opaque preview ID; returns committed counters or an error.
    /// 接收请求操作标识与不透明预览标识，返回已提交计数或错误。
    pub(crate) async fn execute_retention_cleanup(
        &self,
        operation_id: &str,
        preview_id: &str,
    ) -> Result<NativeVmmRetentionApplyResult, String> {
        // Serialize with scheduled full scans, then take the common catalog/storage barriers.
        // 与周期完整扫描串行，然后获取共同目录与存储屏障。
        let _run_guard = self.retention_run_gate.lock().await;
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.write().await;
        if !self.native_backend_enabled.load(Ordering::Acquire) {
            return Err(String::from("原生记忆已关闭，请重新启用并预览。"));
        }
        // Bind the durable result to the exact preview, so retries cannot reuse a different plan.
        // 将耐久结果绑定到准确预览，防止重试复用不同计划。
        let mutation_kind = format!("native-vmm-confirmed-retention-v1:{preview_id}");
        if let Some(result) = self
            .runtime
            .replay_storage_snapshot_transform(operation_id, &mutation_kind)
            .await?
        {
            return Ok(result);
        }
        let prepared = {
            // Consume the matching plan before validation; a failed confirmation requires a fresh preview.
            // 校验前消费匹配计划，确认失败后必须重新预览。
            let mut slot = self.retention_preview.lock().await;
            match slot.as_ref() {
                Some(prepared) if prepared.summary.preview_id == preview_id => {}
                _ => return Err(String::from("清洗预览不存在或已被替换，请重新预览。")),
            }
            slot.take()
                .ok_or_else(|| String::from("清洗预览已被消费，请重新预览。"))?
        };
        let policy = retention_policy::resolve(&self.runtime.retention_settings().await?)?;
        let projects = self.data_plane.list_projects().await?;
        let policies = self
            .runtime
            .project_retention_policies(
                &policy,
                &projects
                    .iter()
                    .map(|project| project.id)
                    .collect::<Vec<_>>(),
            )
            .await?;
        prepared.validate_confirmation(
            self.runtime.storage_snapshot()?.storage_version,
            &projects,
            &policies,
        )?;
        let started_at_unix_ms = unix_ns_to_ms(native_vmm_now_ns()?);
        self.retention_running.store(true, Ordering::Release);
        {
            let mut status = self.retention_last_status.write().await;
            status.last_started_at_unix_ms = Some(started_at_unix_ms);
            status.last_error = None;
        }
        let storage_version = prepared.summary.storage_version;
        let outcome = async {
            let result = self
                .runtime
                .transform_storage_snapshot(
                    operation_id,
                    &mutation_kind,
                    storage_version,
                    move |snapshot| prepared.apply(snapshot),
                )
                .await?;
            // Include audit pruning in the reported outcome instead of displaying a false success.
            // 将审计清理纳入所报告的结果，避免显示错误的成功状态。
            self.prune_orphaned_terminal_analysis_jobs().await?;
            Ok::<_, String>(result)
        }
        .await;
        {
            let mut status = self.retention_last_status.write().await;
            status.last_finished_at_unix_ms = native_vmm_now_ns().map(unix_ns_to_ms).ok();
            match &outcome {
                Ok(result) => status.last_result = Some(result.clone()),
                Err(error) => status.last_error = Some(error.clone()),
            }
        }
        self.retention_running.store(false, Ordering::Release);
        outcome
    }
}

impl PreparedRetentionCleanup {
    /// Verifies the current version, project identities and policy against the admitted preview.
    /// 对照已准入预览验证当前版本、项目标识和策略。
    /// Returns no value on an exact match; display-name changes do not invalidate stable IDs.
    /// 准确匹配时无返回值，显示名称变化不会使稳定标识失效。
    fn validate_confirmation(
        &self,
        version: u64,
        projects: &[NativeVmmProject],
        policies: &BTreeMap<u64, RetentionPolicy>,
    ) -> Result<(), String> {
        let current_ids: BTreeSet<_> = projects.iter().map(|project| project.id).collect();
        let preview_ids: BTreeSet<_> = self
            .summary
            .projects
            .iter()
            .map(|project| project.project_id)
            .collect();
        if self.created_at.elapsed() > PREVIEW_LIFETIME
            || version != self.summary.storage_version
            || current_ids != preview_ids
            || policies != &self.policies
        {
            return Err(String::from(
                "清洗预览已过期，或记忆数据、项目、规则已改变，请重新预览后确认。",
            ));
        }
        Ok(())
    }

    /// Replays immutable steps on the version-checked transaction copy, never replanning new targets.
    /// 在已校验版本的事务副本上重放不可变步骤，绝不重新规划新增目标。
    /// Returns exact expected counters or rejects the entire copied transition.
    /// 返回准确预期计数，否则拒绝整个副本转换。
    fn apply(
        self,
        snapshot: &mut StorageSnapshot,
    ) -> Result<NativeVmmRetentionApplyResult, String> {
        // Freeze selection at preview time, but start new retention windows at actual confirmation.
        // 选择范围冻结于预览时，但新的保留期限从实际确认执行开始。
        let applied_at_ns = native_vmm_now_ns()?;
        if applied_at_ns < self.evaluated_at_ns {
            return Err(String::from(
                "系统时间早于清洗预览时间，请校准时间并重新预览。",
            ));
        }
        // Only records created by these admitted steps may receive the new commit timestamp.
        // 仅允许这些准入步骤创建的记录使用新的提交时间戳。
        let existing_batches: BTreeSet<_> = snapshot
            .retention_trash_batches
            .iter()
            .map(|batch| batch.batch_id.clone())
            .collect();
        let existing_jobs: BTreeSet<_> = snapshot
            .retention_cold_turn_jobs
            .iter()
            .map(|job| job.job_id.clone())
            .collect();
        let mut actual = NativeVmmRetentionApplyResult::default();
        for step in self.steps {
            actual.merge(&apply_retention_plan(
                snapshot,
                &step.plan,
                &step.policy,
                self.evaluated_at_ns,
                &step.batch_prefix,
            )?);
        }
        if actual != self.expected {
            return Err(String::from(
                "实际清洗影响与预览不一致，已拒绝提交，请重新预览。",
            ));
        }
        for batch in &mut snapshot.retention_trash_batches {
            if !existing_batches.contains(&batch.batch_id) {
                // Each simulated batch already carries its owner's selected duration, not a shared default.
                // 每个模拟批次已携带其归属方选定的期限，而非共同默认值。
                let duration_ns = batch
                    .expires_at_ns
                    .checked_sub(batch.recycled_at_ns)
                    .filter(|duration| *duration > 0)
                    .ok_or_else(|| String::from("预览批次的冻结保留期限无效。"))?;
                batch.recycled_at_ns = applied_at_ns;
                batch.expires_at_ns = applied_at_ns
                    .checked_add(duration_ns)
                    .ok_or_else(|| String::from("新回收批次截止时间超出可表示范围。"))?;
            }
        }
        for job in &mut snapshot.retention_cold_turn_jobs {
            if !existing_jobs.contains(&job.job_id) {
                job.next_run_at_ns = applied_at_ns;
            }
        }
        Ok(actual)
    }
}

/// Prepares all ordered project stages on one disposable snapshot using the production planner.
/// 使用生产规划器在一个可丢弃快照上准备全部有序项目阶段。
/// Takes immutable projects and policy; returns only plans, counters and exact impacted IDs.
/// 接收不可变项目和策略，仅返回计划、计数以及准确受影响标识。
async fn prepare_cleanup(
    runtime: &NativeVmmCoreRuntime,
    mut snapshot: StorageSnapshot,
    projects: &[NativeVmmProject],
    policies: BTreeMap<u64, RetentionPolicy>,
    now_ns: i64,
) -> Result<PreparedRetentionCleanup, String> {
    // One unique preview owns all simulated job/batch identities.
    // 一个唯一预览拥有全部模拟作业和批次标识。
    let preview_id = format!("native-vmm-cleanup:{}", uuid::Uuid::new_v4());
    let mut prepared = PreparedRetentionCleanup {
        summary: VmmNativeRetentionPreview {
            preview_id,
            storage_version: snapshot.storage_version,
            evaluated_at_unix_ms: unix_ns_to_ms(now_ns),
            expires_at_unix_ms: unix_ns_to_ms(now_ns) + 300_000,
            projects: Vec::new(),
        },
        created_at: std::time::Instant::now(),
        evaluated_at_ns: now_ns,
        policies: policies.clone(),
        steps: Vec::new(),
        expected: NativeVmmRetentionApplyResult::default(),
    };
    for project in projects {
        let policy = policies
            .get(&project.id)
            .ok_or_else(|| format!("缺少已准入的记忆项目 {} 保留策略。", project.id))?;
        // Preserve exact identities before this project's stages, including the shared personal profile.
        // 在当前项目各阶段前保留准确标识，包括共享个人画像。
        let before = snapshot.clone();
        let first =
            simulate_step(runtime, &mut snapshot, &mut prepared, project.id, policy).await?;
        let completed =
            u32::try_from(first.completed_cold_jobs).map_err(|error| error.to_string())?;
        let remaining = policy.cold_turn_job_limit.saturating_sub(completed);
        if first.enqueued_cold_jobs > 0 && remaining > 0 {
            simulate_step(
                runtime,
                &mut snapshot,
                &mut prepared,
                project.id,
                &native_cold_turn_follow_up_policy(policy, remaining),
            )
            .await?;
        }
        if first.enqueued_cold_jobs > 0 {
            simulate_step(
                runtime,
                &mut snapshot,
                &mut prepared,
                project.id,
                &native_idle_session_follow_up_policy(policy),
            )
            .await?;
        }
        prepared
            .summary
            .projects
            .push(project_impact(project, &before, &snapshot));
    }
    Ok(prepared)
}

/// Simulates and records one bounded Core plan; returns the same counters as real application.
/// 模拟并记录一个有界 Core 计划，返回与实际应用相同的计数。
async fn simulate_step(
    runtime: &NativeVmmCoreRuntime,
    snapshot: &mut StorageSnapshot,
    prepared: &mut PreparedRetentionCleanup,
    project_id: u64,
    policy: &RetentionPolicy,
) -> Result<NativeVmmRetentionApplyResult, String> {
    let batch_prefix = format!("{}:{}", prepared.summary.preview_id, prepared.steps.len());
    let command = retention_command(
        project_id,
        prepared.evaluated_at_ns,
        policy,
        batch_prefix.clone(),
    )?;
    let plan = runtime
        .preview_retention_plan(snapshot.clone(), command)
        .await?;
    if retention_plan_is_empty(&plan) {
        return Ok(NativeVmmRetentionApplyResult::default());
    }
    let result = apply_retention_plan(
        snapshot,
        &plan,
        policy,
        prepared.evaluated_at_ns,
        &batch_prefix,
    )?;
    prepared.steps.push(PreparedRetentionStep {
        plan,
        policy: policy.clone(),
        batch_prefix,
    });
    prepared.expected.merge(&result);
    Ok(result)
}

/// Derives visible impact from actual simulated transitions rather than estimating plan lengths.
/// 从实际模拟转换推导可见影响，而非估计计划长度。
/// Returns stable record identities grouped under the project whose stage changed them.
/// 返回按实际改变记录的项目阶段分组的稳定标识。
fn project_impact(
    project: &NativeVmmProject,
    before: &StorageSnapshot,
    after: &StorageSnapshot,
) -> VmmNativeRetentionProjectPreview {
    let remaining_memories: BTreeSet<_> = after
        .dataset
        .memories
        .iter()
        .map(|memory| &memory.id)
        .collect();
    let remaining_turns: BTreeSet<_> = after.dataset.turns.iter().map(|turn| &turn.id).collect();
    let remaining_batches: BTreeSet<_> = after
        .retention_trash_batches
        .iter()
        .map(|batch| &batch.batch_id)
        .collect();
    let expired_profiles: BTreeSet<_> = after
        .dataset
        .profile_nodes
        .iter()
        .filter(|node| node.state == ProfileNodeState::Expired)
        .map(|node| &node.id)
        .collect();
    VmmNativeRetentionProjectPreview {
        project_id: project.id,
        project_name: project.name.clone(),
        recycled_memory_ids: before
            .dataset
            .memories
            .iter()
            .filter(|memory| !remaining_memories.contains(&memory.id))
            .map(|memory| memory.id.to_string())
            .collect(),
        recycled_turn_ids: before
            .dataset
            .turns
            .iter()
            .filter(|turn| !remaining_turns.contains(&turn.id))
            .map(|turn| turn.id.to_string())
            .collect(),
        expired_profile_node_ids: before
            .dataset
            .profile_nodes
            .iter()
            .filter(|node| {
                node.state == ProfileNodeState::Active && expired_profiles.contains(&node.id)
            })
            .map(|node| node.id.to_string())
            .collect(),
        purged_batch_ids: before
            .retention_trash_batches
            .iter()
            .filter(|batch| !remaining_batches.contains(&batch.batch_id))
            .map(|batch| batch.batch_id.clone())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a plan-free preview for testing confirmation identity and time guards.
    /// 创建无步骤预览，用于测试确认身份与时间保护。
    fn empty_prepared() -> PreparedRetentionCleanup {
        PreparedRetentionCleanup {
            summary: VmmNativeRetentionPreview {
                preview_id: String::from("preview"),
                storage_version: 7,
                evaluated_at_unix_ms: 0,
                expires_at_unix_ms: 300_000,
                projects: vec![VmmNativeRetentionProjectPreview {
                    project_id: 1,
                    project_name: String::from("original"),
                    recycled_memory_ids: Vec::new(),
                    recycled_turn_ids: Vec::new(),
                    expired_profile_node_ids: Vec::new(),
                    purged_batch_ids: Vec::new(),
                }],
            },
            created_at: std::time::Instant::now(),
            evaluated_at_ns: 0,
            policies: BTreeMap::from([(1, RetentionPolicy::v1())]),
            steps: Vec::new(),
            expected: NativeVmmRetentionApplyResult::default(),
        }
    }

    /// Accepts display-name changes but rejects stale versions, rules, project sets and elapsed tokens.
    /// 接受显示名称变化，但拒绝过期版本、规则、项目集合与超时令牌。
    #[test]
    fn confirmation_guards_exact_identity_and_monotonic_lifetime() {
        let mut prepared = empty_prepared();
        let mut projects = vec![NativeVmmProject {
            id: 1,
            scope_id: String::from("vmm-project:1"),
            name: String::from("renamed"),
            tag: String::from("default"),
            protected: true,
        }];
        let policy = BTreeMap::from([(1, RetentionPolicy::v1())]);
        assert!(
            prepared
                .validate_confirmation(7, &projects, &policy)
                .is_ok()
        );
        assert!(
            prepared
                .validate_confirmation(8, &projects, &policy)
                .is_err()
        );
        let mut changed = policy.clone();
        changed
            .get_mut(&1)
            .expect("project rule")
            .turn_hot_window_size += 1;
        assert!(
            prepared
                .validate_confirmation(7, &projects, &changed)
                .is_err()
        );
        projects[0].id = 2;
        assert!(
            prepared
                .validate_confirmation(7, &projects, &policy)
                .is_err()
        );
        projects[0].id = 1;
        prepared.created_at = std::time::Instant::now() - PREVIEW_LIFETIME - Duration::from_secs(1);
        assert!(
            prepared
                .validate_confirmation(7, &projects, &policy)
                .is_err()
        );
    }
}
