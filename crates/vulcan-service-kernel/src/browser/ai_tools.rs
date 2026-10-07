use super::{
    BrowserAutomationState, BrowserLeaseState, BrowserManager, BrowserProfileMode,
    BrowserSessionProjection, BrowserSessionStatus,
};
use crate::state::SharedServiceBridgeState;
use std::collections::BTreeSet;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use uuid::Uuid;
use vulcan_agent_runtime::mcp::{
    McpRuntimeAccess, McpRuntimeToolAllowlist, McpRuntimeToolBinding,
    plugin_conversation_mcp_server_config,
};
use vulcan_agent_runtime::model::SessionId;
use vulcan_agent_runtime::tooling::native::{NativeBrowserRuntime, NativeBrowserSnapshot};
use vulcan_plugin_runtime::VulcanPluginRuntimeManager;

/// Stable identity of the plugin that enables direct AI browser tools.
/// 启用 AI 直接浏览器工具的插件稳定身份。
const BROWSER_PLUGIN_ID: &str = "vulcan-browser";

/// Exact plugin-local MCP server providing the approved Playwright page tools.
/// 提供已批准 Playwright 页面工具的精确插件内 MCP 服务。
const PLAYWRIGHT_SERVER_ID: &str = "playwright";

/// Unreachable local endpoint used only to discover fixed-version MCP schemas before a browser opens.
/// 在打开浏览器前仅用于发现固定版本 MCP Schema 的不可达本地端点。
const DISCOVERY_ENDPOINT: &str = "ws://127.0.0.1:1/devtools/browser/unavailable";

/// Lifetime of an AI control lease, renewed by its owner MCP supervisor.
/// AI 控制租约的有效时长，由其所有者 MCP 监督任务续订。
const AI_LEASE_TTL: Duration = Duration::from_secs(600);

/// Interval for lease renewal and closed-host detection.
/// 续订租约与检测宿主关闭的间隔。
const AI_LEASE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Approved Playwright MCP tools exposed directly to the model.
/// 直接暴露给模型的已批准 Playwright MCP 工具。
const APPROVED_PLAYWRIGHT_TOOLS: &[&str] = &[
    "browser_navigate",
    "browser_navigate_back",
    "browser_snapshot",
    "browser_take_screenshot",
    "browser_click",
    "browser_type",
    "browser_press_key",
    "browser_hover",
    "browser_select_option",
    "browser_fill_form",
    "browser_wait_for",
    "browser_tabs",
    "browser_handle_dialog",
    // File upload accepts host paths; expose it only after Core-owned staging can bind paths to
    // the same conversation instead of letting model arguments choose plugin-private files.
    // 文件上传接受宿主路径；须等 Core 暂存把路径绑定到同一会话后才开放，避免模型参数任选插件私有文件。
    "browser_console_messages",
    "browser_network_requests",
    "browser_resize",
];

/// Current owner-scoped MCP partition and optional live browser authorization.
/// 当前所有者范围 MCP 分区及可选的活动浏览器授权。
pub(super) struct BrowserAiConnection {
    /// Plugin generation used as part of the owner partition key.
    /// 用作所有者分区键一部分的插件代次。
    plugin_generation: u64,
    /// MCP access object sharing the exact owner partition registry.
    /// 共享精确所有者分区注册表的 MCP 访问对象。
    mcp_runtime: McpRuntimeAccess,
    /// Core browser identity, absent while this is only a schema-discovery template.
    /// Core 浏览器身份；仅用于 Schema 发现的模板阶段为空。
    browser_id: Option<String>,
    /// Exclusive Core control credential, absent for the template.
    /// Core 独占控制凭据；模板阶段为空。
    lease_secret: Option<String>,
    /// Host gateway connection identity, absent for the template.
    /// 宿主网关连接身份；模板阶段为空。
    connection_id: Option<String>,
    /// Supervisor that keeps a live Core lease aligned with the MCP process lifetime.
    /// 使活动 Core 租约与 MCP 进程寿命保持一致的监督任务。
    heartbeat: Option<tokio::task::JoinHandle<()>>,
    /// Whether MCP startup and the host gateway are confirmed usable.
    /// MCP 启动与宿主网关是否已确认可用。
    ready: bool,
}

/// Removes an idle owner slot after its asynchronous transition releases the slot lock.
/// 异步所有者转换释放槽位锁后移除空闲槽位。
///
/// The guard borrows the local slot Arc instead of cloning it, so a strong count of two proves
/// that only the registry and this transition can still access the slot. A concurrent waiter
/// keeps its own Arc and will perform its own cleanup after finishing.
/// 此守卫借用本地槽位 Arc 而不复制；强引用数为二时可证明只有注册表和当前转换仍能访问该槽位。
/// 并发等待者持有自己的 Arc，并会在完成转换后自行清理。
struct BrowserAiIdleSlotPrune<'a> {
    /// Manager owning the exact registry entry.
    /// 拥有精确注册表条目的管理器。
    manager: &'a BrowserManager,
    /// Trusted conversation identity used as the registry key.
    /// 作为注册表键的可信会话身份。
    owner: &'a str,
    /// Local Arc whose slot lock is released before this guard is dropped.
    /// 在此守卫析构前释放槽位锁的本地 Arc。
    slot: &'a Arc<tokio::sync::Mutex<Option<BrowserAiConnection>>>,
}

impl Drop for BrowserAiIdleSlotPrune<'_> {
    /// Prunes only an empty slot with no other live transition reference.
    /// 仅清理没有其他活动转换引用的空槽位。
    fn drop(&mut self) {
        let Ok(connection) = self.slot.try_lock() else {
            return;
        };
        if connection.is_some() {
            return;
        }
        let Ok(mut slots) = self.manager.ai_connections.lock() else {
            tracing::warn!("browser AI idle slot registry was poisoned during cleanup");
            return;
        };
        let removable = slots.get(self.owner).is_some_and(|registered| {
            Arc::ptr_eq(registered, self.slot) && Arc::strong_count(registered) == 2
        });
        if removable {
            slots.remove(self.owner);
        }
    }
}

/// Holds ordered browser preparation admission before its cancellation-independent task is spawned.
/// 在生成独立于取消的任务之前，按顺序持有浏览器准备准入。
struct BrowserAiPreparationReservation {
    /// Owned slot lock; optional only so Drop can unlock it before pruning the slot.
    /// 自有槽位锁；可选性仅用于 Drop 在清理槽位之前解锁。
    connection: Option<tokio::sync::OwnedMutexGuard<Option<BrowserAiConnection>>>,
    /// Manager owning the slot registry.
    /// 拥有槽位注册表的管理器。
    manager: BrowserManager,
    /// Exact conversation identity supplied by the host.
    /// 宿主提供的精确会话身份。
    owner: String,
    /// Slot reserved before Stop can observe a completed preparation cancellation.
    /// 在 Stop 可以观察到准备取消完成之前预留的槽位。
    slot: Arc<tokio::sync::Mutex<Option<BrowserAiConnection>>>,
    /// Plugin admission acquired before the slot to preserve drain lock ordering.
    /// 先于槽位取得的插件准入，用于保持排空锁顺序。
    _admission: tokio::sync::OwnedRwLockReadGuard<()>,
}

impl Drop for BrowserAiPreparationReservation {
    /// Unlocks before idle-slot pruning on success, error, cancellation, or panic.
    /// 在成功、错误、取消或 panic 时，先解锁再清理空闲槽位。
    fn drop(&mut self) {
        drop(self.connection.take());
        let _prune = BrowserAiIdleSlotPrune {
            manager: &self.manager,
            owner: &self.owner,
            slot: &self.slot,
        };
    }
}

/// Enabled plugin resources captured atomically before one browser transition.
/// 一次浏览器转换前原子捕获的已启用插件资源。
struct BrowserAiEnvironment {
    /// Core browser manager shared by every client and AI conversation.
    /// 所有客户端及 AI 会话共享的 Core 浏览器管理器。
    manager: BrowserManager,
    /// Plugin manager resolving the private managed Node launcher.
    /// 解析插件私有受管 Node 启动器的插件管理器。
    plugins: VulcanPluginRuntimeManager,
    /// Exact enabled plugin generation.
    /// 精确的已启用插件代次。
    plugin_generation: u64,
    /// MCP runtime registry shared by the current extension generation.
    /// 当前扩展代次共享的 MCP 运行时注册表。
    mcp_runtime: McpRuntimeAccess,
}

/// Captures an automation grant that failed before its endpoint became usable.
/// 记录在端点变得可用前失败的自动化授权。
///
/// Keeping every identifier lets the cleanup path distinguish a safe rejection from an
/// uncertain host operation and close the browser context when revocation is not confirmed.
/// 保留全部标识，使清理路径能够区分安全拒绝与未知宿主操作；撤销未确认时关闭浏览器上下文。
struct AutomationGatewayOpenFailure {
    /// Browser session that received the attempted grant.
    /// 收到本次尝试授权的浏览器会话。
    browser_id: String,
    /// Lease credential acquired before opening the gateway.
    /// 打开网关前取得的租约凭据。
    lease_secret: String,
    /// Fresh connection identity used for the attempted gateway.
    /// 本次尝试网关使用的新连接身份。
    connection_id: String,
    /// Original failure returned by the host gateway operation.
    /// 宿主网关操作返回的原始错误。
    error: String,
}

