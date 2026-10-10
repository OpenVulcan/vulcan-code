//! Dedicated-session installation, memory locking and frozen-setting guards.
//! 专职会话安装、记忆锁定与冻结设置守卫。

use super::persistence::DirtyFile;
use super::{SessionDocument, validate_timestamp};
use crate::v1::dedicated::{DedicatedLockedSpan, DedicatedSessionProfile};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    DisplayNoteEvent, DisplayNoticeKind, EventBody, EventVisibility, NoticeSeverity,
};
use crate::v1::ids::Seq;

/// Stable reason code of the lock timeline note.
/// 锁定时间线通知的稳定原因码。
pub const DEDICATED_MEMORY_LOCKED_REASON: &str = "dedicated_memory_locked";

/// Stable reason code of the unlock timeline note.
/// 解锁时间线通知的稳定原因码。
pub const DEDICATED_MEMORY_UNLOCKED_REASON: &str = "dedicated_memory_unlocked";

impl SessionDocument {
    /// Returns the dedicated profile, or `None` for an ordinary session.
    /// 返回专职会话配置；普通会话返回 `None`。
    pub fn dedicated_profile(&self) -> Option<&DedicatedSessionProfile> {
        self.metadata.dedicated.as_ref()
    }

    /// Rejects a real change to a setting that dedicated sessions freeze at creation.
    /// 拒绝对专职会话在创建时冻结的设置进行真实修改。
    pub(super) fn reject_dedicated_change(&self, setting: &'static str) -> Result<(), DomainError> {
        if self.metadata.dedicated.is_some() && !self.dedicated_creation_open() {
            return Err(DomainError::DedicatedSettingFrozen { setting });
        }
        Ok(())
    }

    /// Returns whether the creating submission may still write the frozen settings.
    /// 返回创建提交是否仍可写入冻结设置。
    ///
    /// Creation installs the profile together with the first user input; the same submission then
    /// writes the frozen selections before that turn issues its first Provider request. The window
    /// is open only while the complete stream is resident, the only turn is the open first turn,
    /// and no request has started.
    /// 创建流程随首条用户输入一起安装配置；同一提交在该回合发出第一个 Provider 请求之前写入冻结
    /// 选择。仅当完整事件流常驻、唯一回合是开放的首回合且尚无请求开始时，该窗口才开放。
    fn dedicated_creation_open(&self) -> bool {
        let Some(turn) = self.active_turn.as_ref().filter(|turn| !turn.closed) else {
            return false;
        };
        self.resident_start_seq == 1
            && self.events.iter().all(|event| {
                event.turn_id.is_none_or(|turn_id| turn_id == turn.turn_id)
                    && !matches!(event.event, EventBody::ApiStarted(_))
            })
    }

    /// Turns a freshly created, event-free session into a dedicated session.
    /// 把刚创建且没有任何事件的会话转为专职会话。
    ///
    /// Creation applies the frozen selections first, then installs the profile;
    /// from then on every frozen setting rejects changes. With `lock_immediately`
    /// the first lock note is written in the same revision.
    /// 创建流程先写入冻结的选择，再安装配置；此后所有冻结设置都拒绝修改。
    /// `lock_immediately` 为真时，第一条锁定通知在同一修订中写入。
    pub fn install_dedicated_profile(
        &mut self,
        profile: DedicatedSessionProfile,
        lock_immediately: bool,
        now_ms: i64,
    ) -> Result<Option<Seq>, DomainError> {
        validate_timestamp("dedicated.installed_at_ms", now_ms)?;
        profile.validate()?;
        if self.metadata.dedicated.is_some()
            || !profile.locked_spans.is_empty()
            || !self.events.is_empty()
            || self.resident_start_seq != 1
        {
            return Err(DomainError::InvalidDedicatedProfile {
                field: "installation",
            });
        }
        if self.metadata.mode.goal.enabled || self.metadata.mode.plan.enabled {
            return Err(DomainError::InvalidDedicatedProfile {
                field: "installation.mode",
            });
        }
        if !lock_immediately {
            self.next_mutation_revision()?;
            self.metadata.dedicated = Some(profile);
            self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
            return Ok(None);
        }
        self.metadata.dedicated = Some(profile);
        match self.stage_memory_lock_change(true, now_ms) {
            Ok(seq) => Ok(Some(seq)),
            Err(error) => {
                self.metadata.dedicated = None;
                Err(error)
            }
        }
    }

