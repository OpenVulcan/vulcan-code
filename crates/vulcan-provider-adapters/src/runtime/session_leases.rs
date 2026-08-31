//! Immutable session-route lease ownership and reversible revocation guards.
//! 不可变会话路由租约所有权与可逆撤销 Guard。

use super::{PublicAiRuntime, ResolvedPublicAiSelection};
use std::collections::BTreeMap;

/// Maximum immutable session route leases retained by one runtime process.
/// 单个运行时进程允许保留的不可变会话路由租约最大数量。
const SESSION_SELECTION_LEASE_MAX_ENTRIES: usize = 4_096;
/// Maximum UTF-8 bytes accepted for one session route lease key.
/// 单个会话路由租约键允许接受的 UTF-8 字节最大数量。
const SESSION_SELECTION_LEASE_ID_MAX_BYTES: usize = 1_024;

/// Opaque proof that a session set owns no process-local provider route lease.
/// 证明一组会话不持有任何进程内供应商路由租约的不透明凭据。
///
/// The write guard remains held until this value is dropped, preventing a concurrent turn from
/// installing a lease between a caller's reconciliation check and its matching state publication.
/// 写锁会一直持有到该值被释放，防止并发回合在调用方协调检查与配套状态发布之间安装租约。
pub struct SessionSelectionLeaseAbsenceGuard<'a> {
    /// Opaque exclusive lease-map guard retained across the caller's state publication.
    /// 在调用方状态发布期间保留的不透明独占租约映射 Guard。
    _guard: std::sync::RwLockWriteGuard<'a, BTreeMap<String, ResolvedPublicAiSelection>>,
}

/// Reversible exclusive transaction for explicit session route-lease revocation.
/// 用于显式撤销会话路由租约的可逆独占事务。
///
/// The write guard excludes every concurrent lease writer until the caller either commits the
/// matching Service-state publication or drops this value. An uncommitted drop restores the exact
/// removed selections before releasing the lock.
/// 写锁会排除全部并发租约写入方，直到调用方提交配套 Service 状态发布或释放本值；未提交时
/// 释放会在解锁前精确恢复所有已移除选择。
pub struct SessionSelectionLeaseReconciliationGuard<'a> {
    /// Exclusive lease map retained across the caller-owned publication boundary.
    /// 在调用方持有的发布边界期间保留的独占租约映射。
    guard: std::sync::RwLockWriteGuard<'a, BTreeMap<String, ResolvedPublicAiSelection>>,
    /// Exact selections removed at acquisition and restored on rollback.
    /// 获取 Guard 时移除并在回滚时恢复的精确选择。
    removed: BTreeMap<String, ResolvedPublicAiSelection>,
    /// Whether the paired Service-state publication completed successfully.
    /// 配套 Service 状态发布是否已成功完成。
    committed: bool,
}

impl SessionSelectionLeaseReconciliationGuard<'_> {
    /// Commits the exact lease removals and releases writer exclusion.
    /// 提交精确租约删除并释放写入排他锁。
    ///
    /// # Returns
    /// 返回值
    /// Nothing; consuming this guard makes its removed selections permanently absent.
    /// 无返回值；消费本 Guard 后，其已移除选择将永久保持缺失。
    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for SessionSelectionLeaseReconciliationGuard<'_> {
    /// Restores every removed lease when the caller abandons the paired state publication.
    /// 当调用方放弃配套状态发布时恢复全部已移除租约。
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for (session_id, selection) in std::mem::take(&mut self.removed) {
            self.guard.insert(session_id, selection);
        }
    }
}

