import React from "react";
import { TerminalSquare } from "lucide-react";
import { createPortal } from "react-dom";

import type { TerminalSessionViewModel } from "@/components/conversation/conversation-terminal-model";
import { acquireVisibleTerminalDisplayListener } from "@/components/terminal/terminal-display-listener";
import { TerminalViewport } from "@/components/terminal/terminal-viewport";
import { FloatingWindowHead } from "@/components/workspace/floating-window-head";
import { useTerminalDisplayConnectionState } from "@/components/terminal/use-terminal-display-connection-epoch";
import {
  ProjectFloatingLayer,
  type ProjectFloatingLayerControls,
} from "@/components/workspace/project-floating-layer";
import { useI18n } from "@/i18n";
import { serviceClient } from "@/service/webui-service-runtime";
import { attachCommandSessionDisplay, detachCommandSessionDisplay } from "@/service/terminal-service";
import type {
  CommandSessionDisplayChangedEvent,
  CommandSessionSummary,
  TerminalDisplayChunk,
} from "@/types";

// ConversationReadOnlyTerminalSurfaceProps binds a visible dialog to one AI-owned terminal.
// ConversationReadOnlyTerminalSurfaceProps 将可见对话框绑定到一个 AI 持有的终端。
export interface ConversationReadOnlyTerminalSurfaceProps {
  // Live row button used as the macOS-style minimize and restore animation anchor.
  // 用作 macOS 风格收回与显示动画锚点的实时行按钮。
  minimizeAnchorRef: React.RefObject<HTMLButtonElement | null>;
  // Whether the read-only display is currently visible and subscribed.
  // 只读显示当前是否可见且已订阅。
  open: boolean;
  // Normalized AI terminal row used only for display labels.
  // 仅用于显示标签的归一化 AI 终端行。
  session: TerminalSessionViewModel;
  // Authoritative backend summary carrying display ownership.
  // 携带显示归属的权威后端摘要。
  summary: CommandSessionSummary | null;
  // Updates dialog visibility and therefore subscription lifetime.
  // 更新对话框可见性以及相应的订阅生命周期。
  onOpenChange: (open: boolean) => void;
}

// CommandSessionScreenBaseline stores one atomic frame used to initialize or recover xterm.
// CommandSessionScreenBaseline 保存一份用于初始化或恢复 xterm 的原子帧。
interface CommandSessionScreenBaseline {
  // Monotonic local presentation sequence that also advances after reconnect.
  // 本地单调展示序号，并且会在重连后继续推进。
  seq: number;
  // Renderable screen text belonging to the frame.
  // 属于该帧的可渲染屏幕文本。
  text: string;
}

