import type { ServiceRequestOptions, ServiceUnsubscribe } from "@/service/protocol";
import type {
  ConversationEventHydrationV1Response,
  ConversationEventLiveDeltaV1,
  ConversationEventReconnectV1Response,
  ConversationEventWindowRpc,
  ConversationEventWindowV1BaseResponse,
  ConversationSessionConfigV1,
} from "@/service/conversation-event-window-client";
import {
  ConversationEventWindowStore,
  ConversationEventRevisionGapError,
  ConversationEventServiceEpochMismatchError,
  assertSafeSeq,
  type ConversationEventSummary,
  type ConversationEventWindowOwner,
} from "@/service/conversation-event-window-store";

/** Minimal sink used to hand an ephemeral request draft to a canonical V1 ToolCall.
 * 用于把瞬态请求草稿交接给 canonical V1 ToolCall 的最小 sink。 */
export interface ConversationToolRequestProgressSettlementSink {
  /** Settles one exact request-stream draft after its V1 ToolCall enters the event window.
   * 在对应 V1 ToolCall 进入事件窗口后封口一条精确请求流草稿。 */
  settleToolRequestProgress(conversationId: string, requestStreamId: string): boolean;
}

/** Listener notified after one complete controller snapshot is atomically published.
 * 在一份完整控制器快照完成原子发布后收到通知的监听器。 */
export type ConversationEventWindowControllerListener = () => void;

/** Immutable controller snapshot rendered by the Stage 8 test host.
 * 阶段 8 测试宿主渲染的不可变控制器快照。 */
export interface ConversationEventWindowControllerSnapshot {
  /** Current ownership boundary, or null before hydration.
   * 当前归属边界；hydration 前为空。 */
  owner: ConversationEventWindowOwner | null;
  /** Stable identity of the service process owning the current frontier.
   * 拥有当前前沿的服务进程稳定身份。 */
  service_epoch: string | null;
  /** Latest authoritative in-memory document revision for the owner generation.
   * 当前归属代际最新的权威内存文档修订号。 */
  memory_revision: number | null;
  /** Latest whole-file revision confirmed durable for the owner generation.
   * 当前归属代际最新已确认整文件可靠落盘的修订号。 */
  durable_revision: number | null;
  /** Canonical events in strict seq order.
   * 按严格 seq 顺序排列的 canonical 事件。 */
  events: readonly ConversationEventSummary[];
  /** Exclusive cursor for the immediately older page.
   * 紧邻更早分页的排他游标。 */
  older_before_seq: number | null;
  /** Whether another older complete-Turn page exists.
   * 是否存在另一页更早完整 Turn。 */
  has_more: boolean;
  /** Current open Turn identity, when known.
   * 当前开放 Turn 身份（已知时）。 */
  open_turn_id: number | null;
  /** Exact start seq of the current open Turn, when known.
   * 当前开放 Turn 的精确起始 seq（已知时）。 */
  open_turn_start_seq: number | null;
  /** Selected-session V1 metadata configuration, or null before hydration.
   * 选中会话的 V1 metadata 配置；hydration 前为空。 */
  session_config: ConversationSessionConfigV1 | null;
}

/** Stage 8 controller that applies every service boundary through one atomic
 * ConversationEventWindowStore.
 * 通过同一个原子 ConversationEventWindowStore 应用全部服务边界的阶段 8 控制器。 */
