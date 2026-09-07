//! Revocable process-local authority for one generation of unaccepted host writebacks.
//! 一代尚未被接受的宿主写回使用的可撤销进程内权限。

use crate::{PostActionRequest, VmmClientError};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Monotonic process-local permit identity; it is never a memory project or database identifier.
/// 单调进程内许可身份；绝不作为记忆项目或数据库标识。
static NEXT_PERMIT_ID: AtomicU64 = AtomicU64::new(1);

/// Shared revocation state retained by every clone of one admission generation.
/// 同一准入代次的所有克隆共同持有的撤销状态。
#[derive(Debug)]
struct PermitState {
    /// Non-reusable process-local generation identity.
    /// 不复用的进程内代次身份。
    id: u64,
    /// False permanently revokes this generation; no operation can set it back to true.
    /// false 永久撤销本代次；任何操作都不能将其恢复为 true。
    active: AtomicBool,
}

/// A cloneable delivery permit, deliberately absent from serializable business requests and stored records.
/// 可克隆投递许可，刻意不进入可序列化业务请求及已存记录。
#[derive(Clone, Debug)]
pub struct VmmWritebackPermit {
    /// Exact shared generation, independent of mutable binding names or repeated destination IDs.
    /// 准确共享代次，独立于可变绑定名称或重复出现的目的地 ID。
    state: Arc<PermitState>,
}

impl VmmWritebackPermit {
    /// Allocates a fresh active generation, or reports identity exhaustion without wrapping.
    /// 分配全新活动代次，身份耗尽时明确报错而不回绕。
    /// Takes no caller data and returns a permit that cannot revive any previous generation.
    /// 不接收调用方数据，返回无法复活任何旧代次的许可。
    pub fn new() -> Result<Self, VmmClientError> {
        // Checked allocation prevents a stale receipt from ever matching a recycled process identity.
        // 经检查分配防止旧回执匹配被重复使用的进程身份。
        let id = NEXT_PERMIT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| {
                VmmClientError::new(
                    "writeback_permit_exhausted",
                    "VMM writeback permit identities are exhausted.",
                )
            })?;
        Ok(Self {
            state: Arc::new(PermitState {
                id,
                active: AtomicBool::new(true),
            }),
        })
    }

    /// Returns the exact local generation for correlating queued entries and delayed effects.
    /// 返回用于关联排队条目及迟到效果的准确本地代次。
    pub fn id(&self) -> u64 {
        self.state.id
    }

    /// Permanently revokes this generation and every outstanding clone; stored memory remains untouched.
    /// 永久撤销本代次及所有尚存克隆；已存记忆不受影响。
    pub fn revoke(&self) {
        self.state.active.store(false, Ordering::Release);
    }

    /// Validates current delivery authority after waiting for a route or binding lock.
    /// 等待路由或绑定锁后校验当前投递权限。
    /// Returns a terminal cancellation error instead of allowing automatic retry into a later generation.
    /// 返回终态取消错误，不允许自动重试进入后续代次。
    pub fn ensure_active(&self) -> Result<(), VmmClientError> {
        if self.state.active.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(VmmClientError::new(
                "writeback_cancelled",
                "The pending VMM writeback was cancelled; automatic replay is not allowed.",
            ))
        }
    }

    /// Validates both the live permit and the mandatory source of a queued host delivery.
    /// 同时校验活动许可及排队宿主投递必须携带的来源。
    /// Takes the frozen request and rejects missing provenance before any backend side effect.
    /// 接收冻结请求，在任何后端副作用之前拒绝缺失来源。
    pub fn validate_request(&self, request: &PostActionRequest) -> Result<(), VmmClientError> {
        self.ensure_active()?;
        request
            .expected_source
            .as_ref()
            .ok_or_else(|| {
                VmmClientError::new(
                    "missing_memory_source",
                    "A guarded VMM writeback requires its exact frozen memory source.",
                )
            })?
            .validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves cancelling an old generation cannot affect or be undone by creating a new one.
    /// 证明取消旧代次既不会影响新代次，也不会因创建新代次而被撤销。
    #[test]
    fn revoked_generation_never_revives_and_clones_observe_cancellation() {
        // One exact request can be valid only through its separately admitted live generation.
        // 同一准确请求只能通过独立准入的活动代次获得有效权限。
        let request = PostActionRequest {
            expected_source: Some(crate::VmmMemorySource::native(7).expect("source")),
            ..Default::default()
        };
        let old = VmmWritebackPermit::new().expect("old");
        let delayed = old.clone();
        assert_eq!(old.id(), delayed.id());
        assert!(delayed.validate_request(&request).is_ok());
        old.revoke();
        old.revoke();
        let new = VmmWritebackPermit::new().expect("new");
        assert_ne!(old.id(), new.id());
        assert!(new.validate_request(&request).is_ok());
        assert_eq!(
            delayed
                .validate_request(&request)
                .expect_err("cancelled")
                .kind,
            "writeback_cancelled"
        );
        assert_eq!(
            new.validate_request(&PostActionRequest::default())
                .expect_err("missing source")
                .kind,
            "missing_memory_source"
        );
    }
}
