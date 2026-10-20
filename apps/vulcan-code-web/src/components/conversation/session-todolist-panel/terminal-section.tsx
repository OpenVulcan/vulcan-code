import React from "react";
import { runConversationControlAction } from "../conversation-control-feedback";
import { Square, TerminalSquare } from "lucide-react";
import {
  terminalStateTone,
  type TerminalSessionViewModel,
  type TerminalVisualState,
} from "@/components/conversation/conversation-terminal-model";
import { TerminalStopConfirmDialog } from "@/components/conversation/terminal-stop-confirm-dialog";
import { ConversationReadOnlyTerminalSurface } from "@/components/conversation/terminal/conversation-read-only-terminal-surface";
import { IconButton } from "@/ui-kit/primitives/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui-kit/primitives/tooltip";
import { useI18n, type TranslationKey } from "@/i18n";
import type { CommandSessionSummary } from "@/types";

// ConversationSessionTerminalSection renders AI-owned terminal sessions in the session tool panel.
// ConversationSessionTerminalSection 在会话工具面板中渲染 AI 持有的终端会话。
export function ConversationSessionTerminalSection(props: {
  // Normalized terminal session rows.
  // 归一化终端会话行。
  terminalSessions: TerminalSessionViewModel[];
  // Original summaries keyed by terminal id.
  // 按终端标识索引的原始摘要。
  summariesById: Map<string, CommandSessionSummary>;
  // Force-stops one live terminal session.
  // 强制停止一个实时终端会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
}) {
  // t resolves active-locale interface copy.
  // t 解析当前语言的界面文案。
  const { t } = useI18n();
  return (
    <section className="vk-stools-sec" data-kind="terminal">
      <div className="vk-stools-title">
        <TerminalSquare size={13} />
        <span>{t("sessionTools.terminal")}</span>
        <b>{props.terminalSessions.length}</b>
        <small className="vk-stools-aside">{t("sessionTools.aiCommands")}</small>
      </div>
      <div className="vk-stools-list">
        {props.terminalSessions.map((session) => (
          <ConversationSessionTerminalRow
            key={session.id}
            session={session}
            summary={props.summariesById.get(session.id) ?? null}
            onStopCommandSession={props.onStopCommandSession}
          />
        ))}
      </div>
    </section>
  );
}