export class ConversationEventWindowController {
  /** Strongly typed service client.
   * 强类型服务客户端。 */
  private readonly rpc: ConversationEventWindowRpc;
  /** Sole in-memory event window.
   * 唯一内存事件窗口。 */
  private readonly store: ConversationEventWindowStore;
  /** Optional low-latency draft store notified only after a V1 boundary commits.
   * 仅在 V1 边界提交后收到通知的可选低延迟草稿 store。 */
  private readonly toolRequestProgressSettlement: ConversationToolRequestProgressSettlementSink | null;
  /** External-store listeners notified only after an observable snapshot changes.
   * 仅在可观察快照发生变化后通知的外部 store 监听器。 */
  private readonly listeners = new Set<ConversationEventWindowControllerListener>();
  /** Stable cached snapshot required by React useSyncExternalStore.
   * React useSyncExternalStore 所需的稳定缓存快照。 */
  private snapshotCache: ConversationEventWindowControllerSnapshot;
  /** Current immediately older page cursor.
   * 当前紧邻更早分页游标。 */
  private olderBeforeSeq: number | null = null;
  /** Whether another older page exists.
   * 是否存在另一页更早记录。 */
  private hasMore = false;
  /** Single in-flight historical prepend shared by duplicate pagination requests.
   * 并发重复分页请求共享的唯一历史前插任务。 */
  private prependOlderPromise: Promise<boolean> | null = null;
  /** Current open Turn identity.
   * 当前开放 Turn 身份。 */
  private openTurnId: number | null = null;
  /** Current open Turn start seq.
   * 当前开放 Turn 起始 seq。 */
  private openTurnStartSeq: number | null = null;
  /** Current selected-session configuration from the latest atomic boundary.
   * 来自最近一次原子边界的当前选中会话配置。 */
  private sessionConfig: ConversationSessionConfigV1 | null = null;
  /** Monotonic hydration request token used to discard delayed old ownership.
   * 用于丢弃延迟旧归属的单调 hydration 请求令牌。 */
  private hydrationRequestToken = 0;
  /** Active production live subscription release callback.
   * 当前生产实时订阅的释放回调。*/
  private liveUnsubscribe: ServiceUnsubscribe | null = null;
  /** In-flight subscription setup shared by concurrent hydration requests.
   * 并发 hydration 请求共享的订阅建立任务。*/
  private liveSubscriptionPromise: Promise<void> | null = null;
  /** Session currently establishing a hydration ownership boundary.
   * 当前正在建立 hydration 归属边界的会话。*/
  private hydratingSessionId: string | null = null;
  /** Ordered live deltas received after subscription but before hydration commits.
   * 订阅后、hydration 提交前收到的有序实时增量。*/
  private pendingHydrationDeltas: ConversationEventLiveDeltaV1[] = [];
  /** Highest edit generation observed while the current owner remains visible.
   * 当前归属窗口保持可见期间观察到的最高编辑代际。 */
  private generationRebaseTarget: ConversationEventWindowOwner | null = null;
  /** Single in-flight hydration that atomically rebases an edited generation.
   * 用于原子重建编辑后代际的唯一进行中 hydration。 */
  private generationRebasePromise: Promise<void> | null = null;
  /** Ordered same-generation pushes held while one revision-gap reconnect is in flight.
   * 一次修订缺口重连期间暂存的同代次有序推送。 */
  private pendingRevisionDeltas: ConversationEventLiveDeltaV1[] = [];
  /** Single in-flight reconnect that repairs a missing revision edge.
   * 用于修复缺失修订边的唯一进行中重连。 */
  private revisionReconnectPromise: Promise<void> | null = null;
  /** New service-process epoch observed before its durable hydration commits.
   * 可靠 hydration 提交前观察到的新服务进程代次。 */
  private serviceEpochRebaseTarget: string | null = null;
  /** Ordered pushes owned by the replacement service while durable hydration is in flight.
   * 新服务的可靠 hydration 进行期间暂存的有序推送。 */
  private pendingServiceEpochDeltas: ConversationEventLiveDeltaV1[] = [];
  /** Single in-flight durable hydration for one service-process replacement.
   * 面向一次服务进程替换的唯一进行中可靠 hydration。 */
  private serviceEpochRebasePromise: Promise<void> | null = null;
  /** Whether the production owner permanently released this controller.
   * 生产归属方是否已经永久释放该控制器。 */
  private disposed = false;

  /** Creates one controller around an RPC client and optional test-owned store.
   * 围绕 RPC 客户端与可选测试自有 store 创建控制器。 */
  constructor(
    rpc: ConversationEventWindowRpc,
    store: ConversationEventWindowStore = new ConversationEventWindowStore(),
    toolRequestProgressSettlement: ConversationToolRequestProgressSettlementSink | null = null,
  ) {
    this.rpc = rpc;
    this.store = store;
    this.toolRequestProgressSettlement = toolRequestProgressSettlement;
    this.snapshotCache = this.buildSnapshot();
  }

  /** Returns one immutable view without rebuilding a message/record model.
   * 返回一份不可变视图，且不重建 message/record 双模型。 */
  snapshot(): ConversationEventWindowControllerSnapshot {
    return this.snapshotCache;
  }

