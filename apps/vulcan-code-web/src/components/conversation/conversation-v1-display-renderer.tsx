import React from "react";
import { ModelSwitchNotice, modelSwitchRouteLabel } from "./conversation-model-switch";
import type { ProviderSummary } from "@/types";
import {
  BrainCircuit,
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  Pencil,
  Split,
} from "lucide-react";

import { MarkdownImageWorkspacePathProvider, MarkdownMessage } from "./conversation-markdown";
import { ConversationV1ResourceList } from "./conversation-v1-resource-list";
import { ConversationV1ToolCard } from "./conversation-v1-tool-card";
import { ConversationV1TurnStatsCard } from "./conversation-v1-turn-stats-card";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import {
  projectConversationEventDisplay,
  projectConversationTranscriptDisplay,
  requestConfigChangeKind,
  type ConversationAssistantTranscriptNode,
  type ConversationDisplayNoteTranscriptNode,
  type ConversationEventDisplayItem,
  type ConversationProcessDisplayItem,
  type ConversationProcessTranscriptNode,
  type ConversationTranscriptDisplayNode,
  type ConversationTurnStatsTranscriptNode,
  type ConversationUserInputTranscriptNode,
  type RequestConfigChangedDisplayItem,
} from "@/service/conversation-event-display-projection";
import type { ConversationEventWindowController } from "@/service/conversation-event-window-controller";
import type { ConversationEventSummary, ExternalResourceRef, ResourceId } from "@/service/conversation-event-window-store";
import type { SessionResourceRpcV1 } from "@/service/conversation-session-resources-v1";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import { useI18n } from "@/i18n";
import { GoalStatusNotice } from "./conversation-goal-notice";
import { useConversationEventWindowSnapshot } from "@/state/conversation-event-window-react";
import {
  useConversationSessionResourcesV1,
  type ConversationSessionResourceCatalogV1,
} from "@/state/conversation-session-resources-v1";

/** Generation action with distinct edit-root and completed-Turn fork anchors.
 * 使用独立编辑根锚点与完整回合分叉锚点的 generation 动作。
 */
export type ConversationV1GenerationAction =
  | {
    /** Edit replaces one persisted root input and restarts that Turn.
     * 编辑会替换一条持久化根输入并重新启动该回合。 */
    kind: "edit";
    /** Exact persisted root-input seq.
     * 精确的持久化根输入 seq。 */
    targetSeq: number;
    /** Exact persisted root-input text.
     * 精确的持久化根输入正文。 */
    text: string;
  }
  | {
    /** Fork copies history through one completed Turn without starting another Turn.
     * 分叉复制到一个完整回合结束，且不会启动另一个回合。 */
    kind: "fork";
    /** Exact terminal TurnStats seq of the copied Turn.
     * 被复制回合的精确终态 TurnStats seq。 */
    sourceTurnEndSeq: number;
  };

/** Props for the V1 event-native conversation renderer.
 * V1 事件原生会话渲染器的属性。 */
