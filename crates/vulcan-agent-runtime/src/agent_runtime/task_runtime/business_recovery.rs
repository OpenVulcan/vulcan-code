//! Starts an original child business execution from captured settings and a prepared stream.
//! 从已捕获设置与已准备流启动原子业务执行。

use super::*;
use crate::agent_runtime::execution_models::{ExecutionPromptPurpose, ExecutionTurnStartMode};
use crate::agent_runtime::runtime_stream_launch_payload::{
    RuntimeStreamLaunch, RuntimeStreamTurnStartMode,
};
use vulcan_session_core::v1::{collaboration::CollaborationExecutionId, document::SessionDocument};

/// Owns only the resident projection created by one recovery admission until execution handoff.
/// 在执行交接前，仅持有一次恢复准入新建常驻投影的所有权。
struct RecoveredChildAdmissionGuard {
    /// Runtime whose exclusive admission installed the record.
    /// 通过排他准入安装记录的运行时。
    runtime: Arc<TaskRuntime>,
    /// Exact immutable execution that may be rolled back before handoff.
    /// 交接前允许回滚的精确不可变执行。
    request: Arc<TaskExecutionRequest>,
    /// Successful handoff disables rollback; an unwind retains cleanup ownership.
    /// 成功交接禁用回滚；栈展开仍保留清理责任。
    owns_record: bool,
}

impl RecoveredChildAdmissionGuard {
    /// Finishes `result`, preserving both the admission error and any rollback error.
    /// 结束 `result`，同时保留准入错误与回滚错误。
    /// Success transfers the projection to the finalizer; failure returns after exact rollback.
    /// 成功将投影移交终态处理器；失败在精确回滚后返回。
    fn finish(
        mut self,
        result: Result<TaskExecutionOutcome, String>,
    ) -> Result<TaskExecutionOutcome, String> {
        if let Err(error) = result {
            // Stream/lease guards have already settled before this outer guard removes projections.
            // 此外层守卫移除投影前，事件流与租约守卫已经结算。
            let rollback = self
                .runtime
                .discard_recovered_admission_record(&self.request);
            self.owns_record = false;
            return match rollback {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!(
                    "{error} Recovered admission rollback failed: {cleanup}"
                )),
            };
        }
        self.owns_record = false;
        result
    }
}

impl Drop for RecoveredChildAdmissionGuard {
    /// Rolls back an unwound pre-handoff admission after its inner lease guard has dropped.
    /// 内层租约守卫析构后，回滚因栈展开退出的交接前准入。
    /// No return channel exists during Drop, so an ownership/registry failure is reported explicitly.
    /// Drop 没有返回通道，因此明确报告归属或注册表失败。
    fn drop(&mut self) {
        if self.owns_record
            && let Err(error) = self
                .runtime
                .discard_recovered_admission_record(&self.request)
        {
            eprintln!("[task-runtime] recovered admission rollback failed: {error}");
        }
    }
}

