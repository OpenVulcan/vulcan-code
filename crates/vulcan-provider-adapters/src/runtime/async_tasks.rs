//! Durable provider-task dispatch, recovery, observation, and cancellation.
//! 可持久化供应商任务的分派、恢复、观测与取消。

use super::tool_diagnostics::{
    diagnose_provider_tool_failure, diagnose_provider_tool_failure_with_secret,
    redact_provider_tool_secret,
};
use super::tool_targets::{ResolvedProviderToolTarget, provider_tool_target_allows_credential};
use super::{PublicAiRuntime, current_unix_millis, remaining_provider_operation_timeout};
use crate::provider_task::{
    ProviderAsyncTaskPhase, ProviderAsyncTaskPoll, ProviderAsyncTaskRecord,
    ProviderAsyncTaskRecoveryContext, ProviderAsyncTaskRoute, bound_provider_task_diagnostic,
    provider_task_observation_delay_ms,
};
use crate::{
    CredentialExecutionSecret, PersistedAiConfiguration, ProviderToolExecutionResult,
    ProviderToolTargetSelection,
};
use futures_util::{StreamExt, stream};
use std::future::Future;
use std::path::Path;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Maximum wall time for one provider task poll or cancellation observation.
/// 单次供应商任务轮询或取消观测的最大墙钟时间。
const PROVIDER_ASYNC_TASK_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(60);
/// Maximum independent provider tasks observed concurrently by one recovery pass.
/// 单次恢复允许并发观测的独立供应商任务上限。
const PROVIDER_TASK_RECOVERY_MAX_CONCURRENCY: usize = 8;

/// Observes one durable task batch with a fixed cross-provider concurrency ceiling.
/// 以固定的跨供应商并发上限观测一批持久任务。
///
/// # Parameters
/// 参数
/// `records` is one immutable recovery-pass snapshot and `observe` performs exactly one task's
/// cancellation or poll action.
/// `records` 是一次不可变恢复轮次快照，`observe` 执行精确单个任务的取消或轮询动作。
///
/// # Returns
/// 返回值
/// Every task identity paired with its independent observation result in completion order.
/// 返回完成顺序下每个任务身份及其独立观测结果。
async fn observe_provider_task_records_bounded<Observe, Observation>(
    records: Vec<ProviderAsyncTaskRecord>,
    observe: Observe,
) -> Vec<(String, Result<(), String>)>
where
    Observe: Fn(ProviderAsyncTaskRecord) -> Observation,
    Observation: Future<Output = Result<(), String>>,
{
    stream::iter(records)
        .map(|record| {
            // execution_id survives moving the complete record into its independent future.
            // execution_id 在完整记录移入独立 Future 后仍保留任务身份。
            let execution_id = record.execution_id.clone();
            // observation starts only when buffer_unordered admits this task below the ceiling.
            // observation 仅在 buffer_unordered 于上限内接纳当前任务时启动。
            let observation = observe(record);
            async move { (execution_id, observation.await) }
        })
        .buffer_unordered(PROVIDER_TASK_RECOVERY_MAX_CONCURRENCY)
        .collect::<Vec<_>>()
        .await
}

/// Awaits one provider-task network operation without allowing it to block the recovery scheduler forever.
/// 等待一次供应商任务网络操作，同时避免其永久阻塞恢复调度器。
///
/// # Parameters
/// 参数
/// `timeout` is the exact host limit, `operation` is a controlled diagnostic label, and
/// `future` is one non-replayed provider operation.
/// `timeout` 是精确宿主限制，`operation` 是受控诊断标签，`future` 是一次不重放的供应商操作。
///
/// # Returns
/// 返回值
/// The exact operation result, or a controlled timeout diagnostic after dropping the in-flight future.
/// 返回精确操作结果，或在丢弃进行中 Future 后返回受控超时诊断。
async fn await_bounded_provider_task_operation<T, F>(
    timeout: Duration,
    operation: &str,
    future: F,
) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    tokio::time::timeout(timeout, future).await.map_err(|_| {
        format!(
            "{operation} exceeded its {} millisecond host timeout.",
            timeout.as_millis()
        )
    })?
}

/// Builds one deterministic asynchronous-task timeout diagnostic after its exact deadline.
/// 在精确截止时间后构建一条确定性的异步任务超时诊断。
///
/// # Parameters
/// 参数
/// `created_at_unix_ms`, `deadline_unix_ms`, and `now_unix_ms` are one monotonic wall-clock domain.
/// `created_at_unix_ms`、`deadline_unix_ms` 与 `now_unix_ms` 属于同一单调墙钟域。
///
/// # Returns
/// 返回值
/// A controlled diagnostic at or after the deadline, otherwise `None`.
/// 在截止时间或之后返回受控诊断，否则返回 `None`。
fn provider_task_timeout_diagnostic(
    created_at_unix_ms: u64,
    deadline_unix_ms: u64,
    now_unix_ms: u64,
) -> Option<String> {
    (now_unix_ms >= deadline_unix_ms).then(|| {
        format!(
            "Provider asynchronous task exceeded its {} millisecond deadline.",
            deadline_unix_ms.saturating_sub(created_at_unix_ms)
        )
    })
}

