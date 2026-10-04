//! Original flat removal retains package hiding, state preimages and physical cleanup ownership across uncertain effects.
//! 原平铺移除跨不确定效果保留包隐藏、状态前像及物理清理所有权。

use super::retained::{remove_plain_owned_tree, require_absent, validate_state_directories};
use super::*;
use std::fs::File;
use std::sync::{Arc, Weak};

mod deactivation;
mod flat_predecessor;
mod local_state;
pub use deactivation::{
    VulcanPluginDeactivationStateObservation, VulcanPluginDeactivationStateReservation,
};
use local_state::{
    capture_original_states, collect_original_environment, confirm_local_absence,
    freeze_cleanup_states, remove_frozen_states, retain_environment_cleanup,
    validate_cleanup_state, validate_environment_cleanup,
};

#[cfg(test)]
mod observation;
#[cfg(test)]
mod tests;

/// Actual original filesystem observations never claim a host Full commit, native drain or external-effect rollback.
/// 实际原文件系统观测绝不宣称宿主完整提交、原生排空或外部效果回滚。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginLegacyUninstallationObservation {
    /// Exact original package remains at its fixed discoverable path.
    /// 精确原包仍位于固定可发现路径。
    Original,
    /// Exact original package has moved to this request's one fixed hidden path.
    /// 精确原包已移动到此请求单个固定隐藏路径。
    Hidden,
    /// Original irreversible cleanup was admitted and may have deleted some bytes.
    /// 原不可逆清理已准入，且可能已删除部分字节。
    CleanupPending,
    /// Package, original cleanup state and environments are actually absent under retained ownership.
    /// 在保留所有权内，包、原清理状态及环境均实际缺失。
    Collected,
}

/// One exact original state path retains its pre-callback bytes separately from post-callback cleanup authority.
/// 单个精确原状态路径将回调前字节与回调后清理权威分别保留。
struct UninstallationStateFile {
    /// The host-derived fixed path selects one ordinary state file only.
    /// 宿主推导固定路径仅选择单个普通状态文件。
    path: PathBuf,
    /// Actual original bytes or absence are captured before any callback; never applied as an automatic rollback.
    /// 实际原字节或缺失在任何回调之前捕获；绝不作为自动回滚应用。
    original: Option<Vec<u8>>,
    /// None means not frozen; Some contains exact post-callback bytes or explicit absence before the sole hide intent.
    /// 空值表示未固定；非空包含唯一隐藏意图之前的精确回调后字节或明确缺失。
    cleanup: Option<Option<Vec<u8>>>,
    /// Original removal intent is stored before this path may actually disappear.
    /// 原移除意图在此路径可能实际消失之前存储。
    deletion_started: bool,
}

/// A non-cloneable, non-serializable original flat removal outlives cancelled observers and failed workers.
/// 不可克隆、不可序列化原平铺移除跨已取消观测者及失败工作者存续。
/// The host must strongly retain this object before callbacks and independently confirm Full removal before collection.
/// 宿主必须在回调之前强保留此对象，并在回收之前独立确认完整移除。
pub struct VulcanPluginLegacyUninstallationReservation {
    /// The exact original manager identity excludes another manager's publication or cleanup.
    /// 精确原管理器身份排除其他管理器的发布或清理。
    manager: Weak<super::super::VulcanPluginManagerState>,
    /// The host allocates this nonnil UUID before original ownership enters its retained slot.
    /// 宿主在原所有权进入保留槽之前分配此非空 UUID。
    request_id: Uuid,
    /// Frozen actual descriptor fixes the original flat identity and declaration inventory.
    /// 固定实际描述符固定原平铺身份及声明清单。
    source: crate::VulcanPluginDescriptor,
    /// Canonical original root bounds both discoverable and hidden package paths.
    /// 规范原根限定可发现及隐藏包路径。
    user_root: PathBuf,
    /// The sole hidden destination derives from the original UUID, never from a later random request or directory search.
    /// 唯一隐藏目标来自原 UUID，绝不来自随后随机请求或目录搜索。
    tombstone: PathBuf,
    /// All three actual original state files precede lifecycle or policy effects.
    /// 全部三个实际原状态文件先于生命周期或策略效果。
    states: Vec<UninstallationStateFile>,
    /// Same actual namespace File is shared only with the original guarded callback.
    /// 相同实际命名空间文件仅与原受保护回调共享。
    mutation: Arc<crate::VulcanPluginLegacyMutationGuard>,
    /// Sole rename intent is admitted before the actual operation can succeed or lose acknowledgement.
    /// 唯一重命名意图在实际操作能够成功或丢失确认之前准入。
    hide_started: bool,
    /// Cleanup admission precedes irreversible state, environment and hidden-package deletion.
    /// 清理准入先于不可逆状态、环境及隐藏包删除。
    cleanup_started: bool,
    /// The actual exclusive environment File and its canonical root survive every cleanup error or panic.
    /// 实际排他环境文件及其规范根跨每个清理错误或 panic 存续。
    environment_cleanup: Option<(File, PathBuf)>,
    /// Terminal completion is retained before return and requires exact absence again on recovery.
    /// 终态完成在返回之前保留，恢复时仍要求再次精确缺失。
    collected: bool,
}

