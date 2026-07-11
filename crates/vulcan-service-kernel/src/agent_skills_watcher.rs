//! Automatic Agent Skills catalog filesystem synchronization.
//! Agent Skills 目录文件系统自动同步。

use super::*;
use std::time::Duration;
use vulcan_file_watcher::live::{
    LiveFileWatcher, LiveFileWatcherSubscriber, LiveWatchRegistration, ThrottledLiveWatchReceiver,
};

/// Process-lifetime Agent Skills watcher ownership.
/// 进程生命周期 Agent Skills 监听器所有权。
static AGENT_SKILLS_WATCHER: OnceLock<AgentSkillsWatcherHandle> = OnceLock::new();

/// Owns every resource required to keep Agent Skills synchronization alive.
/// 持有保持 Agent Skills 同步存活所需的全部资源。
struct AgentSkillsWatcherHandle {
    /// Shared notify-backed filesystem watcher.
    /// 共享 notify 文件系统监听器。
    _watcher: Arc<LiveFileWatcher>,
    /// Logical subscriber registered for both catalog roots.
    /// 为两个目录根注册的逻辑订阅者。
    _subscriber: LiveFileWatcherSubscriber,
    /// Registration retained until process shutdown.
    /// 保留到进程退出的监听注册。
    _registration: LiveWatchRegistration,
    /// Background event-consumer task.
    /// 后台事件消费任务。
    _task: ServiceTaskHandle<()>,
}

/// Starts the unique process-wide Agent Skills catalog watcher.
/// 启动唯一的进程级 Agent Skills 目录监听器。
///
/// # Parameters
/// 参数
/// - `state`: Shared service state updated after every stable event batch.
/// - `state`：每批稳定事件后更新的共享服务状态。
/// - `event_emitter`: WebUI event sink receiving the refreshed workbench.
/// - `event_emitter`：接收刷新后工作台的 WebUI 事件接收器。
/// - `system_root`: Exact configured host-managed package root.
/// - `system_root`：精确配置的宿主管理技能包根目录。
/// - `user_root`: Exact configured user package and host-catalog root.
/// - `user_root`：精确配置的用户技能包与宿主目录根目录。
///
/// # Returns
/// 返回值
/// Success after the watcher and consumer task are retained for the process lifetime.
/// 在监听器与消费任务被保留到进程生命周期后返回成功。
pub(crate) fn start_agent_skills_catalog_watcher(
    state: SharedServiceBridgeState,
    event_emitter: ServiceEventEmitter,
    system_root: PathBuf,
    user_root: PathBuf,
) -> Result<(), String> {
    if AGENT_SKILLS_WATCHER.get().is_some() {
        return Err(String::from(
            "Agent Skills catalog watcher is already initialized.",
        ));
    }
    // watcher is the sole notify backend shared by this catalog subscriber.
    // watcher 是当前目录订阅者共享的唯一 notify 后端。
    let watcher =
        Arc::new(LiveFileWatcher::new().map_err(|error| {
            format!("failed to initialize Agent Skills catalog watcher: {error}")
        })?);
    // subscriber and receiver form the exact catalog event stream.
    // subscriber 与 receiver 组成精确目录事件流。
    let (subscriber, receiver) = watcher.add_subscriber();
    // registration watches both independent source roots recursively.
    // registration 递归监听两个独立来源根目录。
    let registration = subscriber.register_paths(vec![
        vulcan_file_watcher::live::LiveWatchPath {
            path: system_root,
            recursive: true,
        },
        vulcan_file_watcher::live::LiveWatchPath {
            path: user_root,
            recursive: true,
        },
    ]);
    // throttled_receiver coalesces editor save bursts into one deterministic full rescan.
    // throttled_receiver 将编辑器保存突发事件合并为一次确定性完整重扫。
    let mut throttled_receiver =
        ThrottledLiveWatchReceiver::new(receiver, Duration::from_millis(250));
    // task owns the long-running rescan and WebUI event loop.
    // task 持有长生命周期重扫与 WebUI 事件循环。
    let task = spawn_service_task(async move {
        while let Some(event) = throttled_receiver.recv().await {
            if !agent_skills_watch_event_requires_refresh(&event.paths) {
                continue;
            }
            // catalog_guard prevents external file events from observing a host-managed package transaction mid-commit.
            // catalog_guard 防止外部文件事件在宿主管理的技能包事务提交中途观察到中间状态。
            let _catalog_guard = lock_agent_skills_catalog_mutation().await;
            match refresh_agent_skills_local_state_if_changed(&state).await {
                Ok(Some(update)) => emit_agent_skills_catalog_delta(&event_emitter, update),
                Ok(None) => {}
                Err(error) => {
                    record_agent_skills_watcher_error(&state, &event_emitter, error);
                }
            }
        }
    });
    AGENT_SKILLS_WATCHER
        .set(AgentSkillsWatcherHandle {
            _watcher: watcher,
            _subscriber: subscriber,
            _registration: registration,
            _task: task,
        })
        .map_err(|_| String::from("Agent Skills catalog watcher initialization raced."))
}

