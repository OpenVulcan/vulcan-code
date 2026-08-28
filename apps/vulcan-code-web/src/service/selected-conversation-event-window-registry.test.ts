import { describe, expect, it, vi } from "vitest";

import type {
  ConversationEventWindowControllerListener,
  ConversationEventWindowControllerSnapshot,
} from "./conversation-event-window-controller";
import { createConversationEventWindowExternalStore } from "@/state/conversation-event-window-react";
import {
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
    expect(controller?.hydrate).toHaveBeenCalledTimes(2);
    expect(controller?.hydrate).toHaveBeenLastCalledWith("session-a");
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
