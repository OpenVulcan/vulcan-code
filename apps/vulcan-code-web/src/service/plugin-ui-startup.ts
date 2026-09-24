import type { StartupSnapshot } from "@/types";

// Startup transport exposes only the authoritative startup snapshot and its event subscription.
// 启动传输仅暴露权威启动快照及其事件订阅。
export interface PluginStartupSource {
  // Read the current authoritative snapshot, cancelling the transport on retirement.
  // 读取当前权威快照，退役时取消传输。
  read: (signal: AbortSignal) => Promise<StartupSnapshot>;
  // Register snapshots and return the exact listener cleanup function.
  // 注册快照监听并返回该监听器的清理函数。
  subscribe: (listener: (snapshot: StartupSnapshot) => void) => Promise<() => void>;
}

// Subscribe before reading to close the ready-event race; abort releases the exact subscription.
// 先订阅再读取以消除就绪事件竞态；取消释放精确订阅。
// Resolves only when the full runtime is ready, rejecting startup failure or generation retirement.
// 仅在完整运行时就绪时返回，启动失败或代次退役时拒绝。
export async function waitForPluginStartup(source: PluginStartupSource, signal: AbortSignal): Promise<void> {
  signal.throwIfAborted();
  // Separate readiness from transport admission so events arriving during the read are retained.
  // 就绪状态与传输入场分离，保留读取期间到达的事件。
  let accept!: () => void;
  let fail!: (reason: unknown) => void;
  const ready = new Promise<void>((resolve, reject) => { accept = resolve; fail = reject; });
  // A pushed failure can arrive before the snapshot finishes; observe it immediately.
  // 推送失败可能早于快照返回，立即挂接异常观察。
  void ready.catch(() => undefined);
  // Evaluate the authoritative readiness flag or explicit startup failure.
  // 判断权威就绪标记或明确的启动失败。
  const observe = (snapshot: StartupSnapshot) => {
    if (snapshot.ready) accept();
    else if (snapshot.phase === "failed") fail(new Error("Plugin runtime startup failed; inspect the service startup diagnostics."));
  };
  // Retirement rejects the wait without replaying a module or business action.
  // 退役拒绝等待，不重放模块或业务操作。
  const abort = () => fail(new DOMException("Plugin module generation retired.", "AbortError"));
  signal.addEventListener("abort", abort, { once: true });
  // Cleanup exists only after subscription admission completes.
  // 只有订阅入场成功后才存在清理函数。
  let unsubscribe: (() => void) | undefined;
  try {
    unsubscribe = await source.subscribe(observe);
    signal.throwIfAborted();
    observe(await source.read(signal));
    await ready;
  } finally {
    unsubscribe?.();
    signal.removeEventListener("abort", abort);
  }
}