impl std::fmt::Debug for VulcanPluginLegacyUninstallationReservation {
    /// Show only public request and plugin identity; formatter exposes no paths, state bytes or resource handles.
    /// 仅显示公共请求及插件身份；formatter 不暴露路径、状态字节或资源句柄。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VulcanPluginLegacyUninstallationReservation")
            .field("request_id", &self.request_id)
            .field("plugin_id", &self.source.id)
            .finish_non_exhaustive()
    }
}

impl VulcanPluginLegacyUninstallationReservation {
    /// Borrow the same host-allocated request UUID; no parameters and no completion authority is granted.
    /// 借用相同宿主分配请求 UUID；无参数，不授予完成权威。
    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    /// Borrow the frozen plugin identity; no parameters and no mutable catalog lookup occurs.
    /// 借用固定插件身份；无参数，不进行可变目录查询。
    pub fn plugin_id(&self) -> &str {
        &self.source.id
    }

    /// Borrow the original flat descriptor and declared inventory; no parameters and no new physical source is acquired.
    /// 借用原平铺描述符及声明清单；无参数，不取得新物理来源。
    pub fn original_plugin(&self) -> &crate::VulcanPluginDescriptor {
        &self.source
    }

    /// Borrow the same actual namespace guard for original callback and guarded catalog preparation.
    /// 为原回调及受保护目录准备借用相同实际命名空间守卫。
    /// No parameters; returns the retained Arc reference, never a replacement File or host publication proof.
    /// 无参数；返回保留 Arc 引用，绝不返回替代文件或宿主发布证明。
    pub fn mutation_guard(&self) -> &Arc<crate::VulcanPluginLegacyMutationGuard> {
        &self.mutation
    }

    /// Borrow actual pre-callback state preimages for explicit host recovery decisions; no parameters and no writes occur.
    /// 为明确宿主恢复决策借用实际回调前状态前像；无参数，不发生写入。
    /// Returns fixed paths and original bytes/absence without granting automatic restoration or serialization authority.
    /// 返回固定路径及原字节／缺失，不授予自动恢复或序列化权威。
    pub fn original_state_preimages(&self) -> impl Iterator<Item = (&Path, Option<&[u8]>)> {
        self.states
            .iter()
            .map(|state| (state.path.as_path(), state.original.as_deref()))
    }

    /// Validate original manager, canonical namespace and literal state parents; manager must be the reserving instance.
    /// 校验原管理器、规范命名空间及字面状态父级；manager 必须为预留实例。
    /// Returns unit without requiring that a now-hidden package remain in current routing.
    /// 返回空值，不要求现已隐藏包仍位于当前路由。
    pub(in crate::runtime) fn validate(
        &self,
        manager: &VulcanPluginRuntimeManager,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if !Weak::ptr_eq(&self.manager, &Arc::downgrade(&manager.state)) {
            return Err(VulcanPluginRuntimeError::new(
                "Original uninstallation belongs to another manager.",
            ));
        }
        self.mutation
            .validate_user_root(&manager.state.user_plugin_root, self.plugin_id())
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        validate_state_directories(manager, self.plugin_id())?;
        if self.environment_cleanup.is_some() {
            validate_environment_cleanup(manager, &self.environment_cleanup)?;
        }
        Ok(())
    }

