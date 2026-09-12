//! Provider-neutral asynchronous embedding Batch contracts.
//! 供应商无关的异步向量 Batch 契约。

use crate::{EmbeddingBatchCapabilities, EmbeddingResponse, ExecutionRouteSnapshot};
use serde::{Deserialize, Serialize};

/// Describes one deterministic contiguous provider Batch payload.
/// 描述一个确定性的连续供应商 Batch 负载。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingBatchTaskPlan {
    /// Stable task identity derived from the frozen request and range.
    /// 从冻结请求及范围派生的稳定任务标识。
    pub task_id: String,
    /// Zero-based first input index owned by this task.
    /// 当前任务持有的从零开始首个输入索引。
    pub first_input_index: u32,
    /// Number of contiguous inputs owned by this task.
    /// 当前任务持有的连续输入数量。
    pub input_count: u32,
    /// Exact encoded provider payload length.
    /// 精确的供应商编码负载长度。
    pub encoded_payload_bytes: u64,
    /// Lowercase SHA-256 of the exact encoded provider payload.
    /// 精确供应商编码负载的小写 SHA-256。
    pub payload_sha256: String,
}

impl EmbeddingBatchTaskPlan {
    /// Validates the immutable identity, range, and payload fingerprint.
    /// 校验不可变身份、范围及负载指纹。
    ///
    /// # Returns
    /// 返回值
    /// Unit for a closed task plan, or a precise contract message.
    /// 任务计划封闭时返回空值，否则返回明确契约消息。
    pub fn validate(&self) -> Result<(), String> {
        if self.task_id.trim().is_empty()
            || self.input_count == 0
            || self.encoded_payload_bytes == 0
            || self.payload_sha256.len() != 64
            || !self
                .payload_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || self
                .first_input_index
                .checked_add(self.input_count)
                .is_none()
        {
            return Err(String::from(
                "Embedding Batch task plan identity, range, or payload fingerprint is invalid",
            ));
        }
        Ok(())
    }

    /// Returns the exclusive input boundary of this contiguous task.
    /// 返回当前连续任务输入范围的排他结束边界。
    ///
    /// # Returns
    /// 返回值
    /// The checked exclusive end index.
    /// 返回经过校验的排他结束索引。
    pub fn end_input_index(&self) -> Result<u32, String> {
        self.first_input_index
            .checked_add(self.input_count)
            .ok_or_else(|| String::from("Embedding Batch task input range overflows u32"))
    }
}

/// Freezes one complete deterministic embedding Batch plan before provider I/O.
/// 在供应商 I/O 前冻结一份完整且确定性的向量 Batch 计划。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingBatchPlan {
    /// Original typed embedding request identity.
    /// 原始类型化向量请求标识。
    pub request_id: String,
    /// SHA-256 over the immutable typed request and selected route.
    /// 不可变类型化请求及所选路由的 SHA-256。
    pub request_fingerprint: String,
    /// Exact selected provider route.
    /// 精确选择的供应商路由。
    pub route: ExecutionRouteSnapshot,
    /// Frozen capability and effective provider/host limits.
    /// 冻结能力以及有效供应商与宿主限制。
    pub capabilities: EmbeddingBatchCapabilities,
    /// Ordered, gap-free provider task partition.
    /// 有序且无间隙的供应商任务分区。
    pub tasks: Vec<EmbeddingBatchTaskPlan>,
}

/// Records one durable provider input file uploaded before Batch creation.
/// 记录一份在创建 Batch 前已上传并持久化的供应商输入文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingBatchUploadedInput {
    /// Exact deterministic task identity represented by the uploaded file.
    /// 已上传文件所表示的精确确定性任务身份。
    pub task_id: String,
    /// Provider-owned input file identifier.
    /// 供应商持有的输入文件标识。
    pub provider_input_file_id: String,
    /// Host observation time immediately after successful upload.
    /// 上传成功后宿主立即记录的观测时间。
    pub uploaded_at_unix_ms: u64,
}