/// Host-owned runtime that derives browser ownership from trusted invocation context.
/// 从可信调用上下文确定浏览器归属的宿主运行时。
struct CoreNativeBrowserRuntime {
    /// Shared service state that owns the current browser manager and extension generation.
    /// 拥有当前浏览器管理器和扩展代次的共享服务状态。
    state: SharedServiceBridgeState,
}

impl fmt::Debug for CoreNativeBrowserRuntime {
    /// Redacts service internals while identifying the runtime in diagnostics.
    /// 隐去服务内部状态，仅在诊断中标识运行时。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CoreNativeBrowserRuntime")
    }
}

/// Builds the host-injected domain port for direct AI browser management.
/// 构建由宿主注入的 AI 直接浏览器管理领域端口。
pub(crate) fn native_browser_runtime(
    state: SharedServiceBridgeState,
) -> Result<Option<Arc<dyn NativeBrowserRuntime>>, String> {
    let enabled = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard
            .agent_runtime
            .as_ref()
            .and_then(|runtime| runtime.extension_generation.plugin_catalog())
            .is_some_and(|catalog| {
                catalog
                    .enabled_mcp_servers
                    .values()
                    .any(|server| server.plugin_id == BROWSER_PLUGIN_ID)
            })
    };
    Ok(enabled
        .then(|| Arc::new(CoreNativeBrowserRuntime { state }) as Arc<dyn NativeBrowserRuntime>))
}

/// Discovers approved owner-scoped Playwright tools before the provider request is launched.
/// 在启动 Provider 请求前发现已批准的所有者范围 Playwright 工具。
pub(crate) async fn prepare_browser_mcp_bindings(
    state: SharedServiceBridgeState,
    owner_session_id: SessionId,
) -> Result<Vec<McpRuntimeToolBinding>, String> {
    if native_browser_runtime(state.clone())?.is_none() {
        return Ok(Vec::new());
    }
    CoreNativeBrowserRuntime { state }
        .prepare_bindings(owner_session_id)
        .await
}

impl BrowserManager {
    /// Refuses new AI browser transitions before the plugin owner partition is drained.
    /// 在插件所有者分区排空前拒绝新的 AI 浏览器转换。
    pub(crate) fn begin_ai_drain(&self) {
        self.ai_drain_closed.store(true, Ordering::SeqCst);
    }

    /// Reopens AI browser transitions after an enabled plugin projection is published.
    /// 在已启用插件投影发布后重新开放 AI 浏览器转换。
    pub(crate) fn finish_ai_drain(&self) {
        self.ai_drain_closed.store(false, Ordering::SeqCst);
    }

    /// Retires one conversation's MCP connection and host grant when its logical AI turn ends.
    /// 在某个会话的逻辑 AI 回合结束时退休其 MCP 连接和宿主授权。
    /// `owner_session_id` is the trusted conversation owner; the Core browser and selection remain available for later turns.
    /// `owner_session_id` 是可信会话所有者；Core 浏览器及其选择保留给后续回合。
    pub(crate) async fn retire_ai_connection_after_turn(
        &self,
        owner_session_id: SessionId,
    ) -> Result<(), String> {
        let manager = self.clone();
        CoreNativeBrowserRuntime::await_owner_transition(async move {
            let owner = owner_session_id.to_string();
            let slot = manager
                .ai_connections
                .lock()
                .map_err(|error| format!("failed to lock browser AI connections: {error}"))?
                .get(&owner)
                .cloned();
            let Some(slot) = slot else {
                return Ok(());
            };
            let _idle_slot_prune = BrowserAiIdleSlotPrune {
                manager: &manager,
                owner: &owner,
                slot: &slot,
            };
            let mut connection = slot.lock().await;
            if let Some(current) = connection.take() {
                CoreNativeBrowserRuntime::retire_connection(
                    &manager,
                    owner_session_id,
                    current,
                    &mut connection,
                )
                .await?;
            }
            Ok(())
        })
        .await
    }

    /// Revokes the selected AI controller before granting a user takeover lease.
    /// 在向用户授予接管租约之前撤销已选中的 AI 控制者。
    ///
    /// `browser_id` identifies the Core session and `holder` names the new UI controller.
    /// Returns the new projection and secret only after the host confirms revocation.
    /// `browser_id` 标识 Core 会话，`holder` 命名新的界面控制者；仅在宿主确认撤销后返回
    /// 新投影和凭据。
    pub(crate) async fn takeover_after_retiring_ai(
        &self,
        browser_id: &str,
        holder: &str,
    ) -> Result<(BrowserSessionProjection, String), String> {
        let owner = self.session_projection(browser_id)?.owner_session_id;
        let slot = self
            .ai_connections
            .lock()
            .map_err(|error| format!("failed to lock browser AI connections: {error}"))?
            .get(&owner)
            .cloned();
        let Some(slot) = slot else {
            let browser_id_owned = String::from(browser_id);
            let holder_owned = String::from(holder);
            return self
                .run_blocking(browser_id, move |manager| {
                    manager.takeover_lease(&browser_id_owned, &holder_owned, None)
                })
                .await;
        };
        let owner_session_id = Uuid::parse_str(&owner)
            .map_err(|error| format!("invalid stored browser owner identity: {error}"))?;
        let manager = self.clone();
        let browser_id = String::from(browser_id);
        let holder = String::from(holder);
        CoreNativeBrowserRuntime::await_owner_transition(async move {
            let _idle_slot_prune = BrowserAiIdleSlotPrune {
                manager: &manager,
                owner: &owner,
                slot: &slot,
            };
            let mut connection = slot.lock().await;
            if connection
                .as_ref()
                .is_some_and(|current| current.browser_id.as_deref() == Some(browser_id.as_str()))
            {
                let current = connection
                    .take()
                    .ok_or_else(|| String::from("browser AI connection is unavailable"))?;
                CoreNativeBrowserRuntime::retire_connection(
                    &manager,
                    owner_session_id,
                    current,
                    &mut connection,
                )
                .await?;
            }
            manager
                .run_blocking(&browser_id.clone(), move |manager| {
                    manager.takeover_lease(&browser_id, &holder, None)
                })
                .await
        })
        .await
    }

    /// Stops every owner-scoped browser MCP process and revokes its host grant during plugin disable.
    /// 插件停用时停止所有所有者范围浏览器 MCP 进程并撤销宿主授权。
    pub(crate) async fn shutdown_ai_connections(&self) -> Result<(), String> {
        let manager = self.clone();
        CoreNativeBrowserRuntime::await_owner_transition(async move {
            // A writer waits for transitions admitted before drain and excludes late slot creation.
            // 写锁等待排空前已准入的转换，并阻止迟到的槽位创建。
            let _admission = manager.ai_transition_gate.clone().write_owned().await;
            let slots = manager
                .ai_connections
                .lock()
                .map_err(|error| format!("failed to lock browser AI connections: {error}"))?
                .iter()
                .map(|(owner, slot)| (owner.clone(), Arc::clone(slot)))
                .collect::<Vec<_>>();
            let mut failures = Vec::new();
            for (owner, slot) in slots {
                let owner_session_id = match Uuid::parse_str(&owner) {
                    Ok(owner_session_id) => owner_session_id,
                    Err(error) => {
                        failures.push(format!("invalid stored browser owner identity: {error}"));
                        continue;
                    }
                };
                let _idle_slot_prune = BrowserAiIdleSlotPrune {
                    manager: &manager,
                    owner: &owner,
                    slot: &slot,
                };
                let mut connection = slot.lock().await;
                if let Some(current) = connection.take()
                    && let Err(error) = CoreNativeBrowserRuntime::retire_connection(
                        &manager,
                        owner_session_id,
                        current,
                        &mut connection,
                    )
                    .await
                {
                    failures.push(error);
                }
            }
            if failures.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "one or more browser MCP grants could not be safely retired: {}",
                    failures.join("; ")
                ))
            }
        })
        .await
    }
}

impl CoreNativeBrowserRuntime {
    /// Reserves the plugin gate then the exact owner slot before any detached preparation is created.
    /// 在创建任何分离准备前，先预留插件门，再预留精确所有者槽位。
    /// `manager` owns both locks and `owner_session_id` selects the slot; returns an owned reservation.
    /// `manager` 拥有两把锁，`owner_session_id` 选择槽位；返回自有预留。
    async fn reserve_preparation_slot(
        manager: BrowserManager,
        owner_session_id: SessionId,
    ) -> Result<BrowserAiPreparationReservation, String> {
        // Waiting here is cancellable and has not started any independent browser operation.
        // 此处等待可以取消，且尚未启动任何独立浏览器操作。
        let admission = manager.ai_transition_gate.clone().read_owned().await;
        if manager.ai_drain_closed.load(Ordering::SeqCst) {
            return Err(String::from("browser plugin owner admission is draining"));
        }
        let owner = owner_session_id.to_string();
        let slot = Self::connection_slot(&manager, &owner)?;
        // A cancelled slot waiter must not leave an otherwise unreferenced empty slot.
        // 被取消的槽位等待者不得遗留没有其他引用的空槽位。
        let _prune = BrowserAiIdleSlotPrune {
            manager: &manager,
            owner: &owner,
            slot: &slot,
        };
        let connection = slot.clone().lock_owned().await;
        Ok(BrowserAiPreparationReservation {
            connection: Some(connection),
            manager: manager.clone(),
            owner: owner.clone(),
            slot: slot.clone(),
            _admission: admission,
        })
    }

