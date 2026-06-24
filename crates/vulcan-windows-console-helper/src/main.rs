//! Windows native console helper for Vulcan terminal sessions.
//! Vulcan 终端会话使用的 Windows 原生控制台 helper。

#[cfg(windows)]
mod windows_helper {
    //! Windows implementation that owns one real console session.
    //! 持有一个真实控制台会话的 Windows 实现。

    use serde::{Deserialize, Serialize};
    use std::ffi::OsStr;
    use std::io::{self, BufRead, Write};
    use std::mem::zeroed;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};
    use std::sync::mpsc::{self, Receiver};
    use std::thread;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{
        CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        AttachConsole, BACKGROUND_BLUE, BACKGROUND_GREEN, BACKGROUND_INTENSITY, BACKGROUND_RED,
        CHAR_INFO, CONSOLE_SCREEN_BUFFER_INFO, COORD, ENHANCED_KEY, FOREGROUND_BLUE,
        FOREGROUND_GREEN, FOREGROUND_INTENSITY, FOREGROUND_RED, FreeConsole,
        GetConsoleScreenBufferInfo, INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT, KEY_EVENT_RECORD,
        KEY_EVENT_RECORD_0, LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED, RIGHT_ALT_PRESSED,
        RIGHT_CTRL_PRESSED, ReadConsoleOutputW, SHIFT_PRESSED, SMALL_RECT,
        SetConsoleScreenBufferSize, SetConsoleWindowInfo, WriteConsoleInputW,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        CREATE_NEW_CONSOLE, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetExitCodeProcess,
        OpenProcess, PROCESS_INFORMATION, PROCESS_TERMINATE, STARTF_USESHOWWINDOW, STARTUPINFOW,
        TerminateProcess, WaitForSingleObject,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MAPVK_VK_TO_VSC, MapVirtualKeyW, VK_BACK, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1,
        VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_TAB, VK_UP,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    /// One structured terminal input event accepted by the helper.
    /// helper 接受的一条结构化终端输入事件。
    #[derive(Debug, Clone, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum HelperInputEvent {
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
    enum HelperRequest {
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
    enum HelperEvent {
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

    /// Target process and attached console handles owned by the helper.
    /// helper 持有的目标进程与已附着控制台句柄。
    struct ConsoleSession {
        /// Target process handle.
        /// 目标进程句柄。
        process: HANDLE,
        /// Target thread handle.
        /// 目标线程句柄。
        thread: HANDLE,
        /// Console input handle.
        /// 控制台输入句柄。
        input: HANDLE,
        /// Console output handle.
        /// 控制台输出句柄。
        output: HANDLE,
        /// Target process identifier.
        /// 目标进程标识。
        pid: u32,
        /// Whether stop was requested.
        /// 是否已请求停止。
        stopped: bool,
    }

    impl Drop for ConsoleSession {
        /// Releases target process and console handles.
        /// 释放目标进程与控制台句柄。
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.input);
                CloseHandle(self.output);
                CloseHandle(self.thread);
                CloseHandle(self.process);
                FreeConsole();
            }
        }
    }

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
                if let Ok((rows, cols, frame)) = read_screen_text(session.output) {
                    if frame != last_frame {
                        frame_seq = frame_seq.saturating_add(1);
                        last_frame = frame.clone();
                        emit_event(&HelperEvent::Frame {
                            seq: frame_seq,
                            rows,
                            cols,
                            text: frame,
                        })?;
                    }
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
        if let Ok((rows, cols, frame)) = read_screen_text(output) {
            if frame != *last_frame {
                *frame_seq = frame_seq.saturating_add(1);
                *last_frame = frame.clone();
                emit_event(&HelperEvent::Frame {
                    seq: *frame_seq,
                    rows,
                    cols,
                    text: frame,
                })?;
            }
        }
        Ok(())
    }

    /// Starts the target process in a new Windows console and opens console handles.
    /// 在新的 Windows 控制台中启动目标进程并打开控制台句柄。
    fn start_console_session(
        program: &str,
        args: &[String],
        cwd: &str,
        rows: u16,
        cols: u16,
    ) -> Result<ConsoleSession, String> {
        let mut command_line = wide_null(&windows_command_line(program, args));
        let mut cwd_wide = wide_null(cwd);
        let mut startup_info = unsafe { zeroed::<STARTUPINFOW>() };
        startup_info.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        startup_info.dwFlags = STARTF_USESHOWWINDOW;
        startup_info.wShowWindow = SW_HIDE as u16;
        let mut process_info = unsafe { zeroed::<PROCESS_INFORMATION>() };
        let created = unsafe {
            CreateProcessW(
                null(),
                command_line.as_mut_ptr(),
                null(),
                null(),
                0,
                CREATE_NEW_CONSOLE | CREATE_UNICODE_ENVIRONMENT,
                null(),
                cwd_wide.as_mut_ptr(),
                &startup_info,
                &mut process_info,
            )
        };
        if created == 0 {
            return Err(last_os_error("CreateProcessW failed"));
        }
        if let Err(error) = attach_console_with_retry(process_info.dwProcessId) {
            cleanup_failed_console_start(&process_info, None, None);
            return Err(error);
        }
        let input = match open_console_handle("CONIN$") {
            Ok(input) => input,
            Err(error) => {
                cleanup_failed_console_start(&process_info, None, None);
                return Err(error);
            }
        };
        let output = match open_console_handle("CONOUT$") {
            Ok(output) => output,
            Err(error) => {
                cleanup_failed_console_start(&process_info, Some(input), None);
                return Err(error);
            }
        };
        if let Err(error) = resize_console(output, rows, cols) {
            cleanup_failed_console_start(&process_info, Some(input), Some(output));
            return Err(error);
        }
        Ok(ConsoleSession {
            process: process_info.hProcess,
            thread: process_info.hThread,
            input,
            output,
            pid: process_info.dwProcessId,
            stopped: false,
        })
    }

    /// Cleans up a target process when console attachment or handle initialization fails.
    /// 当控制台附着或句柄初始化失败时清理目标进程。
    fn cleanup_failed_console_start(
        process_info: &PROCESS_INFORMATION,
        input: Option<HANDLE>,
        output: Option<HANDLE>,
    ) {
        unsafe {
            if let Some(input) = input {
                CloseHandle(input);
            }
            if let Some(output) = output {
                CloseHandle(output);
            }
        }
        let _ = terminate_target_process_tree(process_info.dwProcessId, process_info.hProcess, 1);
        unsafe {
            CloseHandle(process_info.hThread);
            CloseHandle(process_info.hProcess);
            FreeConsole();
        }
    }

    /// Terminates descendants of one target process before terminating the root process handle.
    /// 先终止目标进程的后代进程，再终止根进程句柄。
    fn terminate_target_process_tree(
        root_pid: u32,
        root_process: HANDLE,
        exit_code: u32,
    ) -> Result<(), String> {
        let mut cleanup_diagnostics = Vec::new();
        match collect_descendant_process_ids(root_pid) {
            Ok(descendants) => {
                for pid in descendants.into_iter().rev() {
                    if let Err(error) = terminate_process_by_pid(pid, exit_code) {
                        cleanup_diagnostics.push(error);
                    }
                }
            }
            Err(error) => cleanup_diagnostics.push(error),
        }
        // Root may exit naturally while the stop request is waiting in the helper queue.
        // 当停止请求在 helper 队列中等待时，根进程可能已经自然退出。
        if unsafe { WaitForSingleObject(root_process, 0) } == WAIT_OBJECT_0 {
            return Ok(());
        }
        let root_terminated = unsafe { TerminateProcess(root_process, exit_code) } != 0;
        if !root_terminated {
            let mut error = last_os_error(&format!("TerminateProcess({root_pid}) failed"));
            if !cleanup_diagnostics.is_empty() {
                error.push_str("; child cleanup diagnostics: ");
                error.push_str(&cleanup_diagnostics.join("; "));
            }
            return Err(error);
        }
        Ok(())
    }

    /// Collects descendant process ids for one root process from a ToolHelp snapshot.
    /// 从 ToolHelp 快照中收集一个根进程的后代进程标识。
    fn collect_descendant_process_ids(root_pid: u32) -> Result<Vec<u32>, String> {
        let entries = process_entries_snapshot()?;
        let mut descendants = Vec::new();
        let mut stack = vec![root_pid];
        while let Some(parent_pid) = stack.pop() {
            for entry in &entries {
                if entry.parent_pid == parent_pid && entry.pid != root_pid {
                    descendants.push(entry.pid);
                    stack.push(entry.pid);
                }
            }
        }
        Ok(descendants)
    }

    /// One compact process entry captured from a ToolHelp process snapshot.
    /// 从 ToolHelp 进程快照中捕获的一条紧凑进程记录。
    #[derive(Debug, Clone, Copy)]
    struct ProcessSnapshotEntry {
        /// Process identifier.
        /// 进程标识。
        pid: u32,
        /// Parent process identifier.
        /// 父进程标识。
        parent_pid: u32,
    }

    /// Returns all process id and parent id pairs from one ToolHelp snapshot.
    /// 从 ToolHelp 快照返回全部进程标识与父进程标识对。
    fn process_entries_snapshot() -> Result<Vec<ProcessSnapshotEntry>, String> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(last_os_error("CreateToolhelp32Snapshot failed"));
        }
        let mut entries = Vec::new();
        let mut entry = unsafe { zeroed::<PROCESSENTRY32W>() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while ok {
            entries.push(ProcessSnapshotEntry {
                pid: entry.th32ProcessID,
                parent_pid: entry.th32ParentProcessID,
            });
            ok = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        unsafe {
            CloseHandle(snapshot);
        }
        Ok(entries)
    }

    /// Terminates one process by id when the helper can open it with terminate permission.
    /// 当 helper 可用终止权限打开进程时按进程标识终止该进程。
    fn terminate_process_by_pid(pid: u32, exit_code: u32) -> Result<(), String> {
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
        if handle.is_null() {
            return Err(last_os_error(&format!("OpenProcess({pid}) failed")));
        }
        let terminated = unsafe { TerminateProcess(handle, exit_code) } != 0;
        unsafe {
            CloseHandle(handle);
        }
        if terminated {
            Ok(())
        } else {
            Err(last_os_error(&format!("TerminateProcess({pid}) failed")))
        }
    }

    /// Attaches the helper process to the target console, retrying while the console initializes.
    /// 将 helper 进程附着到目标控制台，并在控制台初始化期间重试。
    fn attach_console_with_retry(process_id: u32) -> Result<(), String> {
        unsafe {
            FreeConsole();
        }
        let mut last_error = None;
        for _ in 0..20 {
            if unsafe { AttachConsole(process_id) } != 0 {
                return Ok(());
            }
            last_error = Some(io::Error::last_os_error());
            thread::sleep(Duration::from_millis(50));
        }
        Err(format!(
            "AttachConsole({process_id}) failed: {}",
            last_error.unwrap_or_else(io::Error::last_os_error)
        ))
    }

    /// Opens one console pseudo-file handle.
    /// 打开一个控制台伪文件句柄。
    fn open_console_handle(name: &str) -> Result<HANDLE, String> {
        let mut wide = wide_null(name);
        let handle = unsafe {
            CreateFileW(
                wide.as_mut_ptr(),
                0x8000_0000u32 | 0x4000_0000u32,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            Err(last_os_error(&format!("CreateFileW({name}) failed")))
        } else {
            Ok(handle)
        }
    }

    /// Resizes the attached console screen buffer and visible window.
    /// 调整已附着控制台的屏幕缓冲区与可见窗口。
    fn resize_console(output: HANDLE, rows: u16, cols: u16) -> Result<(), String> {
        let rows = rows.clamp(1, 300) as i16;
        let cols = cols.clamp(1, 500) as i16;
        let size = COORD { X: cols, Y: rows };
        let ok_buffer = unsafe { SetConsoleScreenBufferSize(output, size) } != 0;
        let mut rect = SMALL_RECT {
            Left: 0,
            Top: 0,
            Right: cols.saturating_sub(1),
            Bottom: rows.saturating_sub(1),
        };
        let ok_window = unsafe { SetConsoleWindowInfo(output, 1, &mut rect) } != 0;
        if ok_buffer || ok_window {
            Ok(())
        } else {
            Err(last_os_error("Failed to resize console"))
        }
    }

    /// Reads the visible console screen as ANSI-styled text.
    /// 将可见控制台屏幕读取为带 ANSI 样式的文本。
    fn read_screen_text(output: HANDLE) -> Result<(u16, u16, String), String> {
        let mut info = unsafe { zeroed::<CONSOLE_SCREEN_BUFFER_INFO>() };
        if unsafe { GetConsoleScreenBufferInfo(output, &mut info) } == 0 {
            return Err(last_os_error("GetConsoleScreenBufferInfo failed"));
        }
        let rows = (info.srWindow.Bottom - info.srWindow.Top + 1).max(1) as u16;
        let cols = (info.srWindow.Right - info.srWindow.Left + 1).max(1) as u16;
        let cell_count = rows as usize * cols as usize;
        let mut buffer = vec![CHAR_INFO::default(); cell_count];
        let mut read_region = info.srWindow;
        let ok = unsafe {
            ReadConsoleOutputW(
                output,
                buffer.as_mut_ptr(),
                COORD {
                    X: cols as i16,
                    Y: rows as i16,
                },
                COORD { X: 0, Y: 0 },
                &mut read_region,
            )
        } != 0;
        if !ok {
            return Err(last_os_error("ReadConsoleOutputW failed"));
        }
        let mut lines = Vec::new();
        for row in 0..rows as usize {
            let start = row * cols as usize;
            let end = start + cols as usize;
            lines.push(render_screen_row_as_ansi(&buffer[start..end]));
        }
        Ok((rows, cols, lines.join("\n").trim_end().to_string()))
    }

    /// Renders one console row as text with SGR spans for changed attributes.
    /// 将单行控制台单元格渲染为属性变化时带 SGR 片段的文本。
    fn render_screen_row_as_ansi(cells: &[CHAR_INFO]) -> String {
        let visible_end = cells
            .iter()
            .rposition(|cell| console_cell_char(*cell) != ' ')
            .map(|index| index + 1)
            .unwrap_or(0);
        let mut rendered = String::new();
        let mut active_attr = None;
        for cell in &cells[..visible_end] {
            let attr = console_visible_color_attr(cell.Attributes);
            if active_attr != Some(attr) {
                rendered.push_str(&console_attr_to_sgr(attr));
                active_attr = Some(attr);
            }
            rendered.push(console_cell_char(*cell));
        }
        if active_attr.is_some() {
            rendered.push_str("\x1b[0m");
        }
        rendered
    }

    /// Extracts the Unicode character from one console cell.
    /// 从单个控制台单元格中提取 Unicode 字符。
    fn console_cell_char(cell: CHAR_INFO) -> char {
        let code_unit = unsafe { cell.Char.UnicodeChar };
        char::from_u32(code_unit as u32).unwrap_or(' ')
    }

    /// Keeps only foreground and background color bits from a console attribute.
    /// 仅保留控制台属性中的前景色与背景色位。
    fn console_visible_color_attr(attr: u16) -> u16 {
        attr & (FOREGROUND_BLUE
            | FOREGROUND_GREEN
            | FOREGROUND_RED
            | FOREGROUND_INTENSITY
            | BACKGROUND_BLUE
            | BACKGROUND_GREEN
            | BACKGROUND_RED
            | BACKGROUND_INTENSITY)
    }

    /// Converts a Windows console color attribute into one ANSI SGR sequence.
    /// 将 Windows 控制台颜色属性转换为一个 ANSI SGR 序列。
    fn console_attr_to_sgr(attr: u16) -> String {
        format!(
            "\x1b[{};{}m",
            console_color_bits_to_sgr(attr, false),
            console_color_bits_to_sgr(attr >> 4, true)
        )
    }

    /// Converts one four-bit console color nibble into a foreground or background SGR code.
    /// 将四位控制台颜色值转换为前景或背景 SGR 码。
    fn console_color_bits_to_sgr(bits: u16, background: bool) -> u16 {
        let base = if background { 40 } else { 30 };
        let bright_base = if background { 100 } else { 90 };
        let ansi_index = match bits & 0x07 {
            0 => 0,
            1 => 4,
            2 => 2,
            3 => 6,
            4 => 1,
            5 => 5,
            6 => 3,
            _ => 7,
        };
        if bits & 0x08 != 0 {
            bright_base + ansi_index
        } else {
            base + ansi_index
        }
    }

    /// Writes text as a sequence of console key events.
    /// 将文本写为一组控制台按键事件。
    fn write_text_input(input: HANDLE, text: &str) -> Result<(), String> {
        let events = text
            .chars()
            .map(|character| HelperInputEvent::Key {
                key: character.to_string(),
            })
            .collect::<Vec<_>>();
        write_input_events(input, &events)
    }

    /// Writes structured input events into the console input buffer.
    /// 将结构化输入事件写入控制台输入缓冲区。
    fn write_input_events(input: HANDLE, events: &[HelperInputEvent]) -> Result<(), String> {
        let mut records = Vec::new();
        for event in events {
            append_input_records(event, &mut records)?;
        }
        if records.is_empty() {
            return Ok(());
        }
        let mut written = 0u32;
        let ok = unsafe {
            WriteConsoleInputW(input, records.as_ptr(), records.len() as u32, &mut written)
        } != 0;
        if ok && written == records.len() as u32 {
            Ok(())
        } else {
            Err(last_os_error("WriteConsoleInputW failed"))
        }
    }

    /// Appends key-down and key-up input records for one logical event.
    /// 为一个逻辑事件追加 key-down 与 key-up 输入记录。
    fn append_input_records(
        event: &HelperInputEvent,
        records: &mut Vec<INPUT_RECORD>,
    ) -> Result<(), String> {
        match event {
            HelperInputEvent::Text { text } | HelperInputEvent::Paste { text } => {
                for character in text.chars() {
                    append_text_character_records(character, records);
                }
            }
            HelperInputEvent::Key { key } => {
                append_named_key_records(key, false, false, false, records)?
            }
            HelperInputEvent::Chord {
                key,
                ctrl,
                alt,
                shift,
                meta,
            } => append_named_key_records(key, *ctrl || *meta, *alt, *shift, records)?,
        }
        Ok(())
    }

    /// Appends one named key or literal character input record pair.
    /// 追加一组命名按键或字面字符输入记录。
    fn append_named_key_records(
        key: &str,
        ctrl: bool,
        alt: bool,
        shift: bool,
        records: &mut Vec<INPUT_RECORD>,
    ) -> Result<(), String> {
        let modifier_state = modifier_control_state(ctrl, alt, shift);
        if key.chars().count() == 1 {
            let mut character = key.chars().next().unwrap_or_default();
            if ctrl && character.is_ascii_alphabetic() {
                character = (character.to_ascii_uppercase() as u8 - b'A' + 1) as char;
            }
            append_character_records(character, modifier_state, records);
            return Ok(());
        }
        let normalized = key.to_ascii_lowercase();
        let (vk, character, enhanced) = match normalized.as_str() {
            "enter" => (VK_RETURN, '\r', false),
            "tab" => (VK_TAB, '\t', false),
            "esc" | "escape" => (VK_ESCAPE, '\x1b', false),
            "backspace" => (VK_BACK, '\x08', false),
            "delete" => (VK_DELETE, '\0', true),
            "arrowup" => (VK_UP, '\0', true),
            "arrowdown" => (VK_DOWN, '\0', true),
            "arrowleft" => (VK_LEFT, '\0', true),
            "arrowright" => (VK_RIGHT, '\0', true),
            "home" => (VK_HOME, '\0', true),
            "end" => (VK_END, '\0', true),
            "pageup" => (VK_PRIOR, '\0', true),
            "pagedown" => (VK_NEXT, '\0', true),
            "insert" => (VK_INSERT, '\0', true),
            value if value.starts_with('f') => {
                let number = value[1..]
                    .parse::<u16>()
                    .map_err(|_| format!("Unsupported function key `{key}`."))?;
                if !(1..=12).contains(&number) {
                    return Err(format!("Unsupported function key `{key}`."));
                }
                (VK_F1 + number - 1, '\0', false)
            }
            _ => return Err(format!("Unsupported terminal key `{key}`.")),
        };
        let control_state = if enhanced {
            modifier_state | ENHANCED_KEY
        } else {
            modifier_state
        };
        append_key_records(vk, character, control_state, records);
        Ok(())
    }

    /// Appends one literal character input record pair.
    /// 追加一组字面字符输入记录。
    fn append_text_character_records(character: char, records: &mut Vec<INPUT_RECORD>) {
        let (vk, character) = match character {
            '\n' | '\r' => (VK_RETURN, '\r'),
            _ => (0, character),
        };
        append_key_records(vk, character, 0, records);
    }

    /// Appends one keyboard-oriented character input record pair.
    /// 追加一组面向键盘按键的字符输入记录。
    fn append_character_records(
        character: char,
        control_key_state: u32,
        records: &mut Vec<INPUT_RECORD>,
    ) {
        let vk = match character {
            '\n' | '\r' => VK_RETURN,
            '\t' => VK_TAB,
            '\x1b' => VK_ESCAPE,
            _ => 0,
        };
        let character = if character == '\n' { '\r' } else { character };
        append_key_records(vk, character, control_key_state, records);
    }

    /// Appends key-down and key-up records.
    /// 追加 key-down 与 key-up 记录。
    fn append_key_records(
        virtual_key_code: u16,
        character: char,
        control_key_state: u32,
        records: &mut Vec<INPUT_RECORD>,
    ) {
        records.push(key_record(
            true,
            virtual_key_code,
            character,
            control_key_state,
        ));
        records.push(key_record(
            false,
            virtual_key_code,
            character,
            control_key_state,
        ));
    }

    /// Builds one Win32 key input record.
    /// 构建一条 Win32 按键输入记录。
    fn key_record(
        key_down: bool,
        virtual_key_code: u16,
        character: char,
        control_key_state: u32,
    ) -> INPUT_RECORD {
        let mut record = INPUT_RECORD {
            EventType: KEY_EVENT as u16,
            Event: INPUT_RECORD_0 {
                KeyEvent: KEY_EVENT_RECORD {
                    bKeyDown: i32::from(key_down),
                    wRepeatCount: 1,
                    wVirtualKeyCode: virtual_key_code,
                    wVirtualScanCode: unsafe {
                        MapVirtualKeyW(virtual_key_code as u32, MAPVK_VK_TO_VSC) as u16
                    },
                    uChar: KEY_EVENT_RECORD_0 {
                        UnicodeChar: character as u16,
                    },
                    dwControlKeyState: control_key_state,
                },
            },
        };
        record.EventType = KEY_EVENT as u16;
        record
    }

    /// Builds a Win32 modifier control-state bit mask.
    /// 构建 Win32 修饰键控制状态位掩码。
    fn modifier_control_state(ctrl: bool, alt: bool, shift: bool) -> u32 {
        let mut state = 0u32;
        if ctrl {
            state |= LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED;
        }
        if alt {
            state |= LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED;
        }
        if shift {
            state |= SHIFT_PRESSED;
        }
        state
    }

    /// Emits one JSONL event to stdout.
    /// 向 stdout 发出一条 JSONL 事件。
    fn emit_event(event: &HelperEvent) -> Result<(), String> {
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

    /// Renders a Windows command line from program and arguments.
    /// 根据程序与参数渲染 Windows 命令行。
    fn windows_command_line(program: &str, args: &[String]) -> String {
        std::iter::once(program.to_string())
            .chain(args.iter().cloned())
            .map(|value| quote_windows_argument(&value))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Quotes one Windows command-line argument.
    /// 引用一个 Windows 命令行参数。
    fn quote_windows_argument(value: &str) -> String {
        if value.is_empty() {
            return String::from("\"\"");
        }
        if !value.contains([' ', '\t', '"', '\\']) {
            return value.to_string();
        }
        let mut quoted = String::from("\"");
        let mut backslashes = 0usize;
        for character in value.chars() {
            if character == '\\' {
                backslashes += 1;
            } else if character == '"' {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            } else {
                quoted.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                quoted.push(character);
            }
        }
        quoted.push_str(&"\\".repeat(backslashes * 2));
        quoted.push('"');
        quoted
    }

    /// Converts a string into a null-terminated UTF-16 buffer.
    /// 将字符串转换为以空字符结尾的 UTF-16 缓冲区。
    fn wide_null(value: &str) -> Vec<u16> {
        OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// Formats the last operating-system error with context.
    /// 使用上下文格式化最后一个操作系统错误。
    fn last_os_error(context: &str) -> String {
        format!("{context}: {}", io::Error::last_os_error())
    }

    #[cfg(test)]
    mod tests {
        use super::{
            BACKGROUND_INTENSITY, BACKGROUND_RED, FOREGROUND_BLUE, FOREGROUND_GREEN,
            FOREGROUND_INTENSITY, FOREGROUND_RED, console_color_bits_to_sgr,
        };

        /// Verifies Windows color bits are converted to standard ANSI SGR color order.
        /// 验证 Windows 颜色位会转换为标准 ANSI SGR 颜色顺序。
        #[test]
        fn console_color_bits_to_sgr_maps_windows_colors_to_ansi_order() {
            assert_eq!(console_color_bits_to_sgr(FOREGROUND_RED, false), 31);
            assert_eq!(console_color_bits_to_sgr(FOREGROUND_GREEN, false), 32);
            assert_eq!(console_color_bits_to_sgr(FOREGROUND_BLUE, false), 34);
            assert_eq!(
                console_color_bits_to_sgr(FOREGROUND_RED | FOREGROUND_INTENSITY, false),
                91
            );
            assert_eq!(
                console_color_bits_to_sgr((BACKGROUND_RED | BACKGROUND_INTENSITY) >> 4, true),
                101
            );
        }
    }
}

#[cfg(windows)]
fn main() {
    std::process::exit(windows_helper::run());
}

#[cfg(not(windows))]
fn main() {
    eprintln!("vulcan-windows-console-helper is only supported on Windows.");
    std::process::exit(1);
}