/// Returns the next observation sleep without crossing the frozen task deadline.
/// 返回不会越过冻结任务截止时间的下一次观测等待时长。
///
/// # Parameters
/// 参数
/// `requested_delay_ms` is the frozen backoff result; the deadline and current time share one
/// wall-clock domain.
/// `requested_delay_ms` 是冻结退避结果；截止时间与当前时间使用同一墙钟域。
///
/// # Returns
/// 返回值
/// The smaller of the backoff delay and remaining lifecycle budget, in milliseconds.
/// 返回退避时长与剩余生命周期预算中的较小值，单位为毫秒。
fn clamp_provider_task_observation_delay_ms(
    requested_delay_ms: u64,
    deadline_unix_ms: u64,
    now_unix_ms: u64,
) -> u64 {
    requested_delay_ms.min(deadline_unix_ms.saturating_sub(now_unix_ms))
}

impl PublicAiRuntime {
    /// Configures durable provider-task recovery and seals interrupted dispatch intents.
    /// 配置持久化供应商任务恢复并封闭中断的分派意图。
    ///
    /// # Parameters
    /// 参数
    /// `directory` stores only versioned non-secret provider-task snapshots.
    /// `directory` 仅存储带版本的非秘密供应商任务快照。
    ///
    /// # Returns
    /// 返回值
    /// The loaded records after every interrupted dispatch is marked outcome-unknown.
    /// 返回加载后的记录，其中每个中断分派都已标记为结果不确定。
    pub fn configure_provider_task_recovery(
        &self,
        directory: &Path,
    ) -> Result<Vec<ProviderAsyncTaskRecord>, String> {
        let records = self.provider_tasks.configure(directory)?;
        for mut record in records {
            if record.phase == ProviderAsyncTaskPhase::Dispatching {
                record.phase = ProviderAsyncTaskPhase::DispatchOutcomeUnknown;
                record.updated_at_unix_ms = current_unix_millis()?;
                record.error = Some(String::from(
                    "Provider dispatch was interrupted before a task identifier was confirmed; automatic replay is forbidden.",
                ));
                record.consecutive_observation_failures = 0;
                self.provider_tasks.replace(record)?;
            }
        }
        self.provider_tasks.snapshot()
    }

    /// Returns one stable provider-task status snapshot.
    /// 返回一份稳定供应商任务状态快照。
    ///
    /// # Parameters
    /// 参数
    /// `execution_id` is the host-owned stable task identifier.
    /// `execution_id` 是宿主持有的稳定任务标识。
    ///
    /// # Returns
    /// 返回值
    /// The exact retained non-secret record.
    /// 返回精确保留的非秘密记录。
    pub fn provider_task_status(
        &self,
        execution_id: &str,
    ) -> Result<ProviderAsyncTaskRecord, String> {
        self.provider_tasks.require(execution_id)
    }

