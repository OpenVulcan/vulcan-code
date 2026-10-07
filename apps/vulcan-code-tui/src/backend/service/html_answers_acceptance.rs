//! Opt-in production transport and browser acceptance using only an owned synthetic fixture.
//! 仅使用自有合成夹具的显式正式传输与浏览器验收。

use super::*;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command as ProcessCommand, Stdio};

/// Own a temporary process; drop always terminates and reaps it, including failed assertions.
/// 持有临时进程；析构始终终止并回收，包含断言失败路径。
struct OwnedChild(Child);
impl Drop for OwnedChild {
    /// Reap the exact child, without touching any user-owned service.
    /// 回收精确子进程，不影响用户服务。
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Build a hidden process from an explicit executable; returns an unstarted command.
/// 根据显式程序路径构建隐藏进程；返回尚未启动的命令。
fn hidden(program: impl AsRef<std::ffi::OsStr>) -> ProcessCommand {
    let mut command = ProcessCommand::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

/// Hydrate through real Core HTTP, invoke the TUI action, and consume its exact link in Chromium.
/// 经真实 Core HTTP 加载，调用 TUI 动作，再在 Chromium 消费其精确链接。
/// Requires explicit fixture, built bundle and browser paths; never reads live service discovery.
/// 要求显式夹具、已构建包与浏览器路径；绝不读取实时服务发现。
#[test]
#[ignore = "Run explicitly with VULCAN_HTML_FIXTURE, VULCAN_HTML_BUNDLE and VULCAN_HTML_CHROMIUM"]
fn html_tui_production_viewer_acceptance() {
    let executable = std::env::var_os("VULCAN_HTML_FIXTURE").expect("explicit fixture required");
    let bundle = std::env::var_os("VULCAN_HTML_BUNDLE").expect("explicit bundle required");
    let browser = std::env::var_os("VULCAN_HTML_CHROMIUM").expect("explicit browser required");
    let mut fixture = OwnedChild(
        hidden(executable)
            .arg(bundle)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let output = fixture.0.stdout.take().unwrap();
    let (ready, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let line = BufReader::new(output)
            .lines()
            .next()
            .expect("fixture readiness")
            .unwrap();
        let _ = ready.send(serde_json::from_str::<Value>(&line).unwrap());
    });
    let public = receive
        .recv_timeout(Duration::from_secs(20))
        .expect("fixture readiness timeout");
    let origin = public["viewer"].as_str().unwrap();
    let address = origin.strip_prefix("http://").unwrap().parse().unwrap();
    let client = RpcClient::new(discovery::Endpoint::viewer_fixture(address));
    let id = public["session"].as_str().unwrap().to_owned();
    let (out, events) = std::sync::mpsc::channel();
    let (inbox, _) = std::sync::mpsc::channel();
    let mut worker = Worker::new(out, inbox, None);
    worker.client = Some(client);
    worker.hydrate(&id);

    let (commands, requests) = std::sync::mpsc::channel();
    let (_, empty_events) = std::sync::mpsc::channel();
    let mut app = crate::app::App::new(
        crate::backend::BackendHandle {
            commands,
            events: empty_events,
            label: "synthetic".into(),
            is_mock: true,
        },
        crate::ui::theme::Theme::dark(),
        true,
    );
    app.selected = Some(id.clone());
    // This fixture installs an authenticated client directly instead of running the discovery loop.
    // 此夹具直接安装已认证客户端，不执行服务发现循环。
    app.connection = crate::backend::ConnectionState::Connected;
    for event in events.try_iter() {
        app.apply(event);
    }
    let seq = *worker.windows[&id]
        .html_answers
        .projections
        .keys()
        .next()
        .expect("Core annotation");
    // Drive the rendered result button so transport acceptance covers the user-visible opening path.
    // 驱动实际渲染的结果按钮，使传输验收覆盖用户可见的打开路径。
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 42)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut app))
        .unwrap();
    let at = app.hits.find(|target| matches!(target, crate::app::hit::Target::Do(crate::app::action::UiAction::OpenHtmlAnswer(source)) if source.event_seq == seq)).expect("TUI inline result button");
    crate::app::input::handle_event(
        &mut app,
        ratatui::crossterm::event::Event::Mouse(ratatui::crossterm::event::MouseEvent {
            kind: ratatui::crossterm::event::MouseEventKind::Down(
                ratatui::crossterm::event::MouseButton::Left,
            ),
            column: at.x,
            row: at.y,
            modifiers: ratatui::crossterm::event::KeyModifiers::NONE,
        }),
    );
    assert!(matches!(
        app.overlay,
        Some(crate::app::overlay::Overlay::Confirm(_))
    ));
    assert!(
        requests.try_recv().is_err(),
        "No ticket before confirmation"
    );
    crate::app::action::perform(&mut app, crate::app::action::UiAction::ConfirmAccept);
    let Command::CreateHtmlAnswerViewLink {
        source,
        delivery,
        locale,
    } = requests.recv_timeout(Duration::from_secs(1)).unwrap()
    else {
        panic!("typed TUI request")
    };
    worker.create_html_answer_link(source, delivery, locale);
    let BackendEvent::HtmlAnswerLink {
        source,
        link,
        delivery,
    } = events
        .try_iter()
        .find(|event| matches!(event, BackendEvent::HtmlAnswerLink { .. }))
        .expect("real signed link")
    else {
        unreachable!()
    };
    let url = link.0.viewer_url.clone();
    app.apply(BackendEvent::HtmlAnswerLink {
        source,
        link,
        delivery,
    });
    // The inert terminal deliberately cannot launch an OS browser; the actual signed URL remains visible.
    // 静默终端刻意不能启动系统浏览器；实际签发的地址仍保持可见。
    let Some(crate::app::overlay::Overlay::Detail(sheet)) = &app.overlay else {
        panic!("TUI browser fallback sheet")
    };
    assert!(
        sheet.sections.iter().any(|(_, body)| body == &url),
        "Actual fallback URL must remain accessible"
    );

    let helper = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../vulcan-code-desktop/tools/html-answers-tui-browser.mjs");
    let mut browser_process = OwnedChild(
        hidden("node")
            .arg(helper)
            .arg(browser)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    // Capability travels only through stdin, never argv, logs, disk or assertion diagnostics.
    // 能力仅经过标准输入，绝不进入参数、日志、磁盘或断言诊断。
    browser_process
        .0
        .stdin
        .take()
        .unwrap()
        .write_all(url.as_bytes())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if let Some(status) = browser_process.0.try_wait().unwrap() {
            assert!(status.success(), "synthetic browser acceptance failed");
            break;
        }
        assert!(Instant::now() < deadline, "synthetic browser timeout");
        std::thread::sleep(Duration::from_millis(25));
    }
}
