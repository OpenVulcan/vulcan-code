import type {
  ServiceEventHandler,
  ServiceRequestOptions,
  ServiceUnsubscribe,
} from "@/service/protocol";
import type { ServiceClient } from "@/service/client";
import type { AnswerModePreference } from "@/service/answer-mode";
import type { PermissionMode, UnattendedConfig } from "@/service/permission-mode";
import type {
  ConversationEventPatchDurability,
  ConversationEventLivePatch,
  ConversationEventReconnectPatch,
  ConversationEventWindowBatch,
  ConversationEventWindowBaseline,
  ConversationEventWindowOwner,
} from "@/service/conversation-event-window-store";

/** RPC method for one initial V1 event-window hydration.
 * 单次 V1 事件窗口首次 hydration 的 RPC 方法。 */
export const SESSION_EVENT_HYDRATION_V1_METHOD = "service_session_event_hydration_v1";

/** RPC method for one adjacent V1 historical page.
 * 单次相邻 V1 历史分页的 RPC 方法。 */
export const SESSION_EVENT_PAGE_V1_METHOD = "service_session_event_page_v1";

/** RPC method for one duplicate-safe V1 reconnect delta.
 * 单次可安全去重 V1 重连增量的 RPC 方法。 */
export const SESSION_EVENT_RECONNECT_V1_METHOD = "service_session_event_reconnect_v1";

/** Service topic carrying volatile or durable canonical V1 conversation-event patches.
 * 承载瞬态或可靠规范 V1 会话事件 patch 的服务主题。*/
export const SESSION_EVENT_DELTA_V1_TOPIC = "conversation.event.delta.v1";

/** Current or next-safe-boundary Provider request configuration from V1 metadata.
 * 来自 V1 metadata 的当前或下一安全边界 Provider 请求配置。 */
export interface ConversationRequestConfigV1 {
  provider: string | null;
  connection_id: string | null;
  model: string | null;
  thinking_level: string | null;
  temperature: number | null;
  set_at_ms: number | null;
}

/** Selected-session configuration whose sole authority is V1 metadata.config.
 * 以 V1 metadata.config 为唯一权威的选中会话配置。 */
export interface ConversationSessionConfigV1 {
  /** Persisted answer mode from Core metadata.
   * 来自 Core 元数据的持久回答模式。 */
  answer_mode: AnswerModePreference;
  /** Persisted session permission mode from Core metadata.
   * 来自 Core 元数据的持久会话权限模式。 */
  permission_mode: PermissionMode;
  /** Unattended mode from the same access selections; absent while it is off.
   * 来自同一份授权选择的无人值守模式；关闭时缺省。 */
  unattended?: UnattendedConfig;
  current_request: ConversationRequestConfigV1;
  next_request: ConversationRequestConfigV1 | null;
  context_compaction_window_node_id: string | null;
  /** Dedicated-session profile from Core metadata; null for regular sessions.
   * 来自 Core 元数据的专职会话配置；普通会话为 null。 */
  dedicated: DedicatedSessionProfileV1 | null;
}

/** Tool access fixed when a dedicated session is created.
 * 专职会话创建时固定的工具访问级别。 */
export type DedicatedToolAccess = "none" | "standard";

/** One memory-lock span; both endpoints are the seq of the lock/unlock notes.
 * 一段记忆锁定区间；端点是锁定/解锁通知的 seq。 */
export interface DedicatedLockedSpanV1 {
  locked_at_seq: number;
  unlocked_at_seq: number | null;
}

/** Immutable dedicated-session profile plus its memory-lock history.
 * 不可变的专职会话配置及其记忆锁定历史。 */
export interface DedicatedSessionProfileV1 {
  tool_access: DedicatedToolAccess;
  base_prompt: string;
  locked_spans: DedicatedLockedSpanV1[];
}

/** Common V1 response fields shared by authoritative hydration and historical paging.
 * 权威 hydration 与历史分页共享的 V1 响应字段。 */
export interface ConversationEventWindowV1BaseResponse extends ConversationEventWindowBatch {
  /** Exclusive cursor for the immediately older complete-Turn page.
   * 紧邻更早完整 Turn 分页的排他游标。 */
  older_before_seq: number | null;
  /** Whether another immediately older complete-Turn page exists.
   * 是否存在另一页紧邻更早的完整 Turn。 */
  has_more: boolean;
  /** Current open logical Turn identity, when any.
   * 当前开放逻辑 Turn 身份（如有）。 */
  open_turn_id: number | null;
  /** Exact start seq of the current open logical Turn, when any.
   * 当前开放逻辑 Turn 的精确起始 seq（如有）。 */
  open_turn_start_seq: number | null;
  /** Current/next request and compaction selection read from V1 metadata.
   * 从 V1 metadata 读取的当前/下一请求与压缩选择。 */
  session_config: ConversationSessionConfigV1;
}

