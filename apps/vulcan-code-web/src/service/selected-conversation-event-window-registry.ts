import type { BrowserServiceConnectionStatus } from "./browser-service-discovery";
import type {
  ConversationEventWindowControllerListener,
  ConversationEventWindowControllerSnapshot,
} from "./conversation-event-window-controller";

/** Tracks effect setup generations so React StrictMode rehearsal cleanup cannot destroy a live registry.
 * 跟踪副作用建立代次，避免 React StrictMode 演练清理销毁仍在使用的 registry。 */
export interface SelectedConversationRegistryDisposalLease {
  // generation identifies the newest mounted effect setup that owns disposal authority.
  // generation 标识当前拥有释放权限的最新已挂载副作用建立。
  generation: number;
}

/** Creates the disposal lease owned by one WorkbenchApp component instance.
 * 创建由一个 WorkbenchApp 组件实例持有的释放租约。
 *
 * @returns A fresh lease with no mounted effect generation.
 *          返回尚无已挂载副作用代次的新租约。
 */
export function createSelectedConversationRegistryDisposalLease(): SelectedConversationRegistryDisposalLease {
  return { generation: 0 };
}

/** Activates one effect generation and returns an idempotent deferred cleanup.
 * 激活一个副作用代次并返回幂等的延迟清理函数。
 *
 * React StrictMode immediately runs setup-cleanup-setup in development. Deferring the release to
 * a microtask lets the second setup supersede the rehearsal cleanup, while a real final unmount
 * has no successor and therefore still releases the registry.
 * React StrictMode 在开发环境会立即执行建立-清理-再建立。将释放延迟到微任务后，第二次建立
 * 可以取代演练清理；真实最终卸载没有后继建立，因此仍会释放 registry。
 *
 * @param lease Component-owned generation lease.
 *              组件持有的代次租约。
 * @param dispose Permanently releases the selected-session registry.
 *                永久释放选中会话 registry。
 * @returns Cleanup function for this exact effect setup.
 *          返回本次精确副作用建立的清理函数。
 */
export function activateSelectedConversationRegistryDisposalLease(
  lease: SelectedConversationRegistryDisposalLease,
  dispose: () => void,
): () => void {
  // generation captures this exact setup before a later StrictMode setup can supersede it.
  // generation 在后续 StrictMode 建立取代本次建立前捕获其精确代次。
  const generation = ++lease.generation;
  // cleanupScheduled keeps duplicate cleanup calls idempotent.
  // cleanupScheduled 保证重复清理调用保持幂等。
  let cleanupScheduled = false;
  return () => {
    if (cleanupScheduled) return;
    cleanupScheduled = true;
    queueMicrotask(() => {
      if (lease.generation === generation) dispose();
    });
  };
}

/** Minimal controller lifecycle owned by the selected-session registry.
 * 选中会话 registry 持有的最小控制器生命周期。 */
export interface SelectedConversationEventWindowController {
  /** Installs a complete cached local window synchronously without fetching its tail again.
   * 同步安装完整的本地缓存窗口，不重新获取尾部历史。 */
  restoreCachedSnapshot(snapshot: ConversationEventWindowControllerSnapshot): void;
  /** Resumes live delivery and reconciles the cached revision frontier.
   * 恢复实时交付并协调缓存的修订前沿。 */
  resumeCachedWindow(): Promise<void>;
  /** Establishes one exact selected-session ownership boundary.
   * 建立一个精确的选中会话归属边界。 */
  hydrate(sessionId: string): Promise<boolean>;
  /** Refreshes the exact owned session through a generation-aware atomic replacement.
   * 通过可识别 generation 的原子替换刷新精确的已持有会话。 */
  rehydrate(sessionId: string): Promise<boolean>;
  /** Recovers transport continuity while leaving rebuild authority to the service response.
   * 恢复传输连续性，同时把是否重建的权威留给服务响应。 */
  recoverConnection(serviceInstanceChanged: boolean): Promise<void>;
  /** Prepends the immediately older complete-Turn page through the controller single-flight.
   * 通过控制器单飞前插紧邻的更早完整 Turn 分页。 */
  prependOlder(): Promise<boolean>;
  /** Returns the latest immutable event-window snapshot.
   * 返回最近的不可变事件窗口快照。 */
  snapshot(): ConversationEventWindowControllerSnapshot;
  /** Subscribes to atomic controller publications.
   * 订阅控制器原子发布。 */
  subscribe(listener: ConversationEventWindowControllerListener): () => void;
  /** Permanently releases live subscriptions and pending ownership.
   * 永久释放实时订阅与等待中的归属。 */
  dispose(): void;
}

