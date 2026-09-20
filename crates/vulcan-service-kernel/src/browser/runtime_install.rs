//! Explicit Core browser installation with queryable, process-owned progress.
//! 显式 Core 浏览器安装及可查询的进程级进度。

use super::{BrowserHostRuntimeConfig, BrowserManager};
use serde::Serialize;
use std::sync::Arc;

/// Host-supplied installation operation; returns a verified runtime or an actionable error.
/// 宿主提供的安装操作；返回已验证的运行时或可操作的错误。
pub type BrowserRuntimeInstaller =
    Arc<dyn Fn() -> Result<BrowserHostRuntimeConfig, String> + Send + Sync>;

/// Last installation attempt, retained after completion for reconnecting clients.
/// 最后一次安装尝试，结束后仍保留供重连客户端查询。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeInstallTask {
    /// Stable attempt identity.
    /// 稳定的尝试标识。
    pub task_id: String,
    /// Running, succeeded or failed.
    /// 运行中、成功或失败。
    pub state: String,
    /// Exact bounded failure detail, absent on success.
    /// 精确且有界的失败详情，成功时为空。
    pub error: Option<String>,
}

/// Runtime health and available recovery action, independent of plugin dependency state.
/// 运行时健康与可用恢复操作，独立于插件依赖状态。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeStatus {
    /// Whether all configured host files exist.
    /// 配置的全部宿主文件是否存在。
    pub ready: bool,
    /// Whether this Core host registered an installer.
    /// 此 Core 宿主是否注册了安装器。
    pub can_install: bool,
    /// Diagnostic for a missing or damaged runtime.
    /// 缺失或损坏运行时的诊断。
    pub diagnostic: Option<String>,
    /// Latest process-owned installation attempt.
    /// 最近的进程持有安装尝试。
    pub installation: Option<BrowserRuntimeInstallTask>,
}

/// Shared maintenance state owned by one browser manager.
/// 单个浏览器管理器持有的共享维护状态。
#[derive(Default)]
pub(super) struct BrowserRuntimeMaintenance {
    /// Explicit host capability; no guessed installer or download path.
    /// 显式宿主能力；不猜测安装器或下载路径。
    installer: Option<BrowserRuntimeInstaller>,
    /// Read-only resolver discovers completed installation after a Core restart.
    /// 只读解析器在 Core 重启后发现已完成的安装。
    inspector: Option<BrowserRuntimeInstaller>,
    /// Startup failure retained for the recovery UI.
    /// 为恢复界面保留的启动失败信息。
    diagnostic: Option<String>,
    /// One deduplicated attempt across all clients.
    /// 全部客户端之间去重的单次尝试。
    task: Option<BrowserRuntimeInstallTask>,
}

impl BrowserManager {
    /// Registers the host installer and startup diagnostic; returns after updating the shared state.
    /// 注册宿主安装器与启动诊断；更新共享状态后返回。
    pub fn configure_runtime_installation(
        &self,
        installer: Option<BrowserRuntimeInstaller>,
        inspector: Option<BrowserRuntimeInstaller>,
        diagnostic: Option<String>,
    ) -> Result<(), String> {
        let mut maintenance = self
            .runtime_maintenance
            .lock()
            .map_err(|_| "browser runtime maintenance lock poisoned")?;
        maintenance.installer = installer;
        maintenance.inspector = inspector;
        maintenance.diagnostic = diagnostic;
        Ok(())
    }

    /// Reads live file availability and the latest install task without starting any work.
    /// 读取实时文件可用性及最近安装任务，不启动任何操作。
    pub fn runtime_status(&self) -> Result<BrowserRuntimeStatus, String> {
        // Inspect outside the maintenance lock; filesystem recovery must not block task publication.
        // 在维护锁之外检查；文件系统恢复不得阻塞任务状态发布。
        let inspector = self.runtime_maintenance.lock().map_err(|_| "browser runtime maintenance lock poisoned")?.inspector.clone();
        if let Some(inspector) = inspector {
            match inspector() {
                Ok(config) => self.configure_host_runtime(config)?,
                Err(error) => {
                    self.runtime_maintenance.lock().map_err(|_| "browser runtime maintenance lock poisoned")?.diagnostic = Some(error);
                    // Invalidate stale paths when the runtime package changed or was removed.
                    // 运行包发生变化或被移除时，使旧路径失效。
                    *self.host_config.lock().map_err(|_| "browser host configuration lock poisoned")? = None;
                }
            }
        }
        let ready = self.host_runtime_config()?.is_some_and(|config| {
            config.executable.is_file()
                && config.entry.is_file()
                && config.chromium_executable.is_file()
        });
        let maintenance = self
            .runtime_maintenance
            .lock()
            .map_err(|_| "browser runtime maintenance lock poisoned")?;
        Ok(BrowserRuntimeStatus {
            ready,
            can_install: maintenance.installer.is_some(),
            diagnostic: if ready {
                None
            } else {
                Some(maintenance.diagnostic.clone().unwrap_or_else(|| "Core browser runtime is missing or damaged. Install or repair the browser component.".into()))
            },
            installation: maintenance.task.clone(),
        })
    }

