import React from "react";

import { MarkdownImageWorkspacePathProvider, MarkdownMessage } from "./conversation-markdown";
import { ConversationV1ResourceList } from "./conversation-v1-resource-list";
import { ConversationV1ToolCard } from "./conversation-v1-tool-card";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import {
  projectConversationEventDisplay,
  visibleConversationEventDisplayItems,
  type AuxiliaryEventDisplayItem,
  type ConversationEventDisplayItem,
  type RequestConfigChangedDisplayItem,
} from "@/service/conversation-event-display-projection";
import type { ConversationEventWindowController } from "@/service/conversation-event-window-controller";
import type {
  ConversationEventBody,
  ConversationEventSummary,
  ExternalResourceRef,
  ResourceId,
  UsageSnapshot,
} from "@/service/conversation-event-window-store";
import type { SessionResourceRpcV1 } from "@/service/conversation-session-resources-v1";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import { useI18n } from "@/i18n";
import { useConversationEventWindowSnapshot } from "@/state/conversation-event-window-react";
import {
  useConversationSessionResourcesV1,
  type ConversationSessionResourceCatalogV1,
} from "@/state/conversation-session-resources-v1";

/** Root-input generation action emitted directly from one V1 user-input event.
 * 直接从一条 V1 用户输入事件发出的根输入 generation 动作。
 */
export interface ConversationV1RootInputGenerationAction {
  /** Requested immutable action kind.
   * 请求的不可变动作类型。
   */
  kind: "edit" | "fork";
  /** Persisted root-input seq used as the only action anchor.
   * 作为唯一动作锚点的持久化根输入 seq。
   */
  seq: number;
  /** Persisted root-input text shown by the action dialog.
   * 动作对话框展示的持久化根输入正文。
   */
  text: string;
}

/** Props for the V1 event-native conversation renderer.
 * V1 事件原生会话渲染器的属性。 */