/** Creates one fresh controller for the newly selected durable session.
 * 为新选中的持久化会话创建一个全新控制器。 */
export type SelectedConversationEventWindowControllerFactory =
  () => SelectedConversationEventWindowController;

/** Commands bound to one exact selected-session controller ownership generation.
 * 绑定到一个精确选中会话控制器归属代次的命令。 */
export interface SelectedConversationEventWindowCommands {
  /** Rehydrates the exact bound session after an authoritative generation replacement.
   * 在权威 generation 被替换后重新加载精确绑定的会话。
   */
  rehydrate(): Promise<boolean>;
  /** Prepends one adjacent historical page for the bound owner.
   * 为绑定归属前插一页相邻历史记录。 */
  prependOlder(): Promise<boolean>;
}

/** Selected-session hydration lifecycle published independently from canonical event data.
 * 独立于 canonical 事件数据发布的选中会话 hydration 生命周期。 */
export type SelectedConversationEventWindowStatus =
  | { readonly kind: "idle" }
  | { readonly kind: "loading"; readonly session_id: string }
  | { readonly kind: "ready"; readonly session_id: string }
  | { readonly kind: "failed"; readonly session_id: string; readonly message: string };

/** One active selected-session controller and its registry relay.
 * 一条活动选中会话控制器及其 registry 转发订阅。 */
interface SelectedConversationEventWindowEntry {
  /** Exact durable session identity.
   * 精确的持久化会话身份。 */
  sessionId: string;
  /** Controller that solely owns this selected-session window.
   * 唯一持有该选中会话窗口的控制器。 */
  controller: SelectedConversationEventWindowController;
  /** Releases the controller-to-registry snapshot relay.
   * 释放控制器到 registry 的快照转发。 */
  unsubscribe: () => void;
  /** Whether initial hydration established a recoverable owner/frontier.
   * 首次 hydration 是否已经建立可恢复的归属与前沿。 */
  hydrated: boolean;
  /** Current single hydration task for this exact entry.
   * 当前精确条目的唯一 hydration 任务。 */
  hydrationPromise: Promise<boolean> | null;
  /** Commands permanently bound to this exact entry.
   * 永久绑定到该精确条目的命令。 */
  commands: SelectedConversationEventWindowCommands;
}

/** Stable empty snapshot exposed while no durable session is selected.
 * 未选中持久化会话时暴露的稳定空快照。 */
const EMPTY_SELECTED_CONVERSATION_EVENT_WINDOW_SNAPSHOT:
ConversationEventWindowControllerSnapshot = {
  owner: null,
  service_epoch: null,
  memory_revision: null,
  durable_revision: null,
  events: [],
  older_before_seq: null,
  has_more: false,
  open_turn_id: null,
  open_turn_start_seq: null,
  session_config: null,
};

/** Stable status exposed while no durable session is selected.
 * 未选中持久化会话时暴露的稳定状态。 */
const IDLE_SELECTED_CONVERSATION_EVENT_WINDOW_STATUS:
SelectedConversationEventWindowStatus = { kind: "idle" };

/** Shared command surface used while no durable selected-session owner exists.
 * 不存在持久化选中会话归属时使用的共享命令面。 */