/// Records and emits one explicit watcher refresh failure in the Agent Skills diagnostics domain.
/// 在 Agent Skills 诊断领域记录并发出一次明确的监听刷新失败。
fn record_agent_skills_watcher_error(
    state: &SharedServiceBridgeState,
    event_emitter: &ServiceEventEmitter,
    error: String,
) {
    // update is emitted only after the diagnostic revision commits under the state lock.
    // update 仅在诊断版本于状态锁内提交后发出。
    let update = state.lock().ok().and_then(|mut guard| {
        let issue_path = String::from("Agent Skills filesystem watcher");
        if guard
            .workbench
            .agent_skills
            .errors
            .iter()
            .any(|issue| issue.path == issue_path && issue.message == error)
        {
            return None;
        }
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        guard
            .workbench
            .agent_skills
            .errors
            .push(AgentSkillLoadIssueSummary {
                path: issue_path,
                message: error,
            });
        // chinese resolves the current UI locale for the watcher failure notice.
        // chinese 解析监听失败摘要使用的当前界面语言。
        let chinese = guard
            .workbench
            .ui_locale
            .as_deref()
            .unwrap_or("zh-CN")
            .starts_with("zh");
        guard.workbench.agent_skills.notice = if chinese {
            String::from("提示词技能自动同步失败；请查看目录问题并修复后重新保存文件。")
        } else {
            String::from(
                "Prompt Skills automatic synchronization failed. Review the catalog issues and save the corrected files again.",
            )
        };
        Some(AgentSkillsCatalogUpdate {
            revision: guard.workbench_generation,
            agent_skills: guard.workbench.agent_skills.clone(),
        })
    });
    if let Some(update) = update {
        emit_agent_skills_catalog_delta(event_emitter, update);
    }
}

/// Returns whether one coalesced event contains a catalog-visible path.
/// 返回一批合并事件是否包含目录可见路径。
fn agent_skills_watch_event_requires_refresh(paths: &[PathBuf]) -> bool {
    paths.iter().any(|path| {
        // components stores exact path segments used to identify private transaction churn.
        // components 保存用于识别私有事务抖动的精确路径片段。
        let components = path
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>();
        !components.windows(2).any(|window| {
            window[0] == vulcan_prompt_skills::VULCAN_SKILL_CATALOG_DIR_NAME
                && window[1] == "transactions"
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies private transaction churn is ignored while committed package paths refresh.
    /// 验证私有事务抖动会被忽略，而已提交技能包路径会触发刷新。
    #[test]
    fn agent_skills_watch_event_filters_private_transactions() {
        assert!(!agent_skills_watch_event_requires_refresh(&[
            PathBuf::from("/skills/.vulcan/transactions/1/package/SKILL.md",)
        ]));
        assert!(agent_skills_watch_event_requires_refresh(&[PathBuf::from(
            "/skills/reviewer/SKILL.md",
        )]));
    }
}
