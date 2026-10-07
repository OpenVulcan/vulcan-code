//! Shared keyboard/mouse HTML actions; no source parsing or authority inference lives here.
//! 键盘与鼠标共享的 HTML 动作；此处不解析源码或推断权威。

use super::{
    App,
    action::UiAction,
    overlay::{DetailSheet, Overlay},
};
use crate::{
    backend::{Command, ConnectionState},
    model::*,
    tr,
};

/// Request the opposite preference using the last authoritative revision; never optimistically save.
/// 使用最后权威修订请求相反偏好；绝不乐观标记为已保存。
pub fn toggle(app: &mut App) {
    let Some(id) = app.selected.clone() else {
        return;
    };
    let Some(mode) = app
        .session()
        .and_then(|session| session.html_answers.mode.as_ref())
    else {
        app.toast(NoticeSeverity::Warning, tr!("htmlAnswers.saveFailed"));
        return;
    };
    if app.connection != ConnectionState::Connected {
        return;
    }
    app.backend.send(Command::SetSessionHtmlAnswerMode(
        vulcan_service_api::SessionHtmlAnswerModeUpdateRequest {
            session_id: id,
            enabled: !mode.enabled,
            expected_revision: mode.revision,
        },
    ));
}

/// Show Core-described results for `seq`; return false when normal inspection should handle it.
/// 显示 `seq` 的 Core 结果描述；应由常规检查处理时返回 false。
pub fn inspect(app: &mut App, seq: u64) -> bool {
    let Some(id) = app.selected.as_deref() else {
        return false;
    };
    let Some(session) = app.session() else {
        return false;
    };
    let Some(projection) = session
        .html_answers
        .projections
        .get(&seq)
        .filter(|projection| !projection.answers.is_empty())
    else {
        return false;
    };
    let mut sheet = DetailSheet::new(tr!("htmlAnswers.label"));
    for answer in &projection.answers {
        let title = answer
            .title
            .clone()
            .unwrap_or_else(|| tr!("htmlAnswers.label"));
        if let Some(source) = session.html_answers.source(id, seq, answer.ordinal) {
            for (label, delivery) in [
                (tr!("htmlAnswers.open"), HtmlAnswerDelivery::Open),
                (tr!("htmlAnswers.copyLink"), HtmlAnswerDelivery::Copy),
            ] {
                sheet = sheet.action(
                    format!("{title} · {label}"),
                    false,
                    match delivery {
                        HtmlAnswerDelivery::Open => UiAction::OpenHtmlAnswer(source.clone()),
                        HtmlAnswerDelivery::Copy => UiAction::RequestHtmlAnswer {
                            source: source.clone(),
                            delivery,
                        },
                    },
                );
            }
        } else {
            sheet = sheet.field(title.clone(), tr!("htmlAnswers.cannotOpen"));
        }
        for (label, copy) in [
            (tr!("htmlAnswers.source"), false),
            (tr!("htmlAnswers.copySource"), true),
        ] {
            sheet = sheet.action(
                format!("{title} · {label}"),
                false,
                UiAction::HtmlAnswerSource {
                    seq,
                    ordinal: answer.ordinal,
                    copy,
                },
            );
        }
    }
    sheet.focus = (!sheet.actions.is_empty()).then_some(0);
    app.overlay = Some(Overlay::Detail(Box::new(sheet)));
    true
}

/// Look up the exact selected immutable identity; returns its title only while still current.
/// 查找精确选中的不可变身份；仅在身份仍为当前值时返回标题。
fn current_title(
    app: &App,
    source: &vulcan_service_api::HtmlAnswerSourceIdentity,
) -> Option<String> {
    let id = source.session_id.to_string();
    if app.selected.as_deref() != Some(id.as_str()) {
        return None;
    }
    let session = app.session()?;
    let projection = session.html_answers.projections.get(&source.event_seq)?;
    projection
        .answers
        .iter()
        .find(|answer| {
            session
                .html_answers
                .source(&id, source.event_seq, answer.ordinal)
                .as_ref()
                == Some(source)
        })
        .and_then(|answer| answer.title.clone())
}

