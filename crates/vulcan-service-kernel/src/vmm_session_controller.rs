//! Core-owned registry and serialized per-session VMM controllers.
//! Core 所有的注册表与串行化的每会话 VMM 控制器。

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex},
};

use uuid::Uuid;
use vulcan_vmm::VmmSessionMemoryState;

/// Result of one serialized Core VMM controller command.
/// 单个串行化 Core VMM 控制器命令的结果。
#[derive(Debug, Clone)]
pub(crate) struct SessionVmmCommandCommit {
    /// Whether this call applied and durably committed a new command.
    /// 本次调用是否应用并持久提交了新命令。
    pub(crate) applied: bool,
    /// State observed immediately before the command, or the current state for a duplicate.
    /// 命令执行前观察到的状态；重复命令时为当前状态。
    pub(crate) previous_state: VmmSessionMemoryState,
    /// Authoritative state after the command completed.
    /// 命令完成后的权威状态。
    pub(crate) state: VmmSessionMemoryState,
}

/// Classifies controller-open failures without weakening strict VMM state validation.
/// 在不削弱严格 VMM 状态校验的前提下分类控制器打开失败。
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum SessionVmmControllerOpenError {
    /// A classified historical or damaged VMM attachment can be replaced only after explicit user authorization.
    /// 已分类的历史或损坏 VMM 附件仅可在用户明确授权后替换。
    RebuildRequired(String),
    /// Every non-recoverable registry, storage, ownership, or validation failure.
    /// 其他所有不可恢复的注册表、存储、所有权或校验失败。
    Other(String),
}

impl SessionVmmControllerOpenError {
    /// Returns whether this failure permits the isolated VMM rebuild startup action.
    /// 返回该失败是否允许执行独立 VMM 重建启动操作。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for a storage-classified historical or damaged VMM attachment.
    /// 仅对存储层已分类的历史或损坏 VMM 附件返回 `true`。
    pub(crate) fn requires_rebuild(&self) -> bool {
        matches!(self, Self::RebuildRequired(_))
    }

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
            Self::RebuildRequired(message) | Self::Other(message) => formatter.write_str(message),
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
    /// Controller-managed data root used by the storage adapter.
    /// 存储适配器使用的控制器管理数据根目录。
    data_root: PathBuf,
    /// Serialized authoritative domain state.
    /// 串行化的权威领域状态。
    state: Mutex<VmmSessionMemoryState>,
    /// Permanent lifecycle fence set before the registry releases this controller.
    /// 注册表释放该控制器前设置的永久生命周期栅栏。
    closed: AtomicBool,
}

