//! Synthetic answer mode semantics, without opening a real service.
//! 合成回答模式语义，不打开真实服务。

use super::*;

/// The mock follows compare-and-set instead of acknowledging stale writes.
/// 模拟后端遵循比较交换，不确认过期写入。
#[test]
fn answer_mode_mock_follows_compare_and_set() {
    let (out, events) = mpsc::channel();
    let mut world = World::new(out, Pacing::Immediate);
    let id = uuid::Uuid::new_v4().to_string();
    world.logs.insert(id.clone(), ConversationLog::default());
    for mode in [AnswerMode::Interactive, AnswerMode::Standard] {
        world.handle(Command::SetSessionAnswerMode(
            vulcan_service_api::SessionAnswerModeUpdateRequest {
                session_id: id.clone(),
                mode,
                expected_revision: 0,
            },
        ));
    }
    let mode = world.logs[&id].answer_mode.mode.as_ref().unwrap();
    assert_eq!(mode.mode, AnswerMode::Interactive);
    assert_eq!(mode.revision, 1);
    assert!(events.try_iter().any(|event| matches!(
        event,
        BackendEvent::Toast {
            severity: NoticeSeverity::Warning,
            ..
        }
    )));
}

/// Saving the global default changes only conversations created afterwards.
/// 保存全局默认只影响之后新建的会话。
#[test]
fn new_conversations_copy_the_global_default_once() {
    let (out, _events) = mpsc::channel();
    let mut world = World::new(out, Pacing::Immediate);
    let mode_of = |world: &World, id: &str| world.logs[id].answer_mode.mode.as_ref().unwrap().mode;
    world.handle(Command::NewConversation { project_id: None });
    let before = world.conversations[0].id.clone();
    world.handle(Command::Call(
        crate::service_api::settings_actions::save_prompt_answer_mode("interactive"),
    ));
    world.handle(Command::NewConversation { project_id: None });
    let after = world.conversations[0].id.clone();
    assert_eq!(mode_of(&world, &before), AnswerMode::Standard);
    assert_eq!(mode_of(&world, &after), AnswerMode::Interactive);
}
