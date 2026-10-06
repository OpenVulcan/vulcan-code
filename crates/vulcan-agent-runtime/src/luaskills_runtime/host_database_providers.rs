//! Publication of the host's in-process database providers for LuaSkills.
//! 为 LuaSkills 发布宿主进程内数据库 provider。
//!
//! LuaSkills chooses its database path through `LuaRuntimeDatabaseProviderMode`. The mode that forwards every
//! statement to an external database controller process is not used here, while `HostCallback` calls a callback
//! registered inside this process. **This application uses `HostCallback` only**: the storage engine is in
//! process, and no controller process is shipped, configured or started any more. This module owns that choice:
//! [`install`] starts the execution channel, registers both provider callbacks and records the fact
//! process-wide, and [`provider_mode`] is what an entry point consults while it builds host options.
//! LuaSkills 通过 `LuaRuntimeDatabaseProviderMode` 选择其数据库路径。把每条语句转发给外部数据库控制器进程的模式
//! 在此不被使用，而 `HostCallback` 调用注册在本进程内的回调。**本应用只使用 `HostCallback`**：存储引擎在进程内，
//! 且不再随附、配置或启动任何控制器进程。本模块持有该选择：`install` 启动执行通道、注册两个 provider 回调并在
//! 进程级记录该事实，`provider_mode` 是入口在构建宿主选项时所查询的东西。
//!
//! # Registration order / 注册顺序
//!
//! The engine refuses to start when `HostCallback` is selected and no callback is registered, so the mode and
//! the callbacks must agree. [`provider_mode`] answers `HostCallback` unconditionally and never varies, which is
//! why an entry point can call it immediately before it creates its engine without a second ordering rule to
//! remember. When [`install`] has not run, the engine's own refusal names the missing callback — a loud failure
//! that is strictly preferable to silently selecting a mode this application cannot serve.
//! 当选择了 `HostCallback` 而未注册回调时，引擎拒绝启动，因此模式与回调必须一致。`provider_mode` 无条件回答
//! `HostCallback` 且从不变化，这正是入口可以在创建引擎之前紧邻调用它而无需记住第二条顺序规则的原因。当
//! `install` 尚未运行时，引擎自身的拒绝会指名缺失的回调——这是一次响亮的失败，远优于静默选择一条本应用无法
//! 承载的模式。

use std::sync::{Arc, Mutex, OnceLock};

use vulcan_workspace_store::local_database_runtime::LocalDatabaseRuntime;

use super::LuaRuntimeDatabaseProviderMode;
use super::database_bridge::{HostExecutionChannel, HostExecutionChannelSnapshot};
use super::database_callbacks::{SkillLanceDbProvider, SkillSqliteProvider};

/// One installed pair of providers together with the channel that runs their work.
/// 已安装的一对 provider，以及运行其工作的通道。
pub struct InstalledHostDatabaseProviders {
    channel: Arc<HostExecutionChannel<Arc<LocalDatabaseRuntime>>>,
}

impl InstalledHostDatabaseProviders {
    /// Reports whether the execution channel still accepts work.
    /// 报告执行通道是否仍接受工作。
    ///
    /// # Parameters / 参数
    /// This method takes no parameters.
    /// 本方法不接收参数。
    ///
    /// # Returns / 返回值
    /// Returns `true` until the last owner of this installation is dropped.
    /// 在本次安装的最后一个所有者被丢弃前返回 `true`。
    pub fn is_open(&self) -> bool {
        !self.channel.is_closed()
    }

    /// Return exact database channel counters for diagnostics without accessing stored data.
    /// 返回数据库通道的精确诊断计数，不访问已存数据。
    ///
    /// Takes no parameters and returns the current monotonic channel snapshot.
    /// 不接收参数，返回当前单调的通道快照。
    pub fn diagnostics_snapshot(&self) -> HostExecutionChannelSnapshot {
        self.channel.diagnostics_snapshot()
    }
}