impl EmbeddingBatchUploadedInput {
    /// Validates the durable file identity required between upload and Batch creation.
    /// 校验上传与创建 Batch 之间所需的持久文件身份。
    ///
    /// # Returns
    /// 返回值
    /// Unit when the exact uploaded input can be reused without another upload.
    /// 可以在不重复上传的情况下复用精确输入文件时返回空值。
    pub fn validate(&self) -> Result<(), String> {
        if self.task_id.trim().is_empty()
            || !embedding_batch_provider_identifier_valid(&self.provider_input_file_id)
            || self.uploaded_at_unix_ms == 0
        {
            return Err(String::from(
                "Embedding Batch uploaded input identity is incomplete",
            ));
        }
        Ok(())
    }
}

impl EmbeddingBatchPlan {
    /// Validates that tasks form one ordered gap-free partition of `expected_inputs`.
    /// 校验任务形成 `expected_inputs` 的一个有序无间隙分区。
    ///
    /// # Parameters
    /// 参数
    /// `expected_inputs` is the exact original request input count.
    /// `expected_inputs` 是原始请求的精确输入数量。
    ///
    /// # Returns
    /// 返回值
    /// Unit when all immutable planning invariants hold.
    /// 全部不可变规划约束成立时返回空值。
    pub fn validate(&self, expected_inputs: usize) -> Result<(), String> {
        self.capabilities.validate()?;
        let maximum_tasks = usize::try_from(self.capabilities.effective_maximum_tasks_per_run())
            .map_err(|_| String::from("Embedding Batch task limit exceeds usize"))?;
        if self.request_id.trim().is_empty()
            || self.request_fingerprint.len() != 64
            || !self
                .request_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || expected_inputs == 0
            || self.tasks.is_empty()
            || self.tasks.len() > maximum_tasks
        {
            return Err(String::from("Embedding Batch plan identity is incomplete"));
        }
        let expected_inputs = u32::try_from(expected_inputs)
            .map_err(|_| String::from("Embedding Batch input count exceeds u32"))?;
        let mut next_input_index = 0u32;
        for task in &self.tasks {
            task.validate()?;
            if task.first_input_index != next_input_index
                || task.input_count > self.capabilities.effective_maximum_requests()
                || task.encoded_payload_bytes > self.capabilities.effective_maximum_payload_bytes()
            {
                return Err(String::from(
                    "Embedding Batch tasks are not an ordered bounded partition",
                ));
            }
            next_input_index = task.end_input_index()?;
        }
        if next_input_index != expected_inputs {
            return Err(String::from(
                "Embedding Batch tasks do not cover the complete request",
            ));
        }
        Ok(())
    }
}

/// Records the durable provider identity returned after one successful Batch submission.
/// 记录一次 Batch 成功提交后返回的持久供应商身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingBatchSubmission {
    /// Exact deterministic task identity submitted.
    /// 已提交的精确确定性任务标识。
    pub task_id: String,
    /// Provider-owned asynchronous Batch identifier.
    /// 供应商持有的异步 Batch 标识。
    pub provider_batch_id: String,
    /// Provider-owned input file identifier for file-backed transports.
    /// 文件传输使用的供应商输入文件标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_input_file_id: Option<String>,
    /// Host observation time immediately after successful submission.
    /// 成功提交后宿主立即记录的观测时间。
    pub submitted_at_unix_ms: u64,
}

impl EmbeddingBatchSubmission {
    /// Validates the durable provider identity required for restart recovery.
    /// 校验重启恢复所需的持久供应商身份。
    ///
    /// # Returns
    /// 返回值
    /// Unit when the submission can be polled without replay.
    /// 提交记录可在不重放的情况下轮询时返回空值。
    pub fn validate(&self) -> Result<(), String> {
        if self.task_id.trim().is_empty()
            || !embedding_batch_provider_identifier_valid(&self.provider_batch_id)
            || self.submitted_at_unix_ms == 0
            || self
                .provider_input_file_id
                .as_ref()
                .is_some_and(|file_id| !embedding_batch_provider_identifier_valid(file_id))
        {
            return Err(String::from(
                "Embedding Batch submission identity is incomplete",
            ));
        }
        Ok(())
    }
}