// ConversationSessionTerminalRow renders one AI terminal as a single pressable row: its state, command and state
// label open the read-only screen, and only a live terminal adds a stop button at the end.
// ConversationSessionTerminalRow 将一个 AI 终端渲染为可整行按下的行：状态、命令与状态标签都会打开只读屏幕，
// 只有仍在运行的终端才在行尾附加停止按钮。
function ConversationSessionTerminalRow(props: {
  // Normalized terminal session row.
  // 归一化终端会话行。
  session: TerminalSessionViewModel;
  // Original summary carrying owner metadata.
  // 携带归属元数据的原始摘要。
  summary: CommandSessionSummary | null;
  // Force-stops one live terminal session.
  // 强制停止一个实时终端会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
}) {
  // t resolves active-locale interface copy.
  // t 解析当前语言的界面文案。
  const { t } = useI18n();
  // stopping guards duplicate force-stop requests.
  // stopping 防止重复强制停止请求。
  const [stopping, setStopping] = React.useState(false);
  // stopInFlight prevents duplicate stop requests before React renders the disabled controls.
  // stopInFlight 防止 React 渲染禁用控件前重复提交停止请求。
  const stopInFlight = React.useRef(false);
  // confirmOpen controls the destructive stop confirmation.
  // confirmOpen 控制破坏性停止确认。
  const [confirmOpen, setConfirmOpen] = React.useState(false);
  // screenOpen is the sole display-attachment visibility source.
  // screenOpen 是显示订阅可见性的唯一来源。
  const [screenOpen, setScreenOpen] = React.useState(false);
  // screenButtonRef (the row itself) is the macOS-style minimize and restore animation anchor for this terminal.
  // screenButtonRef（即该行本身）是当前终端 macOS 风格收回与显示动画的锚点。
  const screenButtonRef = React.useRef<HTMLButtonElement | null>(null);
  const handleStopRequest = React.useCallback(() => {
    if (props.session.canStop && !stopping) setConfirmOpen(true);
  }, [props.session.canStop, stopping]);
  const handleConfirmStop = React.useCallback(async () => {
    if (!props.session.canStop || stopInFlight.current) return;
    stopInFlight.current = true;
    setStopping(true);
    try {
      if (await runConversationControlAction(
        () => props.onStopCommandSession(props.session.id, props.summary?.ownerSessionId ?? null),
        t("conversation.control.actionFailed"),
      )) setConfirmOpen(false);
    } finally {
      stopInFlight.current = false;
      setStopping(false);
    }
  }, [props, t]);
  const tooltipText = terminalSessionTooltipText(props.session, props.summary, t);
  const stateLabel = localizedTerminalStateLabel(props.session.state, t);
  return (
    <div className="vk-stools-item" data-row="terminal" data-terminal-session={props.session.id}>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            aria-label={`${t("terminalSession.openScreen")} · ${props.session.name}`}
            className="vk-stools-term"
            data-action="screen"
            disabled={!props.summary}
            onClick={() => setScreenOpen(true)}
            ref={screenButtonRef}
            type="button"
          >
            <TerminalSquare
              className="vk-term-mark"
              data-state={props.session.state}
              data-tone={terminalStateTone(props.session.state)}
              size={14}
            />
            <code>{props.session.name}</code>
            <small data-tone={terminalStateTone(props.session.state)}>{stateLabel}</small>
          </button>
        </TooltipTrigger>
        <TooltipContent className="vk-stools-tip" side="left" sideOffset={8}>
          <pre>{props.summary ? tooltipText : `${tooltipText}\n${t("terminalSession.noScreen")}`}</pre>
        </TooltipContent>
      </Tooltip>
      {props.session.canStop ? (
        <IconButton
          aria-label={t("terminalSession.forceStopSession")}
          data-action="stop"
          disabled={stopping}
          onClick={handleStopRequest}
          size="xs"
          title={t("terminalSession.forceStop")}
        >
          <Square fill="currentColor" size={10} strokeWidth={0} />
        </IconButton>
      ) : null}
      <TerminalStopConfirmDialog
        command={props.session.command}
        open={confirmOpen}
        sessionName={props.session.name}
        stopping={stopping}
        onConfirm={handleConfirmStop}
        onOpenChange={(open) => {
          if (!stopping) setConfirmOpen(open);
        }}
      />
      <ConversationReadOnlyTerminalSurface
        minimizeAnchorRef={screenButtonRef}
        open={screenOpen}
        session={props.session}
        summary={props.summary}
        onOpenChange={setScreenOpen}
      />
    </div>
  );
}

// terminalSessionTooltipText builds complete hover diagnostics for one compact terminal row.
// terminalSessionTooltipText 为一个紧凑终端行构建完整悬浮诊断。
function terminalSessionTooltipText(
  session: TerminalSessionViewModel,
  summary: CommandSessionSummary | null,
  t: (key: TranslationKey) => string,
): string {
  const separator = t("global.labelSeparator");
  const lines = [
    `${t("terminalSession.tooltip.status")}${separator}${localizedTerminalStateLabel(session.state, t)}`,
    `${t("terminalSession.tooltip.command")}${separator}${session.command}`,
  ];
  if (summary?.cwd) lines.push(`${t("terminalSession.tooltip.cwd")}${separator}${summary.cwd}`);
  if (summary?.shellLabel) lines.push(`${t("terminalSession.tooltip.shell")}${separator}${summary.shellLabel}`);
  if (summary?.backend) lines.push(`${t("terminalSession.tooltip.backend")}${separator}${summary.backend}`);
  if (typeof session.exitCode === "number") lines.push(`${t("terminalSession.tooltip.exitCode")}${separator}${session.exitCode}`);
  if (summary?.failureReason) lines.push(`${t("terminalSession.tooltip.error")}${separator}${summary.failureReason}`);
  if (session.preview) lines.push(`${t("terminalSession.tooltip.output")}${separator}${session.preview}`);
  lines.push(`${t("terminalSession.tooltip.session")}${separator}${session.id}`);
  return lines.join("\n");
}

// localizedTerminalStateLabel returns the active-locale lifecycle label.
// localizedTerminalStateLabel 返回当前语言的生命周期标签。
function localizedTerminalStateLabel(
  state: TerminalVisualState,
  t: (key: TranslationKey) => string,
): string {
  if (state === "running") return t("terminalState.running");
  if (state === "waiting") return t("terminalState.waiting");
  if (state === "success") return t("terminalState.success");
  if (state === "failed") return t("terminalState.failed");
  if (state === "stopped") return t("terminalState.stopped");
  if (state === "lost") return t("terminalState.lost");
  return t("terminalState.unknown");
}