    /// Runs an AI transition only while the plugin admission barrier remains open.
    /// 仅在插件准入屏障开放期间执行 AI 转换。
    async fn await_admitted_owner_transition<T, F>(
        manager: BrowserManager,
        operation: F,
    ) -> Result<T, String>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, String>> + Send + 'static,
    {
        Self::await_owner_transition(async move {
            let _admission = manager.ai_transition_gate.clone().read_owned().await;
            if manager.ai_drain_closed.load(Ordering::SeqCst) {
                return Err(String::from("browser plugin owner admission is draining"));
            }
            operation.await
        })
        .await
    }

    /// Runs one owner transition in a detached task that survives caller cancellation.
    /// 在一个即使调用者取消也会继续运行的脱离任务中执行所有者转换。
    ///
    /// The task owns the per-owner transition gate and must finish either by publishing its
    /// connection into that gate or by completing deterministic cleanup. Dropping the returned
    /// join handle therefore cannot abandon a lease after `automation.open` has attached it.
    /// 任务持有每个所有者的转换门，并且必须以连接写入门或确定性清理完成。调用者丢弃返回的
    /// JoinHandle 也不能在 `automation.open` 已挂接租约后遗弃该租约。
    async fn await_owner_transition<T, F>(operation: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, String>> + Send + 'static,
    {
        tokio::spawn(operation)
            .await
            .map_err(|error| format!("browser owner transition task failed: {error}"))?
    }

    /// Captures the current enabled browser plugin and its owner partition access.
    /// 捕获当前已启用的浏览器插件及其所有者分区访问面。
    fn environment(&self) -> Result<BrowserAiEnvironment, String> {
        let guard = self
            .state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let runtime = guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| String::from("browser plugin runtime is unavailable"))?;
        let catalog = runtime
            .extension_generation
            .plugin_catalog()
            .ok_or_else(|| String::from("browser plugin is disabled"))?;
        let descriptor = catalog
            .enabled_mcp_servers
            .get(&format!("{BROWSER_PLUGIN_ID}:{PLAYWRIGHT_SERVER_ID}"))
            .ok_or_else(|| String::from("browser plugin is disabled"))?;
        Ok(BrowserAiEnvironment {
            manager: guard.browser_manager.clone(),
            plugins: runtime.vulcan_plugin_runtime.clone(),
            plugin_generation: descriptor.plugin_generation,
            mcp_runtime: runtime.extension_generation.mcp_runtime(),
        })
    }

    /// Rejects a captured plugin generation after disable or replacement has been published.
    /// 在插件停用或替换已发布后拒绝之前捕获的旧代次。
    ///
    /// `environment` is the generation captured before asynchronous host and MCP startup.
    /// Returns an error if that generation is no longer the enabled browser plugin.
    /// `environment` 是异步启动宿主和 MCP 前捕获的代次；若它不再是已启用的浏览器插件，
    /// 则返回错误。
    fn ensure_environment_current(&self, environment: &BrowserAiEnvironment) -> Result<(), String> {
        let guard = self
            .state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let current = guard
            .agent_runtime
            .as_ref()
            .and_then(|runtime| runtime.extension_generation.plugin_catalog())
            .and_then(|catalog| {
                catalog
                    .enabled_mcp_servers
                    .get(&format!("{BROWSER_PLUGIN_ID}:{PLAYWRIGHT_SERVER_ID}"))
                    .map(|server| server.plugin_generation)
            });
        if current == Some(environment.plugin_generation) {
            Ok(())
        } else {
            Err(String::from(
                "browser plugin changed while the owner MCP connection was starting",
            ))
        }
    }

    /// Creates or finds the per-owner transition gate without holding a global lock during I/O.
    /// 不在 I/O 期间持有全局锁，为每个所有者创建或获取转换门。
    fn connection_slot(
        manager: &BrowserManager,
        owner: &str,
    ) -> Result<Arc<tokio::sync::Mutex<Option<BrowserAiConnection>>>, String> {
        let mut slots = manager
            .ai_connections
            .lock()
            .map_err(|error| format!("failed to lock browser AI connections: {error}"))?;
        Ok(Arc::clone(slots.entry(String::from(owner)).or_insert_with(
            || Arc::new(tokio::sync::Mutex::new(None)),
        )))
    }

    /// Materializes one plugin-owned stdio server with an endpoint kept inside Core.
    /// 使用仅留在 Core 内的端点物化一个插件拥有的 stdio 服务。
    fn server_config(
        environment: &BrowserAiEnvironment,
        owner: &str,
        endpoint: &str,
    ) -> Result<vulcan_agent_runtime::mcp::McpServerConfig, String> {
        plugin_conversation_mcp_server_config(
            &environment.plugins,
            BROWSER_PLUGIN_ID,
            PLAYWRIGHT_SERVER_ID,
            &crate::plugin_mcp_secret_store::OsPluginMcpSecretStore,
            owner,
            endpoint,
        )
    }

    /// Returns only reviewed browser tools from the exact owner's current MCP partition.
    /// 仅从精确所有者的当前 MCP 分区返回已审核的浏览器工具。
    fn approved_bindings(
        connection: &BrowserAiConnection,
        owner_session_id: SessionId,
    ) -> Result<Vec<McpRuntimeToolBinding>, String> {
        if !connection.ready {
            return Err(String::from(
                "browser AI connection startup is still being cleaned up",
            ));
        }
        let allowlist = McpRuntimeToolAllowlist::new(
            Some(BTreeSet::from([format!(
                "{BROWSER_PLUGIN_ID}:{PLAYWRIGHT_SERVER_ID}"
            )])),
            Some(
                APPROVED_PLAYWRIGHT_TOOLS
                    .iter()
                    .map(|name| String::from(*name))
                    .collect(),
            ),
        );
        let bindings = connection
            .mcp_runtime
            .owned_stdio_tool_bindings_following_replacement(
                BROWSER_PLUGIN_ID,
                connection.plugin_generation,
                owner_session_id,
                &allowlist,
            )?;
        for required in [
            "browser_navigate",
            "browser_snapshot",
            "browser_take_screenshot",
            "browser_click",
        ] {
            if !bindings
                .iter()
                .any(|binding| binding.origin_tool_name() == Some(required))
            {
                return Err(format!(
                    "Playwright MCP is missing required tool `{required}`"
                ));
            }
        }
        Ok(bindings)
    }

    /// Retires one gateway while preserving the browser when host confirmation is uncertain.
    /// 在宿主确认不明确时保留浏览器并撤销一个网关。
    ///
    /// This is shared by normal retirement and failures that happen after `automation.open` has
    /// attached the lease. A direct `release_lease` is deliberately insufficient for an attached
    /// Playwright socket, so an uncertain close escalates to context closure and process exit.
    /// 该逻辑同时用于正常退休和 `automation.open` 挂接租约后的失败。对于已挂接的 Playwright
    /// 套接字，直接 `release_lease` 不足以保证安全；关闭不确定时必须升级为上下文关闭并确认进程退出。
    fn retire_gateway_blocking(
        manager: &BrowserManager,
        browser_id: &str,
        owner: &str,
        lease_secret: &str,
        connection_id: &str,
    ) -> Result<(), String> {
        let projection = manager.session_projection(browser_id)?;
        // A confirmed host exit destroys its gateway. Retire only the stale AI connection;
        // the crashed browser and any uncertain Chromium operations remain blocked and visible.
        // 已确认的宿主退出会销毁其网关。这里只退休陈旧 AI 连接；
        // 崩溃浏览器及其未知 Chromium 操作仍保留阻塞状态并对外可见。
        if matches!(
            projection.status,
            BrowserSessionStatus::Closed | BrowserSessionStatus::Crashed
        ) {
            return Ok(());
        }
        if projection.automation_state == BrowserAutomationState::Attached {
            match manager.revoke_automation_gateway(browser_id, owner, connection_id) {
                Ok(projection) => match projection.lease_state {
                    BrowserLeaseState::Held => {
                        manager.release_lease(browser_id, lease_secret)?;
                        Ok(())
                    }
                    BrowserLeaseState::Free => Ok(()),
                    BrowserLeaseState::Blocked
                    | BrowserLeaseState::Revoking
                    | BrowserLeaseState::Draining => Err(String::from(
                        "browser lease is not releasable after automation revocation",
                    )),
                },
                Err(revoke_error) => manager
                    .close_uncertain_automation_session(browser_id, owner, connection_id)
                    .map(|_| ())
                    .map_err(|close_error| {
                        format!(
                            "automation revocation failed ({revoke_error}); browser context closure also failed ({close_error})"
                        )
                    }),
            }
        } else {
            match projection.lease_state {
                BrowserLeaseState::Held => {
                    manager.release_lease(browser_id, lease_secret)?;
                    Ok(())
                }
                BrowserLeaseState::Free => Ok(()),
                BrowserLeaseState::Blocked
                | BrowserLeaseState::Revoking
                | BrowserLeaseState::Draining => Err(String::from(
                    "browser lease is not releasable after automation retirement",
                )),
            }
        }
    }

