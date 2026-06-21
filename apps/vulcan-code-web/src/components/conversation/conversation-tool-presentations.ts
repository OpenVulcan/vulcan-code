import type {
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";

// ToolPresentationDisplayState normalizes tool execution state for compact presentation cards.
// ToolPresentationDisplayState 为紧凑工具展示卡片规范化工具执行状态。
export type ToolPresentationDisplayState = "running" | "completed" | "failed" | "stopped" | "unknown";

// CommandExecutionPresentation describes one one-shot command-line execution display.
// CommandExecutionPresentation 描述一次性命令行执行展示。
export interface CommandExecutionPresentation {
  // Discriminator for future presentation adapters.
  // 供未来展示适配器使用的判别字段。
  kind: "command_execution";
  // Stable id used for expansion state across virtualized remounts.
  // 用于跨虚拟滚动重挂保持展开状态的稳定标识。
  id: string;
  // Command text shown in both collapsed and expanded states.
  // 折叠与展开状态都会展示的命令文本。
  command: string;
  // Best known stdout text, usually hydrated from hidden tool detail.
  // 当前可获得的 stdout 文本，通常从隐藏工具详情中补水。
  stdout: string;
  // Best known stderr text, usually hydrated from hidden tool detail.
  // 当前可获得的 stderr 文本，通常从隐藏工具详情中补水。
  stderr: string;
  // Process exit code when the host reported one.
  // 宿主已报告时的进程退出码。
  exitCode: number | null;
  // Current compact display state.
  // 当前紧凑展示状态。
  state: ToolPresentationDisplayState;
  // Detail reference used to hydrate full command output lazily.
  // 用于懒加载完整命令输出的详情引用。
  detailRef?: ConversationDetailRef;
}

// ToolPresentation is the union of all recognized custom tool displays.
// ToolPresentation 是所有已识别自定义工具展示的联合类型。
export type ToolPresentation = CommandExecutionPresentation;

// resolveToolPresentation builds a specialized presentation model or returns null for default rendering.
// resolveToolPresentation 构建专用展示模型；无法识别时返回 null 以回退默认渲染。
export function resolveToolPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  detailRef?: ConversationDetailRef,
): ToolPresentation | null {
  return resolveCommandExecutionPresentation(blocks, displayState, detailRef);
}

// hydrateCommandExecutionPresentation merges one hidden tool detail payload into an existing command presentation.
// hydrateCommandExecutionPresentation 将单条隐藏工具详情载荷合并进已有命令展示模型。
export function hydrateCommandExecutionPresentation(
  presentation: CommandExecutionPresentation,
  payload: ToolDetailPayload | null,
): CommandExecutionPresentation {
  if (!payload) {
    return presentation;
  }
  const display = shellCommandDisplayFromMetadata(payload.metadata) ?? parseShellCommandDisplayFromText(payload.body);
  if (!display) {
    return presentation;
  }
  return mergeCommandExecutionDisplay(presentation, display);
}

// commandPresentationStatusLabel returns the short Chinese state label shown in collapsed rows.
// commandPresentationStatusLabel 返回折叠行展示的短中文状态文案。
export function commandPresentationStatusLabel(state: ToolPresentationDisplayState): string {
  if (state === "running") return "执行中";
  if (state === "failed") return "执行失败";
  if (state === "stopped") return "已停止";
  return "已执行";
}