/** Authoritative hydration/rebase response carrying exact revision frontiers.
 * 携带精确修订前沿的权威 hydration/rebase 响应。 */
export interface ConversationEventHydrationV1Response
  extends ConversationEventWindowV1BaseResponse,
    ConversationEventWindowBaseline {}

/** Historical page response that is structurally unable to reset live frontiers.
 * 结构上无法重置实时前沿的历史分页响应。 */
export interface ConversationEventPageV1Response extends ConversationEventWindowV1BaseResponse {
  /** Historical pages never carry a memory frontier.
   * 历史分页绝不携带内存前沿。 */
  memory_revision?: never;
  /** Historical pages never carry a durable frontier.
   * 历史分页绝不携带可靠前沿。 */
  durable_revision?: never;
}

/** Rust V1 window response union retained for transport and test fixtures.
 * 为传输与测试夹具保留的 Rust V1 窗口响应联合。 */
export type ConversationEventWindowV1Response =
  | ConversationEventHydrationV1Response
  | ConversationEventPageV1Response;

/** Live and reconnect DTO carrying sparse events in the exact hydration summary shape.
 * 以与 hydration 摘要完全相同的形态携带稀疏事件的实时与重连 DTO。 */
/** Durability frontier represented by one transport-level event patch revision.
 * 单个传输层事件 patch 修订所表示的持久化前沿。
 */
export type ConversationEventPatchDurabilityV1 = ConversationEventPatchDurability;

/** Live and reconnect DTO carrying sparse events in the exact hydration summary shape.
 * 以与 hydration 摘要完全相同的形态携带稀疏事件的实时与重连 DTO。
 */
export type ConversationEventLiveDeltaV1 = ConversationEventLivePatch;

/** Reconnect response with explicit client base and authoritative memory/durable targets.
 * 携带显式客户端前驱及权威内存/可靠目标的重连响应。 */
/** Exact reason why incremental reconnect cannot safely repair the current window.
 * 增量重连无法安全修复当前窗口的精确原因。 */
export type ConversationEventRehydrateReasonV1 =
  | "service_epoch_changed"
  | "window_generation_changed"
  | "memory_revision_ahead"
  | "cursor_ahead";

/** Authoritative reconnect disposition produced by the Rust service contract.
 * Rust 服务契约产生的权威重连处置。 */
export type ConversationEventReconnectDispositionV1 =
  | { readonly kind: "recovery" }
  | {
    readonly kind: "rehydrate_required";
    readonly reason: ConversationEventRehydrateReasonV1;
  };

/** Reconnect response with explicit frontiers and a recovery-or-rehydrate decision.
 * 携带显式前沿以及恢复或重新 hydration 决策的重连响应。 */
export interface ConversationEventReconnectV1Response extends ConversationEventReconnectPatch {
  /** Whether the included range is applicable or a fresh hydration is mandatory.
   * 所含范围可以应用，还是必须重新 hydration。 */
  readonly disposition: ConversationEventReconnectDispositionV1;
}

/** Transport-neutral V1 event-window RPC contract used by the controller.
 * 控制器使用的传输无关 V1 事件窗口 RPC 契约。 */
export interface ConversationEventWindowRpc {
  /** Loads the latest complete-Turn window for one session.
   * 加载一个会话最近的完整 Turn 窗口。 */
  hydrate(
    sessionId: string,
    options?: ServiceRequestOptions,
  ): Promise<ConversationEventHydrationV1Response>;
  /** Loads one complete-Turn page immediately before the supplied cursor.
   * 加载给定游标之前紧邻的一页完整 Turn。 */
  page(
    owner: ConversationEventWindowOwner,
    beforeSeq: number,
    options?: ServiceRequestOptions,
  ): Promise<ConversationEventPageV1Response>;
  /** Loads the duplicate-safe range beginning at the last confirmed seq.
   * 加载从最后确认 seq 开始的可安全去重范围。 */
  reconnect(
    owner: ConversationEventWindowOwner,
    lastConfirmedSeq: number | null,
    lastConfirmedMemoryRevision: number,
    serviceEpoch: string,
    options?: ServiceRequestOptions,
  ): Promise<ConversationEventReconnectV1Response>;
  /** Subscribes to volatile mutation patches and duplicate-safe durable confirmations.
   * 订阅瞬态内存变更 patch 与可安全去重的可靠确认。*/
  subscribeLive(
    handler: ServiceEventHandler<ConversationEventLiveDeltaV1>,
  ): Promise<ServiceUnsubscribe>;
}