    /// Revokes old MCP bindings before closing their host gateway and control lease.
    /// 在关闭旧宿主网关及控制租约前先撤销旧 MCP 绑定。
    async fn retire_connection(
        manager: &BrowserManager,
        owner_session_id: SessionId,
        mut connection: BrowserAiConnection,
        pending_slot: &mut Option<BrowserAiConnection>,
    ) -> Result<(), String> {
        if let Some(heartbeat) = connection.heartbeat.take() {
            heartbeat.abort();
            let _ = heartbeat.await;
        }
        // These credentials belong only to the retiring owner generation.
        // 这些凭据仅属于正在退休的所有者代次。
        let browser_id = connection.browser_id.clone();
        let lease_secret = connection.lease_secret.clone();
        let connection_id = connection.connection_id.clone();
        let mcp_shutdown = connection.mcp_runtime.shutdown_owned_stdio_partition(
            BROWSER_PLUGIN_ID,
            connection.plugin_generation,
            owner_session_id,
        );
        // Always retire the host grant even when local MCP teardown fails. The shared helper
        // recognizes confirmed host exit without clearing the browser's uncertainty block.
        // 即使本地 MCP 清理失败，也必须退休宿主授权。共享方法识别已确认的宿主退出，
        // 同时保留浏览器未知结果的阻塞状态。
        let gateway_retirement = match (browser_id, lease_secret, connection_id) {
            (Some(browser_id), Some(lease_secret), Some(connection_id)) => {
                // The blocking host channel must not stall the asynchronous owner transition executor.
                // 阻塞的宿主通道不得阻塞异步所有者转换执行器。
                let manager = manager.clone();
                let owner = owner_session_id.to_string();
                tokio::task::spawn_blocking(move || {
                    Self::retire_gateway_blocking(
                        &manager,
                        &browser_id,
                        &owner,
                        &lease_secret,
                        &connection_id,
                    )
                })
                .await
                .map_err(|error| format!("browser gateway task failed: {error}"))?
            }
            (None, None, None) => Ok(()),
            _ => Err(String::from("browser AI connection state is inconsistent")),
        };
        // Keep failure metadata available for exact cleanup retries rather than losing a live grant.
        // 保留失败元数据供精确重试清理，避免遗失仍存活的授权。
        let result = Self::combine_cleanup_results(mcp_shutdown, gateway_retirement);
        if result.is_err() {
            connection.ready = false;
            connection.heartbeat = None;
            *pending_slot = Some(connection);
        }
        result
    }

    /// Releases a newly opened gateway when server materialization or startup fails.
    /// 在服务物化或启动失败时释放刚打开的网关。
    async fn release_failed_gateway(
        manager: &BrowserManager,
        owner_session_id: SessionId,
        browser_id: String,
        lease_secret: String,
        connection_id: String,
    ) -> Result<(), String> {
        let manager = manager.clone();
        let owner = owner_session_id.to_string();
        tokio::task::spawn_blocking(move || {
            Self::retire_gateway_blocking(
                &manager,
                &browser_id,
                &owner,
                &lease_secret,
                &connection_id,
            )
        })
        .await
        .map_err(|error| format!("browser gateway cleanup task failed: {error}"))?
    }

