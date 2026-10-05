import { afterEach, describe, expect, it, vi } from "vitest";
import { useConversationSessionEventWindowOwnerV1 } from "./conversation-session-event-window-owner-v1";

// lifecycle runs the actual effect callbacks without introducing a DOM or another renderer dependency.
// lifecycle 执行真实 effect 回调，不引入 DOM 或额外渲染器依赖。
const lifecycle = vi.hoisted(() => ({
  // effects retains the hook's setup functions for explicit lifetime transitions.
  // effects 保留 hook 的 setup 函数，以显式推进生命周期。
  effects: [] as Array<() => void | (() => void)>,
  // updates captures every attempted React state write after an asynchronous result.
  // updates 捕获异步结果后的每次 React 状态写入尝试。
  updates: [] as unknown[],
  // recoverConnection is the controllable in-flight request, independent of effect cleanup.
  // recoverConnection 是可控制的在途请求，与 effect 清理相互独立。
  recoverConnection: vi.fn<() => Promise<void>>(),
}));

vi.mock("react", () => ({
  // Memo factories run once for this effect-only fixture; returns the constructed value.
  // 记忆工厂在此仅验证 effect 的夹具中执行一次；返回构造的值。
  useMemo: <T,>(factory: () => T): T => factory(),
  // Ref cells retain their supplied value across the captured effects.
  // Ref 单元在捕获的 effect 之间保留传入值。
  useRef: <T,>(current: T): { current: T } => ({ current }),
  // State writes are observed without pretending to test React rendering or StrictMode.
  // 观察状态写入，但不将其宣称为 React 渲染或 StrictMode 测试。
  useState: <T,>(initial: T) => [initial, (next: T) => lifecycle.updates.push(next)],
  // Effect setup is deferred so tests control setup, cleanup and late completion ordering.
  // 延迟 effect setup，让测试控制 setup、清理及迟到完成的顺序。
  useEffect: (setup: () => void | (() => void)) => lifecycle.effects.push(setup),
}));
vi.mock("@/service", () => ({ BROWSER_SERVICE_CONNECTION_EVENT: "test-service-connection" }));
vi.mock("@/service/webui-service-runtime", () => ({ serviceClient: {} }));
vi.mock("@/service/conversation-event-window-client", () => ({ createConversationEventWindowClient: () => ({}) }));
vi.mock("@/state/conversation-event-window-react", () => ({ useConversationEventWindowSnapshot: () => ({}) }));
vi.mock("@/service/conversation-event-window-controller", () => ({
  // Only hydration and connection recovery participate in these effect-lifetime tests.
  // 这些 effect 生命周期测试仅涉及 hydration 与连接恢复。
  ConversationEventWindowController: class {
    // Hydrates a live baseline; no parameters are needed by the fixture and success is returned.
    // 建立实时基线；夹具无需参数，返回成功。
    async hydrate(): Promise<boolean> { return true; }
    // Returns the controlled recovery promise for the current request.
    // 返回当前请求对应的可控恢复 Promise。
    recoverConnection(): Promise<void> { return lifecycle.recoverConnection(); }
    // Disposal has no external resources in this fixture and returns nothing.
    // 此夹具销毁时没有外部资源，无返回值。
    dispose(): void {}
  },
}));

afterEach(() => {
  lifecycle.effects.length = 0;
  lifecycle.updates.length = 0;
  lifecycle.recoverConnection.mockReset();
  vi.unstubAllGlobals();
});

describe("conversation session event-window owner lifecycle", () => {
  // Tests both sides of retirement: current failures remain visible, retired failures cannot contaminate a replacement.
  // 同时测试退役前后：当前失败仍可见，退役失败不得污染替代页面。
  it.each([false, true])("scopes late reconnect failure to its effect lifetime (retired=%s)", async (retired) => {
    // events supplies the browser connection event contract using Node's standard event target.
    // events 使用 Node 标准事件目标提供浏览器连接事件契约。
    const events = new EventTarget();
    vi.stubGlobal("window", events);
    // rejectRecovery is assigned synchronously by the Promise constructor before any effect runs.
    // rejectRecovery 由 Promise 构造器同步赋值，早于所有 effect 执行。
    let rejectRecovery!: (error: Error) => void;
    // recovery remains pending while the owner is optionally replaced or unmounted.
    // recovery 在所有者可选地被替换或卸载时保持在途。
    const recovery = new Promise<void>((_resolve, reject) => { rejectRecovery = reject; });
    lifecycle.recoverConnection.mockReturnValue(recovery);
    useConversationSessionEventWindowOwnerV1({ sessionId: "child", parentSessionId: "parent", live: true });
    // cleanup functions represent all three actual effect lifetimes, not duplicated recovery logic.
    // cleanup 函数代表三个真实 effect 的生命周期，而不是复制恢复逻辑。
    const cleanup = lifecycle.effects.map((setup) => setup());
    await Promise.resolve();
    for (const status of ["disconnected", "connected"]) {
      events.dispatchEvent(Object.assign(new Event("test-service-connection"), { detail: { status } }));
    }
    expect(lifecycle.recoverConnection).toHaveBeenCalledOnce();
    if (retired) cleanup.forEach((dispose) => dispose?.());
    lifecycle.updates.length = 0;
    rejectRecovery(new Error("old reconnect failed"));
    await Promise.resolve();
    expect(lifecycle.updates.includes("old reconnect failed")).toBe(!retired);
    if (!retired) cleanup.forEach((dispose) => dispose?.());
  });
});
