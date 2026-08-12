import React from "react";
import { Maximize2, Square, TerminalSquare } from "lucide-react";
import {
  terminalStateClass,
  type TerminalSessionViewModel,
  type TerminalVisualState,
} from "@/components/conversation/conversation-terminal-model";
import { TerminalStopConfirmDialog } from "@/components/conversation/terminal-stop-confirm-dialog";
import { ConversationReadOnlyTerminalSurface } from "@/components/conversation/terminal/conversation-read-only-terminal-surface";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
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
    <section className="conversation-session-tool-section conversation-session-terminal-section">
      <div className="conversation-session-tool-section-title">
        <span className="conversation-session-tool-section-title-main">
          <span>{t("sessionTools.terminal")}</span>
          <strong>{props.terminalSessions.length}</strong>
        </span>
      </div>
      <div className="workspace-tool-panel-menu-list conversation-session-terminal-list">
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

// ConversationSessionTerminalRow renders one AI terminal row and its read-only screen action.
// ConversationSessionTerminalRow 渲染一个 AI 终端行及其只读屏幕操作。
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
  // confirmOpen controls the destructive stop confirmation.
  // confirmOpen 控制破坏性停止确认。
  const [confirmOpen, setConfirmOpen] = React.useState(false);
  // screenOpen is the sole display-attachment visibility source.
  // screenOpen 是显示订阅可见性的唯一来源。
  const [screenOpen, setScreenOpen] = React.useState(false);
  const handleStopRequest = React.useCallback(() => {
    if (props.session.canStop && !stopping) setConfirmOpen(true);
  }, [props.session.canStop, stopping]);
  const handleConfirmStop = React.useCallback(async () => {
    if (!props.session.canStop || stopping) return;
    setStopping(true);
    try {
      await props.onStopCommandSession(props.session.id, props.summary?.ownerSessionId ?? null);
      setConfirmOpen(false);
    } finally {
      setStopping(false);
    }
  }, [props, stopping]);
  const tooltipText = terminalSessionTooltipText(props.session, props.summary, t);
  const stateLabel = localizedTerminalStateLabel(props.session.state, t);
  return (
    <div className="workspace-tool-panel-menu-row conversation-session-terminal-row">
      <Tooltip>
        <TooltipTrigger asChild>
          <div className="conversation-session-terminal-summary">
            <TerminalSquare className={cn("terminal-session-icon", terminalStateClass(props.session.state))} size={15} />
            <span className="conversation-session-terminal-name">{props.session.name}</span>
          </div>
        </TooltipTrigger>
        <TooltipContent className="conversation-session-terminal-tooltip" side="left" sideOffset={8}>
          <pre>{tooltipText}</pre>
        </TooltipContent>
      </Tooltip>
      <button
        aria-label={t("terminalSession.openScreen")}
        className="conversation-session-terminal-action conversation-session-terminal-screen-button"
        disabled={!props.summary}
        onClick={() => setScreenOpen(true)}
        title={props.summary ? t("terminalSession.openScreen") : t("terminalSession.noScreen")}
        type="button"
      >
        <Maximize2 size={12} />
      </button>
      <button
        aria-label={t("terminalSession.forceStopSession")}
        className="conversation-session-terminal-action conversation-session-terminal-stop-button"
        disabled={!props.session.canStop || stopping}
        onClick={handleStopRequest}
        title={props.session.canStop ? t("terminalSession.forceStop") : stateLabel}
        type="button"
      >
        <Square fill="currentColor" size={11} strokeWidth={0} />
      </button>
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