    /// Combines MCP partition cleanup and host authorization cleanup without losing either error.
    /// 合并 MCP 分区清理与宿主授权清理结果，避免丢失任一错误。
    fn combine_cleanup_results(
        mcp_cleanup: Result<(), String>,
        gateway_cleanup: Result<(), String>,
    ) -> Result<(), String> {
        match (mcp_cleanup, gateway_cleanup) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(mcp_error), Ok(())) => Err(format!("owner MCP cleanup failed: {mcp_error}")),
            (Ok(()), Err(gateway_error)) => Err(gateway_error),
            (Err(mcp_error), Err(gateway_error)) => Err(format!(
                "owner MCP cleanup failed ({mcp_error}); authorization cleanup failed ({gateway_error})"
            )),
        }
    }

    /// Retains pending grant metadata only when cleanup itself is not confirmed.
    /// 仅在清理本身未确认时保留待处理授权元数据。
    ///
    /// A successful cleanup removes the pending slot so the next request cannot mistake an
    /// unusable MCP partition for a reusable connection. An unsuccessful cleanup leaves the
    /// metadata discoverable by the owner shutdown path and keeps the browser fail-closed.
    /// 清理成功后移除待处理槽位，避免下一次请求把不可用 MCP 分区误认为可复用连接；清理失败时
    /// 保留元数据，供所有者关闭路径发现，同时让浏览器保持故障关闭状态。
    fn establishment_failure_message(
        pending_slot: &mut Option<BrowserAiConnection>,
        original_error: String,
        cleanup: Result<(), String>,
    ) -> String {
        match cleanup {
            Ok(()) => {
                pending_slot.take();
                original_error
            }
            Err(cleanup_error) => {
                format!("{original_error}; authorization cleanup failed: {cleanup_error}")
            }
        }
    }

    /// Closes a failed browser only after its authorization state is explicitly safe.
    /// 仅在授权状态明确安全后关闭失败的浏览器。
    ///
    /// `close_session` is intentionally not called for an attached or blocked session: forcing
    /// that transition would discard the only evidence needed for later recovery.
    /// 对于已挂接或已阻塞的会话故意不调用 `close_session`，避免强制转换丢失后续恢复所需的唯一证据。
    fn close_failed_browser_if_safe(manager: &BrowserManager, browser_id: &str) {
        let Ok(projection) = manager.session_projection(browser_id) else {
            return;
        };
        let Ok(unconfirmed) = manager.unconfirmed_operations(browser_id) else {
            return;
        };
        if projection.lease_state == BrowserLeaseState::Free
            && projection.automation_state == BrowserAutomationState::Detached
            && unconfirmed.is_empty()
        {
            let _ = manager.close_session(browser_id);
        }
    }

    /// Opens a template or a real browser-bound owner MCP process after a prior grant is retired.
    /// 在旧授权撤销后启动模板或绑定真实浏览器的所有者 MCP 进程。
    async fn establish_connection(
        environment: &BrowserAiEnvironment,
        owner_session_id: SessionId,
        browser_id: Option<String>,
        pending_slot: &mut Option<BrowserAiConnection>,
    ) -> Result<BrowserAiConnection, String> {
        let owner = owner_session_id.to_string();
        let Some(browser_id) = browser_id else {
            let config = Self::server_config(environment, &owner, DISCOVERY_ENDPOINT)?;
            let partition = environment
                .mcp_runtime
                .open_ordered_owned_stdio_partition(
                    BROWSER_PLUGIN_ID,
                    environment.plugin_generation,
                    owner_session_id,
                    vec![config],
                )
                .await;
            if let Err(error) = partition {
                // A failed bootstrap may have created a process before publishing its lease, so
                // request exact owner shutdown and retain metadata if that cleanup is uncertain.
                // 启动失败可能在发布租约前已经创建进程，因此请求精确所有者分区关闭，并在清理不确定时保留元数据。
                *pending_slot = Some(BrowserAiConnection {
                    plugin_generation: environment.plugin_generation,
                    mcp_runtime: environment.mcp_runtime.clone(),
                    browser_id: None,
                    lease_secret: None,
                    connection_id: None,
                    heartbeat: None,
                    ready: false,
                });
                let cleanup = environment.mcp_runtime.shutdown_owned_stdio_partition(
                    BROWSER_PLUGIN_ID,
                    environment.plugin_generation,
                    owner_session_id,
                );
                return Err(Self::establishment_failure_message(
                    pending_slot,
                    error,
                    cleanup,
                ));
            }
            return Ok(BrowserAiConnection {
                plugin_generation: environment.plugin_generation,
                mcp_runtime: environment.mcp_runtime.clone(),
                browser_id: None,
                lease_secret: None,
                connection_id: None,
                heartbeat: None,
                ready: true,
            });
        };
        let connection_id = Uuid::new_v4().to_string();
        let manager = environment.manager.clone();
        let host_browser_id = browser_id.clone();
        let host_owner = owner.clone();
        let host_connection_id = connection_id.clone();
        let gateway = tokio::task::spawn_blocking(move || {
            let (_, lease_secret) = manager.acquire_lease(
                &host_browser_id,
                &format!("ai:{host_owner}"),
                Some(AI_LEASE_TTL),
            )?;
            match manager.open_automation_gateway(
                &host_browser_id,
                &host_owner,
                &lease_secret,
                &host_connection_id,
            ) {
                Ok(grant) => Ok::<_, String>(Ok((lease_secret, grant))),
                Err(error) => Ok(Err(AutomationGatewayOpenFailure {
                    browser_id: host_browser_id,
                    lease_secret,
                    connection_id: host_connection_id,
                    error,
                })),
            }
        })
        .await
        .map_err(|error| format!("browser grant task failed: {error}"))??;
        let (lease_secret, grant) = match gateway {
            Ok(grant) => grant,
            Err(failure) => {
                let AutomationGatewayOpenFailure {
                    browser_id,
                    lease_secret,
                    connection_id,
                    error,
                } = failure;
                *pending_slot = Some(BrowserAiConnection {
                    plugin_generation: environment.plugin_generation,
                    mcp_runtime: environment.mcp_runtime.clone(),
                    browser_id: Some(browser_id.clone()),
                    lease_secret: Some(lease_secret.clone()),
                    connection_id: Some(connection_id.clone()),
                    heartbeat: None,
                    ready: false,
                });
                let cleanup = Self::release_failed_gateway(
                    &environment.manager,
                    owner_session_id,
                    browser_id,
                    lease_secret,
                    connection_id,
                )
                .await;
                return Err(Self::establishment_failure_message(
                    pending_slot,
                    error,
                    cleanup,
                ));
            }
        };
        *pending_slot = Some(BrowserAiConnection {
            plugin_generation: environment.plugin_generation,
            mcp_runtime: environment.mcp_runtime.clone(),
            browser_id: Some(grant.session_id.clone()),
            lease_secret: Some(lease_secret.clone()),
            connection_id: Some(grant.connection_id.clone()),
            heartbeat: None,
            ready: false,
        });
        let config = match Self::server_config(environment, &owner, &grant.endpoint) {
            Ok(config) => config,
            Err(error) => {
                let cleanup = Self::release_failed_gateway(
                    &environment.manager,
                    owner_session_id,
                    browser_id,
                    lease_secret,
                    connection_id,
                )
                .await;
                return Err(Self::establishment_failure_message(
                    pending_slot,
                    error,
                    cleanup,
                ));
            }
        };
        if let Err(partition_error) = environment
            .mcp_runtime
            .replace_ordered_owned_stdio_partition(
                BROWSER_PLUGIN_ID,
                environment.plugin_generation,
                owner_session_id,
                vec![config],
            )
            .await
        {
            let mcp_cleanup = environment.mcp_runtime.shutdown_owned_stdio_partition(
                BROWSER_PLUGIN_ID,
                environment.plugin_generation,
                owner_session_id,
            );
            let gateway_cleanup = Self::release_failed_gateway(
                &environment.manager,
                owner_session_id,
                browser_id,
                lease_secret,
                connection_id,
            )
            .await;
            let cleanup = Self::combine_cleanup_results(mcp_cleanup, gateway_cleanup);
            return Err(Self::establishment_failure_message(
                pending_slot,
                format!("failed to start owner-scoped Playwright MCP: {partition_error}"),
                cleanup,
            ));
        }
        // The host's one-use gateway expires if nobody connects within 30 seconds. MCP schema
        // discovery does not establish CDP, so consume the grant now and retain its live socket.
        // 宿主一次性网关若 30 秒无人连接便会过期；MCP Schema 发现不会建立 CDP，故此处立即
        // 消费授权并保持活动套接字。
        let warmup = environment
            .mcp_runtime
            .invoke_owned_stdio_tool(
                BROWSER_PLUGIN_ID,
                environment.plugin_generation,
                owner_session_id,
                &format!("{BROWSER_PLUGIN_ID}:{PLAYWRIGHT_SERVER_ID}/browser_snapshot"),
                serde_json::json!({}),
            )
            .await;
        if !matches!(&warmup, Ok(value) if value.get("isError").and_then(serde_json::Value::as_bool) != Some(true))
        {
            let mcp_cleanup = environment.mcp_runtime.shutdown_owned_stdio_partition(
                BROWSER_PLUGIN_ID,
                environment.plugin_generation,
                owner_session_id,
            );
            let gateway_cleanup = Self::release_failed_gateway(
                &environment.manager,
                owner_session_id,
                browser_id,
                lease_secret,
                connection_id,
            )
            .await;
            let authorization_cleanup = Self::combine_cleanup_results(mcp_cleanup, gateway_cleanup);
            return Err(Self::establishment_failure_message(
                pending_slot,
                String::from("Playwright MCP could not attach to the Core browser gateway"),
                authorization_cleanup,
            ));
        }
        // The supervisor never receives model parameters. A failed renewal first revokes its
        // MCP transport, so an expired Core lease cannot leave a live page controller behind.
        // 监督任务不接触模型参数；续订失败时先撤销 MCP 传输，避免 Core 租约到期后仍有活动页面控制者。
        let heartbeat_manager = environment.manager.clone();
        let heartbeat_runtime = environment.mcp_runtime.clone();
        let heartbeat_browser_id = browser_id;
        let heartbeat_lease_secret = lease_secret.clone();
        let heartbeat_generation = environment.plugin_generation;
        let heartbeat = tokio::spawn(async move {
            let mut interval = tokio::time::interval(AI_LEASE_HEARTBEAT_INTERVAL);
            interval.tick().await;
            loop {
                interval.tick().await;
                if let Err(error) = heartbeat_manager.heartbeat_lease_with_ttl(
                    &heartbeat_browser_id,
                    &heartbeat_lease_secret,
                    AI_LEASE_TTL,
                ) {
                    let _ = heartbeat_runtime.shutdown_owned_stdio_partition(
                        BROWSER_PLUGIN_ID,
                        heartbeat_generation,
                        owner_session_id,
                    );
                    tracing::error!("browser AI lease renewal failed; MCP access revoked: {error}");
                    break;
                }
            }
        });
        Ok(BrowserAiConnection {
            plugin_generation: environment.plugin_generation,
            mcp_runtime: environment.mcp_runtime.clone(),
            browser_id: Some(grant.session_id),
            lease_secret: Some(lease_secret),
            connection_id: Some(grant.connection_id),
            heartbeat: Some(heartbeat),
            ready: true,
        })
    }

    /// Ensures model-visible schemas come from the current exact owner partition.
    /// 确保模型可见 Schema 来自当前精确所有者分区。
    async fn prepare_bindings(
        &self,
        owner_session_id: SessionId,
    ) -> Result<Vec<McpRuntimeToolBinding>, String> {
        let environment = self.environment()?;
        // Acquire ordered admission before creating cancellation-independent work, closing the late-slot race.
        // 创建独立于取消的工作前取得有序准入，封闭迟到槽位竞争。
        let reservation =
            Self::reserve_preparation_slot(environment.manager.clone(), owner_session_id).await?;
        let state = self.state.clone();
        Self::await_owner_transition(async move {
            Self::prepare_bindings_transition(state, environment, owner_session_id, reservation)
                .await
        })
        .await
    }

    /// Completes one owner binding transition while holding its slot until cleanup is settled.
    /// 持有所有者槽位直到清理确定后完成一次所有者绑定转换。
    async fn prepare_bindings_transition(
        state: SharedServiceBridgeState,
        environment: BrowserAiEnvironment,
        owner_session_id: SessionId,
        mut reservation: BrowserAiPreparationReservation,
    ) -> Result<Vec<McpRuntimeToolBinding>, String> {
        let owner = owner_session_id.to_string();
        // The detached task already owns this lock; Stop must wait for its conversion before retirement.
        // 分离任务已经拥有此锁；Stop 必须等此转换完成后才能退休。
        let connection = reservation
            .connection
            .as_deref_mut()
            .expect("browser preparation reservation remains locked until Drop");
        let selected = environment.manager.selected_ai_browser(&owner)?;
        let selected_browser_id = selected.map(|projection| projection.session_id);
        let reused = connection.as_ref().is_some_and(|current| {
            // A disable rollback can reopen plugin admission after its drain removed the old
            // partition. Slot metadata alone cannot prove that the captured MCP process exists.
            // 停用回滚可能在排空移除旧分区后重新开放插件准入；仅靠槽位元数据不能证明 MCP 进程仍存在。
            current.ready
                && current.plugin_generation == environment.plugin_generation
                && current.browser_id == selected_browser_id
                && current
                    .mcp_runtime
                    .owned_stdio_partition(
                        BROWSER_PLUGIN_ID,
                        current.plugin_generation,
                        owner_session_id,
                    )
                    .is_ok()
                && current
                    .heartbeat
                    .as_ref()
                    .is_none_or(|task| !task.is_finished())
        });
        if !reused {
            if let Some(previous) = connection.take() {
                Self::retire_connection(
                    &environment.manager,
                    owner_session_id,
                    previous,
                    connection,
                )
                .await?;
            }
            let next = Self::establish_connection(
                &environment,
                owner_session_id,
                selected_browser_id,
                connection,
            )
            .await?;
            *connection = Some(next);
        }
        let runtime = Self { state };
        if let Err(error) = runtime.ensure_environment_current(&environment) {
            if let Some(current) = connection.take() {
                Self::retire_connection(
                    &environment.manager,
                    owner_session_id,
                    current,
                    connection,
                )
                .await?;
            }
            return Err(error);
        }
        Self::approved_bindings(
            connection
                .as_ref()
                .ok_or_else(|| String::from("browser MCP connection is unavailable"))?,
            owner_session_id,
        )
    }

    /// Resolves the currently enabled plugin generation together with Core's browser manager.
    /// 解析当前已启用的插件代次和 Core 浏览器管理器。
    fn enabled_manager(&self) -> Result<BrowserManager, String> {
        let guard = self
            .state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let runtime = guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| String::from("browser plugin runtime is unavailable"))?;
        let enabled = runtime
            .extension_generation
            .plugin_catalog()
            .is_some_and(|catalog| {
                catalog
                    .enabled_mcp_servers
                    .values()
                    .any(|server| server.plugin_id == BROWSER_PLUGIN_ID)
            });
        if !enabled {
            return Err(String::from("browser plugin is disabled"));
        }
        Ok(guard.browser_manager.clone())
    }

    /// Projects one Core browser into the stable, credential-free AI tool result.
    /// 将单个 Core 浏览器投影为稳定且不含凭据的 AI 工具结果。
    fn snapshot(
        projection: BrowserSessionProjection,
        selected_browser_id: Option<&str>,
    ) -> NativeBrowserSnapshot {
        let status = match projection.status {
            BrowserSessionStatus::Creating => "creating",
            BrowserSessionStatus::Running => "running",
            BrowserSessionStatus::Closing => "closing",
            BrowserSessionStatus::Closed => "closed",
            BrowserSessionStatus::Crashed => "crashed",
            BrowserSessionStatus::Failed => "failed",
        };
        NativeBrowserSnapshot {
            selected: selected_browser_id == Some(projection.session_id.as_str()),
            browser_id: projection.session_id,
            status: String::from(status),
            current_url: projection.current_url,
            current_title: projection.current_title,
        }
    }
}