impl std::fmt::Debug for InstalledHostDatabaseProviders {
    /// Renders the installation without touching the channel's internals.
    /// 在不触碰通道内部的情况下渲染该安装。
    ///
    /// The channel itself is not `Debug`, and a caller that only has a `Result` needs the installation to be.
    /// The rendered form therefore reports the one property that matters to a reader: whether it is still open.
    /// 通道自身不是 `Debug`，而只拿到 `Result` 的调用方需要该安装是 `Debug`。因此渲染形式只报告对读者重要的那一个
    /// 属性：它是否仍然打开。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InstalledHostDatabaseProviders")
            .field("open", &self.is_open())
            .finish()
    }
}

impl Drop for InstalledHostDatabaseProviders {
    /// Clears both callbacks and closes the channel.
    /// 清空两个回调并关闭通道。
    fn drop(&mut self) {
        // A removed installation may remain alive in a bootstrap/service owner while a later generation has
        // already occupied the process slot. Only an installation with no active successor may clear the global
        // callbacks; otherwise this stale drop would silently disable the successor.
        // 已移除的安装可能仍由引导/服务所有者保持存活，而后一代安装已经占据进程槽位。只有不存在活动后继者
        // 的安装才可清空全局回调；否则这次过期销毁会静默禁用后继者。
        let may_clear_callbacks = slot()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none();
        if may_clear_callbacks {
            // Clear first so no new callback work can enter while the execution channel is closing.
            // 先清空回调，避免执行通道关闭期间仍有新的回调工作进入。
            luaskills::set_sqlite_provider_callback(None);
            luaskills::set_lancedb_provider_callback(None);
        }
        // The registered callbacks were this channel's other owners, so after clearing them this handle is
        // normally unique and the channel closes. When some other holder remains, the channel stays open
        // rather than cancelling work that holder may still be awaiting.
        // 已注册的回调曾是该通道的其他所有者，因此清空它们之后本句柄通常已唯一，通道随之关闭。若仍有其他持有者，
        // 通道保持打开，而不是取消该持有者可能仍在等待的工作。
        if let Some(channel) = Arc::get_mut(&mut self.channel) {
            channel.close();
        }
    }
}

/// Process-wide slot holding the installation, if one exists.
/// 持有安装（若存在）的进程级槽位。
static INSTALLED: OnceLock<Mutex<Option<Arc<InstalledHostDatabaseProviders>>>> = OnceLock::new();

/// Returns the process-wide slot, creating it on first use.
/// 返回进程级槽位，首次使用时创建它。
///
/// # Parameters / 参数
/// This function takes no parameters.
/// 本函数不接收参数。
///
/// # Returns / 返回值
/// Returns a reference to the slot for the lifetime of the process.
/// 返回该槽位的引用，其生命周期为整个进程。
fn slot() -> &'static Mutex<Option<Arc<InstalledHostDatabaseProviders>>> {
    INSTALLED.get_or_init(|| Mutex::new(None))
}

