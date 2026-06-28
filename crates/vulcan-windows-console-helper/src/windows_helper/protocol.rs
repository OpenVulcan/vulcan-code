//! JSONL protocol models exchanged with the host runtime.
//! 与宿主运行时交换的 JSONL 协议模型。

use serde::{Deserialize, Serialize};
use std::io::{self, Write};

/// One structured terminal input event accepted by the helper.
/// helper 接受的一条结构化终端输入事件。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum HelperInputEvent {
    /// Raw text typed into the terminal.
    /// 输入到终端的原始文本。
    Text {
        /// Text payload to type.
        /// 需要输入的文本载荷。
        text: String,
    },
    /// Text pasted into the terminal.
    /// 粘贴到终端的文本。
    Paste {
        /// Text payload to paste.
        /// 需要粘贴的文本载荷。
        text: String,
    },
    /// One standardized terminal key.
    /// 一个标准化终端按键。
    Key {
        /// Stable key name.
        /// 稳定按键名称。
        key: String,
    },
    /// One standardized key chord.
    /// 一个标准化组合键。
    Chord {
        /// Base key for the chord.
        /// 组合键基础按键。
        key: String,
        /// Whether Ctrl is held.
        /// 是否按住 Ctrl。
        ctrl: bool,
        /// Whether Alt is held.
        /// 是否按住 Alt。
        alt: bool,
        /// Whether Shift is held.
        /// 是否按住 Shift。
        shift: bool,
        /// Whether Meta is held.
        /// 是否按住 Meta。
        meta: bool,
    },
}

/// One request sent by the host runtime.
/// 宿主运行时发送的一条请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum HelperRequest {
    /// Starts the target process.
    /// 启动目标进程。
    Start {
        /// Program executable path or name.
        /// 程序可执行路径或名称。
        program: String,
        /// Program arguments.
        /// 程序参数。
        args: Vec<String>,
        /// Working directory.
        /// 工作目录。
        cwd: String,
        /// Initial row count.
        /// 初始行数。
        rows: u16,
        /// Initial column count.
        /// 初始列数。
        cols: u16,
        /// Optional initial stdin text.
        /// 可选初始标准输入文本。
        initial_stdin: Option<String>,
        /// Whether to append the interactive newline.
        /// 是否追加交互换行。
        initial_stdin_append_newline: bool,
        /// Interactive newline string.
        /// 交互换行字符串。
        interactive_newline: String,
    },
    /// Sends ordered input events.
    /// 发送保序输入事件。
    Input {
        /// Ordered input events.
        /// 保序输入事件。
        events: Vec<HelperInputEvent>,
    },
    /// Sends raw text input.
    /// 发送原始文本输入。
    Text {
        /// Text payload.
        /// 文本载荷。
        text: String,
    },
    /// Resizes the console.
    /// 调整控制台大小。
    Resize {
        /// Desired row count.
        /// 期望行数。
        rows: u16,
        /// Desired column count.
        /// 期望列数。
        cols: u16,
    },
    /// Stops the target process.
    /// 停止目标进程。
    Stop,
}

/// One event emitted back to the host runtime.
/// 回传给宿主运行时的一条事件。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum HelperEvent {
    /// Target process started successfully.
    /// 目标进程已成功启动。
    Started {
        /// Target process identifier.
        /// 目标进程标识。
        pid: u32,
    },
    /// Current terminal screen frame.
    /// 当前终端屏幕帧。
    Frame {
        /// Stable frame sequence.
        /// 稳定帧序号。
        seq: u64,
        /// Frame row count.
        /// 帧行数。
        rows: u16,
        /// Frame column count.
        /// 帧列数。
        cols: u16,
        /// Renderable screen text with optional ANSI SGR styling.
        /// 可渲染屏幕文本，可包含 ANSI SGR 样式。
        text: String,
    },
    /// Target process exited.
    /// 目标进程已退出。
    Exited {
        /// Optional target exit code.
        /// 可选目标退出码。
        exit_code: Option<i32>,
        /// Whether the target was stopped by request.
        /// 目标是否按请求停止。
        stopped: bool,
    },
    /// Helper error event.
    /// helper 错误事件。
    Error {
        /// Error message.
        /// 错误消息。
        message: String,
    },
}
/// Emits one JSONL event to stdout.
/// 向 stdout 发出一条 JSONL 事件。
pub(super) fn emit_event(event: &HelperEvent) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, event)
        .map_err(|error| format!("Failed to serialize helper event: {error}"))?;
    stdout
        .write_all(b"\n")
        .map_err(|error| format!("Failed to write helper event newline: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("Failed to flush helper event: {error}"))
}