impl TaskRuntime {
    /// Admits one recovered stream without another delegation, root input or configuration resolution.
    /// 准入一个恢复流，不再次委派、创建根输入或解析配置。
    /// `document` is the current durable child; `admit_listener` must revalidate its canonical parent and attach the sink.
    /// `document` 为当前可靠子文档；`admit_listener` 必须重查规范父归属并附加接收器。
    /// The host must retain lifecycle admission throughout this synchronous call; returns a running acknowledgement.
    /// 宿主必须在整个同步调用期间保留生命周期准入；返回运行接纳回执。
    pub fn start_recovered_child_business<AdmitListener>(
        self: &Arc<Self>,
        mut launch: RuntimeStreamLaunch,
        document: &SessionDocument,
        execution_id: &CollaborationExecutionId,
        admit_listener: AdmitListener,
    ) -> Result<TaskExecutionOutcome, String>
    where
        AdmitListener: FnOnce(&TaskExecutionRequest) -> Result<(), String>,
    {
        // Read the original profile only from a fully published and currently executable document.
        // 仅从完整发布且当前可执行的文档读取原配置。
        let profile = document
            .child_business_recovery_profile(execution_id)
            .map_err(|error| error.to_string())?
            .clone();
        let mut context = launch.task_execution_context.clone().ok_or_else(|| {
            String::from("Recovered child stream has no prepared execution context.")
        })?;
        if !Arc::ptr_eq(self, &launch.task_runtime)
            || !Arc::ptr_eq(self, &context.foundation.task_runtime())
            || launch.session_id.to_string() != document.metadata.session_id
            || launch.preparation.prompt_purpose != ExecutionPromptPurpose::AgentTask
            || launch.turn_start_mode != RuntimeStreamTurnStartMode::RestoredCheckpoint
            || !launch.effective_prompt.is_empty()
            || context.request_template.turn_start_mode
                != Some(ExecutionTurnStartMode::RestoredCheckpoint)
            || context
                .request_template
                .captured_child_tool_admission
                .as_ref()
                != Some(&profile.admitted_tools)
            || launch.preparation.model_blueprint.connection_id != profile.connection_id
            || launch.preparation.temperature != profile.temperature
            || Some(&launch.preparation.model_blueprint.provider_id)
                != profile.effective_config.provider_model.provider_id.as_ref()
            || Some(&launch.preparation.model_blueprint.model_id)
                != profile.effective_config.provider_model.model_id.as_ref()
            || Some(&launch.preparation.agent_id) != profile.execution_agent_id.as_ref()
        {
            return Err(String::from(
                "Recovered child stream differs from its captured business request.",
            ));
        }
        // This rechecks purpose, original root and absence of a borrowed transport cursor before mutation.
        // 此操作在修改前重查用途、原根以及没有借用传输游标。
        let mut validated_request = context.request_template.clone();
        restore_child_business_request(&mut validated_request, document, execution_id)?;
        context.recovered_child_profile = Some(profile.clone());
        launch.task_execution_context = Some(context.clone());
        let root_session_id =
            Uuid::parse_str(&profile.root_session_id).map_err(|error| error.to_string())?;
        let parent_session_id =
            Uuid::parse_str(&profile.parent_session_id).map_err(|error| error.to_string())?;
        let task_id = launch.session_id;
        let controller = launch.stream_state.controller().config().clone();
        let request = Arc::new(TaskExecutionRequest {
            current_session_id: parent_session_id,
            root_session_id,
            task_id,
            execution_id: execution_id.as_str().to_owned(),
            public_task_id: profile.runtime_identity.public_task_id.clone(),
            prompt: profile.invocation_config.prompt.clone(),
            description: profile.runtime_identity.description.clone(),
            subagent: profile.runtime_identity.subagent.clone(),
            invocation_config: profile.invocation_config.clone(),
            display_title: profile.runtime_identity.display_title.clone(),
            parent_task_id: (parent_session_id != root_session_id).then_some(parent_session_id),
            depth: usize::try_from(profile.runtime_identity.depth)
                .map_err(|error| error.to_string())?,
            snapshot: launch.core_snapshot.clone(),
            controller_config: Some(controller.clone()),
            // This is a restored run of an existing task; no new-root admission consumes this flag.
            // 这是已有任务的恢复运行；不会有新根准入消费此标记。
            is_resumed: true,
        });
        // Exclusive claim refuses duplicate recovery instead of aborting an already running owner.
        // 排他认领拒绝重复恢复，不中止已经运行的所有者。
        let active_run = self.claim_recovered_child_record(&request, &profile, document)?;
        // Only a newly installed record receives rollback ownership; pre-existing records never do.
        // 只有新安装记录获得回滚所有权；已有记录绝不会获得此权限。
        let admission = RecoveredChildAdmissionGuard {
            runtime: Arc::clone(self),
            request: Arc::clone(&request),
            owns_record: true,
        };
        // The closure releases its stream and lease on every failed or unwound pre-handoff path.
        // 闭包在每条失败或栈展开的交接前路径释放事件流及租约。
        let result = (move || {
            let mut startup = DetachedTaskStreamStartupGuard::open(
                Arc::clone(self),
                Arc::clone(&request),
                active_run,
            )?;
            startup.observe(admit_listener(&request))?;
            spawn_admitted_task_execution(
                Arc::clone(self),
                context,
                request,
                profile.resolved_config,
                controller,
                startup,
                current_unix_millis_u64(),
                Some(launch),
            )
        })();
        admission.finish(result)
    }