    /// Require that actual callback and diagnostic borrowers released this same guard before package or state mutation.
    /// 包或状态变更之前，要求实际回调及诊断借用者已释放此相同守卫。
    /// No parameters; returns unit for the sole owner, without claiming the complete native/MCP union drained.
    /// 无参数；唯一所有者返回空值，不宣称完整原生／MCP 集合已排空。
    fn require_no_callback_borrowers(&self) -> Result<(), VulcanPluginRuntimeError> {
        if Arc::strong_count(&self.mutation) != 1 {
            return Err(VulcanPluginRuntimeError::new(
                "Original uninstall callback ownership remains retained.",
            ));
        }
        Ok(())
    }

    /// Require the actual current flat descriptor to remain the same disabled zero-session original before a rename.
    /// 重命名之前要求实际当前平铺描述符保持相同停用零会话原实例。
    /// manager is already protected by its registration gate; returns unit or a retained current-routing conflict.
    /// manager 已由登记门保护；返回空值或保留当前路由冲突。
    pub(in crate::runtime) fn validate_current_disabled(
        &self,
        manager: &VulcanPluginRuntimeManager,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Freeze this actual observation while the caller excludes new session registrations.
        // 调用方排除新会话登记期间，固定此实际观测。
        let snapshot = current_uninstallation_snapshot(manager)?;
        // The one exact current descriptor is required; absence is not accepted as an original source.
        // 要求单个精确当前描述符；缺失不被接受为原来源。
        let current = snapshot.plugins.get(self.plugin_id()).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Original flat package is no longer selected for hiding.")
        })?;
        if current.origin != VulcanPluginPackageOrigin::UserInstalled
            || current.managed_package().is_some()
            || current.generation != self.source.generation
            || current.version != self.source.version
            || current.content_fingerprint != self.source.content_fingerprint
            || current.package_root != self.source.package_root
            || current.runtime_state != VulcanPluginRuntimeState::Disabled
            || manager.active_session_count(self.plugin_id())? != 0
            || manager.has_cleanup_diagnostics(self.plugin_id())?
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original flat package is no longer the same disabled zero-session source.",
            ));
        }
        Ok(())
    }

    /// Verify actual original bytes at path under the fixed canonical user root; returns unit or a retained conflict.
    /// 在固定规范用户根内验证 path 处实际原字节；返回空值或保留冲突。
    fn verify_package(&self, path: &Path) -> Result<(), VulcanPluginRuntimeError> {
        require_plain_directory(path)?;
        if path
            .canonicalize()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            != path
            || path.parent() != Some(self.user_root.as_path())
            || crate::catalog::compute_package_fingerprint(path)
                .map_err(VulcanPluginRuntimeError::new)?
                != self.source.content_fingerprint
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original uninstall package bytes or canonical boundary changed.",
            ));
        }
        Ok(())
    }

    /// Observe only this original's declared source, hidden target or admitted cleanup; returns no host Full receipt.
    /// 仅观测此原实例声明来源、隐藏目标或已准入清理；不返回宿主完整回执。
    fn observe(
        &self,
    ) -> Result<VulcanPluginLegacyUninstallationObservation, VulcanPluginRuntimeError> {
        if self.cleanup_started {
            require_absent(&self.source.package_root)?;
            if self.collected {
                self.confirm_absence()?;
                return Ok(VulcanPluginLegacyUninstallationObservation::Collected);
            }
            plain_directory_presence(&self.tombstone)?;
            for state in &self.states {
                validate_cleanup_state(state)?;
            }
            return Ok(VulcanPluginLegacyUninstallationObservation::CleanupPending);
        }
        // These two exact paths are explicit protocol states, not candidate-path recovery guesses.
        // 此两个精确路径为明确协议状态，而非候选路径恢复猜测。
        let source_present = plain_directory_presence(&self.source.package_root)?;
        // Only this request's already allocated hidden destination is inspected.
        // 仅检查此请求已分配隐藏目标。
        let hidden_present = plain_directory_presence(&self.tombstone)?;
        match (source_present, hidden_present) {
            (true, false) => {
                self.verify_package(&self.source.package_root)?;
                Ok(VulcanPluginLegacyUninstallationObservation::Original)
            }
            (false, true) if self.hide_started => {
                self.verify_package(&self.tombstone)?;
                Ok(VulcanPluginLegacyUninstallationObservation::Hidden)
            }
            _ => Err(VulcanPluginRuntimeError::new(
                "Original uninstall paths have an unowned outcome; retain the same request.",
            )),
        }
    }

    /// Confirm actual package, exact state and environment absence; no parameters and no deletion is replayed.
    /// 确认实际包、精确状态及环境缺失；无参数，不重放删除。
    fn confirm_absence(&self) -> Result<(), VulcanPluginRuntimeError> {
        require_absent(&self.source.package_root)?;
        require_absent(&self.tombstone)?;
        confirm_local_absence(&self.states, self.plugin_id(), &self.environment_cleanup)
    }
}