const UNAVAILABLE_SELECTED_CONVERSATION_EVENT_WINDOW_COMMANDS:
SelectedConversationEventWindowCommands = {
  /** Rejects rehydration because no exact durable owner is available.
   * 因不存在精确持久化归属而拒绝重新加载。
   */
  async rehydrate(): Promise<boolean> {
    throw new Error("selected conversation event window commands are unavailable");
  },
  /** Rejects pagination because no exact durable owner is available.
   * 因不存在精确持久化归属而拒绝分页。 */
  async prependOlder(): Promise<boolean> {
    throw new Error("selected conversation event window commands are unavailable");
  },
};

/** Owns exactly one V1 controller for the current durable selected session.
 * 为当前持久化选中会话精确持有一个 V1 控制器。 */
export class SelectedConversationEventWindowRegistry {
  /** Factory used only after a real durable session becomes selected.
   * 仅在真实持久化会话被选中后使用的控制器工厂。 */
  private readonly createController: SelectedConversationEventWindowControllerFactory;
  /** Registry-level listeners retained across selected-session switches.
   * 跨选中会话切换保留的 registry 级监听器。 */
  private readonly listeners = new Set<ConversationEventWindowControllerListener>();
  /** Currently owned controller entry, or null for drafts and empty workbenches.
   * 当前持有的控制器条目；草稿与空工作台时为 null。 */
  private activeEntry: SelectedConversationEventWindowEntry | null = null;
  /** Recently visited immutable windows; eight inactive sessions bound retained history memory without keeping sockets alive.
   * 最近访问的不可变窗口；最多八个非活动会话限制历史内存占用，且不保留实时连接。 */
  private readonly retainedWindows = new Map<string, ConversationEventWindowControllerSnapshot>();
  /** Stable snapshot consumed by the React external-store adapter.
   * React 外部 store 适配器消费的稳定快照。 */
  private snapshotCache = EMPTY_SELECTED_CONVERSATION_EVENT_WINDOW_SNAPSHOT;
  /** Stable selected-session hydration status consumed by the React status store.
   * React 状态 store 消费的稳定选中会话 hydration 状态。 */
  private statusCache = IDLE_SELECTED_CONVERSATION_EVENT_WINDOW_STATUS;
  /** Monotonic token invalidating delayed hydration after a selection change.
   * 在选择变化后使延迟 hydration 失效的单调令牌。 */
  private selectionToken = 0;
  /** Whether one connected state has established the initial transport baseline.
   * 是否已有一次 connected 状态建立初始传输基线。 */
  private hasConnectedBaseline = false;
  /** Whether a real post-baseline disconnect still requires one recovery.
   * 是否存在一次真实基线后断链仍需恢复。 */
  private recoveryRequired = false;
  /** Whether the WorkbenchApp owner permanently released this registry.
   * WorkbenchApp 归属方是否已经永久释放该 registry。 */
  private disposed = false;

  /** Creates a selected-session registry around one production controller factory.
   * 围绕一个生产控制器工厂创建选中会话 registry。
   *
   * @param createController factory for a fresh, unsubscribed V1 controller.
   *                         用于创建全新且尚未订阅的 V1 控制器的工厂。 */
  constructor(createController: SelectedConversationEventWindowControllerFactory) {
    this.createController = createController;
  }

  /** Returns the latest selected-session snapshot without rebuilding it.
   * 返回最近的选中会话快照，不进行重建。 */
  snapshot(): ConversationEventWindowControllerSnapshot {
    return this.snapshotCache;
  }

  /** Returns the latest selected-session hydration lifecycle without rebuilding it.
   * 返回最近的选中会话 hydration 生命周期，不进行重建。 */
  status(): SelectedConversationEventWindowStatus {
    return this.statusCache;
  }

  /** Reports whether an exact session already has a hydrated active or retained window.
   * 报告精确会话是否已有完成初始化的活动窗口或保留窗口。
   *
   * @param sessionId exact durable session identity to inspect.
   *                  待检查的精确持久化会话标识。
   * @returns whether switching can restore an initialized event window.
   *          切换时是否能恢复已初始化的事件窗口。 */
  hasInitializedSession(sessionId: string): boolean {
    if (this.disposed) return false;
    if (this.activeEntry?.sessionId === sessionId) return this.activeEntry.hydrated;
    return this.retainedWindows.has(sessionId);
  }