export interface ConversationV1DisplayRendererProps {
  /** Configured provider/model display catalog, absent in isolated hosts without configuration.
   * 已配置的供应商和模型显示目录；不带配置的隔离宿主可省略。 */
  providers?: readonly ProviderSummary[];
  /** Complete canonical window including HostOnly events in strict seq order.
   * 包含 HostOnly 事件且按严格 seq 排序的完整 canonical 窗口。 */
  events: readonly ConversationEventSummary[];
  /** Current project path used only by the established Markdown renderer.
   * 仅供既有 Markdown 渲染器使用的当前项目路径。 */
  workspacePath?: string | null;
  /** Whether tool cards begin expanded in this surface.
   * 当前界面中的工具卡片是否默认展开。 */
  defaultExpandedTools?: boolean;
  /** Opens one typed stateful-resource navigation reference.
   * 打开一条强类型有状态资源导航引用。 */
  onOpenExternalRef?: (reference: ExternalResourceRef) => void;
  /** Kind-qualified external references that still have a live navigation target.
   * 当前仍有实时导航目标的带类型外部引用。 */
  availableExternalRefKeys?: ReadonlySet<string>;
  /** Opens one attachment referenced directly by assistant Markdown.
   * 打开一条由 assistant Markdown 直接引用的附件。 */
  onOpenMarkdownMedia?: (attachment: GeneratedMediaAttachment) => void;
  /** Copies one exact persisted user or assistant text through the shell boundary.
   * 通过外壳边界复制一份精确持久化的用户或 assistant 正文。 */
  onCopyText?: (text: string) => Promise<void> | void;
  /** Owner-bound project resource metadata for persisted user-input attachments.
   * 持久化用户输入附件所使用的绑定所有者项目资源元数据。 */
  resourceCatalog?: ConversationSessionResourceCatalogV1 | null;
  /** Persisted reasoning presentation mode selected for the current session.
   * 当前会话选择的持久化思考展示模式。 */
  reasoningDisplayMode?: ThinkingDisplayMode;
  /** Receives edit-root or completed-Turn fork actions with distinct anchors.
   * 接收锚点彼此独立的编辑根操作或完整回合分叉操作。
   */
  onRootInputGenerationAction?: (action: ConversationV1GenerationAction) => void;
  /** Whether root-input generation actions are temporarily unavailable.
   * 根输入 generation 动作当前是否暂不可用。
   */
  rootInputGenerationActionsDisabled?: boolean;
}

/** Props for the standalone V1 controller-to-renderer test host.
 * 独立 V1 控制器到渲染器测试宿主的属性。 */
export interface ConversationV1EventWindowViewProps
  extends Omit<ConversationV1DisplayRendererProps, "events" | "resourceCatalog"> {
  /** Controller that owns hydration, pagination, reconnect, and live patches.
   * 持有 hydration、分页、重连与实时 patch 的控制器。 */
  controller: ConversationEventWindowController;
  /** Optional injectable resource client for isolated tests.
   * 可供隔离测试注入的资源客户端。 */
  resourceClient?: SessionResourceRpcV1;
}

/** Connects one V1 controller to the event-native renderer without production routing.
 * 在不接入生产路由的情况下连接 V1 控制器与事件原生渲染器。
 *
 * @param props standalone controller plus renderer presentation options.
 *              独立控制器与渲染器展示选项。
 * @returns one useSyncExternalStore-backed V1 transcript surface.
 *          一个基于 useSyncExternalStore 的 V1 转录界面。 */
export function ConversationV1EventWindowView(props: ConversationV1EventWindowViewProps) {
  // snapshot is tear-free because the controller publishes complete atomic boundaries only.
  // snapshot 无撕裂，因为控制器只发布完整原子边界。
  const snapshot = useConversationEventWindowSnapshot(props.controller);
  return (
    <ConversationV1SessionDisplayRenderer
      providers={props.providers}
      defaultExpandedTools={props.defaultExpandedTools}
      events={snapshot.events}
      availableExternalRefKeys={props.availableExternalRefKeys}
      onOpenExternalRef={props.onOpenExternalRef}
      onOpenMarkdownMedia={props.onOpenMarkdownMedia}
      onCopyText={props.onCopyText}
      reasoningDisplayMode={props.reasoningDisplayMode}
      onRootInputGenerationAction={props.onRootInputGenerationAction}
      resourceClient={props.resourceClient}
      rootInputGenerationActionsDisabled={props.rootInputGenerationActionsDisabled}
      sessionId={snapshot.owner?.session_id ?? null}
      workspacePath={props.workspacePath}
    />
  );
}

/** Props for the owner-bound resource-aware V1 renderer.
 * 绑定所有者且感知资源的 V1 渲染器属性。 */
export interface ConversationV1SessionDisplayRendererProps
  extends Omit<ConversationV1DisplayRendererProps, "resourceCatalog"> {
  /** Exact V1 session owner used by project resource resolution.
   * 项目资源解析使用的精确 V1 会话所有者。 */
  sessionId: string | null;
  /** Optional injectable resource client for isolated tests.
   * 可供隔离测试注入的资源客户端。 */
  resourceClient?: SessionResourceRpcV1;
}