/// Ask before opening `source`; cancel creates no ticket and launches no browser.
/// 打开 `source` 前提问；取消不会生成票据或启动浏览器。
pub fn confirm_open(app: &mut App, source: vulcan_service_api::HtmlAnswerSourceIdentity) {
    let Some(title) = current_title(app, &source) else {
        app.toast(NoticeSeverity::Warning, tr!("htmlAnswers.stale"));
        return;
    };
    app.overlay = Some(Overlay::Confirm(Box::new(super::action::ConfirmDialog {
        title: tr!("browser.openInDefaultBrowser"),
        body: tr!("tui.htmlAnswers.confirmOpen", title = title),
        confirm_label: tr!("htmlAnswers.open"),
        danger: false,
        then: UiAction::RequestHtmlAnswer {
            source,
            delivery: HtmlAnswerDelivery::Open,
        },
        focus_confirm: false,
    })));
}

/// Revalidate a confirmed/copy `source` and submit the matching backend request without launching here.
/// 重新校验已确认或复制的 `source`，提交匹配的后端请求，不在此处启动浏览器。
pub fn request(
    app: &mut App,
    source: vulcan_service_api::HtmlAnswerSourceIdentity,
    delivery: HtmlAnswerDelivery,
) {
    if current_title(app, &source).is_none() {
        app.toast(NoticeSeverity::Warning, tr!("htmlAnswers.stale"));
        return;
    }
    app.backend.send(Command::CreateHtmlAnswerViewLink {
        source,
        delivery,
        locale: crate::i18n::active_locale().to_owned(),
    });
}

/// Activate the selected result from the keyboard; multiple results open the existing chooser.
/// 从键盘激活选中结果；多个结果打开既有选择面板。
pub fn activate_selected(app: &mut App) -> bool {
    let Some((seq, sources)) = app.session().and_then(|session| {
        let seq = session.selected?;
        let projection = session
            .html_answers
            .projections
            .get(&seq)
            .filter(|projection| !projection.answers.is_empty())?;
        let id = app.selected.as_deref()?;
        Some((
            seq,
            projection
                .answers
                .iter()
                .filter_map(|answer| session.html_answers.source(id, seq, answer.ordinal))
                .collect::<Vec<_>>(),
        ))
    }) else {
        return false;
    };
    if sources.len() == 1 {
        super::action::perform(
            app,
            UiAction::OpenHtmlAnswer(sources.into_iter().next().expect("one source")),
        );
        true
    } else {
        super::action::perform(app, UiAction::Inspect(seq));
        true
    }
}

/// Read the exact selected raw range; source UI uses text, never an HTML renderer.
/// 读取精确选中的原始区间；源码界面使用文本，绝不使用 HTML 渲染器。
pub fn source(app: &mut App, seq: u64, ordinal: usize, copy: bool) {
    let selected = app.session().and_then(|session| {
        let answer = session
            .html_answers
            .projections
            .get(&seq)?
            .answers
            .iter()
            .find(|answer| answer.ordinal == ordinal)?;
        let event = session.events.iter().find(|event| event.seq == seq)?;
        let EventBody::AssistantText { text, .. } = &event.body else {
            return None;
        };
        let range = match &answer.availability {
            HtmlAnswerAvailability::Ready { source, .. } => source,
            _ => &answer.fence,
        };
        text.get(range.byte_start..range.byte_end)
            .map(str::to_owned)
    });
    let Some(text) = selected else {
        app.toast(NoticeSeverity::Warning, tr!("htmlAnswers.stale"));
        return;
    };
    if copy {
        app.clipboard.copy(text);
        app.toast(NoticeSeverity::Info, tr!("htmlAnswers.copied"));
    } else {
        app.overlay = Some(Overlay::Detail(Box::new(
            DetailSheet::new(tr!("htmlAnswers.source")).section(String::new(), text),
        )));
    }
}

/// Check a Core-issued URL is an HTTP(S) viewer for this exact source, with one fragment ticket.
/// 检查 Core 签发地址是此精确源码的 HTTP(S) 查看器，且只有一个片段票据。
/// No local path, credentials or alternate scheme may reach the platform launcher.
/// 本地路径、凭据或其他协议不得到达平台启动器。
pub fn valid_viewer_url(
    source: &vulcan_service_api::HtmlAnswerSourceIdentity,
    value: &str,
) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.path() == format!("/html-answers/{}/{}", source.session_id, source.artifact_id)
        && url
            .fragment()
            .and_then(|fragment| fragment.strip_prefix("ticket="))
            .is_some_and(|ticket| {
                !ticket.is_empty()
                    && ticket
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            })
}

