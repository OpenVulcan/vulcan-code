//! Windows implementation that owns one real console session.
//! 持有一个真实控制台会话的 Windows 实现。

mod input;
mod process;
mod protocol;
mod screen;
mod util;

use std::io::{self, BufRead};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};

use input::{write_input_events, write_text_input};
use process::{ConsoleSession, start_console_session, terminate_target_process_tree};
use protocol::{HelperEvent, HelperRequest, emit_event};
use screen::{read_screen_text, resize_console};

/// Runs the Windows helper entrypoint.
/// 运行 Windows helper 入口。
pub fn run() -> i32 {
    match run_inner() {
        Ok(()) => 0,
        Err(error) => {
            let _ = emit_event(&HelperEvent::Error { message: error });
            1
        }
    }
}

/// Runs the main helper control loop.
/// 运行 helper 主控制循环。
fn run_inner() -> Result<(), String> {
    let first_line = {
        let stdin = io::stdin();
        let mut lines = stdin.lock().lines();
        lines
            .next()
            .ok_or_else(|| String::from("Missing start request."))?
            .map_err(|error| format!("Failed to read start request: {error}"))?
    };
    let start_request = serde_json::from_str::<HelperRequest>(&first_line)
        .map_err(|error| format!("Failed to parse start request: {error}"))?;
    let HelperRequest::Start {
        program,
        args,
        cwd,
        rows,
        cols,
        initial_stdin,
        initial_stdin_append_newline,
        interactive_newline,
    } = start_request
    else {
        return Err(String::from("First helper request must be start."));
    };
    let mut session = start_console_session(&program, &args, &cwd, rows, cols)?;
    emit_event(&HelperEvent::Started { pid: session.pid })?;
    if let Some(mut text) = initial_stdin {
        if initial_stdin_append_newline {
            text.push_str(&interactive_newline);
        }
        write_text_input(session.input, &text)?;
    }
    let (request_sender, request_receiver) = mpsc::channel();
    thread::spawn(move || {
        let stdin = io::stdin();
        let lines = stdin.lock().lines();
        for line in lines.map_while(Result::ok) {
            if let Ok(request) = serde_json::from_str::<HelperRequest>(&line) {
                let _ = request_sender.send(request);
            }
        }
    });
    control_loop(&mut session, request_receiver)
}

/// Runs the target-process polling and host-request loop.
/// 运行目标进程轮询与宿主请求循环。
fn control_loop(
    session: &mut ConsoleSession,
    request_receiver: Receiver<HelperRequest>,
) -> Result<(), String> {
    let mut last_frame = String::new();
    let mut frame_seq = 0u64;
    let mut next_frame_at = Instant::now();
    loop {
        while let Ok(request) = request_receiver.try_recv() {
            match request {
                HelperRequest::Input { events } => write_input_events(session.input, &events)?,
                HelperRequest::Text { text } => write_text_input(session.input, &text)?,
                HelperRequest::Resize { rows, cols } => {
                    resize_console(session.output, rows, cols)?;
                }
                HelperRequest::Stop => {
                    session.stopped = true;
                    if let Err(error) =
                        terminate_target_process_tree(session.pid, session.process, 1)
                    {
                        emit_event(&HelperEvent::Error { message: error })?;
                    }
                }
                HelperRequest::Start { .. } => {
                    emit_event(&HelperEvent::Error {
                        message: String::from("Nested start request is not supported."),
                    })?;
                }
            }
        }
        if Instant::now() >= next_frame_at {
            if let Ok((rows, cols, frame)) = read_screen_text(session.output)
                && frame.as_str() != last_frame.as_str()
            {
                frame_seq = frame_seq.saturating_add(1);
                last_frame = frame.clone();
                emit_event(&HelperEvent::Frame {
                    seq: frame_seq,
                    rows,
                    cols,
                    text: frame,
                })?;
            }
            next_frame_at = Instant::now() + Duration::from_millis(100);
        }
        let wait_result = unsafe { WaitForSingleObject(session.process, 20) };
        if wait_result == WAIT_OBJECT_0 {
            emit_current_frame(session.output, &mut last_frame, &mut frame_seq)?;
            let mut exit_code = 0u32;
            let ok = unsafe { GetExitCodeProcess(session.process, &mut exit_code) } != 0;
            emit_event(&HelperEvent::Exited {
                exit_code: ok.then_some(exit_code as i32),
                stopped: session.stopped,
            })?;
            return Ok(());
        }
        if wait_result != WAIT_TIMEOUT {
            return Err(format!("Unexpected process wait result: {wait_result}"));
        }
    }
}

/// Emits the current console frame when it changed.
/// 当当前控制台帧发生变化时发出该帧。
fn emit_current_frame(
    output: HANDLE,
    last_frame: &mut String,
    frame_seq: &mut u64,
) -> Result<(), String> {
    if let Ok((rows, cols, frame)) = read_screen_text(output)
        && frame.as_str() != last_frame.as_str()
    {
        *frame_seq = frame_seq.saturating_add(1);
        *last_frame = frame.clone();
        emit_event(&HelperEvent::Frame {
            seq: *frame_seq,
            rows,
            cols,
            text: frame,
        })?;
    }
    Ok(())
}