/** Resolves one complete event window's resources as a single batch before pure rendering.
 * 在纯渲染前将一个完整事件窗口的资源解析为单一批次。
 *
 * @param props canonical events, exact owner, and presentation options.
 *              canonical 事件、精确所有者与展示选项。
 * @returns one event-native transcript with owner-bound project resources.
 *          一个带绑定所有者项目资源的事件原生转录。 */
export function ConversationV1SessionDisplayRenderer(
  props: ConversationV1SessionDisplayRendererProps,
) {
  const resourceIds = React.useMemo(
    () => collectConversationResourceIdsV1(props.events),
    [props.events],
  );
  const resourceCatalog = useConversationSessionResourcesV1(
    props.sessionId,
    resourceIds,
    props.resourceClient,
  );
  return (
    <ConversationV1DisplayRenderer
      providers={props.providers}
      defaultExpandedTools={props.defaultExpandedTools}
      events={props.events}
      availableExternalRefKeys={props.availableExternalRefKeys}
      onOpenExternalRef={props.onOpenExternalRef}
      onOpenMarkdownMedia={props.onOpenMarkdownMedia}
      onCopyText={props.onCopyText}
      onRootInputGenerationAction={props.onRootInputGenerationAction}
      reasoningDisplayMode={props.reasoningDisplayMode}
      resourceCatalog={resourceCatalog}
      rootInputGenerationActionsDisabled={props.rootInputGenerationActionsDisabled}
      workspacePath={props.workspacePath}
    />
  );
}

/** Renders a complete V1 event window without legacy messages, parser, or detail loading.
 * 在不使用旧 messages、parser 或详情加载的情况下渲染完整 V1 事件窗口。
 *
 * @param props canonical V1 events and optional navigation callbacks.
 *              canonical V1 事件与可选导航回调。
 * @returns one event-native transcript whose visible order remains exact seq order.
 *          一个可见顺序始终保持精确 seq 顺序的事件原生转录。 */
export function ConversationV1DisplayRenderer(props: ConversationV1DisplayRendererProps) {
  // transcriptNodes validates the complete ledger before deriving the established visual hierarchy.
  // transcriptNodes 在派生既有视觉层级前校验完整账本。
  const transcriptNodes = React.useMemo(
    () => projectConversationTranscriptDisplay(
      conversationDisplayItemsForReasoningMode(
        projectConversationEventDisplay(props.events),
        props.reasoningDisplayMode ?? "inline_output",
      ),
    ),
    [props.events, props.reasoningDisplayMode],
  );
  return (
    <MarkdownImageWorkspacePathProvider value={props.workspacePath}>
      <div className="conversation-v1-display" data-conversation-v1-display="true">
        {transcriptNodes.map((node) => (
          <ConversationV1TranscriptNodeView
            providers={props.providers}
            defaultExpandedTools={props.defaultExpandedTools}
            node={node}
            availableExternalRefKeys={props.availableExternalRefKeys}
            key={`${node.kind}:${node.seq}`}
            onCopyText={props.onCopyText}
            onOpenExternalRef={props.onOpenExternalRef}
            onOpenMarkdownMedia={props.onOpenMarkdownMedia}
            onRootInputGenerationAction={props.onRootInputGenerationAction}
            resourceCatalog={props.resourceCatalog}
            rootInputGenerationActionsDisabled={props.rootInputGenerationActionsDisabled}
          />
        ))}
      </div>
    </MarkdownImageWorkspacePathProvider>
  );
}

/** Props for rendering one visual transcript node derived from canonical events.
 * 渲染一个从 canonical 事件派生的视觉转录节点所需属性。 */