  /** Subscribes to complete atomic snapshot publications.
   * 订阅完整原子快照的发布通知。
   *
   * @param listener callback invoked after the new snapshot becomes readable.
   * @param listener 在新快照可读取后调用的回调。
   * @returns an idempotent unsubscribe callback for this exact listener.
   * @returns 用于当前精确监听器的幂等取消订阅回调。 */
  subscribe(listener: ConversationEventWindowControllerListener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /** Builds one snapshot from the already committed store and boundary fields.
   * 从已经提交的 store 与边界字段构建一份快照。 */
  private buildSnapshot(): ConversationEventWindowControllerSnapshot {
    const frontier = this.store.getFrontier();
    return {
      owner: this.store.getOwner(),
      service_epoch: frontier?.service_epoch ?? null,
      memory_revision: frontier?.memory_revision ?? null,
      durable_revision: frontier?.durable_revision ?? null,
      events: this.store.render(),
      older_before_seq: this.olderBeforeSeq,
      has_more: this.hasMore,
      open_turn_id: this.openTurnId,
      open_turn_start_seq: this.openTurnStartSeq,
      session_config: this.sessionConfig,
    };
  }

  /** Publishes one stable snapshot only when its observable state changed.
   * 仅在可观察状态发生变化时发布一份稳定快照。 */
  private publishSnapshotIfChanged(): void {
    const next = this.buildSnapshot();
    if (sameControllerSnapshot(this.snapshotCache, next)) return;
    this.snapshotCache = next;
    for (const listener of this.listeners) listener();
  }

  /** Establishes initial ownership and events through one atomic store swap.
   * 通过一次原子 store 交换建立首次归属与事件。 */
  async hydrate(sessionId: string, options?: ServiceRequestOptions): Promise<boolean> {
    if (this.disposed) throw new Error("conversation event window controller is disposed");
    const requestToken = ++this.hydrationRequestToken;
    this.hydratingSessionId = sessionId;
    this.pendingHydrationDeltas = [];
    this.generationRebaseTarget = null;
    // liveSubscriptionReady observes physical socket readiness without allowing it to block
    // the durable HTTP hydration that can already render the selected conversation.
    // liveSubscriptionReady 观察物理套接字就绪状态，但不允许它阻塞已经可以渲染选中会话的可靠 HTTP hydration。
    const liveSubscriptionReady = this.ensureLiveSubscription().then(
      () => true,
      (error: unknown) => {
        console.error("[conversation-event-window] live subscription setup failed", error);
        return false;
      },
    );
    try {
      const response = await this.rpc.hydrate(sessionId, options);
      if (requestToken !== this.hydrationRequestToken) return false;
      assertWindowBoundary(response);
      this.store.hydrate(response);
      this.settleToolRequestProgress(response.events, response.session_id);
      this.commitWindowBoundary(response);
      const pending = this.pendingHydrationDeltas;
      this.pendingHydrationDeltas = [];
      this.hydratingSessionId = null;
      for (const delta of pending) this.applyOwnedPushedDelta(delta, false);
      this.publishSnapshotIfChanged();
      if (this.liveUnsubscribe === null) {
        this.reconcileAfterDelayedLiveSubscription(
          requestToken,
          sessionId,
          liveSubscriptionReady,
        );
      }
      return true;
    } catch (error) {
      if (requestToken === this.hydrationRequestToken) {
        this.pendingHydrationDeltas = [];
        this.hydratingSessionId = null;
      }
      throw error;
    }
  }

  /** Refreshes an already-owned session without reusing the initial-only hydration transition.
   * 在不复用仅限首次归属的 hydration 转换前提下刷新已持有的会话。 */
  async rehydrate(sessionId: string, options?: ServiceRequestOptions): Promise<boolean> {
    if (this.disposed) throw new Error("conversation event window controller is disposed");
    const initialOwner = this.store.getOwner();
    if (initialOwner === null) return this.hydrate(sessionId, options);
    if (initialOwner.session_id !== sessionId) {
      throw new Error("conversation event rehydration cannot change the owned session");
    }
    const requestToken = ++this.hydrationRequestToken;
    const response = await this.rpc.hydrate(sessionId, options);
    if (requestToken !== this.hydrationRequestToken) return false;
    assertWindowBoundary(response);
    if (response.session_id !== sessionId) {
      throw new Error("conversation event rehydration returned another session");
    }
    const currentOwner = this.store.getOwner();
    const currentFrontier = this.store.getFrontier();
    if (currentOwner === null || currentFrontier === null) {
      throw new Error("conversation event window owner disappeared during rehydration");
    }
    if (currentOwner.session_id !== sessionId) return false;
    if (response.window_generation < currentOwner.window_generation) return true;

    if (response.window_generation > currentOwner.window_generation) {
      this.store.rebase(response);
      this.generationRebaseTarget = null;
    } else {
      const serviceEpochChanged = response.service_epoch !== currentFrontier.service_epoch;
      const frontierAdvanced = response.memory_revision > currentFrontier.memory_revision
        || response.durable_revision > currentFrontier.durable_revision;
      if (!serviceEpochChanged && !frontierAdvanced) return true;
      this.store.restoreDurableBaseline(response);
    }
    this.settleToolRequestProgress(response.events, response.session_id);
    this.commitWindowBoundary(response);
    this.publishSnapshotIfChanged();
    return true;
  }

  /** Prepends one adjacent historical page without clearing current events.
   * 在不清空当前事件的情况下前插一页相邻历史。 */
  async prependOlder(options?: ServiceRequestOptions): Promise<boolean> {
    if (this.disposed) throw new Error("conversation event window controller is disposed");
    if (this.prependOlderPromise !== null) return this.prependOlderPromise;
    const owner = this.requireOwner();
    if (!this.hasMore || this.olderBeforeSeq === null) return false;
    const requestedCursor = this.olderBeforeSeq;
    const prependOlderPromise = this.loadOlderPage(owner, requestedCursor, options);
    this.prependOlderPromise = prependOlderPromise;
    try {
      return await prependOlderPromise;
    } finally {
      if (this.prependOlderPromise === prependOlderPromise) this.prependOlderPromise = null;
    }
  }

  /** Requests and atomically applies duplicate-safe reconnect events.
   * 请求并原子应用可安全去重的重连事件。 */
  async reconnect(options?: ServiceRequestOptions): Promise<void> {
    const owner = this.requireOwner();
    const frontier = this.store.getFrontier();
    if (frontier === null) throw new Error("conversation event window frontier is not hydrated");
    const response = await this.rpc.reconnect(
      owner,
      this.store.getLatestSeq(),
      frontier.memory_revision,
      frontier.service_epoch,
      options,
    );
    if (response.disposition.kind === "rehydrate_required") {
      await this.recoverConnection(true, options);
      return;
    }
    this.store.applyReconnect(response);
    this.settleToolRequestProgress(response.events, response.session_id);
    this.publishSnapshotIfChanged();
  }

  /** Recovers one transport interruption according to the verified service-instance boundary.
   * 按已确认的服务实例边界恢复一次传输中断。 */
  async recoverConnection(
    serviceInstanceChanged: boolean,
    options?: ServiceRequestOptions,
  ): Promise<void> {
    if (!serviceInstanceChanged) {
      await this.reconnect(options);
      return;
    }
    const owner = this.requireOwner();
    const response = await this.rpc.hydrate(owner.session_id, options);
    assertWindowBoundary(response);
    this.store.restoreDurableBaseline(response);
    this.settleToolRequestProgress(response.events, response.session_id);
    this.commitWindowBoundary(response);
    this.publishSnapshotIfChanged();
  }

  /** Applies one pushed live batch through the same strict store path.
   * 通过同一严格 store 路径应用一批推送实时事件。 */
  applyLive(delta: ConversationEventLiveDeltaV1): void {
    this.store.applyLive(delta);
    this.settleToolRequestProgress(delta.events, delta.session_id);
    this.publishSnapshotIfChanged();
  }

  /** Releases the production live subscription owned by this controller.
   * 释放当前控制器拥有的生产实时订阅。*/
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.hydrationRequestToken += 1;
    this.liveUnsubscribe?.();
    this.liveUnsubscribe = null;
    this.liveSubscriptionPromise = null;
    this.pendingHydrationDeltas = [];
    this.pendingRevisionDeltas = [];
    this.pendingServiceEpochDeltas = [];
    this.hydratingSessionId = null;
    this.prependOlderPromise = null;
    this.generationRebaseTarget = null;
    this.serviceEpochRebaseTarget = null;
    this.serviceEpochRebasePromise = null;
    this.listeners.clear();
  }

