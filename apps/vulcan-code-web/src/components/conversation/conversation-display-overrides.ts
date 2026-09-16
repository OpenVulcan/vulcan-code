import { toast } from "sonner";

/** Creates one isolated boolean display-preference store; storageKey identifies its existing local document.
 * 创建一个隔离的布尔展示偏好存储；storageKey 标识其本地文档。
 * Returns exact-session reads, explicit writes, draft transfer and leaf subscriptions, without service mutations.
 * 返回精确会话读取、明确写入、草稿转移和叶子订阅，不修改服务设置。
 */
export function createConversationDisplayOverrides(storageKey: string) {
  // Cache and subscriptions belong only to this display setting.
  // 缓存与订阅仅属于此展示设置。
  let overrides: Record<string, boolean> | null = null;
  const listeners = new Set<() => void>();
  /** Reads validated boolean choices; absent keys represent inheritance rather than a copied default.
   * 读取经过验证的布尔选择；缺省键表示继承，而非复制默认值。
   */
  const read = (): Record<string, boolean> => {
    if (overrides !== null) return overrides;
    overrides = Object.create(null) as Record<string, boolean>;
    if (typeof window === "undefined") return overrides;
    try {
      // stored is untrusted and cannot introduce coerced or inherited values.
      // stored 不可信，不能引入强制转换或继承值。
      const stored: unknown = JSON.parse(window.localStorage.getItem(storageKey) ?? "{}");
      if (stored && typeof stored === "object" && !Array.isArray(stored)) {
        for (const [id, value] of Object.entries(stored)) {
          if (typeof value === "boolean") overrides[id] = value;
        }
      }
    } catch (error) {
      console.warn("Failed to load conversation display preferences.", error);
    }
    return overrides;
  };
  /** Persists next and notifies consumers; storage failures stay explicit while the current view remains usable.
   * 持久化 next 并通知消费者；存储失败明确展示，当前界面仍可使用。
   */
  const commit = (next: Record<string, boolean>): void => {
    overrides = next;
    try {
      window.localStorage.setItem(storageKey, JSON.stringify(next));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
    for (const listener of listeners) listener();
  };
  /** Writes enabled for sessionId; null removes the override and restores inheritance.
   * 为 sessionId 写入 enabled；null 移除覆盖值并恢复继承。
   */
  const set = (sessionId: string, enabled: boolean | null): void => {
    // next preserves a prototype-free document even for untrusted keys.
    // next 即使面对不可信键也保持无原型文档。
    const next = Object.assign(Object.create(null) as Record<string, boolean>, read());
    if (enabled === null) delete next[sessionId];
    else next[sessionId] = enabled;
    commit(next);
  };
  /** Transfers only draftId's explicit choice to its acknowledged sessionId in one atomic local update.
   * 通过一次本地原子更新，仅将 draftId 的明确选择转移到已确认的 sessionId。
   */
  const move = (draftId: string, sessionId: string): void => {
    // choice never copies an inherited system default.
    // choice 绝不复制继承的系统默认值。
    const choice = read()[draftId];
    if (choice === undefined || draftId === sessionId) return;
    const next = Object.assign(Object.create(null) as Record<string, boolean>, read());
    next[sessionId] = choice;
    delete next[draftId];
    commit(next);
  };
  /** Subscribes listener until the returned cleanup runs.
   * 订阅 listener，直到返回的清理函数执行。
   */
  const subscribe = (listener: () => void): (() => void) => {
    listeners.add(listener);
    return () => { listeners.delete(listener); };
  };
  /** Reads the exact session override; null scope and absent values both represent inheritance.
   * 读取精确会话覆盖值；null 作用域和缺省值均表示继承。
   */
  const get = (sessionId: string | null): boolean | undefined => sessionId === null ? undefined : read()[sessionId];
  return { get, set, move, subscribe };
}