interface ConversationV1TranscriptNodeViewProps {
  /** Provider/model display catalog owned by the containing surface.
   * 所属界面持有的供应商和模型显示目录。 */
  providers?: readonly ProviderSummary[];
  /** Exact visual transcript node.
   * 精确的视觉转录节点。 */
  node: ConversationTranscriptDisplayNode;
  /** Whether tool cards start expanded.
   * 工具卡片是否默认展开。 */
  defaultExpandedTools?: boolean;
  /** Kind-qualified external references that remain navigable.
   * 仍可导航的带类型外部引用。 */
  availableExternalRefKeys?: ReadonlySet<string>;
  /** Optional stateful-resource navigation callback.
   * 可选的有状态资源导航回调。 */
  onOpenExternalRef?: (reference: ExternalResourceRef) => void;
  /** Optional assistant-Markdown media callback.
   * 可选的 assistant Markdown 媒体回调。 */
  onOpenMarkdownMedia?: (attachment: GeneratedMediaAttachment) => void;
  /** Optional shell clipboard boundary.
   * 可选的外壳剪贴板边界。 */
  onCopyText?: (text: string) => Promise<void> | void;
  /** Optional owner-bound resource catalog for user-input attachments.
   * 可选的用户输入附件绑定所有者资源目录。 */
  resourceCatalog?: ConversationSessionResourceCatalogV1 | null;
  /** Optional V1 root-input action callback.
   * 可选的 V1 根输入动作回调。
   */
  onRootInputGenerationAction?: (action: ConversationV1GenerationAction) => void;
  /** Whether root-input actions are temporarily disabled.
   * 根输入动作当前是否暂时禁用。
   */
  rootInputGenerationActionsDisabled?: boolean;
}

/** Renders one visual transcript node using the established conversation hierarchy.
 * 使用既有会话层级渲染一个视觉转录节点。 */
function ConversationV1TranscriptNodeView(props: ConversationV1TranscriptNodeViewProps) {
  switch (props.node.kind) {
    case "user_input":
      return <ConversationV1UserInputView {...props} node={props.node} />;
    case "process_group":
      return <ConversationV1ProcessGroup {...props} node={props.node} />;
    case "assistant_final":
      return <ConversationV1AssistantView {...props} node={props.node} />;
    case "turn_stats":
      return <ConversationV1TurnStatsView node={props.node} />;
    case "request_config_changed":
      return <RequestConfigChangedEventView item={props.node.item} providers={props.providers} />;
    case "display_note":
      return <ConversationV1DisplayNoteView node={props.node} />;
  }
}

/** Applies the selected reasoning presentation without reordering canonical display items.
 * 在不重排 canonical 显示项的情况下应用选中的思考展示方式。
 *
 * @param items already visibility-filtered display items in exact seq order.
 *              已完成可见性过滤且保持精确 seq 顺序的显示项。
 * @param mode selected reasoning display mode.
 *             选中的思考展示方式。
 * @returns the exact visible inline sequence for the selected mode.
 *          当前展示方式对应的精确内联可见序列。 */
function conversationDisplayItemsForReasoningMode(
  items: readonly ConversationEventDisplayItem[],
  mode: ThinkingDisplayMode,
): ConversationEventDisplayItem[] {
  if (mode === "inline_output") {
    return [...items];
  }
  if (mode === "popup_only") {
    return items.filter((item) => item.kind !== "assistant_reasoning");
  }
  // firstReasoningTurns records only the first reasoning event in each logical Turn.
  // firstReasoningTurns 仅记录每个逻辑 Turn 中第一条思考事件。
  const firstReasoningTurns = new Set<number>();
  return items.filter((item) => {
    if (item.kind !== "assistant_reasoning") {
      return true;
    }
    if (item.turn_id === null || firstReasoningTurns.has(item.turn_id)) {
      return false;
    }
    firstReasoningTurns.add(item.turn_id);
    return true;
  });
}

/** Renders one canonical user input with the established bubble and subordinate actions.
 * 使用既有气泡与从属动作渲染一条 canonical 用户输入。 */