// resolveCommandExecutionPresentation recognizes one-shot shell_command rows and excludes command sessions.
// resolveCommandExecutionPresentation 识别一次性 shell_command 行，并排除命令会话。
function resolveCommandExecutionPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  detailRef?: ConversationDetailRef,
): CommandExecutionPresentation | null {
  if (blocks.some((block) => block.commandSessionId || metadataString(block.rawMetadata, ["host_result.kind", "hostResult.kind", "kind"]) === "command_session")) {
    return null;
  }
  const toolName = blocks.find((block) => block.toolName)?.toolName
    ?? blocks.map((block) => metadataString(block.rawMetadata, [
      "tool_name",
      "toolName",
      "host_result.payload.tool_name",
      "hostResult.payload.toolName",
      "host_result.payload.tool",
      "hostResult.payload.tool",
    ])).find(Boolean)
    ?? null;
  const hostKind = blocks
    .map((block) => metadataString(block.rawMetadata, ["host_result.kind", "hostResult.kind", "kind"]))
    .find(Boolean);
  if (toolName !== "shell_command" && hostKind !== "command_run") {
    return null;
  }

  const display = firstShellCommandDisplay(blocks);
  const command = display?.command
    ?? blocks.map((block) => commandFromMetadata(block.rawMetadata)).find(Boolean)
    ?? blocks.map((block) => commandFromText(block.body)).find(Boolean)
    ?? "";
  if (!command) {
    return null;
  }

  return {
    command,
    detailRef,
    exitCode: display?.exitCode ?? null,
    id: commandPresentationId(blocks, command),
    kind: "command_execution",
    state: displayState,
    stderr: display?.stderr ?? "",
    stdout: display?.stdout ?? "",
  };
}

// firstShellCommandDisplay returns the first structured shell_command display found on visible blocks.
// firstShellCommandDisplay 返回可见块上找到的首个结构化 shell_command 展示。
function firstShellCommandDisplay(blocks: ToolExecutionConversationBlock[]): ShellCommandDisplay | null {
  for (const block of blocks) {
    const display = shellCommandDisplayFromMetadata(block.rawMetadata) ?? parseShellCommandDisplayFromText(block.body);
    if (display) {
      return display;
    }
  }
  return null;
}

// mergeCommandExecutionDisplay applies structured shell_command fields without losing known values.
// mergeCommandExecutionDisplay 合并结构化 shell_command 字段，同时保留已有已知值。
function mergeCommandExecutionDisplay(
  presentation: CommandExecutionPresentation,
  display: ShellCommandDisplay,
): CommandExecutionPresentation {
  return {
    ...presentation,
    command: display.command || presentation.command,
    exitCode: display.exitCode ?? presentation.exitCode,
    stderr: display.stderr || presentation.stderr,
    stdout: display.stdout || presentation.stdout,
  };
}

// commandPresentationId creates a stable expansion key from tool identifiers and command text.
// commandPresentationId 根据工具标识与命令文本创建稳定展开键。
function commandPresentationId(blocks: ToolExecutionConversationBlock[], command: string): string {
  const identifier = blocks
    .map((block) => block.toolCallId ?? block.toolResultId ?? block.id)
    .find(Boolean);
  return `command:${identifier ?? compactHash(command)}`;
}

// ShellCommandDisplay stores parsed command output fields before they become a presentation model.
// ShellCommandDisplay 保存转为展示模型前解析出的命令输出字段。
interface ShellCommandDisplay {
  // Raw command line.
  // 原始命令行。
  command: string;
  // Parsed stdout text.
  // 解析出的 stdout 文本。
  stdout: string;
  // Parsed stderr text.
  // 解析出的 stderr 文本。
  stderr: string;
  // Parsed process exit code.
  // 解析出的进程退出码。
  exitCode: number | null;
}

// shellCommandDisplayFromMetadata reads a structured shell_command display from metadata.
// shellCommandDisplayFromMetadata 从 metadata 中读取结构化 shell_command 展示。
function shellCommandDisplayFromMetadata(metadata: unknown): ShellCommandDisplay | null {
  const display = objectValue(metadataValue(metadata, [
    "display",
    "metadata.display",
    "payload.display",
    "host_result.display",
    "hostResult.display",
    "host_result.payload.display",
    "hostResult.payload.display",
  ]));
  if (!display || stringValue(display.kind) !== "shell_command") {
    return null;
  }
  const command = stringValue(display.command) ?? "";
  const stdout = stringValue(display.stdout) ?? "";
  const stderr = stringValue(display.stderr) ?? "";
  const exitCode = numberValue(display.exit_code ?? display.exitCode);
  if (!command && !stdout && !stderr && exitCode === null) {
    return null;
  }
  return { command, exitCode, stderr, stdout };
}