/// Inspect literal directory presence; path must be an exact declared original and returns only plain presence or actual NotFound.
/// 检查字面目录存在性；path 必须为精确声明原路径，仅返回普通存在或实际不存在。
fn plain_directory_presence(path: &Path) -> Result<bool, VulcanPluginRuntimeError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(VulcanPluginRuntimeError::new(error.to_string())),
        Ok(metadata)
            if !crate::metadata_is_link_or_reparse_point(&metadata) && metadata.is_dir() =>
        {
            Ok(true)
        }
        Ok(_) => Err(VulcanPluginRuntimeError::new(
            "Original uninstall path is not a plain directory.",
        )),
    }
}

/// Require the exact literal ordinary directory; path is host-derived and returns unit or an explicit absence/type error.
/// 要求精确字面普通目录；path 由宿主推导，返回空值或明确缺失／类型错误。
fn require_plain_directory(path: &Path) -> Result<(), VulcanPluginRuntimeError> {
    if !plain_directory_presence(path)? {
        return Err(VulcanPluginRuntimeError::new(
            "Original uninstall directory is missing.",
        ));
    }
    Ok(())
}

impl VulcanPluginRuntimeManager {
    /// Reserve one exact disabled flat package and every actual state preimage before lifecycle or policy effects.
    /// 在生命周期或策略效果之前预留单个精确停用平铺包及每个实际状态前像。
    /// request_id is host-allocated and nonnil; plugin_id selects the real current user descriptor; returns an inert retained owner.
    /// request_id 由宿主分配且非空；plugin_id 选择真实当前用户描述符；返回未启动保留所有者。
    pub fn reserve_legacy_user_plugin_uninstallation(
        &self,
        request_id: Uuid,
        plugin_id: &str,
    ) -> Result<VulcanPluginLegacyUninstallationReservation, VulcanPluginRuntimeError> {
        if request_id.is_nil() {
            return Err(VulcanPluginRuntimeError::new(
                "Original uninstallation requires a nonnil request.",
            ));
        }
        // The actual registration gate excludes a new session before original selection and namespace reservation.
        // 实际登记门在原选择及命名空间预留之前排除新会话。
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin uninstallation gate is poisoned.")
        })?;
        self.require_routing_publication_locked(&_registration, None)?;
        // Only the current actual descriptor grants the initial flat source identity.
        // 仅当前实际描述符授予初始平铺来源身份。
        let source = current_uninstallation_snapshot(self)?
            .plugins
            .get(plugin_id)
            .cloned()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Original user package is not installed.")
            })?;
        if source.origin != VulcanPluginPackageOrigin::UserInstalled
            || source.managed_package().is_some()
            || source.runtime_state != VulcanPluginRuntimeState::Disabled
            || self.active_session_count(plugin_id)? != 0
            || self.has_cleanup_diagnostics(plugin_id)?
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original flat uninstall requires an actual disabled user package without sessions or managed selection.",
            ));
        }
        // The existing actual namespace producer rejects any managed active or tombstone record.
        // 既有实际命名空间生产者拒绝任何受管有效或停用记录。
        let store = crate::VulcanPluginGenerationStore::open(&self.state.user_plugin_root)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Acquire the same File before capturing any state or admitting any callback.
        // 在捕获任何状态或准入任何回调之前取得相同文件。
        let mutation = Arc::new(
            store
                .acquire_legacy_mutation_guard(plugin_id, None)
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?,
        );
        mutation
            .validate_user_root(&self.state.user_plugin_root, plugin_id)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        validate_state_directories(self, plugin_id)?;
        // The canonical original root supplies both exact source and hidden target boundaries.
        // 规范原根提供精确来源及隐藏目标边界。
        let user_root = self
            .state
            .user_plugin_root
            .canonicalize()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        require_plain_directory(&source.package_root)?;
        if source.package_root != user_root.join(plugin_id) {
            return Err(VulcanPluginRuntimeError::new(
                "Original flat package is outside its exact user-root child.",
            ));
        }
        // Allocate the sole intended hidden path before the first possibly uncertain rename.
        // 在首个可能不确定重命名之前分配唯一期望隐藏路径。
        let tombstone = uninstallation_tombstone(&user_root, plugin_id, request_id);
        require_absent(&tombstone)?;
        // All actual preimages, including absence, enter the same owner before any callback exists.
        // 全部实际前像，包括缺失，在任何回调存在之前进入相同所有者。
        let states = capture_original_states(self, plugin_id)?;
        // This one allocation remains live through observation, hiding and collection recovery.
        // 此单个分配跨观测、隐藏及回收恢复保持存活。
        let original = VulcanPluginLegacyUninstallationReservation {
            manager: Arc::downgrade(&self.state),
            request_id,
            source,
            user_root,
            tombstone,
            states,
            mutation,
            hide_started: false,
            cleanup_started: false,
            environment_cleanup: None,
            collected: false,
        };
        original.verify_package(&original.source.package_root)?;
        // Existing runtime roots must already be ordinary; absence explicitly permits a later original callback preparation.
        // 既有运行时根必须已经为普通目录；缺失明确允许随后原回调准备。
        plain_directory_presence(&self.state.plugin_runtime_root)?;
        Ok(original)
    }

    /// Observe the same retained flat original; returns physical outcome without performing callbacks, cleanup or publication.
    /// 观测相同保留平铺原实例；返回物理结果，不执行回调、清理或发布。
    /// original must belong to this manager and remain strongly retained by the caller.
    /// original 必须属于此管理器并由调用方继续强保留。
    pub fn observe_legacy_user_plugin_uninstallation(
        &self,
        original: &VulcanPluginLegacyUninstallationReservation,
    ) -> Result<VulcanPluginLegacyUninstallationObservation, VulcanPluginRuntimeError> {
        original.validate(self)?;
        original.observe()
    }

    /// Admit this original's sole package hide after actual callback ownership retires, freezing cleanup bytes first.
    /// 实际回调所有权退役之后准入此原实例唯一包隐藏，先固定清理字节。
    /// original is borrowed in place; returns exact hidden observation or an error retaining its same File and preimages.
    /// 原地借用 original；返回精确隐藏观测，或保留相同文件及前像的错误。
    pub fn hide_legacy_user_plugin_uninstallation(
        &self,
        original: &mut VulcanPluginLegacyUninstallationReservation,
    ) -> Result<VulcanPluginLegacyUninstallationObservation, VulcanPluginRuntimeError> {
        original.validate(self)?;
        original.require_no_callback_borrowers()?;
        // Hold the actual session gate across current-source validation and the sole rename admission.
        // 跨当前来源校验及唯一重命名准入持有实际会话门。
        let _registration = lock_original_uninstallation_gate(self, original)?;
        self.require_routing_publication_locked(
            &_registration,
            Some(
                crate::runtime::routing::ActivationRoutingOriginal::LegacyUninstallation(original),
            ),
        )?;
        original.validate_current_disabled(self)?;
        if original.hide_started {
            return Err(VulcanPluginRuntimeError::new(
                "Original hide was already admitted; observe or resume the same request.",
            ));
        }
        if original.observe()? != VulcanPluginLegacyUninstallationObservation::Original {
            return Err(VulcanPluginRuntimeError::new(
                "Original package is not available for hiding.",
            ));
        }
        // Capture all post-callback cleanup bytes before any assignment admits the actual rename.
        // 在任何赋值准入实际重命名之前，捕获全部回调后清理字节。
        freeze_cleanup_states(&mut original.states)?;
        original.hide_started = true;
        resume_hide_locked(self, original)
    }

    /// Resume only this original's admitted fixed hide; exact prior success is read-only and never selects a new UUID.
    /// 仅恢复此原实例已准入固定隐藏；精确先前成功为只读，绝不选择新 UUID。
    /// original is the retained caller allocation; returns physical hidden observation or a retained error.
    /// original 为调用方保留分配；返回物理隐藏观测或保留错误。
    pub fn resume_legacy_user_plugin_uninstallation(
        &self,
        original: &mut VulcanPluginLegacyUninstallationReservation,
    ) -> Result<VulcanPluginLegacyUninstallationObservation, VulcanPluginRuntimeError> {
        // Recovery shares the same gate and sole rename producer without reacquiring the namespace.
        // 恢复共享相同门及唯一重命名生产者，不重新取得命名空间。
        let _registration = lock_original_uninstallation_gate(self, original)?;
        self.require_routing_publication_locked(
            &_registration,
            Some(
                crate::runtime::routing::ActivationRoutingOriginal::LegacyUninstallation(original),
            ),
        )?;
        resume_hide_locked(self, original)
    }

    /// Continue the actual retained `original` using its sole existing hide intent.
    /// 使用实际保留 `original` 唯一既有隐藏意图继续处理。
    /// Returns the existing initial/resume outcome, retaining all source, namespace and callback-retirement checks.
    /// 返回既有初始／恢复结果，保留全部来源、命名空间及回调退役检查。
    pub fn continue_legacy_user_plugin_uninstallation(
        &self,
        original: &mut VulcanPluginLegacyUninstallationReservation,
    ) -> Result<VulcanPluginLegacyUninstallationObservation, VulcanPluginRuntimeError> {
        if original.hide_started {
            self.resume_legacy_user_plugin_uninstallation(original)
        } else {
            self.hide_legacy_user_plugin_uninstallation(original)
        }
    }

    /// Collect this original only after the host independently confirms full removal and actual native/MCP union drain.
    /// 仅在宿主独立确认完整移除及实际原生／MCP 集合排空之后回收此原实例。
    /// original remains borrowed across errors and panic; returns physical cleanup confirmation, never a host receipt.
    /// original 跨错误及 panic 保持借用；返回物理清理确认，绝不返回宿主回执。
    pub fn collect_legacy_user_plugin_uninstallation(
        &self,
        original: &mut VulcanPluginLegacyUninstallationReservation,
    ) -> Result<VulcanPluginLegacyUninstallationObservation, VulcanPluginRuntimeError> {
        original.validate(self)?;
        original.require_no_callback_borrowers()?;
        // The real registration gate excludes new sessions while retiring original capacities and acquiring cleanup ownership.
        // 实际登记门在退役原容量及取得清理所有权期间排除新会话。
        let registration = lock_original_uninstallation_gate(self, original)?;
        if original.collected {
            return original.observe();
        }
        if !original.cleanup_started
            && original.observe()? != VulcanPluginLegacyUninstallationObservation::Hidden
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original collection requires its verified hidden package.",
            ));
        }
        if let Some(owner) = self.formal_runtime_owner()? {
            owner.retire_plugin_capacities(original.plugin_id(), &registration)?;
        }
        retain_environment_cleanup(self, &original.source.id, &mut original.environment_cleanup)?;
        // Revalidate every exact state before admitting irreversible effects; no foreign bytes are adopted on retry.
        // 准入不可逆效果之前重新验证每个精确状态；重试时不接纳外来字节。
        for state in &original.states {
            validate_cleanup_state(state)?;
        }
        require_absent(&original.source.package_root)?;
        // Only this fixed hidden tree may be partial once actual original cleanup has started.
        // 实际原清理启动之后，仅此固定隐藏目录树可以不完整。
        if original.cleanup_started {
            plain_directory_presence(&original.tombstone)?;
        }
        // The captured environment root must remain the same actual host directory, excluding redirection after the lease.
        // 捕获环境根必须保持相同实际宿主目录，排除取得租约之后的重定向。
        validate_environment_cleanup(self, &original.environment_cleanup)?;
        original.cleanup_started = true;
        // Borrow only disjoint original fields while mutable state intents remain in the same owner.
        // 可变状态意图保持于相同所有者期间，仅借用互不重叠原字段。
        let mutation = &original.mutation;
        // The frozen descriptor supplies the sole validated cleanup identity.
        // 固定描述符提供唯一已验证清理身份。
        let plugin_id = &original.source.id;
        remove_frozen_states(&mut original.states, || {
            mutation
                .validate_user_root(&self.state.user_plugin_root, plugin_id)
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))
        })?;
        // Original environment ownership remains registered even when recursive deletion fails partway.
        // 即使递归删除中途失败，原环境所有权仍保持登记。
        collect_original_environment(self, plugin_id, &original.environment_cleanup, || {
            mutation
                .validate_user_root(&self.state.user_plugin_root, plugin_id)
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))
        })?;
        // A partially removed package is never renamed back into discovery or treated as an intact rollback.
        // 被部分移除包绝不重新重命名进入发现，也不被当成完整回滚。
        remove_plain_owned_tree(&original.tombstone, &original.user_root)?;
        original.confirm_absence()?;
        original.collected = true;
        // A real terminal receipt precedes any test-only post-success acknowledgement fault.
        // 真实终态回执先于任何仅测试成功后确认故障。
        #[cfg(test)]
        observation::checkpoint(original, observation::Checkpoint::Collected)?;
        Ok(VulcanPluginLegacyUninstallationObservation::Collected)
    }
}

