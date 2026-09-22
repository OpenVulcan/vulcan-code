//! Per-action authorization coverage for the native collaboration port.
//! 原生协作端口的逐动作授权覆盖。

use super::*;
use vulcan_agent_runtime::tooling::collaboration::{
    ASK_QUESTION_TOOL_ID, AskQuestionArgs, LIST_SESSIONS_TOOL_ID, ListSessionsArgs,
    READ_SESSION_TOOL_ID, ReadSessionArgs, SEND_NOTIFICATION_TOOL_ID, SendNotificationArgs,
};

/// Adds one user-owned cross-project grant for exactly one action.
/// 为恰好一个动作添加一条用户持有的跨项目授权。
async fn add_grant(dispatch: &NativeDispatch, action: CollaborationGrantAction) -> String {
    let granted = crate::project_conversation_services::service_add_collaboration_grant(
        dispatch.invocation.state.clone(),
        dispatch.invocation.source_session_id.to_string(),
        dispatch.target.session_id().to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [action].into_iter().collect(),
    )
    .await
    .unwrap();
    granted["grant"]["grant_id"].as_str().unwrap().to_owned()
}

/// Revokes one previously granted cross-project action.
/// 撤销此前授予的一条跨项目动作。
async fn revoke_grant(dispatch: &NativeDispatch, grant_id: String) {
    crate::project_conversation_services::service_remove_collaboration_grant(
        dispatch.invocation.state.clone(),
        dispatch.invocation.source_session_id.to_string(),
        grant_id,
        format!("user-grant-remove:v1:{}", Uuid::new_v4()),
    )
    .await
    .unwrap();
}

/// A cross-project notification must require its OWN grant, not a read grant.
/// 跨项目通知必须要求它自己的授权，而不是读取授权。
#[tokio::test]
async fn cross_project_notification_requires_notify_grant_specifically() {
    let dispatch = native_dispatch_fixture(true, SEND_NOTIFICATION_TOOL_ID).await;
    // A read grant must not authorize sending unsolicited data to the target.
    // 读取授权不得授权向目标发送未获授权的数据。
    let read_grant = add_grant(&dispatch, CollaborationGrantAction::Read).await;
    let refused = dispatch
        .invoke(AgentCollaborationCommand::SendNotification(
            SendNotificationArgs {
                target_session_id: dispatch.target.session_id().to_string(),
                message: "Read grant must not send this.".into(),
            },
        ))
        .await
        .unwrap_err();
    assert!(refused.contains("grant"), "{refused}");
    revoke_grant(&dispatch, read_grant).await;
    // The matching grant authorizes exactly this action.
    // 匹配的授权恰好授权该动作。
    let notify_grant = add_grant(&dispatch, CollaborationGrantAction::Notify).await;
    let accepted = dispatch
        .invoke(AgentCollaborationCommand::SendNotification(
            SendNotificationArgs {
                target_session_id: dispatch.target.session_id().to_string(),
                message: "Notify grant sends this.".into(),
            },
        ))
        .await
        .unwrap();
    assert_eq!(accepted["status"], "accepted");
    // Revocation after grant must be observed on the next dispatch.
    // 授权撤销必须在下次分发时被观察到。
    revoke_grant(&dispatch, notify_grant).await;
    let dispatch = dispatch.retool(SEND_NOTIFICATION_TOOL_ID).await;
    let revoked = dispatch
        .invoke(AgentCollaborationCommand::SendNotification(
            SendNotificationArgs {
                target_session_id: dispatch.target.session_id().to_string(),
                message: "Revoked notify grant must not send this.".into(),
            },
        ))
        .await
        .unwrap_err();
    assert!(revoked.contains("grant"), "{revoked}");
}