/// Installs both provider callbacks over one shared runtime.
/// 在一个共享运行时之上安装两个 provider 回调。
///
/// # Parameters / 参数
/// `runtime` is the in-process database runtime that owns every database the host serves, and `handle` is the
/// Tokio runtime the channel submits its blocking work to. The caller must keep that runtime alive: a handle
/// does not keep its runtime alive, and dropping the runtime cancels work the channel already queued.
/// `runtime` 是拥有宿主所服务每个数据库的进程内数据库运行时，`handle` 是通道向其提交阻塞工作的 Tokio 运行时。
/// 调用方必须保持该运行时存活：句柄不会延长其运行时的寿命，而丢弃运行时会取消通道已排队的工作。
///
/// # Returns / 返回值
/// Returns the installation, or an explicit error when one is already installed. Installing twice is refused
/// rather than replacing, because a silent replacement would overwrite callbacks that still own the previous
/// channel, leaving that channel unreachable with its workers still running.
/// 返回该安装；当已存在安装时返回明确错误。重复安装被拒绝而非替换，因为静默替换会覆盖仍拥有先前通道的回调，使该
/// 通道不可达而其工作线程仍在运行。
pub fn install(
    runtime: Arc<LocalDatabaseRuntime>,
    handle: tokio::runtime::Handle,
) -> Result<Arc<InstalledHostDatabaseProviders>, String> {
    // A poisoned slot is recovered rather than refused. The guarded value is a single `Option` assignment, so a
    // panic while the lock was held cannot leave a half-updated value behind, and refusing instead would leave a
    // process that had registered its callbacks permanently unable to install — `provider_mode` would then keep
    // misreporting the mode for a process whose callbacks exist. This also matches the recovery convention
    // used throughout `local_database_runtime`.
    // 毒化的槽位被恢复而非拒绝。受保护的值是一次 `Option` 赋值，因此持锁期间的崩溃不可能留下半更新值；而拒绝会让
    // 一个已注册回调的进程永久无法安装——`provider_mode` 随后会为一个回调确实存在的进程持续报告
    // 误报模式。这也与 `local_database_runtime` 全篇采用的恢复惯例一致。
    let mut guard = slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if guard.is_some() {
        return Err(String::from(
            "the LuaSkills host database providers are already installed; shut them down before replacing them",
        ));
    }
    let channel = Arc::new(HostExecutionChannel::start(handle, runtime));
    let sqlite = Arc::new(SkillSqliteProvider::new(Arc::clone(&channel)));
    let lancedb = Arc::new(SkillLanceDbProvider::new(Arc::clone(&channel)));
    // Both callbacks are registered before the installation is published, so `provider_mode` can never report
    // `HostCallback` for a process whose callbacks are missing.
    // 两个回调在安装被发布之前完成注册，因此 `provider_mode` 绝不会为缺少回调的进程报告 `HostCallback`。
    luaskills::set_sqlite_provider_callback(Some(sqlite.callback()));
    luaskills::set_lancedb_provider_callback(Some(lancedb.callback()));
    let installed = Arc::new(InstalledHostDatabaseProviders { channel });
    *guard = Some(Arc::clone(&installed));
    Ok(installed)
}

