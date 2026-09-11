//! End-to-end draft admission after recovering a malformed Goal session.
//! 恢复异常目标会话后的端到端草稿准入验证。

use super::*;
use crate::conversation_drafts_v1::service_session_draft_v1;
use vulcan_session_core::v1::{DisplayNoticeKind, NoticeSeverity, TurnOutcome};
use vulcan_workspace_store::workspace_store::load_session_v1_tail;

/// The real draft RPC must recover both ownership corruption and an unclosed pre-API failure.
/// 真实草稿 RPC 必须恢复归属损坏以及未闭合的 API 前失败。
#[tokio::test(flavor = "current_thread")]
async fn goal_recovery_unblocks_draft_rpc_and_preserves_unsent_text() {
    // Isolated SQLite and session files exercise production loading without touching user data.
    // 隔离 SQLite 与会话文件验证生产加载路径，不触碰用户数据。
    let session_id = SessionId::from_u128(0x62d1);
    let fixture = TestHostRuntimeRoot::new(session_id);
    let controller = fixture.attached_host_runtime_handle().await;
    let locator = fixture.v1_storage_locator();
    let mut document = load_session_v1_tail(&locator).unwrap().document;
    document.create_goal("work", None, "work", 2).unwrap();
    document.block_goal("goal_blocked", 3).unwrap();
    let closed_turn = document.active_turn().unwrap().turn_id;
    document.close_turn(TurnOutcome::Completed, 2, 4).unwrap();
    let edited = document
        .update_goal_definition("edited", None, "edited", 5)
        .unwrap()
        .unwrap();
    document.resume_goal("edited", None, "edited", 6).unwrap();
    document.block_goal("goal_blocked", 7).unwrap();
    document
        .append_display_note(
            Some(document.active_turn().unwrap().turn_id),
            DisplayNoticeKind::SystemError,
            NoticeSeverity::Error,
            "stream_finished_without_v1_api_terminal",
            None,
            None,
            None,
            None,
            8,
        )
        .unwrap();
    document.set_draft("my unsent prompt", vec![], 9).unwrap();
    let handle = crate::session_runtime_registry_v1::SessionHandle::new(
        session_id,
        locator.clone(),
        document,
    );
    handle.flush_all().unwrap();
    // Reintroduce only the historical producer bug in the exact active segment.
    // 仅在精确活动分片中重新引入历史生产端缺陷。
    let loaded = load_session_v1_tail(&locator).unwrap();
    let mut events = loaded.document.events().to_vec();
    for event in &mut events {
        if Some(event.seq) == edited.context_seq || event.seq == edited.notice_seq {
            event.turn_id = Some(closed_turn);
        }
    }
    // Generation directories use the frozen seven-digit SessionV1Layout contract.
    // generation 目录使用冻结的七位 SessionV1Layout 契约。
    let tail = fixture.v1_session_directory().join(format!(
        "segments/g{:07}/{}",
        loaded.metadata.segment_generation,
        loaded.metadata.split_list.last().unwrap().file_name
    ));
    let mut bytes = Vec::new();
    for event in events {
        serde_json::to_writer(&mut bytes, &event).unwrap();
        bytes.push(b'\n');
    }
    std::fs::write(tail, bytes).unwrap();
    assert!(load_session_v1_tail(&locator).is_err());
    let mut bridge = test_service_state_with_parent_session(session_id);
    bridge.controller_runtime = Some(controller);
    let state = Arc::new(Mutex::new(bridge));
    let response = service_session_draft_v1(
        state.clone(),
        vulcan_service_api::SessionDraftReadV1Request {
            session_id: session_id.to_string(),
        },
    )
    .await
    .unwrap();
    assert_eq!(response.text, "my unsent prompt");
    let restored = state
        .lock()
        .unwrap()
        .session_runtime_registry
        .get(session_id)
        .unwrap();
    restored
        .read(|document| {
            assert!(document.active_turn().unwrap().closed);
            assert_eq!(
                document.metadata.mode.goal.state,
                vulcan_session_core::v1::config::GoalState::Blocked
            );
            assert_eq!(document.memory_revision(), document.durable_revision());
        })
        .unwrap();
    assert!(load_session_v1_tail(&locator).is_ok());
}