// ConversationReadOnlyTerminalSurface renders AI output without registering any input path.
// ConversationReadOnlyTerminalSurface 渲染 AI 输出，并且不注册任何输入路径。
export function ConversationReadOnlyTerminalSurface(props: ConversationReadOnlyTerminalSurfaceProps) {
  // t resolves active-locale accessibility copy.
  // t 解析当前语言的无障碍文案。
  const { t } = useI18n();
  // liveSummary follows only the currently visible display subscription.
  // liveSummary 仅跟随当前可见显示订阅。
  const [liveSummary, setLiveSummary] = React.useState<CommandSessionSummary | null>(props.summary);
  // chunks retains a bounded ordered visible delivery queue for the stable xterm instance.
  // chunks 为稳定 xterm 实例保留有界有序的可见交付队列。
  const [chunks, setChunks] = React.useState<TerminalDisplayChunk[]>([]);
  // attachmentError distinguishes a failed live subscription from an unchanged historical screen.
  // attachmentError 区分实时订阅失败与没有变化的历史屏幕。
  const [attachmentError, setAttachmentError] = React.useState<string | null>(null);
  // screenPresentationSeq forces a fresh atomic baseline after every successful attachment.
  // screenPresentationSeq 在每次成功挂接后强制应用一份新的原子基线。
  const screenPresentationSeq = React.useRef(0);
  // screenBaseline initializes xterm from the latest terminal-host PTY checkpoint after attachment.
  // screenBaseline 在挂接后使用最新 terminal-host PTY 检查点初始化 xterm。
  const [screenBaseline, setScreenBaseline] = React.useState<CommandSessionScreenBaseline | null>(null);
  // publishScreenBaseline installs one frame with a local sequence independent from backend counters.
  // publishScreenBaseline 使用独立于后端计数器的本地序号安装一份帧。
  const publishScreenBaseline = React.useCallback((summary: CommandSessionSummary | null) => {
    const text = commandSessionScreenText(summary);
    if (text === null) {
      setScreenBaseline(null);
      return false;
    }
    screenPresentationSeq.current += 1;
    setScreenBaseline({ seq: screenPresentationSeq.current, text });
    return true;
  }, []);
  // connectionState detaches display delivery during interruption and forces a new baseline after recovery.
  // connectionState 在中断期间解除显示交付，并在恢复后强制获取新基线。
  const { connectionAvailable, connectionEpoch } = useTerminalDisplayConnectionState();
  // workspaceHost keeps every AI terminal constrained below the conversation title bar like project terminals.
  // workspaceHost 使每个 AI 终端像项目终端一样限制在会话标题栏下方。
  const workspaceHost = props.minimizeAnchorRef.current?.closest<HTMLElement>(".vk-workbench-main") ?? null;

  React.useEffect(() => {
    if (!props.open || !props.summary || !connectionAvailable || !workspaceHost) return;
    let cancelled = false;
    let subscriptionId: string | null = null;
    let unsubscribe: (() => void) | null = null;
    const pendingEvents: CommandSessionDisplayChangedEvent[] = [];
    // releaseDisplayListener removes the browser topic handler exactly once.
    // releaseDisplayListener 仅一次移除浏览器主题处理器。
    const releaseDisplayListener = () => {
      // retainedUnsubscribe moves the disposer out before invocation to make cleanup idempotent.
      // retainedUnsubscribe 在调用前移出释放函数，使清理具备幂等性。
      const retainedUnsubscribe = unsubscribe;
      unsubscribe = null;
      retainedUnsubscribe?.();
    };
    // releaseDisplayAttachment detaches the service-side push subscription exactly once.
    // releaseDisplayAttachment 仅一次解除服务端推送订阅。
    const releaseDisplayAttachment = () => {
      // retainedSubscriptionId moves the service id out before issuing the detach request.
      // retainedSubscriptionId 在发出解除请求前移出服务标识。
      const retainedSubscriptionId = subscriptionId;
      subscriptionId = null;
      if (retainedSubscriptionId) void detachCommandSessionDisplay(retainedSubscriptionId);
    };
    // applyEvent updates only the exact subscription created by this visible mount.
    // applyEvent 仅更新由当前可见挂载创建的精确订阅。
    const applyEvent = (event: CommandSessionDisplayChangedEvent) => {
      if (cancelled || event.subscriptionId !== subscriptionId) return;
      setLiveSummary(event.terminal);
      setChunks((current) => mergeVisibleTerminalChunks(current, event.chunks));
    };
    // attach registers transport delivery before the atomic service attach/query boundary.
    // attach 在原子 Service 挂接/查询边界之前注册传输交付。
    const attach = async () => {
      try {
        // listenerAcquired reports whether this dialog remained visible through asynchronous topic setup.
        // listenerAcquired 报告当前对话框在异步主题建立期间是否始终保持可见。
        const listenerAcquired = await acquireVisibleTerminalDisplayListener(
          () => serviceClient.subscribe<CommandSessionDisplayChangedEvent>(
            "command.session.display.changed",
            (event) => {
              // Ignore other visible AI terminals while this attachment id is not known yet.
              // 当前挂接标识尚未返回时，忽略其他可见 AI 终端的事件。
              if (event.terminal.id !== props.session.id) return;
              if (subscriptionId === null) pendingEvents.push(event);
              else applyEvent(event);
            },
          ),
          () => cancelled,
          (retainedUnsubscribe) => {
            unsubscribe = retainedUnsubscribe;
          },
        );
        if (!listenerAcquired) return;
        const attachment = await attachCommandSessionDisplay(
          props.session.id,
          props.summary?.ownerSessionId ?? null,
        );
        if (!attachment) {
          releaseDisplayListener();
          return;
        }
        subscriptionId = attachment.subscriptionId;
        if (cancelled) {
          releaseDisplayListener();
          // cancelledSubscriptionId retains the just-created service id for awaited cancellation cleanup.
          // cancelledSubscriptionId 为等待完成的取消清理保留刚创建的服务标识。
          const cancelledSubscriptionId = subscriptionId;
          subscriptionId = null;
          if (cancelledSubscriptionId) await detachCommandSessionDisplay(cancelledSubscriptionId);
          return;
        }
        setLiveSummary(attachment.terminal);
        const hasAttachmentBaseline = publishScreenBaseline(attachment.terminal);
        setChunks(hasAttachmentBaseline ? [] : attachment.chunks);
        for (const event of pendingEvents) applyEvent(event);
      } catch (error) {
        releaseDisplayListener();
        releaseDisplayAttachment();
        if (!cancelled) setAttachmentError(error instanceof Error ? error.message : String(error));
        console.warn("Failed to attach the visible command-session terminal display.", error);
      }
    };
    setLiveSummary(props.summary);
    setChunks([]);
    setAttachmentError(null);
    void attach();
    return () => {
      cancelled = true;
      releaseDisplayListener();
      releaseDisplayAttachment();
    };
  }, [
    connectionAvailable,
    connectionEpoch,
    props.open,
    props.session.id,
    props.summary?.ownerSessionId,
    publishScreenBaseline,
    workspaceHost,
  ]);

  if (!workspaceHost) return null;

  // floatingIdentity keeps layout persistence isolated by owner conversation and terminal identity.
  // floatingIdentity 按归属会话与终端标识隔离布局持久化。
  const floatingIdentity = conversationTerminalFloatingIdentity(
    props.session.id,
    props.summary?.ownerSessionId ?? null,
  );
  // header renders read-only ownership plus the shared floating window commands.
  // header 渲染只读归属以及共享浮动窗口命令。
  const header = (floatingControls: ProjectFloatingLayerControls) => (
    <FloatingWindowHead
      controls={floatingControls}
      detail={`${liveSummary?.backend ?? props.summary?.backend ?? "unknown"} · ${t("workspaceAudit.aiReadOnly")}`}
      hoverTitle={props.session.name}
      icon={<TerminalSquare />}
      labels={{ maximize: t("workspaceAudit.maximizeAi"), restore: t("workspaceAudit.restoreAi"), minimize: t("workspaceAudit.minimizeAi") }}
      title={props.session.name}
      onMinimize={() => props.onOpenChange(false)}
    />
  );

  return createPortal(
    <ProjectFloatingLayer
      expanded={props.open}
      header={header}
      minimizeAnchorRef={props.minimizeAnchorRef}
      projectId={floatingIdentity}
    >
      <div className="vk-term-readonly">
        {attachmentError !== null ? (
          <div role="alert">
            <details>
              <summary>{t("conversation.control.detailFailed")}</summary>
              <pre>{attachmentError}</pre>
            </details>
          </div>
        ) : null}
        <div className="vk-term-pane">
          <TerminalViewport
            ariaLabel={`${props.session.name} · ${t("terminalSession.openScreen")}`}
            chunks={chunks}
            filePaths={{ cwd: liveSummary?.cwd ?? props.summary?.cwd }}
            fixedCols={liveSummary?.screenCols ?? props.summary?.screenCols}
            fixedRows={liveSummary?.screenRows ?? props.summary?.screenRows}
            onChunksRendered={(lastRenderedSeq) => {
              setChunks((current) => retainRenderedTerminalCursor(current, lastRenderedSeq));
            }}
            readOnly
            screenFrameSeq={screenBaseline?.seq}
            screenText={screenBaseline?.text}
          />
        </div>
      </div>
    </ProjectFloatingLayer>,
    workspaceHost,
  );
}

