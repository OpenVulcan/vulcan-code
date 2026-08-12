use super::*;

/// Converts terminal-session inputs into backend-neutral structured input events.
/// 将终端会话输入转换为后端无关的结构化输入事件。
pub(crate) fn terminal_input_events_from_inputs(
    inputs: &[TerminalSessionInput],
) -> Vec<CommandSessionInputEvent> {
    inputs
        .iter()
        .map(|input| match input {
            TerminalSessionInput::Text { text } => {
                CommandSessionInputEvent::Text { text: text.clone() }
            }
            TerminalSessionInput::Paste { text } => {
                CommandSessionInputEvent::Paste { text: text.clone() }
            }
            TerminalSessionInput::Key { key } => CommandSessionInputEvent::Key { key: key.clone() },
            TerminalSessionInput::Chord {
                key,
                ctrl,
                alt,
                shift,
                meta,
            } => CommandSessionInputEvent::Chord {
                key: key.clone(),
                ctrl: ctrl.unwrap_or(false),
                alt: alt.unwrap_or(false),
                shift: shift.unwrap_or(false),
                meta: meta.unwrap_or(false),
            },
        })
        .collect()
}

/// Returns whether the input list requests a foreground-process interrupt.
/// 返回输入列表是否请求中断前台进程。
pub(crate) fn terminal_inputs_request_interrupt(inputs: &[TerminalSessionInput]) -> bool {
    inputs.iter().any(|input| {
        matches!(
            input,
            TerminalSessionInput::Chord {
                key,
                ctrl: Some(true),
                alt,
                shift,
                meta,
            } if key.eq_ignore_ascii_case("c")
                && !alt.unwrap_or(false)
                && !shift.unwrap_or(false)
                && !meta.unwrap_or(false)
        )
    })
}

/// Returns the visible label for one terminal input item.
/// 返回一条终端输入项的可见标签。
pub(crate) fn terminal_input_label(input: &TerminalSessionInput) -> String {
    match input {
        TerminalSessionInput::Text { text } => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                String::from("text")
            } else {
                trimmed.chars().take(80).collect()
            }
        }
        TerminalSessionInput::Paste { text } => {
            format!("paste {} chars", text.chars().count())
        }
        TerminalSessionInput::Key { key } => key.clone(),
        TerminalSessionInput::Chord {
            key,
            ctrl,
            alt,
            shift,
            meta,
        } => {
            let mut parts = Vec::new();
            if ctrl.unwrap_or(false) {
                parts.push("Ctrl");
            }
            if alt.unwrap_or(false) {
                parts.push("Alt");
            }
            if shift.unwrap_or(false) {
                parts.push("Shift");
            }
            if meta.unwrap_or(false) {
                parts.push("Meta");
            }
            parts.push(key.as_str());
            parts.join("+")
        }
    }
}