  /** Returns commands bound to the current exact controller entry.
   * 返回绑定到当前精确控制器条目的命令。
   *
   * @returns current owner commands, or an explicitly rejecting unavailable surface.
   *          当前归属命令；不存在归属时返回明确拒绝的不可用命令面。 */
  commands(): SelectedConversationEventWindowCommands {
    return this.activeEntry?.commands
      ?? UNAVAILABLE_SELECTED_CONVERSATION_EVENT_WINDOW_COMMANDS;
  }

  /** Subscribes one listener across controller replacements.
   * 跨控制器替换订阅一个监听器。
   *
   * @param listener callback notified after an atomic registry publication.
   *                 registry 原子发布后通知的回调。
   * @returns idempotent listener release callback.
   *          幂等的监听器释放回调。 */
  subscribe(listener: ConversationEventWindowControllerListener): () => void {
    if (this.disposed) return () => undefined;
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Selects one durable session and disposes the previous controller before hydration.
   * 选择一个持久化会话，并在 hydration 前释放旧控制器。
   *
   * @param sessionId exact durable session id, or null for a draft/empty surface.
   *                  精确的持久化会话 id；草稿或空界面时为 null。
   * @returns whether the selected controller committed its initial hydration.
   *          选中控制器是否提交了首次 hydration。 */
  async selectSession(sessionId: string | null): Promise<boolean> {
    if (this.disposed) throw new Error("selected conversation event window registry is disposed");
    if (this.activeEntry?.sessionId === sessionId) {
      return this.hydrateEntry(this.activeEntry);
    }

    const selectionToken = ++this.selectionToken;
    // Remove the requested warm window before caching the outgoing one, so eviction cannot discard the destination.
    // 在缓存离开窗口前取出目标缓存，避免淘汰操作丢弃即将进入的窗口。
    const retained = sessionId === null ? undefined : this.retainedWindows.get(sessionId);
    if (sessionId !== null) this.retainedWindows.delete(sessionId);
    this.releaseActiveEntry();
    if (sessionId === null) {
      this.publishSnapshot(EMPTY_SELECTED_CONVERSATION_EVENT_WINDOW_SNAPSHOT);
      this.publishStatus(IDLE_SELECTED_CONVERSATION_EVENT_WINDOW_STATUS);
      return true;
    }

    const controller = this.createController();
    // Restore before subscribing or publishing so a warm switch never exposes an empty owner.
    // 在订阅或发布前恢复，使已缓存切换绝不暴露空归属。
    if (retained) controller.restoreCachedSnapshot(retained);
    const commands = this.createEntryCommands(selectionToken, controller);
    const entry: SelectedConversationEventWindowEntry = {
      sessionId,
      controller,
      unsubscribe: () => undefined,
      hydrated: Boolean(retained),
      hydrationPromise: null,
      commands,
    };
    entry.unsubscribe = controller.subscribe(() => {
      if (this.activeEntry === entry) this.publishSnapshot(controller.snapshot());
    });
    this.activeEntry = entry;
    this.publishSnapshot(controller.snapshot());
    if (retained) {
      this.publishStatus({ kind: "ready", session_id: sessionId });
      try {
        await controller.resumeCachedWindow();
        return selectionToken === this.selectionToken && this.activeEntry === entry;
      } catch (error) {
        if (this.activeEntry === entry) {
          this.publishStatus({ kind: "failed", session_id: sessionId, message: selectedConversationHydrationErrorMessage(error) });
        }
        throw error;
      }
    }
    this.publishStatus({ kind: "loading", session_id: sessionId });
    const hydrated = await this.hydrateEntry(entry);
    if (selectionToken !== this.selectionToken || this.activeEntry !== entry) {
      entry.unsubscribe();
      controller.dispose();
      return false;
    }
    return hydrated;
  }

  /** Observes browser-service connectivity and performs exactly one post-disconnect recovery.
   * 观察浏览器服务连接状态，并在真实断链后精确执行一次恢复。
   *
   * @param status transport status emitted by the existing browser service boundary.
   *               既有浏览器服务边界发出的传输状态。
   * @returns after the single required recovery finishes, when one exists.
   *          若存在恢复，则在唯一所需恢复完成后返回。 */
  async observeConnectionStatus(status: BrowserServiceConnectionStatus): Promise<void> {
    if (this.disposed) return;
    if (status !== "connected") {
      if (this.hasConnectedBaseline) this.recoveryRequired = true;
      return;
    }
    if (!this.hasConnectedBaseline) {
      this.hasConnectedBaseline = true;
      return;
    }
    if (!this.recoveryRequired) return;
    this.recoveryRequired = false;
    const entry = this.activeEntry;
    if (entry === null || !entry.hydrated) return;
    await entry.controller.recoverConnection(false);
  }

  /** Permanently releases the selected controller and every registry listener.
   * 永久释放选中控制器与全部 registry 监听器。 */
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.selectionToken += 1;
    this.releaseActiveEntry();
    this.retainedWindows.clear();
    this.snapshotCache = EMPTY_SELECTED_CONVERSATION_EVENT_WINDOW_SNAPSHOT;
    this.listeners.clear();
  }