  /** Loads and commits one exact historical page only while this controller remains owned.
   * 仅在该控制器仍被持有时加载并提交一页精确历史记录。
   *
   * @param owner selected session and generation captured before the request starts.
   *              请求开始前捕获的选中会话与代际。
   * @param requestedCursor exclusive cursor for the adjacent older page.
   *                        紧邻更早分页的排他游标。
   * @param options optional transport request controls.
   *                可选传输请求控制。
   * @returns whether the committed page contained any events.
   *          已提交分页是否包含事件。 */
  private async loadOlderPage(
    owner: ConversationEventWindowOwner,
    requestedCursor: number,
    options?: ServiceRequestOptions,
  ): Promise<boolean> {
    const response = await this.rpc.page(owner, requestedCursor, options);
    if (this.disposed) throw new Error("conversation event window controller is disposed");
    assertWindowBoundary(response);
    this.store.prependPage(response);
    this.commitHistoricalBoundary(response);
    this.publishSnapshotIfChanged();
    return response.events.length > 0;
  }

  /** Atomically replaces the loaded window after an explicit generation or
   * session ownership change.
   * 在显式代次或会话归属变化后原子替换已加载窗口。 */
  rebase(response: ConversationEventHydrationV1Response): void {
    this.hydrationRequestToken += 1;
    this.generationRebaseTarget = null;
    assertWindowBoundary(response);
    this.store.rebase(response);
    this.settleToolRequestProgress(response.events, response.session_id);
    this.commitWindowBoundary(response);
    this.publishSnapshotIfChanged();
  }

