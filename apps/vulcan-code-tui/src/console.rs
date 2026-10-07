//! Terminal lifecycle bound to the console this client is attached to.
//! 绑定到本客户端所附着控制台的终端生命周期。
//!
//! A launcher that opens the client in a new console (the service tray) still hands over its own
//! standard handles, which may point at a pipe, a file or another console. Input and console modes
//! already use the attached console, so frames are written there too instead of to stdout.
//! 在新控制台中打开客户端的启动器（服务托盘）仍会交出自身的标准句柄，它们可能指向管道、文件或另一个
//! 控制台。输入与控制台模式本就使用所附着的控制台，因此画面也写入该控制台，而不是标准输出。

use std::io;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

/// Terminal that draws on the attached console.
/// 在所附着控制台上绘制的终端。
pub type ConsoleTerminal = Terminal<CrosstermBackend<ConsoleOutput>>;

/// Enters raw mode, the alternate screen, mouse capture and bracketed paste on the attached console.
/// 在所附着控制台上进入原始模式、备用屏幕、鼠标捕获与括号粘贴。
/// The terminal is restored by `restore` and by the panic hook installed here.
/// 终端由 `restore` 以及此处安装的 panic 钩子恢复。
pub fn init() -> io::Result<ConsoleTerminal> {
    set_panic_hook();
    enable_raw_mode()?;
    let mut output = ConsoleOutput::open()?;
    execute!(
        output,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    Terminal::new(CrosstermBackend::new(output))
}

/// Writes a control sequence the frame does not carry, such as an OSC 52 clipboard copy.
/// 写出画面之外的控制序列，例如 OSC 52 剪贴板复制。
pub fn write_sequence(terminal: &mut ConsoleTerminal, sequence: &str) -> io::Result<()> {
    let output = terminal.backend_mut();
    io::Write::write_all(output, sequence.as_bytes())?;
    io::Write::flush(output)
}

/// Leaves every mode `init` entered; failures are reported on stderr because nothing else can be done.
/// 退出 `init` 进入的全部模式；失败只能输出到 stderr。
pub fn restore() {
    if let Err(error) = try_restore() {
        eprintln!("failed to restore terminal: {error}");
    }
}

fn try_restore() -> io::Result<()> {
    // Raw mode goes first because it has more side effects than leaving the alternate screen.
    // 先退出原始模式，因为它比离开备用屏幕有更多副作用。
    disable_raw_mode()?;
    execute!(
        ConsoleOutput::open()?,
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen
    )
}

fn set_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        hook(info);
    }));
}

/// Writer for the active screen buffer of the attached console (`CONOUT$`).
/// 所附着控制台活动屏幕缓冲区（`CONOUT$`）的写入器。
/// Bytes are kept until `flush` and written as UTF-16, so the console code page never garbles text;
/// an incomplete UTF-8 sequence at the end waits for the next flush.
/// 字节保留到 `flush` 时以 UTF-16 写出，控制台代码页不会使文字乱码；末尾不完整的 UTF-8 序列等待下次刷新。
#[cfg(windows)]
pub struct ConsoleOutput {
    console: crossterm_winapi::Console,
    pending: Vec<u8>,
}

#[cfg(windows)]
impl ConsoleOutput {
    fn open() -> io::Result<Self> {
        Ok(Self {
            console: crossterm_winapi::Console::from(
                crossterm_winapi::Handle::current_out_handle()?
            ),
            pending: Vec::new(),
        })
    }
}

#[cfg(windows)]
impl io::Write for ConsoleOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let complete = complete_utf8_prefix(&self.pending)?;
        if complete > 0 {
            self.console.write_char_buffer(&self.pending[..complete])?;
            self.pending.drain(..complete);
        }
        Ok(())
    }
}

/// Length of the longest prefix of `bytes` that ends on a UTF-8 character boundary.
/// `bytes` 中以 UTF-8 字符边界结尾的最长前缀长度。
/// Fails only for bytes that can never become valid UTF-8.
/// 仅当字节永远无法成为有效 UTF-8 时失败。
#[cfg_attr(not(windows), allow(dead_code))]
fn complete_utf8_prefix(bytes: &[u8]) -> io::Result<usize> {
    match std::str::from_utf8(bytes) {
        Ok(_) => Ok(bytes.len()),
        Err(error) if error.error_len().is_none() => Ok(error.valid_up_to()),
        Err(error) => Err(io::Error::new(io::ErrorKind::InvalidData, error)),
    }
}

/// Outside Windows the controlling terminal is reached through stdout.
/// 在 Windows 以外通过标准输出访问控制终端。
#[cfg(not(windows))]
pub struct ConsoleOutput(io::Stdout);

#[cfg(not(windows))]
impl ConsoleOutput {
    fn open() -> io::Result<Self> {
        Ok(Self(io::stdout()))
    }
}

#[cfg(not(windows))]
impl io::Write for ConsoleOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Write::write(&mut self.0, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::complete_utf8_prefix;

    #[test]
    fn utf8_prefix_holds_back_only_an_unfinished_character() {
        let text = "a中".as_bytes();
        assert_eq!(complete_utf8_prefix(text).unwrap(), text.len());
        assert_eq!(complete_utf8_prefix(&text[..2]).unwrap(), 1);
        assert!(complete_utf8_prefix(&[b'a', 0xff]).is_err());
    }
}
