//! Core-owned registry and serialized per-session VMM controllers.
//! Core 所有的注册表与串行化的每会话 VMM 控制器。

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex},
};

use sha2::{Digest, Sha256};
use uuid::Uuid;
use vulcan_vmm::VmmSessionMemoryState;
use vulcan_workspace_store::workspace_store::SessionV1StorageLocator;

use crate::session_goal_lifecycle::registered_session_v1_handle_from_state;
use crate::state::SharedServiceBridgeState;

/// Result of one serialized Core VMM controller command.
/// 单个串行化 Core VMM 控制器命令的结果。
#[derive(Debug, Clone)]
pub(crate) struct SessionVmmCommandCommit {
    /// Whether this call applied and durably committed a new command.
    /// 本次调用是否应用并持久提交了新命令。
    pub(crate) applied: bool,
    /// Authoritative state after the command completed.
    /// 命令完成后的权威状态。
    pub(crate) state: VmmSessionMemoryState,
}

/// Classifies controller-open failures without weakening strict VMM state validation.
/// 在不削弱严格 VMM 状态校验的前提下分类控制器打开失败。
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum SessionVmmControllerOpenError {
    /// Every registry, storage, ownership, or validation failure.
    /// 所有注册表、存储、所有权或校验失败。
    Other(String),
}

impl SessionVmmControllerOpenError {
    /// Searches the stable human-readable diagnostic retained by this error.
    /// 搜索该错误保留的稳定人类可读诊断。
    ///
    /// # Parameters
    /// 参数
    /// `needle` is the exact diagnostic fragment to search for.
    /// `needle` 是要搜索的精确诊断片段。
    ///
    /// # Returns
    /// 返回值
    /// Whether the retained message contains the requested fragment.
    /// 返回保留消息是否包含所请求片段。
    #[cfg(test)]
    pub(crate) fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}

impl fmt::Display for SessionVmmControllerOpenError {
    /// Formats the original strict-open diagnostic without adding a fallback path.
    /// 原样格式化严格打开诊断，不添加回退路径。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Other(message) => formatter.write_str(message),
        }
    }
}

impl From<String> for SessionVmmControllerOpenError {
    /// Converts an ordinary controller-open failure into the non-recoverable variant.
    /// 将普通控制器打开失败转换为不可恢复变体。
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl From<SessionVmmControllerOpenError> for String {
    /// Preserves existing string-error boundaries outside startup recovery classification.
    /// 在启动恢复分类之外保留既有字符串错误边界。
    fn from(error: SessionVmmControllerOpenError) -> Self {
        error.to_string()
    }
}

/// Single serialized VMM state owner for one exact host session.
/// 单个精确宿主会话的唯一串行化 VMM 状态所有者。
#[derive(Debug)]
pub(crate) struct SessionVmmController {
    /// Exact session identity owned by this controller.
    /// 该控制器拥有的精确会话身份。
    session_id: Uuid,
    /// Exact V1 storage locator used by the storage adapter.
    /// 存储适配器使用的精确 V1 存储定位器。
    storage_locator: SessionV1StorageLocator,
    /// Serialized authoritative domain state.
    /// 串行化的权威领域状态。
    state: Mutex<VmmSessionMemoryState>,
    /// Permanent lifecycle fence set before the registry releases this controller.
    /// 注册表释放该控制器前设置的永久生命周期栅栏。
    closed: AtomicBool,
}

impl SessionVmmController {
    /// Reconciles the cache with the exact V1 generation, compaction and admitted memory source.
    /// 根据准确 V1 代次、压缩及准入记忆来源协调缓存。
    /// Returns the current durable state; repeated unchanged transitions do not append receipts.
    /// 返回当前持久状态；重复且无变化的转换不追加回执。
    pub(crate) fn reconcile_context_epoch(
        &self,
        context: &vulcan_vmm::VmmContextEpoch,
        source: &vulcan_vmm::VmmMemoryReadSource,
        updated_at: &str,
    ) -> Result<VmmSessionMemoryState, String> {
        // Use the committed numeric identity, not a workspace mirror or message position.
        // 使用已提交数字身份，而非工作区镜像或消息位置。
        let epoch_id = context.cache_epoch_id();
        // No canonical user is invented; the pending baseline remains request-local until a real user arrives.
        // 不虚构规范用户；等待基线在真实用户到来前保持请求局部内容。
        let transition = |candidate: &mut VmmSessionMemoryState| -> Result<(), String> {
            candidate.select_read_source(&self.session_id.to_string(), Some(source), updated_at)?;
            candidate.synchronize_stable_anchor_rendering(
                vulcan_vmm::VmmRuntimeFeaturePolicy::from_env().stable_anchors_enabled,
                updated_at,
            )?;
            candidate.synchronize_context_epoch(context, updated_at)?;
            Ok(())
        };
        // Detect a no-op using a detached observation, then recompute under the unique writer if necessary.
        // 用脱离观察检测无操作，必要时在唯一写入锁内重新计算。
        let before = self.snapshot()?;
        let mut observed = before.clone();
        transition(&mut observed)?;
        if observed == before {
            return Ok(before);
        }
        // Revision distinguishes reconciliation after later source changes while the V1 compaction stays the same.
        // 修订号区分 V1 压缩相同但之后来源改变的再次对账。
        let command_id = format!("vmm_context_epoch:{epoch_id}:{}", before.revision);
        Ok(self
            .execute_command(&command_id, updated_at, transition)?
            .state)
    }

    /// Persists a source selection independently of an already committed provider-boundary receipt.
    /// 独立于已经提交的 Provider 边界回执持久化来源选择。
    /// Takes the host's current optional read source and timestamp; returns the durable selected state.
    /// 接收宿主当前可选读取来源与时间戳；返回已持久化的选择状态。
    /// Exact no-ops create no receipt; changed state is recomputed under the controller's existing commit lock.
    /// 准确无操作不创建回执；变化状态在控制器既有提交锁内重新计算。
    pub(crate) fn reconcile_read_source(
        &self,
        source: Option<&vulcan_vmm::VmmMemoryReadSource>,
        updated_at: &str,
    ) -> Result<VmmSessionMemoryState, String> {
        // Read the current controller, not the detached state returned by the original provider receipt.
        // 读取当前控制器，而不是原 Provider 回执返回的脱离状态。
        let before = self.snapshot()?;
        // This private projection decides only whether a new durable command is necessary.
        // 此私有投影仅用于判断是否需要新的持久命令。
        let mut observed = before.clone();
        if !observed.select_read_source(&self.session_id.to_string(), source, updated_at)? {
            return Ok(before);
        }
        // Include the requested source as well as revision so distinct concurrent selections cannot share a receipt.
        // 同时包含请求来源与版本，避免不同并发选择共享同一回执。
        let source_digest = hex::encode(Sha256::digest(
            serde_json::to_vec(&source).map_err(|error| error.to_string())?,
        ));
        // Only an opaque digest enters the bounded command identity; no transport credential or long scope list is stored.
        // 有界命令身份只包含不透明摘要，不保存传输凭据或长范围列表。
        let command_id = format!(
            "vmm_read_source_reconcile:{}:{source_digest}",
            before.revision
        );
        // Recompute on the locked candidate to preserve intervening terminal counters and controller receipts.
        // 在持锁候选上重新计算，保留期间发生的终态计数及控制器回执。
        let committed = self.execute_command(&command_id, updated_at, |candidate| {
            candidate.select_read_source(&self.session_id.to_string(), source, updated_at)?;
            Ok(())
        })?;
        Ok(committed.state)
    }

    /// Opens one controller from the strict Core-owned document or initializes a new state.
    /// 从严格的 Core 所有文档打开一个控制器，或初始化新状态。
    ///
    /// # Parameters
    /// 参数
    /// `storage_locator` is the exact SQLite-derived V1 session locator.
    /// `storage_locator` 是由 SQLite 精确派生的 V1 会话定位器。
    /// `session_id` is the exact session identity owned by the new controller.
    /// `session_id` 是新控制器拥有的精确会话身份。
    /// `updated_at` is the timestamp used only when no durable state exists.
    /// `updated_at` 仅在不存在持久状态时用于初始化时间。
    ///
    /// # Returns
    /// 返回值
    /// A ready controller or an explicit strict-load error.
    /// 返回就绪控制器或明确的严格加载错误。
    fn open(
        storage_locator: SessionV1StorageLocator,
        session_id: Uuid,
        updated_at: &str,
    ) -> Result<Self, SessionVmmControllerOpenError> {
        if storage_locator.session_id() != session_id.to_string() {
            return Err(SessionVmmControllerOpenError::Other(format!(
                "V1 VMM locator owner mismatch: expected `{session_id}`, found `{}`.",
                storage_locator.session_id()
            )));
        }
        let loaded_state =
            vulcan_workspace_store::workspace_store::load_session_vmm_state(&storage_locator)
                .map_err(SessionVmmControllerOpenError::Other)?;
        // state becomes the controller's sole mutable copy.
        // state 成为控制器唯一的可变副本。
        let state = match loaded_state {
            Some(state) => state,
            None => VmmSessionMemoryState::new(session_id.to_string(), updated_at)?,
        };
        Ok(Self {
            session_id,
            storage_locator,
            state: Mutex::new(state),
            closed: AtomicBool::new(false),
        })
    }

