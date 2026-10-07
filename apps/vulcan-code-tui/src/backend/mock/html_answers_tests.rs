//! Synthetic HTML state and failure semantics, without opening a real service or browser.
//! 合成 HTML 状态与失败语义，不打开真实服务或浏览器。

use super::*;

/// The mock follows CAS and exact source ownership instead of acknowledging unknown writes.
/// 模拟后端遵循比较交换与精确源码归属，不确认未知写入。
#[test]
fn html_mock_preferences_and_links_preserve_source_and_failure_semantics() {
    let (out, events) = mpsc::channel();
    let mut world = World::new(out, Pacing::Immediate);
    let (id, event, projection) = crate::model::html_answers::fixture();
    let state = HtmlAnswerSession {
        generation: Some(1),
        mode: Some(HtmlAnswerMode::default()),
        projections: [(event.seq.value(), projection)].into(),
    };
    let source = state.source(&id, event.seq.value(), 0).unwrap();
    world.logs.insert(
        id.clone(),
        ConversationLog {
            html_answers: state,
            ..ConversationLog::default()
        },
    );
    for enabled in [true, false] {
        world.handle(Command::SetSessionHtmlAnswerMode(
            vulcan_service_api::SessionHtmlAnswerModeUpdateRequest {
                session_id: id.clone(),
                enabled,
                expected_revision: 0,
            },
        ));
    }
    let mode = world.logs[&id].html_answers.mode.as_ref().unwrap();
    assert!(mode.enabled);
    assert_eq!(mode.revision, 1);
    assert!(events.try_iter().any(|event| matches!(
        event,
        BackendEvent::Toast {
            severity: NoticeSeverity::Warning,
            ..
        }
    )));
    world.handle(Command::CreateHtmlAnswerViewLink {
        source: source.clone(),
        delivery: HtmlAnswerDelivery::Copy,
        locale: "en-US".into(),
    });
    assert!(events.try_iter().any(|event| matches!(event, BackendEvent::HtmlAnswerLink { source: actual, .. } if actual == source)));
    let mut stale = source;
    stale.window_generation += 1;
    world.handle(Command::CreateHtmlAnswerViewLink {
        source: stale,
        delivery: HtmlAnswerDelivery::Open,
        locale: "en-US".into(),
    });
    let failures: Vec<_> = events.try_iter().collect();
    assert!(failures.iter().any(|event| matches!(
        event,
        BackendEvent::Toast {
            severity: NoticeSeverity::Warning,
            ..
        }
    )));
    assert!(
        !failures
            .iter()
            .any(|event| matches!(event, BackendEvent::HtmlAnswerLink { .. }))
    );
}
