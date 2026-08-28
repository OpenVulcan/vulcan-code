import type { BrowserServiceConnectionStatus } from "./browser-service-discovery";
import type {
  ConversationEventWindowControllerListener,
  ConversationEventWindowControllerSnapshot,
} from "./conversation-event-window-controller";

/** Minimal controller lifecycle owned by the selected-session registry.
 * 选中会话 registry 持有的最小控制器生命周期。 */
export interface SelectedConversationEventWindowController {
  /** Establishes one exact selected-session ownership boundary.
   * 建立一个精确的选中会话归属边界。 */
  hydrate(sessionId: string): Promise<boolean>;
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
  /** Stable snapshot consumed by the React external-store adapter.
   * React 外部 store 适配器消费的稳定快照。 */
  private snapshotCache = EMPTY_SELECTED_CONVERSATION_EVENT_WINDOW_SNAPSHOT;
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
    this.releaseActiveEntry();
    if (sessionId === null) {
      this.publishSnapshot(EMPTY_SELECTED_CONVERSATION_EVENT_WINDOW_SNAPSHOT);
      return true;
    }

    const controller = this.createController();
    const commands = this.createEntryCommands(selectionToken, controller);
    const entry: SelectedConversationEventWindowEntry = {
      sessionId,
      controller,
      unsubscribe: () => undefined,
      hydrated: false,
      hydrationPromise: null,
      commands,
    };
    entry.unsubscribe = controller.subscribe(() => {
      if (this.activeEntry === entry) this.publishSnapshot(controller.snapshot());
    });
    this.activeEntry = entry;
    this.publishSnapshot(controller.snapshot());
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
    this.snapshotCache = EMPTY_SELECTED_CONVERSATION_EVENT_WINDOW_SNAPSHOT;
    this.listeners.clear();
  }

  /** Hydrates one current entry once and shares the in-flight task with duplicate selection signals.
   * 对当前条目执行一次 hydration，并与重复选择信号共享进行中任务。 */
  private hydrateEntry(entry: SelectedConversationEventWindowEntry): Promise<boolean> {
    if (entry.hydrated) return Promise.resolve(true);
    if (entry.hydrationPromise !== null) return entry.hydrationPromise;
    entry.hydrationPromise = entry.controller
      .hydrate(entry.sessionId)
      .then((hydrated) => {
        if (this.activeEntry === entry) entry.hydrated = hydrated;
        return hydrated;
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
   * @param controller captured controller that must still be active and hydrated.
   *                   仍须处于活动且已加载状态的捕获控制器。
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
      || !entry.hydrated
    ) {
      throw new Error("selected conversation event window command owner is no longer active");
    }
    const hydrated = await controller.hydrate(entry.sessionId);
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
}