    /// Locks or unlocks the dedicated memory between turns; returns the note seq when changed.
    /// 在回合之间锁定或解锁专职记忆；状态变化时返回通知 seq。
    pub fn set_dedicated_memory_locked(
        &mut self,
        locked: bool,
        now_ms: i64,
    ) -> Result<Option<Seq>, DomainError> {
        validate_timestamp("dedicated.memory_lock_at_ms", now_ms)?;
        let Some(profile) = &self.metadata.dedicated else {
            return Err(DomainError::NotDedicatedSession);
        };
        if profile.is_locked() == locked {
            return Ok(None);
        }
        if self.metadata.archived_at_ms.is_some() {
            return Err(DomainError::InvalidDedicatedProfile {
                field: "archived_session",
            });
        }
        if let Some(turn) = &self.active_turn
            && !turn.closed
        {
            return Err(DomainError::DedicatedMemoryLockWhileTurnOpen {
                turn_id: turn.turn_id.value(),
            });
        }
        self.stage_memory_lock_change(locked, now_ms).map(Some)
    }

    /// Appends one session-level lock note and updates the spans in one revision.
    /// 在一次修订中追加一条会话级锁定通知并更新区间。
    fn stage_memory_lock_change(&mut self, locked: bool, now_ms: i64) -> Result<Seq, DomainError> {
        self.ensure_transition_capacity(1, 0)?;
        self.next_mutation_revision()?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        let (notice_kind, reason_code) = if locked {
            (
                DisplayNoticeKind::MemoryLocked,
                DEDICATED_MEMORY_LOCKED_REASON,
            )
        } else {
            (
                DisplayNoticeKind::MemoryUnlocked,
                DEDICATED_MEMORY_UNLOCKED_REASON,
            )
        };
        let seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            None,
            EventVisibility::UiOnly,
            EventBody::DisplayNote(DisplayNoteEvent {
                notice_kind,
                severity: NoticeSeverity::Info,
                reason_code: reason_code.to_string(),
                redacted_detail: None,
                api_call_id: None,
                retry_count: None,
                retry_limit: None,
            }),
            now_ms,
        )?;
        let mut profile = self
            .metadata
            .dedicated
            .clone()
            .ok_or(DomainError::NotDedicatedSession)?;
        if locked {
            profile.locked_spans.push(DedicatedLockedSpan {
                locked_at_seq: seq,
                unlocked_at_seq: None,
            });
        } else {
            let span = profile
                .locked_spans
                .last_mut()
                .filter(|span| span.unlocked_at_seq.is_none())
                .ok_or(DomainError::InvalidDedicatedProfile {
                    field: "locked_spans.open",
                })?;
            span.unlocked_at_seq = Some(seq);
        }
        profile.validate()?;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.metadata.dedicated = Some(profile);
        self.bump_revision(
            &[DirtyFile::Metadata, DirtyFile::ActiveTail],
            now_ms,
            &[seq],
        );
        Ok(seq)
    }
}

#[cfg(test)]
mod tests {
    use crate::v1::config::VmmSessionPolicyConfig;
    use crate::v1::dedicated::{
        DedicatedSessionProfile, DedicatedToolAccess, dedicated_provider_events,
    };
    use crate::v1::document::{ClientInputSubmission, SessionDocument};
    use crate::v1::errors::DomainError;
    use crate::v1::event::{ApiPurposeKind, ApiRequestSnapshot, DisplayNoticeKind, EventBody};
    use crate::v1::ids::ClientActionId;
    use crate::v1::projection::project_provider_items;

    /// Fixed test time in Unix milliseconds.
    /// 固定测试时间（Unix 毫秒）。
    const NOW: i64 = 1_700_000_000_000;

    /// Builds an empty document.
    /// 构建空文档。
    fn empty_document() -> SessionDocument {
        SessionDocument::new("session-dedicated", "project", "title", NOW)
    }

    /// Builds the default dedicated profile used by these tests.
    /// 构建这些测试使用的默认专职配置。
    fn profile() -> DedicatedSessionProfile {
        DedicatedSessionProfile::new(DedicatedToolAccess::None, "Translate EN<->FR.").unwrap()
    }