function ConversationV1UserInputView(
  props: ConversationV1TranscriptNodeViewProps & { node: ConversationUserInputTranscriptNode },
) {
  const { locale, t } = useI18n();
  const event = props.node.item.event;
  // copied controls only the transient icon confirmation and never changes persisted transcript state.
  // copied 仅控制瞬态图标确认，不会修改持久化转录状态。
  const [copied, setCopied] = React.useState(false);

  /** Copies the exact persisted prompt and exposes the established confirmation icon.
   * 复制精确持久化提示词，并显示既有确认图标。 */
  const handleCopy = React.useCallback(async () => {
    if (!props.onCopyText) return;
    await props.onCopyText(event.text);
    setCopied(true);
  }, [event.text, props.onCopyText]);
  return (
    <article
      className="message-row user conversation-v1-event"
      data-event-kind="user_input"
      data-input-id={event.input_id}
      data-seq={props.node.seq}
    >
      <div className="user-bubble">
        <p>{event.text}</p>
        <ConversationV1ResourceList
          catalog={props.resourceCatalog ?? null}
          resourceIds={event.resource_ids ?? []}
        />
      </div>
      <div className="user-message-meta">
        <time dateTime={new Date(props.node.item.created_at_ms).toISOString()}>
          {formatConversationTime(props.node.item.created_at_ms, locale)}
        </time>
        {props.onCopyText ? (
          <button
            aria-label={t("conversation.userPrompt.copy")}
            className="user-message-copy"
            onClick={() => void handleCopy()}
            onMouseLeave={() => setCopied(false)}
            title={t("conversation.userPrompt.copy")}
            type="button"
          >
            {copied ? <Check aria-hidden="true" size={14} /> : <Copy aria-hidden="true" size={14} />}
          </button>
        ) : null}
        {event.input_source === "root" && props.onRootInputGenerationAction ? (
          <button
            aria-label={t("conversation.userPrompt.edit")}
            className="user-message-copy"
            disabled={props.rootInputGenerationActionsDisabled}
              onClick={() => props.onRootInputGenerationAction?.({
                kind: "edit",
                targetSeq: props.node.seq,
                text: event.text,
              })}
            title={t("conversation.userPrompt.edit")}
            type="button"
          >
            <Pencil aria-hidden="true" size={14} />
          </button>
        ) : null}
      </div>
    </article>
  );
}

/** Renders one seq-preserving collapsible process group.
 * 渲染一个保持 seq 顺序的可折叠处理过程组。 */
function ConversationV1ProcessGroup(
  props: ConversationV1TranscriptNodeViewProps & { node: ConversationProcessTranscriptNode },
) {
  const { t } = useI18n();
  // hasOperationalItems keeps a reasoning-only Turn in the established independent thinking-card form.
  // hasOperationalItems 让只有思考的 Turn 保持既有独立思考卡形态。
  const hasOperationalItems = props.node.items.some((item) => item.kind !== "assistant_reasoning");
  const [expanded, setExpanded] = React.useState(props.node.active);
  React.useEffect(() => {
    if (props.node.active) setExpanded(true);
  }, [props.node.active]);
  if (!hasOperationalItems) {
    return (
      <div className="conversation-v1-reasoning-stack" data-seq={props.node.seq}>
        {props.node.items.map((item) => (
          <ConversationV1ProcessItem
            availableExternalRefKeys={props.availableExternalRefKeys}
            defaultExpandedTools={props.defaultExpandedTools}
            item={item}
            key={item.seq}
            onOpenExternalRef={props.onOpenExternalRef}
            onOpenMarkdownMedia={props.onOpenMarkdownMedia}
            resourceCatalog={props.resourceCatalog}
          />
        ))}
      </div>
    );
  }
  return (
    <section
      className="work-process-group conversation-v1-process-group"
      data-process-body-visible={expanded ? "true" : "false"}
      data-seq={props.node.seq}
    >
      <button
        aria-expanded={expanded}
        className="work-process-toggle"
        onClick={() => setExpanded((current) => !current)}
        type="button"
      >
        <span>{t("conversation.process.header")}</span>
        <ChevronDown aria-hidden="true" className="work-process-toggle-icon" size={14} />
      </button>
      <div className="work-process-body" hidden={!expanded}>
        {props.node.items.map((item) => (
          <ConversationV1ProcessItem
            availableExternalRefKeys={props.availableExternalRefKeys}
            defaultExpandedTools={props.defaultExpandedTools}
            item={item}
            key={item.seq}
            onOpenExternalRef={props.onOpenExternalRef}
            onOpenMarkdownMedia={props.onOpenMarkdownMedia}
            resourceCatalog={props.resourceCatalog}
          />
        ))}
      </div>
    </section>
  );
}