    /// Returns one immutable clone of the current authoritative state.
    /// 返回当前权威状态的一份不可变副本。
    ///
    /// # Returns
    /// 返回值
    /// The validated controller-owned state snapshot.
    /// 返回已校验的控制器所有状态快照。
    pub(crate) fn snapshot(&self) -> Result<VmmSessionMemoryState, String> {
        // guard serializes the observation with every command commit.
        // guard 将本次观察与每个命令提交串行化。
        let guard = self
            .state
            .lock()
            .map_err(|error| format!("Core VMM controller state lock failed: {error}"))?;
        if self.closed.load(Ordering::SeqCst) {
            return Err(format!(
                "Core VMM controller for session `{}` is closed.",
                self.session_id
            ));
        }
        Ok(guard.clone())
    }

    /// Executes one idempotent command and commits memory only after durable publication succeeds.
    /// 执行一个幂等命令，并且仅在持久发布成功后提交内存状态。
    ///
    /// # Parameters
    /// 参数
    /// `command_id` is the stable session-scoped command identity.
    /// `command_id` 是稳定的会话级命令身份。
    /// `updated_at` is the exact command timestamp.
    /// `updated_at` 是精确的命令时间戳。
    /// `mutator` applies one domain transition to an isolated candidate.
    /// `mutator` 向隔离候选状态应用一次领域转换。
    ///
    /// # Returns
    /// 返回值
    /// The committed before-and-after snapshots, or an error with no in-memory candidate commit.
    /// 返回已提交的变更前后快照；失败时不会提交内存候选状态。
    pub(crate) fn execute_command<F>(
        &self,
        command_id: &str,
        updated_at: &str,
        mutator: F,
    ) -> Result<SessionVmmCommandCommit, String>
    where
        F: FnOnce(&mut VmmSessionMemoryState) -> Result<(), String>,
    {
        if command_id.trim().is_empty() {
            return Err(String::from(
                "Core VMM controller command id must not be empty.",
            ));
        }
        // guard is the complete per-session serialization boundary, including durable publication.
        // guard 是完整的每会话串行边界，包含持久发布过程。
        let mut guard = self
            .state
            .lock()
            .map_err(|error| format!("Core VMM controller state lock failed: {error}"))?;
        if self.closed.load(Ordering::SeqCst) {
            return Err(format!(
                "Core VMM controller for session `{}` is closed.",
                self.session_id
            ));
        }
        if guard.has_committed_controller_command(command_id) {
            // state is returned without running the mutator, making retries side-effect free.
            // state 在不运行变更器的情况下返回，使重试不产生副作用。
            let state = guard.clone();
            return Ok(SessionVmmCommandCommit {
                applied: false,
                state,
            });
        }
        // previous_state remains authoritative until the candidate is durably published.
        // previous_state 在候选状态持久发布前始终保持权威。
        let previous_state = guard.clone();
        // candidate is the isolated transaction workspace for this command.
        // candidate 是本次命令使用的隔离事务工作区。
        let mut candidate = previous_state.clone();
        // revision_before_command lets the receipt share an existing domain revision when one changed.
        // revision_before_command 使领域变更发生时命令回执共享其现有 revision。
        let revision_before_command = candidate.revision;
        mutator(&mut candidate)?;
        candidate.record_committed_controller_command(
            command_id,
            revision_before_command,
            updated_at,
        )?;
        candidate.validate(&self.session_id.to_string())?;
        vulcan_workspace_store::workspace_store::persist_session_vmm_state(
            &self.storage_locator,
            &candidate,
        )?;
        *guard = candidate.clone();
        Ok(SessionVmmCommandCommit {
            applied: true,
            state: candidate,
        })
    }