    /// Submits one root input with a unique client action.
    /// 以唯一客户端动作提交一条根输入。
    fn submit(document: &mut SessionDocument, action: &str, text: &str) {
        document
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new(action).unwrap(),
                    text,
                    Vec::new(),
                ),
                NOW,
            )
            .unwrap();
    }

    /// Installation is allowed only on an empty session and freezes later setting changes.
    /// 只能在空会话上安装，安装后冻结之后的设置修改。
    #[test]
    fn installation_requires_empty_session_and_freezes_settings() {
        let mut document = empty_document();
        document
            .install_dedicated_profile(profile(), false, NOW)
            .unwrap();
        assert!(document.dedicated_profile().is_some());
        assert!(matches!(
            document.install_dedicated_profile(profile(), false, NOW),
            Err(DomainError::InvalidDedicatedProfile { .. })
        ));
        let vmm = VmmSessionPolicyConfig {
            precheck_enabled: Some(true),
            ..VmmSessionPolicyConfig::default()
        };
        assert!(matches!(
            document.set_vmm_session_policy(vmm, NOW),
            Err(DomainError::DedicatedSettingFrozen { .. })
        ));
        assert!(matches!(
            document.set_output_style(Some("brief".to_string()), NOW),
            Err(DomainError::DedicatedSettingFrozen { .. })
        ));

        let mut used = empty_document();
        submit(&mut used, "action-1", "hello");
        assert!(
            used.install_dedicated_profile(profile(), false, NOW)
                .is_err()
        );
    }

    /// The creating submission may write frozen settings until its first request starts.
    /// 创建提交在其首个请求开始之前可以写入冻结设置。
    #[test]
    fn creation_window_closes_after_first_turn() {
        let mut document = empty_document();
        document
            .install_dedicated_profile(profile(), true, NOW)
            .unwrap();
        submit(&mut document, "action-1", "hello");
        assert_eq!(
            document.set_output_style(Some("brief".to_string()), NOW),
            Ok(true)
        );
        document
            .start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                NOW,
            )
            .unwrap();
        assert!(matches!(
            document.set_output_style(Some("plain".to_string()), NOW),
            Err(DomainError::DedicatedSettingFrozen { .. })
        ));
    }

    /// Immediate locking writes the first lock note in the installation revision.
    /// 立即锁定会在安装修订中写入第一条锁定通知。
    #[test]
    fn immediate_lock_writes_first_note() {
        let mut document = empty_document();
        let seq = document
            .install_dedicated_profile(profile(), true, NOW)
            .unwrap()
            .unwrap();
        let profile = document.dedicated_profile().unwrap();
        assert!(profile.is_locked());
        assert_eq!(profile.locked_spans[0].locked_at_seq, seq);
        assert!(matches!(
            &document.events().last().unwrap().event,
            EventBody::DisplayNote(note) if note.notice_kind == DisplayNoticeKind::MemoryLocked
        ));
    }

    /// Locking is refused on ordinary sessions and while a turn is open; repeating it is a no-op.
    /// 普通会话及开放回合期间拒绝锁定；重复锁定是空操作。
    #[test]
    fn lock_requires_dedicated_session_between_turns() {
        let mut ordinary = empty_document();
        assert_eq!(
            ordinary.set_dedicated_memory_locked(true, NOW),
            Err(DomainError::NotDedicatedSession)
        );
        let mut document = empty_document();
        document
            .install_dedicated_profile(profile(), false, NOW)
            .unwrap();
        submit(&mut document, "action-1", "hello");
        assert!(matches!(
            document.set_dedicated_memory_locked(true, NOW),
            Err(DomainError::DedicatedMemoryLockWhileTurnOpen { .. })
        ));
        let mut idle = empty_document();
        idle.install_dedicated_profile(profile(), true, NOW)
            .unwrap();
        assert_eq!(idle.set_dedicated_memory_locked(true, NOW), Ok(None));
        assert!(
            idle.set_dedicated_memory_locked(false, NOW)
                .unwrap()
                .is_some()
        );
        assert!(!idle.dedicated_profile().unwrap().is_locked());
    }

    /// While locked, the open turn is projected but a closed locked turn is forgotten.
    /// 锁定期间开放回合会被投影，已闭合的锁定回合则被遗忘。
    #[test]
    fn locked_turns_are_forgotten_once_closed() {
        let mut document = empty_document();
        document
            .install_dedicated_profile(profile(), true, NOW)
            .unwrap();
        submit(&mut document, "action-1", "first locked input");
        let profile = document.dedicated_profile().unwrap().clone();
        let open_turn = document.active_turn().map(|turn| turn.turn_id);
        let visible = dedicated_provider_events(&profile, document.events(), open_turn);
        let items = project_provider_items(&visible, None, &[]).unwrap();
        assert_eq!(items.len(), 1, "the open locked turn must stay visible");

        // Once that turn is no longer the open one, it disappears from the request.
        // 一旦该回合不再是开放回合，它就从请求中消失。
        let visible = dedicated_provider_events(&profile, document.events(), None);
        let items = project_provider_items(&visible, None, &[]).unwrap();
        assert!(items.is_empty(), "a closed locked turn must be forgotten");
    }
}
