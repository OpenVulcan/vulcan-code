import type { PluginUiActionPolicy } from "@/types";

// One renderer retains at most this many bytes of explicitly cacheable projections.
// 单个渲染进程最多保留这些字节的明确可缓存投影。
const RESULT_CACHE_BYTES = 4 * 1024 * 1024;

// Immutable host context prevents plugins from choosing another owner's query namespace.
// 不可变宿主上下文禁止插件选择其他所有者的查询命名空间。
export interface PluginQueryContext {
  // Authenticated module owner and exact service/generation identity.
  // 已认证模块所有者及精确服务与代次身份。
  pluginId: string;
  runtimeIdentity: string;
  // Host-selected workspace and persisted conversation, each explicitly optional.
  // 宿主选中的工作区及持久会话，两者都明确可选。
  workspacePath: string | null;
  conversationId: string | null;
}

// Query options affect only one subscriber; refresh still joins an existing identical request.
// 查询选项仅影响一个订阅者；刷新仍加入既有相同请求。
export interface PluginQueryOptions {
  // Ignore a completed cache entry, never duplicate an in-flight read.
  // 忽略已完成缓存，但不重复在途读取。
  refresh?: boolean;
  // Cancels this subscription when a filter is superseded.
  // 筛选被替换时取消当前订阅。
  signal?: AbortSignal;
}

// One read entry owns shared work independently from React component instances.
// 单个读取条目独立于 React 组件实例持有共享任务。
interface QueryEntry {
  // Only superseded filters cancel backend admission; ordinary navigation preserves shared work.
  // 仅被替换筛选取消后端准入；普通导航保留共享任务。
  cancellation: AbortController;
  superseded: boolean;
  key: string;
  // Exact owner, cache generation and subscriber count.
  // 精确所有者、缓存代次及订阅者数量。
  pluginId: string;
  revision: number;
  subscribers: number;
  // Queued entries may be dropped; running entries retain actual backend ownership.
  // 可丢弃排队条目；执行中条目继续保留实际后端所有权。
  state: "queued" | "running" | "complete";
  promise: Promise<unknown>;
  // Only manifest-approved completed projections receive a retention deadline and byte size.
  // 仅清单批准的已完成投影具有保留截止时间及字节数。
  expiresAt: number;
  bytes: number;
}

// Per-renderer registries retain in-flight work and isolate invalidated generations.
// 渲染进程内注册表保留在途任务，并隔离已失效代次。
const queries = new Map<string, QueryEntry>();
const revisions = new Map<string, number>();
// Reads wait for submitted writes, including event-triggered refreshes during those writes.
// 读取等待已提交写入，包括写入期间由事件触发的刷新。
const mutations = new Map<string, Set<Promise<unknown>>>();

// Serialize exact JSON values canonically so object property order cannot duplicate a query.
// 精确规范化 JSON 值，避免对象属性顺序产生重复查询。
// Invalid JSON values fail before dispatch rather than silently changing request meaning.
// 无效 JSON 值在调度前失败，不静默改变请求含义。
function canonicalJson(value: unknown): string {
  if (value === null || typeof value === "string" || typeof value === "boolean") return JSON.stringify(value);
  if (typeof value === "number" && Number.isFinite(value)) return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (typeof value === "object" && value !== null && Object.getPrototypeOf(value) === Object.prototype) {
    return `{${Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([key, entry]) => `${JSON.stringify(key)}:${canonicalJson(entry)}`).join(",")}}`;
  }
  throw new Error("Plugin action arguments must be exact JSON values.");
}

// Invalidate reads atomically; a late completion cannot republish an earlier revision.
// 原子使读取失效；迟到结果不能重新发布旧修订。
export function invalidatePluginQueries(pluginId: string): void {
  revisions.set(pluginId, (revisions.get(pluginId) ?? 0) + 1);
  for (const [key, entry] of queries) if (entry.pluginId === pluginId) queries.delete(key);
}

// Clear all service-bound reads when transport reconnects or the service incarnation changes.
// 传输重连或服务实例改变时清空全部服务绑定读取。
export function invalidateAllPluginQueries(): void {
  for (const pluginId of new Set([...revisions.keys(), ...Array.from(queries.values(), entry => entry.pluginId)])) invalidatePluginQueries(pluginId);
}

// Evict expired completed results and then oldest retained entries within the shared byte budget.
// 驱逐到期结果，再按共享字节预算驱逐最早保留的已完成条目。
function trimResults(): void {
  let bytes = 0;
  for (const [key, entry] of queries) {
    if (entry.state !== "complete") continue;
    if (entry.expiresAt <= Date.now()) queries.delete(key);
    else bytes += entry.bytes;
  }
  for (const [key, entry] of queries) {
    if (bytes <= RESULT_CACHE_BYTES) break;
    if (entry.state === "complete") { queries.delete(key); bytes -= entry.bytes; }
  }
}

