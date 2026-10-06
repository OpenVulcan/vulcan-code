// `vulcan-tui [--backend service|mock] [--service-root <dir>] [--locale <tag>] [--theme dark|light]`
//
// Runs the TUI against the running Vulcan Code service (default) or the scripted mock backend.
// The terminal is restored on exit and on panic (`ratatui::init` installs the panic hook).
// 连接运行中的 Vulcan Code 服务（默认）或脚本化模拟后端运行 TUI。退出与 panic 时都会恢复终端
// （`ratatui::init` 会安装 panic 钩子）。

use std::process::ExitCode;
use std::time::{Duration, Instant};

use crate::app::{App, composer, input};
use crate::backend::{BackendHandle, mock, service};
use crate::i18n;
use crate::ui::{self, theme::Theme};
use anyhow::{Result, bail};
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use ratatui::crossterm::execute;

/// Frame budget while something animates; idle frames wait for input or backend events.
/// 有动画时的帧间隔；空闲时等待输入或后端事件。
const FRAME: Duration = Duration::from_millis(50);
/// Poll interval while nothing animates.
/// 无动画时的轮询间隔。
const IDLE_POLL: Duration = Duration::from_millis(250);
/// How long to wait for the rest of a paste that arrives as separate key presses (terminals
/// without bracketed paste).
/// 等待以单独按键到达的粘贴剩余部分的时长（不支持括号粘贴的终端）。
const BURST_WAIT: Duration = Duration::from_millis(3);

/// Command line synopsis printed by `--help`.
/// `--help` 打印的命令行概要。
const USAGE: &str = "usage: vulcan-tui [--backend service|mock] [--service-root <dir>] [--locale <tag>] [--theme dark|light]";

/// Parsed command line options.
/// 已解析的命令行选项。
struct Args {
    locale: Option<String>,
    theme: Theme,
    mock: bool,
    service: service::Options,
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args().skip(1);
    let mut parsed = Args {
        locale: None,
        theme: Theme::dark(),
        mock: false,
        service: service::Options::default(),
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--locale" => match args.next() {
                Some(tag) => parsed.locale = Some(tag),
                None => bail!("--locale needs a language tag, e.g. zh-TW"),
            },
            "--theme" => match args.next().as_deref() {
                Some("dark") => parsed.theme = Theme::dark(),
                Some("light") => parsed.theme = Theme::light(),
                _ => bail!("--theme expects `dark` or `light`"),
            },
            "--backend" => match args.next().as_deref() {
                Some("service") => parsed.mock = false,
                Some("mock") => parsed.mock = true,
                _ => bail!("--backend expects `service` or `mock`"),
            },
            "--service-root" => match args.next() {
                Some(dir) => parsed.service.root = Some(dir.into()),
                None => bail!(
                    "--service-root needs the service user root that holds code/webui-service.json"
                ),
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("unknown argument `{other}`"),
        }
    }
    Ok(parsed)
}

/// Runs the command line client and maps its outcome to the process exit code.
/// 运行命令行客户端，并将结果映射为进程退出码。
pub fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args = parse_args()?;
    i18n::set_active_locale(i18n::resolve_locale(args.locale.as_deref(), None));
    let backend: BackendHandle = if args.mock {
        mock::spawn()
    } else {
        service::spawn(args.service)
    };
    let mut app = App::new(backend, args.theme, args.locale.is_some());

    let mut terminal = ratatui::init();
    execute!(std::io::stdout(), EnableMouseCapture, EnableBracketedPaste)?;
    let result = event_loop(&mut terminal, &mut app);
    let _ = execute!(
        std::io::stdout(),
        DisableMouseCapture,
        DisableBracketedPaste
    );
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    let mut dirty = true;
    loop {
        while let Ok(event) = app.backend.events.try_recv() {
            app.apply(event);
            dirty = true;
        }
        app.tick(Instant::now());
        if dirty || app.animating() {
            terminal.draw(|frame| ui::draw(frame, app))?;
            dirty = false;
        }
        if app.should_quit {
            return Ok(());
        }
        let wait = if app.animating() { FRAME } else { IDLE_POLL };
        if event::poll(wait)? {
            // Drain everything already queued so fast typing costs one frame. A paste typed out as
            // key presses is waited for and turned back into one paste.
            // 一次处理完已排队的输入，使快速输入只消耗一帧。以按键形式逐个到达的粘贴会被等齐，
            // 并还原为一次粘贴。
            let mut batch = vec![event::read()?];
            loop {
                let wait = if composer::burst_pending(&batch) {
                    BURST_WAIT
                } else {
                    Duration::ZERO
                };
                if !event::poll(wait)? {
                    break;
                }
                batch.push(event::read()?);
            }
            for event in composer::coalesce(batch) {
                input::handle_event(app, event);
            }
            dirty = true;
        }
    }
}