    /// Performs one recovery observation for every active retained provider task.
    /// 对每个活动保留供应商任务执行一次恢复观测。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies current credential material for exact retained connection IDs.
    /// `configuration` 为精确保留连接标识提供当前凭据材料。
    ///
    /// # Returns
    /// 返回值
    /// Updated records; transient observation failures remain non-terminal diagnostics.
    /// 返回更新记录；瞬时观测失败保留为非终态诊断。
    pub async fn recover_provider_tasks(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<Vec<ProviderAsyncTaskRecord>, String> {
        let records = self.provider_tasks.snapshot()?;
        // observations isolate slow providers behind a fixed concurrency ceiling; one exhausted
        // 60-second request budget cannot serially delay every unrelated task.
        // observations 以固定并发上限隔离慢供应商；单次耗尽 60 秒请求预算不能串行拖延全部
        // 无关任务。
        let mut observations = observe_provider_task_records_bounded(records, |record| {
            self.recover_provider_task_record(configuration, record)
        })
        .await;
        // failures are sorted back to stable task identity so concurrent completion order never
        // changes the surfaced store failure.
        // failures 按稳定任务身份重新排序，因此并发完成顺序不会改变对外呈现的存储失败。
        observations.sort_by(|left, right| left.0.cmp(&right.0));
        if let Some((execution_id, error)) = observations
            .into_iter()
            .find_map(|(execution_id, result)| result.err().map(|error| (execution_id, error)))
        {
            return Err(format!(
                "Provider task recovery failed for execution `{execution_id}`: {error}"
            ));
        }
        self.provider_tasks.snapshot()
    }

    /// Recovers one retained provider task without allowing it to block unrelated tasks.
    /// 恢复一条保留供应商任务，且不允许其阻塞无关任务。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies current credential material and `record` is one immutable
    /// recovery-pass input selected from the durable snapshot.
    /// `configuration` 提供当前凭据材料，`record` 是从持久快照选出的一条不可变恢复轮次输入。
    ///
    /// # Returns
    /// 返回值
    /// Success after a no-op, cancellation observation, poll observation, or durable transient
    /// diagnostic; only state-store failures escape to the aggregate pass.
    /// 在无操作、取消观测、轮询观测或持久化瞬时诊断后返回成功；只有状态存储失败会返回给
    /// 聚合恢复轮次。
    async fn recover_provider_task_record(
        &self,
        configuration: &PersistedAiConfiguration,
        record: ProviderAsyncTaskRecord,
    ) -> Result<(), String> {
        // needs_completed_result_recovery identifies durable completion records whose transient
        // result bytes were intentionally omitted.
        // needs_completed_result_recovery 标识有意省略瞬时结果字节的持久完成记录。
        let needs_completed_result_recovery =
            record.phase == ProviderAsyncTaskPhase::Completed && record.result.is_none();
        if record.phase.is_terminal() && !needs_completed_result_recovery {
            return Ok(());
        }
        if record.phase == ProviderAsyncTaskPhase::CancellationRequested {
            if let Err(error) = self
                .cancel_provider_task_internal(configuration, record.execution_id.as_str())
                .await
            {
                self.record_provider_task_observation_error(record.execution_id.as_str(), error)?;
            }
            return Ok(());
        }
        if let Err(error) = self
            .poll_provider_async_task_once(configuration, record.execution_id.as_str())
            .await
        {
            self.record_provider_task_observation_error(record.execution_id.as_str(), error)?;
        }
        Ok(())
    }

    /// Requests provider cancellation while durably recording intent first.
    /// 请求供应商取消，并首先持久化记录取消意图。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `execution_id`, and optional `owner_session_id` identify one exact task.
    /// `configuration`、`execution_id` 与可选 `owner_session_id` 标识一个精确任务。
    ///
    /// # Returns
    /// 返回值
    /// A provider-confirmed cancelled record; uncertain cancellation remains recoverable.
    /// 返回供应商确认取消的记录；不确定取消保持可恢复。
    pub async fn cancel_provider_task(
        &self,
        configuration: &PersistedAiConfiguration,
        execution_id: &str,
        owner_session_id: Option<&str>,
    ) -> Result<ProviderAsyncTaskRecord, String> {
        let mut record = self.provider_tasks.require(execution_id)?;
        if let Some(expected_owner) = record.owner_session_id.as_deref()
            && Some(expected_owner) != owner_session_id
        {
            return Err(format!(
                "Provider task execution `{execution_id}` is owned by another session."
            ));
        }
        if record.phase.is_terminal() {
            return Err(format!(
                "Provider task execution `{execution_id}` is already terminal."
            ));
        }
        if !record.cancellation_supported {
            return Err(format!(
                "Provider task execution `{execution_id}` does not support provider cancellation."
            ));
        }
        record.phase = ProviderAsyncTaskPhase::CancellationRequested;
        record.updated_at_unix_ms = current_unix_millis()?;
        record.error = None;
        record.consecutive_observation_failures = 0;
        self.provider_tasks.replace(record)?;
        self.cancel_provider_task_internal(configuration, execution_id)
            .await
    }

    /// Durably records cancellation intent for every active provider task owned by one session.
    /// 为某个会话拥有的全部活动供应商任务持久化记录取消意图。
    ///
    /// # Parameters
    /// 参数
    /// `owner_session_id` is the exact conversation identity supplied when the tool started.
    /// `owner_session_id` 是工具启动时提供的精确会话身份。
    ///
    /// # Returns
    /// 返回值
    /// The number of records changed before the caller's asynchronous future is dropped.
    /// 返回调用方异步 Future 被丢弃前已变更的记录数量。
    pub fn request_provider_task_cancellation_for_owner(
        &self,
        owner_session_id: &str,
    ) -> Result<usize, String> {
        let mut changed = 0usize;
        for mut record in self.provider_tasks.snapshot()? {
            if record.owner_session_id.as_deref() != Some(owner_session_id)
                || record.phase.is_terminal()
            {
                continue;
            }
            if record.provider_task_id.is_none() {
                // A dropped dispatch without a confirmed task identity must never be replayed.
                // 未确认任务身份的已丢弃分派绝不能被重放。
                record.phase = ProviderAsyncTaskPhase::DispatchOutcomeUnknown;
                record.error = Some(String::from(
                    "The owning conversation stopped before provider dispatch confirmed a task identifier; automatic replay is forbidden.",
                ));
                record.consecutive_observation_failures = 0;
            } else if record.cancellation_supported {
                record.phase = ProviderAsyncTaskPhase::CancellationRequested;
                record.error = None;
                record.consecutive_observation_failures = 0;
            } else {
                // Non-cancellable provider work remains observable in the background until terminal.
                // 不支持取消的供应商任务继续由后台观测直至终态。
                record.error = Some(String::from(
                    "The owning conversation stopped, but this provider task has no cancellation endpoint; background observation continues.",
                ));
                record.consecutive_observation_failures = 0;
            }
            record.updated_at_unix_ms = current_unix_millis()?;
            self.provider_tasks.replace(record)?;
            changed = changed.saturating_add(1);
        }
        Ok(changed)
    }

    /// Starts one durable task after the route and credential are already frozen.
    /// 在路由与凭据已冻结后启动一个可持久化任务。
    ///
    /// # Parameters
    /// 参数
    /// `resolved`, `selection`, `arguments`, and `secret` freeze the exact dispatch;
    /// `owner_session_id` owns recovery, and `operation_deadline_unix_ms` preserves the total
    /// operation budget already consumed by credential and upload preparation.
    /// `resolved`、`selection`、`arguments` 与 `secret` 冻结精确分派；`owner_session_id`
    /// 持有恢复归属，`operation_deadline_unix_ms` 保留凭据与上传准备已经消耗的总操作预算。
    ///
    /// # Returns
    /// 返回值
    /// The durable execution id after confirmed task creation, or a controlled pre-dispatch,
    /// timeout, transport, or persistence failure.
    /// 确认任务创建后返回持久执行标识，或返回受控的分派前、超时、传输与持久化失败。
    pub(super) async fn start_provider_async_task(
        &self,
        resolved: ResolvedProviderToolTarget,
        selection: ProviderToolTargetSelection,
        arguments: serde_json::Value,
        secret: CredentialExecutionSecret,
        owner_session_id: Option<String>,
        operation_deadline_unix_ms: u64,
    ) -> Result<String, String> {
        let diagnostic_context = resolved.diagnostic_context();
        let now = current_unix_millis()?;
        let execution_id = Uuid::new_v4().to_string();
        let operation = resolved.operation.as_ref().ok_or_else(|| {
            format!(
                "Provider asynchronous target `{}` is not owned by one operation declaration.",
                resolved.target.target_id
            )
        })?;
        let async_execution = operation.async_execution.as_ref().ok_or_else(|| {
            format!(
                "Provider asynchronous operation `{}` has no async execution declaration.",
                operation.operation_id
            )
        })?;
        let operation_id = operation.operation_id.clone();
        let poll_interval_ms = async_execution.poll_interval_ms;
        let task_timeout_ms = async_execution.timeout_ms;
        let manifest_allows_cancellation = async_execution.cancellation_supported;
        // Freeze the source manifest revision and the complete non-secret task route at dispatch;
        // every later recovery or poll resolves the service entry and target from this snapshot
        // and never reads a newer manifest (plan §R9.6).
        // 在分派时冻结源清单修订号与完整非秘密任务路由；后续每次恢复或轮询都从该快照
        // 解析服务入口与目标，绝不读取更新的 Manifest（方案 §R9.6）。
        let manifest_revision = resolved.manifest_revision;
        let (frozen_target, frozen_service_entry) = if manifest_revision > 0 {
            (Some(resolved.target.clone()), Some(resolved.entry.clone()))
        } else {
            // Code-owned targets are static host code with no manifest provenance; the route is
            // reconstructed from the immutable catalog at recovery.
            // 代码持有目标是静态宿主代码且无清单溯源；恢复时从不可变目录重建路由。
            (None, None)
        };
        let task_deadline_unix_ms = now
            .checked_add(task_timeout_ms)
            .ok_or_else(|| String::from("Provider asynchronous task deadline overflowed."))?
            .min(operation_deadline_unix_ms);
        if task_deadline_unix_ms <= now {
            return Err(String::from(
                "Provider asynchronous task exhausted its frozen operation timeout before dispatch.",
            ));
        }
        let mut record = ProviderAsyncTaskRecord {
            execution_id: execution_id.clone(),
            owner_session_id,
            route: ProviderAsyncTaskRoute {
                provider_id: resolved.connection.provider_id.clone(),
                service_entry_id: resolved.entry.id.clone(),
                connection_id: resolved.connection.id.clone(),
                selection,
                adapter: resolved.target.execution.adapter,
                manifest_revision,
                frozen_target,
                frozen_service_entry,
                operation_id: Some(operation_id),
                frozen_operation: Some(operation.clone()),
                frozen_request_profiles: resolved.request_profiles.clone(),
                frozen_response_profiles: resolved.response_profiles.clone(),
            },
            provider_task_id: None,
            recovery: ProviderAsyncTaskRecoveryContext::None,
            phase: ProviderAsyncTaskPhase::Dispatching,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
            deadline_unix_ms: task_deadline_unix_ms,
            poll_interval_ms,
            consecutive_observation_failures: 0,
            cancellation_supported: manifest_allows_cancellation,
            error: None,
            result: None,
        };
        self.provider_tasks.replace(record.clone())?;
        let started = match await_bounded_provider_task_operation(
            PROVIDER_ASYNC_TASK_OBSERVATION_TIMEOUT.min(Duration::from_millis(
                task_deadline_unix_ms.saturating_sub(now),
            )),
            "Provider asynchronous task start",
            crate::provider_tool_execution::start_provider_async_task(
                crate::provider_tool_execution::ProviderAsyncExecutionContext {
                    http: &self.execution_client.http,
                    entry: &resolved.entry,
                    target: &resolved.target,
                    request_profiles: &resolved.request_profiles,
                    response_profiles: &resolved.response_profiles,
                    secret: &secret,
                },
                arguments,
                Some(async_execution),
            ),
        )
        .await
        {
            Ok(started) => started,
            Err(error) => {
                let error = diagnose_provider_tool_failure_with_secret(
                    error,
                    diagnostic_context.as_str(),
                    &secret,
                );
                record.phase = ProviderAsyncTaskPhase::DispatchOutcomeUnknown;
                record.updated_at_unix_ms = current_unix_millis()?;
                record.error = Some(format!(
                    "Provider task start did not confirm a task identifier; automatic replay is forbidden: {error}"
                ));
                self.provider_tasks.replace(record)?;
                return Err(error);
            }
        };
        record.provider_task_id = Some(started.provider_task_id);
        record.recovery = started.recovery;
        record.phase = ProviderAsyncTaskPhase::Queued;
        record.cancellation_supported =
            manifest_allows_cancellation && started.cancellation_supported;
        record.updated_at_unix_ms = current_unix_millis()?;
        self.provider_tasks.replace(record)?;
        Ok(execution_id)
    }

    /// Waits for one durable task while polling only the same provider task identifier.
    /// 等待一个可持久化任务，同时仅轮询同一个供应商任务标识。
    pub(super) async fn await_provider_async_task(
        &self,
        configuration: &PersistedAiConfiguration,
        execution_id: &str,
    ) -> Result<ProviderToolExecutionResult, String> {
        loop {
            let record = self.provider_tasks.require(execution_id)?;
            match record.phase {
                ProviderAsyncTaskPhase::Completed => {
                    if let Some(result) = record.result {
                        return Ok(result);
                    }
                    // Durable snapshots intentionally omit generated bytes and temporary URLs.
                    // 持久快照有意省略生成字节与临时 URL。
                    self.poll_provider_async_task_once(configuration, execution_id)
                        .await?;
                    continue;
                }
                ProviderAsyncTaskPhase::Failed
                | ProviderAsyncTaskPhase::Cancelled
                | ProviderAsyncTaskPhase::DispatchOutcomeUnknown => {
                    return Err(record.error.unwrap_or_else(|| {
                        format!(
                            "Provider task execution `{execution_id}` ended without a diagnostic."
                        )
                    }));
                }
                ProviderAsyncTaskPhase::Dispatching
                | ProviderAsyncTaskPhase::Queued
                | ProviderAsyncTaskPhase::Running
                | ProviderAsyncTaskPhase::CancellationRequested => {}
            }
            if let Some(timeout_error) = provider_task_timeout_diagnostic(
                record.created_at_unix_ms,
                record.deadline_unix_ms,
                current_unix_millis()?,
            ) {
                let mut timed_out = record;
                timed_out.phase = ProviderAsyncTaskPhase::Failed;
                timed_out.updated_at_unix_ms = current_unix_millis()?;
                timed_out.error = Some(timeout_error);
                timed_out.consecutive_observation_failures = 0;
                self.provider_tasks.replace(timed_out.clone())?;
                return Err(timed_out.error.expect("timeout diagnostic is present"));
            }
            // nextDelay cannot cross the frozen lifecycle deadline even after failure backoff.
            // nextDelay 即使处于失败退避也不能越过冻结生命周期截止时间。
            let next_delay_ms = clamp_provider_task_observation_delay_ms(
                provider_task_observation_delay_ms(&record),
                record.deadline_unix_ms,
                current_unix_millis()?,
            );
            tokio::time::sleep(Duration::from_millis(next_delay_ms)).await;
            if let Err(error) = self
                .poll_provider_async_task_once(configuration, execution_id)
                .await
            {
                self.record_provider_task_observation_error(execution_id, error)?;
            }
        }
    }

    /// Observes one retained provider task once and persists every confirmed transition.
    /// 单次观测一个保留供应商任务并持久化每个已确认转换。
    async fn poll_provider_async_task_once(
        &self,
        configuration: &PersistedAiConfiguration,
        execution_id: &str,
    ) -> Result<ProviderAsyncTaskRecord, String> {
        let mut record = self.provider_tasks.require(execution_id)?;
        let needs_completed_result_recovery =
            record.phase == ProviderAsyncTaskPhase::Completed && record.result.is_none();
        if record.phase.is_terminal() && !needs_completed_result_recovery {
            return Ok(record);
        }
        if record.phase == ProviderAsyncTaskPhase::CancellationRequested {
            return self
                .cancel_provider_task_internal(configuration, execution_id)
                .await;
        }
        // A provider-confirmed completed task is no longer executing. Its original work deadline
        // must not block the read-only re-observation needed to reconstruct omitted result bytes.
        // 供应商已确认完成的任务不再执行；原始工作截止时间不得阻止为重建已省略结果字节而进行的
        // 只读重新观测。
        if !needs_completed_result_recovery
            && let Some(timeout_error) = provider_task_timeout_diagnostic(
                record.created_at_unix_ms,
                record.deadline_unix_ms,
                current_unix_millis()?,
            )
        {
            record.phase = ProviderAsyncTaskPhase::Failed;
            record.updated_at_unix_ms = current_unix_millis()?;
            record.error = Some(timeout_error);
            record.consecutive_observation_failures = 0;
            self.provider_tasks.replace(record.clone())?;
            return Ok(record);
        }
        let provider_task_id = record.provider_task_id.clone().ok_or_else(|| {
            format!(
                "Provider task execution `{execution_id}` has no confirmed provider task identifier."
            )
        })?;
        let resolved = self.resolve_recovered_provider_task(configuration, &record)?;
        let diagnostic_context = resolved.diagnostic_context();
        // observationDeadline covers credential resolution and transport together. Otherwise a
        // blocked refresh lock could bypass both the task lifecycle deadline and the host's
        // nominal 60-second observation ceiling.
        // observationDeadline 同时覆盖凭据解析与传输。否则阻塞的刷新锁可能绕过任务生命周期
        // 截止时间以及宿主名义上的 60 秒观测上限。
        let observation_deadline = Instant::now()
            .checked_add(PROVIDER_ASYNC_TASK_OBSERVATION_TIMEOUT)
            .ok_or_else(|| String::from("Provider task observation deadline overflowed."))?;
        let credential_timeout = if needs_completed_result_recovery {
            remaining_provider_operation_timeout(
                observation_deadline,
                "Provider asynchronous task credential resolution",
            )?
        } else {
            remaining_provider_operation_timeout(
                observation_deadline,
                "Provider asynchronous task credential resolution",
            )?
            .min(Duration::from_millis(
                record
                    .deadline_unix_ms
                    .saturating_sub(current_unix_millis()?),
            ))
        };
        let secret = await_bounded_provider_task_operation(
            credential_timeout,
            "Provider asynchronous task credential resolution",
            self.resolve_fresh_execution_secret(
                resolved.connection.id.as_str(),
                resolved.connection.secret_reference.as_str(),
            ),
        )
        .await
        .map_err(|error| diagnose_provider_tool_failure(error, diagnostic_context.as_str()))?;
        // observationTimeout is recomputed after credential resolution so secret refresh time also
        // consumes the frozen lifecycle budget. Completed-result recovery is read-only and retains
        // the independent bounded observation timeout.
        // observationTimeout 在凭据解析后重新计算，使 Secret 刷新耗时同样消耗冻结生命周期
        // 预算；已完成结果恢复属于只读操作，继续使用独立的有界观测超时。
        let observation_timeout = if needs_completed_result_recovery {
            remaining_provider_operation_timeout(
                observation_deadline,
                "Provider asynchronous task observation",
            )?
        } else {
            let now_unix_ms = current_unix_millis()?;
            if let Some(timeout_error) = provider_task_timeout_diagnostic(
                record.created_at_unix_ms,
                record.deadline_unix_ms,
                now_unix_ms,
            ) {
                record.phase = ProviderAsyncTaskPhase::Failed;
                record.updated_at_unix_ms = now_unix_ms;
                record.error = Some(timeout_error);
                record.consecutive_observation_failures = 0;
                self.provider_tasks.replace(record.clone())?;
                return Ok(record);
            }
            remaining_provider_operation_timeout(
                observation_deadline,
                "Provider asynchronous task observation",
            )?
            .min(Duration::from_millis(
                record.deadline_unix_ms.saturating_sub(now_unix_ms),
            ))
        };
        match await_bounded_provider_task_operation(
            observation_timeout,
            "Provider asynchronous task observation",
            crate::provider_tool_execution::poll_provider_async_task(
                crate::provider_tool_execution::ProviderAsyncExecutionContext {
                    http: &self.execution_client.http,
                    entry: &resolved.entry,
                    target: &resolved.target,
                    request_profiles: &resolved.request_profiles,
                    response_profiles: &resolved.response_profiles,
                    secret: &secret,
                },
                provider_task_id.as_str(),
                &record.recovery,
                resolved
                    .operation
                    .as_ref()
                    .and_then(|operation| operation.async_execution.as_ref()),
            ),
        )
        .await
        .map_err(|error| {
            diagnose_provider_tool_failure_with_secret(error, diagnostic_context.as_str(), &secret)
        })? {
            ProviderAsyncTaskPoll::Pending(phase) => {
                record.phase = phase;
                record.error = None;
                record.consecutive_observation_failures = 0;
            }
            ProviderAsyncTaskPoll::Completed(result) => {
                record.phase = ProviderAsyncTaskPhase::Completed;
                record.result = Some(result);
                record.error = None;
                record.consecutive_observation_failures = 0;
            }
            ProviderAsyncTaskPoll::Failed(error) => {
                record.phase = ProviderAsyncTaskPhase::Failed;
                record.error = Some(redact_provider_tool_secret(error.as_str(), &secret));
                record.consecutive_observation_failures = 0;
            }
            ProviderAsyncTaskPoll::Cancelled => {
                record.phase = ProviderAsyncTaskPhase::Cancelled;
                record.error = Some(String::from("Provider task was cancelled."));
                record.consecutive_observation_failures = 0;
            }
        }
        record.updated_at_unix_ms = current_unix_millis()?;
        self.provider_tasks.replace(record.clone())?;
        Ok(record)
    }

    /// Sends one cancellation request for a previously persisted intent.
    /// 为先前已持久化的意图发送一次取消请求。
    async fn cancel_provider_task_internal(
        &self,
        configuration: &PersistedAiConfiguration,
        execution_id: &str,
    ) -> Result<ProviderAsyncTaskRecord, String> {
        let mut record = self.provider_tasks.require(execution_id)?;
        if record.phase != ProviderAsyncTaskPhase::CancellationRequested {
            return Err(format!(
                "Provider task execution `{execution_id}` has no durable cancellation intent."
            ));
        }
        let provider_task_id = record.provider_task_id.clone().ok_or_else(|| {
            format!(
                "Provider task execution `{execution_id}` has no confirmed provider task identifier."
            )
        })?;
        let resolved = self.resolve_recovered_provider_task(configuration, &record)?;
        let diagnostic_context = resolved.diagnostic_context();
        // cancellationDeadline is one total host budget for both credential refresh and the
        // provider cancellation request.
        // cancellationDeadline 是凭据刷新与供应商取消请求共同使用的一份宿主总预算。
        let cancellation_deadline = Instant::now()
            .checked_add(PROVIDER_ASYNC_TASK_OBSERVATION_TIMEOUT)
            .ok_or_else(|| String::from("Provider task cancellation deadline overflowed."))?;
        let secret = await_bounded_provider_task_operation(
            remaining_provider_operation_timeout(
                cancellation_deadline,
                "Provider asynchronous task credential resolution",
            )?,
            "Provider asynchronous task credential resolution",
            self.resolve_fresh_execution_secret(
                resolved.connection.id.as_str(),
                resolved.connection.secret_reference.as_str(),
            ),
        )
        .await
        .map_err(|error| diagnose_provider_tool_failure(error, diagnostic_context.as_str()))?;
        await_bounded_provider_task_operation(
            remaining_provider_operation_timeout(
                cancellation_deadline,
                "Provider asynchronous task cancellation",
            )?,
            "Provider asynchronous task cancellation",
            crate::provider_tool_execution::cancel_provider_async_task(
                &self.execution_client.http,
                &resolved.entry,
                &resolved.target,
                provider_task_id.as_str(),
                &secret,
                resolved
                    .operation
                    .as_ref()
                    .and_then(|operation| operation.async_execution.as_ref()),
            ),
        )
        .await
        .map_err(|error| {
            diagnose_provider_tool_failure_with_secret(error, diagnostic_context.as_str(), &secret)
        })?;
        record.phase = ProviderAsyncTaskPhase::Cancelled;
        record.updated_at_unix_ms = current_unix_millis()?;
        record.error = Some(String::from("Provider cancellation was confirmed."));
        record.consecutive_observation_failures = 0;
        self.provider_tasks.replace(record.clone())?;
        Ok(record)
    }

    /// Resolves the exact retained route without consulting current tool selections.
    /// 解析精确保留路由且不查询当前工具选择。
    fn resolve_recovered_provider_task(
        &self,
        configuration: &PersistedAiConfiguration,
        record: &ProviderAsyncTaskRecord,
    ) -> Result<ResolvedProviderToolTarget, String> {
        let connection = configuration
            .connection_graph
            .connections
            .get(record.route.connection_id.as_str())
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Provider task connection `{}` no longer exists.",
                    record.route.connection_id
                )
            })?;
        if connection.provider_id != record.route.provider_id
            || connection.service_entry_id != record.route.service_entry_id
            || record.route.selection.connection_id != connection.id
        {
            return Err(String::from(
                "Provider task route affinity no longer matches its retained connection.",
            ));
        }
        // Modern manifest tasks use both frozen halves of the non-secret route. The current
        // registry is deliberately not consulted, so replacing or retiring the provider entry
        // cannot redirect or reinterpret an already-created task (plan §R9.6).
        // 现代 Manifest 任务使用非秘密路由的两份冻结组成。这里刻意不查询当前注册表，
        // 因此替换或退役供应商入口不会重定向或重新解释已创建任务（方案 §R9.6）。
        let (entry, target) = if record.route.manifest_revision > 0 {
            let entry = record
                .route
                .frozen_service_entry
                .clone()
                .ok_or_else(|| {
                    format!(
                        "Provider task execution `{}` at manifest revision {} is missing its dispatch-frozen service-entry snapshot.",
                        record.execution_id, record.route.manifest_revision
                    )
                })?;
            let target = record.route.frozen_target.clone().ok_or_else(|| {
                format!(
                    "Provider task execution `{}` at manifest revision {} is missing its dispatch-frozen target snapshot.",
                    record.execution_id, record.route.manifest_revision
                )
            })?;
            (entry, target)
        } else {
            // Historical revision-zero records have no frozen endpoint evidence. Their exact
            // retained target id is reconstructed through the compatibility path and only then
            // receives the current connection origin.
            // 历史零修订记录没有冻结端点证据。兼容路径按其精确保留目标标识重建，
            // 并仅在此后应用当前连接源地址。
            let registry = self.effective_providers(configuration)?;
            let provider = registry
                .provider(record.route.provider_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Provider task provider `{}` is not registered.",
                        record.route.provider_id
                    )
                })?;
            let entry = provider
                .service_entries
                .iter()
                .find(|entry| entry.id == record.route.service_entry_id)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Provider task service entry `{}` is not registered.",
                        record.route.service_entry_id
                    )
                })?;
            let legacy_target = self
                .resolved_provider_tool_targets(provider.id.as_str())
                .into_iter()
                .find(|target| target.target_id == record.route.selection.target_id)
                .ok_or_else(|| {
                    format!(
                        "Provider task target `{}` is not registered.",
                        record.route.selection.target_id
                    )
                })?;
            let target = crate::connection_endpoint::apply_connection_provider_tool_origin(
                &connection,
                &entry,
                legacy_target,
            );
            (entry, target)
        };
        let credential_profile_matches = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .is_some_and(|profile| profile.credential_mode == connection.credential_mode);
        if entry.id != record.route.service_entry_id
            || target.provider_id != record.route.provider_id
            || target.service_entry_id != entry.id
            || target.execution.adapter != record.route.adapter
            || !credential_profile_matches
            || !provider_tool_target_allows_credential(
                &target,
                connection.credential_profile_id.as_str(),
            )
        {
            return Err(String::from(
                "Provider task target no longer matches its immutable route affinity.",
            ));
        }
        Ok(ResolvedProviderToolTarget {
            connection,
            entry,
            target,
            operation: record.route.frozen_operation.clone(),
            request_profiles: record.route.frozen_request_profiles.clone(),
            response_profiles: record.route.frozen_response_profiles.clone(),
            upload_channels: Vec::new(),
            manifest_revision: record.route.manifest_revision,
            manifest_generation: 0,
            route_revision: format!(
                "provider-tool:{}:{}:{}",
                record.route.provider_id,
                record.route.connection_id,
                record.route.selection.target_id
            ),
        })
    }

    /// Retains one transient observation diagnostic without changing lifecycle phase.
    /// 保留一个瞬时观测诊断且不改变生命周期阶段。
    fn record_provider_task_observation_error(
        &self,
        execution_id: &str,
        error: String,
    ) -> Result<(), String> {
        let mut record = self.provider_tasks.require(execution_id)?;
        // Durable completed records intentionally lack process-local result bytes, so a failed
        // re-observation still needs a visible diagnostic even though the provider phase is terminal.
        // 持久完成记录刻意不含进程内结果字节，因此重新观测失败时即使供应商阶段已终结，
        // 仍需保留可见诊断。
        let needs_completed_result_recovery =
            record.phase == ProviderAsyncTaskPhase::Completed && record.result.is_none();
        if !record.phase.is_terminal() || needs_completed_result_recovery {
            let now_unix_ms = current_unix_millis()?;
            record.updated_at_unix_ms = now_unix_ms;
            if !needs_completed_result_recovery
                && let Some(timeout_error) = provider_task_timeout_diagnostic(
                    record.created_at_unix_ms,
                    record.deadline_unix_ms,
                    now_unix_ms,
                )
            {
                // A failed observation that consumed the frozen deadline is terminal now; it
                // must not wait for another recovery pass to reveal the timeout.
                // 已消耗冻结截止时间的失败观测立即终态化；不得等待下一轮恢复才暴露超时。
                record.phase = ProviderAsyncTaskPhase::Failed;
                record.error = Some(timeout_error);
                record.consecutive_observation_failures = 0;
            } else {
                record.error = Some(bound_provider_task_diagnostic(error.as_str()));
                record.consecutive_observation_failures = record
                    .consecutive_observation_failures
                    .saturating_add(1)
                    .min(crate::provider_task::PROVIDER_TASK_MAXIMUM_OBSERVATION_FAILURES);
            }
            self.provider_tasks.replace(record)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
