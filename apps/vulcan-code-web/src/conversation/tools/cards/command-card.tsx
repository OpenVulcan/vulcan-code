// Command cards answer three questions at a glance: what ran, how it ended, how long it took. The body is the
// terminal transcript, ending with where it ran.
// 命令卡一眼回答三个问题：运行了什么、如何结束、耗时多久。正文是终端记录，末尾注明运行位置。
import * as React from "react";
import { SquareTerminal } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import { ToolCardSessionContext, ToolFrame, type ToolCardProps } from "../tool-card";
import { listOf, numberOf, objectOf, textOf, type ToolCall } from "../tool-call";
import { TerminalView } from "../views/terminal-view";
import { useShellCommandLiveOutput, type ShellCommandLiveTarget } from "./use-shell-command-live-output";

/** Renders one shell_command call.
 * 渲染一次 shell_command 调用。 */
export function ShellCommandCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const run = call.host?.kind === "command_run" ? call.host.payload : null;
  const command = textOf(run?.command) ?? textOf(call.input.command) ?? "";
  const hook = objectOf(run?.hook_transform);
  const effective = textOf(hook?.effective_command);
  const exitCode = numberOf(run?.exit_code) ?? exitCodeFromText(call.modelOutput);
  const output = call.modelOutput ? commandOutputFromText(call.modelOutput) : "";
  const failure = textOf(run?.failure_reason);
  const badges = listOf(hook?.contributions).map(objectOf).filter((item) => textOf(item?.badge));
  // A running call without a result opens onto its recent output, read from Core only while the body is open.
  // 尚无结果的运行中调用展开后显示其最近输出，仅在正文展开期间从 Core 读取。
  const sessionId = React.useContext(ToolCardSessionContext);
  const liveTarget = call.state === "running" && !output && sessionId
    ? { apiCallId: call.event.api_call_id, sessionId, toolCallId: call.event.tool_call_id }
    : null;
  return (
    <ToolFrame
      {...props}
      icon={SquareTerminal}
      meta={(
        <>
          {badges.map((badge, index) => (
            <span className="vk-badge" key={index} title={`${textOf(badge?.label) ?? ""}: ${textOf(badge?.value) ?? ""}`}>{textOf(badge?.badge)}</span>
          ))}
          {exitCode !== null && exitCode !== 0 ? <span className="vk-del">{t("toolCard.exitCode").replace("{code}", String(exitCode))}</span> : null}
        </>
      )}
      state={call.state}
      target={command}
      targetTitle={command}
      verb={t("toolCard.verb.runCommand")}
    >
      {liveTarget ? (
        <LiveCommandOutput command={command} cwd={textOf(run?.cwd) ?? textOf(call.input.cwd)} target={liveTarget} />
      ) : call.state === "running" && !output ? null : (
        <>
          <TerminalView
            cwd={textOf(run?.cwd)}
            emptyText={t("toolPresentation.output.empty")}
            prompt={`$ ${command}${effective && effective !== command ? `\n→ ${effective}` : ""}`}
            text={output}
          />
          {failure ? <div className="vk-tool-note" data-tone="">{failure}</div> : null}
          <RunLocation cwd={textOf(run?.cwd)} shell={textOf(run?.shell)} />
        </>
      )}
    </ToolFrame>
  );
}

// LiveCommandOutput shows the most recent lines of a command that is still running. It mounts only while the card is
// open, so a closed card never asks Core for anything.
// LiveCommandOutput 显示仍在运行的命令的最近若干行。它只在卡片展开时挂载，因此收起的卡片不会向 Core 请求任何内容。
function LiveCommandOutput(props: { command: string; cwd: string | null; target: ShellCommandLiveTarget }) {
  const { t } = useI18n();
  const live = useShellCommandLiveOutput(props.target);
  const note = !live
    ? null
    : live.running
      ? t("toolCard.liveOutput.recent").replace("{count}", String(live.lineLimit))
      : t("toolCard.liveOutput.ended");
  return (
    <>
      <TerminalView
        cwd={props.cwd}
        emptyText={t(live ? "toolCard.liveOutput.none" : "toolCard.liveOutput.reading")}
        prompt={`$ ${props.command}`}
        text={live?.lines.join("\n") ?? ""}
      />
      {note ? <div className="vk-tool-foot"><span>{note}</span></div> : null}
    </>
  );
}

// TERMINAL_VERBS names each terminal_session action.
// TERMINAL_VERBS 命名每个 terminal_session 动作。
const TERMINAL_VERBS: Record<string, TranslationKey> = {
  list: "toolCard.terminal.list",
  read: "toolCard.terminal.read",
  send: "toolCard.terminal.send",
  snapshot: "toolCard.terminal.snapshot",
  start: "toolCard.terminal.start",
  stop: "toolCard.terminal.stop",
};

// TERMINAL_STATUS names the live status a terminal reported with this action.
// TERMINAL_STATUS 命名终端随本次动作上报的实时状态。
const TERMINAL_STATUS: Record<string, TranslationKey> = {
  completed: "terminalState.success",
  failed: "terminalState.failed",
  running: "terminalState.running",
  stopped: "terminalState.stopped",
};

