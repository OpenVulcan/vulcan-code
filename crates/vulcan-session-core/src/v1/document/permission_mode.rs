//! Session permission mode selection.
//! 会话权限模式选择。

use super::{DirtyFile, SessionDocument, validate_timestamp};
use crate::v1::DomainError;
use crate::v1::config::{MAX_UNATTENDED_DENY_POLICY_CHARS, PermissionMode, UnattendedConfig};

impl SessionDocument {
    /// Saves `mode` as the session permission mode; returns whether it changed.
    /// 将 `mode` 保存为会话权限模式；返回是否发生变化。
    /// Metadata only: an active API keeps running and the next tool call reads the new mode.
    /// 仅修改元数据：活动 API 继续运行，下一次工具调用读取新模式。
    pub fn set_permission_mode(
        &mut self,
        mode: PermissionMode,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("access.permission.updated_at_ms", now_ms)?;
        if self.metadata.archived_at_ms.is_some() {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "access.permission.archived_session",
            });
        }
        if self.metadata.mode.access.permission == mode {
            return Ok(false);
        }
        self.next_mutation_revision()?;
        self.metadata.mode.access.permission = mode;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Turns unattended mode on with `unattended`, or off with `None`; returns whether it changed.
    /// 以 `unattended` 开启无人值守模式，或以 `None` 关闭；返回是否发生变化。
    /// Metadata only, like the permission mode: the next tool call reads the new state.
    /// 与权限模式相同仅修改元数据：下一次工具调用读取新状态。
    pub fn set_unattended(
        &mut self,
        unattended: Option<UnattendedConfig>,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("access.unattended.updated_at_ms", now_ms)?;
        if self.metadata.archived_at_ms.is_some() {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "access.unattended.archived_session",
            });
        }
        if unattended.as_ref().is_some_and(|config| {
            config.deny_policy.chars().count() > MAX_UNATTENDED_DENY_POLICY_CHARS
        }) {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "access.unattended.deny_policy",
            });
        }
        if self.metadata.mode.access.unattended == unattended {
            return Ok(false);
        }
        self.next_mutation_revision()?;
        self.metadata.mode.access.unattended = unattended;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::{ApiPurposeKind, event::ApiRequestSnapshot};

    /// New sessions start in workspace-write; repeated saves are idempotent.
    /// 新会话以工作区修改开始；重复保存是幂等的。
    #[test]
    fn permission_mode_defaults_and_is_idempotent() {
        let mut document = SessionDocument::new("perm", "p", "t", 1);
        assert_eq!(
            document.metadata.mode.access.permission,
            PermissionMode::WorkspaceWrite
        );
        assert!(
            !document
                .set_permission_mode(PermissionMode::WorkspaceWrite, 2)
                .unwrap()
        );
        assert!(
            document
                .set_permission_mode(PermissionMode::ReadOnly, 3)
                .unwrap()
        );
        let revision = document.memory_revision();
        assert!(
            !document
                .set_permission_mode(PermissionMode::ReadOnly, 4)
                .unwrap()
        );
        assert_eq!(document.memory_revision(), revision);
    }

    /// Switching during an API changes metadata only and leaves events and the API untouched.
    /// API 期间切换只改元数据，事件与 API 保持不变。
    #[test]
    fn permission_mode_switch_during_api_is_metadata_only() {
        let mut document = SessionDocument::new("perm-api", "p", "t", 1);
        document
            .submit_root_input_fixture("hello", Vec::new(), 2)
            .unwrap();
        document
            .start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("p", "m"),
                3,
            )
            .unwrap();
        let events = document.events().to_vec();
        let active = document.active_api().cloned();
        assert!(
            document
                .set_permission_mode(PermissionMode::FullAccess, 4)
                .unwrap()
        );
        assert_eq!(document.events(), events);
        assert_eq!(document.active_api(), active.as_ref());
        assert_eq!(
            document.metadata.mode.access.permission,
            PermissionMode::FullAccess
        );
    }

    /// Unattended mode starts off, keeps the confirmed policy, and rejects an oversized one.
    /// 无人值守模式默认关闭，保存确认的策略，并拒绝超长策略。
    #[test]
    fn unattended_mode_saves_the_confirmed_policy() {
        let mut document = SessionDocument::new("unattended", "p", "t", 1);
        assert_eq!(document.metadata.mode.access.unattended, None);
        let config = UnattendedConfig {
            deny_policy: String::from("never touch production"),
        };
        assert!(document.set_unattended(Some(config.clone()), 2).unwrap());
        assert!(!document.set_unattended(Some(config.clone()), 3).unwrap());
        assert_eq!(document.metadata.mode.access.unattended, Some(config));
        let oversized = UnattendedConfig {
            deny_policy: "x".repeat(MAX_UNATTENDED_DENY_POLICY_CHARS + 1),
        };
        assert!(document.set_unattended(Some(oversized), 4).is_err());
        assert!(document.set_unattended(None, 5).unwrap());
        assert_eq!(document.metadata.mode.access.unattended, None);
    }

    /// Archived sessions reject changes and keep their metadata.
    /// 已归档会话拒绝修改并保持元数据不变。
    #[test]
    fn permission_mode_archived_session_rejects_changes() {
        let mut document = SessionDocument::new("perm-archive", "p", "t", 1);
        document.set_archived_at(Some(2), 2).unwrap();
        let before = document.metadata.clone();
        assert!(matches!(
            document.set_permission_mode(PermissionMode::ReadOnly, 3),
            Err(DomainError::InvalidSelectionConfiguration {
                field: "access.permission.archived_session"
            })
        ));
        assert_eq!(document.metadata, before);
    }
}