    /// Claims an active lease and restores its missing resident record in one task transaction.
    /// 在同一任务事务中认领活动租约并恢复其缺失常驻记录。
    /// Exact source timestamps and fixed prompts come from the durable document and profile.
    /// 精确来源时间及固定提示来自可靠文档和配置。
    /// Returns the lease or an error without mutating any pre-existing process-local record.
    /// 返回租约或错误，不修改任何已有进程内记录。
    fn claim_recovered_child_record(
        self: &Arc<Self>,
        request: &TaskExecutionRequest,
        profile: &vulcan_session_core::v1::child_execution::ChildExecutionProfile,
        document: &SessionDocument,
    ) -> Result<ActiveTaskRunGuard, String> {
        let transaction_lock = self.task_execution_state_lock(request.task_id)?;
        let _transaction = transaction_lock
            .lock()
            .map_err(|_| String::from("Recovered child transaction is unavailable."))?;
        if self
            .sessions
            .read()
            .map_err(|_| String::from("Recovered child session registry is unavailable."))?
            .contains_key(&request.task_id)
        {
            return Err(String::from(
                "Recovered child already has a resident task record.",
            ));
        }
        // The caller-held transaction keeps ordinary preparation from publishing a competing record.
        // 调用方持有的事务阻止普通准备发布竞争记录。
        let active_run = self.claim_active_task_run_with_transaction_lock_held(request)?;
        // Acquire the record writer only after the claim helper has released its own registry reads.
        // 认领帮助函数释放自己的注册表读取后才取得记录写锁。
        let mut sessions = self
            .sessions
            .write()
            .map_err(|_| String::from("Recovered child session registry is unavailable."))?;
        let mut record = build_task_session_record(TaskSessionRecordInit {
            task_id: request.task_id,
            public_task_id: request.public_task_id.clone(),
            origin_session_id: request.root_session_id,
            parent_task_id: request.parent_task_id,
            depth: request.depth,
            description: request.description.clone(),
            subagent: request.subagent.clone(),
            invocation_config: request.invocation_config.clone(),
            display_title: request.display_title.clone(),
            controller_config: request.controller_config.clone(),
            snapshot: request.snapshot.clone(),
            last_prompt: request.prompt.clone(),
            last_status: String::from("created"),
        });
        record.execution_id = Some(request.execution_id.clone());
        record.created_at = document.metadata.created_at_ms.to_string();
        record.updated_at = document.metadata.updated_at_ms.to_string();
        record.agent_prompt_snapshot = profile
            .fixed_agent_prompt
            .as_deref()
            .and_then(ExecutionAgentPromptSnapshot::fixed_parent_runtime);
        sessions.insert(request.task_id, record);
        Ok(active_run)
    }

    /// Removes only a newly installed pre-handoff record and its exact optional registry leaf.
    /// 仅移除新安装的交接前记录及其精确可选注册表叶子。
    /// `request` is held by the admission guard, never reconstructed from a failed task status.
    /// `request` 由准入守卫持有，绝不从失败任务状态重建。
    /// Returns after atomic projection removal; stopped, superseded and terminal owners stay intact.
    /// 原子移除投影后返回；停止、替代与终态所有者保持完整。
    fn discard_recovered_admission_record(
        &self,
        request: &TaskExecutionRequest,
    ) -> Result<(), String> {
        // Serialize with every ordinary claim, stop and record publication for this physical child.
        // 与此物理子任务的普通认领、停止及记录发布串行。
        let transaction_lock = self.task_execution_state_lock(request.task_id)?;
        let _transaction = transaction_lock.lock().map_err(|_| {
            String::from("Recovered admission rollback transaction is unavailable.")
        })?;
        if self.task_has_active_or_starting_run(request.task_id)? {
            return Err(String::from(
                "Recovered admission rollback cannot remove an active run.",
            ));
        }
        // Match close-agent's global registry-then-session order, including unrelated child transactions.
        // 遵循关闭代理的全局注册表先、会话后顺序，包括不相关子任务事务。
        let mut registry = self.subagent_registry.write().map_err(|_| {
            String::from("Recovered admission rollback agent registry is unavailable.")
        })?;
        // Keep both projection writers until the matching leaf is validated and removed.
        // 保持两个投影写锁，直到匹配叶子被校验并移除。
        let mut sessions = self.sessions.write().map_err(|_| {
            String::from("Recovered admission rollback session registry is unavailable.")
        })?;
        // Absence is an already-settled rollback, never an instruction to synthesize a record.
        // 缺失表示回滚已结算，绝不指示合成记录。
        let Some(record) = sessions.get(&request.task_id) else {
            return Ok(());
        };
        if record.execution_id.as_deref() != Some(request.execution_id.as_str())
            || record.public_task_id != request.public_task_id
            || record.origin_session_id != request.root_session_id
            || record.parent_task_id != request.parent_task_id
        {
            return Ok(());
        }
        // Failed means this guard's rejected admission, not an arbitrary failed business execution.
        // Failed 表示本守卫被拒的准入，不代表任意失败业务执行。
        let status = task_session_status_from_record_status(&record.last_status)?;
        if !matches!(
            status,
            TaskSessionStatus::Created | TaskSessionStatus::Running | TaskSessionStatus::Failed
        ) {
            return Ok(());
        }
        // The direct parent owns the public registry slot even for a nested child.
        // 即使是嵌套子代理，公开注册表槽位也由直接父级持有。
        let parent = SessionNodeId::from(request.current_session_id);
        if registry
            .get_for_parent(parent, &request.public_task_id)
            .is_some()
        {
            registry
                .remove_exact_leaf(
                    parent,
                    &request.public_task_id,
                    SessionNodeId::from(request.task_id),
                    subagent_status_from_task_session_status(status),
                )
                .map_err(|error| error.to_string())?;
        }
        sessions.remove(&request.task_id);
        Ok(())
    }
}