/// Normalizes the lifecycle states shared by supported asynchronous Batch providers.
/// 归一化受支持异步 Batch 供应商共享的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingBatchProviderState {
    /// Provider is validating the accepted task.
    /// 供应商正在校验已受理任务。
    Validating,
    /// Provider is processing request items.
    /// 供应商正在处理请求项。
    InProgress,
    /// Provider is assembling terminal output.
    /// 供应商正在组装终态输出。
    Finalizing,
    /// Every result file or inline item is ready.
    /// 全部结果文件或内嵌结果项已经就绪。
    Completed,
    /// Provider rejected or failed the task.
    /// 供应商拒绝任务或任务执行失败。
    Failed,
    /// Completion window elapsed before completion.
    /// 任务在完成前超过完成窗口。
    Expired,
    /// Cancellation has been accepted but is not terminal.
    /// 取消已被受理但尚未达到终态。
    Cancelling,
    /// Provider reached a cancelled terminal state.
    /// 供应商已达到已取消终态。
    Cancelled,
}

impl EmbeddingBatchProviderState {
    /// Returns whether no later provider state is expected.
    /// 返回是否不再预期后续供应商状态。
    ///
    /// # Returns
    /// 返回值
    /// `true` for completed, failed, expired, or cancelled states.
    /// 对完成、失败、过期或已取消状态返回 `true`。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Expired | Self::Cancelled
        )
    }
}

/// Carries normalized progress and result references from one status poll.
/// 承载一次状态轮询返回的归一化进度与结果引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingBatchSnapshot {
    /// Durable submission being observed.
    /// 当前观测的持久提交记录。
    pub submission: EmbeddingBatchSubmission,
    /// Current normalized provider state.
    /// 当前归一化供应商状态。
    pub state: EmbeddingBatchProviderState,
    /// Total request items reported by the provider.
    /// 供应商报告的请求项总数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_requests: Option<u32>,
    /// Completed request items reported by the provider.
    /// 供应商报告的已完成请求项数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_requests: Option<u32>,
    /// Failed request items reported by the provider.
    /// 供应商报告的失败请求项数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_requests: Option<u32>,
    /// Provider output file identity for file-backed delivery.
    /// 文件式结果交付使用的供应商输出文件标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_file_id: Option<String>,
    /// Provider error file identity for file-backed delivery.
    /// 文件式错误交付使用的供应商错误文件标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_file_id: Option<String>,
    /// Controlled provider diagnostic for failed terminal states.
    /// 失败终态对应的受控供应商诊断。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl EmbeddingBatchSnapshot {
    /// Validates provider identity, coherent request counters, and bounded result references.
    /// 校验供应商身份、一致请求计数及有界结果引用。
    ///
    /// # Returns
    /// 返回值
    /// Unit for one coherent provider observation, otherwise a precise contract message.
    /// 供应商观测一致时返回空值，否则返回明确契约消息。
    pub fn validate(&self) -> Result<(), String> {
        self.submission.validate()?;
        let counters = [
            self.total_requests,
            self.completed_requests,
            self.failed_requests,
        ];
        if counters.iter().any(Option::is_some) && counters.iter().any(Option::is_none) {
            return Err(String::from(
                "Embedding Batch snapshot request counters are partial",
            ));
        }
        if let (Some(total), Some(completed), Some(failed)) = (
            self.total_requests,
            self.completed_requests,
            self.failed_requests,
        ) {
            let settled = completed.checked_add(failed).ok_or_else(|| {
                String::from("Embedding Batch snapshot request counters overflow u32")
            })?;
            if settled > total {
                return Err(String::from(
                    "Embedding Batch snapshot request counters exceed the total",
                ));
            }
            if self.state == EmbeddingBatchProviderState::Completed && settled != total {
                return Err(String::from(
                    "Completed embedding Batch snapshot has unsettled requests",
                ));
            }
        }
        for file_id in [&self.output_file_id, &self.error_file_id]
            .into_iter()
            .flatten()
        {
            if !embedding_batch_provider_identifier_valid(file_id) {
                return Err(String::from(
                    "Embedding Batch snapshot contains an invalid result file identity",
                ));
            }
        }
        if self
            .error
            .as_ref()
            .is_some_and(|error| error.trim().is_empty() || error.len() > 8_192)
        {
            return Err(String::from(
                "Embedding Batch snapshot contains an invalid diagnostic",
            ));
        }
        Ok(())
    }
}