/// Returns the current installation, if any.
/// 返回当前的安装（若有）。
///
/// # Parameters / 参数
/// This function takes no parameters.
/// 本函数不接收参数。
///
/// # Returns / 返回值
/// Returns the installation, or `None` when none is installed.
/// 返回该安装；未安装时返回 `None`。
pub fn installed() -> Option<Arc<InstalledHostDatabaseProviders>> {
    slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// Reports whether both provider callbacks are installed.
/// 报告两个 provider 回调是否已安装。
///
/// # Parameters / 参数
/// This function takes no parameters.
/// 本函数不接收参数。
///
/// # Returns / 返回值
/// Returns `true` while an installation is registered.
/// 在安装已注册期间返回 `true`。
pub fn is_installed() -> bool {
    installed().is_some()
}

/// Returns the provider mode an entry point must select for its engine.
/// 返回入口必须为其引擎选择的 provider 模式。
///
/// # Parameters / 参数
/// This function takes no parameters.
/// 本函数不接收参数。
///
/// # Returns / 返回值
/// Always returns `HostCallback`. Vulcan Code's storage engine is in process, so this is the only mode the
/// application supports; the LuaSkills mode that forwards every statement to an external controller
/// process is one this application no longer ships, configures or starts. Answering `HostCallback` unconditionally is
/// deliberate: when the providers really are missing, LuaSkills refuses to build the engine and names the
/// missing callback, which is a loud failure. Answering the external-controller mode would silently select a mode
/// that cannot work, which is exactly the kind of fallback this migration removes.
/// 始终返回 `HostCallback`。Vulcan Code 的存储引擎在进程内，这是应用唯一支持的模式；LuaSkills 的
/// 把每条语句转发给外部控制器进程的那个模式，本应用已不再随附、不再配置、也不再启动。
/// 无条件回答 `HostCallback` 是刻意的：当 provider 确实缺失时，LuaSkills 会拒绝构建引擎并指名缺失的回调，
/// 这是一次响亮的失败；若改答外部控制器模式，则会静默选择一条无法工作的模式，而那正是本次迁移要移除的
/// 那类回退。
pub fn provider_mode() -> LuaRuntimeDatabaseProviderMode {
    LuaRuntimeDatabaseProviderMode::HostCallback
}

/// Removes the installation from the process-wide slot.
/// 从进程级槽位移除该安装。
///
/// # Parameters / 参数
/// This function takes no parameters.
/// 本函数不接收参数。
///
/// # Returns / 返回值
/// Returns `true` when an installation was registered. The callbacks clear and the channel closes when the
/// last handle is dropped, which is deliberate: shutdown must not cancel work some other holder is still
/// awaiting.
/// 已注册安装时返回 `true`。回调的清空与通道的关闭发生在最后一个句柄被丢弃时，这是刻意的：关闭不得取消其他持有者
/// 仍在等待的工作。
pub fn shutdown() -> bool {
    let taken = slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    match taken {
        Some(installed) => {
            drop(installed);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
    use std::collections::HashMap;

    /// Serializes the tests in this module, because the installation is process-wide.
    /// 串行化本模块的测试，因为该安装是进程级的。
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Builds a runtime and a handle for one test.
    /// 为一个测试构建运行时与句柄。
    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime")
    }

    /// Installs the providers, or fails the test with the installation error.
    /// 安装 provider，或以安装错误使测试失败。
    ///
    /// The installation is not `Debug`, so `expect` cannot be used on the result; reporting the error text is
    /// also more useful than a value dump would be.
    /// 安装不是 `Debug`，因此无法对结果使用 `expect`；报告错误文本也比转储取值更有用。
    fn install_or_panic(
        runtime: Arc<LocalDatabaseRuntime>,
        handle: tokio::runtime::Handle,
    ) -> Arc<InstalledHostDatabaseProviders> {
        match install(runtime, handle) {
            Ok(installed) => installed,
            Err(error) => panic!("install the host database providers: {error}"),
        }
    }

    /// Builds an empty in-process database runtime for one test.
    /// 为一个测试构建空的进程内数据库运行时。
    fn database_runtime() -> Arc<LocalDatabaseRuntime> {
        Arc::new(LocalDatabaseRuntime::open(HashMap::new()).expect("open in-process runtime"))
    }

    /// Verifies the mode is `HostCallback` whether or not an installation is present.
    /// 验证无论是否存在安装，模式都是 `HostCallback`。
    ///
    /// This is the ordering invariant entry points rely on: an entry point may consult the mode immediately
    /// before it creates its engine without remembering a second rule. The mode must never answer
    /// the external-controller mode, because that mode talks to a controller process this application does not
    /// ship, configure or start.
    /// 这是入口所依赖的顺序不变量：入口可以在创建引擎之前紧邻查询模式，而无需记住第二条规则。模式绝不能回答
    /// 外部控制器模式，因为该模式与一个本应用不随附、不配置也不启动的控制器进程通信。
    #[test]
    fn the_mode_is_always_host_callback() {
        let _guard = TEST_LOCK.lock().expect("test lock");
        let _ = shutdown();
        assert!(!is_installed());
        assert_eq!(
            provider_mode(),
            LuaRuntimeDatabaseProviderMode::HostCallback
        );

        let runtime = runtime();
        let installed = install_or_panic(database_runtime(), runtime.handle().clone());
        assert!(is_installed());
        assert_eq!(
            provider_mode(),
            LuaRuntimeDatabaseProviderMode::HostCallback
        );
        assert!(installed.is_open());

        assert!(
            shutdown(),
            "shutdown must report that it removed an installation"
        );
        assert!(!is_installed());
        assert_eq!(
            provider_mode(),
            LuaRuntimeDatabaseProviderMode::HostCallback
        );
        // The test still holds the handle, so the channel is deliberately still open until it goes away.
        // 测试仍持有该句柄，因此通道刻意保持打开直到句柄消失。
        assert!(installed.is_open());
        drop(installed);
    }

    /// Verifies a second install is refused rather than silently replacing the first.
    /// 验证第二次安装被拒绝，而不是静默替换第一次。
    #[test]
    fn a_second_install_is_refused() {
        let _guard = TEST_LOCK.lock().expect("test lock");
        let _ = shutdown();
        let runtime = runtime();
        let first = install_or_panic(database_runtime(), runtime.handle().clone());
        let error = install(database_runtime(), runtime.handle().clone())
            .expect_err("a second install must be refused");
        assert!(
            error.contains("already installed"),
            "unexpected error: {error}"
        );
        assert!(
            is_installed(),
            "the first installation must survive the refusal"
        );
        drop(first);
        let _ = shutdown();
        assert!(!is_installed());
    }

    /// Verifies dropping a removed old generation cannot clear a newly installed generation's callbacks.
    /// 验证销毁已移除的旧代安装不能清空新安装代次的回调。
    #[test]
    fn stale_installation_drop_preserves_successor_callbacks() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = shutdown();
        let runtime = runtime();
        let old = install_or_panic(database_runtime(), runtime.handle().clone());
        assert!(shutdown(), "old generation must leave the process slot");

        let successor = install_or_panic(database_runtime(), runtime.handle().clone());
        drop(old);

        let host_options = LuaRuntimeHostOptions {
            sqlite_provider_mode: provider_mode(),
            lancedb_provider_mode: provider_mode(),
            ..LuaRuntimeHostOptions::default()
        };
        LuaEngine::new(LuaEngineOptions::new(
            LuaVmPoolConfig {
                min_size: 1,
                max_size: 1,
                idle_ttl_secs: 30,
            },
            host_options,
        ))
        .expect("successor callbacks must remain registered after stale owner drop");

        assert!(successor.is_open());
        assert!(shutdown());
        drop(successor);
    }

    /// Verifies the last owner closing the installation leaves the process back on the controller path.
    /// 验证最后一个所有者关闭安装后，进程回到控制器路径。
    #[test]
    fn dropping_the_last_owner_clears_the_installation() {
        let _guard = TEST_LOCK.lock().expect("test lock");
        let _ = shutdown();
        let runtime = runtime();
        let installed = install_or_panic(database_runtime(), runtime.handle().clone());
        assert_eq!(
            provider_mode(),
            LuaRuntimeDatabaseProviderMode::HostCallback
        );

        let slot_handle = shutdown();
        assert!(slot_handle);
        // The slot is empty even while this test still holds the handle.
        // 即使本测试仍持有句柄，槽位也已为空。
        assert!(!is_installed());
        // Dropping the last owner closes the channel and clears the callbacks.
        // 丢弃最后一个所有者会关闭通道并清空回调。
        drop(installed);
        assert!(!is_installed());
        // The mode stays `HostCallback` regardless of the slot: this application has no other storage mode, so
        // an empty slot must surface as LuaSkills refusing to build an engine rather than as a silent switch.
        // 无论槽位如何，模式都保持 `HostCallback`：本应用没有其他存储模式，因此空槽位必须以 LuaSkills 拒绝
        // 构建引擎的方式暴露，而不是变成一次静默切换。
        assert_eq!(
            provider_mode(),
            LuaRuntimeDatabaseProviderMode::HostCallback
        );
    }
    /// Verifies a poisoned slot still reports the installation that exists.
    /// 验证被毒化的槽位仍报告确实存在的安装。
    ///
    /// The slot lock is poisoned deliberately, by panicking while holding it. Treating that poison as "nothing is
    /// installed" would misreport the mode for a process whose callbacks are registered, and refusing to
    /// install would leave such a process permanently unable to repair itself. The recovery must therefore keep
    /// reading the value that is really there, which is what this test pins.
    /// 槽位锁被刻意毒化：持锁时触发 panic。把该毒化当作"未安装"会为一个回调已注册的进程误报模式，
    /// 而拒绝安装会让这样的进程永久无法自我修复。因此恢复必须继续读取确实存在的值，这正是本测试所钉定的。
    #[test]
    fn a_poisoned_slot_still_reports_the_installation() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = shutdown();

        let runtime = runtime();
        let _installed = install_or_panic(database_runtime(), runtime.handle().clone());

        let poisoned = std::panic::catch_unwind(|| {
            let _held = slot().lock().expect("slot lock before poisoning");
            panic!("poison the provider slot on purpose");
        });
        assert!(
            poisoned.is_err(),
            "the poisoning helper must actually panic"
        );

        assert!(
            installed().is_some(),
            "a poisoned slot must still report the installation that is really there"
        );
        assert!(is_installed());
        assert_eq!(
            provider_mode(),
            LuaRuntimeDatabaseProviderMode::HostCallback
        );
        assert!(
            shutdown(),
            "a poisoned slot must still remove its installation"
        );
        assert!(!is_installed());
    }
}