  /** Hydrates one current entry once and shares the in-flight task with duplicate selection signals.
   * 对当前条目执行一次 hydration，并与重复选择信号共享进行中任务。 */
  private hydrateEntry(
    entry: SelectedConversationEventWindowEntry,
    refresh: boolean = false,
  ): Promise<boolean> {
    if (entry.hydrated && !refresh) return Promise.resolve(true);
    if (entry.hydrationPromise !== null) return entry.hydrationPromise;
    if (this.activeEntry === entry) {
      this.publishStatus({ kind: "loading", session_id: entry.sessionId });
    }
    const hydration = refresh
      ? entry.controller.rehydrate(entry.sessionId)
      : entry.controller.hydrate(entry.sessionId);
    entry.hydrationPromise = hydration
      .then((hydrated) => {
        if (this.activeEntry === entry) {
          entry.hydrated = hydrated;
          if (hydrated) {
            this.publishStatus({ kind: "ready", session_id: entry.sessionId });
          }
        }
        return hydrated;
      })
      .catch((error: unknown) => {
        if (this.activeEntry === entry) {
          entry.hydrated = false;
          this.publishStatus({
            kind: "failed",
            session_id: entry.sessionId,
            message: selectedConversationHydrationErrorMessage(error),
          });
        }
        throw error;
      })
      .finally(() => {
        entry.hydrationPromise = null;
      });
    return entry.hydrationPromise;
  }

  /** Creates commands that can never retarget themselves after a selection change.
   * 创建在选择变化后绝不会自行重定向的命令。
   *
   * @param selectionToken exact registry ownership generation.
   *                       精确 registry 归属代次。
   * @param controller exact controller owned by that generation.
   *                   该代次持有的精确控制器。
   * @returns immutable command methods bound to the supplied generation.
   *          绑定到指定代次的不可变命令方法。 */
  private createEntryCommands(
    selectionToken: number,
    controller: SelectedConversationEventWindowController,
  ): SelectedConversationEventWindowCommands {
    return {
      rehydrate: () => this.rehydrateForEntry(selectionToken, controller),
      prependOlder: () => this.prependOlderForEntry(selectionToken, controller),
    };
  }

