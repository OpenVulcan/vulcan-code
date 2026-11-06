// Recognises the background shell job finish notices the Core records as agent-message inputs.
// The format is defined by `ShellJobNoticeFacts` in `vulcan-session-core` (v1/shell_job_notice.rs); this parser mirrors it.
// 识别 Core 记录为代理消息输入的后台 shell 任务结束通知。
// 格式由 `vulcan-session-core` 的 `ShellJobNoticeFacts`（v1/shell_job_notice.rs）定义；此解析器与之保持一致。

const OPEN_PREFIX = "<vulcan_shell_job_notice version=\"1\" job_id=\"";
const CLOSE_TAG = "</vulcan_shell_job_notice>";
const PURPOSE = "\nPurpose: ";
const COMMAND = "\nCommand: ";
const STATUS = "\nStatus: ";
const EXIT_CODE = "\nExit code: ";
const READ_HINT = "\nRead its output with shell_job";

// ShellJobNoticeFacts names the finished job and how it ended.
// ShellJobNoticeFacts 指明已结束的任务及其结束方式。
export interface ShellJobNoticeFacts {
  jobId: string;
  purpose: string;
  command: string;
  status: string;
  exitCode: number | null;
}

// decodeShellJobNotice returns the facts of one notice text, or null for any other text.
// Purpose and command are delimited by the fields after them, so multi-line commands survive.
// decodeShellJobNotice 返回一条通知文本的事实；其他文本返回 null。用途与命令由其后的字段界定，因此多行命令得以保留。
export function decodeShellJobNotice(text: string): ShellJobNoticeFacts | null {
  if (!text.startsWith(OPEN_PREFIX) || !text.endsWith(CLOSE_TAG)) return null;
  const rest = text.slice(OPEN_PREFIX.length, text.length - CLOSE_TAG.length);
  const idEnd = rest.indexOf("\">");
  if (idEnd < 0) return null;
  const body = rest.slice(idEnd + 2);
  const purposeAt = body.indexOf(PURPOSE);
  const commandAt = purposeAt < 0 ? -1 : body.indexOf(COMMAND, purposeAt + PURPOSE.length);
  const statusAt = body.lastIndexOf(STATUS);
  const hintAt = body.lastIndexOf(READ_HINT);
  if (commandAt < 0 || statusAt < commandAt + COMMAND.length || hintAt < statusAt) return null;
  const tail = body.slice(statusAt, hintAt);
  const field = (marker: string): string | null => {
    const at = tail.indexOf(marker);
    return at < 0 ? null : tail.slice(at + marker.length).split("\n")[0]!;
  };
  const exitText = field(EXIT_CODE);
  const exitCode = exitText === null ? null : Number.parseInt(exitText, 10);
  return {
    jobId: rest.slice(0, idEnd),
    purpose: body.slice(purposeAt + PURPOSE.length, commandAt),
    command: body.slice(commandAt + COMMAND.length, statusAt),
    status: field(STATUS) ?? "",
    exitCode: exitCode === null || Number.isNaN(exitCode) ? null : exitCode,
  };
}