impl CoreNativeBrowserRuntime {
    /// Creates and binds a browser while holding the owner transition slot.
    /// 持有所有者转换槽创建并绑定浏览器。
    async fn open_transition(
        state: SharedServiceBridgeState,
        environment: BrowserAiEnvironment,
        owner_session_id: SessionId,
        persistent_profile: bool,
    ) -> Result<NativeBrowserSnapshot, String> {
        let owner = owner_session_id.to_string();
        let slot = Self::connection_slot(&environment.manager, &owner)?;
        let _idle_slot_prune = BrowserAiIdleSlotPrune {
            manager: &environment.manager,
            owner: &owner,
            slot: &slot,
        };
        let mut connection = slot.lock().await;
        let profile = if persistent_profile {
            BrowserProfileMode::Persistent
        } else {
            BrowserProfileMode::Ephemeral
        };
        let manager = environment.manager.clone();
        let host_owner = owner.clone();
        let browser_id = tokio::task::spawn_blocking(move || {
            let created = manager.create_owned_session(&host_owner, profile)?;
            let browser_id = created.session_id;
            if let Err(error) = manager.start_host(&browser_id) {
                let _ = manager.close_session(&browser_id);
                return Err(error);
            }
            Ok(browser_id)
        })
        .await
        .map_err(|error| format!("browser host task failed: {error}"))??;
        if let Some(previous) = connection.take()
            && let Err(error) = Self::retire_connection(
                &environment.manager,
                owner_session_id,
                previous,
                &mut connection,
            )
            .await
        {
            let _ = environment.manager.close_session(&browser_id);
            return Err(error);
        }
        let next = match Self::establish_connection(
            &environment,
            owner_session_id,
            Some(browser_id.clone()),
            &mut connection,
        )
        .await
        {
            Ok(next) => next,
            Err(error) => {
                Self::close_failed_browser_if_safe(&environment.manager, &browser_id);
                return Err(error);
            }
        };
        let runtime = Self { state };
        if let Err(error) = runtime.ensure_environment_current(&environment) {
            *connection = Some(next);
            let current = connection
                .take()
                .ok_or_else(|| String::from("browser AI connection is unavailable"))?;
            return match Self::retire_connection(
                &environment.manager,
                owner_session_id,
                current,
                &mut connection,
            )
            .await
            {
                Ok(()) => {
                    environment.manager.close_session(&browser_id)?;
                    Err(error)
                }
                Err(cleanup_error) => Err(format!(
                    "{error}; browser authorization cleanup failed: {cleanup_error}"
                )),
            };
        }
        match environment.manager.select_ai_browser(&owner, &browser_id) {
            Ok(selected) => {
                *connection = Some(next);
                Ok(Self::snapshot(selected, Some(&browser_id)))
            }
            Err(error) => {
                *connection = Some(next);
                let current = connection
                    .take()
                    .ok_or_else(|| String::from("browser AI connection is unavailable"))?;
                let cleanup = Self::retire_connection(
                    &environment.manager,
                    owner_session_id,
                    current,
                    &mut connection,
                )
                .await;
                Self::close_failed_browser_if_safe(&environment.manager, &browser_id);
                match cleanup {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(format!(
                        "{error}; browser authorization cleanup failed: {cleanup_error}"
                    )),
                }
            }
        }
    }
}

impl CoreNativeBrowserRuntime {
    /// Selects and binds one owner browser while holding its transition slot.
    /// 持有所有者转换槽选中并绑定一个浏览器。
    async fn select_transition(
        state: SharedServiceBridgeState,
        environment: BrowserAiEnvironment,
        owner_session_id: SessionId,
        browser_id: String,
    ) -> Result<NativeBrowserSnapshot, String> {
        let owner = owner_session_id.to_string();
        environment.manager.require_owner(&browser_id, &owner)?;
        let projection = environment.manager.session_projection(&browser_id)?;
        if projection.status != BrowserSessionStatus::Running {
            return Err(String::from("browser is not running"));
        }
        let slot = Self::connection_slot(&environment.manager, &owner)?;
        let _idle_slot_prune = BrowserAiIdleSlotPrune {
            manager: &environment.manager,
            owner: &owner,
            slot: &slot,
        };
        let mut connection = slot.lock().await;
        let already_connected = connection.as_ref().is_some_and(|current| {
            // A ready slot cannot be reused after plugin drain removed its MCP partition.
            // 插件排空移除 MCP 分区后，即使槽位仍标记就绪也不能复用。
            current.ready
                && current.plugin_generation == environment.plugin_generation
                && current.browser_id.as_deref() == Some(browser_id.as_str())
                && current
                    .mcp_runtime
                    .owned_stdio_partition(
                        BROWSER_PLUGIN_ID,
                        current.plugin_generation,
                        owner_session_id,
                    )
                    .is_ok()
                && current
                    .heartbeat
                    .as_ref()
                    .is_some_and(|task| !task.is_finished())
        });
        if !already_connected {
            if let Some(previous) = connection.take() {
                Self::retire_connection(
                    &environment.manager,
                    owner_session_id,
                    previous,
                    &mut connection,
                )
                .await?;
            }
            *connection = Some(
                Self::establish_connection(
                    &environment,
                    owner_session_id,
                    Some(browser_id.clone()),
                    &mut connection,
                )
                .await?,
            );
        }
        let runtime = Self { state };
        if let Err(error) = runtime.ensure_environment_current(&environment) {
            if let Some(current) = connection.take() {
                Self::retire_connection(
                    &environment.manager,
                    owner_session_id,
                    current,
                    &mut connection,
                )
                .await?;
            }
            return Err(error);
        }
        let selected = environment.manager.select_ai_browser(&owner, &browser_id)?;
        Ok(Self::snapshot(selected, Some(&browser_id)))
    }
}

impl CoreNativeBrowserRuntime {
    /// Closes one owner browser while serializing every MCP retirement and host shutdown.
    /// 串行完成 MCP 退休与宿主关闭后关闭一个所有者浏览器。
    async fn close_transition(
        _state: SharedServiceBridgeState,
        environment: BrowserAiEnvironment,
        owner_session_id: SessionId,
        browser_id: String,
    ) -> Result<NativeBrowserSnapshot, String> {
        let owner = owner_session_id.to_string();
        environment.manager.require_owner(&browser_id, &owner)?;
        let slot = Self::connection_slot(&environment.manager, &owner)?;
        let _idle_slot_prune = BrowserAiIdleSlotPrune {
            manager: &environment.manager,
            owner: &owner,
            slot: &slot,
        };
        let mut connection = slot.lock().await;
        if connection
            .as_ref()
            .is_some_and(|current| current.browser_id.as_deref() == Some(browser_id.as_str()))
        {
            let current = connection
                .take()
                .ok_or_else(|| String::from("browser AI connection is unavailable"))?;
            Self::retire_connection(
                &environment.manager,
                owner_session_id,
                current,
                &mut connection,
            )
            .await?;
        }
        let manager = environment.manager.clone();
        let closed = tokio::task::spawn_blocking(move || manager.close_session(&browser_id))
            .await
            .map_err(|error| format!("browser close task failed: {error}"))??;
        Ok(Self::snapshot(closed, None))
    }
}

#[async_trait::async_trait]
impl NativeBrowserRuntime for CoreNativeBrowserRuntime {
    /// Lists only the exact trusted conversation's browsers and selected browser.
    /// 仅列出精确可信会话拥有的浏览器与已选浏览器。
    async fn list(
        &self,
        owner_session_id: SessionId,
    ) -> Result<Vec<NativeBrowserSnapshot>, String> {
        let manager = self.enabled_manager()?;
        let owner = owner_session_id.to_string();
        let selected = manager
            .selected_ai_browser(&owner)?
            .map(|item| item.session_id);
        Ok(manager
            .list_owned_sessions(&owner)?
            .into_iter()
            .map(|item| Self::snapshot(item, selected.as_deref()))
            .collect())
    }

