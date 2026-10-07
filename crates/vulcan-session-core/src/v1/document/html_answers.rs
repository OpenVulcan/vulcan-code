//! Revision-checked desired preferences and atomic request-boundary HTML context snapshots.
//! 修订校验的期望偏好与请求边界的原子 HTML 上下文快照。

use super::{DirtyFile, IdempotentMutation, SessionDocument, validate_timestamp};
use crate::v1::config::HtmlAnswerPreparedMode;
use crate::v1::ids::JS_SAFE_INTEGER_MAX;
use crate::v1::{DomainError, DynamicContextInput, Seq, SystemContextScope};

/// Unique dynamic producer shared by mode changes, compaction replay and generation reset.
/// 模式变化、压缩重放和代次重置共用的唯一动态生产者。
pub const HTML_ANSWER_CONTEXT_KEY: &str = "dynamic.html_answers";

impl SessionDocument {
    /// Save desired `enabled` only if `expected_revision` still matches; return whether it changed.
    /// 仅当 `expected_revision` 仍匹配时保存期望的 `enabled`；返回是否发生变化。
    /// `now_ms` timestamps metadata only. Active APIs, events, Goal and Plan remain unchanged.
    /// `now_ms` 仅记录元数据时间。活动 API、事件、Goal 和 Plan 保持不变。
    pub fn set_html_answer_mode(
        &mut self,
        enabled: bool,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("html_answers.updated_at_ms", now_ms)?;
        // Lifecycle admission may wait behind archival; validate the document under its writer.
        // 生命周期准入可能等待归档完成；因此必须在写入器内校验文档本身。
        if self.metadata.archived_at_ms.is_some() {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "html_answers.archived_session",
            });
        }
        let current = &self.metadata.config.html_answers;
        if current.revision != expected_revision {
            return Err(DomainError::HtmlAnswerModeConflict {
                expected_revision,
                actual_revision: current.revision,
            });
        }
        if current.enabled == enabled {
            return Ok(false);
        }
        if current.revision >= JS_SAFE_INTEGER_MAX {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "html_answers.revision",
            });
        }
        self.next_mutation_revision()?;
        self.metadata.config.html_answers.enabled = enabled;
        self.metadata.config.html_answers.revision += 1;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Synchronize the full producer inventory plus HTML guidance at one business preparation boundary.
    /// 在一次业务准备边界同步完整生产者清单及 HTML 指引。
    /// `inputs` excludes the HTML producer; `enabled_body`/`disabled_body` are authoritative prompt resources.
    /// `inputs` 不含 HTML 生产者；`enabled_body` 和 `disabled_body` 是权威提示词资源。
    /// Returns appended context seqs with idempotent/changed status; the prepared preference commits atomically.
    /// 返回追加的上下文序号及幂等或变更状态；准备偏好与上下文原子提交。
    pub fn synchronize_context_with_html_answers(
        &mut self,
        mut inputs: Vec<DynamicContextInput>,
        enabled_body: &str,
        disabled_body: &str,
        now_ms: i64,
    ) -> Result<IdempotentMutation<Vec<Seq>>, DomainError> {
        if let Some(active) = self.active_api() {
            return Err(DomainError::ModeTransitionWhileApiActive {
                api_call_id: active.api_call_id.value(),
            });
        }
        if inputs
            .iter()
            .any(|input| input.producer_key == HTML_ANSWER_CONTEXT_KEY)
        {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "html_answers.duplicate_producer",
            });
        }
        let desired = HtmlAnswerPreparedMode {
            enabled: self.metadata.config.html_answers.enabled,
            revision: self.metadata.config.html_answers.revision,
        };
        // An untouched default-off session needs no revocation message; an observed producer does.
        // 未触碰的默认关闭会话不需要撤销消息；已有生产者则需要。
        let raw_content = if desired.enabled {
            Some(enabled_body.to_owned())
        } else if self
            .metadata
            .dynamic_context
            .contains_key(HTML_ANSWER_CONTEXT_KEY)
        {
            Some(disabled_body.to_owned())
        } else {
            None
        };
        inputs.push(DynamicContextInput::new(
            HTML_ANSWER_CONTEXT_KEY,
            raw_content,
            SystemContextScope::Session,
            None,
        ));
        let preparation_changed = self.metadata.config.html_answers.prepared != Some(desired);
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
        self.metadata.config.html_answers.prepared = Some(desired);
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

    /// An archive that wins admission keeps its saved preference and revision unchanged.
    /// 优先完成准入的归档保持已保存偏好和修订不变。
    #[test]
    fn html_answers_archived_session_rejects_preference_changes() {
        let mut document = SessionDocument::new("html-archive", "p", "t", 1);
        document.set_archived_at(Some(2), 2).unwrap();
        let before = document.metadata.clone();
        assert!(matches!(
            document.set_html_answer_mode(true, 0, 3),
            Err(DomainError::InvalidSelectionConfiguration {
                field: "html_answers.archived_session"
            })
        ));
        assert_eq!(document.metadata, before);
    }

    /// Saving during an API updates only desired state; conflicts and repeated writes preserve facts.
    /// API 期间保存仅更新期望状态；冲突和重复写入保持原有事实。
    #[test]
    fn html_answers_desired_toggle_is_independent_and_revision_checked() {
        let mut document = SessionDocument::new("html-mode", "p", "t", 1);
        document
            .submit_root_input_fixture("hello", Vec::new(), 2)
            .unwrap();
        document
            .synchronize_context_with_html_answers(Vec::new(), "enabled", "disabled", 3)
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
        assert!(document.set_html_answer_mode(true, 0, 5).unwrap());
        assert_eq!(document.events(), events);
        assert_eq!(document.active_api(), active.as_ref());
        assert!(
            !document
                .metadata
                .config
                .html_answers
                .prepared
                .unwrap()
                .enabled
        );
        let revision = document.memory_revision();
        assert!(!document.set_html_answer_mode(true, 1, 6).unwrap());
        assert!(matches!(
            document.set_html_answer_mode(false, 0, 6),
            Err(DomainError::HtmlAnswerModeConflict { .. })
        ));
        assert_eq!(document.memory_revision(), revision);
        assert!(document.metadata.config.html_answers.enabled);
        assert!(
            document
                .synchronize_context_with_html_answers(Vec::new(), "enabled", "disabled", 7)
                .is_err()
        );
        assert_eq!(document.memory_revision(), revision);
    }

    /// Context is explicit on exit, idempotent across repeated preparation and failure-atomic.
    /// 退出上下文明示关闭，重复准备幂等，失败时原子回滚。
    #[test]
    fn html_answers_context_preparation_is_atomic_and_only_emits_observed_changes() {
        let mut document = SessionDocument::new("html-mode", "p", "t", 1);
        document
            .submit_root_input_fixture("hello", Vec::new(), 2)
            .unwrap();
        let synchronize = |document: &mut SessionDocument| {
            document.synchronize_context_with_html_answers(
                Vec::new(),
                "HTML allowed",
                "HTML preference disabled",
                3,
            )
        };
        assert!(synchronize(&mut document).unwrap().into_value().is_empty());
        assert!(
            !document
                .metadata
                .dynamic_context
                .contains_key(HTML_ANSWER_CONTEXT_KEY)
        );
        assert!(!synchronize(&mut document).unwrap().changed());
        document.set_html_answer_mode(true, 0, 4).unwrap();
        document.set_html_answer_mode(false, 1, 5).unwrap();
        assert!(synchronize(&mut document).unwrap().into_value().is_empty());
        document.set_html_answer_mode(true, 2, 6).unwrap();
        let before = document.clone();
        assert!(
            document
                .synchronize_context_with_html_answers(Vec::new(), " ", "disabled", 7)
                .is_err()
        );
        assert_eq!(document.metadata, before.metadata);
        assert_eq!(document.events(), before.events());
        assert_eq!(document.memory_revision(), before.memory_revision());
        assert_eq!(synchronize(&mut document).unwrap().into_value().len(), 1);
        assert!(!synchronize(&mut document).unwrap().changed());
        document.set_html_answer_mode(false, 3, 8).unwrap();
        assert_eq!(synchronize(&mut document).unwrap().into_value().len(), 1);
        assert!(!synchronize(&mut document).unwrap().changed());
        assert_eq!(
            document.metadata.config.html_answers.prepared,
            Some(HtmlAnswerPreparedMode {
                enabled: false,
                revision: 4
            })
        );
    }

    /// Current wire data requires every field and cannot lie about a newer or incompatible preparation.
    /// 当前传输数据要求全部字段，不能谎报更新或不兼容的准备状态。
    #[test]
    fn html_answers_mode_wire_is_strict() {
        use crate::v1::config::HtmlAnswerMode;
        let default = HtmlAnswerMode::default();
        let wire = serde_json::to_value(&default).unwrap();
        assert_eq!(
            serde_json::from_value::<HtmlAnswerMode>(wire.clone()).unwrap(),
            default
        );
        for key in ["enabled", "revision", "prepared"] {
            let mut incomplete = wire.clone();
            incomplete.as_object_mut().unwrap().remove(key);
            assert!(
                serde_json::from_value::<HtmlAnswerMode>(incomplete).is_err(),
                "missing {key}"
            );
        }
        for invalid in [
            serde_json::json!({"enabled": false,"revision": JS_SAFE_INTEGER_MAX + 1,"prepared": null}),
            serde_json::json!({"enabled": false,"revision": 0,"prepared": {"enabled": false,"revision": 1}}),
            serde_json::json!({"enabled": false,"revision": 0,"prepared": {"enabled": true,"revision": 0}}),
        ] {
            assert!(serde_json::from_value::<HtmlAnswerMode>(invalid).is_err());
        }
    }

    /// An enabled conversation receives upgraded renderer guidance once without rewriting history.
    /// 已开启会话仅接收一次升级后的渲染指引，不重写历史。
    #[test]
    fn html_answers_prompt_upgrade_is_append_only_without_a_toggle() {
        let mut document = SessionDocument::new("html-upgrade", "p", "t", 1);
        document.set_html_answer_mode(true, 0, 2).unwrap();
        document
            .synchronize_context_with_html_answers(Vec::new(), "old renderer", "off", 3)
            .unwrap();
        let before = document.events().to_vec();
        let mode = document.metadata.config.html_answers.clone();
        assert!(
            document
                .synchronize_context_with_html_answers(Vec::new(), "new renderer", "off", 4)
                .unwrap()
                .changed()
        );
        assert!(document.events().starts_with(&before));
        assert_eq!(document.metadata.config.html_answers, mode);
        assert_eq!(
            document.metadata.dynamic_context[HTML_ANSWER_CONTEXT_KEY].raw_content,
            "new renderer"
        );
        assert!(
            !document
                .synchronize_context_with_html_answers(Vec::new(), "new renderer", "off", 5)
                .unwrap()
                .changed()
        );
    }
}