/** Renders one item inside a process group without changing its canonical order.
 * 在不改变 canonical 顺序的情况下渲染处理过程组中的一项。 */
function ConversationV1ProcessItem(props: {
  item: ConversationProcessDisplayItem;
  defaultExpandedTools?: boolean;
  availableExternalRefKeys?: ReadonlySet<string>;
  onOpenExternalRef?: (reference: ExternalResourceRef) => void;
  onOpenMarkdownMedia?: (attachment: GeneratedMediaAttachment) => void;
  resourceCatalog?: ConversationSessionResourceCatalogV1 | null;
}) {
  if (props.item.kind === "assistant_reasoning") {
    return (
      <ConversationV1ThinkingCard
        item={props.item}
        onOpenMarkdownMedia={props.onOpenMarkdownMedia}
      />
    );
  }
  if (props.item.kind === "tool_call") {
    return (
      <div className="conversation-v1-process-item" data-event-kind="tool_call" data-seq={props.item.seq}>
        <ConversationV1ToolCard
          availableExternalRefKeys={props.availableExternalRefKeys}
          defaultExpanded={props.defaultExpandedTools}
          item={props.item}
          onOpenExternalRef={props.onOpenExternalRef}
          resourceCatalog={props.resourceCatalog}
        />
      </div>
    );
  }
  return (
    <div
      className="conversation-v1-process-assistant"
      data-event-kind={props.item.kind}
      data-seq={props.item.seq}
    >
      <MarkdownMessage
        content={props.item.markdown}
        eager
        onOpenMedia={props.onOpenMarkdownMedia}
        streaming={props.item.assistant.status === "streaming"}
      />
    </div>
  );
}

/** Renders one persisted reasoning event through the established independent folding card.
 * 通过既有独立折叠卡渲染一条持久化思考事件。 */
