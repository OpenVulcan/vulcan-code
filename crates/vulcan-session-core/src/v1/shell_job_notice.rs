//! Text form of a background shell job finish notice delivered to the model as in-turn guidance.
//! 作为回合内指引送达模型的后台 shell 任务结束通知的文本形式。
//!
//! The Core writes the notice with [`ShellJobNoticeFacts::render`]; clients recognise the stored
//! agent-message input with [`ShellJobNoticeFacts::parse`] to label it, so this module is the only
//! Rust definition of the format. The WebUI mirrors the parser in `shell-job-notice.ts`.
//! Core 用 [`ShellJobNoticeFacts::render`] 写入通知；客户端用 [`ShellJobNoticeFacts::parse`]
//! 识别已存储的代理消息输入并为其加标签，因此本模块是该格式唯一的 Rust 定义。WebUI 在
//! `shell-job-notice.ts` 中镜像该解析器。

/// Opening tag prefix; the job id attribute follows.
/// 开始标签前缀；其后是任务标识属性。
const OPEN_PREFIX: &str = "<vulcan_shell_job_notice version=\"1\" job_id=\"";
/// Closing tag.
/// 结束标签。
const CLOSE_TAG: &str = "</vulcan_shell_job_notice>";
/// Line separators of the body fields, searched in this order.
/// 正文字段的行分隔符，按此顺序查找。
const PURPOSE: &str = "\nPurpose: ";
const COMMAND: &str = "\nCommand: ";
const STATUS: &str = "\nStatus: ";
const DURATION: &str = "\nDuration: ";
const EXIT_CODE: &str = "\nExit code: ";
const REASON: &str = "\nReason: ";
const READ_HINT: &str = "\nRead its output with shell_job";

/// Facts one finish notice carries.
/// 一条结束通知携带的事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellJobNoticeFacts {
    /// Job identity.
    /// 任务身份。
    pub job_id: String,
    /// Purpose the model gave for the command.
    /// 模型给出的命令用途。
    pub purpose: String,
    /// Command as the model wrote it; may span several lines.
    /// 模型书写的命令；可跨多行。
    pub command: String,
    /// Final status word, such as `completed` or `failed`.
    /// 最终状态词，例如 `completed` 或 `failed`。
    pub status: String,
    /// Run time in milliseconds.
    /// 运行时长（毫秒）。
    pub duration_ms: u64,
    /// Exit code when the process reported one.
    /// 进程报告退出码时的退出码。
    pub exit_code: Option<i32>,
    /// Why the job ended abnormally, when known.
    /// 已知时，任务异常结束的原因。
    pub failure_reason: Option<String>,
}

impl ShellJobNoticeFacts {
    /// Renders the model-facing notice text.
    /// 渲染面向模型的通知文本。
    pub fn render(&self) -> String {
        let mut text = format!(
            "{OPEN_PREFIX}{job}\">\nBackground shell job {job} finished.{PURPOSE}{purpose}{COMMAND}{command}{STATUS}{status}{DURATION}{duration} ms",
            job = self.job_id,
            purpose = self.purpose,
            command = self.command,
            status = self.status,
            duration = self.duration_ms,
        );
        if let Some(code) = self.exit_code {
            text.push_str(&format!("{EXIT_CODE}{code}"));
        }
        if let Some(reason) = &self.failure_reason {
            text.push_str(&format!("{REASON}{reason}"));
        }
        text.push_str(&format!(
            "{READ_HINT} (action `read` or `search`, job_id `{}`) if you need it.\n{CLOSE_TAG}",
            self.job_id
        ));
        text
    }

    /// Recognises one rendered notice; any other text yields `None`.
    /// 识别一条已渲染的通知；其他文本返回 `None`。
    ///
    /// Purpose and command are delimited by the fields that follow them, so multi-line commands
    /// survive; the status block is located from the end.
    /// 用途与命令由其后的字段界定，因此多行命令得以保留；状态块从末尾定位。
    pub fn parse(text: &str) -> Option<Self> {
        let rest = text.strip_prefix(OPEN_PREFIX)?;
        let (job_id, rest) = rest.split_once("\">")?;
        let body = rest.strip_suffix(CLOSE_TAG)?;
        let purpose_start = body.find(PURPOSE)? + PURPOSE.len();
        let command_marker = purpose_start + body[purpose_start..].find(COMMAND)?;
        let status_marker = body.rfind(STATUS)?;
        let hint_marker = body.rfind(READ_HINT)?;
        let command_start = command_marker + COMMAND.len();
        if status_marker < command_start || hint_marker < status_marker {
            return None;
        }
        let tail = &body[status_marker..hint_marker];
        let field = |marker: &str| {
            tail.find(marker).map(|start| {
                let value = &tail[start + marker.len()..];
                value.split('\n').next().unwrap_or_default()
            })
        };
        let duration_ms = field(DURATION)?.strip_suffix(" ms")?.parse().ok()?;
        let exit_code = match field(EXIT_CODE) {
            Some(code) => Some(code.parse().ok()?),
            None => None,
        };
        let failure_reason = tail
            .find(REASON)
            .map(|start| tail[start + REASON.len()..].to_string());
        Some(Self {
            job_id: job_id.to_string(),
            purpose: body[purpose_start..command_marker].to_string(),
            command: body[command_start..status_marker].to_string(),
            status: field(STATUS)?.to_string(),
            duration_ms,
            exit_code,
            failure_reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rendered notices parse back to the same facts, including multi-line commands and reasons.
    /// 渲染的通知可解析回相同事实，包括多行命令与原因。
    #[test]
    fn rendered_notices_parse_back() {
        let finished = ShellJobNoticeFacts {
            job_id: String::from("job-1"),
            purpose: String::from("Run the tests"),
            command: String::from("cargo test\necho Status: done"),
            status: String::from("completed"),
            duration_ms: 1200,
            exit_code: Some(0),
            failure_reason: None,
        };
        assert_eq!(
            ShellJobNoticeFacts::parse(&finished.render()),
            Some(finished)
        );
        let stopped = ShellJobNoticeFacts {
            job_id: String::from("job-2"),
            purpose: String::new(),
            command: String::from("npm run dev"),
            status: String::from("stopped"),
            duration_ms: 5,
            exit_code: None,
            failure_reason: Some(String::from("stopped by the user")),
        };
        assert_eq!(ShellJobNoticeFacts::parse(&stopped.render()), Some(stopped));
    }

    /// Ordinary agent messages and truncated notices are not recognised.
    /// 普通代理消息与截断的通知不被识别。
    #[test]
    fn other_text_is_not_a_notice() {
        assert_eq!(
            ShellJobNoticeFacts::parse("Please review the result."),
            None
        );
        let text = ShellJobNoticeFacts {
            job_id: String::from("job-3"),
            purpose: String::from("Build"),
            command: String::from("make"),
            status: String::from("failed"),
            duration_ms: 1,
            exit_code: Some(2),
            failure_reason: None,
        }
        .render();
        assert_eq!(ShellJobNoticeFacts::parse(&text[..text.len() - 3]), None);
    }
}