// Subscribe to one result while allowing either the mount or a changed filter to detach.
// 订阅单个结果，允许挂载卸载或筛选改变时分离。
// Detachment never aborts already running work used by another subscriber.
// 分离不会中止其他订阅者使用的已运行任务。
function subscribe<Result>(entry: QueryEntry, lifetime: AbortSignal, filter?: AbortSignal): Promise<Result> {
  const signals = filter ? [lifetime, filter] : [lifetime];
  entry.subscribers += 1;
  return new Promise<Result>((resolve, reject) => {
    let active = true;
    const release = () => {
      if (!active) return;
      active = false;
      entry.subscribers -= 1;
      for (const signal of signals) signal.removeEventListener("abort", abort);
    };
    const abort = () => {
      if (!active) return;
      if (filter?.aborted) entry.superseded = true;
      release();
      if (entry.subscribers === 0 && entry.superseded && entry.state !== "complete") {
        // Remove before sending cancellation so a new mount cannot join withdrawn admission.
        // 发送取消前移除条目，防止新挂载加入已撤回的准入。
        if (queries.get(entry.key) === entry) queries.delete(entry.key);
        entry.cancellation.abort();
      }
      reject(new DOMException("Plugin query subscription ended.", "AbortError"));
    };
    for (const signal of signals) signal.addEventListener("abort", abort, { once: true });
    if (signals.some(signal => signal.aborted)) abort();
    entry.promise.then(value => {
      if (!active) return;
      release();
      resolve(structuredClone(value) as Result);
    }, error => { if (active) { release(); reject(error); } });
  });
}

// Creates a mount-owned subscription boundary over renderer-shared query work.
// 在渲染进程共享查询任务上创建挂载自有订阅边界。
// Policies and identity come exclusively from the authenticated module, dispatch remains host-owned.
// 策略与身份仅来自已认证模块，调度仍由宿主持有。
export function createPluginQueryClient(
  context: PluginQueryContext,
  policies: PluginUiActionPolicy[],
  dispatch: (actionId: string, argumentsValue: unknown, cancellation?: AbortSignal) => Promise<unknown>,
) {
  const lifetime = new AbortController();
  const policyById = new Map(policies.map(policy => [policy.id, policy]));
  // A previous service incarnation's unfinished write must not block a new service.
  // 旧服务实例未结束的写入不能阻塞新服务。
  const mutationKey = JSON.stringify([context.runtimeIdentity, context.pluginId]);
  const invoke = async <Result>(actionId: string, argumentsValue: unknown, options: PluginQueryOptions = {}): Promise<Result> => {
    lifetime.signal.throwIfAborted();
    options.signal?.throwIfAborted();
    const policy = policyById.get(actionId);
    if (!policy) throw new Error(`Plugin action ${actionId} is not authorized by this module.`);
    // Snapshot caller-owned arguments before awaiting so later mutations cannot change the request.
    // 等待前冻结调用方参数，避免后续修改改变请求。
    const encoded = canonicalJson(argumentsValue);
    const args: unknown = JSON.parse(encoded);
    if (policy.effect === "mutation") {
      invalidatePluginQueries(context.pluginId);
      const pending = Promise.resolve().then(() => dispatch(actionId, args));
      const writes = mutations.get(mutationKey) ?? new Set<Promise<unknown>>();
      mutations.set(mutationKey, writes);
      writes.add(pending);
      try { return await pending as Result; }
      finally {
        writes.delete(pending);
        if (writes.size === 0) mutations.delete(mutationKey);
      }
    }
    // Do not start a read between a write's invalidation event and its actual completion.
    // 不在写入失效事件与实际完成之间启动读取。
    while (mutations.has(mutationKey)) await Promise.allSettled([...mutations.get(mutationKey)!]);
    lifetime.signal.throwIfAborted();
    options.signal?.throwIfAborted();
    trimResults();
    const key = JSON.stringify([context.runtimeIdentity, context.pluginId, context.workspacePath, context.conversationId, actionId, encoded]);
    let entry = queries.get(key);
    if (entry?.state === "queued" && entry.subscribers === 0) { queries.delete(key); entry = undefined; }
    if (entry?.state === "complete" && options.refresh) { queries.delete(key); entry = undefined; }
    if (!entry) {
      const revision = revisions.get(context.pluginId) ?? 0;
      const created: QueryEntry = { key, cancellation: new AbortController(), superseded: false, pluginId: context.pluginId, revision, subscribers: 0, state: "queued", expiresAt: 0, bytes: 0, promise: Promise.resolve() };
      created.promise = Promise.resolve().then(async () => {
        if (created.subscribers === 0 || queries.get(key) !== created) throw new DOMException("Plugin query cancelled before dispatch.", "AbortError");
        created.state = "running";
        const value = await dispatch(actionId, args, created.cancellation.signal);
        if (queries.get(key) !== created || (revisions.get(context.pluginId) ?? 0) !== revision) throw new Error("Plugin query became stale; refresh the current page.");
        created.state = "complete";
        if (policy.resultCacheMs > 0) {
          created.expiresAt = Date.now() + policy.resultCacheMs;
          created.bytes = new TextEncoder().encode(JSON.stringify(value)).length;
          trimResults();
        } else queries.delete(key);
        return value;
      }).catch(error => { if (queries.get(key) === created) queries.delete(key); throw error; });
      queries.set(key, created);
      entry = created;
    }
    return subscribe<Result>(entry, lifetime.signal, options.signal);
  };
  return { invoke, dispose: () => lifetime.abort() };
}
