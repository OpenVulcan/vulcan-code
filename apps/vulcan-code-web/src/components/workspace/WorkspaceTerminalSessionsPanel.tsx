import React from "react";
import { TerminalSquare, X } from "lucide-react";
import {
  terminalSessionViewModel,
  terminalStateClass,
  type TerminalSessionViewModel,
} from "@/components/conversation/conversation-terminal-model";
import { cn } from "@/lib/utils";
import type { CommandSessionSummary } from "@/types";

// WorkspaceTerminalSessionsPanel renders live terminal sessions inside the context panel.
// WorkspaceTerminalSessionsPanel 在上下文面板中渲染实时终端会话。
export function WorkspaceTerminalSessionsPanel(props: {
  // Current command-session summaries from the workbench snapshot.
  // 当前工作台快照中的命令会话摘要。
  commandSessions: CommandSessionSummary[];
  // Force-stops one live command session.
  // 强制停止单条实时命令会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
}) {
  const terminalSessions = React.useMemo(
    () => props.commandSessions
      .filter((summary) => summary.id !== "vldb-controller")
      .map(terminalSessionViewModel)
      .filter((session) => session.canStop || session.state === "lost" || session.state === "waiting"),
    [props.commandSessions],
  );
  if (terminalSessions.length === 0) {
    return null;
  }
  return (
    <section className="workspace-terminal-sessions-panel">
      <div className="workspace-terminal-sessions-header">
        <TerminalSquare size={15} />
        <span>终端会话</span>
      </div>
      <div className="workspace-terminal-sessions-list">
        {terminalSessions.map((session) => (
          <WorkspaceTerminalSessionRow
            key={session.id}
            session={session}
            summary={props.commandSessions.find((summary) => summary.id === session.id) ?? null}
            onStopCommandSession={props.onStopCommandSession}
          />
        ))}
      </div>
    </section>
  );
}

// WorkspaceTerminalSessionRow renders one terminal session with a force-stop action.
// WorkspaceTerminalSessionRow 渲染单条带强制停止动作的终端会话。
function WorkspaceTerminalSessionRow(props: {
  // Normalized terminal-session model.
  // 归一化后的终端会话模型。
  session: TerminalSessionViewModel;
  // Original backend summary used for owner metadata.
  // 用于归属元数据的原始后端摘要。
  summary: CommandSessionSummary | null;
  // Force-stops one live command session.
  // 强制停止单条实时命令会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
}) {
  const [stopping, setStopping] = React.useState(false);
  const handleStop = React.useCallback(async () => {
    if (!props.session.canStop || stopping) {
      return;
    }
    if (!window.confirm(`确认强制终止终端会话？\n\n${props.session.command}`)) {
      return;
    }
    setStopping(true);
    try {
      await props.onStopCommandSession(props.session.id, props.summary?.ownerSessionId ?? null);
    } finally {
      setStopping(false);
    }
  }, [props, stopping]);
  return (
    <div className="workspace-terminal-session-row">
      <TerminalSquare className={cn("terminal-session-icon", terminalStateClass(props.session.state))} size={15} />
      <div className="workspace-terminal-session-main">
        <div className="workspace-terminal-session-title">
          <span className="workspace-terminal-session-label">{props.session.label}</span>
          <span className="workspace-terminal-session-command">{props.session.command}</span>
        </div>
        {props.session.preview ? <div className="workspace-terminal-session-preview">{props.session.preview}</div> : null}
      </div>
      <button
        aria-label="强制终止终端会话"
        className="workspace-terminal-session-stop"
        disabled={!props.session.canStop || stopping}
        onClick={handleStop}
        title={props.session.canStop ? "强制终止" : props.session.label}
        type="button"
      >
        <X size={13} />
      </button>
    </div>
  );
}
