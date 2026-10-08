//! Answer mode toggle shared by the keyboard, mouse and command palette.
//! 键盘、鼠标与指令面板共享的回答模式切换。

use super::App;
use crate::{
    backend::{Command, ConnectionState},
    model::*,
    tr,
};

/// Request the opposite mode using the last authoritative revision; never optimistically save.
/// 使用最后权威修订请求相反模式；绝不乐观标记为已保存。
pub fn toggle(app: &mut App) {
    let Some(id) = app.selected.clone() else {
        return;
    };
    let Some(mode) = app
        .session()
        .and_then(|session| session.answer_mode.mode.as_ref())
    else {
        app.toast(NoticeSeverity::Warning, tr!("answerMode.saveFailed"));
        return;
    };
    if app.connection != ConnectionState::Connected {
        return;
    }
    let next = match mode.mode {
        AnswerMode::Standard => AnswerMode::Interactive,
        AnswerMode::Interactive => AnswerMode::Standard,
    };
    app.backend.send(Command::SetSessionAnswerMode(
        vulcan_service_api::SessionAnswerModeUpdateRequest {
            session_id: id,
            mode: next,
            expected_revision: mode.revision,
        },
    ));
}