  /** Returns the initialized owner or reports an explicit controller misuse.
   * 返回已初始化归属，或显式报告控制器误用。 */
  private requireOwner(): ConversationEventWindowOwner {
    const owner = this.store.getOwner();
    if (owner === null) throw new Error("conversation event window is not hydrated");
    return owner;
  }

  /** Installs the sole production V1 live subscription before hydration reads.
   * 在 hydration 读取前安装唯一生产 V1 实时订阅。*/
  private async ensureLiveSubscription(): Promise<void> {
    if (this.disposed) throw new Error("conversation event window controller is disposed");
    if (this.liveUnsubscribe !== null) return;
    if (this.liveSubscriptionPromise !== null) return this.liveSubscriptionPromise;
    this.liveSubscriptionPromise = this.rpc
      .subscribeLive((delta) => this.receivePushedDelta(delta))
      .then((unsubscribe) => {
        if (this.disposed) {
          unsubscribe();
        } else {
          this.liveUnsubscribe = unsubscribe;
        }
      })
      .finally(() => {
        this.liveSubscriptionPromise = null;
      });
    return this.liveSubscriptionPromise;
  }

  /** Reconnects from the committed hydration frontier after a delayed socket becomes usable.
   * 在延迟套接字变为可用后，从已提交的 hydration 前沿执行重连。
   *
   * The live handler is registered synchronously before HTTP hydration starts. If physical
   * WebSocket readiness trails the hydration response, this reconciliation closes only that
   * transport gap while keeping persisted conversation data immediately visible.
   * 实时处理器会在 HTTP hydration 开始前同步登记；若物理 WebSocket 晚于 hydration 响应就绪，
   * 此协调仅补齐该传输缺口，同时让已持久化会话数据立即可见。
   *
   * @param requestToken hydration ownership token captured by the committed request.
   *                     已提交请求捕获的 hydration 归属令牌。
   * @param sessionId exact selected session owned by the committed hydration.
   *                  已提交 hydration 持有的精确选中会话。
   * @param liveSubscriptionReady non-throwing physical subscription readiness result.
   *                              不抛错的物理订阅就绪结果。
   * @returns This method schedules reconciliation and returns immediately.
   *          此方法安排协调任务后立即返回。
   */
  private reconcileAfterDelayedLiveSubscription(
    requestToken: number,
    sessionId: string,
    liveSubscriptionReady: Promise<boolean>,
  ): void {
    void liveSubscriptionReady.then(async (ready) => {
      if (!ready || this.disposed || requestToken !== this.hydrationRequestToken) return;
      const owner = this.store.getOwner();
      if (owner === null || owner.session_id !== sessionId) return;
      try {
        await this.reconnect();
      } catch (error) {
        if (this.disposed || requestToken !== this.hydrationRequestToken) return;
        console.error("[conversation-event-window] delayed live reconciliation failed", error);
      }
    });
  }

  /** Receives one pushed delta while preserving hydration/live atomic ownership.
   * 在保持 hydration/live 原子归属的前提下接收一条推送增量。*/
  private receivePushedDelta(delta: ConversationEventLiveDeltaV1): void {
    const owner = this.store.getOwner();
    if (owner === null) {
      if (delta.session_id === this.hydratingSessionId) this.pendingHydrationDeltas.push(delta);
      return;
    }
    this.applyOwnedPushedDelta(delta);
  }

  /** Applies an exact-owner pushed delta and ignores unrelated or stale owners.
   * 应用精确归属的推送增量，并忽略无关或过期归属。*/
  private applyOwnedPushedDelta(
    delta: ConversationEventLiveDeltaV1,
    publishSnapshot = true,
  ): void {
    const owner = this.requireOwner();
    if (delta.session_id !== owner.session_id) return;
    const frontier = this.store.getFrontier();
    if (frontier === null) throw new Error("conversation event window frontier is not hydrated");
    if (this.serviceEpochRebaseTarget !== null) {
      if (delta.service_epoch !== frontier.service_epoch) {
        this.requestServiceEpochRebase(delta.service_epoch, delta);
      }
      return;
    }
    if (delta.service_epoch !== frontier.service_epoch) {
      this.requestServiceEpochRebase(delta.service_epoch, delta);
      return;
    }
    if (delta.window_generation < owner.window_generation) return;
    if (this.generationRebaseTarget?.session_id === owner.session_id) {
      if (delta.window_generation > this.generationRebaseTarget.window_generation) {
        this.generationRebaseTarget = {
          session_id: owner.session_id,
          window_generation: delta.window_generation,
        };
      }
      return;
    }
    if (delta.window_generation > owner.window_generation) {
      this.requestGenerationRebase(delta);
      return;
    }
    if (this.revisionReconnectPromise !== null) {
      this.pendingRevisionDeltas.push(delta);
      return;
    }
    try {
      this.store.applyLive(delta);
      this.settleToolRequestProgress(delta.events, delta.session_id);
      if (publishSnapshot) this.publishSnapshotIfChanged();
    } catch (error) {
      if (error instanceof ConversationEventServiceEpochMismatchError) {
        this.requestServiceEpochRebase(error.actual_service_epoch, delta);
        return;
      }
      // Every rejected pushed patch is recovered through the authoritative reconnect boundary.
      // 每个被拒绝的推送 patch 都通过权威重连边界恢复。
      if (!(error instanceof ConversationEventRevisionGapError)) {
        console.warn("[conversation-event-window] pushed delta validation failed; recovering", error);
      }
      this.pendingRevisionDeltas.push(delta);
      this.startRevisionReconnectTask();
    }
  }