/** Strongly typed V1 client over the transport-neutral service facade.
 * 建立在传输无关服务门面上的强类型 V1 客户端。 */
export class ServiceConversationEventWindowClient implements ConversationEventWindowRpc {
  /** Generic service facade that owns the actual HTTP or native transport.
   * 持有实际 HTTP 或原生传输的通用服务门面。 */
  private readonly service: Pick<ServiceClient, "request" | "subscribe">;

  /** Creates one V1 event-window client over an existing service facade.
   * 基于既有服务门面创建一个 V1 事件窗口客户端。 */
  constructor(service: Pick<ServiceClient, "request" | "subscribe">) {
    this.service = service;
  }

  /** Loads the latest 30 complete Turns plus an entire open Turn.
   * 加载最近 30 个完整 Turn 及整个开放 Turn。 */
  hydrate(
    sessionId: string,
    options?: ServiceRequestOptions,
  ): Promise<ConversationEventHydrationV1Response> {
    return this.service.request<ConversationEventHydrationV1Response>(
      SESSION_EVENT_HYDRATION_V1_METHOD,
      { sessionId },
      options,
    );
  }

  /** Loads one immutable recent window from an exact embedded TaskSession file without runtime admission.
   * 从精确嵌入式 TaskSession 文件加载一份不可变最近窗口，且不执行运行时准入。 */
  hydrateEmbeddedTaskSnapshot(
    parentSessionId: string,
    sessionId: string,
    options?: ServiceRequestOptions,
  ): Promise<ConversationEventHydrationV1Response> {
    return this.service.request<ConversationEventHydrationV1Response>(
      SESSION_EVENT_HYDRATION_V1_METHOD,
      { sessionId, parentSessionId },
      options,
    );
  }

  /** Loads one historical page bound to the current owner generation.
   * 加载绑定到当前归属代次的一页历史记录。 */
  page(
    owner: ConversationEventWindowOwner,
    beforeSeq: number,
    options?: ServiceRequestOptions,
  ): Promise<ConversationEventPageV1Response> {
    return this.service.request<ConversationEventPageV1Response>(
      SESSION_EVENT_PAGE_V1_METHOD,
      {
        sessionId: owner.session_id,
        windowGeneration: owner.window_generation,
        beforeSeq,
      },
      options,
    );
  }

  /** Loads one duplicate-safe reconnect delta for the current owner.
   * 为当前归属加载一份可安全去重的重连增量。 */
  reconnect(
    owner: ConversationEventWindowOwner,
    lastConfirmedSeq: number | null,
    lastConfirmedMemoryRevision: number,
    serviceEpoch: string,
    options?: ServiceRequestOptions,
  ): Promise<ConversationEventReconnectV1Response> {
    return this.service.request<ConversationEventReconnectV1Response>(
      SESSION_EVENT_RECONNECT_V1_METHOD,
      {
        sessionId: owner.session_id,
        windowGeneration: owner.window_generation,
        lastConfirmedSeq,
        lastConfirmedMemoryRevision,
        serviceEpoch,
      },
      options,
    );
  }

  /** Subscribes to the production V1 live topic without rebuilding event DTOs.
   * 订阅生产 V1 实时主题，且不重建事件 DTO。*/
  subscribeLive(
    handler: ServiceEventHandler<ConversationEventLiveDeltaV1>,
  ): Promise<ServiceUnsubscribe> {
    return this.service.subscribe(SESSION_EVENT_DELTA_V1_TOPIC, handler);
  }
}

/** Creates the strongly typed V1 client used by the Stage 8 test host.
 * 创建阶段 8 测试宿主使用的强类型 V1 客户端。 */
export function createConversationEventWindowClient(
  service: Pick<ServiceClient, "request" | "subscribe">,
): ServiceConversationEventWindowClient {
  return new ServiceConversationEventWindowClient(service);
}
