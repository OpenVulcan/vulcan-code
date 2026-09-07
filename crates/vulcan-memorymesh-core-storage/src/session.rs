//! Durable session lifecycle rules independent of retained conversation bodies.
//! 独立于所保留会话正文的持久会话生命周期规则。

use serde::{Deserialize, Serialize};

/// Records the authoritative management state, never inferred from content counts.
/// 记录权威管理状态，绝不从内容数量推断。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageSessionStatus {
    /// Allows ordinary recall and persistence.
    /// 允许普通召回和持久化。
    Active,
    /// Changes management classification while keeping runtime memory enabled.
    /// 改变管理分类，同时保持运行时记忆启用。
    Archived,
    /// Pauses runtime memory while recoverable content remains in a recycle batch.
    /// 可恢复内容保留于回收批次期间暂停运行时记忆。
    Recycled,
    /// Keeps a no-content tombstone until the user explicitly restarts memory.
    /// 保留无内容墓碑，直到用户显式重新启用记忆。
    Forgotten,
}

impl StorageSessionStatus {
    /// Reactivates only a forgotten session without restoring content or replaying conversation history.
    /// 只重新启用已遗忘会话，不恢复内容或重放会话历史。
    /// Returns Active for Forgotten and an explicit error for every other lifecycle state.
    /// Forgotten 返回 Active，其他生命周期状态均返回明确错误。
    pub fn restart_transition(self) -> Result<Self, String> {
        match self {
            Self::Forgotten => Ok(Self::Active),
            _ => Err("only forgotten memory sessions can be restarted".to_owned()),
        }
    }

    /// Returns whether this exact state permits ordinary recall and writeback.
    /// 返回该准确状态是否允许普通召回与写回。
    #[must_use]
    pub const fn allows_runtime_memory(self) -> bool {
        matches!(self, Self::Active | Self::Archived)
    }

    /// Validates an archive toggle and returns the resulting state without mutating content.
    /// 校验归档切换并返回结果状态，不修改内容。
    /// `archived` selects archive when true and unarchive when false; invalid transitions fail.
    /// `archived` 为真时选择归档，为假时选择取消归档；非法转换报错。
    pub fn archive_transition(self, archived: bool) -> Result<Self, String> {
        match (self, archived) {
            (Self::Active, true) => Ok(Self::Archived),
            (Self::Archived, false) => Ok(Self::Active),
            (_, true) => Err("only active memory sessions can be archived".to_owned()),
            (_, false) => Err("only archived memory sessions can be unarchived".to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StorageSessionStatus;

    /// Exercises all lifecycle states against both archive actions and runtime eligibility.
    /// 遍历全部生命周期状态，验证两种归档操作及运行时资格。
    #[test]
    fn archive_matrix_preserves_restore_and_restart_boundaries() {
        for state in [
            StorageSessionStatus::Active,
            StorageSessionStatus::Archived,
            StorageSessionStatus::Recycled,
            StorageSessionStatus::Forgotten,
        ] {
            if state == StorageSessionStatus::Forgotten {
                assert_eq!(
                    state.restart_transition().expect("restart"),
                    StorageSessionStatus::Active
                );
            } else {
                assert!(state.restart_transition().is_err());
            }
            assert_eq!(
                state.allows_runtime_memory(),
                matches!(
                    state,
                    StorageSessionStatus::Active | StorageSessionStatus::Archived
                )
            );
            for archived in [false, true] {
                // Compare the entire result, not only whether a transition returned an error.
                // 比较完整结果，而不只判断转换是否报错。
                let result = state.archive_transition(archived);
                match (state, archived) {
                    (StorageSessionStatus::Active, true) => {
                        assert_eq!(result.expect("archive"), StorageSessionStatus::Archived)
                    }
                    (StorageSessionStatus::Archived, false) => {
                        assert_eq!(result.expect("unarchive"), StorageSessionStatus::Active)
                    }
                    _ => assert!(result.is_err()),
                }
            }
        }
    }
}
