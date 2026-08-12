//! Exercises the packaged terminal-host binary through its real framed stdio contract.
//! 通过真实分帧标准输入输出合同验证已构建的终端宿主二进制。

use std::io::{self, Read};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use vulcan_terminal_host_protocol::{
    TERMINAL_HOST_CONTRACT, TerminalHostErrorEvent, TerminalHostExitedEvent, TerminalHostFrame,
    TerminalHostFrameKind, TerminalHostResizeRequest, TerminalHostStartRequest,
    decode_terminal_host_json, encode_terminal_host_json, read_terminal_host_frame,
    validate_terminal_host_sequence, write_terminal_host_frame,
};

/// Verifies start, resize, exact input, output, operating-system exit, and frame ordering end to end.
/// 端到端验证启动、调整尺寸、精确输入、输出、操作系统退出及帧顺序。
#[test]
fn terminal_host_round_trips_real_shell_protocol() {
    let executable = env!("CARGO_BIN_EXE_vulcan-terminal-host");
    let mut child = Command::new(executable)
        .args(["--contract", TERMINAL_HOST_CONTRACT])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("terminal host binary must start");
    let mut stdin = child.stdin.take().expect("terminal host stdin");
    let mut stdout = child.stdout.take().expect("terminal host stdout");
    let (program, args, input) = platform_shell_fixture();
    let start = TerminalHostStartRequest {
        program,
        args,
        cwd: std::env::current_dir().expect("current test directory"),
        cols: 80,
        rows: 24,
    };
    write_terminal_host_frame(
        &mut stdin,
        &TerminalHostFrame {
            kind: TerminalHostFrameKind::Start,
            sequence: 1,
            payload: encode_terminal_host_json(&start).expect("encode start"),
        },
    )
    .expect("write start frame");
    let started = read_terminal_host_frame(&mut stdout)
        .expect("read started frame")
        .expect("started frame must exist");
    assert_eq!(started.kind, TerminalHostFrameKind::Started);
    assert_eq!(started.sequence, 1);

    write_terminal_host_frame(
        &mut stdin,
        &TerminalHostFrame {
            kind: TerminalHostFrameKind::Resize,
            sequence: 2,
            payload: encode_terminal_host_json(&TerminalHostResizeRequest {
                cols: 100,
                rows: 30,
            })
            .expect("encode resize"),
        },
    )
    .expect("write resize frame");
    write_terminal_host_frame(
        &mut stdin,
        &TerminalHostFrame {
            kind: TerminalHostFrameKind::InputBytes,
            sequence: 3,
            payload: input,
        },
    )
    .expect("write input frame");

    let (result_sender, result_receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = collect_terminal_events(&mut stdout);
        let _ = result_sender.send(result);
    });
    let collected = match result_receiver.recv_timeout(Duration::from_secs(15)) {
        Ok(result) => result.expect("terminal event stream must remain valid"),
        Err(error) => {
            let _ = child.kill();
            panic!("terminal host event stream timed out: {error}");
        }
    };
    drop(stdin);
    let status = child.wait().expect("wait for terminal host");
    assert!(status.success(), "terminal host stderr: {:?}", child.stderr);
    assert!(collected.output.contains("VULCAN_TERMINAL_HOST_E2E"));
    assert!(collected.saw_exit);
}

/// Verifies a missing terminal program returns a framed startup diagnostic instead of an ambiguous EOF.
/// 验证终端程序缺失时返回分帧启动诊断，而不是含糊的 EOF。
#[test]
fn terminal_host_reports_structured_child_spawn_failure() {
    let executable = env!("CARGO_BIN_EXE_vulcan-terminal-host");
    let mut child = Command::new(executable)
        .args(["--contract", TERMINAL_HOST_CONTRACT])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("terminal host binary must start");
    let mut stdin = child.stdin.take().expect("terminal host stdin");
    let mut stdout = child.stdout.take().expect("terminal host stdout");
    let start = TerminalHostStartRequest {
        program: String::from("vulcan-terminal-program-that-does-not-exist"),
        args: Vec::new(),
        cwd: std::env::current_dir().expect("current test directory"),
        cols: 80,
        rows: 24,
    };
    write_terminal_host_frame(
        &mut stdin,
        &TerminalHostFrame {
            kind: TerminalHostFrameKind::Start,
            sequence: 1,
            payload: encode_terminal_host_json(&start).expect("encode start"),
        },
    )
    .expect("write start frame");
    let failure = read_terminal_host_frame(&mut stdout)
        .expect("read startup failure")
        .expect("startup failure frame must exist");
    assert_eq!(failure.kind, TerminalHostFrameKind::Error);
    assert_eq!(failure.sequence, 1);
    let error: TerminalHostErrorEvent =
        decode_terminal_host_json(&failure.payload).expect("decode startup failure");
    assert_eq!(error.code, "terminal_child_spawn_failed");
    assert!(error.message.contains("failed to spawn terminal child"));
    drop(stdin);
    let status = child.wait().expect("wait for failed terminal host");
    assert!(!status.success());
}