// conversationTerminalFloatingIdentity returns one collision-safe persistent layout identity.
// conversationTerminalFloatingIdentity 返回一个无碰撞的持久化布局标识。
//
// Parameters / 参数:
// - `terminalId`: Exact AI terminal identifier. / 精确的 AI 终端标识。
// - `ownerSessionId`: Optional owning conversation identifier. / 可选的归属会话标识。
//
// Returns / 返回值:
// Returns a stable serialized tuple reserved for the floating layer cache and storage key.
// 返回为浮动层缓存与存储键保留的稳定序列化元组。
export function conversationTerminalFloatingIdentity(
  terminalId: string,
  ownerSessionId: string | null,
): string {
  return JSON.stringify(["ai-terminal", ownerSessionId ?? "", terminalId]);
}

// commandSessionScreenText extracts complete frame text only when both backend fields exist.
// commandSessionScreenText 仅在两个后端字段均存在时提取完整帧文本。
function commandSessionScreenText(
  summary: CommandSessionSummary | null,
): string | null {
  if (summary?.screenFrameSeq === null || summary?.screenFrameSeq === undefined
    || summary.screenText === null || summary.screenText === undefined) return null;
  return summary.screenText;
}

// retainRenderedTerminalCursor keeps one sequence marker while releasing rendered output text history.
// retainRenderedTerminalCursor 保留一个序号标记，同时释放已渲染的输出文本历史。
function retainRenderedTerminalCursor(
  current: TerminalDisplayChunk[],
  lastRenderedSeq: number,
): TerminalDisplayChunk[] {
  const retained = current.filter((chunk) => chunk.seq >= lastRenderedSeq);
  return retained.length === current.length ? current : retained;
}

// mergeVisibleTerminalChunks preserves every ordered push across React event batching.
// mergeVisibleTerminalChunks 在 React 事件批处理中保留每个有序推送。
function mergeVisibleTerminalChunks(
  current: TerminalDisplayChunk[],
  incoming: TerminalDisplayChunk[],
): TerminalDisplayChunk[] {
  const lastSequence = current.at(-1)?.seq ?? 0;
  const appended = incoming.filter((chunk) => chunk.seq > lastSequence);
  if (appended.length === 0) return current;
  const merged = [...current, ...appended];
  return merged.length > 4096 ? merged.slice(-4096) : merged;
}
