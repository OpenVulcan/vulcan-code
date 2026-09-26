import { describe, expect, it, vi } from "vitest";

import type {
  ConversationEventWindowControllerListener,
  ConversationEventWindowControllerSnapshot,
} from "./conversation-event-window-controller";
import { createConversationEventWindowExternalStore } from "@/state/conversation-event-window-react";
import {
  activateSelectedConversationRegistryDisposalLease,
  createSelectedConversationRegistryDisposalLease,
  SelectedConversationEventWindowRegistry,
  type SelectedConversationEventWindowController,
} from "./selected-conversation-event-window-registry";

/** Builds one hydrated snapshot owned by an exact test session.
 * 构建一份由精确测试会话持有的已 hydration 快照。 */
function hydratedSnapshot(sessionId: string): ConversationEventWindowControllerSnapshot {
  return {
    owner: { session_id: sessionId, window_generation: 1 },
    service_epoch: "service-registry-test",
    memory_revision: 1,
    durable_revision: 1,
    events: [],
    older_before_seq: null,
    has_more: false,
    open_turn_id: null,
    open_turn_start_seq: null,
    session_config: null,
  };
}

/** Test controller that exposes exact lifecycle call evidence without transport behavior.
 * 不包含传输行为、仅暴露精确生命周期调用证据的测试控制器。 */