/// Verifies parent control-pipe EOF stops the isolated terminal and lets the host exit cleanly.
/// 验证父控制管道 EOF 会停止隔离终端并使宿主正常退出。
#[test]
fn terminal_host_stops_terminal_after_parent_eof() {
    let executable = env!("CARGO_BIN_EXE_vulcan-terminal-host");
    let mut child = Command::new(executable)
        .args(["--contract", TERMINAL_HOST_CONTRACT])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("terminal host binary must start");
    let mut stdin = child.stdin.take().expect("terminal host stdin");
    let mut stdout = child.stdout.take().expect("terminal host stdout");
    let (program, args, _) = platform_shell_fixture();
    let start = TerminalHostStartRequest {
        program,
        args,
        cwd: std::env::current_dir().expect("current test directory"),
        cols: 80,
        rows: 24,
    };
    write_terminal_host_frame(
        &mut stdin,
        &TerminalHostFrame {
            kind: TerminalHostFrameKind::Start,
            sequence: 1,
            payload: encode_terminal_host_json(&start).expect("encode start"),
        },
    )
    .expect("write start frame");
    let started = read_terminal_host_frame(&mut stdout)
        .expect("read started frame")
        .expect("started frame must exist");
    assert_eq!(started.kind, TerminalHostFrameKind::Started);
    drop(stdin);

    let (result_sender, result_receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = collect_terminal_events(&mut stdout);
        let _ = result_sender.send(result);
    });
    let collected = match result_receiver.recv_timeout(Duration::from_secs(15)) {
        Ok(result) => result.expect("terminal event stream must remain valid"),
        Err(error) => {
            let _ = child.kill();
            panic!("terminal host did not stop after parent EOF: {error}");
        }
    };
    let status = child.wait().expect("wait for terminal host");
    let stderr = child
        .stderr
        .take()
        .map(|mut stderr| {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        })
        .unwrap_or_default();
    assert!(status.success(), "terminal host stderr: {stderr}");
    assert!(collected.saw_exit);
    assert!(collected.stopped_by_request);
}

/// Stores the observable end-to-end event results.
/// 保存可观察的端到端事件结果。
struct CollectedTerminalEvents {
    /// Decoded terminal output bytes.
    /// 已解码终端输出字节。
    output: String,
    /// Whether a valid terminal exit event was received.
    /// 是否收到有效终端退出事件。
    saw_exit: bool,
    /// Whether the terminal was stopped by parent lifecycle control.
    /// 终端是否由父生命周期控制停止。
    stopped_by_request: bool,
}

/// Reads every host event until the terminal exit frame and validates contiguous sequencing.
/// 读取直到终端退出帧的全部宿主事件，并验证连续序号。
fn collect_terminal_events(stdout: &mut impl io::Read) -> io::Result<CollectedTerminalEvents> {
    let mut last_sequence = 1_u64;
    let mut output = Vec::new();
    let mut saw_exit = false;
    let mut stopped_by_request = false;
    while let Some(frame) = read_terminal_host_frame(stdout)? {
        validate_terminal_host_sequence(&mut last_sequence, frame.sequence)?;
        match frame.kind {
            TerminalHostFrameKind::OutputBytes => output.extend_from_slice(&frame.payload),
            TerminalHostFrameKind::Exited => {
                let exited: TerminalHostExitedEvent = decode_terminal_host_json(&frame.payload)?;
                stopped_by_request = exited.stopped_by_request;
                saw_exit = true;
                break;
            }
            TerminalHostFrameKind::Error => {
                return Err(io::Error::other(format!(
                    "terminal host emitted an error event: {}",
                    String::from_utf8_lossy(&frame.payload)
                )));
            }
            _ => {}
        }
    }
    Ok(CollectedTerminalEvents {
        output: String::from_utf8_lossy(&output).into_owned(),
        saw_exit,
        stopped_by_request,
    })
}

/// Returns one deterministic interactive shell command for the current target platform.
/// 返回适用于当前目标平台的一条确定性交互 Shell 命令。
fn platform_shell_fixture() -> (String, Vec<String>, Vec<u8>) {
    #[cfg(windows)]
    {
        (
            String::from("cmd.exe"),
            vec![String::from("/Q")],
            b"echo VULCAN_TERMINAL_HOST_E2E\r\nexit\r\n".to_vec(),
        )
    }
    #[cfg(not(windows))]
    {
        (
            String::from("/bin/sh"),
            Vec::new(),
            b"printf 'VULCAN_TERMINAL_HOST_E2E\\n'\nexit\n".to_vec(),
        )
    }
}