/// A cross-project question must require an Ask grant and must not accept a read grant.
/// 跨项目提问必须要求提问授权，且不得接受读取授权。
#[tokio::test]
async fn cross_project_question_requires_ask_grant_specifically() {
    let dispatch = native_dispatch_fixture(true, ASK_QUESTION_TOOL_ID).await;
    let absent = dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "No grant at all.".into(),
        }))
        .await
        .unwrap_err();
    assert!(absent.contains("grant"), "{absent}");
    let read_grant = add_grant(&dispatch, CollaborationGrantAction::Read).await;
    let refused = dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Read grant must not ask.".into(),
        }))
        .await
        .unwrap_err();
    assert!(refused.contains("grant"), "{refused}");
    revoke_grant(&dispatch, read_grant).await;
    let ask_grant = add_grant(&dispatch, CollaborationGrantAction::Ask).await;
    let asked = dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Ask grant asks this.".into(),
        }))
        .await
        .unwrap();
    assert_eq!(asked["status"], "ok");
    revoke_grant(&dispatch, ask_grant).await;
    let dispatch = dispatch.retool(ASK_QUESTION_TOOL_ID).await;
    let revoked = dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Revoked ask grant must not ask.".into(),
        }))
        .await
        .unwrap_err();
    assert!(revoked.contains("grant"), "{revoked}");
}

/// Cross-project listing and reading must be scoped by the Read grant, not by display labels.
/// 跨项目列出与读取必须由读取授权界定范围，而不是由展示标签决定。
#[tokio::test]
async fn cross_project_listing_and_reading_follow_read_grant_scope() {
    let dispatch = native_dispatch_fixture(true, LIST_SESSIONS_TOOL_ID).await;
    // Without a grant the foreign project must not appear in the authorized inventory.
    // 无授权时，外部项目不得出现在获授权清单中。
    let listed = dispatch
        .invoke(AgentCollaborationCommand::ListSessions(ListSessionsArgs {
            limit: 20,
            cursor: None,
        }))
        .await
        .unwrap();
    assert_eq!(listed["status"], "ok");
    let foreign = dispatch.target.session_id().to_string();
    assert!(
        listed["sessions"]
            .as_array()
            .expect("session list")
            .iter()
            .all(|entry| entry["session_id"] != foreign),
        "an ungranted foreign session must not be listed: {listed}"
    );
    // Reading the foreign session without a grant must fail explicitly.
    // 无授权读取外部会话必须显式失败。
    let dispatch = dispatch.retool(READ_SESSION_TOOL_ID).await;
    let refused = dispatch
        .invoke(AgentCollaborationCommand::ReadSession(ReadSessionArgs {
            session_id: foreign.clone(),
            after_seq: None,
            limit: 10,
        }))
        .await
        .unwrap_err();
    assert!(refused.contains("grant"), "{refused}");
    // A Read grant authorizes reading that exact session by id, but listing stays project-scoped:
    // the authorized inventory of one call is its own project, and a foreign grant never widens it.
    // 读取授权允许按 ID 读取该精确会话，但列表仍限定在来源项目范围：
    // 单次调用的获授权清单就是它自己的项目，外部授权不会扩大它。
    let read_grant = add_grant(&dispatch, CollaborationGrantAction::Read).await;
    let dispatch = dispatch.retool(LIST_SESSIONS_TOOL_ID).await;
    let listed = dispatch
        .invoke(AgentCollaborationCommand::ListSessions(ListSessionsArgs {
            limit: 20,
            cursor: None,
        }))
        .await
        .unwrap();
    assert!(
        listed["sessions"]
            .as_array()
            .expect("session list")
            .iter()
            .all(|entry| entry["session_id"] != foreign),
        "a granted foreign session must not enter the source project inventory: {listed}"
    );
    assert_eq!(
        listed["project_id"],
        dispatch.invocation.source_locator.project_id()
    );
    let dispatch = dispatch.retool(READ_SESSION_TOOL_ID).await;
    let read = dispatch
        .invoke(AgentCollaborationCommand::ReadSession(ReadSessionArgs {
            session_id: foreign.clone(),
            after_seq: None,
            limit: 10,
        }))
        .await
        .unwrap();
    assert_eq!(read["status"], "ok");
    assert_eq!(read["session_id"], foreign);
    // Revocation removes the read authority, so the same id can no longer be read.
    // 撤销移除读取权威，因此同一 ID 不再可读取。
    revoke_grant(&dispatch, read_grant).await;
    let dispatch = dispatch.retool(READ_SESSION_TOOL_ID).await;
    let revoked = dispatch
        .invoke(AgentCollaborationCommand::ReadSession(ReadSessionArgs {
            session_id: dispatch.target.session_id().to_string(),
            after_seq: None,
            limit: 10,
        }))
        .await
        .unwrap_err();
    assert!(revoked.contains("grant"), "{revoked}");
}