class TestSelectedConversationController implements SelectedConversationEventWindowController {
  /** Restores the exact cached snapshot synchronously; no RPC is performed.
   * 同步恢复精确缓存快照，不执行 RPC。 */
  readonly restoreCachedSnapshot = vi.fn((snapshot: ConversationEventWindowControllerSnapshot) => {
    this.currentSnapshot = snapshot;
  });
  /** Records background reconciliation independently of initial hydration.
   * 独立于首次加载记录后台协调。 */
  readonly resumeCachedWindow = vi.fn(async (): Promise<void> => undefined);
  /** Active registry relay listeners.
   * 活动 registry 转发监听器。 */
  private readonly listeners = new Set<ConversationEventWindowControllerListener>();
  /** Current immutable test snapshot.
   * 当前不可变测试快照。 */
  private currentSnapshot: ConversationEventWindowControllerSnapshot = {
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
  /** Hydration call recorder.
   * hydration 调用记录器。 */
  readonly hydrate = vi.fn(async (sessionId: string) => {
    this.currentSnapshot = hydratedSnapshot(sessionId);
    for (const listener of this.listeners) listener();
    return true;
  });
  /** Rehydration call recorder for an already-owned session.
   * 已持有会话的重新加载调用记录器。 */
  readonly rehydrate = vi.fn(async (sessionId: string) => {
    this.currentSnapshot = hydratedSnapshot(sessionId);
    for (const listener of this.listeners) listener();
    return true;
  });
  /** Connection recovery call recorder.
   * 连接恢复调用记录器。 */
  readonly recoverConnection = vi.fn(async (_serviceInstanceChanged: boolean) => undefined);
  /** Historical pagination call recorder.
   * 历史分页调用记录器。 */
  readonly prependOlder = vi.fn(async () => true);
  /** Permanent release call recorder.
   * 永久释放调用记录器。 */
  readonly dispose = vi.fn(() => {
    this.listeners.clear();
  });

  /** Returns the exact current test snapshot.
   * 返回精确的当前测试快照。 */
  snapshot(): ConversationEventWindowControllerSnapshot {
    return this.currentSnapshot;
  }

  /** Installs one registry relay listener.
   * 安装一个 registry 转发监听器。 */
  subscribe(listener: ConversationEventWindowControllerListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Returns the number of live registry relay listeners.
   * 返回活动 registry 转发监听器数量。 */
  listenerCount(): number {
    return this.listeners.size;
  }
}

/** Creates one registry and captures every controller allocated by its factory.
 * 创建一个 registry，并捕获其工厂分配的每个控制器。 */
function registryFixture(): {
  registry: SelectedConversationEventWindowRegistry;
  controllers: TestSelectedConversationController[];
} {
  // controllers preserves exact allocation order for switch and disposal assertions.
  // controllers 为切换与释放断言保留精确分配顺序。
  const controllers: TestSelectedConversationController[] = [];
  // registry owns every controller created by this fixture.
  // registry 持有该夹具创建的每个控制器。
  const registry = new SelectedConversationEventWindowRegistry(() => {
    const controller = new TestSelectedConversationController();
    controllers.push(controller);
    return controller;
  });
  return { registry, controllers };
}

describe("selected conversation event window registry", () => {
  it("reports initialization only after exact hydration and while the window remains active or retained", async () => {
    // The active entry starts uninitialized and becomes reusable only after hydration finishes.
    // 活动条目起初未初始化，只有 hydration 完成后才可复用。
    const fixture = registryFixture();
    expect(fixture.registry.hasInitializedSession("session-a")).toBe(false);
    const firstSelection = fixture.registry.selectSession("session-a");
    expect(fixture.registry.hasInitializedSession("session-a")).toBe(false);
    await firstSelection;
    expect(fixture.registry.hasInitializedSession("session-a")).toBe(true);

    await fixture.registry.selectSession("session-b");
    expect(fixture.registry.hasInitializedSession("session-a")).toBe(true);
    expect(fixture.registry.hasInitializedSession("session-b")).toBe(true);
    expect(fixture.registry.hasInitializedSession("session-c")).toBe(false);

    fixture.registry.dispose();
    expect(fixture.registry.hasInitializedSession("session-a")).toBe(false);
    expect(fixture.registry.hasInitializedSession("session-b")).toBe(false);
  });

  it("retains the requested oldest cached window when caching the outgoing session reaches capacity", async () => {
    // Eight inactive windows and one active window exercise the exact cache-capacity boundary.
    // 八个非活动窗口与一个活动窗口覆盖精确缓存容量边界。
    const fixture = registryFixture();
    for (let index = 0; index < 9; index += 1) await fixture.registry.selectSession(`session-${index}`);
    await fixture.registry.selectSession("session-0");
    expect(fixture.controllers[9].restoreCachedSnapshot).toHaveBeenCalledOnce();
    expect(fixture.controllers[9].hydrate).not.toHaveBeenCalled();
    fixture.registry.dispose();
    expect(fixture.controllers.every((controller) => controller.dispose.mock.calls.length === 1)).toBe(true);
  });

  it("does not let delayed cached-window reconciliation replace the newly selected session", async () => {
    // The old command stays bound to the old selection even while its background resume is pending.
    // 旧命令即使仍在等待后台恢复，也始终绑定旧选择。
    // Create a controller with an externally completed resume to exercise the selection race.
    // 创建由外部完成恢复的控制器，以覆盖选择竞争。
    let completeResume: (() => void) | undefined;
    const delayed = new TestSelectedConversationController();
    delayed.resumeCachedWindow.mockImplementation(() => new Promise<void>((resolve) => { completeResume = resolve; }));
    const controllers = [new TestSelectedConversationController(), new TestSelectedConversationController(), delayed, new TestSelectedConversationController()];
    const registry = new SelectedConversationEventWindowRegistry(() => {
      const controller = controllers.shift();
      if (!controller) throw new Error("Missing controller");
      return controller;
    });
    await registry.selectSession("session-a");
    await registry.selectSession("session-b");
    const pending = registry.selectSession("session-a");
    await registry.selectSession("session-c");
    if (!completeResume) throw new Error("Missing resume completion");
    completeResume();
    expect(await pending).toBe(false);
    expect(registry.snapshot().owner?.session_id).toBe("session-c");
    expect(registry.status()).toEqual({ kind: "ready", session_id: "session-c" });
    expect(delayed.dispose).toHaveBeenCalledOnce();
    registry.dispose();
  });

  it("restores a visited window synchronously without publishing loading or discarding its history", async () => {
    // cached is the initialized owner's exact window, including its pagination and event identities.
    // cached 是已初始化归属的精确窗口，包括分页与事件身份。
    const fixture = registryFixture();
    await fixture.registry.selectSession("session-a");
    const cached = fixture.registry.snapshot();
    const oldCommands = fixture.registry.commands();
    await fixture.registry.selectSession("session-b");
    const observed: string[] = [];
    fixture.registry.subscribe(() => observed.push(fixture.registry.status().kind));
    const selected = fixture.registry.selectSession("session-a");
    expect(fixture.registry.snapshot()).toBe(cached);
    expect(fixture.registry.status()).toEqual({ kind: "ready", session_id: "session-a" });
    expect(observed).not.toContain("loading");
    expect(await selected).toBe(true);
    expect(fixture.controllers[2].hydrate).not.toHaveBeenCalled();
    expect(fixture.controllers[2].resumeCachedWindow).toHaveBeenCalledOnce();
    expect(fixture.controllers[0].dispose).toHaveBeenCalledOnce();
    await expect(oldCommands.prependOlder()).rejects.toThrow(/owner is no longer active/);
  });

  it("treats the first connected state as a baseline without recovery", async () => {
    // fixture contains one hydrated selected-session controller.
    // fixture 包含一个已 hydration 的选中会话控制器。
    const fixture = registryFixture();
    await fixture.registry.selectSession("session-a");

    await fixture.registry.observeConnectionStatus("connected");

    expect(fixture.controllers[0]?.recoverConnection).not.toHaveBeenCalled();
  });

  it("recovers exactly once after a real disconnect and ignores duplicate connected states", async () => {
    // fixture establishes the selected controller before transport state transitions.
    // fixture 在传输状态变化前建立选中控制器。
    const fixture = registryFixture();
    await fixture.registry.selectSession("session-a");
    await fixture.registry.observeConnectionStatus("connected");

    await fixture.registry.observeConnectionStatus("disconnected");
    await fixture.registry.observeConnectionStatus("reconnecting");
    await fixture.registry.observeConnectionStatus("connected");
    await fixture.registry.observeConnectionStatus("connected");

    expect(fixture.controllers[0]?.recoverConnection).toHaveBeenCalledTimes(1);
    expect(fixture.controllers[0]?.recoverConnection).toHaveBeenCalledWith(false);
  });

  it("retains failed recovery for the workbench retry without repeating a successful recovery", async () => {
    // A transient reconnect RPC failure must not consume the only recovery request.
    // 短暂的重连 RPC 失败不能消耗唯一恢复请求。
    const fixture = registryFixture();
    await fixture.registry.selectSession("session-retry");
    await fixture.registry.observeConnectionStatus("connected");
    await fixture.registry.observeConnectionStatus("disconnected");
    // The exact selected controller fails once, then supplies the authoritative completion.
    // 精确选中控制器失败一次，然后提供权威完成状态。
    const controller = fixture.controllers[0]!;
    controller.recoverConnection.mockRejectedValueOnce(new Error("temporary read failure"));
    await expect(fixture.registry.observeConnectionStatus("connected")).rejects.toThrow("temporary read failure");
    await fixture.registry.observeConnectionStatus("connected");
    await fixture.registry.observeConnectionStatus("connected");
    expect(controller.recoverConnection).toHaveBeenCalledTimes(2);
    fixture.registry.dispose();
  });

  it("keeps repeated workbench recovery attempts waiting for the same transcript read", async () => {
    // A fast snapshot retry cannot declare recovery complete while the slower transcript RPC is pending.
    // 快照的快速重试不能在较慢的正文 RPC 仍未完成时宣告恢复完成。
    const fixture = registryFixture();
    await fixture.registry.selectSession("session-shared-recovery");
    await fixture.registry.observeConnectionStatus("connected");
    await fixture.registry.observeConnectionStatus("disconnected");
    // The deferred read exposes the overlap deterministically without timers.
    // 延迟读取无需定时器即可确定性地暴露重叠。
    let finishRead!: () => void;
    const pendingRead = new Promise<undefined>((resolve) => { finishRead = () => resolve(undefined); });
    const controller = fixture.controllers[0]!;
    controller.recoverConnection.mockReturnValueOnce(pendingRead);
    const first = fixture.registry.observeConnectionStatus("connected");
    const settled = vi.fn();
    const second = fixture.registry.observeConnectionStatus("connected").then(settled);
    await Promise.resolve();
    expect(settled).not.toHaveBeenCalled();
    expect(controller.recoverConnection).toHaveBeenCalledTimes(1);
    finishRead();
    await Promise.all([first, second]);
    expect(settled).toHaveBeenCalledTimes(1);
    fixture.registry.dispose();
  });

  it("releases the old controller and relay before hydrating a switched session", async () => {
    // fixture captures both controller generations in allocation order.
    // fixture 按分配顺序捕获两代控制器。
    const fixture = registryFixture();
    // listener observes registry publications while controller identity changes.
    // listener 在控制器身份变化期间观察 registry 发布。
    const listener = vi.fn();
    fixture.registry.subscribe(listener);
    await fixture.registry.selectSession("session-a");
    const first = fixture.controllers[0];

    await fixture.registry.selectSession("session-b");
    const second = fixture.controllers[1];

    expect(first?.dispose).toHaveBeenCalledTimes(1);
    expect(first?.listenerCount()).toBe(0);
    expect(second?.hydrate).toHaveBeenCalledWith("session-b");
    expect(fixture.registry.snapshot().owner?.session_id).toBe("session-b");
    expect(fixture.registry.status()).toEqual({ kind: "ready", session_id: "session-b" });
    expect(listener).toHaveBeenCalled();
  });

  it("maps a draft selection to no controller and releases the durable owner", async () => {
    // fixture starts with one real durable selected session.
    // fixture 从一个真实持久化选中会话开始。
    const fixture = registryFixture();
    await fixture.registry.selectSession("session-a");

    await fixture.registry.selectSession(null);

    expect(fixture.controllers[0]?.dispose).toHaveBeenCalledTimes(1);
    expect(fixture.registry.snapshot().owner).toBeNull();
    expect(fixture.registry.status()).toEqual({ kind: "idle" });
  });

  it("releases the current controller on unmount and ignores later connection events", async () => {
    // fixture establishes both controller and connected baseline before disposal.
    // fixture 在释放前建立控制器与 connected 基线。
    const fixture = registryFixture();
    await fixture.registry.selectSession("session-a");
    await fixture.registry.observeConnectionStatus("connected");
    const controller = fixture.controllers[0];

    fixture.registry.dispose();
    await fixture.registry.observeConnectionStatus("disconnected");
    await fixture.registry.observeConnectionStatus("connected");

    expect(controller?.dispose).toHaveBeenCalledTimes(1);
    expect(controller?.recoverConnection).not.toHaveBeenCalled();
    expect(fixture.registry.snapshot().owner).toBeNull();
  });

  it("routes the current command surface only to the hydrated selected controller", async () => {
    // fixture creates one exact selected-session owner.
    // fixture 创建一个精确的选中会话归属。
    const fixture = registryFixture();
    // externalStore is the same stable WorkbenchApp-owned adapter used in production.
    // externalStore 是生产环境中由 WorkbenchApp 持有的同一稳定适配器。
    const externalStore = createConversationEventWindowExternalStore(fixture.registry);
    await fixture.registry.selectSession("session-a");

    const result = await externalStore.getCommands().prependOlder();

    expect(result).toBe(true);
    expect(fixture.controllers[0]?.prependOlder).toHaveBeenCalledTimes(1);
  });

  it("rehydrates only the exact bound owner after a generation replacement", async () => {
    // fixture starts with one hydrated owner whose command remains generation-bound.
    // fixture 从一个已加载归属开始，其命令始终绑定到该归属代次。
    const fixture = registryFixture();
    await fixture.registry.selectSession("session-a");
    const commands = fixture.registry.commands();
    const controller = fixture.controllers[0];

    const hydrated = await commands.rehydrate();

    expect(hydrated).toBe(true);
    expect(controller?.hydrate).toHaveBeenCalledTimes(1);
    expect(controller?.rehydrate).toHaveBeenCalledTimes(1);
    expect(controller?.rehydrate).toHaveBeenLastCalledWith("session-a");
  });

  it("publishes a terminal hydration failure and retries through the same bound owner", async () => {
    // controller fails exactly once so the registry can prove the error state is not an infinite loader.
    // controller 精确失败一次，使 registry 能证明错误状态不会成为无限加载动画。
    const controller = new TestSelectedConversationController();
    controller.hydrate.mockRejectedValueOnce(new Error("session metadata is unavailable"));
    const registry = new SelectedConversationEventWindowRegistry(() => controller);

    await expect(registry.selectSession("session-failed")).rejects.toThrow(
      "session metadata is unavailable",
    );

    expect(registry.status()).toEqual({
      kind: "failed",
      session_id: "session-failed",
      message: "session metadata is unavailable",
    });
    expect(registry.snapshot().owner).toBeNull();

    const retried = await registry.commands().rehydrate();

    expect(retried).toBe(true);
    expect(controller.hydrate).toHaveBeenCalledTimes(2);
    expect(registry.status()).toEqual({ kind: "ready", session_id: "session-failed" });
    expect(registry.snapshot().owner?.session_id).toBe("session-failed");
  });

  it("invalidates a captured command after switching sessions without retargeting it", async () => {
    // fixture captures commands permanently bound to the first owner generation.
    // fixture 捕获永久绑定到第一代归属的命令。
    const fixture = registryFixture();
    await fixture.registry.selectSession("session-a");
    const staleCommands = fixture.registry.commands();

    await fixture.registry.selectSession("session-b");

    await expect(staleCommands.prependOlder()).rejects.toThrow(/owner is no longer active/);
    await expect(staleCommands.rehydrate()).rejects.toThrow(/owner is no longer active/);
    expect(fixture.controllers[0]?.prependOlder).not.toHaveBeenCalled();
    expect(fixture.controllers[1]?.prependOlder).not.toHaveBeenCalled();
    expect(await fixture.registry.commands().prependOlder()).toBe(true);
    expect(fixture.controllers[1]?.prependOlder).toHaveBeenCalledTimes(1);
  });

  it("rejects both current and captured pagination commands after selecting a draft", async () => {
    // fixture starts with one durable owner before the draft removes command ownership.
    // fixture 在草稿移除命令归属前从一个持久化归属开始。
    const fixture = registryFixture();
    await fixture.registry.selectSession("session-a");
    const staleCommands = fixture.registry.commands();

    await fixture.registry.selectSession(null);

    await expect(staleCommands.prependOlder()).rejects.toThrow(/owner is no longer active/);
    await expect(fixture.registry.commands().prependOlder()).rejects.toThrow(/commands are unavailable/);
    expect(fixture.controllers[0]?.prependOlder).not.toHaveBeenCalled();
  });

  it("rejects captured pagination commands after registry disposal", async () => {
    // fixture captures a command before the WorkbenchApp owner is permanently released.
    // fixture 在 WorkbenchApp 归属被永久释放前捕获一条命令。
    const fixture = registryFixture();
    await fixture.registry.selectSession("session-a");
    const staleCommands = fixture.registry.commands();

    fixture.registry.dispose();

    await expect(staleCommands.prependOlder()).rejects.toThrow(/owner is no longer active/);
    await expect(fixture.registry.commands().prependOlder()).rejects.toThrow(/commands are unavailable/);
    expect(fixture.controllers[0]?.prependOlder).not.toHaveBeenCalled();
  });

  it("ignores StrictMode rehearsal cleanup and disposes only after the final effect cleanup", async () => {
    // lease models the stable WorkbenchApp-owned disposal authority across effect rehearsals.
    // lease 模拟跨副作用演练保持稳定的 WorkbenchApp 释放权限。
    const lease = createSelectedConversationRegistryDisposalLease();
    // dispose records the only permitted permanent registry release.
    // dispose 记录唯一允许的永久 registry 释放。
    const dispose = vi.fn();
    // rehearsalCleanup belongs to StrictMode's first setup.
    // rehearsalCleanup 属于 StrictMode 的第一次建立。
    const rehearsalCleanup = activateSelectedConversationRegistryDisposalLease(lease, dispose);

    rehearsalCleanup();
    // finalCleanup supersedes the queued rehearsal cleanup before its microtask runs.
    // finalCleanup 在演练清理微任务执行前取代其释放权限。
    const finalCleanup = activateSelectedConversationRegistryDisposalLease(lease, dispose);
    await Promise.resolve();

    expect(dispose).not.toHaveBeenCalled();

    finalCleanup();
    finalCleanup();
    await Promise.resolve();

    expect(dispose).toHaveBeenCalledTimes(1);
  });

  it("preserves the published snapshot when pagination fails", async () => {
    // fixture exposes one hydrated snapshot whose identity must survive command failure.
    // fixture 暴露一份已 hydration 快照，其身份必须在命令失败后保持不变。
    const fixture = registryFixture();
    await fixture.registry.selectSession("session-a");
    const controller = fixture.controllers[0];
    controller?.prependOlder.mockRejectedValueOnce(new Error("page failed"));
    const snapshotBeforeFailure = fixture.registry.snapshot();

    await expect(fixture.registry.commands().prependOlder()).rejects.toThrow("page failed");

    expect(fixture.registry.snapshot()).toBe(snapshotBeforeFailure);
  });
});