  /** Starts one durable hydration after a different service-process epoch is observed.
   * 在观察到不同服务进程代次后启动一次可靠 hydration。 */
  private requestServiceEpochRebase(
    serviceEpoch: string,
    delta?: ConversationEventLiveDeltaV1,
  ): void {
    if (this.serviceEpochRebaseTarget !== serviceEpoch) {
      this.serviceEpochRebaseTarget = serviceEpoch;
      this.pendingServiceEpochDeltas = [];
    }
    if (delta !== undefined) {
      this.pendingServiceEpochDeltas.push(delta);
    }
    this.pendingRevisionDeltas = [];
    if (this.serviceEpochRebasePromise !== null) return;
    const task = this.runServiceEpochRebase();
    this.serviceEpochRebasePromise = task;
    void task.then(
      () => {
        if (this.serviceEpochRebasePromise !== task) return;
        this.serviceEpochRebasePromise = null;
        if (this.serviceEpochRebaseTarget !== null) {
          this.requestServiceEpochRebase(this.serviceEpochRebaseTarget);
        }
      },
      (error: unknown) => {
        if (this.serviceEpochRebasePromise !== task) return;
        this.serviceEpochRebasePromise = null;
        this.serviceEpochRebaseTarget = null;
        this.pendingServiceEpochDeltas = [];
        console.error("[conversation-event-window] service epoch rebase failed", error);
      },
    );
  }

  /** Hydrates and atomically drops volatile state owned by an earlier service process.
   * hydration 并原子丢弃由较早服务进程拥有的 volatile 状态。 */
  private async runServiceEpochRebase(): Promise<void> {
    const owner = this.requireOwner();
    const requestedEpoch = this.serviceEpochRebaseTarget;
    if (requestedEpoch === null) return;
    const response = await this.rpc.hydrate(owner.session_id);
    const currentOwner = this.store.getOwner();
    if (currentOwner === null || currentOwner.session_id !== owner.session_id) return;
    assertWindowBoundary(response);
    const latestTarget = this.serviceEpochRebaseTarget;
    if (latestTarget !== requestedEpoch) {
      return;
    }
    if (response.service_epoch !== requestedEpoch) {
      throw new Error(
        `service epoch hydration expected ${requestedEpoch}, got ${response.service_epoch}`,
      );
    }
    const pending = this.pendingServiceEpochDeltas;
    this.pendingServiceEpochDeltas = [];
    this.serviceEpochRebaseTarget = null;
    this.store.restoreDurableBaseline(response);
    this.settleToolRequestProgress(response.events, response.session_id);
    this.commitWindowBoundary(response);
    this.generationRebaseTarget = null;
    for (const delta of pending) this.applyOwnedPushedDelta(delta, false);
    this.publishSnapshotIfChanged();
  }

  /** Starts one single-flight reconnect after a pushed revision edge is missing.
   * 在推送修订边缺失后启动一次单飞重连。 */
  private startRevisionReconnectTask(): void {
    if (this.revisionReconnectPromise !== null) return;
    const task = this.runRevisionReconnect();
    this.revisionReconnectPromise = task;
    void task.then(
      () => {
        if (this.revisionReconnectPromise !== task) return;
        this.revisionReconnectPromise = null;
        if (this.pendingRevisionDeltas.length > 0) this.startRevisionReconnectTask();
      },
      (error: unknown) => {
        if (this.revisionReconnectPromise !== task) return;
        this.revisionReconnectPromise = null;
        console.error("[conversation-event-window] revision reconnect failed", error);
      },
    );
  }