/// Derive the sole hidden child from the validated original root/plugin/request; returns a path without effects or recovery ownership.
/// 从已验证原根／插件／请求推导唯一隐藏子级；返回路径，不产生效果或恢复所有权。
fn uninstallation_tombstone(user_root: &Path, plugin_id: &str, request_id: Uuid) -> PathBuf {
    user_root.join(format!(".uninstall-{plugin_id}-{request_id}"))
}

/// Finish only the same admitted hide while the caller holds the actual registration gate.
/// 调用方持有实际登记门期间，仅完成相同已准入隐藏。
/// manager validates original ownership; original is borrowed and returns exact physical outcome without replaying callbacks.
/// manager 校验原所有权；original 保持借用并返回精确物理结果，不重放回调。
fn resume_hide_locked(
    manager: &VulcanPluginRuntimeManager,
    original: &mut VulcanPluginLegacyUninstallationReservation,
) -> Result<VulcanPluginLegacyUninstallationObservation, VulcanPluginRuntimeError> {
    original.validate(manager)?;
    original.require_no_callback_borrowers()?;
    if !original.hide_started || original.cleanup_started {
        return Err(VulcanPluginRuntimeError::new(
            "Original hide recovery requires its admitted intent before cleanup.",
        ));
    }
    match original.observe()? {
        VulcanPluginLegacyUninstallationObservation::Hidden => {
            Ok(VulcanPluginLegacyUninstallationObservation::Hidden)
        }
        VulcanPluginLegacyUninstallationObservation::Original => {
            original.validate_current_disabled(manager)?;
            fs::rename(&original.source.package_root, &original.tombstone).map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Original uninstall hide failed; retain the same request: {error}"
                ))
            })?;
            // Observe only true original rename success, after ownership and intent were already retained.
            // 仅观察真实原重命名成功，先前已经保留所有权及意图。
            #[cfg(test)]
            observation::checkpoint(original, observation::Checkpoint::Hidden)?;
            original.observe()
        }
        _ => Err(VulcanPluginRuntimeError::new(
            "Original hide recovery observed an invalid phase.",
        )),
    }
}

