//! Registers native business-session ownership independently of extraction, noise decisions and deduplication.
//! 独立于提炼、噪音判定及去重登记原生业务会话归属。

use super::*;
use vulcan_memorymesh_core::{StorageMemorySession, StorageSessionStatus, StorageSnapshot};

impl NativeVmmClient {
    /// Resolves the bound project and registers a missing session while preserving existing lifecycle state.
    /// 解析绑定项目并登记缺失会话，同时保留既有生命周期状态。
    /// Takes trusted host/project/subject/session identities; returns the stable access and its read guard.
    /// 接收可信宿主/项目/主体/会话身份；返回稳定访问范围及其读取守卫。
    /// Registration does not imply successful extraction; persistence failures abort admission explicitly.
    /// 登记不代表提炼成功；持久化失败时明确终止准入。
    pub(super) async fn admit_memory_session(
        &self,
        host_project_id: &str,
        project_id: u64,
        subject: &SubjectId,
        session: &SessionSourceId,
        expected_source: Option<&vulcan_vmm::VmmMemorySource>,
        permit: Option<&vulcan_vmm::VmmWritebackPermit>,
    ) -> Result<(tokio::sync::RwLockReadGuard<'_, ()>, ScopeAccess), VmmClientError> {
        // Existing sessions remain on the concurrent path, including paused sessions checked by the caller.
        // 既有会话保持并发路径，包括由调用方检查的暂停会话。
        let read_guard = self.operation_gate.read().await;
        // A revoked generation stays revoked even if the binding changed away and returned to the same ID.
        // 即使绑定切走后又回到同一 ID，已撤销代次也始终保持撤销。
        if let Some(permit) = permit {
            permit.ensure_active()?;
        }
        let access = self.project_access(host_project_id, project_id).await?;
        // Resolving the binding is asynchronous too; revocation during that read must precede registration.
        // 解析绑定同样是异步过程；读取期间的撤销必须先于登记生效。
        if let Some(permit) = permit {
            permit.ensure_active()?;
        }
        // Check the actual binding under the same guard that admits the write, before registering anything.
        // 在写入准入的同一守卫内检查实际绑定，先校验再登记任何信息。
        vulcan_vmm::VmmMemorySource::native(parse_core_numeric_id(
            &access.write_scope,
            "vmm-project:",
        )?)?
        .verify_expected(expected_source)?;
        let snapshot = self
            .runtime
            .storage_snapshot()
            .map_err(native_runtime_error)?;
        if contains_session(&snapshot, subject, &access.write_scope, session) {
            return Ok((read_guard, access));
        }
        drop(read_guard);

        // Bindings can change while waiting for exclusive access; resolve them again under the write guard.
        // 等待独占访问期间绑定可能变化，因此在写入守卫内重新解析。
        let write_guard = self.operation_gate.write().await;
        // Waiting for exclusive registration must not revive a permit cancelled during the lock upgrade.
        // 等待独占登记不能复活锁升级期间已取消的许可。
        if let Some(permit) = permit {
            permit.ensure_active()?;
        }
        let access = self.project_access(host_project_id, project_id).await?;
        if let Some(permit) = permit {
            permit.ensure_active()?;
        }
        // The lock upgrade can race with rebinding, so the second admission must verify again.
        // 锁升级可能与重新绑定交错，因此第二次准入必须重新校验。
        vulcan_vmm::VmmMemorySource::native(parse_core_numeric_id(
            &access.write_scope,
            "vmm-project:",
        )?)?
        .verify_expected(expected_source)?;
        let snapshot = self
            .runtime
            .storage_snapshot()
            .map_err(native_runtime_error)?;
        if !contains_session(&snapshot, subject, &access.write_scope, session) {
            // A fresh admission owns its journal identity; no caller content or display title becomes an ID.
            // 新准入拥有独立日志身份；调用方内容及显示标题不作为标识。
            let operation_id = format!("native-vmm-session-register:{}", Uuid::new_v4());
            self.runtime
                .transform_storage_snapshot(
                    &operation_id,
                    "native-vmm-session-register-v1",
                    snapshot.storage_version,
                    |after| {
                        after.memory_sessions.push(StorageMemorySession {
                            subject_id: subject.clone(),
                            scope_id: access.write_scope.clone(),
                            session_source_id: session.clone(),
                            status: StorageSessionStatus::Active,
                            last_extraction_observed_at_ns: 0,
                            last_extraction_completed_at_ns: None,
                        });
                        // Match the storage snapshot's composite-key order before persisting its vector representation.
                        // 持久化数组表示前，与存储快照的复合键顺序保持一致。
                        after.memory_sessions.sort_by(|left, right| {
                            (&left.subject_id, &left.scope_id, &left.session_source_id).cmp(&(
                                &right.subject_id,
                                &right.scope_id,
                                &right.session_source_id,
                            ))
                        });
                        Ok(())
                    },
                )
                .await
                .map_err(native_runtime_error)?;
        }
        // Downgrading is atomic: maintenance cannot rebind or pause the session before the caller executes.
        // 降级是原子的：调用方执行前维护操作不能重新绑定或暂停会话。
        Ok((
            tokio::sync::RwLockWriteGuard::downgrade(write_guard),
            access,
        ))
    }
}

/// Checks exact persisted ownership without inferring registration from memories or turn counts.
/// 检查准确持久化归属，不从记忆或回合数量推断是否登记。
/// Takes a snapshot and composite identity; returns whether that exact record exists.
/// 接收快照及复合身份；返回该准确记录是否存在。
fn contains_session(
    snapshot: &StorageSnapshot,
    subject: &SubjectId,
    scope: &ScopeId,
    session: &SessionSourceId,
) -> bool {
    snapshot.memory_sessions.iter().any(|record| {
        &record.subject_id == subject
            && &record.scope_id == scope
            && &record.session_source_id == session
    })
}