    /// Waits for any in-flight command and permanently rejects later calls through stale references.
    /// 等待所有在途命令完成，并永久拒绝旧引用发起的后续调用。
    ///
    /// # Returns
    /// 返回值
    /// Success after the lifecycle fence is installed under the command lock.
    /// 在命令锁内安装生命周期栅栏后返回成功。
    fn close(&self) -> Result<(), String> {
        // guard waits for the exact session's current command and excludes every later command.
        // guard 等待当前会话的现有命令，并排斥所有后续命令。
        let _guard = self
            .state
            .lock()
            .map_err(|error| format!("Core VMM controller state lock failed: {error}"))?;
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// Process-wide registry guaranteeing one VMM controller instance per active session.
/// 进程级注册表，保证每个活动会话只有一个 VMM 控制器实例。
#[derive(Clone, Default)]
pub(crate) struct SessionVmmControllerRegistry {
    /// Shared controller and lifecycle-fence state protected by one atomic registry boundary.
    /// 由同一个原子注册表边界保护的共享控制器与生命周期栅栏状态。
    state: Arc<Mutex<SessionVmmControllerRegistryState>>,
}

/// Mutable process-wide VMM registry state.
/// 可变的进程级 VMM 注册表状态。
#[derive(Default)]
struct SessionVmmControllerRegistryState {
    /// Controllers keyed by their exact host session identity.
    /// 按精确宿主会话身份索引的控制器。
    controllers: BTreeMap<Uuid, Arc<SessionVmmController>>,
    /// Sessions temporarily blocked from reopening during filesystem lifecycle transitions.
    /// 在文件系统生命周期转换期间暂时禁止重新打开的会话。
    lifecycle_fences: BTreeSet<Uuid>,
}

/// RAII fence preventing session controllers from reopening during a storage lifecycle transition.
/// 在存储生命周期转换期间阻止会话控制器重新打开的 RAII 栅栏。
pub(crate) struct SessionVmmLifecycleFence {
    /// Registry that owns the fenced session identities.
    /// 持有被栅栏化会话身份的注册表。
    registry: SessionVmmControllerRegistry,
    /// Exact session identities blocked by this fence.
    /// 被该栅栏阻止的精确会话身份。
    session_ids: Vec<Uuid>,
    /// Whether explicit release already completed.
    /// 显式释放是否已经完成。
    released: bool,
}

impl SessionVmmLifecycleFence {
    /// Explicitly releases every lifecycle fence after the storage transition and retained projection finish.
    /// 在存储转换与保留投影完成后显式释放全部生命周期栅栏。
    ///
    /// # Returns
    /// 返回值
    /// Success after every session can be opened again.
    /// 在每个会话均可再次打开后返回成功。
    pub(crate) fn release(mut self) -> Result<(), String> {
        self.registry
            .release_lifecycle_fences(self.session_ids.as_slice())?;
        self.released = true;
        Ok(())
    }
}

impl Drop for SessionVmmLifecycleFence {
    /// Releases lifecycle fences on every early-return path.
    /// 在所有提前返回路径上释放生命周期栅栏。
    fn drop(&mut self) {
        if self.released {
            return;
        }
        if let Err(error) = self
            .registry
            .release_lifecycle_fences(self.session_ids.as_slice())
        {
            eprintln!("[webui-service] failed to release Core VMM lifecycle fences: {error}");
        }
    }
}

impl SessionVmmControllerRegistry {
    /// Visits resident controllers under the lifecycle lock without opening or leaking owned handles.
    /// 在生命周期锁内访问驻留控制器，不打开或泄漏拥有所有权的句柄。
    /// Takes a synchronous exact-ID visitor; returns its results or the first explicit error.
    /// 接收同步准确身份访问器；返回其结果或首个明确错误。
    pub(crate) fn map_resident<T, F>(&self, mut visit: F) -> Result<BTreeMap<Uuid, T>, String>
    where
        F: FnMut(Uuid, &SessionVmmController) -> Result<T, String>,
    {
        // Lifecycle removal cannot close a controller until its synchronous visit has completed.
        // 生命周期移除在同步访问完成前不能关闭控制器。
        let guard = self.state.lock().map_err(|error| error.to_string())?;
        guard
            .controllers
            .iter()
            .map(|(session_id, controller)| {
                visit(*session_id, controller).map(|result| (*session_id, result))
            })
            .collect()
    }

    /// Clears derived memory sources in every resident controller when the global memory mode changes.
    /// 全局记忆模式变化时清除全部驻留控制器的派生记忆来源。
    /// Takes a mutation timestamp; retains durable receipts and never opens cold or fenced sessions.
    /// 接收修改时间戳；保留持久回执，绝不打开冷会话或栅栏中的会话。
    pub(crate) fn clear_resident_memory_sources(&self, updated_at: &str) -> Result<(), String> {
        // Serialize with lifecycle removal through each durable per-session command.
        // 在每个持久会话命令期间与生命周期移除保持串行。
        let guard = self.state.lock().map_err(|error| error.to_string())?;
        for controller in guard.controllers.values() {
            controller.reconcile_read_source(None, updated_at)?;
        }
        Ok(())
    }

    /// Reconciles native sources only for already resident controllers selected by exact host session IDs.
    /// 仅对准确宿主会话 ID 选中的已驻留控制器对账原生来源。
    /// Takes the selected IDs, current native binding and timestamp; returns snapshots without opening cold owners.
    /// 接收所选 ID、当前原生绑定和时间戳；返回快照，不打开冷会话归属。
    pub(crate) fn reconcile_resident_native_sources(
        &self,
        session_ids: &BTreeSet<Uuid>,
        source: Option<&vulcan_vmm::VmmMemoryReadSource>,
        updated_at: &str,
    ) -> Result<BTreeMap<Uuid, VmmSessionMemoryState>, String> {
        if let Some(source) = source {
            source.validate().map_err(|error| error.to_string())?;
            if !matches!(source.owner, vulcan_vmm::VmmMemorySource::Native { .. }) {
                return Err("Native cache reconciliation requires a native source.".to_owned());
            }
        }
        // Keep lifecycle removal ordered through every mutation; never retain a controller after this guard.
        // 使生命周期移除与每次修改保持有序；不在守卫结束后保留控制器。
        let guard = self.state.lock().map_err(|error| error.to_string())?;
        // Only resident entries can contribute snapshots; fenced and cold sessions remain unopened.
        // 只有驻留项提供快照；栅栏中及冷会话保持未打开。
        let mut snapshots = BTreeMap::new();
        for session_id in session_ids {
            if let Some(controller) = guard.controllers.get(session_id) {
                // External caches belong to another backend and must survive native binding maintenance.
                // 外置缓存属于另一后端，原生绑定维护不能改变它们。
                let before = controller.snapshot()?;
                let current = if before.read_source.as_ref().is_some_and(|source| {
                    matches!(source.owner, vulcan_vmm::VmmMemorySource::Native { .. })
                }) {
                    controller.reconcile_read_source(source, updated_at)?
                } else {
                    before
                };
                snapshots.insert(*session_id, current);
            }
        }
        Ok(snapshots)
    }

    /// Returns the unique controller for one session, opening it exactly once on a cache miss.
    /// 返回单个会话的唯一控制器，并在缓存未命中时仅打开一次。
    ///
    /// # Parameters
    /// 参数
    /// `storage_locator` is the exact SQLite-derived V1 session locator.
    /// `storage_locator` 是由 SQLite 精确派生的 V1 会话定位器。
    /// `session_id` is the exact requested session identity.
    /// `session_id` 是精确的请求会话身份。
    /// `updated_at` is used only for clean first initialization.
    /// `updated_at` 仅用于干净的首次初始化。
    ///
    /// # Returns
    /// 返回值
    /// The single shared controller instance or a strict open error.
    /// 返回唯一共享控制器实例或严格打开错误。
    pub(crate) fn get_or_open(
        &self,
        storage_locator: SessionV1StorageLocator,
        session_id: Uuid,
        updated_at: &str,
    ) -> Result<Arc<SessionVmmController>, SessionVmmControllerOpenError> {
        // guard makes the existence check and insertion one atomic registry operation.
        // guard 使存在性检查与插入成为一次原子注册表操作。
        let mut guard = self
            .state
            .lock()
            .map_err(|error| format!("Core VMM controller registry lock failed: {error}"))?;
        if guard.lifecycle_fences.contains(&session_id) {
            return Err(format!(
                "Core VMM controller for session `{session_id}` cannot open during a lifecycle transition."
            )
            .into());
        }
        if let Some(controller) = guard.controllers.get(&session_id) {
            if controller.storage_locator != storage_locator {
                return Err(format!(
                    "Core VMM controller V1 locator mismatch for session `{session_id}`: registered `{}` but requested `{}`.",
                    controller.storage_locator.project_directory().display(),
                    storage_locator.project_directory().display()
                )
                .into());
            }
            return Ok(controller.clone());
        }
        // controller is fully loaded before publication so callers never observe partial initialization.
        // controller 在发布前完成完整加载，使调用方不会观察到部分初始化。
        let controller = Arc::new(SessionVmmController::open(
            storage_locator,
            session_id,
            updated_at,
        )?);
        guard.controllers.insert(session_id, controller.clone());
        Ok(controller)
    }

    /// Fences exact sessions against controller reopening for one storage lifecycle transition.
    /// 为一次存储生命周期转换栅栏化精确会话，阻止控制器重新打开。
    ///
    /// # Parameters
    /// 参数
    /// `session_ids` are the exact sessions whose controllers and directories are transitioning.
    /// `session_ids` 是控制器与目录正在转换的精确会话集合。
    ///
    /// # Returns
    /// 返回值
    /// An RAII fence that must remain alive through storage mutation and retained-state refresh.
    /// 返回一个 RAII 栅栏，必须存活至存储变更与保留状态刷新完成。
    pub(crate) fn begin_lifecycle_transition<I>(
        &self,
        session_ids: I,
    ) -> Result<SessionVmmLifecycleFence, String>
    where
        I: IntoIterator<Item = Uuid>,
    {
        // unique_session_ids normalize duplicate project membership before registry mutation.
        // unique_session_ids 在注册表变更前规范化重复的项目成员关系。
        let unique_session_ids = session_ids.into_iter().collect::<BTreeSet<_>>();
        if unique_session_ids.is_empty() {
            return Ok(SessionVmmLifecycleFence {
                registry: self.clone(),
                session_ids: Vec::new(),
                released: false,
            });
        }
        // guard atomically publishes every fence before waiting for in-flight controller commands.
        // guard 在等待在途控制器命令前原子发布全部栅栏。
        let mut guard = self
            .state
            .lock()
            .map_err(|error| format!("Core VMM controller registry lock failed: {error}"))?;
        if let Some(session_id) = unique_session_ids
            .iter()
            .find(|session_id| guard.lifecycle_fences.contains(session_id))
        {
            return Err(format!(
                "Core VMM lifecycle transition is already active for session `{session_id}`."
            ));
        }
        guard
            .lifecycle_fences
            .extend(unique_session_ids.iter().copied());
        // existing_controllers are cloned so rollback can mutate the registry maps safely.
        // existing_controllers 会被克隆，使回滚能够安全修改注册表映射。
        let existing_controllers = unique_session_ids
            .iter()
            .filter_map(|session_id| {
                guard
                    .controllers
                    .get(session_id)
                    .cloned()
                    .map(|controller| (*session_id, controller))
            })
            .collect::<Vec<_>>();
        for (session_id, controller) in existing_controllers {
            if let Err(error) = controller.close() {
                for rollback_session_id in &unique_session_ids {
                    guard.lifecycle_fences.remove(rollback_session_id);
                    guard.controllers.remove(rollback_session_id);
                }
                return Err(format!(
                    "Failed to fence Core VMM controller for session `{session_id}`: {error}"
                ));
            }
        }
        // closed controllers leave the projection map immediately while lifecycle_fences still block replacements.
        // 已关闭控制器立即离开投影映射，同时 lifecycle_fences 继续阻止替代实例。
        for session_id in &unique_session_ids {
            guard.controllers.remove(session_id);
        }
        Ok(SessionVmmLifecycleFence {
            registry: self.clone(),
            session_ids: unique_session_ids.into_iter().collect(),
            released: false,
        })
    }

    /// Releases exact lifecycle fences and removes their permanently closed controller instances.
    /// 释放精确生命周期栅栏，并移除对应已永久关闭的控制器实例。
    ///
    /// # Parameters
    /// 参数
    /// `session_ids` are the exact identities previously fenced by this registry.
    /// `session_ids` 是此前由该注册表栅栏化的精确身份。
    ///
    /// # Returns
    /// 返回值
    /// Success after every identity can be opened through a fresh controller.
    /// 在每个身份均可通过新控制器打开后返回成功。
    fn release_lifecycle_fences(&self, session_ids: &[Uuid]) -> Result<(), String> {
        // guard makes closed-controller removal and reopening permission one atomic transition.
        // guard 使已关闭控制器移除与重新打开许可成为一次原子转换。
        let mut guard = self
            .state
            .lock()
            .map_err(|error| format!("Core VMM controller registry lock failed: {error}"))?;
        for session_id in session_ids {
            guard.controllers.remove(session_id);
            guard.lifecycle_fences.remove(session_id);
        }
        Ok(())
    }

    /// Returns immutable snapshots for every currently open session controller.
    /// 返回当前全部已打开会话控制器的不可变快照。
    ///
    /// # Returns
    /// 返回值
    /// Controller-owned state clones keyed by exact session identity.
    /// 返回按精确会话身份索引的控制器所有状态副本。
    pub(crate) fn snapshots(&self) -> Result<BTreeMap<Uuid, VmmSessionMemoryState>, String> {
        // guard keeps lifecycle removal ordered with every projected controller snapshot.
        // guard 使生命周期移除与每个控制器投影快照保持有序。
        let guard = self
            .state
            .lock()
            .map_err(|error| format!("Core VMM controller registry lock failed: {error}"))?;
        guard
            .controllers
            .iter()
            .map(|(session_id, controller)| controller.snapshot().map(|state| (*session_id, state)))
            .collect()
    }

    /// Returns whether one session currently has an open controller.
    /// 返回某个会话当前是否拥有已打开控制器。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the exact session identity to inspect.
    /// `session_id` 是需要检查的精确会话身份。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the registry currently owns the session controller.
    /// 当注册表当前拥有该会话控制器时返回 `true`。
    #[cfg(test)]
    pub(crate) fn contains(&self, session_id: Uuid) -> Result<bool, String> {
        // guard serializes the observation with open and close.
        // guard 将本次观察与打开和关闭操作串行化。
        let guard = self
            .state
            .lock()
            .map_err(|error| format!("Core VMM controller registry lock failed: {error}"))?;
        Ok(guard.controllers.contains_key(&session_id))
    }

    /// Seeds one in-memory controller state for an isolated service-kernel test.
    /// 为隔离的 service-kernel 测试播种一份内存控制器状态。
    ///
    /// # Parameters
    /// 参数
    /// `storage_locator` is the exact storage locator retained by the seeded controller.
    /// `storage_locator` 是播种控制器保留的精确存储定位器。
    /// `session_id` is the exact controller owner.
    /// `session_id` 是精确的控制器所有者。
    /// `state` is the validated authoritative test state.
    /// `state` 是经过验证的权威测试状态。
    ///
    /// # Returns
    /// 返回值
    /// Success after replacing the exact test registry entry.
    /// 在替换精确的测试注册表条目后返回成功。
    #[cfg(test)]
    pub(crate) fn seed_for_test(
        &self,
        storage_locator: SessionV1StorageLocator,
        session_id: Uuid,
        state: VmmSessionMemoryState,
    ) -> Result<(), String> {
        state.validate(&session_id.to_string())?;
        // controller carries the same production serialization and persistence behavior after seeding.
        // controller 在播种后继续携带与生产一致的串行化和持久化行为。
        let controller = Arc::new(SessionVmmController {
            session_id,
            storage_locator,
            state: Mutex::new(state),
            closed: AtomicBool::new(false),
        });
        // guard makes replacement atomic with concurrent test registry access.
        // guard 使替换与并发测试注册表访问保持原子性。
        let mut guard = self
            .state
            .lock()
            .map_err(|error| format!("Core VMM controller registry lock failed: {error}"))?;
        guard.controllers.insert(session_id, controller);
        Ok(())
    }
}

/// Opens the exact V1-owned VMM controller for one already hydrated session.
/// 为一个已经完成 hydration 的会话打开由 V1 持有的精确 VMM 控制器。
///
/// # Parameters
/// 参数
/// `state` owns both the exact V1 session registry and the VMM controller registry.
/// `state` 同时持有精确 V1 会话注册表与 VMM 控制器注册表。
/// `session_id` is the exact hydrated session owner.
/// `session_id` 是精确的已 hydration 会话所有者。
/// `updated_at` initializes a cleanly absent VMM document only.
/// `updated_at` 仅用于初始化干净缺失的 VMM 文档。
///
/// # Returns
/// 返回值
/// The unique process-local VMM controller bound to the session's V1 locator.
/// 返回绑定到该会话 V1 定位器的唯一进程内 VMM 控制器。
pub(crate) fn open_registered_session_vmm_controller(
    state: &SharedServiceBridgeState,
    session_id: Uuid,
    updated_at: &str,
) -> Result<Arc<SessionVmmController>, String> {
    let handle = registered_session_v1_handle_from_state(state, session_id)?;
    let storage_locator = handle
        .storage_locator()
        .map_err(|error| format!("Failed to resolve V1 VMM locator for `{session_id}`: {error}"))?;
    let registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.vmm_controller_registry.clone()
    };
    registry
        .get_or_open(storage_locator, session_id, updated_at)
        .map_err(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_support::TestControllerRuntimeRoot;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use vulcan_agent_runtime::agent_runtime::vmm_hooks::{
        VmmHookConfigSnapshot, VmmHookRuntimeHandle,
    };

    /// Proves binding maintenance changes only selected resident native owners and respects lifecycle fences.
    /// 证明绑定维护只修改所选驻留原生归属，并遵守生命周期栅栏。
    /// Uses isolated V1 directories; verifies durable reset, no-op saves, cold exclusion and external preservation.
    /// 使用隔离 V1 目录；验证持久重置、无操作保存、冷会话排除及外置保留。
    #[test]
    fn resident_native_binding_reconciliation_is_scoped_and_lifecycle_safe() {
        // The IDs distinguish selected, unrelated, external, fenced and never-opened sessions.
        // ID 区分所选、无关、外置、栅栏中及从未打开的会话。
        let ids = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];
        let roots = ids[..4]
            .iter()
            .map(|id| TestControllerRuntimeRoot::new(*id))
            .collect::<Vec<_>>();
        let registry = SessionVmmControllerRegistry::default();
        let old = vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::native(1).expect("owner"),
            vec![1],
        )
        .expect("source");
        let next = vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::native(2).expect("owner"),
            vec![2, 3],
        )
        .expect("source");
        let remote = vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::external(
                &vulcan_vmm::VmmTransportConfig::from_effective(
                    &vulcan_vmm::VmmEffectiveConfig::resolve(
                        &vulcan_vmm::VmmConfigLayer {
                            grpc_target: Some("http://127.0.0.1:9000".to_owned()),
                            ..Default::default()
                        },
                        None,
                        None,
                    ),
                ),
                1,
                1,
            )
            .expect("remote owner"),
            Vec::new(),
        )
        .expect("remote source");
        // Seed through the same durable writer used by normal recall; no sidecar is written directly.
        // 通过正常召回使用的同一持久写入器播种；不直接写 sidecar。
        let controllers = roots
            .iter()
            .enumerate()
            .map(|(index, root)| {
                let controller = registry
                    .get_or_open(root.v1_storage_locator(), ids[index], "1")
                    .expect("open");
                controller
                    .execute_command("seed", "2", |candidate| {
                        candidate.select_read_source(
                            &ids[index].to_string(),
                            Some(if index == 2 { &remote } else { &old }),
                            "2",
                        )?;
                        candidate.apply_recall_batch(
                            &ids[index].to_string(),
                            "user-1",
                            &vulcan_vmm::VmmRecallBatch {
                                ttl_turns: 3,
                                injection_tag: "vmm".to_owned(),
                                injection_description: "memory".to_owned(),
                                items: vec![vulcan_vmm::ContextItem {
                                    memory_id: 11,
                                    text: "resident body".to_owned(),
                                    ..Default::default()
                                }],
                            },
                            "2",
                        )?;
                        Ok(())
                    })
                    .expect("seed cache");
                controller
            })
            .collect::<Vec<_>>();
        let before = registry.snapshots().expect("before");
        let fence = registry
            .begin_lifecycle_transition([ids[3]])
            .expect("fence");
        let selected = BTreeSet::from([ids[0], ids[2], ids[3], ids[4]]);
        let unchanged = registry
            .reconcile_resident_native_sources(&selected, Some(&old), "3")
            .expect("same source");
        assert_eq!(unchanged.get(&ids[0]), before.get(&ids[0]));
        let changed = registry
            .reconcile_resident_native_sources(&selected, Some(&next), "4")
            .expect("rebind");
        let current = changed.get(&ids[0]).expect("selected");
        assert_eq!(current.read_source.as_ref(), Some(&next));
        assert!(current.active_leases.is_empty());
        assert!(current.cache_epoch.anchors.is_empty());
        assert!(current.has_committed_controller_command("seed"));
        assert_eq!(changed.get(&ids[2]), before.get(&ids[2]));
        assert_eq!(
            controllers[1].snapshot().expect("unrelated"),
            before[&ids[1]]
        );
        assert!(!registry.contains(ids[3]).expect("fenced remains closed"));
        assert!(!registry.contains(ids[4]).expect("cold remains unopened"));
        assert_eq!(changed.len(), 2);
        assert!(
            registry
                .reconcile_resident_native_sources(&selected, Some(&remote), "5")
                .is_err()
        );
        // Closing and reopening proves the new binding is durable, without reviving old recalled bodies.
        // 关闭并重开证明新绑定已持久化，且不会复活旧召回正文。
        registry
            .begin_lifecycle_transition([ids[0]])
            .expect("close selected")
            .release()
            .expect("release selected");
        let reopened = registry
            .get_or_open(roots[0].v1_storage_locator(), ids[0], "6")
            .expect("reopen");
        assert_eq!(reopened.snapshot().expect("durable"), *current);
        let unbound = registry
            .reconcile_resident_native_sources(&selected, None, "7")
            .expect("unbind");
        assert!(unbound[&ids[0]].read_source.is_none());
        assert_eq!(unbound.get(&ids[2]), before.get(&ids[2]));
        fence.release().expect("release fence");
    }

    /// Rejects late source changes before either a new durable command or replay reconciliation runs.
    /// 在新持久命令或重放协调运行前拒绝迟到的来源变化。
    /// Uses private V1 storage and the production Hook guard; returns after disk and receipt invariants hold.
    /// 使用私有 V1 存储和生产 Hook 守卫；磁盘及回执不变量成立后返回。
    #[test]
    fn publication_guard_rejects_changed_source_before_durable_mutation() {
        // Each case owns a separate controller tree and configuration projection.
        // 每个场景拥有独立控制器目录树和配置投影。
        for scenario in [
            "project",
            "user",
            "endpoint",
            "credential",
            "disabled",
            "backend",
        ] {
            // The session is the same stable identity passed to both Hook and storage.
            // 会话是同时传给 Hook 与存储的同一稳定身份。
            let session_id = Uuid::new_v4();
            // Only this test accesses the private storage tree.
            // 仅本测试访问私有存储目录树。
            let controller_root = TestControllerRuntimeRoot::new(session_id);
            // The registry remains the sole durable writer for this session.
            // 注册表始终是本会话唯一持久写入方。
            let registry = SessionVmmControllerRegistry::default();
            // Open before taking the Hook read lock, matching production lock ordering.
            // 在取得 Hook 读锁前打开控制器，与生产锁顺序一致。
            let controller = registry
                .get_or_open(controller_root.v1_storage_locator(), session_id, "1")
                .expect("controller");
            // Explicit remote identity is required; timeout does not participate in source identity.
            // 必须声明准确远端身份；超时不参与来源身份。
            let mut layer = vulcan_vmm::VmmConfigLayer {
                enabled: Some(true),
                backend_mode: Some(vulcan_vmm::VmmBackendMode::ExternalGrpc),
                grpc_target: Some("127.0.0.1:50051".to_owned()),
                user_id: Some(1),
                project_id: Some(7),
                ..Default::default()
            };
            // This is the source admitted before the delayed preparation completed.
            // 这是迟到准备完成前已准入的来源。
            let admitted = vulcan_vmm::VmmMemoryReadSource::new(
                vulcan_vmm::VmmMemorySource::external(
                    &vulcan_vmm::VmmTransportConfig::from_effective(
                        &vulcan_vmm::VmmEffectiveConfig::resolve(&layer, None, None),
                    ),
                    1,
                    7,
                )
                .expect("owner"),
                Vec::new(),
            )
            .expect("source");
            // The real Hook projection governs the same guard used around production durable commits.
            // 真实 Hook 投影控制生产持久提交所使用的同一守卫。
            let hooks = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot {
                global: layer.clone(),
                // A retained local enable override must not reactivate the globally stopped service.
                // 保留的本地启用覆盖不能重新激活已全局停用的服务。
                projects_by_workspace: BTreeMap::from([(
                    "D:/project".to_owned(),
                    vulcan_vmm::VmmConfigLayer {
                        enabled: Some(true),
                        ..Default::default()
                    },
                )]),
                ..Default::default()
            });
            hooks
                .with_current_profile_publication(session_id, "D:/project", &admitted, || {
                    controller.execute_command("provider_boundary:original", "2", |candidate| {
                        candidate.select_read_source(
                            &session_id.to_string(),
                            Some(&admitted),
                            "2",
                        )?;
                        Ok(())
                    })
                })
                .expect("current source")
                .expect("original commit");
            // Preserve the complete authority, including the original idempotent receipt.
            // 保留完整权威状态，包括原始幂等回执。
            let before = controller.snapshot().expect("baseline");
            match scenario {
                "project" => layer.project_id = Some(8),
                "user" => layer.user_id = Some(2),
                "endpoint" => layer.grpc_target = Some("127.0.0.1:50052".to_owned()),
                "credential" => layer.grpc_api_key = Some("synthetic-new-key".to_owned()),
                "disabled" => layer.enabled = Some(false),
                "backend" => layer.backend_mode = Some(vulcan_vmm::VmmBackendMode::Native),
                _ => unreachable!("all declared cases are handled"),
            }
            hooks
                .replace_global_projection(&layer)
                .expect("change configuration");
            // Both closures would persist a mutation if the guard did not reject the stale source first.
            // 如果守卫未先拒绝过期来源，两个闭包都会持久化变更。
            for replay in [false, true] {
                // Count actual callback admission, not just the returned error.
                // 统计实际回调准入，而非仅检查返回错误。
                let mut invoked = false;
                assert!(
                    hooks
                        .with_current_profile_publication(
                            session_id,
                            "D:/project",
                            &admitted,
                            || {
                                invoked = true;
                                if replay {
                                    controller.reconcile_read_source(None, "3").map(|_| ())
                                } else {
                                    controller
                                        .execute_command(
                                            "provider_boundary:late",
                                            "3",
                                            |candidate| {
                                                candidate.select_read_source(
                                                    &session_id.to_string(),
                                                    None,
                                                    "3",
                                                )?;
                                                Ok(())
                                            },
                                        )
                                        .map(|_| ())
                                }
                            },
                        )
                        .is_err(),
                    "{scenario}, replay={replay}"
                );
                assert!(!invoked, "{scenario}, replay={replay}");
                assert_eq!(controller.snapshot().expect("unchanged"), before);
            }
            // Closing and reopening independently verifies no stale command reached disk.
            // 关闭并重开，独立验证没有过期命令写入磁盘。
            registry
                .begin_lifecycle_transition([session_id])
                .expect("fence")
                .release()
                .expect("release");
            assert_eq!(
                registry
                    .get_or_open(controller_root.v1_storage_locator(), session_id, "4")
                    .expect("reopen")
                    .snapshot()
                    .expect("durable state"),
                before
            );
        }
    }

    /// Verifies a repeated provider receipt cannot preserve a foreign durable cache after remote rebinding.
    /// 验证重复 Provider 回执不能在远端重绑后保留其他来源持久缓存。
    /// Uses a private real V1 controller, source switch and reopen; returns after durable no-op and lineage checks.
    /// 使用私有真实 V1 控制器、来源切换及重开；完成持久无操作和沿革检查后返回。
    #[test]
    fn source_reconciliation_persists_after_provider_receipt_replay() {
        // Only this test owns the private session tree and its registry.
        // 仅本测试拥有私有会话目录树及其注册表。
        let session_id = Uuid::new_v4();
        // The fixture uses the same locator-based storage as production.
        // 夹具使用与生产相同的定位器存储。
        let controller_root = TestControllerRuntimeRoot::new(session_id);
        // One registry controls persistence and the later lifecycle-safe reopen.
        // 同一注册表控制持久化和后续生命周期安全重开。
        let registry = SessionVmmControllerRegistry::default();
        // Open the real per-session controller.
        // 打开真实每会话控制器。
        let controller = registry
            .get_or_open(controller_root.v1_storage_locator(), session_id, "1")
            .expect("controller");
        // Remote scope policy stays server-owned; only transport identity and exact IDs participate here.
        // 远端范围策略仍由服务端持有；此处仅涉及传输身份和准确 ID。
        let transport =
            vulcan_vmm::VmmTransportConfig::from_effective(&vulcan_vmm::VmmEffectiveConfig {
                grpc_target: "127.0.0.1:50051".to_owned(),
                grpc_api_key: Some("synthetic-source-credential".to_owned()),
                ..Default::default()
            });
        // The original source owns the provider-boundary receipt and its cached memory.
        // 原始来源拥有 Provider 边界回执及其缓存记忆。
        let original_source = vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::external(&transport, 1, 7).expect("original owner"),
            Vec::new(),
        )
        .expect("original source");
        // The next source deliberately shares the same service and user, changing only project ownership.
        // 下一来源刻意共享同一服务和用户，仅改变项目归属。
        let next_source = vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::external(&transport, 1, 8).expect("next owner"),
            Vec::new(),
        )
        .expect("next source");
        controller
            .execute_command("provider_boundary:turn-1", "2", |candidate| {
                candidate.select_read_source(
                    &session_id.to_string(),
                    Some(&original_source),
                    "2",
                )?;
                candidate.apply_recall_batch(
                    &session_id.to_string(),
                    "user-1",
                    &vulcan_vmm::VmmRecallBatch {
                        ttl_turns: 3,
                        injection_tag: "vmm".to_owned(),
                        injection_description: "memory".to_owned(),
                        items: vec![vulcan_vmm::ContextItem {
                            memory_id: 11,
                            text: "old-remote-body".to_owned(),
                            ..Default::default()
                        }],
                    },
                    "2",
                )?;
                candidate.register_active_turn("turn-1", "2")?;
                candidate.consume_turn("turn-1", "3")?;
                Ok(())
            })
            .expect("original cache");
        // Replaying must still skip its mutator and return the original source, proving a separate command is required.
        // 重放仍必须跳过变更器并返回原来源，证明需要独立命令。
        let replay = controller
            .execute_command("provider_boundary:turn-1", "4", |_| {
                panic!("must not replay mutation")
            })
            .expect("replay");
        assert!(!replay.applied);
        assert_eq!(replay.state.read_source.as_ref(), Some(&original_source));
        assert_eq!(replay.state.active_leases.len(), 1);
        // Call exactly the helper used by the external duplicate-boundary production branch.
        // 调用外置重复边界生产分支使用的准确辅助方法。
        let changed = controller
            .reconcile_read_source(Some(&next_source), "4")
            .expect("source reconciliation");
        assert_eq!(changed.read_source.as_ref(), Some(&next_source));
        assert!(changed.active_leases.is_empty());
        assert!(changed.cache_epoch.anchors.is_empty());
        assert!(changed.pending_prompt_events.is_empty());
        assert_eq!(
            changed.completed_turn_sequence,
            replay.state.completed_turn_sequence
        );
        assert_eq!(
            changed.last_consumed_turn_id,
            replay.state.last_consumed_turn_id
        );
        assert!(changed.has_committed_controller_command("provider_boundary:turn-1"));
        assert_eq!(changed.committed_command_hashes.len(), 2);
        assert_eq!(
            controller
                .reconcile_read_source(Some(&next_source), "5")
                .expect("no-op"),
            changed
        );
        assert!(
            !serde_json::to_string(&changed)
                .expect("serialized state")
                .contains("synthetic-source-credential")
        );
        registry
            .begin_lifecycle_transition([session_id])
            .expect("fence")
            .release()
            .expect("release");
        // Reopening proves the invalidation was not limited to the transient request-building copy.
        // 重开证明失效并未局限于临时请求构建副本。
        let reopened = registry
            .get_or_open(controller_root.v1_storage_locator(), session_id, "6")
            .expect("reopen");
        assert_eq!(reopened.snapshot().expect("durable snapshot"), changed);
        // Switching back must not resurrect old content or delete the old provider receipt.
        // 切回不能复活旧内容，也不能删除旧 Provider 回执。
        let returned = reopened
            .reconcile_read_source(Some(&original_source), "7")
            .expect("return source");
        assert!(returned.active_leases.is_empty());
        assert!(returned.cache_epoch.anchors.is_empty());
        assert!(returned.has_committed_controller_command("provider_boundary:turn-1"));
        assert_eq!(returned.committed_command_hashes.len(), 3);
    }

    /// Verifies every lookup for one session returns the exact same controller instance.
    /// 验证同一会话的每次查找都返回完全相同的控制器实例。
    #[test]
    fn registry_returns_one_controller_instance_per_session() {
        // session_id identifies the exact registry key under test.
        // session_id 标识被测的精确注册表键。
        let session_id = Uuid::new_v4();
        // controller_root owns a valid isolated segmented session directory.
        // controller_root 持有一个有效的隔离分段会话目录。
        let controller_root = TestControllerRuntimeRoot::new(session_id);
        // registry is the process-local owner under test.
        // registry 是被测的进程内所有者。
        let registry = SessionVmmControllerRegistry::default();

        // first is the controller created by the initial lookup.
        // first 是首次查找创建的控制器。
        let first = registry
            .get_or_open(controller_root.v1_storage_locator(), session_id, "1")
            .expect("open first controller");
        // second must reuse the already published registry entry.
        // second 必须复用已经发布的注册表条目。
        let second = registry
            .get_or_open(controller_root.v1_storage_locator(), session_id, "2")
            .expect("open second controller");

        assert!(Arc::ptr_eq(&first, &second));
        assert!(registry.contains(session_id).expect("inspect registry"));
        // lifecycle_fence closes and removes the controller while blocking a replacement.
        // lifecycle_fence 关闭并移除控制器，同时阻止替代实例。
        let lifecycle_fence = registry
            .begin_lifecycle_transition([session_id])
            .expect("fence controller");
        assert!(
            first
                .snapshot()
                .expect_err("stale controller reference must be fenced")
                .contains("is closed")
        );
        lifecycle_fence.release().expect("release controller fence");
        // reopened is the only active replacement after the old instance was permanently fenced.
        // reopened 是旧实例被永久栅栏化后的唯一活动替代实例。
        let reopened = registry
            .get_or_open(controller_root.v1_storage_locator(), session_id, "3")
            .expect("reopen controller");
        assert!(!Arc::ptr_eq(&first, &reopened));
    }

    /// Persists the exact compaction epoch without retaining old position anchors or consuming a user turn.
    /// 持久保存准确压缩纪元，不保留旧位置锚点，也不消费用户回合。
    /// Uses a real private cache document and verifies restart, idempotency and source replacement.
    /// 使用真实私有缓存文档，验证重启、幂等及来源替换。
    #[test]
    fn compaction_epoch_reconciliation_is_durable_and_source_exact() {
        // This root contains the validated V1 locator used by the production cache owner.
        // 此根包含生产缓存归属方使用的已验证 V1 定位器。
        let session_id = Uuid::new_v4();
        let root = TestControllerRuntimeRoot::new(session_id);
        let registry = SessionVmmControllerRegistry::default();
        let controller = registry
            .get_or_open(root.v1_storage_locator(), session_id, "1")
            .expect("controller");
        // The native owner identity is fixed explicitly; display names never participate.
        // 显式固定原生归属身份；显示名称不参与判断。
        let source = vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::native(1).expect("owner"),
            vec![1],
        )
        .expect("source");
        controller
            .execute_command("original-recall", "1", |candidate| {
                candidate.synchronize_stable_anchor_rendering(true, "1")?;
                candidate.select_read_source(&session_id.to_string(), Some(&source), "1")?;
                candidate.apply_recall_batch(
                    &session_id.to_string(),
                    "old-position",
                    &vulcan_vmm::VmmRecallBatch {
                        ttl_turns: 3,
                        injection_tag: String::from("vmm"),
                        injection_description: String::from("memory"),
                        items: vec![vulcan_vmm::ContextItem {
                            memory_id: 7,
                            text: String::from("active-body"),
                            ..Default::default()
                        }],
                    },
                    "1",
                )?;
                Ok(())
            })
            .expect("seed active cache");
        // This fixture supplies the same explicit lineage captured from the V1 owner in production.
        // 此夹具提供与生产从 V1 归属方捕获时相同的明确沿革。
        let context = vulcan_vmm::VmmContextEpoch::new(1, Some("9".to_owned())).expect("context");
        let compacted = controller
            .reconcile_context_epoch(&context, &source, "2")
            .expect("epoch transition");
        assert_eq!(
            compacted.cache_epoch.covered_compaction_id.as_deref(),
            Some("9")
        );
        assert!(compacted.cache_epoch.anchors.is_empty());
        assert_eq!(compacted.active_leases.len(), 1);
        assert!(
            compacted
                .pending_active_baseline_prefix()
                .expect("pending baseline")
                .contains("active-body")
        );
        assert_eq!(compacted.completed_turn_sequence, 0);
        assert!(compacted.has_committed_controller_command("original-recall"));
        assert_eq!(
            controller
                .reconcile_context_epoch(&context, &source, "3")
                .expect("idempotent"),
            compacted
        );
        registry
            .begin_lifecycle_transition([session_id])
            .expect("close")
            .release()
            .expect("release");
        let reopened = registry
            .get_or_open(root.v1_storage_locator(), session_id, "4")
            .expect("restart");
        assert_eq!(reopened.snapshot().expect("durable"), compacted);
        reopened
            .execute_command("bind-before-edit", "4", |candidate| {
                candidate.bind_pending_state_to_user("same-position", "4")?;
                Ok(())
            })
            .expect("bind before edit");
        // A rewind reuses message positions but advances generation and may remove compaction altogether.
        // 回退复用消息位置但推进代次，并可能完全移除压缩。
        let edited_context = vulcan_vmm::VmmContextEpoch::new(2, None).expect("edited context");
        let edited = reopened
            .reconcile_context_epoch(&edited_context, &source, "5")
            .expect("edit");
        assert!(edited.cache_epoch.anchors.is_empty());
        assert!(edited.cache_epoch.covered_compaction_id.is_none());
        assert_eq!(edited.cache_epoch.epoch_id, edited_context.cache_epoch_id());
        assert!(
            edited
                .pending_active_baseline_prefix()
                .expect("edited baseline")
                .contains("active-body")
        );
        assert!(edited.has_committed_controller_command("original-recall"));
        assert_eq!(
            reopened
                .reconcile_context_epoch(&edited_context, &source, "5")
                .expect("duplicate edit"),
            edited
        );
        // A later rebind under the same compaction cannot keep the previous owner's baseline.
        // 同一压缩后的再次绑定不能保留前归属方基线。
        let other_source = vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::native(2).expect("other owner"),
            vec![2],
        )
        .expect("other source");
        let changed = reopened
            .reconcile_context_epoch(&edited_context, &other_source, "6")
            .expect("new source");
        assert!(changed.active_leases.is_empty());
        assert!(changed.pending_active_baseline_prefix().is_none());
        assert!(changed.cache_epoch.covered_compaction_id.is_none());
        assert_eq!(
            changed.cache_epoch.epoch_id,
            edited_context.cache_epoch_id()
        );
        assert_eq!(changed.read_source.as_ref(), Some(&other_source));
    }

    /// Verifies an obsolete flat VMM document is rejected without a rebuild path.
    /// 验证废弃的扁平 VMM 文档会被拒绝且不存在重建路径。
    #[test]
    fn obsolete_flat_state_is_rejected_without_rebuild() {
        // session_id identifies the exact historical controller owner.
        // session_id 标识精确的历史控制器所有者。
        let session_id = Uuid::new_v4();
        // controller_root owns the production-shaped isolated session directory.
        // controller_root 持有生产结构的隔离会话目录。
        let controller_root = TestControllerRuntimeRoot::new(session_id);
        // session_dir is the exact directory created by the shared test fixture.
        // session_dir 是共享测试夹具创建的精确目录。
        let session_dir = controller_root.v1_session_directory();
        // historical_state is the removed flat VMM document written before controller opening.
        // historical_state 是控制器打开前写入的已移除扁平 VMM 文档。
        let historical_state =
            VmmSessionMemoryState::new(session_id.to_string(), "1").expect("create flat VMM state");
        std::fs::write(
            session_dir.join("vmm.json"),
            serde_json::to_vec_pretty(&historical_state).expect("encode flat VMM state"),
        )
        .expect("write flat VMM state");
        // registry must not publish a partial controller for an unsupported document.
        // registry 不得为不受支持的文档发布部分控制器。
        let registry = SessionVmmControllerRegistry::default();

        let error = registry
            .get_or_open(controller_root.v1_storage_locator(), session_id, "2")
            .expect_err("obsolete VMM state must be rejected");

        assert!(error.contains("Failed to decode strict V1 session VMM document"));
        assert!(!registry.contains(session_id).expect("inspect registry"));
    }

    /// Verifies a damaged VMM document is reported without mutation or automatic repair.
    /// 验证损坏的 VMM 文档会直接报告，且不会发生变更或自动修复。
    #[test]
    fn damaged_state_is_reported_without_mutation() {
        // session_id identifies the exact damaged VMM attachment owner.
        // session_id 标识精确的损坏 VMM 附件所有者。
        let session_id = Uuid::new_v4();
        // controller_root owns the production-shaped isolated conversation directory.
        // controller_root 持有生产结构的隔离会话目录。
        let controller_root = TestControllerRuntimeRoot::new(session_id);
        // session_dir is the exact conversation directory whose transcript stays independent.
        // session_dir 是转录保持独立的精确会话目录。
        let session_dir = controller_root.v1_session_directory();
        // damaged is retained so the test can prove strict open never rewrites it.
        // damaged 被保留，用于证明严格打开绝不会重写它。
        let damaged = b"{\"schema_version\":1,\"document_kind\":";
        std::fs::write(session_dir.join("vmm.json"), damaged)
            .expect("write damaged VMM attachment");
        // registry must reject the damaged attachment without publishing a controller.
        // registry 必须拒绝损坏附件且不得发布控制器。
        let registry = SessionVmmControllerRegistry::default();
        // open_error proves strict storage validation reaches the controller boundary.
        // open_error 证明严格存储校验能够到达控制器边界。
        let open_error = registry
            .get_or_open(controller_root.v1_storage_locator(), session_id, "2")
            .expect_err("damaged VMM attachment must fail strict open");

        assert!(open_error.contains("Failed to decode strict V1 session VMM document"));
        assert!(!registry.contains(session_id).expect("inspect registry"));
        assert_eq!(
            std::fs::read(session_dir.join("vmm.json")).expect("read unchanged damage"),
            damaged
        );
    }

    /// Verifies one registered session identity cannot be rebound to another durable data root.
    /// 验证一个已注册会话身份不能重新绑定到另一个持久数据根目录。
    #[test]
    fn registry_rejects_session_directory_mismatch_for_registered_session() {
        // session_id identifies the single registry identity whose storage ownership is fixed.
        // session_id 标识存储所有权固定的唯一注册表身份。
        let session_id = Uuid::new_v4();
        // registered_root owns the durable state used for the first controller publication.
        // registered_root 持有首次发布控制器所使用的持久状态。
        let registered_root = TestControllerRuntimeRoot::new(session_id);
        // conflicting_root provides a distinct valid data root for the same session identity.
        // conflicting_root 为同一个会话身份提供另一个不同且有效的数据根目录。
        let conflicting_root = TestControllerRuntimeRoot::new(session_id);
        // registry owns the process-local session-to-controller identity mapping.
        // registry 持有进程内会话到控制器的身份映射。
        let registry = SessionVmmControllerRegistry::default();

        registry
            .get_or_open(registered_root.v1_storage_locator(), session_id, "1")
            .expect("open registered controller");
        // error must reject rebinding before any conflicting durable state is loaded or published.
        // error 必须在加载或发布任何冲突持久状态前拒绝重新绑定。
        let error = registry
            .get_or_open(conflicting_root.v1_storage_locator(), session_id, "2")
            .expect_err("session directory mismatch must be rejected");

        assert!(error.contains("V1 locator mismatch"));
        assert!(
            error.contains(
                &registered_root
                    .v1_storage_locator()
                    .project_directory()
                    .display()
                    .to_string()
            )
        );
        assert!(
            error.contains(
                &conflicting_root
                    .v1_storage_locator()
                    .project_directory()
                    .display()
                    .to_string()
            )
        );
    }

    /// Verifies a lifecycle fence blocks stale and replacement controllers until explicit release.
    /// 验证生命周期栅栏会在显式释放前阻止旧控制器与替代控制器。
    #[test]
    fn lifecycle_fence_blocks_reopen_until_storage_transition_finishes() {
        // session_id identifies the exact registry entry under lifecycle transition.
        // session_id 标识进入生命周期转换的精确注册表条目。
        let session_id = Uuid::new_v4();
        // controller_root owns the isolated durable session directory.
        // controller_root 拥有隔离的持久会话目录。
        let controller_root = TestControllerRuntimeRoot::new(session_id);
        // registry owns the only controller and lifecycle fence state.
        // registry 持有唯一控制器与生命周期栅栏状态。
        let registry = SessionVmmControllerRegistry::default();
        // original is retained to prove stale references remain permanently closed.
        // original 被保留，以证明旧引用会保持永久关闭。
        let original = registry
            .get_or_open(controller_root.v1_storage_locator(), session_id, "1")
            .expect("open original controller");
        // lifecycle_fence remains alive for the simulated storage transition.
        // lifecycle_fence 在模拟存储转换期间保持存活。
        let lifecycle_fence = registry
            .begin_lifecycle_transition([session_id])
            .expect("begin lifecycle transition");

        assert!(
            original
                .snapshot()
                .expect_err("stale reference must be closed")
                .contains("is closed")
        );
        assert!(
            registry
                .get_or_open(controller_root.v1_storage_locator(), session_id, "2")
                .expect_err("replacement must be blocked")
                .contains("during a lifecycle transition")
        );

        lifecycle_fence
            .release()
            .expect("release lifecycle transition");
        // replacement is created only after the storage transition explicitly releases its fence.
        // replacement 仅在存储转换显式释放栅栏后创建。
        let replacement = registry
            .get_or_open(controller_root.v1_storage_locator(), session_id, "3")
            .expect("open replacement controller");
        assert!(!Arc::ptr_eq(&original, &replacement));
    }

    /// Verifies duplicate command identities never rerun the mutator or advance state.
    /// 验证重复命令身份不会重新运行变更器或推进状态。
    #[test]
    fn duplicate_command_id_is_side_effect_free() {
        // session_id identifies the exact controller state under test.
        // session_id 标识被测的精确控制器状态。
        let session_id = Uuid::new_v4();
        // controller_root owns a valid isolated segmented session directory.
        // controller_root 持有一个有效的隔离分段会话目录。
        let controller_root = TestControllerRuntimeRoot::new(session_id);
        // controller is the sole state owner for both attempts.
        // controller 是两次尝试的唯一状态所有者。
        let controller = SessionVmmControllerRegistry::default()
            .get_or_open(controller_root.v1_storage_locator(), session_id, "1")
            .expect("open controller");
        // mutation_count proves the duplicate path does not invoke its closure.
        // mutation_count 证明重复路径不会调用其闭包。
        let mutation_count = Arc::new(AtomicUsize::new(0));
        // first_count is retained by the first command closure.
        // first_count 由第一次命令闭包持有。
        let first_count = mutation_count.clone();
        let first = controller
            .execute_command("command-a", "2", move |_| {
                first_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .expect("commit first command");
        // first_state is the durable baseline that the duplicate must preserve exactly.
        // first_state 是重复命令必须精确保留的持久基线。
        let first_state = first.state.clone();
        // second_count would expose an incorrect duplicate mutator invocation.
        // second_count 会暴露错误的重复变更器调用。
        let second_count = mutation_count.clone();
        let second = controller
            .execute_command("command-a", "3", move |_| {
                second_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .expect("deduplicate second command");

        assert!(first.applied);
        assert!(!second.applied);
        assert_eq!(mutation_count.load(Ordering::SeqCst), 1);
        assert_eq!(second.state, first_state);
    }

    /// Verifies concurrent commands serialize into one monotonic durable lineage.
    /// 验证并发命令会串行化为一条单调的持久谱系。
    #[test]
    fn concurrent_commands_commit_one_serial_lineage() {
        // session_id identifies the controller shared by every worker.
        // session_id 标识全部工作线程共享的控制器。
        let session_id = Uuid::new_v4();
        // controller_root owns the shared durable session directory.
        // controller_root 持有共享的持久会话目录。
        let controller_root = TestControllerRuntimeRoot::new(session_id);
        // registry supplies the one process-wide controller instance.
        // registry 提供唯一的进程级控制器实例。
        let registry = SessionVmmControllerRegistry::default();
        // controller is cloned into every worker without cloning its authoritative state.
        // controller 被克隆到每个工作线程，但不会克隆其权威状态。
        let controller = registry
            .get_or_open(controller_root.v1_storage_locator(), session_id, "1")
            .expect("open concurrent controller");
        // worker_count defines the exact number of unique serialized commands.
        // worker_count 定义唯一串行命令的精确数量。
        let worker_count = 16_u64;
        // workers execute unique no-op domain commands whose receipts still advance revision.
        // workers 执行唯一的空领域命令，其回执仍会推进 revision。
        let workers = (0..worker_count)
            .map(|index| {
                let controller = controller.clone();
                std::thread::spawn(move || {
                    controller
                        .execute_command(
                            &format!("concurrent-command-{index}"),
                            &format!("{}", index + 2),
                            |_| Ok(()),
                        )
                        .expect("commit concurrent command")
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            assert!(worker.join().expect("join concurrent command").applied);
        }
        // committed is the single authoritative state after every worker joined.
        // committed 是全部工作线程结束后的单一权威状态。
        let committed = controller.snapshot().expect("read concurrent state");

        assert_eq!(committed.revision, worker_count + 1);
        assert_eq!(
            committed.committed_command_hashes.len(),
            worker_count as usize
        );
        // lifecycle_fence closes the current instance before durable reload.
        // lifecycle_fence 在持久重新加载前关闭当前实例。
        let lifecycle_fence = registry
            .begin_lifecycle_transition([session_id])
            .expect("fence controller");
        lifecycle_fence.release().expect("release controller fence");
        // reloaded proves the same serialized lineage was durably published.
        // reloaded 证明同一串行谱系已经持久发布。
        let reloaded = registry
            .get_or_open(controller_root.v1_storage_locator(), session_id, "99")
            .expect("reload concurrent controller")
            .snapshot()
            .expect("read reloaded concurrent state");
        assert_eq!(reloaded, committed);
    }

    /// Verifies a storage failure cannot publish the isolated candidate into controller memory.
    /// 验证存储失败无法把隔离候选发布到控制器内存。
    #[test]
    fn persistence_failure_preserves_authoritative_memory() {
        // session_id identifies the controller whose backing directory is deliberately absent.
        // session_id 标识其后端目录被刻意省略的控制器。
        let session_id = Uuid::new_v4();
        // controller_root supplies a valid locator whose exact V1 tree is then removed.
        // controller_root 提供一个有效定位器，随后移除其精确 V1 目录树。
        let controller_root = TestControllerRuntimeRoot::new(session_id);
        let storage_locator = controller_root.v1_storage_locator();
        let missing_session_directory = controller_root.v1_session_directory().to_path_buf();
        std::fs::remove_dir_all(&missing_session_directory)
            .expect("remove exact V1 session directory");
        // initial_state is the authoritative in-memory baseline before the failed command.
        // initial_state 是失败命令前的权威内存基线。
        let initial_state = VmmSessionMemoryState::new(session_id.to_string(), "1")
            .expect("create failure baseline");
        // registry owns a seeded production controller pointing at the missing storage location.
        // registry 持有一个指向缺失存储位置的生产控制器。
        let registry = SessionVmmControllerRegistry::default();
        registry
            .seed_for_test(storage_locator.clone(), session_id, initial_state.clone())
            .expect("seed failure controller");
        // controller is the exact seeded registry entry.
        // controller 是精确的已播种注册表条目。
        let controller = registry
            .get_or_open(storage_locator, session_id, "2")
            .expect("open seeded failure controller");

        let error = controller
            .execute_command("must-not-commit", "2", |_| Ok(()))
            .expect_err("missing session directory must reject persistence");

        assert!(error.contains("metadata.json"), "unexpected error: {error}");
        assert!(
            error.contains(&controller_root.data_root().display().to_string())
                && error.contains(&session_id.to_string()),
            "unexpected error: {error}"
        );
        assert_eq!(
            controller.snapshot().expect("read state after failure"),
            initial_state
        );
        // A failed durable source transition must not publish the private observed projection in memory.
        // 持久来源转换失败时不能把私有观察投影发布到内存。
        let source = vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::native(7).expect("owner"),
            vec![7],
        )
        .expect("source");
        assert!(
            controller
                .reconcile_read_source(Some(&source), "3")
                .expect_err("source persistence must fail")
                .contains("metadata.json")
        );
        assert_eq!(
            controller.snapshot().expect("state after source failure"),
            initial_state
        );
    }
}
