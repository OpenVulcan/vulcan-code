//! Answer mode facts shared by the service and synthetic backends.
//! 真实服务与合成后端共享的回答模式事实。

pub use vulcan_session_core::v1::config::{AnswerMode, AnswerModePreference};

/// Durable answer mode of one loaded conversation or draft.
/// 单个已加载会话或草稿的可靠回答模式。
#[derive(Debug, Clone, Default)]
pub struct AnswerModeSession {
    /// None means the authority is not loaded, rather than a confirmed standard mode.
    /// 为空表示权威尚未加载，不表示已确认标准模式。
    pub mode: Option<AnswerModePreference>,
}

impl AnswerModeSession {
    /// Apply only forward saved/prepared revisions.
    /// 仅应用向前的已保存及准备修订。
    pub fn accept_mode(&mut self, next: AnswerModePreference) {
        let newer = self.mode.as_ref().is_none_or(|current| {
            if next.revision != current.revision {
                return next.revision > current.revision;
            }
            next.prepared.as_ref().is_some_and(|prepared| {
                current
                    .prepared
                    .as_ref()
                    .is_none_or(|old| prepared.revision > old.revision)
            })
        });
        if newer {
            self.mode = Some(next);
        }
    }

    /// Return whether the saved mode has not reached the next prepared business request.
    /// 返回已保存模式是否尚未到达下一次业务请求准备阶段。
    pub fn awaiting_request(&self) -> bool {
        self.mode.as_ref().is_some_and(|mode| {
            mode.prepared
                .as_ref()
                .map_or(mode.mode == AnswerMode::Interactive, |prepared| {
                    prepared.revision != mode.revision
                })
        })
    }

    /// Return whether the loaded mode is interactive.
    /// 返回已加载模式是否为互动。
    pub fn interactive(&self) -> bool {
        self.mode
            .as_ref()
            .is_some_and(|mode| mode.mode == AnswerMode::Interactive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A late save acknowledgement cannot erase the request's already-confirmed preparation.
    /// 迟到的保存确认不能抹除请求已确认的准备状态。
    #[test]
    fn answer_mode_preparation_never_regresses() {
        let mut state = AnswerModeSession::default();
        let desired: AnswerModePreference = serde_json::from_value(
            serde_json::json!({"mode":"interactive","revision":2,"prepared":null}),
        )
        .unwrap();
        state.accept_mode(desired.clone());
        assert!(state.awaiting_request());
        let prepared: AnswerModePreference = serde_json::from_value(serde_json::json!({
            "mode":"interactive","revision":2,"prepared":{"mode":"interactive","revision":2}
        }))
        .unwrap();
        state.accept_mode(prepared.clone());
        state.accept_mode(desired);
        assert_eq!(state.mode, Some(prepared));
        assert!(!state.awaiting_request());
    }
}
