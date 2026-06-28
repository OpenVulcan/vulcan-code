import type { ConversationDetailRef, ToolDetailPayload } from "@/components/conversation/conversation-detail";
import { shortenTerminalSessionId, terminalSummaryById, terminalVisualState } from "@/components/conversation/conversation-terminal-model";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type { CommandExecutionPresentation, ToolPresentationDisplayState, ToolPresentationResolveOptions } from "./types";
import { argumentsFromMetadata, arrayValue, compactHash, fixedHostKind, fixedHostPayload, fixedToolName, metadataString, metadataValue, normalizeToolName, numberValue, objectValue, stringValue, textValue } from "./presentation-value-readers";

// hydrateCommandExecutionPresentation merges one hidden tool detail payload into an existing command presentation.
// hydrateCommandExecutionPresentation 将单条隐藏工具详情载荷合并进已有命令展示模型。
export function hydrateCommandExecutionPresentation(
  presentation: CommandExecutionPresentation,
  payload: ToolDetailPayload | null,
): CommandExecutionPresentation {
  if (!payload) {
    return presentation;
  }
  const displayKind = metadataString(payload.metadata, ["display.kind"]);
  if (presentation.commandSessionKind === "terminal" || displayKind === "terminal_session") {
    const terminalDisplay = terminalSessionDisplayFromText(payload.body);
    return terminalDisplay ? mergeTerminalSessionDisplay(presentation, terminalDisplay) : presentation;
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

// TerminalSessionDisplay stores normalized terminal-session metadata for command presentation.
// TerminalSessionDisplay 保存命令展示使用的归一化终端会话元数据。
interface TerminalSessionDisplay {
  // Parsed action name.
  // 解析出的动作名称。
  action: string | null;
  // Parsed terminal-session id.
  // 解析出的终端会话标识。
  terminalSessionId: string | null;
  // Parsed owner session id.
  // 解析出的归属会话标识。
  ownerSessionId: string | null;
  // Parsed terminal display name.
  // 解析出的终端显示名称。
  name: string | null;
  // Parsed start command.
  // 解析出的启动命令。
  command: string | null;
  // Parsed working directory.
  // 解析出的工作目录。
  cwd: string | null;
  // Parsed shell label.
  // 解析出的 shell 标签。
  shellLabel: string | null;
  // Parsed input label summary.
  // 解析出的输入标签摘要。
  inputLabels: string | null;
  // Parsed transaction exit code.
  // 解析出的事务退出码。
  transactionExitCode: number | null;
  // Parsed stdout text.
  // 解析出的 stdout 文本。
  stdout: string;
  // Parsed stderr text.
  // 解析出的 stderr 文本。
  stderr: string;
}

// firstTerminalSessionDisplay returns the first structured terminal_session display found on visible blocks.
// firstTerminalSessionDisplay 返回可见块上找到的首个结构化 terminal_session 展示。
function firstTerminalSessionDisplay(blocks: ToolExecutionConversationBlock[]): TerminalSessionDisplay | null {
  const payload = fixedHostPayload(blocks);
  if (payload) {
    const display = terminalSessionDisplayFromPayload(payload);
    if (display) {
      return display;
    }
  }
  for (const block of blocks) {
    const display = terminalSessionDisplayFromArguments(argumentsFromMetadata(block.rawMetadata));
    if (display) {
      return display;
    }
  }
  for (const block of blocks) {
    const display = terminalSessionDisplayFromText(block.body);
    if (display) {
      return display;
    }
  }
  return null;
}

// terminalSessionDisplayFromPayload reads terminal-session display data from host_result.payload.
// terminalSessionDisplayFromPayload 从 host_result.payload 读取终端会话展示数据。
function terminalSessionDisplayFromPayload(payload: Record<string, unknown>): TerminalSessionDisplay | null {
  const action = stringValue(payload.action);
  const terminalSessionId = stringValue(payload.terminal_session_id ?? payload.terminalSessionId ?? payload.command_session_id);
  const output = terminalSessionOutputFromEvents(arrayValue(payload.events));
  const transaction = objectValue(payload.transaction);
  const transactionExitCode = numberValue(transaction?.exit_code ?? transaction?.exitCode);
  const inputLabels = stringValue(payload.input_labels ?? payload.inputLabels);
  if (!action && !terminalSessionId && !output.stdout && !output.stderr) {
    return null;
  }
  return {
    action,
    command: stringValue(payload.command),
    cwd: stringValue(payload.cwd),
    inputLabels,
    name: stringValue(payload.name),
    ownerSessionId: stringValue(payload.owner_session_id ?? payload.ownerSessionId),
    shellLabel: stringValue(payload.shell),
    stderr: output.stderr,
    stdout: output.stdout,
    terminalSessionId,
    transactionExitCode,
  };
}

// terminalSessionDisplayFromArguments reads fixed terminal_session call arguments before detail hydration.
// terminalSessionDisplayFromArguments 在详情水合前读取固定 terminal_session 调用参数。
function terminalSessionDisplayFromArguments(value: unknown): TerminalSessionDisplay | null {
  const args = objectValue(value);
  if (!args) {
    return null;
  }
  const action = stringValue(args.action)?.toLowerCase() ?? null;
  const terminalSessionId = stringValue(args.terminal_session_id);
  const inputLabels = terminalSessionInputLabelsFromArguments(arrayValue(args.inputs));
  const command = stringValue(args.command);
  const name = stringValue(args.name);
  if (!action && !terminalSessionId && !command && !name && !inputLabels) {
    return null;
  }
  return {
    action,
    command,
    cwd: stringValue(args.cwd),
    inputLabels,
    name,
    ownerSessionId: null,
    shellLabel: stringValue(args.shell),
    stderr: "",
    stdout: "",
    terminalSessionId,
    transactionExitCode: null,
  };
}

// terminalSessionInputLabelsFromArguments builds a compact label from fixed terminal_session input items.
// terminalSessionInputLabelsFromArguments 从固定 terminal_session 输入项构建紧凑标签。
function terminalSessionInputLabelsFromArguments(inputs: unknown[]): string | null {
  const labels = inputs
    .map(terminalSessionInputLabelFromArgument)
    .filter(Boolean) as string[];
  if (!labels.length) {
    return null;
  }
  const visibleLabels = labels.slice(0, 3).join(" + ");
  return labels.length > 3 ? `${visibleLabels} 等 ${labels.length} 项` : visibleLabels;
}

// terminalSessionInputLabelFromArgument formats one fixed terminal_session input item.
// terminalSessionInputLabelFromArgument 格式化单条固定 terminal_session 输入项。
function terminalSessionInputLabelFromArgument(value: unknown): string | null {
  const input = objectValue(value);
  if (!input) {
    return null;
  }
  const kind = stringValue(input.kind);
  if (kind === "text") {
    return compactTerminalSessionInputText(stringValue(input.text), "输入");
  }
  if (kind === "paste") {
    return compactTerminalSessionInputText(stringValue(input.text), "粘贴");
  }
  if (kind === "key") {
    return stringValue(input.key);
  }
  if (kind === "chord") {
    const key = stringValue(input.key);
    if (!key) {
      return null;
    }
    const modifiers = [
      input.ctrl === true ? "Ctrl" : null,
      input.alt === true ? "Alt" : null,
      input.shift === true ? "Shift" : null,
      input.meta === true ? "Meta" : null,
      key,
    ].filter(Boolean);
    return modifiers.join("+");
  }
  return null;
}

// compactTerminalSessionInputText keeps terminal input summaries readable in collapsed rows.
// compactTerminalSessionInputText 让终端输入摘要在折叠行中保持可读。
function compactTerminalSessionInputText(value: string | null, fallback: string): string {
  const compact = (value ?? "").replace(/\s+/g, " ").trim();
  if (!compact) {
    return fallback;
  }
  return compact.length > 42 ? `${compact.slice(0, 39)}...` : compact;
}

// terminalSessionDisplayFromText parses the host-rendered terminal_session output format.
// terminalSessionDisplayFromText 解析宿主渲染的 terminal_session 输出格式。
function terminalSessionDisplayFromText(source: string): TerminalSessionDisplay | null {
  const action = lineValueFromText(source, "Action")?.toLowerCase() ?? null;
  const terminalSessionId = lineValueFromText(source, "Terminal session");
  const ownerSessionId = lineValueFromText(source, "Owner session");
  const name = lineValueFromText(source, "Name");
  const command = lineValueFromText(source, "Command");
  const cwd = lineValueFromText(source, "Working directory");
  const shellLabel = lineValueFromText(source, "Shell");
  const transactionExitCode = numberValue(lineValueFromText(source, "Transaction exit code"));
  const output = streamChunksFromText(source);
  if (!action && !terminalSessionId && !command && !output.stdout.length && !output.stderr.length && transactionExitCode === null) {
    return null;
  }
  return {
    action,
    command,
    cwd,
    inputLabels: lineValueFromText(source, "Inputs"),
    name,
    ownerSessionId,
    shellLabel,
    stderr: output.stderr.join("\n").trim(),
    stdout: output.stdout.join("\n").trim(),
    terminalSessionId,
    transactionExitCode,
  };
}

// terminalSessionOutputFromEvents splits terminal-session output events by stream.
// terminalSessionOutputFromEvents 按输出流拆分终端会话输出事件。
function terminalSessionOutputFromEvents(events: unknown[]): { stdout: string; stderr: string } {
  const stdout: string[] = [];
  const stderr: string[] = [];
  for (const event of events) {
    const record = objectValue(event);
    if (!record) {
      continue;
    }
    const text = textValue(record.text) ?? "";
    if (!text) {
      continue;
    }
    if (stringValue(record.stream) === "stderr") {
      stderr.push(text);
    } else {
      stdout.push(text);
    }
  }
  return {
    stderr: stderr.join("").trim(),
    stdout: stdout.join("").trim(),
  };
}

// terminalSessionDisplayCommand builds a per-action terminal_session summary label.
// terminalSessionDisplayCommand 构建按动作区分的 terminal_session 摘要文案。
function terminalSessionDisplayCommand(
  action: string | null,
  command: string | null,
  terminalSessionId: string | null,
  inputLabels: string | null,
  name: string | null,
): string | null {
  const sessionLabel = terminalSessionConversationLabel(name, terminalSessionId);
  if (action === "start") {
    return name || terminalSessionId ? `启动${sessionLabel}` : command ? `启动终端 ${command}` : "启动终端会话";
  }
  if (action === "send") {
    return inputLabels ? `向${sessionLabel}发送 ${inputLabels}` : `发送到${sessionLabel}`;
  }
  if (action === "list") {
    return "列出终端会话";
  }
  if (action === "read") {
    return `读取${sessionLabel}`;
  }
  if (action === "snapshot") {
    return `检查${sessionLabel}`;
  }
  if (action === "stop") {
    return `停止${sessionLabel}`;
  }
  return sessionLabel;
}

// terminalSessionConversationLabel formats the stable visible terminal conversation name.
// terminalSessionConversationLabel 格式化稳定可见的终端会话名称。
function terminalSessionConversationLabel(name: string | null, terminalSessionId: string | null): string {
  const targetLabel = (name ?? "").trim() || (terminalSessionId ? shortenTerminalSessionId(terminalSessionId) : "");
  return targetLabel ? `终端[${targetLabel}]的会话` : "终端会话";
}

// resolveCommandExecutionPresentation recognizes one-shot shell_command rows and excludes command sessions.
// resolveCommandExecutionPresentation 识别一次性 shell_command 行，并排除命令会话。
export function resolveCommandExecutionPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  detailRef?: ConversationDetailRef,
  options: ToolPresentationResolveOptions = {},
): CommandExecutionPresentation | null {
  const toolName = fixedToolName(blocks);
  const normalizedToolName = toolName ? normalizeToolName(toolName) : null;
  if (normalizedToolName === "terminal_session" || fixedHostKind(blocks) === "terminal_session") {
    const terminalDisplay = firstTerminalSessionDisplay(blocks);
    const action = terminalDisplay?.action ?? "session";
    const effectiveCommandSessionId = terminalDisplay?.terminalSessionId ?? blocks.find((block) => block.commandSessionId)?.commandSessionId ?? null;
    const liveCommandSession = effectiveCommandSessionId
      ? (options.commandSessionById ?? terminalSummaryById(options.commandSessions ?? [])).get(effectiveCommandSessionId) ?? null
      : null;
    const displayName = terminalDisplay?.name ?? liveCommandSession?.name ?? null;
    const command = terminalSessionDisplayCommand(action, terminalDisplay?.command ?? null, effectiveCommandSessionId, terminalDisplay?.inputLabels ?? null, displayName)
      ?? blocks.find((block) => block.summary)?.summary
      ?? `terminal_session ${action}`;
    const exitCode = terminalDisplay?.transactionExitCode ?? liveCommandSession?.exitCode ?? null;
    const terminalState = liveCommandSession
      ? terminalVisualState(liveCommandSession.status, exitCode, Boolean(liveCommandSession.lost))
      : effectiveCommandSessionId
        ? "lost"
        : undefined;
    return {
      command,
      commandSessionAction: action,
      commandSessionId: effectiveCommandSessionId,
      commandSessionInput: terminalDisplay?.inputLabels ?? null,
      commandSessionKind: "terminal",
      cwd: liveCommandSession?.cwd ?? terminalDisplay?.cwd ?? null,
      detailRef,
      exitCode,
      failureReason: liveCommandSession?.failureReason ?? null,
      id: commandPresentationId(blocks, effectiveCommandSessionId ? `terminal:${effectiveCommandSessionId}:${action}` : command),
      kind: "command_execution",
      lost: effectiveCommandSessionId ? !liveCommandSession : false,
      ownerSessionId: liveCommandSession?.ownerSessionId ?? terminalDisplay?.ownerSessionId ?? null,
      shellLabel: liveCommandSession?.shellLabel ?? terminalDisplay?.shellLabel ?? null,
      state: displayState,
      stderr: terminalDisplay?.stderr ?? liveCommandSession?.failureReason ?? "",
      stdout: terminalDisplay?.stdout ?? "",
      terminalState,
    };
  }
  if (normalizedToolName !== "shell_command" && fixedHostKind(blocks) !== "command_run") {
    return null;
  }

  const display = firstShellCommandDisplay(blocks);
  const command = display?.command
    ?? blocks.map((block) => shellCommandArgumentFromMetadata(block.rawMetadata)).find(Boolean)
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

// shortenCommandSessionId keeps long command-session ids readable in compact rows.
// shortenCommandSessionId 让较长命令会话标识在紧凑行中保持可读。
function shortenCommandSessionId(value: string): string {
  const trimmed = value.trim();
  return shortenTerminalSessionId(trimmed);
}

// streamChunksFromText parses `[stdout #n]` and `[stderr #n]` sections from rendered terminal reads.
// streamChunksFromText 从已渲染终端读取文本解析 `[stdout #n]` 与 `[stderr #n]` 区段。
function streamChunksFromText(source: string): { stdout: string[]; stderr: string[] } {
  const stdout: string[] = [];
  const stderr: string[] = [];
  const lines = source.split(/\r?\n/);
  let current: "stdout" | "stderr" | null = null;
  let buffer: string[] = [];
  const flush = () => {
    const text = buffer.join("\n").trimEnd();
    if (text && current === "stdout") {
      stdout.push(text);
    } else if (text && current === "stderr") {
      stderr.push(text);
    }
    buffer = [];
  };
  for (const line of lines) {
    const marker = line.match(/^\[(stdout|stderr)\s+#\d+\]\s*$/i);
    if (marker) {
      flush();
      current = marker[1].toLowerCase() as "stdout" | "stderr";
      continue;
    }
    if (/^\[[a-z]+\s+#\d+\]\s*$/i.test(line)) {
      flush();
      current = null;
      continue;
    }
    if (current) {
      buffer.push(line);
    }
  }
  flush();
  return { stdout, stderr };
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

// mergeTerminalSessionDisplay applies terminal_session detail fields without falling back to legacy shell parsing.
// mergeTerminalSessionDisplay 合并 terminal_session 详情字段，避免退回旧 shell 解析。
function mergeTerminalSessionDisplay(
  presentation: CommandExecutionPresentation,
  display: TerminalSessionDisplay,
): CommandExecutionPresentation {
  const action = display.action ?? presentation.commandSessionAction ?? "session";
  const commandSessionId = display.terminalSessionId ?? presentation.commandSessionId ?? null;
  const command = terminalSessionDisplayCommand(action, display.command, commandSessionId, display.inputLabels ?? presentation.commandSessionInput ?? null, display.name)
    ?? presentation.command;
  return {
    ...presentation,
    command,
    commandSessionAction: action,
    commandSessionId,
    commandSessionInput: display.inputLabels ?? presentation.commandSessionInput,
    commandSessionKind: "terminal",
    cwd: display.cwd ?? presentation.cwd,
    exitCode: display.transactionExitCode ?? presentation.exitCode,
    ownerSessionId: display.ownerSessionId ?? presentation.ownerSessionId,
    shellLabel: display.shellLabel ?? presentation.shellLabel,
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
  const display = objectValue(metadataValue(metadata, ["display"]));
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

// shellCommandArgumentFromMetadata reads shell_command from the fixed ToolStarted arguments field.
// shellCommandArgumentFromMetadata 从固定 ToolStarted arguments 字段读取 shell_command 命令。
function shellCommandArgumentFromMetadata(metadata: unknown): string | null {
  return metadataString(metadata, ["arguments.command"]);
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

// lineValueFromText extracts one stable `Label: value` line from host-rendered text.
// lineValueFromText 从宿主渲染文本中提取一条稳定的 `Label: value` 行。
function lineValueFromText(source: string, label: string): string | null {
  const escaped = label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = source.match(new RegExp(`^${escaped}:\\s*(.+?)\\s*$`, "im"));
  return match?.[1]?.trim() || null;
}
