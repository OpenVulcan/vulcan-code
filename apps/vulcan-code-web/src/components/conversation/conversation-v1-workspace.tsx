import * as React from "react";

import {
  ConversationV1SessionDisplayRenderer,
  type ConversationV1RootInputGenerationAction,
} from "@/components/conversation/conversation-v1-display-renderer";
import {
  PlanArtifactBlock,
  type PlanArtifactActionPayload,
} from "@/components/conversation/conversation-plan-artifact";
import type { ConversationPlanArtifact } from "@/components/conversation/conversation-model";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import { useI18n } from "@/i18n";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { SelectedConversationEventWindowCommands } from "@/service/selected-conversation-event-window-registry";
import type { ExternalResourceRef } from "@/service/conversation-event-window-store";

/** Process-local user input shown only until its canonical V1 event appears.
 * 仅在对应 canonical V1 事件出现前展示的进程内用户输入。 */
export interface ConversationV1OptimisticUserInput {
  /** Browser idempotency identity used only for exact reconciliation.
   * 仅用于精确对账的浏览器幂等身份。 */
  clientActionId: string;
  /** User-visible prompt text.
   * 用户可见提示词正文。 */
  text: string;
  /** Ordered display-only attachment names copied from the composer.
   * 从输入区复制的有序纯展示附件名称。 */
  attachmentNames: readonly string[];
}

/** Props for the production V1 event-native transcript shell.
 * 生产 V1 事件原生转录外壳的属性。 */
export interface ConversationV1WorkspaceProps {
  /** Exact selected persisted session identity.
   * 精确的当前选中持久化会话身份。 */
  sessionId: string;
  /** Atomic selected-session event-window snapshot.
   * 当前选中会话的原子事件窗口快照。 */
  snapshot: ConversationEventWindowControllerSnapshot;
  /** Commands permanently bound to the same selected owner generation.
   * 永久绑定到同一选中归属代次的命令。 */
  commands: SelectedConversationEventWindowCommands;
  /** Project workspace path used only by the established Markdown renderer.
   * 仅供既有 Markdown 渲染器使用的项目工作区路径。 */
  workspacePath?: string | null;
  /** Current reasoning display preference.
   * 当前思考展示偏好。 */
  reasoningDisplayMode: ThinkingDisplayMode;
  /** Whether the owning session is currently executing.
   * 所属会话当前是否正在执行。 */
  sessionRunning: boolean;
  /** Whether ChatArea already owns the empty-conversation presentation.
   * ChatArea 是否已经持有空会话展示。 */
  suppressEmptyPlaceholder?: boolean;
  /** Process-local optimistic row waiting for the authoritative V1 UserInput.
   * 等待权威 V1 UserInput 的进程内乐观行。 */
  optimisticUserInput?: ConversationV1OptimisticUserInput | null;
  /** Current pending Plan artifact requiring one user decision.
   * 当前需要用户决策的待处理计划产物。 */
  pendingPlanArtifact?: ConversationPlanArtifact | null;
  /** Executes the pending Plan artifact.
   * 执行待处理计划产物。 */
  onPlanArtifactExecute?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  /** Sends a revision suggestion for the pending Plan artifact.
   * 为待处理计划产物发送修改建议。 */
  onPlanArtifactSuggest?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  /** Cancels the pending Plan artifact.
   * 取消待处理计划产物。 */
  onPlanArtifactCancel?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  /** Copies visible Plan content through the shell clipboard boundary.
   * 通过外壳剪贴板边界复制可见计划内容。 */
  onCopyText: (text: string) => Promise<void> | void;
  /** Preserves the current semantic viewport while older events are prepended.
   * 在前置更早事件时保留当前语义视口。 */
  onPreserveHistoryPrepend: (load: () => Promise<boolean>) => Promise<boolean>;
  /** Receives scroll events from the exact transcript element.
   * 接收精确转录元素的滚动事件。 */
  onScrollStateChange: () => void;
  /** Scroll element reference owned by the ChatArea controller.
   * 由 ChatArea 控制器持有的滚动元素引用。 */
  scrollRef: React.RefObject<HTMLElement | null>;
  /** Request-local status rendered after the durable event window.
   * 渲染在可靠事件窗口之后的请求局部状态。 */
  transientReasoningStatus?: React.ReactNode;
  /** Sticky footer rendered inside the transcript scroll surface.
   * 渲染在转录滚动界面内的粘性底部区域。 */
  footer?: React.ReactNode;
  /** Kind-qualified live external resource identities.
   * 带类型的实时外部资源身份。 */
  availableExternalRefKeys?: ReadonlySet<string>;
  /** Opens one currently available external resource.
   * 打开一个当前可用的外部资源。 */
  onOpenExternalRef?: (reference: ExternalResourceRef) => void;
  /** Receives one edit or fork action anchored by a canonical root-input seq.
   * 接收一个由 canonical 根输入 seq 锚定的编辑或分叉动作。
   */
  onRootInputGenerationAction?: (action: ConversationV1RootInputGenerationAction) => void;
}

