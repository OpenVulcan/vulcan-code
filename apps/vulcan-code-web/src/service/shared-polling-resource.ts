// One resource owns polling and ordering for every observer of the same metadata identity.
// 同一元数据身份的全部观察者共享轮询与结果排序。
export interface PollingObserver<T> {
  next: (value: T) => void;
  error: (error: unknown) => void;
}

export interface SharedPollingResource<T> {
  subscribe: (observer: PollingObserver<T>) => () => void;
  // A manual refresh may supersede an old read; it never adopts a pre-mutation request.
  // 手动刷新可以取代旧读取，绝不直接采用变更之前已发出的请求。
  refresh: () => Promise<T>;
}

// Polling is single-flight. One additional manual request keeps refresh responsive without
// allowing repeated clicks to create an unbounded number of in-flight requests.
// 自动轮询不重叠；额外允许一条手动请求，使刷新可响应且连点不会无限增加在途请求。
const MAX_ACTIVE_READS = 2;

export function createSharedPollingResource<T>(
  load: () => Promise<T>,
  intervalMs: number,
  equal: (previous: T, next: T) => boolean,
  onRetired: () => void,
): SharedPollingResource<T> {
  if (!Number.isFinite(intervalMs) || intervalMs <= 0) throw new Error("Polling interval must be positive.");
  const observers = new Set<PollingObserver<T>>();
  const active = new Set<Promise<T>>();
  let current: T;
  let hasValue = false;
  let hasError = false;
  let lastError: unknown;
  let retired = false;
  let issued = 0;
  let settled = 0;
  let queued: { promise: Promise<T>; resolve: (value: T | PromiseLike<T>) => void; reject: (error: unknown) => void } | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const visible = () => typeof document === "undefined" || document.visibilityState !== "hidden";
  const clearTimer = () => { if (timer !== null) clearTimeout(timer); timer = null; };
  const retire = () => {
    if (retired) return;
    retired = true;
    clearTimer();
    if (typeof document !== "undefined") document.removeEventListener("visibilitychange", visibilityChanged);
    const pending = queued;
    queued = null;
    pending?.reject(new Error("The metadata observation has ended."));
    onRetired();
  };
  const schedule = () => {
    clearTimer();
    if (retired || observers.size === 0 || active.size > 0 || !visible()) return;
    timer = setTimeout(() => {
      timer = null;
      if (visible()) poll();
    }, intervalMs);
  };
  const read = (): Promise<T> => {
    if (retired) return Promise.reject(new Error("The metadata observation has ended."));
    clearTimer();
    const sequence = ++issued;
    const request = Promise.resolve().then(load).then(next => {
      if (!retired && sequence > settled) {
        settled = sequence;
        const changed = !hasValue || !equal(current, next);
        if (changed) current = next;
        const recovered = hasError;
        hasValue = true;
        hasError = false;
        lastError = undefined;
        if (changed || recovered) {
          for (const observer of [...observers]) if (observers.has(observer)) observer.next(current);
        }
      }
      return next;
    }, error => {
      if (!retired && sequence > settled) {
        settled = sequence;
        hasError = true;
        lastError = error;
        for (const observer of [...observers]) if (observers.has(observer)) observer.error(error);
      }
      throw error;
    }).finally(() => {
      active.delete(request);
      const pending = queued;
      if (!retired && pending && active.size < MAX_ACTIVE_READS) {
        queued = null;
        read().then(pending.resolve, pending.reject);
      } else if (active.size === 0) {
        if (observers.size === 0) retire();
        else schedule();
      }
    });
    active.add(request);
    return request;
  };
  const poll = () => {
    // Timer failures are already delivered to observers; do not create an unhandled rejection.
    // 定时失败已交给观察者，不再制造未处理的 Promise 拒绝。
    if (active.size === 0) void read().catch(() => {});
  };
  function visibilityChanged() {
    clearTimer();
    if (!retired && visible() && observers.size > 0) poll();
  }
  return {
    subscribe(observer) {
      if (retired) throw new Error("Cannot subscribe to a retired metadata observation.");
      observers.add(observer);
      if (observers.size === 1 && typeof document !== "undefined") {
        document.addEventListener("visibilitychange", visibilityChanged);
      }
      if (hasValue) observer.next(current);
      if (hasError) observer.error(lastError);
      if (!hasValue && active.size === 0) poll();
      else if (active.size === 0 && timer === null) schedule();
      let subscribed = true;
      return () => {
        if (!subscribed) return;
        subscribed = false;
        observers.delete(observer);
        // Old responses cannot republish after this identity is reopened in a new lifetime.
        // 身份以新生命周期重新打开后，旧响应不得重新发布。
        if (observers.size === 0) retire();
      };
    },
    refresh() {
      if (retired) return Promise.reject(new Error("The metadata observation has ended."));
      if (active.size < MAX_ACTIVE_READS) return read();
      if (!queued) {
        let resolve!: (value: T | PromiseLike<T>) => void;
        let reject!: (error: unknown) => void;
        const promise = new Promise<T>((accept, refuse) => { resolve = accept; reject = refuse; });
        queued = { promise, resolve, reject };
      }
      return queued.promise;
    },
  };
}