function ConversationV1ThinkingCard(props: {
  /** Exact persisted reasoning display item.
   * 精确持久化的思考显示项。 */
  item: Extract<ConversationProcessDisplayItem, { kind: "assistant_reasoning" }>;
  /** Opens one attachment referenced by reasoning Markdown.
   * 打开一条由思考 Markdown 引用的附件。 */
  onOpenMarkdownMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  // t resolves the established thinking-card labels.
  // t 解析既有思考卡文案。
  const { t } = useI18n();
  // streaming determines the historical auto-expand and completion-collapse behavior.
  // streaming 决定历史行为中的流式自动展开与完成后折叠。
  const streaming = props.item.reasoning.status === "streaming";
  // expanded is local presentation state and never becomes a second transcript authority.
  // expanded 是本地展示状态，不会成为第二转录权威。
  const [expanded, setExpanded] = React.useState(streaming);
  // wasStreaming distinguishes initial completed history from one live stream that just terminated.
  // wasStreaming 区分初始已完成历史与刚刚终止的实时流。
  const wasStreaming = React.useRef(streaming);
  React.useEffect(() => {
    if (streaming) {
      setExpanded(true);
    } else if (wasStreaming.current) {
      setExpanded(false);
    }
    wasStreaming.current = streaming;
  }, [streaming]);
  return (
    <section
      className={`thinking-block${expanded ? " expanded" : ""}`}
      data-event-kind="assistant_reasoning"
      data-seq={props.item.seq}
    >
      <button
        aria-expanded={expanded}
        className="thinking-toggle"
        onClick={() => setExpanded((current) => !current)}
        type="button"
      >
        <span className="thinking-toggle-label">
          <BrainCircuit aria-hidden="true" size={15} />
          <strong>{t("contextCompaction.thinkingLabel")}</strong>
          {streaming ? <span>{t("contextCompaction.thinkingActive")}</span> : null}
        </span>
        {expanded ? (
          <ChevronDown aria-hidden="true" className="tool-presentation-toggle thinking-toggle-icon" size={15} />
        ) : (
          <ChevronRight aria-hidden="true" className="tool-presentation-toggle thinking-toggle-icon" size={15} />
        )}
      </button>
      <div className="thinking-body" hidden={!expanded}>
        <div className="thinking-content">
          <MarkdownMessage
            content={props.item.reasoning.text}
            eager
            onOpenMedia={props.onOpenMarkdownMedia}
            streaming={streaming}
          />
        </div>
      </div>
    </section>
  );
}

/** Renders one final assistant answer and attaches matching API metadata below it.
 * 渲染一条最终 assistant 回答，并在其下方附着匹配的 API 元信息。 */
function ConversationV1AssistantView(
  props: ConversationV1TranscriptNodeViewProps & { node: ConversationAssistantTranscriptNode },
) {
  const { locale, t } = useI18n();
  // copied controls the historical copy/check icon feedback for this exact final answer.
  // copied 控制该精确最终回答既有的复制/确认图标反馈。
  const [copied, setCopied] = React.useState(false);

  /** Copies the exact persisted final assistant Markdown.
   * 复制精确持久化的最终 assistant Markdown。 */
  const handleCopy = React.useCallback(async () => {
    if (!props.onCopyText) return;
    await props.onCopyText(props.node.item.markdown);
    setCopied(true);
  }, [props.node.item.markdown, props.onCopyText]);
  return (
    <article
      className="message-row assistant assistant-final-output conversation-v1-event"
      data-event-kind="assistant_text"
      data-seq={props.node.seq}
    >
      <div className="assistant-flow">
        <MarkdownMessage
          content={props.node.item.markdown}
          eager
          onOpenMedia={props.onOpenMarkdownMedia}
          streaming={props.node.item.assistant.status === "streaming"}
        />
        <div className="assistant-final-meta">
          {props.onCopyText ? (
            <button
              aria-label={t("conversation.action.copyFinal")}
              className="assistant-final-meta-button"
              onClick={() => void handleCopy()}
              onMouseLeave={() => setCopied(false)}
              title={t("conversation.action.copyFinal")}
              type="button"
            >
              {copied ? <Check aria-hidden="true" size={13} /> : <Copy aria-hidden="true" size={13} />}
            </button>
          ) : null}
          {props.node.turn_end_seq !== null && props.onRootInputGenerationAction ? (
            <button
              aria-label={t("conversation.action.fork")}
              className="assistant-final-meta-button"
              disabled={props.rootInputGenerationActionsDisabled}
              onClick={() => props.onRootInputGenerationAction?.({
                kind: "fork",
                sourceTurnEndSeq: props.node.turn_end_seq!,
              })}
              title={t("conversation.action.fork")}
              type="button"
            >
              <Split aria-hidden="true" size={13} />
            </button>
          ) : null}
          <time dateTime={new Date(props.node.item.created_at_ms).toISOString()}>
            {formatConversationTime(props.node.item.created_at_ms, locale)}
          </time>
        </div>
      </div>
    </article>
  );
}

/** Renders one terminal Turn statistics node as the established bordered row.
 * 将一条终态 Turn 统计节点渲染为既有带边框统计行。 */
function ConversationV1TurnStatsView(props: { node: ConversationTurnStatsTranscriptNode }) {
  return <ConversationV1TurnStatsCard node={props.node} />;
}

/** Renders one explicit request-configuration transition without reading current settings.
 * 在不读取当前设置的情况下渲染一条明确的请求配置切换。 */
function RequestConfigChangedEventView(props: { item: RequestConfigChangedDisplayItem; providers?: readonly ProviderSummary[] }) {
  // t localizes the exact category reported by the persisted change fields.
  // t 本地化由持久化变化字段确定的精确分类。
  const { t } = useI18n();
  // changeKind prevents a thinking-only transition from masquerading as a model route switch.
  // changeKind 防止仅思考强度变化被伪装成模型路由切换。
  const changeKind = requestConfigChangeKind(props.item.request_config_changed);
  if (changeKind === "none") {
    return null;
  }
  if (changeKind === "route") {
    // summary resolves only provider/model display names, never connection identifiers.
    // summary 仅解析供应商和模型显示名称，绝不展示连接标识。
    const summary = t("conversation.modelSwitch.changed")
      .replaceAll("{previous}", modelSwitchRouteLabel(props.item.request_config_changed.previous, props.providers ?? []))
      .replaceAll("{current}", modelSwitchRouteLabel(props.item.request_config_changed.current, props.providers ?? []));
    return <ModelSwitchNotice summary={summary} seq={props.item.seq} />;
  }
  // previousLabel is derived only from the persisted previous request configuration.
  // previousLabel 仅从持久化的上一请求配置派生。
  const previousLabel = props.item.request_config_changed.previous.thinking_level ?? "—";
  // currentLabel is derived only from the persisted current request configuration.
  // currentLabel 仅从持久化的当前请求配置派生。
  const currentLabel = props.item.request_config_changed.current.thinking_level ?? "—";
  return (
    <section
      className="conversation-v1-event conversation-v1-request-config"
      data-event-kind="request_config_changed"
      data-seq={props.item.seq}
    >
      <strong>{t("conversation.reasoningSwitch.title")}</strong>
      <span>{previousLabel} → {currentLabel}</span>
    </section>
  );
}

/** Renders one fixed display-note family through the shared status-card surface.
 * 通过共享状态卡界面渲染一条固定 display-note 事件。 */
function ConversationV1DisplayNoteView(props: { node: ConversationDisplayNoteTranscriptNode }) {
  const event = props.node.item.event;
  if (event.notice_kind === "goal_status") {
    return <GoalStatusNotice reasonCode={event.reason_code} seq={props.node.seq} />;
  }
  return (
    <aside
      className={`conversation-v1-display-note conversation-v1-display-note-${event.severity}`}
      data-event-kind="display_note"
      data-note-severity={event.severity}
      data-seq={props.node.seq}
    >
      <strong>{displayNoteLabel(event.notice_kind)}</strong>
      {event.redacted_detail ? <span>{event.redacted_detail}</span> : null}
    </aside>
  );
}

/** Collects all event-owned ResourceIds in canonical seq order without legacy lookups.
 * 在不查询旧记录的情况下按 canonical seq 顺序收集全部事件自有 ResourceId。
 *
 * @param events complete canonical V1 window.
 *               完整 canonical V1 窗口。
 * @returns resource identities in event and attachment order.
 *          按事件与附件顺序排列的资源身份。 */
export function collectConversationResourceIdsV1(
  events: readonly ConversationEventSummary[],
): ResourceId[] {
  const values: ResourceId[] = [];
  for (const event of events) {
    if (event.event.kind === "user_input") {
      values.push(...(event.event.resource_ids ?? []));
    } else if (event.event.kind === "tool_call") {
      for (const generatedResource of event.event.generated_resources ?? []) {
        values.push(generatedResource.resource_id);
        if (generatedResource.preview_resource_id !== undefined) {
          values.push(generatedResource.preview_resource_id);
        }
      }
    }
  }
  return values;
}

/** Formats one persisted event timestamp in the current interface locale.
 * 使用当前界面语言格式化一条持久化事件时间戳。 */
function formatConversationTime(timestampMs: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(timestampMs));
}

/** Maps one fixed display-note kind to user-facing copy without leaking protocol identifiers.
 * 将固定 display-note 类型映射为用户文案，同时不泄漏协议标识。 */
function displayNoteLabel(
  kind: ConversationDisplayNoteTranscriptNode["item"]["event"]["notice_kind"],
): string {
  switch (kind) {
    case "provider_error": return "服务请求失败";
    case "system_error": return "系统错误";
    case "network_disconnected": return "网络连接已断开";
    case "balance_insufficient": return "余额不足";
    case "retry_started": return "正在重试";
    case "retry_exhausted": return "重试失败";
    case "resumed": return "已继续";
    case "compaction_completed": return "上下文压缩完成";
    case "goal_status": return "目标状态已更新";
    case "interrupted": return "已中断";
    case "stopped": return "已停止";
  }
}
