//! Revision-checked answer mode preference and atomic request-boundary guidance snapshots.
//! 修订校验的回答模式偏好与请求边界的原子指引快照。

use super::{DirtyFile, IdempotentMutation, SessionDocument, validate_timestamp};
use crate::v1::config::{AnswerMode, AnswerModePrepared};
use crate::v1::ids::JS_SAFE_INTEGER_MAX;
use crate::v1::{DomainError, DynamicContextInput, Seq, SystemContextScope};

/// Unique dynamic producer shared by mode changes, compaction replay and generation reset.
/// 模式变化、压缩重放和代次重置共用的唯一动态生产者。
/// The persisted value predates the rename from HTML answers; keeping it lets existing histories
/// receive the replacement guidance under the same producer without legacy branches.
/// 持久化取值早于从 HTML 回答的更名；保留它可让既有历史在同一生产者下收到替换指引，无需兼容分支。
pub const ANSWER_MODE_CONTEXT_KEY: &str = "dynamic.html_answers";

impl SessionDocument {
    /// Save `mode` only if `expected_revision` still matches; return whether it changed.
    /// 仅当 `expected_revision` 仍匹配时保存 `mode`；返回是否发生变化。
    /// `now_ms` timestamps metadata only. Active APIs, events, Goal and Plan remain unchanged.
    /// `now_ms` 仅记录元数据时间。活动 API、事件、Goal 和 Plan 保持不变。
    pub fn set_answer_mode(
        &mut self,
        mode: AnswerMode,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("answer_mode.updated_at_ms", now_ms)?;
        // Lifecycle admission may wait behind archival; validate the document under its writer.
        // 生命周期准入可能等待归档完成；因此必须在写入器内校验文档本身。
        if self.metadata.archived_at_ms.is_some() {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "answer_mode.archived_session",
            });
        }
        let current = &self.metadata.config.answer_mode;
        if current.revision != expected_revision {
            return Err(DomainError::AnswerModeConflict {
                expected_revision,
                actual_revision: current.revision,
            });
        }
        if current.mode == mode {
            return Ok(false);
        }
        self.reject_dedicated_change("answer_mode")?;
        if current.revision >= JS_SAFE_INTEGER_MAX {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "answer_mode.revision",
            });
        }
        self.next_mutation_revision()?;
        self.metadata.config.answer_mode.mode = mode;
        self.metadata.config.answer_mode.revision += 1;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Synchronize the full producer inventory plus answer mode guidance at one business preparation.
    /// 在一次业务准备边界同步完整生产者清单及回答模式指引。
    /// `inputs` excludes the answer mode producer; `interactive_body`/`standard_body` are
    /// authoritative prompt resources.
    /// `inputs` 不含回答模式生产者；两份正文是权威提示词资源。
    /// Returns appended context seqs with idempotent/changed status; the prepared mode commits atomically.
    /// 返回追加的上下文序号及幂等/变化状态；准备模式原子提交。
    pub fn synchronize_context_with_answer_mode(
        &mut self,
        mut inputs: Vec<DynamicContextInput>,
        interactive_body: &str,
        standard_body: &str,
        now_ms: i64,
    ) -> Result<IdempotentMutation<Vec<Seq>>, DomainError> {
        if let Some(active) = self.active_api() {
            return Err(DomainError::ModeTransitionWhileApiActive {
                api_call_id: active.api_call_id.value(),
            });
        }
        if inputs
            .iter()
            .any(|input| input.producer_key == ANSWER_MODE_CONTEXT_KEY)
        {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "answer_mode.duplicate_producer",
            });
        }
        let preference = &self.metadata.config.answer_mode;
        let desired = AnswerModePrepared {
            mode: preference.mode,
            revision: preference.revision,
        };
        // An untouched standard session needs no notice; once guidance was observed, leaving it does.
        // 未触碰的标准会话不需要说明；一旦观察到过指引，离开时就需要说明。
        let raw_content = match desired.mode {
            AnswerMode::Interactive => Some(interactive_body.to_owned()),
            AnswerMode::Standard
                if self
                    .metadata
                    .dynamic_context
                    .contains_key(ANSWER_MODE_CONTEXT_KEY) =>
            {
                Some(standard_body.to_owned())
            }
            AnswerMode::Standard => None,
        };
        inputs.push(DynamicContextInput::new(
            ANSWER_MODE_CONTEXT_KEY,
            raw_content,
            SystemContextScope::Session,
            None,
        ));
        let preparation_changed = self.metadata.config.answer_mode.prepared != Some(desired);
        if preparation_changed {
            self.next_mutation_revision()?;
        }
        // Context synchronization stages all fallible work before publishing. Preflight the
        // metadata-only revision above, then finish under the same writer without cloning history.
        // 上下文同步先完成全部可失败工作再发布；上面预检元数据修订后，在同一写锁内完成且不复制历史。
        let synchronized = self.synchronize_dynamic_context(inputs, now_ms)?;
        if !preparation_changed {
            return Ok(synchronized);
        }
        self.metadata.config.answer_mode.prepared = Some(desired);
        if !synchronized.changed() {
            self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        }
        Ok(IdempotentMutation::Committed(synchronized.into_value()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::{ApiPurposeKind, event::ApiRequestSnapshot};

    const STANDARD: AnswerMode = AnswerMode::Standard;
    const INTERACTIVE: AnswerMode = AnswerMode::Interactive;

    /// An archive that wins admission keeps its saved preference and revision unchanged.
    /// 优先完成准入的归档保持已保存偏好和修订不变。
    #[test]
    fn answer_mode_archived_session_rejects_preference_changes() {
        let mut document = SessionDocument::new("mode-archive", "p", "t", 1);
        document.set_archived_at(Some(2), 2).unwrap();
        let before = document.metadata.clone();
        assert!(matches!(
            document.set_answer_mode(INTERACTIVE, 0, 3),
            Err(DomainError::InvalidSelectionConfiguration {
                field: "answer_mode.archived_session"
            })
        ));
        assert_eq!(document.metadata, before);
    }

    /// Saving during an API updates only desired state; conflicts and repeated writes preserve facts.
    /// API 期间保存仅更新期望状态；冲突和重复写入保持原有事实。
    #[test]
    fn answer_mode_save_is_independent_and_revision_checked() {
        let mut document = SessionDocument::new("mode", "p", "t", 1);
        // Start from an explicit standard mode so the test does not depend on the system default.
        // 从显式标准模式开始，使测试不依赖系统默认值。
        document.metadata.config.answer_mode.mode = STANDARD;
        document
            .submit_root_input_fixture("hello", Vec::new(), 2)
            .unwrap();
        document
            .synchronize_context_with_answer_mode(Vec::new(), "on", "off", 3)
            .unwrap();
        document
            .start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("p", "m"),
                4,
            )
            .unwrap();
        let events = document.events().to_vec();
        let active = document.active_api().cloned();
        assert!(document.set_answer_mode(INTERACTIVE, 0, 5).unwrap());
        assert_eq!(document.events(), events);
        assert_eq!(document.active_api(), active.as_ref());
        assert_eq!(
            document.metadata.config.answer_mode.prepared.unwrap().mode,
            STANDARD
        );
        let revision = document.memory_revision();
        assert!(!document.set_answer_mode(INTERACTIVE, 1, 6).unwrap());
        assert!(matches!(
            document.set_answer_mode(STANDARD, 0, 6),
            Err(DomainError::AnswerModeConflict { .. })
        ));
        assert_eq!(document.memory_revision(), revision);
        assert!(
            document
                .synchronize_context_with_answer_mode(Vec::new(), "on", "off", 7)
                .is_err()
        );
        assert_eq!(document.memory_revision(), revision);
    }

    /// Guidance is explicit on exit, idempotent across repeated preparation, and failure-atomic.
    /// 退出时明示说明，重复准备幂等，失败时原子回滚。
    #[test]
    fn answer_mode_preparation_only_emits_observed_changes() {
        let mut document = SessionDocument::new("mode", "p", "t", 1);
        // Start from an explicit standard mode so the test does not depend on the system default.
        // 从显式标准模式开始，使测试不依赖系统默认值。
        document.metadata.config.answer_mode.mode = STANDARD;
        document
            .submit_root_input_fixture("hello", Vec::new(), 2)
            .unwrap();
        let synchronize = |document: &mut SessionDocument| {
            document.synchronize_context_with_answer_mode(
                Vec::new(),
                "components allowed",
                "components off",
                3,
            )
        };
        assert!(synchronize(&mut document).unwrap().into_value().is_empty());
        assert!(
            !document
                .metadata
                .dynamic_context
                .contains_key(ANSWER_MODE_CONTEXT_KEY)
        );
        assert!(!synchronize(&mut document).unwrap().changed());
        document.set_answer_mode(INTERACTIVE, 0, 4).unwrap();
        let before = document.clone();
        assert!(
            document
                .synchronize_context_with_answer_mode(Vec::new(), " ", "off", 7)
                .is_err()
        );
        assert_eq!(document.metadata, before.metadata);
        assert_eq!(document.events(), before.events());
        assert_eq!(document.memory_revision(), before.memory_revision());
        assert_eq!(synchronize(&mut document).unwrap().into_value().len(), 1);
        assert!(!synchronize(&mut document).unwrap().changed());
        assert_eq!(
            document.metadata.config.answer_mode.prepared,
            Some(AnswerModePrepared {
                mode: INTERACTIVE,
                revision: 1
            })
        );
        document.set_answer_mode(STANDARD, 1, 5).unwrap();
        assert_eq!(synchronize(&mut document).unwrap().into_value().len(), 1);
        assert_eq!(
            document.metadata.dynamic_context[ANSWER_MODE_CONTEXT_KEY].raw_content,
            "components off"
        );
    }

    /// Current wire data requires every field and cannot claim a preparation ahead of the preference.
    /// 当前传输数据要求全部字段，不能声称准备状态领先于偏好。
    #[test]
    fn answer_mode_wire_is_strict() {
        use crate::v1::config::AnswerModePreference;
        let default = AnswerModePreference::default();
        let wire = serde_json::to_value(&default).unwrap();
        assert_eq!(
            serde_json::from_value::<AnswerModePreference>(wire.clone()).unwrap(),
            default
        );
        for key in ["mode", "revision", "prepared"] {
            let mut incomplete = wire.clone();
            incomplete.as_object_mut().unwrap().remove(key);
            assert!(
                serde_json::from_value::<AnswerModePreference>(incomplete).is_err(),
                "missing {key}"
            );
        }
        for invalid in [
            serde_json::json!({"mode": "standard","revision": JS_SAFE_INTEGER_MAX + 1,"prepared": null}),
            serde_json::json!({"mode": "standard","revision": 0,"prepared": {"mode": "standard","revision": 1}}),
            serde_json::json!({"mode": "html","revision": 0,"prepared": null}),
            serde_json::json!({"mode": null,"revision": 0,"prepared": null}),
        ] {
            assert!(serde_json::from_value::<AnswerModePreference>(invalid).is_err());
        }
    }

    /// An interactive conversation receives upgraded guidance once without rewriting history.
    /// 互动会话仅接收一次升级后的指引，不重写历史。
    #[test]
    fn answer_mode_guidance_upgrade_is_append_only_without_a_toggle() {
        let mut document = SessionDocument::new("mode-upgrade", "p", "t", 1);
        document.set_answer_mode(INTERACTIVE, 0, 2).unwrap();
        document
            .synchronize_context_with_answer_mode(Vec::new(), "old guide", "off", 3)
            .unwrap();
        let before = document.events().to_vec();
        let mode = document.metadata.config.answer_mode.clone();
        assert!(
            document
                .synchronize_context_with_answer_mode(Vec::new(), "new guide", "off", 4)
                .unwrap()
                .changed()
        );
        assert!(document.events().starts_with(&before));
        assert_eq!(document.metadata.config.answer_mode, mode);
        assert_eq!(
            document.metadata.dynamic_context[ANSWER_MODE_CONTEXT_KEY].raw_content,
            "new guide"
        );
    }
}