/** Renders the production transcript from one V1 event window and no legacy messages.
 * 仅从一个 V1 事件窗口渲染生产转录，不读取任何旧消息。
 *
 * @param props exact selected owner, snapshot, and shell callbacks.
 *              精确的选中归属、快照与外壳回调。
 * @returns the scrollable event-native transcript surface.
 *          可滚动的事件原生转录界面。 */
export function ConversationV1Workspace(props: ConversationV1WorkspaceProps) {
  // t resolves the established empty and history-loading labels.
  // t 解析既有空状态与历史加载文案。
  const { t } = useI18n();
  // historyLoading is process-local because the canonical cursor remains in the controller snapshot.
  // historyLoading 仅属于进程内，因为 canonical 游标仍保留在控制器快照中。
  const [historyLoading, setHistoryLoading] = React.useState(false);
  // ownerReady prevents a stale previous-session window from rendering during owner handoff.
  // ownerReady 防止归属切换期间渲染上一会话的过期窗口。
  const ownerReady = props.snapshot.owner?.session_id === props.sessionId;
  // events becomes empty until the exact selected owner is ready; no stale fallback is allowed.
  // 在精确选中归属准备完成前 events 保持为空；不允许使用过期回退。
  const events = ownerReady ? props.snapshot.events : [];

  React.useEffect(() => {
    setHistoryLoading(false);
  }, [props.sessionId, props.snapshot.owner?.window_generation]);

  /** Loads one older complete-Turn page through the owner-bound V1 command.
   * 通过绑定归属的 V1 命令加载一页更早的完整 Turn。 */
  const handleLoadMoreHistory = React.useCallback(async () => {
    if (!ownerReady || !props.snapshot.has_more || historyLoading) {
      return;
    }
    setHistoryLoading(true);
    try {
      await props.onPreserveHistoryPrepend(() => props.commands.prependOlder());
    } finally {
      setHistoryLoading(false);
    }
  }, [
    historyLoading,
    ownerReady,
    props.commands,
    props.onPreserveHistoryPrepend,
    props.snapshot.has_more,
  ]);

  return (
    <section
      className="conversation-panel conversation-v1-workspace"
      data-conversation-surface-id={props.sessionId}
      data-v1-owner-ready={ownerReady ? "true" : "false"}
      onScroll={props.onScrollStateChange}
      ref={props.scrollRef}
    >
      <div className="conversation-scroll-content">
        <div className="timeline conversation-v1-timeline">
          {!ownerReady ? (
            <article className="message-card system" data-v1-owner-loading="true">
              <span>system</span>
              <p>{t("conversation.history.loading")}</p>
            </article>
          ) : null}
          {ownerReady && events.length === 0 && !props.suppressEmptyPlaceholder ? (
            <article className="message-card system">
              <span>system</span>
              <p>{t("conversation.emptyLoaded")}</p>
            </article>
          ) : null}
          {ownerReady && props.snapshot.has_more ? (
            <div className="conversation-history-load-row">
              <button
                className="conversation-history-load-button"
                disabled={historyLoading}
                onClick={() => void handleLoadMoreHistory()}
                type="button"
              >
                {historyLoading ? t("conversation.history.loading") : t("conversation.history.loadMore")}
              </button>
            </div>
          ) : null}
          {ownerReady ? (
            <ConversationV1SessionDisplayRenderer
              availableExternalRefKeys={props.availableExternalRefKeys}
              defaultExpandedTools={false}
              events={events}
              onOpenExternalRef={props.onOpenExternalRef}
              onRootInputGenerationAction={props.onRootInputGenerationAction}
              reasoningDisplayMode={props.reasoningDisplayMode}
              rootInputGenerationActionsDisabled={props.sessionRunning}
              sessionId={props.sessionId}
              workspacePath={props.workspacePath}
            />
          ) : null}
          {props.optimisticUserInput ? (
            <article
              className="conversation-v1-event conversation-v1-optimistic-user-input"
              data-client-action-id={props.optimisticUserInput.clientActionId}
              data-v1-optimistic-user-input="true"
            >
              <p>{props.optimisticUserInput.text}</p>
              {props.optimisticUserInput.attachmentNames.length > 0 ? (
                <ul>
                  {props.optimisticUserInput.attachmentNames.map((name, index) => (
                    <li key={`${index}:${name}`}>{name}</li>
                  ))}
                </ul>
              ) : null}
            </article>
          ) : null}
          {ownerReady && props.pendingPlanArtifact ? (
            <PlanArtifactBlock
              artifact={props.pendingPlanArtifact}
              disabled={props.sessionRunning}
              pending
              onCancel={props.onPlanArtifactCancel ?? (() => undefined)}
              onCopyText={props.onCopyText}
              onExecute={props.onPlanArtifactExecute ?? (() => undefined)}
              onSuggest={props.onPlanArtifactSuggest ?? (() => undefined)}
            />
          ) : null}
          {props.transientReasoningStatus ?? null}
        </div>
        {props.footer ? (
          <div className="conversation-footer" data-conversation-scroll-footer="true">
            {props.footer}
          </div>
        ) : null}
      </div>
    </section>
  );
}