  /** Reconnects from the exact memory frontier, then replays pushes received in flight.
   * 从精确内存前沿重连，随后重放进行中收到的推送。 */
  private async runRevisionReconnect(): Promise<void> {
    while (true) {
      const owner = this.requireOwner();
      const frontier = this.store.getFrontier();
      if (frontier === null) throw new Error("conversation event window frontier is not hydrated");
      const response: ConversationEventReconnectV1Response = await this.rpc.reconnect(
        owner,
        this.store.getLatestSeq(),
        frontier.memory_revision,
        frontier.service_epoch,
      );
      const currentOwner = this.store.getOwner();
      if (
        currentOwner === null ||
        currentOwner.session_id !== owner.session_id ||
        currentOwner.window_generation !== owner.window_generation
      ) {
        return;
      }
      if (response.disposition.kind === "rehydrate_required") {
        this.pendingRevisionDeltas = [];
        await this.recoverConnection(true);
        return;
      }
      try {
        this.store.applyReconnect(response);
      } catch (error) {
        // A reconnect replacement that cannot evolve from local state requires one clean durable baseline.
        // 无法从本地状态演进的重连替换需要重新加载一份干净可靠基线。
        if (error instanceof ConversationEventServiceEpochMismatchError) throw error;
        this.pendingRevisionDeltas = [];
        console.warn("[conversation-event-window] reconnect validation failed; rehydrating", error);
        await this.recoverConnection(true);
        return;
      }
      this.settleToolRequestProgress(response.events, response.session_id);

      const pending = this.pendingRevisionDeltas;
      this.pendingRevisionDeltas = [];
      let retry = false;
      for (let index = 0; index < pending.length; index += 1) {
        const delta = pending[index];
        const replayOwner = this.requireOwner();
        if (delta.session_id !== replayOwner.session_id) continue;
        if (delta.window_generation < replayOwner.window_generation) continue;
        if (delta.window_generation > replayOwner.window_generation) {
          this.pendingRevisionDeltas = [];
          this.requestGenerationRebase(delta);
          return;
        }
        try {
          this.store.applyLive(delta);
          this.settleToolRequestProgress(delta.events, delta.session_id);
        } catch (error) {
          if (!(error instanceof ConversationEventRevisionGapError)) {
            // Replaying a structurally rejected patch cannot make progress from this local baseline.
            // 重放结构上被拒绝的 patch 无法从当前本地基线继续推进。
            this.pendingRevisionDeltas = [];
            console.warn("[conversation-event-window] pending delta validation failed; rehydrating", error);
            await this.recoverConnection(true);
            return;
          }
          this.pendingRevisionDeltas.push(...pending.slice(index));
          retry = true;
          break;
        }
      }
      if (!retry && this.pendingRevisionDeltas.length === 0) {
        this.publishSnapshotIfChanged();
        return;
      }
    }
  }

  /** Starts or advances one single-flight hydration for a newer edit generation.
   * 启动或推进一次面向更新编辑代际的单飞 hydration。 */
  private requestGenerationRebase(delta: ConversationEventLiveDeltaV1): void {
    const owner = this.requireOwner();
    const currentTarget = this.generationRebaseTarget;
    if (
      currentTarget === null ||
      currentTarget.session_id !== owner.session_id ||
      delta.window_generation > currentTarget.window_generation
    ) {
      this.generationRebaseTarget = {
        session_id: owner.session_id,
        window_generation: delta.window_generation,
      };
    }
    this.pendingRevisionDeltas = [];
    this.startGenerationRebaseTask();
  }

  /** Starts the queued generation rebase once and preserves newer targets seen in flight.
   * 单次启动已排队的代际重建，并保留进行中观察到的更新目标。 */
  private startGenerationRebaseTask(): void {
    if (this.generationRebasePromise !== null || this.generationRebaseTarget === null) return;
    const owner = this.requireOwner();
    const requestToken = this.hydrationRequestToken;
    const task = this.runGenerationRebase(owner.session_id, requestToken);
    this.generationRebasePromise = task;
    void task.then(
      () => {
        if (this.generationRebasePromise !== task) return;
        this.generationRebasePromise = null;
        this.startGenerationRebaseTask();
      },
      (error: unknown) => {
        if (this.generationRebasePromise !== task) return;
        this.generationRebasePromise = null;
        this.generationRebaseTarget = null;
        console.error("[conversation-event-window] generation rebase failed", error);
      },
    );
  }