/// Validates one process-local immutable session-route lease admission.
/// 校验一次进程内不可变会话路由租约准入。
///
/// # Parameters
/// 参数
/// `session_id` is the exact key, `lease_count` is the current retained population, and
/// `already_leased` preserves idempotent reads even when the capacity is full.
/// `session_id` 是精确键，`lease_count` 是当前保留数量，`already_leased` 在容量已满时仍
/// 保留幂等读取。
///
/// # Returns
/// 返回值
/// Success for one bounded existing or admissible new key, otherwise a controlled capacity error.
/// 对一个有界的既有键或可准入新键返回成功，否则返回受控容量错误。
fn validate_session_selection_lease_admission(
    session_id: &str,
    lease_count: usize,
    already_leased: bool,
) -> Result<(), String> {
    if session_id.is_empty() || session_id.len() > SESSION_SELECTION_LEASE_ID_MAX_BYTES {
        return Err(format!(
            "Public AI session route lease identifiers must contain 1..={SESSION_SELECTION_LEASE_ID_MAX_BYTES} UTF-8 bytes."
        ));
    }
    if !already_leased && lease_count >= SESSION_SELECTION_LEASE_MAX_ENTRIES {
        return Err(format!(
            "Public AI session route lease capacity of {SESSION_SELECTION_LEASE_MAX_ENTRIES} entries is exhausted."
        ));
    }
    Ok(())
}

impl PublicAiRuntime {
    /// Returns the complete non-secret provider selection leased by one active session.
    /// 返回单个活动会话租用的完整非秘密供应商选择。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the stable session identifier used only as a process-local map key.
    /// `session_id` 是仅用作进程内映射键的稳定会话标识。
    ///
    /// # Returns
    /// 返回值
    /// The frozen selection when the session already owns a lease, or `None` otherwise.
    /// 会话已有租约时返回冻结选择，否则返回 `None`。
    pub fn session_selection_lease(
        &self,
        session_id: &str,
    ) -> Result<Option<ResolvedPublicAiSelection>, String> {
        self.session_selection_leases
            .read()
            .map_err(|_| String::from("The public AI session route lease lock is poisoned."))
            .map(|leases| leases.get(session_id).cloned())
    }

    /// Atomically leases one complete non-secret provider selection to a session if absent.
    /// 当会话尚无租约时原子租用一份完整非秘密供应商选择。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the stable session identifier and `selection` is the exact immutable
    /// route snapshot captured at the session activation boundary.
    /// `session_id` 是稳定会话标识，`selection` 是在会话激活边界捕获的精确不可变路由快照。
    ///
    /// # Returns
    /// 返回值
    /// The already leased selection or the newly installed candidate. Concurrent callers can
    /// therefore never replace an established session route.
    /// 返回已有租约选择或新安装候选，因此并发调用方绝不会替换已建立的会话路由。
    pub fn lease_session_selection(
        &self,
        session_id: String,
        selection: ResolvedPublicAiSelection,
    ) -> Result<ResolvedPublicAiSelection, String> {
        let mut leases = self
            .session_selection_leases
            .write()
            .map_err(|_| String::from("The public AI session route lease lock is poisoned."))?;
        let already_leased = leases.contains_key(session_id.as_str());
        validate_session_selection_lease_admission(
            session_id.as_str(),
            leases.len(),
            already_leased,
        )?;
        if let Some(existing) = leases.get(session_id.as_str()) {
            return Ok(existing.clone());
        }
        leases.insert(session_id, selection.clone());
        Ok(selection)
    }

    /// Clears the process-local provider selection leased by one session.
    /// 清除单个会话租用的进程内供应商选择。
    ///
    /// # Parameters
    /// 参数
    /// `lease_scope_id` identifies the exact lease invalidated by explicit model reselection
    /// or session lifecycle removal.
    /// `lease_scope_id` 标识因显式重新选择模型或会话生命周期移除而失效的精确租约。
    ///
    /// # Returns
    /// 返回值
    /// The removed selection when present, or `None` when no lease existed.
    /// 存在租约时返回被移除选择，否则返回 `None`。
    pub fn clear_session_selection_lease(
        &self,
        lease_scope_id: &str,
    ) -> Result<Option<ResolvedPublicAiSelection>, String> {
        self.session_selection_leases
            .write()
            .map_err(|_| String::from("The public AI session route lease lock is poisoned."))
            .map(|mut leases| leases.remove(lease_scope_id))
    }

    /// Clears the process-local provider-route lease owned by one session.
    /// 清除单个会话拥有的进程内供应商路由租约。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the exact owner removed or explicitly reselecting its model.
    /// `session_id` 是被移除或正在显式重新选择模型的精确所有者。
    ///
    /// # Returns
    /// 返回值
    /// One when the session owned a lease, or zero otherwise.
    /// 会话拥有租约时返回一，否则返回零。
    pub fn clear_session_selection_leases(&self, session_id: &str) -> Result<usize, String> {
        self.clear_session_selection_lease(session_id)
            .map(|selection| usize::from(selection.is_some()))
    }

