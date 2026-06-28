import React from "react";
import { Maximize2, Minimize2, Square, TerminalSquare } from "lucide-react";
import {
  terminalStateClass,
  type TerminalSessionViewModel,
} from "@/components/conversation/conversation-terminal-model";
import { TerminalOutputView } from "@/components/conversation/conversation-terminal-output";
import { TerminalStopConfirmDialog } from "@/components/conversation/terminal-stop-confirm-dialog";
import {
  getCommandSessionSnapshot,
  sendCommandSessionInput,
  type TerminalInputEvent,
} from "@/service/conversation-actions-service";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { CommandSessionSummary } from "@/types";

// ConversationSessionTerminalSection renders active terminal sessions in the shared floating session tool panel.
// ConversationSessionTerminalSection 在共享会话浮动工具面板中渲染活跃终端会话。
export function ConversationSessionTerminalSection(props: {
  // Normalized terminal session rows.
  // 归一化后的终端会话行。
  terminalSessions: TerminalSessionViewModel[];
  // Original summaries keyed by session id for owner metadata.
  // 按会话标识索引的原始摘要，用于归属元数据。
  summariesById: Map<string, CommandSessionSummary>;
  // Whether this section owns the panel collapse action.
  // 当前区域是否拥有面板折叠动作。
  showCollapse: boolean;
  // Collapses the expanded session tool panel.
  // 折叠当前展开的会话工具面板。
  onCollapse: () => void;
  // Force-stops one live terminal session.
  // 强制停止单条实时终端会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
}) {
  return (
    <section className="conversation-session-tool-section conversation-session-terminal-section">
      <div className={cn("conversation-session-tool-section-title", props.showCollapse && "with-action")}>
        <span className="conversation-session-tool-section-title-main">
          <span>终端</span>
          <strong>{props.terminalSessions.length}</strong>
        </span>
        {props.showCollapse ? (
          <button
            aria-label="折叠会话工具"
            className="workspace-tool-panel-menu-icon-button"
            onClick={props.onCollapse}
            title="折叠会话工具"
            type="button"
          >
            <Minimize2 size={13} />
          </button>
        ) : null}
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

// ConversationSessionTerminalRow renders one terminal session row with status and force-stop action.
// ConversationSessionTerminalRow 渲染单条带状态与强制停止动作的终端会话。
function ConversationSessionTerminalRow(props: {
  // Normalized terminal session row.
  // 归一化后的终端会话行。
  session: TerminalSessionViewModel;
  // Original summary used for owner metadata.
  // 用于归属元数据的原始摘要。
  summary: CommandSessionSummary | null;
  // Force-stops one live terminal session.
  // 强制停止单条实时终端会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
}) {
  const [stopping, setStopping] = React.useState(false);
  const [confirmOpen, setConfirmOpen] = React.useState(false);
  const [screenOpen, setScreenOpen] = React.useState(false);
  const handleStopRequest = React.useCallback(() => {
    if (!props.session.canStop || stopping) {
      return;
    }
    setConfirmOpen(true);
  }, [props.session.canStop, stopping]);
  const handleConfirmStop = React.useCallback(async () => {
    if (!props.session.canStop || stopping) {
      return;
    }
    setStopping(true);
    try {
      await props.onStopCommandSession(props.session.id, props.summary?.ownerSessionId ?? null);
      setConfirmOpen(false);
    } finally {
      setStopping(false);
    }
  }, [props, stopping]);
  const handleConfirmOpenChange = React.useCallback((open: boolean) => {
    if (stopping) {
      return;
    }
    setConfirmOpen(open);
  }, [stopping]);
  const tooltipText = terminalSessionTooltipText(props.session, props.summary);
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
        aria-label="打开终端屏幕"
        className="conversation-session-terminal-action conversation-session-terminal-screen-button"
        disabled={!props.summary}
        onClick={() => setScreenOpen(true)}
        title={props.summary ? "打开终端屏幕" : "暂无终端屏幕"}
        type="button"
      >
        <Maximize2 size={12} />
      </button>
      <button
        aria-label="强制终止终端会话"
        className="conversation-session-terminal-action conversation-session-terminal-stop-button"
        disabled={!props.session.canStop || stopping}
        onClick={handleStopRequest}
        title={props.session.canStop ? "强制终止" : props.session.label}
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
        onOpenChange={handleConfirmOpenChange}
      />
      <TerminalScreenDialog
        open={screenOpen}
        session={props.session}
        summary={props.summary}
        onOpenChange={setScreenOpen}
      />
    </div>
  );
}

// TerminalScreenDialog renders the optional floating terminal screen view.
// TerminalScreenDialog 渲染可选的浮动终端屏幕视图。
function TerminalScreenDialog(props: {
  // Whether the floating terminal screen is open.
  // 浮动终端屏幕是否打开。
  open: boolean;
  // Normalized terminal session row.
  // 归一化后的终端会话行。
  session: TerminalSessionViewModel;
  // Original backend summary with raw screen fields.
  // 带原始屏幕字段的后端摘要。
  summary: CommandSessionSummary | null;
  // Updates the floating terminal screen open state.
  // 更新浮动终端屏幕打开状态。
  onOpenChange: (open: boolean) => void;
}) {
  const [sending, setSending] = React.useState(false);
  const [liveSummary, setLiveSummary] = React.useState<CommandSessionSummary | null>(props.summary);
  React.useEffect(() => {
    setLiveSummary(props.summary);
  }, [props.summary]);
  const refreshLiveSummary = React.useCallback(async () => {
    const nextSummary = await getCommandSessionSnapshot(props.session.id, liveSummary?.ownerSessionId ?? props.summary?.ownerSessionId ?? null);
    if (nextSummary) {
      setLiveSummary(nextSummary);
    }
  }, [liveSummary?.ownerSessionId, props.session.id, props.summary?.ownerSessionId]);
  React.useEffect(() => {
    if (!props.open) {
      return;
    }
    let cancelled = false;
    const poll = async () => {
      try {
        const nextSummary = await getCommandSessionSnapshot(props.session.id, liveSummary?.ownerSessionId ?? props.summary?.ownerSessionId ?? null);
        if (!cancelled && nextSummary) {
          setLiveSummary(nextSummary);
        }
      } catch (error) {
        console.warn("Failed to refresh command session screen snapshot.", error);
      }
    };
    void poll();
    const intervalId = window.setInterval(() => {
      void poll();
    }, 250);
    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, [liveSummary?.ownerSessionId, props.open, props.session.id, props.summary?.ownerSessionId]);
  const screenText = liveSummary?.screenText ?? "";
  const backend = liveSummary?.backend ?? props.summary?.backend ?? "unknown";
  const sizeLabel = liveSummary?.screenRows && liveSummary?.screenCols
    ? `${liveSummary.screenRows}x${liveSummary.screenCols}`
    : "unknown";
  const handleKeyDown = React.useCallback(async (event: React.KeyboardEvent<HTMLPreElement>) => {
    const input = terminalInputEventFromKeyboardEvent(event);
    if (!input || sending) {
      return;
    }
    event.preventDefault();
    setSending(true);
    try {
      await sendCommandSessionInput(props.session.id, liveSummary?.ownerSessionId ?? props.summary?.ownerSessionId ?? null, [input]);
      await refreshLiveSummary();
    } finally {
      setSending(false);
    }
  }, [liveSummary?.ownerSessionId, props.session.id, props.summary?.ownerSessionId, refreshLiveSummary, sending]);
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="conversation-session-terminal-screen-dialog">
        <DialogTitle>{props.session.name}</DialogTitle>
        <DialogDescription>
          {backend} · {sizeLabel} · frame {liveSummary?.screenFrameSeq ?? props.summary?.screenFrameSeq ?? 0}
        </DialogDescription>
        <TerminalOutputView
          className="conversation-session-terminal-screen-frame"
          emptyText="暂无终端屏幕内容"
          onKeyDown={handleKeyDown}
          suppressBlackBackground
          tabIndex={0}
          text={screenText}
        />
      </DialogContent>
    </Dialog>
  );
}

// terminalInputEventFromKeyboardEvent converts browser keyboard events into terminal input events.
// terminalInputEventFromKeyboardEvent 将浏览器键盘事件转换为终端输入事件。
function terminalInputEventFromKeyboardEvent(event: React.KeyboardEvent): TerminalInputEvent | null {
  const keyName = terminalKeyNameFromBrowserKey(event.key);
  if (event.ctrlKey || event.altKey || event.metaKey || (event.shiftKey && keyName && event.key !== "Tab")) {
    if (!keyName && event.key.length !== 1) {
      return null;
    }
    return {
      kind: "chord",
      key: keyName ?? event.key,
      ctrl: event.ctrlKey,
      alt: event.altKey,
      shift: event.shiftKey,
      meta: event.metaKey,
    };
  }
  if (keyName) {
    return { kind: "key", key: keyName };
  }
  if (event.key.length === 1) {
    return { kind: "text", text: event.key };
  }
  return null;
}

// terminalKeyNameFromBrowserKey maps browser key names to backend key names.
// terminalKeyNameFromBrowserKey 将浏览器按键名映射为后端按键名。
function terminalKeyNameFromBrowserKey(key: string): string | null {
  if (/^F([1-9]|1[0-2])$/.test(key)) {
    return key;
  }
  switch (key) {
    case "Enter":
    case "Tab":
    case "Backspace":
    case "Delete":
    case "Home":
    case "End":
      return key;
    case "Escape":
      return "Esc";
    case "ArrowUp":
    case "ArrowDown":
    case "ArrowLeft":
    case "ArrowRight":
      return key;
    case "PageUp":
    case "PageDown":
    case "Insert":
      return key;
    default:
      return null;
  }
}

// terminalSessionTooltipText builds a complete hover description for one compact terminal row.
// terminalSessionTooltipText 为单条紧凑终端行构建完整悬浮说明。
function terminalSessionTooltipText(
  // Normalized terminal row shown in the panel.
  // 面板中展示的归一化终端行。
  session: TerminalSessionViewModel,
  // Original backend summary with raw runtime fields.
  // 带原始运行时字段的后端摘要。
  summary: CommandSessionSummary | null,
): string {
  const lines = [
    `状态：${session.label}`,
    `命令：${session.command}`,
  ];
  if (summary?.cwd) {
    lines.push(`目录：${summary.cwd}`);
  }
  if (summary?.shellLabel) {
    lines.push(`Shell：${summary.shellLabel}`);
  }
  if (summary?.backend) {
    lines.push(`后端：${summary.backend}`);
  }
  if (summary?.screenFrameSeq) {
    lines.push(`屏幕帧：${summary.screenFrameSeq}`);
  }
  if (typeof session.exitCode === "number") {
    lines.push(`退出码：${session.exitCode}`);
  }
  if (summary?.failureReason) {
    lines.push(`错误：${summary.failureReason}`);
  }
  if (session.preview) {
    lines.push(`输出：${session.preview}`);
  }
  lines.push(`会话：${session.id}`);
  return lines.join("\n");
}