    /// Starts or joins an explicit install attempt; returns immediately with its stable task ID.
    /// 启动或加入显式安装尝试；立即返回稳定任务标识。
    pub fn start_runtime_installation(&self) -> Result<BrowserRuntimeInstallTask, String> {
        let mut maintenance = self
            .runtime_maintenance
            .lock()
            .map_err(|_| "browser runtime maintenance lock poisoned")?;
        if let Some(task) = &maintenance.task {
            if task.state == "running" {
                return Ok(task.clone());
            }
        }
        let installer = maintenance.installer.clone().ok_or("This Core distribution has no browser installer. Rebuild or update the desktop runtime package.")?;
        let task = BrowserRuntimeInstallTask {
            task_id: uuid::Uuid::new_v4().to_string(),
            state: "running".into(),
            error: None,
        };
        maintenance.task = Some(task.clone());
        let manager = self.clone();
        let worker = std::thread::Builder::new().name("vulcan-browser-install".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| installer()))
                .unwrap_or_else(|_| Err("Browser installer panicked.".into()))
                .and_then(|config| {
                    if !config.executable.is_file() || !config.entry.is_file() || !config.chromium_executable.is_file() {
                        return Err("Browser installation completed without all required runtime files.".into());
                    }
                    manager.configure_host_runtime(config)
                });
            if let Ok(mut state) = manager.runtime_maintenance.lock() {
                let error = result.err().map(|message| message.chars().take(8192).collect::<String>());
                state.diagnostic = error.clone();
                if let Some(task) = &mut state.task {
                    task.state = if error.is_some() { "failed" } else { "succeeded" }.into();
                    task.error = error;
                }
            }
        });
        if let Err(error) = worker {
            let message = format!("Cannot start browser installer: {error}");
            maintenance.task = Some(BrowserRuntimeInstallTask {
                state: "failed".into(),
                error: Some(message.clone()),
                ..task
            });
            return Err(message);
        }
        Ok(task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Barrier,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};

    /// Waits for one background attempt to finish within a bounded test deadline.
    /// 在有界测试期限内等待一次后台尝试结束。
    fn finished(manager: &BrowserManager) -> BrowserRuntimeStatus {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = manager.runtime_status().unwrap();
            if status
                .installation
                .as_ref()
                .is_some_and(|task| task.state != "running")
            {
                return status;
            }
            assert!(Instant::now() < deadline, "installer did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn browser_runtime_installation_deduplicates_recovers_and_publishes_config() {
        let root = tempfile::tempdir().unwrap();
        for name in ["host", "entry", "chromium"] {
            std::fs::write(root.path().join(name), "fixture").unwrap();
        }
        let config = BrowserHostRuntimeConfig {
            executable: root.path().join("host"),
            entry: root.path().join("entry"),
            chromium_executable: root.path().join("chromium"),
            profiles_root: root.path().join("profiles"),
            staging_root: root.path().join("staging"),
            discovery_timeout: Duration::from_secs(1),
            request_timeout: Duration::from_secs(1),
        };
        let manager = BrowserManager::new();
        let gate = Arc::new(Barrier::new(2));
        let attempts = Arc::new(AtomicUsize::new(0));
        let worker_gate = gate.clone();
        let worker_attempts = attempts.clone();
        manager
            .configure_runtime_installation(
                Some(Arc::new(move || {
                    if worker_attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                        worker_gate.wait();
                        return Err("Download failed: fixture network unavailable".into());
                    }
                    Ok(config.clone())
                })),
                None,
                Some("Chromium is missing".into()),
            )
            .unwrap();
        assert!(!manager.runtime_status().unwrap().ready);
        let first = manager.start_runtime_installation().unwrap();
        assert_eq!(
            first.task_id,
            manager.start_runtime_installation().unwrap().task_id
        );
        gate.wait();
        let failed = finished(&manager);
        assert_eq!(failed.installation.unwrap().state, "failed");
        assert!(failed.diagnostic.unwrap().contains("network unavailable"));
        let retry = manager.start_runtime_installation().unwrap();
        assert_ne!(first.task_id, retry.task_id);
        let success = finished(&manager);
        assert!(success.ready);
        assert!(success.diagnostic.is_none());
        assert_eq!(success.installation.unwrap().state, "succeeded");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        std::fs::remove_file(root.path().join("chromium")).unwrap();
        assert!(!manager.runtime_status().unwrap().ready);
    }
}