/// Deliver a matching response to the user's chosen action; stale or malformed receipts are revoked.
/// 将匹配回执交付给用户选定动作；过期或格式错误的回执被撤销。
pub fn deliver(
    app: &mut App,
    source: vulcan_service_api::HtmlAnswerSourceIdentity,
    link: HtmlAnswerLink,
    delivery: HtmlAnswerDelivery,
) {
    let current = current_title(app, &source).is_some();
    if !current || !valid_viewer_url(&source, &link.0.viewer_url) {
        app.backend
            .send(Command::RevokeHtmlAnswerViewLink(link.0.grant_id));
        return;
    }
    if matches!(delivery, HtmlAnswerDelivery::Open) {
        // The safe default handler accepts a URL value; the insecure cmd/start feature is never enabled.
        // 安全默认处理器接收地址值；绝不启用不安全的 cmd/start 特性。
        let opened = !app.backend.is_mock && open::that_detached(&link.0.viewer_url).is_ok();
        // A launcher receipt cannot prove the user can see a browser (SSH/WSL/headless terminals).
        // 启动器回执无法证明用户能看见浏览器（SSH、WSL 或无界面终端）。
        // Always retain the actual URL; fresh-copy obtains a new ticket if the first was consumed.
        // 始终保留实际地址；首次票据已消费时，重新复制会取得新票据。
        app.overlay = Some(Overlay::Detail(Box::new(
            DetailSheet::new(if opened {
                tr!("browser.openInDefaultBrowser")
            } else {
                tr!("htmlAnswers.cannotOpen")
            })
            .section(
                String::new(),
                tr!(
                    "tui.htmlAnswers.openFallback",
                    seconds = link.0.expires_in_seconds
                ),
            )
            .section(tr!("htmlAnswers.copyLink"), link.0.viewer_url.clone())
            .action(
                tr!("htmlAnswers.copyLink"),
                false,
                UiAction::RequestHtmlAnswer {
                    source,
                    delivery: HtmlAnswerDelivery::Copy,
                },
            ),
        )));
        return;
    } else {
        app.toast(
            NoticeSeverity::Info,
            tr!(
                "htmlAnswers.linkCopied",
                seconds = link.0.expires_in_seconds
            ),
        );
    }
    app.clipboard.copy(link.0.viewer_url);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{action::perform, commands::AppCommand, hit::Target, input};
    use crate::backend::{BackendEvent, BackendHandle};
    use ratatui::{
        Terminal,
        backend::TestBackend,
        crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers},
    };
    use std::sync::mpsc::{self, Receiver};

    /// Seed an inert app from the real Core parser, preserving original UTF-8 and CRLF coordinates.
    /// 用真实 Core 解析器初始化静默应用，保留原始 UTF-8 与 CRLF 坐标。
    fn app() -> (
        App,
        Receiver<Command>,
        vulcan_service_api::HtmlAnswerSourceIdentity,
    ) {
        let (commands, receiver) = mpsc::channel();
        let (_, events) = mpsc::channel();
        let mut app = App::new(
            BackendHandle {
                commands,
                events,
                label: "test".into(),
                is_mock: true,
            },
            crate::ui::theme::Theme::dark(),
            true,
        );
        app.connection = ConnectionState::Connected;
        let (id, event, projection) = crate::model::html_answers::fixture();
        let vulcan_session_core::v1::EventBody::AssistantText(body) = event.event else {
            unreachable!()
        };
        app.selected = Some(id.clone());
        app.apply(BackendEvent::ConversationHydrated {
            conversation_id: id.clone(),
            events: vec![ConversationEvent {
                seq: event.seq.value(),
                created_at_ms: 1,
                updated_at_ms: 2,
                body: EventBody::AssistantText {
                    html_answers_allowed: true,
                    text: body.text,
                    status: body.status,
                },
            }],
        });
        app.apply(BackendEvent::HtmlAnswers {
            conversation_id: id.clone(),
            state: HtmlAnswerSession {
                generation: Some(8),
                mode: Some(HtmlAnswerMode::default()),
                projections: [(projection.seq.value(), projection)].into(),
            },
        });
        let source = app
            .session()
            .unwrap()
            .html_answers
            .source(&id, event.seq.value(), 0)
            .unwrap();
        (app, receiver, source)
    }

    /// Open and copy use exact Core identity; source inspection retains the canonical original bytes.
    /// 打开和复制使用精确 Core 身份；源码检查保留权威原始字节。
    #[test]
    fn html_result_actions_share_authority_and_preserve_source() {
        let (mut app, commands, identity) = app();
        perform(&mut app, UiAction::Inspect(identity.event_seq));
        let Some(Overlay::Detail(sheet)) = &app.overlay else {
            panic!("HTML result actions expected");
        };
        let commands_to_run: Vec<_> = sheet
            .actions
            .iter()
            .filter_map(|action| match &action.action {
                UiAction::OpenHtmlAnswer(source) | UiAction::RequestHtmlAnswer { source, .. } => {
                    assert_eq!(source, &identity);
                    Some(action.action.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(commands_to_run.len(), 2);
        for action in commands_to_run {
            let opening = matches!(action, UiAction::OpenHtmlAnswer(_));
            perform(&mut app, action);
            if opening {
                assert!(
                    commands.try_recv().is_err(),
                    "Confirmation must precede ticket issuance"
                );
                assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
                perform(&mut app, UiAction::ConfirmAccept);
            }
        }
        assert_eq!(commands.try_iter().count(), 2);
        source(&mut app, identity.event_seq, 0, false);
        let Some(Overlay::Detail(sheet)) = &app.overlay else {
            panic!("Source sheet expected");
        };
        assert!(
            sheet.sections.iter().any(
                |(_, body)| body == "<!doctype html><button>synthetic-code-marker</button>\r\n"
            )
        );
    }

    /// Late links cannot launch another selected owner; valid copy uses only the terminal clipboard.
    /// 迟到链接不能打开其他当前归属；有效复制仅使用终端剪贴板。
    #[test]
    fn html_link_delivery_rejects_stale_owner_and_redacts_all_diagnostics() {
        let (mut app, commands, identity) = app();
        let link = HtmlAnswerLink(vulcan_service_api::HtmlAnswerViewLinkResponse {
            grant_id: uuid::Uuid::new_v4(),
            viewer_url: format!(
                "http://127.0.0.1:7331/html-answers/{}/{}#ticket=synthetic-secret",
                identity.session_id, identity.artifact_id
            ),
            expires_in_seconds: 300,
        });
        assert!(!format!("{link:?}").contains("synthetic-secret"));
        deliver(
            &mut app,
            identity.clone(),
            link.clone(),
            HtmlAnswerDelivery::Copy,
        );
        assert!(!format!("{:?}", app.clipboard).contains("synthetic-secret"));
        assert!(app.clipboard.take_sequence().is_some());
        app.selected = Some(uuid::Uuid::new_v4().to_string());
        deliver(&mut app, identity, link.clone(), HtmlAnswerDelivery::Open);
        assert!(
            matches!(commands.try_recv(), Ok(Command::RevokeHtmlAnswerViewLink(id)) if id == link.0.grant_id)
        );
        assert!(app.clipboard.take_sequence().is_none());
        let sheet = DetailSheet::new("view".into()).section("link".into(), link.0.viewer_url);
        assert!(!format!("{sheet:?}").contains("synthetic-secret"));
    }

    /// No unsafe scheme, credentials, altered identity or duplicate fragment reaches the launcher.
    /// 不安全协议、凭据、被修改身份或重复片段均不能到达启动器。
    #[test]
    fn html_viewer_url_validation_is_exact() {
        let (_, _, identity) = app();
        let valid = format!(
            "http://127.0.0.1:7331/html-answers/{}/{}#ticket=synthetic",
            identity.session_id, identity.artifact_id
        );
        assert!(valid_viewer_url(&identity, &valid));
        for invalid in [
            valid.replace("http:", "file:"),
            valid.replace("http://", "http://user:secret@"),
            valid.replace(&identity.artifact_id, "other"),
            format!("{valid}&ticket=other"),
            valid.replace("#ticket=", "?token=core#ticket="),
            "javascript:alert(1)".into(),
        ] {
            assert!(!valid_viewer_url(&identity, &invalid));
        }
    }

    /// The same persisted toggle is reachable with keyboard and real hit targets at both terminal sizes.
    /// 两种终端尺寸下，键盘与真实命中目标均能到达同一持久开关。
    #[test]
    fn html_controls_render_at_wide_and_narrow_sizes_without_source_flooding() {
        let _locale = crate::i18n::lock_locale("en-US");
        for (width, height) in [(160, 45), (64, 20)] {
            let (mut app, commands, _) = app();
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(!text.contains("synthetic-code-marker"));
            assert!(
                app.hits
                    .find(|target| matches!(
                        target,
                        Target::Do(UiAction::App(AppCommand::ToggleHtmlAnswers))
                    ))
                    .is_some(),
                "HTML toggle must fit {width} columns"
            );
            input::handle_event(
                &mut app,
                Event::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::ALT)),
            );
            assert!(
                matches!(commands.try_recv(), Ok(Command::SetSessionHtmlAnswerMode(request)) if request.enabled && request.expected_revision == 0)
            );
            assert!(
                !app.session()
                    .unwrap()
                    .html_answers
                    .mode
                    .as_ref()
                    .unwrap()
                    .enabled,
                "must wait for durable acknowledgement"
            );
            perform(&mut app, UiAction::App(AppCommand::ToggleHtmlAnswers));
            assert!(
                matches!(commands.try_recv(), Ok(Command::SetSessionHtmlAnswerMode(request)) if request.enabled)
            );
        }
    }

    /// Real terminal hits and Enter reach confirmation; cancellation and changed owners cannot issue links.
    /// 真实终端命中和回车均到达确认；取消及归属改变后不能签发链接。
    #[test]
    fn html_result_inline_button_requires_confirmation_at_each_width() {
        let _locale = crate::i18n::lock_locale("en-US");
        for (width, height) in [(160, 45), (64, 20)] {
            let (mut app, commands, identity) = app();
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            let at = app.hits.find(|target| matches!(target, Target::Do(UiAction::OpenHtmlAnswer(source)) if source == &identity)).expect("Result button must remain visible at the current terminal width");
            input::handle_event(
                &mut app,
                Event::Mouse(ratatui::crossterm::event::MouseEvent {
                    kind: ratatui::crossterm::event::MouseEventKind::Down(
                        ratatui::crossterm::event::MouseButton::Left,
                    ),
                    column: at.x,
                    row: at.y,
                    modifiers: KeyModifiers::NONE,
                }),
            );
            assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
            assert!(commands.try_recv().is_err());
            input::handle_event(
                &mut app,
                Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            );
            assert!(app.overlay.is_none());
            assert!(commands.try_recv().is_err());
            app.focus = crate::app::Focus::Transcript;
            app.session_mut().unwrap().selected = Some(identity.event_seq);
            input::handle_event(
                &mut app,
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            );
            assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
            assert!(commands.try_recv().is_err());
            perform(&mut app, UiAction::ConfirmAccept);
            assert!(
                matches!(commands.try_recv(), Ok(Command::CreateHtmlAnswerViewLink { source, delivery: HtmlAnswerDelivery::Open, .. }) if source == identity)
            );
            confirm_open(&mut app, identity.clone());
            app.selected = Some(uuid::Uuid::new_v4().to_string());
            perform(&mut app, UiAction::ConfirmAccept);
            assert!(commands.try_recv().is_err());
        }
    }

    /// Unsupported browser environments retain the actual one-use URL in redacted, copyable UI state.
    /// 不支持浏览器的环境在已遮蔽诊断的可复制界面状态中保留实际单次地址。
    #[test]
    fn html_result_browser_fallback_shows_the_actual_url() {
        let (mut app, _, identity) = app();
        let url = format!(
            "http://127.0.0.1:7331/html-answers/{}/{}#ticket=synthetic-fallback",
            identity.session_id, identity.artifact_id
        );
        deliver(
            &mut app,
            identity,
            HtmlAnswerLink(vulcan_service_api::HtmlAnswerViewLinkResponse {
                grant_id: uuid::Uuid::new_v4(),
                viewer_url: url.clone(),
                expires_in_seconds: 45,
            }),
            HtmlAnswerDelivery::Open,
        );
        let Some(Overlay::Detail(sheet)) = &app.overlay else {
            panic!("Fallback URL must be visible");
        };
        assert!(sheet.sections.iter().any(|(_, body)| body == &url));
        assert!(sheet.actions.iter().any(|action| matches!(
            action.action,
            UiAction::RequestHtmlAnswer {
                delivery: HtmlAnswerDelivery::Copy,
                ..
            }
        )));
        assert!(!format!("{sheet:?}").contains("synthetic-fallback"));
    }
}