    /// Proves a complete session set owns no route lease and excludes concurrent installation.
    /// 证明一组完整会话不持有路由租约，并排除并发安装。
    ///
    /// # Parameters
    /// 参数
    /// `session_ids` contains the exact stable session identities whose unleased runtime controls
    /// will be replaced by one atomic caller-owned projection.
    /// `session_ids` 包含将由调用方原子投影替换未租用运行态控制的精确稳定会话身份。
    ///
    /// # Returns
    /// 返回值
    /// An opaque guard retained until the caller publishes its matching state, or a conflict/error
    /// without mutation.
    /// 返回保留到调用方发布配套状态的不透明 Guard，或无变更的冲突/错误。
    pub fn acquire_session_selection_lease_absence_guard<'a>(
        &'a self,
        session_ids: &[String],
    ) -> Result<SessionSelectionLeaseAbsenceGuard<'a>, String> {
        // leases excludes writers before the first absence check and remains owned by the result.
        // leases 在首次缺失检查前排除写入方，并由返回值持续持有。
        let leases = self
            .session_selection_leases
            .write()
            .map_err(|_| String::from("The public AI session route lease lock is poisoned."))?;
        if let Some(session_id) = session_ids
            .iter()
            .find(|session_id| leases.contains_key(session_id.as_str()))
        {
            return Err(format!(
                "Session `{session_id}` acquired a public AI route lease while model reconciliation was being prepared."
            ));
        }
        Ok(SessionSelectionLeaseAbsenceGuard { _guard: leases })
    }

    /// Revokes one exact session set while proving another set remains unleased.
    /// 撤销一组精确会话租约，同时证明另一组会话仍未持有租约。
    ///
    /// # Parameters
    /// 参数
    /// `revoked_session_ids` identifies explicit access revocations whose current leases must be
    /// removed; `required_absent_session_ids` identifies catalog reconciliations that may proceed
    /// only when no route lease exists.
    /// `revoked_session_ids` 标识必须移除当前租约的显式访问撤销；
    /// `required_absent_session_ids` 标识仅在路由租约不存在时才能继续的目录协调。
    ///
    /// # Returns
    /// 返回值
    /// A reversible write guard that keeps every lease writer excluded until commit, or an error
    /// without mutation when one required-absent session already owns a lease.
    /// 返回一个在提交前排除全部租约写入方的可逆写 Guard；若要求缺失的会话已持有租约，
    /// 则无修改返回错误。
    pub fn acquire_session_selection_lease_reconciliation_guard<'a>(
        &'a self,
        revoked_session_ids: &[String],
        required_absent_session_ids: &[String],
    ) -> Result<SessionSelectionLeaseReconciliationGuard<'a>, String> {
        // leases excludes writers before validation so the absence proof and removals are atomic.
        // leases 在校验前排除写入方，使缺失证明与移除操作保持原子性。
        let mut leases = self
            .session_selection_leases
            .write()
            .map_err(|_| String::from("The public AI session route lease lock is poisoned."))?;
        if let Some(session_id) = required_absent_session_ids
            .iter()
            .find(|session_id| leases.contains_key(session_id.as_str()))
        {
            return Err(format!(
                "Session `{session_id}` acquired a public AI route lease while model reconciliation was being prepared."
            ));
        }
        // removed owns the exact before-image while duplicate identifiers remain idempotent.
        // removed 持有精确前镜像，同时让重复标识保持幂等。
        let mut removed = BTreeMap::new();
        for session_id in revoked_session_ids {
            if removed.contains_key(session_id.as_str()) {
                continue;
            }
            if let Some(selection) = leases.remove(session_id.as_str()) {
                removed.insert(session_id.clone(), selection);
            }
        }
        Ok(SessionSelectionLeaseReconciliationGuard {
            guard: leases,
            removed,
            committed: false,
        })
    }
}

#[cfg(test)]
mod tests;
