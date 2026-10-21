//! In-memory registry of media jobs started by MCP clients.
//! MCP 客户端启动的媒体任务的内存注册表。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::AbortHandle;

use crate::backend::McpMediaOutcome;
use crate::{MCP_MAX_RETAINED_MEDIA_JOBS_PER_CLIENT, MCP_MAX_RUNNING_MEDIA_JOBS_PER_CLIENT};

/// State of one media job.
/// 一个媒体任务的状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MediaJobState {
    /// The provider is still working.
    /// 供应商仍在处理。
    Running,
    /// The job produced its outputs.
    /// 任务已产出结果。
    Succeeded(McpMediaOutcome),
    /// The job failed with an English error.
    /// 任务以英文错误失败。
    Failed(String),
    /// The client cancelled the job.
    /// 客户端取消了任务。
    Cancelled,
}

impl MediaJobState {
    /// Returns the stable status label.
    /// 返回稳定状态标签。
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded(_) => "succeeded",
            Self::Failed(_) => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// One registered job.
/// 一个已登记任务。
pub(crate) struct MediaJob {
    /// Job identifier.
    /// 任务标识。
    pub(crate) id: String,
    /// Owning client.
    /// 所属客户端。
    pub(crate) client_id: String,
    /// Public model identifier.
    /// 公开模型标识。
    pub(crate) model: String,
    /// Capability that ran.
    /// 运行的能力。
    pub(crate) capability: String,
    /// Provider-task owner used for cancellation.
    /// 用于取消的供应商任务所有者。
    pub(crate) owner_id: String,
    /// Latest state.
    /// 最新状态。
    state: watch::Receiver<MediaJobState>,
    /// Writer kept so cancellation can publish its state.
    /// 保留的写端，使取消可以发布其状态。
    sender: watch::Sender<MediaJobState>,
    /// Handle that aborts the running provider future.
    /// 中止运行中供应商 Future 的句柄。
    abort: Mutex<Option<AbortHandle>>,
}

impl MediaJob {
    /// Returns the current state.
    /// 返回当前状态。
    pub(crate) fn state(&self) -> MediaJobState {
        self.state.borrow().clone()
    }

    /// Publishes a terminal state unless the job already ended.
    /// 发布终态，除非任务已经结束。
    pub(crate) fn finish(&self, state: MediaJobState) {
        self.sender.send_if_modified(|current| {
            if *current == MediaJobState::Running {
                *current = state;
                true
            } else {
                false
            }
        });
    }

    /// Records the task handle that runs this job.
    /// 记录运行该任务的句柄。
    pub(crate) fn attach_abort(&self, abort: AbortHandle) {
        if let Ok(mut slot) = self.abort.lock() {
            *slot = Some(abort);
        }
    }

    /// Aborts the running future, if any.
    /// 中止运行中的 Future（若有）。
    pub(crate) fn abort(&self) {
        if let Ok(slot) = self.abort.lock()
            && let Some(abort) = slot.as_ref()
        {
            abort.abort();
        }
    }

    /// Waits up to `timeout` for the job to leave the running state and returns the state then.
    /// 最多等待 `timeout` 使任务离开运行状态，并返回届时的状态。
    pub(crate) async fn wait(&self, timeout: Duration) -> MediaJobState {
        let mut receiver = self.state.clone();
        let _ = tokio::time::timeout(
            timeout,
            receiver.wait_for(|state| *state != MediaJobState::Running),
        )
        .await;
        self.state()
    }
}

/// Bounded registry of jobs, oldest first.
/// 有界任务注册表，最早的在前。
#[derive(Default)]
pub(crate) struct MediaJobRegistry {
    /// Jobs in creation order.
    /// 按创建顺序排列的任务。
    jobs: Mutex<VecDeque<Arc<MediaJob>>>,
}

impl MediaJobRegistry {
    /// Registers a running job for `client_id`, evicting the client's oldest finished jobs.
    /// 为 `client_id` 登记一个运行中任务，并淘汰该客户端最早的已结束任务。
    ///
    /// # Returns
    /// 返回值
    /// The job, or an English error when the client already runs the maximum number of jobs.
    /// 返回任务；客户端已运行最大数量任务时返回英文错误。
    pub(crate) fn register(
        &self,
        id: String,
        client_id: &str,
        model: String,
        capability: String,
        owner_id: String,
    ) -> Result<Arc<MediaJob>, String> {
        let mut jobs = self
            .jobs
            .lock()
            .map_err(|_| String::from("The media job registry is unavailable."))?;
        let running = jobs
            .iter()
            .filter(|job| job.client_id == client_id && job.state() == MediaJobState::Running)
            .count();
        if running >= MCP_MAX_RUNNING_MEDIA_JOBS_PER_CLIENT {
            return Err(format!(
                "This client already runs {MCP_MAX_RUNNING_MEDIA_JOBS_PER_CLIENT} media jobs. Wait for one to finish or cancel it first."
            ));
        }
        while jobs.iter().filter(|job| job.client_id == client_id).count()
            >= MCP_MAX_RETAINED_MEDIA_JOBS_PER_CLIENT
        {
            let Some(position) = jobs.iter().position(|job| {
                job.client_id == client_id && job.state() != MediaJobState::Running
            }) else {
                break;
            };
            jobs.remove(position);
        }
        let (sender, state) = watch::channel(MediaJobState::Running);
        let job = Arc::new(MediaJob {
            id,
            client_id: client_id.to_string(),
            model,
            capability,
            owner_id,
            state,
            sender,
            abort: Mutex::new(None),
        });
        jobs.push_back(Arc::clone(&job));
        Ok(job)
    }

    /// Finds a job owned by `client_id`.
    /// 查找 `client_id` 拥有的任务。
    pub(crate) fn find(&self, client_id: &str, job_id: &str) -> Option<Arc<MediaJob>> {
        self.jobs.lock().ok().and_then(|jobs| {
            jobs.iter()
                .find(|job| job.id == job_id && job.client_id == client_id)
                .cloned()
        })
    }

    /// Drops every job of a client that no longer exists, aborting running ones.
    /// 移除已不存在客户端的全部任务，并中止运行中的任务。
    pub(crate) fn retain_clients(&self, live_client_ids: &[&str]) -> Vec<Arc<MediaJob>> {
        let Ok(mut jobs) = self.jobs.lock() else {
            return Vec::new();
        };
        let mut removed = Vec::new();
        jobs.retain(|job| {
            let keep = live_client_ids.contains(&job.client_id.as_str());
            if !keep {
                removed.push(Arc::clone(job));
            }
            keep
        });
        removed
    }
}