impl SessionVmmController {
    /// Opens one controller from the strict Core-owned document or initializes a new state.
    /// 从严格的 Core 所有文档打开一个控制器，或初始化新状态。
    ///
    /// # Parameters
    /// 参数
    /// `data_root` is the controller-managed Vulcan data root.
    /// `data_root` 是控制器管理的 Vulcan 数据根目录。
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
        data_root: &Path,
        session_id: Uuid,
        updated_at: &str,
    ) -> Result<Self, SessionVmmControllerOpenError> {
        // rebuild_reason is produced only by the storage adapter's exact historical-or-damaged classifier.
        // rebuild_reason 仅由存储适配器的精确历史或损坏分类器产生。
        if let Some(rebuild_reason) =
            vulcan_workspace_store::workspace_store::core_session_vmm_rebuild_reason(
                data_root, session_id,
            )
            .map_err(SessionVmmControllerOpenError::Other)?
        {
            return Err(SessionVmmControllerOpenError::RebuildRequired(
                rebuild_reason,
            ));
        }
        // loaded_state is accepted only from the new Core controller document format.
        // loaded_state 仅接受新的 Core 控制器文档格式。
        let loaded_state = vulcan_workspace_store::workspace_store::load_core_session_vmm_state(
            data_root, session_id,
        )
        .map_err(SessionVmmControllerOpenError::Other)?;
        // state becomes the controller's sole mutable copy.
        // state 成为控制器唯一的可变副本。
        let state = match loaded_state {
            Some(state) => state,
            None => VmmSessionMemoryState::new(session_id.to_string(), updated_at)?,
        };
        Ok(Self {
            session_id,
            data_root: data_root.to_path_buf(),
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
                previous_state: state.clone(),
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
        vulcan_workspace_store::workspace_store::persist_core_session_vmm_state(
            &self.data_root,
            self.session_id,
            &candidate,
        )?;
        *guard = candidate.clone();
        Ok(SessionVmmCommandCommit {
            applied: true,
            previous_state,
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
    /// Returns the unique controller for one session, opening it exactly once on a cache miss.
    /// 返回单个会话的唯一控制器，并在缓存未命中时仅打开一次。
    ///
    /// # Parameters
    /// 参数
    /// `data_root` is the controller-managed Vulcan data root.
    /// `data_root` 是控制器管理的 Vulcan 数据根目录。
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
        data_root: &Path,
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
            if controller.data_root != data_root {
                return Err(format!(
                    "Core VMM controller data root mismatch for session `{session_id}`: registered `{}` but requested `{}`.",
                    controller.data_root.display(),
                    data_root.display()
                )
                .into());
            }
            return Ok(controller.clone());
        }
        // controller is fully loaded before publication so callers never observe partial initialization.
        // controller 在发布前完成完整加载，使调用方不会观察到部分初始化。
        let controller = Arc::new(SessionVmmController::open(
            data_root, session_id, updated_at,
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
    /// `data_root` is the storage root retained by the seeded controller.
    /// `data_root` 是播种控制器保留的存储根。
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
        data_root: &Path,
        session_id: Uuid,
        state: VmmSessionMemoryState,
    ) -> Result<(), String> {
        state.validate(&session_id.to_string())?;
        // controller carries the same production serialization and persistence behavior after seeding.
        // controller 在播种后继续携带与生产一致的串行化和持久化行为。
        let controller = Arc::new(SessionVmmController {
            session_id,
            data_root: data_root.to_path_buf(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_support::TestControllerRuntimeRoot;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
            .get_or_open(controller_root.data_root(), session_id, "1")
            .expect("open first controller");
        // second must reuse the already published registry entry.
        // second 必须复用已经发布的注册表条目。
        let second = registry
            .get_or_open(controller_root.data_root(), session_id, "2")
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
            .get_or_open(controller_root.data_root(), session_id, "3")
            .expect("reopen controller");
        assert!(!Arc::ptr_eq(&first, &reopened));
    }

    /// Verifies a historical flat VMM document is classified for explicit startup recovery.
    /// 验证历史扁平 VMM 文档会被分类为可明确执行的启动恢复。
    #[test]
    fn historical_flat_state_requires_explicit_rebuild() {
        // session_id identifies the exact historical controller owner.
        // session_id 标识精确的历史控制器所有者。
        let session_id = Uuid::new_v4();
        // controller_root owns the production-shaped isolated session directory.
        // controller_root 持有生产结构的隔离会话目录。
        let controller_root = TestControllerRuntimeRoot::new(session_id);
        // session_dir is the exact directory created by the shared test fixture.
        // session_dir 是共享测试夹具创建的精确目录。
        let session_dir = controller_root
            .data_root()
            .join("sessions")
            .join("project-a")
            .join(format!("1970-01-01-{session_id}"));
        // historical_state is the removed flat VMM document written before controller opening.
        // historical_state 是控制器打开前写入的已移除扁平 VMM 文档。
        let historical_state =
            VmmSessionMemoryState::new(session_id.to_string(), "1").expect("create flat VMM state");
        std::fs::write(
            session_dir.join("vmm.json"),
            serde_json::to_vec_pretty(&historical_state).expect("encode flat VMM state"),
        )
        .expect("write flat VMM state");
        // registry must not publish a partial controller for the classified historical state.
        // registry 不得为已分类历史状态发布部分控制器。
        let registry = SessionVmmControllerRegistry::default();

        let error = registry
            .get_or_open(controller_root.data_root(), session_id, "2")
            .expect_err("historical VMM state must require rebuild");

        assert!(error.requires_rebuild());
        assert!(error.contains("Unsupported historical VMM state format"));
        assert!(!registry.contains(session_id).expect("inspect registry"));
    }

    /// Verifies a damaged attachment is rebuilt only after classification and accepts later memory commands.
    /// 验证损坏附件仅在分类后重建，并且能够接受后续记忆命令。
    #[test]
    fn damaged_state_rebuild_accepts_later_memory_commands() {
        // session_id identifies the exact damaged VMM attachment owner.
        // session_id 标识精确的损坏 VMM 附件所有者。
        let session_id = Uuid::new_v4();
        // controller_root owns the production-shaped isolated conversation directory.
        // controller_root 持有生产结构的隔离会话目录。
        let controller_root = TestControllerRuntimeRoot::new(session_id);
        // session_dir is the exact conversation directory whose transcript stays independent.
        // session_dir 是转录保持独立的精确会话目录。
        let session_dir = controller_root
            .data_root()
            .join("sessions")
            .join("project-a")
            .join(format!("1970-01-01-{session_id}"));
        std::fs::write(
            session_dir.join("vmm.json"),
            b"{\"schema_version\":2,\"controller_kind\":",
        )
        .expect("write damaged VMM attachment");
        // registry must reject the damaged attachment without publishing a controller.
        // registry 必须拒绝损坏附件且不得发布控制器。
        let registry = SessionVmmControllerRegistry::default();
        // open_error proves storage classification reaches the controller boundary.
        // open_error 证明存储分类能够到达控制器边界。
        let open_error = registry
            .get_or_open(controller_root.data_root(), session_id, "2")
            .expect_err("damaged VMM attachment must require rebuild");

        assert!(open_error.requires_rebuild());
        assert!(open_error.contains("Damaged Core VMM attachment"));
        assert!(!registry.contains(session_id).expect("inspect registry"));
        assert!(
            vulcan_workspace_store::workspace_store::rebuild_damaged_core_session_vmm_state(
                controller_root.data_root(),
                session_id,
                "3",
            )
            .expect("rebuild damaged VMM attachment")
        );
        // controller opens only after the explicit rebuild publishes a clean current document.
        // controller 仅在明确重建发布干净当前文档后打开。
        let controller = registry
            .get_or_open(controller_root.data_root(), session_id, "4")
            .expect("open rebuilt VMM controller");
        // commit is the first durable memory command after the damaged attachment was discarded.
        // commit 是损坏附件被丢弃后的首个持久化记忆命令。
        let commit = controller
            .execute_command("post-rebuild-memory-command", "5", |_| Ok(()))
            .expect("commit post-rebuild memory command");
        // durable_state proves the rebuilt attachment remains writable through the normal controller path.
        // durable_state 证明重建附件仍可通过正常控制器路径写入。
        let durable_state = vulcan_workspace_store::workspace_store::load_core_session_vmm_state(
            controller_root.data_root(),
            session_id,
        )
        .expect("load post-rebuild VMM state")
        .expect("post-rebuild VMM state must exist");

        assert!(commit.applied);
        assert_eq!(durable_state, commit.state);
    }

    /// Verifies one registered session identity cannot be rebound to another durable data root.
    /// 验证一个已注册会话身份不能重新绑定到另一个持久数据根目录。
    #[test]
    fn registry_rejects_data_root_mismatch_for_registered_session() {
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
            .get_or_open(registered_root.data_root(), session_id, "1")
            .expect("open registered controller");
        // error must reject rebinding before any conflicting durable state is loaded or published.
        // error 必须在加载或发布任何冲突持久状态前拒绝重新绑定。
        let error = registry
            .get_or_open(conflicting_root.data_root(), session_id, "2")
            .expect_err("data root mismatch must be rejected");

        assert!(error.contains("data root mismatch"));
        assert!(error.contains(&registered_root.data_root().display().to_string()));
        assert!(error.contains(&conflicting_root.data_root().display().to_string()));
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
            .get_or_open(controller_root.data_root(), session_id, "1")
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
                .get_or_open(controller_root.data_root(), session_id, "2")
                .expect_err("replacement must be blocked")
                .contains("during a lifecycle transition")
        );

        lifecycle_fence
            .release()
            .expect("release lifecycle transition");
        // replacement is created only after the storage transition explicitly releases its fence.
        // replacement 仅在存储转换显式释放栅栏后创建。
        let replacement = registry
            .get_or_open(controller_root.data_root(), session_id, "3")
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
            .get_or_open(controller_root.data_root(), session_id, "1")
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
        assert_eq!(second.previous_state, first_state);
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
            .get_or_open(controller_root.data_root(), session_id, "1")
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
            .get_or_open(controller_root.data_root(), session_id, "99")
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
        // controller_root supplies a safe temporary parent for the deliberately missing data root.
        // controller_root 为刻意缺失的数据根提供安全的临时父目录。
        let controller_root = TestControllerRuntimeRoot::new(Uuid::new_v4());
        // missing_data_root has no segmented directory for session_id.
        // missing_data_root 不包含 session_id 的分段目录。
        let missing_data_root = controller_root.data_root().join("missing-data-root");
        // initial_state is the authoritative in-memory baseline before the failed command.
        // initial_state 是失败命令前的权威内存基线。
        let initial_state = VmmSessionMemoryState::new(session_id.to_string(), "1")
            .expect("create failure baseline");
        // registry owns a seeded production controller pointing at the missing storage location.
        // registry 持有一个指向缺失存储位置的生产控制器。
        let registry = SessionVmmControllerRegistry::default();
        registry
            .seed_for_test(&missing_data_root, session_id, initial_state.clone())
            .expect("seed failure controller");
        // controller is the exact seeded registry entry.
        // controller 是精确的已播种注册表条目。
        let controller = registry
            .get_or_open(&missing_data_root, session_id, "2")
            .expect("open seeded failure controller");

        let error = controller
            .execute_command("must-not-commit", "2", |_| Ok(()))
            .expect_err("missing session directory must reject persistence");

        assert!(error.contains("has no segmented directory"));
        assert_eq!(
            controller.snapshot().expect("read state after failure"),
            initial_state
        );
    }
}