// parseShellCommandDisplayFromText parses the stable shell_command model output format.
// parseShellCommandDisplayFromText 解析稳定的 shell_command 模型输出格式。
function parseShellCommandDisplayFromText(source: string): ShellCommandDisplay | null {
  const command = commandFromText(source) ?? "";
  const exitCode = exitCodeFromText(source);
  const stdout = sectionFromText(source, "[stdout]");
  const stderr = sectionFromText(source, "[stderr]");
  if (!command && !stdout && !stderr && exitCode === null) {
    return null;
  }
  return { command, exitCode, stderr, stdout };
}

// commandFromMetadata extracts the shell command argument from known metadata shapes.
// commandFromMetadata 从已知 metadata 形状中提取 shell 命令参数。
function commandFromMetadata(metadata: unknown): string | null {
  return metadataString(metadata, [
    "arguments.command",
    "payload.arguments.command",
    "host_result.arguments.command",
    "hostResult.arguments.command",
    "host_result.payload.arguments.command",
    "hostResult.payload.arguments.command",
    "host_result.payload.command",
    "hostResult.payload.command",
    "command",
  ]);
}

// commandFromText extracts a command header from rendered shell_command text.
// commandFromText 从已渲染 shell_command 文本中提取命令头。
function commandFromText(source: string): string | null {
  const match = source.match(/^Command:\s*(.+)$/im);
  return match?.[1]?.trim() || null;
}

// exitCodeFromText extracts the exit code from rendered shell_command text.
// exitCodeFromText 从已渲染 shell_command 文本中提取退出码。
function exitCodeFromText(source: string): number | null {
  const match = source.match(/^Exit code:\s*(-?\d+)$/im);
  return match ? numberValue(match[1]) : null;
}

// sectionFromText extracts a bracketed output section until the next bracketed section.
// sectionFromText 提取方括号输出区段直到下一个方括号区段。
function sectionFromText(source: string, marker: "[stdout]" | "[stderr]"): string {
  const lines = source.split(/\r?\n/);
  const markerIndex = lines.findIndex((line) => line.trim() === marker);
  if (markerIndex < 0) {
    return "";
  }
  const collected: string[] = [];
  for (const line of lines.slice(markerIndex + 1)) {
    if (/^\[(stdout|stderr)\]\s*$/.test(line.trim())) {
      break;
    }
    collected.push(line);
  }
  return collected.join("\n").trim();
}

// metadataString returns the first non-empty string at any candidate metadata path.
// metadataString 返回任一候选 metadata 路径上的首个非空字符串。
function metadataString(metadata: unknown, paths: string[]): string | null {
  for (const path of paths) {
    const value = stringValue(metadataValue(metadata, [path]));
    if (value) {
      return value;
    }
  }
  return null;
}

// metadataValue returns the value at the first existing dotted path.
// metadataValue 返回首个存在的点分路径值。
function metadataValue(metadata: unknown, paths: string[]): unknown {
  for (const candidate of paths) {
    const segments = candidate.split(".");
    let current: unknown = metadata;
    let found = true;
    for (const segment of segments) {
      if (!current || typeof current !== "object" || !(segment in current)) {
        found = false;
        break;
      }
      current = (current as Record<string, unknown>)[segment];
    }
    if (found) {
      return current;
    }
  }
  return null;
}

// objectValue narrows unknown values to plain records.
// objectValue 将未知值收窄为普通对象记录。
function objectValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

// stringValue returns trimmed strings and null for blank or non-string values.
// stringValue 返回裁剪后的字符串，空白或非字符串值返回 null。
function stringValue(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

// numberValue normalizes numeric JSON values or numeric strings.
// numberValue 规范化 JSON 数值或数字字符串。
function numberValue(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return Math.trunc(value);
  }
  if (typeof value === "string" && /^-?\d+$/.test(value.trim())) {
    return Number.parseInt(value.trim(), 10);
  }
  return null;
}

// compactHash creates a small deterministic hash for rows without backend ids.
// compactHash 为没有后端标识的行创建短小稳定哈希。
function compactHash(source: string): string {
  let hash = 0;
  for (let index = 0; index < source.length; index += 1) {
    hash = Math.imul(31, hash) + source.charCodeAt(index) | 0;
  }
  return Math.abs(hash).toString(36);
}