export interface ConversationV1DisplayRendererProps {
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
  /** Owner-bound project resource metadata for persisted user-input attachments.
   * 持久化用户输入附件所使用的绑定所有者项目资源元数据。 */
  resourceCatalog?: ConversationSessionResourceCatalogV1 | null;
  /** Persisted reasoning presentation mode selected for the current session.
   * 当前会话选择的持久化思考展示模式。 */
  reasoningDisplayMode?: ThinkingDisplayMode;
  /** Receives edit or fork actions anchored only by one V1 root-input seq.
   * 接收仅由一条 V1 根输入 seq 锚定的编辑或分叉动作。
   */
  onRootInputGenerationAction?: (action: ConversationV1RootInputGenerationAction) => void;
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
      defaultExpandedTools={props.defaultExpandedTools}
      events={snapshot.events}
      availableExternalRefKeys={props.availableExternalRefKeys}
      onOpenExternalRef={props.onOpenExternalRef}
      onOpenMarkdownMedia={props.onOpenMarkdownMedia}
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
      defaultExpandedTools={props.defaultExpandedTools}
      events={props.events}
      availableExternalRefKeys={props.availableExternalRefKeys}
      onOpenExternalRef={props.onOpenExternalRef}
      onOpenMarkdownMedia={props.onOpenMarkdownMedia}
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
  // displayItems validates the complete sequence before HostOnly events are filtered.
  // displayItems 在过滤 HostOnly 事件前校验完整序列。
  const displayItems = React.useMemo(
    () => conversationDisplayItemsForReasoningMode(
      visibleConversationEventDisplayItems(projectConversationEventDisplay(props.events)),
      props.reasoningDisplayMode ?? "inline_output",
    ),
    [props.events, props.reasoningDisplayMode],
  );
  return (
    <MarkdownImageWorkspacePathProvider value={props.workspacePath}>
      <div className="conversation-v1-display" data-conversation-v1-display="true">
        {displayItems.map((item) => (
          <ConversationV1DisplayItem
            defaultExpandedTools={props.defaultExpandedTools}
            item={item}
            availableExternalRefKeys={props.availableExternalRefKeys}
            key={item.seq}
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

/** Props for rendering one already visibility-filtered display item.
 * 渲染一条已经完成可见性过滤的展示项所需属性。 */
interface ConversationV1DisplayItemProps {
  /** Exact event-native display item.
   * 精确的事件原生展示项。 */
  item: ConversationEventDisplayItem;
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
  /** Optional owner-bound resource catalog for user-input attachments.
   * 可选的用户输入附件绑定所有者资源目录。 */
  resourceCatalog?: ConversationSessionResourceCatalogV1 | null;
  /** Optional V1 root-input action callback.
   * 可选的 V1 根输入动作回调。
   */
  onRootInputGenerationAction?: (action: ConversationV1RootInputGenerationAction) => void;
  /** Whether root-input actions are temporarily disabled.
   * 根输入动作当前是否暂时禁用。
   */
  rootInputGenerationActionsDisabled?: boolean;
}

/** Renders one visible event using its discriminated V1 kind.
 * 使用一条可见事件的 V1 判别类型进行渲染。 */
function ConversationV1DisplayItem(props: ConversationV1DisplayItemProps) {
  const { item } = props;
  switch (item.kind) {
    case "assistant_reasoning":
      return (
        <details
          className="conversation-v1-event conversation-v1-reasoning"
          data-event-kind="assistant_reasoning"
          data-seq={item.seq}
          open={item.reasoning.status === "streaming"}
        >
          <summary>思考过程 · {item.reasoning.status}</summary>
          <MarkdownMessage
            content={item.reasoning.text}
            eager
            onOpenMedia={props.onOpenMarkdownMedia}
            streaming={item.reasoning.status === "streaming"}
          />
        </details>
      );
    case "assistant_text":
      return (
        <article
          className="conversation-v1-event conversation-v1-assistant-text"
          data-event-kind="assistant_text"
          data-seq={item.seq}
        >
          <MarkdownMessage
            content={item.markdown}
            eager
            onOpenMedia={props.onOpenMarkdownMedia}
            streaming={item.assistant.status === "streaming"}
          />
        </article>
      );
    case "tool_call":
      return (
        <div className="conversation-v1-event" data-event-kind="tool_call" data-seq={item.seq}>
          <ConversationV1ToolCard
            availableExternalRefKeys={props.availableExternalRefKeys}
            defaultExpanded={props.defaultExpandedTools}
            item={item}
            onOpenExternalRef={props.onOpenExternalRef}
            resourceCatalog={props.resourceCatalog}
          />
        </div>
      );
    case "request_config_changed":
      return <RequestConfigChangedEventView item={item} />;
    case "event":
      return (
        <AuxiliaryEventView
          item={item}
          onOpenMarkdownMedia={props.onOpenMarkdownMedia}
          onRootInputGenerationAction={props.onRootInputGenerationAction}
          resourceCatalog={props.resourceCatalog}
          rootInputGenerationActionsDisabled={props.rootInputGenerationActionsDisabled}
        />
      );
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

/** Renders one explicit request-configuration transition without reading current settings.
 * 在不读取当前设置的情况下渲染一条明确的请求配置切换。 */
function RequestConfigChangedEventView(props: { item: RequestConfigChangedDisplayItem }) {
  // previousLabel is derived only from the persisted previous request configuration.
  // previousLabel 仅从持久化的上一请求配置派生。
  const previousLabel = requestConfigLabel(props.item.request_config_changed.previous);
  // currentLabel is derived only from the persisted current request configuration.
  // currentLabel 仅从持久化的当前请求配置派生。
  const currentLabel = requestConfigLabel(props.item.request_config_changed.current);
  return (
    <section
      className="conversation-v1-event conversation-v1-request-config"
      data-event-kind="request_config_changed"
      data-seq={props.item.seq}
    >
      <strong>请求配置已切换</strong>
      <span>{previousLabel} → {currentLabel}</span>
    </section>
  );
}

/** Renders one auxiliary V1 event exhaustively by its persisted kind.
 * 按持久化类型穷尽渲染一条辅助 V1 事件。 */
function AuxiliaryEventView(props: {
  item: AuxiliaryEventDisplayItem;
  onOpenMarkdownMedia?: (attachment: GeneratedMediaAttachment) => void;
  resourceCatalog?: ConversationSessionResourceCatalogV1 | null;
  onRootInputGenerationAction?: (action: ConversationV1RootInputGenerationAction) => void;
  rootInputGenerationActionsDisabled?: boolean;
}) {
  const { event } = props.item;
  switch (event.kind) {
    case "system_context":
      return (
        <section {...eventContainerProps(props.item, event.kind)} data-context-operation={event.operation}>
          <strong>系统上下文 · {event.context_key}</strong>
          <pre>{event.content}</pre>
        </section>
      );
    case "user_input":
      return (
        <article {...eventContainerProps(props.item, event.kind)} data-input-id={event.input_id}>
          <p>{event.text}</p>
          <ConversationV1ResourceList
            catalog={props.resourceCatalog ?? null}
            resourceIds={event.resource_ids ?? []}
          />
          {event.input_source === "root" && props.onRootInputGenerationAction ? (
            <ConversationV1RootInputActions
              disabled={props.rootInputGenerationActionsDisabled}
              onAction={props.onRootInputGenerationAction}
              seq={props.item.seq}
              text={event.text}
            />
          ) : null}
        </article>
      );
    case "queued_input_resolution":
      // Queued-input receipts drive composer metadata and intentionally have no timeline card.
      // 排队输入处置 receipt 仅驱动输入区 metadata，刻意不生成时间线卡片。
      return null;
    case "api_started":
      return (
        <section {...eventContainerProps(props.item, event.kind)}>
          API {event.api_call_id} · {event.purpose} · {event.request.model}
        </section>
      );
    case "api_stats":
      return (
        <section {...eventContainerProps(props.item, event.kind)} data-api-status={event.status}>
          <strong>{event.provider} · {event.model} · {event.status}</strong>
          <span>{usageLabel(event.usage)}</span>
          {event.error ? <small>{event.error.category}: {event.error.message}</small> : null}
        </section>
      );
    case "turn_boundary":
      return <section {...eventContainerProps(props.item, event.kind)}>回合结束 · {event.outcome}</section>;
    case "turn_stats":
      return (
        <section {...eventContainerProps(props.item, event.kind)}>
          回合统计 · API {event.buckets.total.api_call_count} · 工具 {event.buckets.total.tool_call_count}
        </section>
      );
    case "subagent_return":
      return (
        <section {...eventContainerProps(props.item, event.kind)} data-child-session-id={event.child_session_id}>
          <MarkdownMessage content={event.content} eager onOpenMedia={props.onOpenMarkdownMedia} />
        </section>
      );
    case "subagent_return_stats":
      return (
        <section {...eventContainerProps(props.item, event.kind)} data-child-session-id={event.child_session_id}>
          子代理统计 · {event.status} · API {event.totals.api_call_count}
        </section>
      );
    case "display_note":
      return (
        <aside {...eventContainerProps(props.item, event.kind)} data-note-severity={event.severity}>
          <strong>{event.notice_kind}</strong>
          <span>{event.redacted_detail ?? event.reason_code}</span>
        </aside>
      );
    case "execution_control":
      return <section {...eventContainerProps(props.item, event.kind)}>{event.control_kind}</section>;
    default:
      return assertNeverAuxiliaryEvent(event);
  }
}

/** Props for actions owned by one exact V1 root-input event.
 * 一条精确 V1 根输入事件所拥有动作的属性。
 */
interface ConversationV1RootInputActionsProps {
  /** Persisted root-input seq.
   * 持久化根输入 seq。
   */
  seq: number;
  /** Persisted root-input text.
   * 持久化根输入正文。
   */
  text: string;
  /** Whether both generation actions are unavailable.
   * 两个 generation 动作是否不可用。
   */
  disabled?: boolean;
  /** Receives the exact typed action.
   * 接收精确强类型动作。
   */
  onAction: (action: ConversationV1RootInputGenerationAction) => void;
}

/** Renders localized edit and fork actions for one canonical root input.
 * 为一条 canonical 根输入渲染本地化编辑与分叉动作。
 *
 * @param props exact event identity and action callback.
 *              精确事件身份与动作回调。
 * @returns the localized root-input action row.
 *          本地化根输入动作行。
 */
function ConversationV1RootInputActions(props: ConversationV1RootInputActionsProps) {
  // t resolves the existing root-input edit and fork labels only when actions are visible.
  // t 仅在动作可见时解析既有根输入编辑与分叉文案。
  const { t } = useI18n();
  return (
    <div className="conversation-v1-root-input-actions">
      <button
        disabled={props.disabled}
        onClick={() => props.onAction({ kind: "edit", seq: props.seq, text: props.text })}
        type="button"
      >
        {t("conversation.userPrompt.edit")}
      </button>
      <button
        disabled={props.disabled}
        onClick={() => props.onAction({ kind: "fork", seq: props.seq, text: props.text })}
        type="button"
      >
        {t("conversation.action.fork")}
      </button>
    </div>
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

/** Builds shared machine-readable attributes without changing event order.
 * 在不改变事件顺序的情况下构建共享的机器可读属性。 */
function eventContainerProps(item: AuxiliaryEventDisplayItem, kind: ConversationEventBody["kind"]) {
  return {
    className: "conversation-v1-event conversation-v1-auxiliary-event",
    "data-event-kind": kind,
    "data-seq": item.seq,
  };
}

/** Formats one persisted request configuration without consulting mutable global state.
 * 在不查询可变全局状态的情况下格式化一份持久化请求配置。 */
function requestConfigLabel(config: RequestConfigChangedDisplayItem["request_config_changed"]["current"]): string {
  return [config.provider, config.model, config.thinking_level].filter(Boolean).join(" · ");
}

/** Formats only token dimensions explicitly present in one Provider usage snapshot.
 * 仅格式化一份 Provider 用量快照中明确存在的 token 维度。 */
function usageLabel(usage: UsageSnapshot): string {
  // parts retains unknown dimensions as absent instead of estimating them.
  // parts 将未知维度保持为缺失，而不是进行估算。
  const parts: string[] = [];
  if (usage.input_tokens !== undefined) parts.push(`输入 ${usage.input_tokens}`);
  if (usage.output_tokens !== undefined) parts.push(`输出 ${usage.output_tokens}`);
  if (usage.reasoning_tokens !== undefined) parts.push(`思考 ${usage.reasoning_tokens}`);
  if (usage.total_tokens !== undefined) parts.push(`总计 ${usage.total_tokens}`);
  return parts.join(" · ") || "Token 用量未知";
}

/** Rejects an impossible auxiliary event after TypeScript exhaustiveness checking.
 * 在 TypeScript 穷尽检查后拒绝不可能出现的辅助事件。 */
function assertNeverAuxiliaryEvent(event: never): never {
  throw new Error(`unexpected specialized V1 display event: ${JSON.stringify(event)}`);
}