/// Acquire the same actual sequencing-only registration mutex for explicit retained-original recovery.
/// 为明确保留原实例恢复取得相同实际仅排序登记互斥锁。
/// manager owns Mutex<()> and original proves the actual namespace and phase; returns its real guard or an explicit integrity failure.
/// manager 拥有空元组互斥锁，original 证明实际命名空间及阶段；返回真实守卫或明确完整性失败。
/// A panic can poison this unit gate without corrupting protected data; only exact original readback and borrower release permit clearing it.
/// panic 可污染此空元组门而不会损坏受保护数据；仅精确原回读及借用者释放允许清除污染。
/// Data-bearing registry and snapshot locks retain their own strict poison errors and are never repaired here.
/// 携带数据的注册表及快照锁保留各自严格污染错误，绝不在此修复。
fn lock_original_uninstallation_gate<'a>(
    manager: &'a VulcanPluginRuntimeManager,
    original: &VulcanPluginLegacyUninstallationReservation,
) -> Result<std::sync::MutexGuard<'a, ()>, VulcanPluginRuntimeError> {
    original.validate(manager)?;
    lock_uninstallation_gate(manager, || {
        original.validate(manager)?;
        original.observe()?;
        original.require_no_callback_borrowers()
    })
}

/// Reuse the actual sequencing-only gate for flat or managed retained-original recovery without repairing data locks.
/// 为平铺或受管保留原实例恢复复用实际仅排序门，不修复数据锁。
/// manager supplies Mutex<()> and validate_original proves its exact retained owner/phase; returns the actual guard or explicit error.
/// manager 提供空元组互斥锁，validate_original 证明精确保留所有者／阶段；返回实际守卫或明确错误。
fn lock_uninstallation_gate(
    manager: &VulcanPluginRuntimeManager,
    validate_original: impl FnOnce() -> Result<(), VulcanPluginRuntimeError>,
) -> Result<std::sync::MutexGuard<'_, ()>, VulcanPluginRuntimeError> {
    match manager.state.session_registration_gate.lock() {
        Ok(guard) => Ok(guard),
        Err(poisoned) => {
            // Retain the actual poisoned guard while checking this exact original; no replacement lock or fresh owner is created.
            // 检查此精确原实例期间保留实际污染守卫；不创建替代锁或新所有者。
            let guard = poisoned.into_inner();
            // The actual data-bearing catalog must still be readable; its poison is not sequencing-only and is never cleared.
            // 实际携带数据目录必须仍可读；其污染并非仅排序污染，绝不被清除。
            drop(current_uninstallation_snapshot(manager)?);
            validate_original()?;
            manager.state.session_registration_gate.clear_poison();
            Ok(guard)
        }
    }
}

/// Read the actual catalog for removal without the ordinary snapshot API's poisoned-lock observation behavior.
/// 为移除读取实际目录，不采用普通快照接口对污染锁的观测行为。
/// manager is the original host instance; returns a stable actual Arc or an explicit data-integrity error, never clears poison.
/// manager 为原宿主实例；返回稳定实际引用计数对象或明确数据完整性错误，绝不清除污染。
fn current_uninstallation_snapshot(
    manager: &VulcanPluginRuntimeManager,
) -> Result<Arc<crate::VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
    manager
        .state
        .snapshot
        .read()
        .map(|snapshot| Arc::clone(&snapshot))
        .map_err(|_| {
            VulcanPluginRuntimeError::new("Original uninstallation catalog lock is poisoned.")
        })
}