  /** Hydrates until the response covers the highest observed generation, then swaps once.
   * 持续 hydration 直到响应覆盖观察到的最高代际，然后一次性交换窗口。 */
  private async runGenerationRebase(sessionId: string, requestToken: number): Promise<void> {
    while (true) {
      const target = this.generationRebaseTarget;
      const owner = this.store.getOwner();
      if (
        target === null ||
        owner === null ||
        owner.session_id !== sessionId ||
        target.session_id !== sessionId ||
        owner.window_generation >= target.window_generation
      ) {
        this.generationRebaseTarget = null;
        return;
      }

      const requestedGeneration = target.window_generation;
      const response = await this.rpc.hydrate(sessionId);
      if (requestToken !== this.hydrationRequestToken) return;
      assertWindowBoundary(response);
      if (response.session_id !== sessionId) {
        throw new Error("conversation event generation rebase returned another session");
      }
      if (response.window_generation < requestedGeneration) {
        throw new Error(
          `conversation event generation rebase expected at least ${requestedGeneration}, got ${response.window_generation}`,
        );
      }

      const latestTarget = this.generationRebaseTarget;
      if (
        latestTarget !== null &&
        latestTarget.session_id === sessionId &&
        response.window_generation < latestTarget.window_generation
      ) {
        continue;
      }

      this.store.rebase(response);
      this.settleToolRequestProgress(response.events, response.session_id);
      this.commitWindowBoundary(response);
      this.generationRebaseTarget = null;
      this.publishSnapshotIfChanged();
      return;
    }
  }

  /** Publishes all hydration/rebase boundary fields only after the event swap
   * succeeds.
   * 仅在事件交换成功后发布全部 hydration/rebase 边界字段。 */
  private commitWindowBoundary(response: ConversationEventHydrationV1Response): void {
    this.olderBeforeSeq = response.older_before_seq;
    this.hasMore = response.has_more;
    this.openTurnId = response.open_turn_id;
    this.openTurnStartSeq = response.open_turn_start_seq;
    this.sessionConfig = response.session_config;
  }

  /** Advances only historical pagination fields after an adjacent prepend.
   * 在相邻前插后仅推进历史分页字段。 */
  private commitHistoricalBoundary(response: ConversationEventWindowV1BaseResponse): void {
    this.olderBeforeSeq = response.older_before_seq;
    this.hasMore = response.has_more;
  }

  /** Settles every request draft whose canonical V1 ToolCall was atomically admitted.
   * 封口每条已原子接纳 canonical V1 ToolCall 所对应的请求草稿。 */
  private settleToolRequestProgress(
    events: readonly ConversationEventSummary[],
    sessionId: string,
  ): void {
    if (this.toolRequestProgressSettlement === null) return;
    for (const summary of events) {
      if (summary.event.kind !== "tool_call") continue;
      const requestStreamId = summary.event.request_stream_id?.trim() ?? "";
      if (!requestStreamId) continue;
      this.toolRequestProgressSettlement.settleToolRequestProgress(sessionId, requestStreamId);
    }
  }
}

/** Compares two controller snapshots without serializing canonical event payloads.
 * 在不序列化 canonical 事件载荷的情况下比较两份控制器快照。
 *
 * Event objects preserve their references across stale and idempotent store
 * applications, while real same-seq evolution replaces the affected object.
 * 事件对象在过期或幂等 store 应用中保留引用，而真实同 seq 演进会替换受影响对象。 */
function sameControllerSnapshot(
  left: ConversationEventWindowControllerSnapshot,
  right: ConversationEventWindowControllerSnapshot,
): boolean {
  if (
    left.owner?.session_id !== right.owner?.session_id ||
    left.owner?.window_generation !== right.owner?.window_generation ||
    left.service_epoch !== right.service_epoch ||
    left.memory_revision !== right.memory_revision ||
    left.durable_revision !== right.durable_revision ||
    left.older_before_seq !== right.older_before_seq ||
    left.has_more !== right.has_more ||
    left.open_turn_id !== right.open_turn_id ||
    left.open_turn_start_seq !== right.open_turn_start_seq ||
    left.session_config !== right.session_config ||
    left.events.length !== right.events.length
  ) {
    return false;
  }
  return left.events.every((event, index) => event === right.events[index]);
}

/** Validates service-computed cursor and open-Turn boundary coherence.
 * 校验服务端计算的游标与开放 Turn 边界一致性。 */
function assertWindowBoundary(response: ConversationEventWindowV1BaseResponse): void {
  if (response.has_more !== (response.older_before_seq !== null)) {
    throw new Error("conversation event has_more does not match older_before_seq");
  }
  if (response.older_before_seq !== null) assertSafeSeq(response.older_before_seq);
  const hasOpenTurnId = response.open_turn_id !== null;
  const hasOpenTurnStart = response.open_turn_start_seq !== null;
  if (hasOpenTurnId !== hasOpenTurnStart) {
    throw new Error("conversation event open Turn boundary is incomplete");
  }
  if (response.open_turn_id !== null) assertSafeSeq(response.open_turn_id);
  if (response.open_turn_start_seq !== null) assertSafeSeq(response.open_turn_start_seq);
}