/// Returns whether one provider-owned identifier is safe for persistence and URL expansion.
/// 返回一个供应商持有标识是否可安全用于持久化及 URL 展开。
///
/// # Parameters
/// 参数
/// `identifier` is the exact Batch or file identifier returned by the provider.
/// `identifier` 是供应商返回的精确 Batch 或文件标识。
///
/// # Returns
/// 返回值
/// `true` only for a bounded ASCII identifier without path or query delimiters.
/// 仅对不含路径或查询分隔符的有界 ASCII 标识返回 `true`。
fn embedding_batch_provider_identifier_valid(identifier: &str) -> bool {
    !identifier.trim().is_empty()
        && identifier.len() <= 512
        && identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// Returns the normalized vectors collected from one completed remote task.
/// 返回从一个已完成远端任务收集的归一化向量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingBatchCollectedResult {
    /// Exact deterministic task identity collected.
    /// 已收集的精确确定性任务标识。
    pub task_id: String,
    /// Provider-normalized vectors retaining original request indexes.
    /// 保留原始请求索引的供应商归一化向量。
    pub response: EmbeddingResponse,
    /// Non-fatal cleanup warning when a temporary input file could not be deleted.
    /// 临时输入文件删除失败时的非致命清理警告。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_warning: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one provider snapshot with a durable submission and complete counters.
    /// 构建一份包含持久提交记录和完整计数的供应商快照。
    fn completed_snapshot_fixture() -> EmbeddingBatchSnapshot {
        EmbeddingBatchSnapshot {
            submission: EmbeddingBatchSubmission {
                task_id: String::from("task-1"),
                provider_batch_id: String::from("batch-1"),
                provider_input_file_id: None,
                submitted_at_unix_ms: 1,
            },
            state: EmbeddingBatchProviderState::Completed,
            total_requests: Some(2),
            completed_requests: Some(2),
            failed_requests: Some(0),
            output_file_id: None,
            error_file_id: None,
            error: None,
        }
    }

    /// Verifies a completed task cannot retain unsettled request counters.
    /// 验证已完成任务不能保留尚未结算的请求计数。
    #[test]
    fn completed_snapshot_rejects_unsettled_requests() {
        let mut snapshot = completed_snapshot_fixture();
        snapshot.completed_requests = Some(1);

        let error = snapshot
            .validate()
            .expect_err("completed snapshot must settle every request");
        assert!(error.contains("unsettled"));
    }

    /// Verifies active tasks may report partial progress without pretending to be complete.
    /// 验证活动任务可以报告部分进度而不会冒充完成状态。
    #[test]
    fn active_snapshot_allows_partial_progress() {
        let mut snapshot = completed_snapshot_fixture();
        snapshot.state = EmbeddingBatchProviderState::InProgress;
        snapshot.completed_requests = Some(1);

        snapshot
            .validate()
            .expect("active snapshot may have unsettled requests");
    }

    /// Verifies provider counters cannot hide arithmetic overflow behind a valid total.
    /// 验证供应商计数不能通过合法总数掩盖算术溢出。
    #[test]
    fn snapshot_rejects_counter_overflow() {
        let mut snapshot = completed_snapshot_fixture();
        snapshot.total_requests = Some(u32::MAX);
        snapshot.completed_requests = Some(u32::MAX);
        snapshot.failed_requests = Some(1);

        let error = snapshot
            .validate()
            .expect_err("overflowed counters must be rejected");
        assert!(error.contains("overflow"));
    }

    /// Verifies provider identifiers remain bounded before durable storage or URL expansion.
    /// 验证供应商标识在持久化或 URL 展开前保持有界。
    #[test]
    fn submission_rejects_oversized_provider_identifier() {
        let mut snapshot = completed_snapshot_fixture();
        snapshot.submission.provider_batch_id = "a".repeat(513);

        assert!(snapshot.validate().is_err());
    }
}
