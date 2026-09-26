import type { CoreBrowserStreamCallbacks } from "./core-browser-service";

const RETRY_BASE_MS = 1_000;
const RETRY_CAP_MS = 30_000;
const STABLE_CONNECTION_MS = 30_000;

// One controller owns connection generations, bounded retry delay and exact cleanup for one page.
// 一个控制器持有单个页面的连接代次、有界重试间隔与精确清理。
export function maintainCoreBrowserStream(
  open: (callbacks: CoreBrowserStreamCallbacks) => Promise<() => void>,
  callbacks: CoreBrowserStreamCallbacks,
): () => void {
  let active = true;
  let blocked = false;
  let generation = 0;
  let failures = 0;
  let connectedAt: number | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let close: (() => void) | null = null;
  let opening = false;
  let retryAfterOpen = false;
  const clearRetry = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  };
  const retire = () => {
    generation += 1;
    const ownedClose = close;
    close = null;
    ownedClose?.();
  };
  const schedule = () => {
    if (!active || blocked || timer !== null) return;
    if (connectedAt !== null && performance.now() - connectedAt >= STABLE_CONNECTION_MS) failures = 0;
    connectedAt = null;
    retire();
    // Equal jitter avoids synchronized windows without ever exceeding the single retry ceiling.
    // 等比例抖动避免多个窗口同时重连，且始终不超过唯一重试上限。
    const ceiling = Math.min(RETRY_CAP_MS, RETRY_BASE_MS * 2 ** Math.min(failures, 5));
    failures += 1;
    timer = setTimeout(() => {
      timer = null;
      if (opening) { retryAfterOpen = true; return; }
      connect();
    }, Math.floor(ceiling * (0.75 + Math.random() * 0.25)));
  };
  const connect = () => {
    if (!active || blocked || opening) return;
    opening = true;
    const current = ++generation;
    const live = () => active && !blocked && current === generation;
    callbacks.onState("connecting");
    const handlers: CoreBrowserStreamCallbacks = {
      onFrame: frame => { if (live()) callbacks.onFrame(frame); },
      onError: message => { if (live()) callbacks.onError(message); },
      onFailure: failure => {
        if (!live()) return;
        callbacks.onFailure?.(failure);
        if (!failure.retryable) {
          blocked = true;
          clearRetry();
          retire();
          callbacks.onState("disconnected");
        }
      },
      onState: state => {
        if (!live()) return;
        if (state === "connected" && connectedAt === null) connectedAt = performance.now();
        callbacks.onState(state);
        if (state === "disconnected") schedule();
      },
    };
    // Deferring the opener also captures synchronous failures in exactly the same retry path.
    // 延迟调用 opener，让同步失败也进入相同的重试路径。
    void Promise.resolve().then(() => active ? open(handlers) : (() => {})).then(ownedClose => {
      if (!live()) ownedClose();
      else close = ownedClose;
    }).catch((error: unknown) => {
      if (!live()) return;
      callbacks.onError(error instanceof Error ? error.message : String(error));
      schedule();
    }).finally(() => {
      opening = false;
      if (retryAfterOpen) { retryAfterOpen = false; schedule(); }
    });
  };
  connect();
  return () => {
    active = false;
    clearRetry();
    retire();
  };
}