/** Renders one terminal_session call: the action on a named persistent terminal.
 * 渲染一次 terminal_session 调用：对具名持久终端的动作。 */
export function TerminalSessionCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const session = call.host?.kind === "terminal_session" ? call.host.payload : null;
  const action = textOf(session?.action) ?? textOf(call.input.action) ?? "";
  const name = textOf(session?.name) ?? textOf(call.input.name);
  const id = textOf(session?.terminal_session_id) ?? textOf(call.input.terminal_session_id);
  const label = name ?? (id ? id.slice(0, 8) : "");
  const startCommand = textOf(session?.command) ?? textOf(call.input.command);
  const sent = textOf(session?.input_labels) ?? inputLabels(call.input.inputs);
  const status = textOf(session?.status);
  const exitCode = numberOf(session?.exit_code);
  const output = terminalOutput(session, action);
  const target = action === "start" ? startCommand ?? label : action === "send" ? sent ?? label : action === "list" ? "" : label;
  const prompt = action === "start" && startCommand ? `$ ${startCommand}` : action === "send" && sent ? `> ${sent}` : undefined;
  return (
    <ToolFrame
      {...props}
      icon={SquareTerminal}
      meta={(
        <>
          {target !== label && label ? <span>{label}</span> : null}
          {status && TERMINAL_STATUS[status] ? <span>{t(TERMINAL_STATUS[status])}</span> : null}
          {exitCode !== null && exitCode !== 0 ? <span className="vk-del">{t("toolCard.exitCode").replace("{code}", String(exitCode))}</span> : null}
        </>
      )}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={t(TERMINAL_VERBS[action] ?? "toolCard.terminal.session")}
    >
      {session || call.modelOutput ? (
        <>
          <TerminalView emptyText={t("toolPresentation.output.empty")} prompt={prompt} text={session ? output : call.modelOutput ?? ""} />
          {textOf(session?.failure_reason) ? <div className="vk-tool-note" data-tone="">{textOf(session?.failure_reason)}</div> : null}
          <RunLocation cwd={textOf(session?.cwd)} shell={textOf(session?.shell)} />
        </>
      ) : null}
    </ToolFrame>
  );
}

// RunLocation is the footer naming the working directory and shell.
// RunLocation 是注明工作目录与 shell 的页脚。
function RunLocation(props: { cwd: string | null; shell: string | null }) {
  if (!props.cwd && !props.shell) return null;
  return (
    <div className="vk-tool-foot">
      <span className="vk-truncate" title={props.cwd ?? undefined}>{props.cwd}</span>
      {props.shell ? <span>{props.shell}</span> : null}
    </div>
  );
}

// terminalOutput returns what the action showed: the screen for a snapshot, the new output otherwise.
// terminalOutput 返回动作展示的内容：snapshot 为屏幕，其余为新增输出。
function terminalOutput(session: Record<string, unknown> | null, action: string): string {
  if (!session) return "";
  const screen = textOf(session.screen_text);
  if (action === "snapshot" && screen) return screen;
  const events = listOf(session.events).map(objectOf);
  const text = events.map((event) => textOf(event?.text) ?? "").join("");
  return text || screen || "";
}

// inputLabels summarizes requested inputs before the host reports its own labels.
// inputLabels 在宿主报告自身标签前概括请求的输入。
function inputLabels(value: unknown): string | null {
  const labels = listOf(value).map(objectOf).map((input) => textOf(input?.text)?.trim() ?? textOf(input?.key) ?? "").filter(Boolean);
  return labels.length ? labels.join(", ") : null;
}

// exitCodeFromText reads "Exit code: N" from the stable shell_command model output.
// exitCodeFromText 从稳定的 shell_command 模型输出读取 "Exit code: N"。
function exitCodeFromText(text: string | null): number | null {
  const match = text?.match(/^Exit code:\s*(-?\d+)\s*$/m);
  return match ? Number(match[1]) : null;
}

// commandOutputFromText joins the [stdout] and [stderr] sections of the stable shell_command model output.
// commandOutputFromText 合并稳定 shell_command 模型输出中的 [stdout] 与 [stderr] 区段。
export function commandOutputFromText(text: string): string {
  const sections: string[] = [];
  let current: string[] | null = null;
  for (const line of text.replace(/\r\n?/g, "\n").split("\n")) {
    if (/^\[(stdout|stderr)\]\s*$/.test(line)) {
      current = [];
      sections.push("");
      continue;
    }
    if (current) sections[sections.length - 1] += (sections[sections.length - 1] ? "\n" : "") + line;
  }
  return sections.map((section) => section.trim()).filter(Boolean).join("\n");
}

// toolCallIsShellCommand recognizes shell_command by name or by its host result.
// toolCallIsShellCommand 通过名称或宿主结果识别 shell_command。
export function toolCallIsShellCommand(call: ToolCall): boolean {
  return call.name === "shell_command" || call.host?.kind === "command_run";
}