    /// Creates, starts, and selects one Core browser for the trusted conversation.
    /// 为可信会话创建、启动并选中一个 Core 浏览器。
    async fn open(
        &self,
        owner_session_id: SessionId,
        persistent_profile: bool,
    ) -> Result<NativeBrowserSnapshot, String> {
        let environment = self.environment()?;
        let state = self.state.clone();
        Self::await_admitted_owner_transition(environment.manager.clone(), async move {
            Self::open_transition(state, environment, owner_session_id, persistent_profile).await
        })
        .await
    }

    /// Selects an already running browser after checking immutable conversation ownership.
    /// 校验不可变会话归属后选中一个已运行浏览器。
    async fn select(
        &self,
        owner_session_id: SessionId,
        browser_id: &str,
    ) -> Result<NativeBrowserSnapshot, String> {
        let environment = self.environment()?;
        let state = self.state.clone();
        let browser_id = String::from(browser_id);
        Self::await_admitted_owner_transition(environment.manager.clone(), async move {
            Self::select_transition(state, environment, owner_session_id, browser_id).await
        })
        .await
    }

    /// Closes one browser only after checking its exact trusted conversation owner.
    /// 仅在校验精确可信会话归属后关闭该浏览器。
    async fn close(
        &self,
        owner_session_id: SessionId,
        browser_id: &str,
    ) -> Result<NativeBrowserSnapshot, String> {
        let environment = self.environment()?;
        let state = self.state.clone();
        let browser_id = String::from(browser_id);
        Self::await_admitted_owner_transition(environment.manager.clone(), async move {
            Self::close_transition(state, environment, owner_session_id, browser_id).await
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tokio::sync::oneshot;

    /// A cancelled binding waiter cannot let retirement finish before its reserved conversion publishes.
    /// 绑定等待者取消后，退休不能早于已预留转换的发布完成。
    #[tokio::test]
    async fn preparation_reservation_orders_cancelled_conversion_before_retirement() {
        // Real Core ownership and an empty MCP registry avoid launching any external browser process.
        // 真实 Core 所有权和空 MCP 注册表避免启动外部浏览器进程。
        let manager = BrowserManager::new();
        let owner = SessionId::new_v4();
        let browser = manager
            .create_owned_session(&owner.to_string(), BrowserProfileMode::Ephemeral)
            .unwrap();
        let mcp_runtime = vulcan_agent_runtime::mcp::bootstrap_mcp_runtime_access(Vec::new())
            .await
            .unwrap();
        let mut reservation =
            CoreNativeBrowserRuntime::reserve_preparation_slot(manager.clone(), owner)
                .await
                .unwrap();
        assert!(
            manager.ai_connections.lock().unwrap()[&owner.to_string()]
                .try_lock()
                .is_err()
        );
        // Hold the detached operation before its late lease acquisition and connection publication.
        // 在迟到租约获取及连接发布之前阻挡分离操作。
        let (entered, entered_receiver) = oneshot::channel();
        let (release, release_receiver) = oneshot::channel();
        let transition_manager = manager.clone();
        let browser_id = browser.session_id.clone();
        let waiter = tokio::spawn(CoreNativeBrowserRuntime::await_owner_transition(
            async move {
                entered.send(()).unwrap();
                release_receiver.await.unwrap();
                let (_, lease_secret) = transition_manager.acquire_lease(
                    &browser_id,
                    "ai:preparation-stop-test",
                    None,
                )?;
                // A partially established connection already owns a lease even before gateway attachment.
                // 部分建立的连接在网关附加前已经持有租约。
                *reservation.connection.as_deref_mut().unwrap() = Some(BrowserAiConnection {
                    plugin_generation: 1,
                    mcp_runtime,
                    browser_id: Some(browser_id),
                    lease_secret: Some(lease_secret),
                    connection_id: Some(String::from("preparation-stop-test")),
                    heartbeat: None,
                    ready: false,
                });
                Ok::<(), String>(())
            },
        ));
        tokio::time::timeout(Duration::from_secs(10), entered_receiver)
            .await
            .unwrap()
            .unwrap();
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        let retirement_manager = manager.clone();
        let mut retirement = tokio::spawn(async move {
            retirement_manager
                .retire_ai_connection_after_turn(owner)
                .await
        });
        // Retirement must remain blocked while the independently running operation owns its slot.
        // 独立运行的操作拥有槽位时，退休必须保持阻塞。
        assert!(
            tokio::time::timeout(Duration::from_millis(30), &mut retirement)
                .await
                .is_err()
        );
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(10), retirement)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let projection = manager.session_projection(&browser.session_id).unwrap();
        assert_eq!(projection.lease_state, BrowserLeaseState::Free);
        assert_eq!(
            projection.automation_state,
            BrowserAutomationState::Detached
        );
        assert!(manager.ai_connections.lock().unwrap().is_empty());
    }

    /// Preparation reservations preserve plugin drain ordering and reclaim their empty slots on Drop.
    /// 准备预留保持插件排空锁顺序，并在析构时回收空槽位。
    #[tokio::test]
    async fn preparation_reservation_preserves_plugin_drain_and_idle_cleanup() {
        let manager = BrowserManager::new();
        let owner = SessionId::new_v4();
        let reservation =
            CoreNativeBrowserRuntime::reserve_preparation_slot(manager.clone(), owner)
                .await
                .unwrap();
        assert!(manager.ai_transition_gate.try_write().is_err());
        manager.begin_ai_drain();
        let drain_manager = manager.clone();
        let mut drain = tokio::spawn(async move { drain_manager.shutdown_ai_connections().await });
        assert!(
            tokio::time::timeout(Duration::from_millis(30), &mut drain)
                .await
                .is_err()
        );
        drop(reservation);
        tokio::time::timeout(Duration::from_secs(10), drain)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            CoreNativeBrowserRuntime::reserve_preparation_slot(
                manager.clone(),
                SessionId::new_v4()
            )
            .await
            .is_err()
        );
        assert!(manager.ai_connections.lock().unwrap().is_empty());
        manager.finish_ai_drain();
        // Re-enabling admission must allow the next owner without retaining the previous idle slot.
        // 重新启用准入必须允许下一所有者，且不保留前一个空槽位。
        let next = CoreNativeBrowserRuntime::reserve_preparation_slot(
            manager.clone(),
            SessionId::new_v4(),
        )
        .await
        .unwrap();
        drop(next);
        assert!(manager.ai_connections.lock().unwrap().is_empty());
    }

    /// Verifies completed owners release idle slots without detaching a concurrent holder.
    /// 验证已结束会话释放空闲槽位，同时不会使并发持有者脱离注册表。
    #[tokio::test]
    async fn idle_owner_slots_are_reclaimed_after_transition_cleanup() {
        let manager = BrowserManager::new();
        // Repeated conversations previously left one empty registry entry per finished turn.
        // 先前每个完成的会话都会在注册表留下一个空槽位。
        for _ in 0..64 {
            let owner = SessionId::new_v4();
            let slot = CoreNativeBrowserRuntime::connection_slot(&manager, &owner.to_string())
                .expect("create owner slot");
            drop(slot);
            manager
                .retire_ai_connection_after_turn(owner)
                .await
                .expect("retire empty owner slot");
        }
        assert!(
            manager
                .ai_connections
                .lock()
                .expect("owner registry")
                .is_empty()
        );

        // An extra transition reference keeps the registered slot until that transition ends.
        // 额外的转换引用会使注册槽位保留，直到该转换结束。
        let owner = SessionId::new_v4();
        let owner_text = owner.to_string();
        let slot = CoreNativeBrowserRuntime::connection_slot(&manager, &owner_text)
            .expect("create shared owner slot");
        let waiter_slot = Arc::clone(&slot);
        drop(slot);
        manager
            .retire_ai_connection_after_turn(owner)
            .await
            .expect("retire while another holder exists");
        assert_eq!(
            manager.ai_connections.lock().expect("owner registry").len(),
            1
        );
        let waiter_prune = BrowserAiIdleSlotPrune {
            manager: &manager,
            owner: &owner_text,
            slot: &waiter_slot,
        };
        let waiter_guard = waiter_slot.lock().await;
        drop(waiter_guard);
        drop(waiter_prune);
        assert!(
            manager
                .ai_connections
                .lock()
                .expect("owner registry")
                .is_empty()
        );
    }

    /// Verifies disable drain waits for admitted work and excludes late owner transitions.
    /// 验证停用排空等待已准入任务，并排除迟到的所有者转换。
    #[tokio::test]
    async fn plugin_drain_fences_transitions_before_owner_slot_creation() {
        let manager = BrowserManager::new();
        let (entered_tx, entered_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let admitted_manager = manager.clone();
        let admitted = tokio::spawn(async move {
            CoreNativeBrowserRuntime::await_admitted_owner_transition(
                admitted_manager,
                async move {
                    let _ = entered_tx.send(());
                    let _ = release_rx.await;
                    Ok::<(), String>(())
                },
            )
            .await
        });
        entered_rx.await.expect("first transition was admitted");
        manager.begin_ai_drain();
        let draining_manager = manager.clone();
        let drain = tokio::spawn(async move { draining_manager.shutdown_ai_connections().await });
        let late_calls = Arc::new(AtomicUsize::new(0));
        let late_calls_for_transition = Arc::clone(&late_calls);
        let late_manager = manager.clone();
        let late = tokio::spawn(async move {
            CoreNativeBrowserRuntime::await_admitted_owner_transition(late_manager, async move {
                late_calls_for_transition.fetch_add(1, Ordering::SeqCst);
                Ok::<(), String>(())
            })
            .await
        });
        tokio::task::yield_now().await;
        assert!(!drain.is_finished(), "drain must wait for admitted work");
        release_tx.send(()).expect("release admitted transition");
        admitted
            .await
            .expect("admitted task joined")
            .expect("admitted task finished");
        drain
            .await
            .expect("drain task joined")
            .expect("drain completed");
        assert!(late.await.expect("late task joined").is_err());
        assert_eq!(late_calls.load(Ordering::SeqCst), 0);
        manager.finish_ai_drain();
        CoreNativeBrowserRuntime::await_admitted_owner_transition(manager, async {
            Ok::<(), String>(())
        })
        .await
        .expect("published enable reopens admission");
    }

    /// Verifies caller cancellation cannot abandon a held browser authorization.
    /// 验证调用者取消不会遗弃已持有的浏览器授权。
    ///
    /// The injected operation uses the real BrowserManager lease and automation state, then
    /// pauses after attachment. The outer task is aborted while the detached transition remains
    /// responsible for the exact cleanup, which models cancellation during MCP bootstrap.
    /// 注入操作使用真实 BrowserManager 的租约和自动化状态，并在挂接后暂停。外层任务在此时中止，
    /// 脱离的转换继续负责精确清理，从而模拟 MCP 启动期间的取消。
    #[tokio::test]
    async fn cancelled_owner_transition_finishes_browser_authorization_cleanup() {
        let manager = BrowserManager::new();
        let owner = SessionId::new_v4();
        let session = manager
            .create_owned_session(&owner.to_string(), BrowserProfileMode::Ephemeral)
            .expect("create browser session");
        let slot = CoreNativeBrowserRuntime::connection_slot(&manager, &owner.to_string())
            .expect("owner connection slot");
        let (attached_tx, attached_rx) = oneshot::channel();
        let (continue_tx, continue_rx) = oneshot::channel();
        let transition_slot = slot.clone();
        let transition_manager = manager.clone();
        let browser_id = session.session_id.clone();
        let owner_text = owner.to_string();
        let transition = tokio::spawn(CoreNativeBrowserRuntime::await_owner_transition(
            async move {
                let _slot = transition_slot.lock().await;
                let (_, lease_secret) =
                    transition_manager.acquire_lease(&browser_id, "ai:cancel-test", None)?;
                transition_manager.attach_automation(
                    &browser_id,
                    &lease_secret,
                    "cancel-test-connection",
                )?;
                let _ = attached_tx.send(lease_secret.clone());
                let _ = continue_rx.await;
                transition_manager.detach_automation(&browser_id, "cancel-test-connection")?;
                transition_manager.release_lease(&browser_id, &lease_secret)?;
                Ok::<(), String>(())
            },
        ));
        let lease_secret = attached_rx.await.expect("transition attached automation");
        assert!(
            slot.try_lock().is_err(),
            "owner transition must retain its slot while cleanup is pending"
        );
        transition.abort();
        let _ = transition.await;
        let _ = continue_tx.send(());

        for _ in 0..100 {
            let projection = manager
                .session_projection(&session.session_id)
                .expect("browser projection");
            if projection.lease_state == BrowserLeaseState::Free
                && projection.automation_state == BrowserAutomationState::Detached
            {
                break;
            }
            tokio::task::yield_now().await;
        }
        let projection = manager
            .session_projection(&session.session_id)
            .expect("browser projection after cancellation");
        assert_eq!(projection.lease_state, BrowserLeaseState::Free);
        assert_eq!(
            projection.automation_state,
            BrowserAutomationState::Detached
        );
        assert_eq!(
            manager
                .unconfirmed_operations(&session.session_id)
                .expect("unconfirmed operations"),
            Vec::<String>::new()
        );
        assert_eq!(
            manager
                .takeover_lease(
                    &session.session_id,
                    &owner_text,
                    Some(Duration::from_secs(1))
                )
                .expect("a cleaned browser can be leased again")
                .0
                .lease_state,
            BrowserLeaseState::Held
        );
        assert_eq!(lease_secret.len(), 36);
    }

    /// Verifies failed gateway cleanup never downgrades an attached authorization to free.
    /// 验证网关清理失败时绝不会把已挂接授权错误降级为空闲。
    ///
    /// With no host attached, the real BrowserManager marks the close outcome uncertain. The
    /// shared failure path must preserve `blocked + attached` so a later owner shutdown can make
    /// an explicit context-closure decision instead of silently reusing the page.
    /// 在未连接宿主时，真实 BrowserManager 会把关闭结果标记为不确定。共享失败路径必须保留
    /// `blocked + attached`，让后续所有者关闭流程显式决定关闭上下文，而不是静默复用页面。
    #[tokio::test]
    async fn failed_gateway_cleanup_keeps_attached_browser_fail_closed() {
        let manager = BrowserManager::new();
        let owner = SessionId::new_v4();
        let session = manager
            .create_owned_session(&owner.to_string(), BrowserProfileMode::Ephemeral)
            .expect("create browser session");
        let (_, lease_secret) = manager
            .acquire_lease(&session.session_id, "ai:cleanup-test", None)
            .expect("acquire lease");
        manager
            .attach_automation(
                &session.session_id,
                &lease_secret,
                "cleanup-test-connection",
            )
            .expect("attach automation");

        let error = CoreNativeBrowserRuntime::release_failed_gateway(
            &manager,
            owner,
            session.session_id.clone(),
            lease_secret,
            String::from("cleanup-test-connection"),
        )
        .await
        .expect_err("missing host must make cleanup uncertain");
        assert!(error.contains("browser context closure also failed"));
        let projection = manager
            .session_projection(&session.session_id)
            .expect("browser projection");
        assert_eq!(projection.lease_state, BrowserLeaseState::Blocked);
        assert_eq!(
            projection.automation_state,
            BrowserAutomationState::Attached
        );
        assert!(
            !manager
                .unconfirmed_operations(&session.session_id)
                .expect("unconfirmed operations")
                .is_empty()
        );
    }

    /// Retire stale owner metadata after a confirmed crash without clearing browser uncertainty.
    /// 已确认崩溃后退休陈旧所有者元数据，但不清除浏览器未知结果。
    #[tokio::test]
    async fn crashed_host_does_not_poison_the_next_prompt() {
        // Set up the exact blocked and detached state reported by a crashed production host.
        // 构造生产宿主崩溃时上报的精确 blocked 与 detached 状态。
        let manager = BrowserManager::new();
        let owner = SessionId::new_v4();
        let browser = manager
            .create_owned_session(&owner.to_string(), BrowserProfileMode::Ephemeral)
            .expect("browser session");
        let (_, lease_secret) = manager
            .acquire_lease(&browser.session_id, "ai:crash-test", None)
            .expect("lease");
        manager
            .attach_automation(&browser.session_id, &lease_secret, "crashed-connection")
            .expect("attach");
        manager
            .select_ai_browser(&owner.to_string(), &browser.session_id)
            .expect("select");
        manager
            .mark_operation_unconfirmed(&browser.session_id, "automation.close outcome unknown")
            .expect("uncertain operation");
        manager
            .record_host_crash(&browser.session_id, Some(1), "confirmed host exit")
            .expect("host crash");
        // A real empty MCP registry exercises the same retirement entry used after every turn.
        // 真实的空 MCP 注册表验证每轮结束时使用的同一退休入口。
        let mcp_runtime = vulcan_agent_runtime::mcp::bootstrap_mcp_runtime_access(Vec::new())
            .await
            .expect("MCP registry");
        let slot =
            CoreNativeBrowserRuntime::connection_slot(&manager, &owner.to_string()).expect("slot");
        *slot.lock().await = Some(BrowserAiConnection {
            plugin_generation: 1,
            mcp_runtime,
            browser_id: Some(browser.session_id.clone()),
            lease_secret: Some(lease_secret),
            connection_id: Some(String::from("crashed-connection")),
            heartbeat: None,
            ready: false,
        });
        manager
            .retire_ai_connection_after_turn(owner)
            .await
            .expect("retire stale connection");
        assert!(slot.lock().await.is_none());
        assert!(
            manager
                .selected_ai_browser(&owner.to_string())
                .expect("selection")
                .is_none()
        );
        // Neither ordinary conversation recovery nor stale cleanup authorizes reuse of a crashed page.
        // 普通会话恢复和陈旧连接清理都不会授权复用崩溃页面。
        let projection = manager
            .session_projection(&browser.session_id)
            .expect("projection");
        assert_eq!(projection.status, BrowserSessionStatus::Crashed);
        assert_eq!(projection.lease_state, BrowserLeaseState::Blocked);
        assert!(projection.requires_explicit_reopen);
        assert_eq!(
            manager
                .unconfirmed_operations(&browser.session_id)
                .expect("uncertainty"),
            vec![String::from("automation.close outcome unknown")]
        );
    }
}