  /** Rehydrates only when the captured controller still owns the selected session.
   * 仅在捕获的控制器仍持有选中会话时重新加载。
   *
   * @param selectionToken captured registry ownership generation.
   *                       捕获的 registry 归属代次。
   * @param controller captured controller that must still be the active owner.
   *                   仍须为活动归属方的捕获控制器。
   * @returns whether the replacement generation was committed to the bound controller.
   *          替换 generation 是否已提交到绑定控制器。
   */
  private async rehydrateForEntry(
    selectionToken: number,
    controller: SelectedConversationEventWindowController,
  ): Promise<boolean> {
    const entry = this.activeEntry;
    if (
      this.disposed
      || selectionToken !== this.selectionToken
      || entry === null
      || entry.controller !== controller
    ) {
      throw new Error("selected conversation event window command owner is no longer active");
    }
    const hydrated = await this.hydrateEntry(entry, entry.hydrated);
    if (
      !hydrated
      || this.disposed
      || selectionToken !== this.selectionToken
      || this.activeEntry !== entry
    ) {
      return false;
    }
    entry.hydrated = true;
    return true;
  }

  /** Executes pagination only when the captured controller still owns the selected session.
   * 仅在捕获的控制器仍持有选中会话时执行分页。
   *
   * @param selectionToken captured registry ownership generation.
   *                       捕获的 registry 归属代次。
   * @param controller captured controller that must still be active and hydrated.
   *                   仍须处于活动且已 hydration 状态的捕获控制器。
   * @returns the bound controller pagination result.
   *          绑定控制器的分页结果。 */
  private async prependOlderForEntry(
    selectionToken: number,
    controller: SelectedConversationEventWindowController,
  ): Promise<boolean> {
    const entry = this.activeEntry;
    if (
      this.disposed
      || selectionToken !== this.selectionToken
      || entry === null
      || entry.controller !== controller
      || !entry.hydrated
    ) {
      throw new Error("selected conversation event window command owner is no longer active");
    }
    return controller.prependOlder();
  }

  /** Releases the exact active relay and controller before ownership changes.
   * 在归属变化前释放精确的活动转发与控制器。 */
  private releaseActiveEntry(): void {
    const entry = this.activeEntry;
    this.activeEntry = null;
    if (entry === null) return;
    if (!this.disposed && entry.hydrated) {
      this.retainedWindows.delete(entry.sessionId);
      this.retainedWindows.set(entry.sessionId, entry.controller.snapshot());
      // Map insertion order is the least-recently-used eviction order.
      // Map 插入顺序即最近最少使用淘汰顺序。
      while (this.retainedWindows.size > 8) {
        const oldest = this.retainedWindows.keys().next().value;
        if (oldest !== undefined) this.retainedWindows.delete(oldest);
      }
    }
    entry.unsubscribe();
    entry.controller.dispose();
  }

  /** Publishes one immutable snapshot only when its object identity changes.
   * 仅在对象身份变化时发布一份不可变快照。 */
  private publishSnapshot(snapshot: ConversationEventWindowControllerSnapshot): void {
    if (this.snapshotCache === snapshot) return;
    this.snapshotCache = snapshot;
    for (const listener of this.listeners) listener();
  }

  /** Publishes one stable hydration status only when its semantic value changes.
   * 仅在语义值变化时发布一份稳定 hydration 状态。 */
  private publishStatus(status: SelectedConversationEventWindowStatus): void {
    if (selectedConversationWindowStatusEquals(this.statusCache, status)) return;
    this.statusCache = status;
    for (const listener of this.listeners) listener();
  }
}

/** Normalizes one transport or storage failure for the selected-session error surface.
 * 为选中会话错误界面规范化一条传输或存储失败。 */
function selectedConversationHydrationErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Compares two selected-session lifecycle values without relying on object identity.
 * 不依赖对象身份比较两份选中会话生命周期值。 */
function selectedConversationWindowStatusEquals(
  left: SelectedConversationEventWindowStatus,
  right: SelectedConversationEventWindowStatus,
): boolean {
  if (left.kind !== right.kind) return false;
  if (left.kind === "idle" || right.kind === "idle") return true;
  if (left.session_id !== right.session_id) return false;
  if (left.kind === "failed" && right.kind === "failed") {
    return left.message === right.message;
  }
  return true;
}
