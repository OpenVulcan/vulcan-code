//! Console input event translation and writing helpers.
//! 控制台输入事件转换与写入辅助。

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Console::{
    ENHANCED_KEY, INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT, KEY_EVENT_RECORD, KEY_EVENT_RECORD_0,
    LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED, RIGHT_ALT_PRESSED, RIGHT_CTRL_PRESSED, SHIFT_PRESSED,
    WriteConsoleInputW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MAPVK_VK_TO_VSC, MapVirtualKeyW, VK_BACK, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1,
    VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_TAB, VK_UP,
};

use super::protocol::HelperInputEvent;
use super::util::last_os_error;

/// Writes text as a sequence of console key events.
/// 将文本写为一组控制台按键事件。
pub(super) fn write_text_input(input: HANDLE, text: &str) -> Result<(), String> {
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
pub(super) fn write_input_events(input: HANDLE, events: &[HelperInputEvent]) -> Result<(), String> {
    let mut records = Vec::new();
    for event in events {
        append_input_records(event, &mut records)?;
    }
    if records.is_empty() {
        return Ok(());
    }
    let mut written = 0u32;
    let ok =
        unsafe { WriteConsoleInputW(input, records.as_ptr(), records.len() as u32, &mut written) }
            != 0;
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
