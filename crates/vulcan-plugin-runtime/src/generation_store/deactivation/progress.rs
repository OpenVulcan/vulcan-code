//! Same-original observed cleanup watermarks cannot regress after effects, errors or backend unwinding.
//! 同原实例已观察清理水位在效果、错误或后端展开之后不能回退。

use super::*;
use std::sync::atomic::{AtomicU8, Ordering};

/// One private ordering represents actual cleanup checkpoints for both declared package and credential phase schemas.
/// 单个私有顺序代表声明包及凭据阶段结构的实际清理检查点。
#[repr(u8)]
#[derive(Clone, Copy)]
enum CleanupStage {
    /// The actual producer has not published a cleanup checkpoint yet.
    /// 实际生产者尚未发布清理检查点。
    Unstarted,
    /// The actual producer has published its irreversible cleanup intent.
    /// 实际生产者已发布不可逆清理意图。
    Started,
    /// The actual producer has published its terminal cleanup checkpoint.
    /// 实际生产者已发布终态清理检查点。
    Finished,
}

/// A pure same-original watermark records actual observations without introducing file ownership, serialization or panic-poisoned locks.
/// 纯同原实例水位记录实际观察，不引入文件所有权、序列化或 panic 中毒锁。
pub(super) struct CleanupProgress {
    /// The highest actual checkpoint is monotonic even when a backend panics before returning control.
    /// 最高实际检查点保持单调，即使后端在返回控制前 panic。
    observed: AtomicU8,
}

impl CleanupProgress {
    /// Initialize from a strictly verified actual package phase; returns metadata only, never a package pin.
    /// 从严格验证实际包阶段初始化；仅返回元数据，绝非包固定。
    pub(super) fn package(phase: GenerationState) -> Self {
        Self {
            observed: AtomicU8::new(package_stage(phase) as u8),
        }
    }

    /// Initialize from a strictly verified actual credential phase; returns metadata only, never a revision lease.
    /// 从严格验证实际凭据阶段初始化；仅返回元数据，绝非修订租约。
    pub(super) fn credential(phase: VulcanPluginCredentialPhase) -> Self {
        Self {
            observed: AtomicU8::new(credential_stage(phase) as u8),
        }
    }

    /// Reject a package phase below this original's highest actual checkpoint; returns unit before any physical deletion.
    /// 拒绝低于此原实例最高实际检查点的包阶段；任何物理删除之前返回空值。
    pub(super) fn validate_package(&self, phase: GenerationState) -> Result<()> {
        self.validate(package_stage(phase))
    }

    /// Reject a credential phase below this original's highest actual checkpoint; returns unit before secure effects.
    /// 拒绝低于此原实例最高实际检查点的凭据阶段；安全效果之前返回空值。
    pub(super) fn validate_credential(&self, phase: VulcanPluginCredentialPhase) -> Result<()> {
        self.validate(credential_stage(phase))
    }

    /// Record a fully compared actual package observation or immediately acknowledged producer write; no fallible gap is introduced.
    /// 记录完整比较实际包观察或立即确认生产者写入；不引入可失败间隙。
    pub(super) fn record_package(&self, phase: GenerationState) {
        self.observed
            .fetch_max(package_stage(phase) as u8, Ordering::AcqRel);
    }

    /// Record a fully compared actual credential observation or immediately acknowledged producer write; backend unwinding cannot erase it.
    /// 记录完整比较实际凭据观察或立即确认生产者写入；后端展开不能抹除它。
    pub(super) fn record_credential(&self, phase: VulcanPluginCredentialPhase) {
        self.observed
            .fetch_max(credential_stage(phase) as u8, Ordering::AcqRel);
    }

    /// Compare phase with the actual watermark; returns Conflict on regression without granting collection authority.
    /// 将阶段与实际水位比较；回退时返回冲突，不授予回收权威。
    fn validate(&self, phase: CleanupStage) -> Result<()> {
        if (phase as u8) < self.observed.load(Ordering::Acquire) {
            return Err(Error::new(
                Code::Conflict,
                "Original cleanup history regressed below its observed checkpoint.",
            ));
        }
        Ok(())
    }
}

/// Map the declared package phase to cleanup ordering; phase is actual schema metadata and the result grants no ownership.
/// 将声明包阶段映射至清理顺序；phase 为实际结构元数据，结果不授予所有权。
fn package_stage(phase: GenerationState) -> CleanupStage {
    match phase {
        GenerationState::Preparing | GenerationState::Prepared => CleanupStage::Unstarted,
        GenerationState::Collecting => CleanupStage::Started,
        GenerationState::Collected => CleanupStage::Finished,
    }
}

/// Map the declared credential phase to cleanup ordering; phase is actual schema metadata and the result grants no ownership.
/// 将声明凭据阶段映射至清理顺序；phase 为实际结构元数据，结果不授予所有权。
fn credential_stage(phase: VulcanPluginCredentialPhase) -> CleanupStage {
    match phase {
        VulcanPluginCredentialPhase::Preparing | VulcanPluginCredentialPhase::Staged => {
            CleanupStage::Unstarted
        }
        VulcanPluginCredentialPhase::Discarding => CleanupStage::Started,
        VulcanPluginCredentialPhase::Discarded => CleanupStage::Finished,
    }
}
