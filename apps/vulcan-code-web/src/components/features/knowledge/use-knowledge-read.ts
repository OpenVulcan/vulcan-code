import * as React from "react";

// A read snapshot belongs to exactly one library, selector and pagination scope.
// 读取快照精确属于一个库、选择器及分页范围。
interface KnowledgeReadSnapshot<T> {
  // Exact caller-owned identity, never inferred from response display labels.
  // 精确调用方拥有身份，绝不从响应展示标签推断。
  scope: string;
  // Current successful value, or explicit absence while reading or after failure.
  // 当前成功值，或读取中及失败后的明确空值。
  value: T | null;
  // Exact English transport/service diagnostic.
  // 精确英文传输及服务诊断。
  error: string | null;
  // Whether the current owning read is pending.
  // 当前所属读取是否等待中。
  loading: boolean;
}

// Loads one exact scope and discards superseded or unmounted responses.
// 加载一个精确范围，并丢弃已替代或卸载响应。
// Parameters: scope identifies the owner; stable load reads its current DTO. Returns the value, read status and read-only refresh.
// 参数：scope 标识所有者；稳定 load 读取其当前 DTO。返回值、读取状态及只读刷新。
export function useKnowledgeRead<T>(scope: string, load: () => Promise<T>) {
  // A mounted scope visit owns both initial loading and awaited post-mutation refreshes.
  // 挂载范围访问同时拥有首次加载及变更后等待刷新。
  const owner = React.useRef<{ scope: string; active: boolean; sequence: number } | null>(null);
  // Callback identity owns a unique visit even after A-to-B-to-A navigation with the same scope text.
  // 回调身份拥有唯一访问，即使 A 到 B 再回 A 时范围文本相同。
  const visit = React.useMemo(() => ({ scope, active: false, sequence: 0, load }), [scope, load]);
  // Data and diagnostics publish together under their exact owner.
  // 数据及诊断在精确所有者下共同发布。
  const [snapshot, setSnapshot] = React.useState<KnowledgeReadSnapshot<T>>({ scope, value: null, error: null, loading: true });
  // background retains only the same scope while loading; errors still clear its DTO. Returns whether publication succeeded.
  // background 仅读取中保留相同范围；错误仍清空其 DTO。返回发布是否成功。
  const refresh = React.useCallback(async (background = false): Promise<boolean> => {
    // Old callbacks cannot reactivate an unmounted or differently scoped visit.
    // 旧回调不能重新激活卸载或不同范围访问。
    if (owner.current !== visit || !visit.active) return false;
    // Concurrent reads within one visit use monotonic request ownership.
    // 同一访问内并发读取使用单调请求归属。
    const sequence = ++visit.sequence;
    setSnapshot((previous) => ({ scope, value: background && previous.scope === scope ? previous.value : null, error: null, loading: true }));
    try {
      // Only this declared loader can produce the requested DTO.
      // 仅此声明加载器可产出请求 DTO。
      const value = await visit.load();
      if (!visit.active || owner.current !== visit || sequence !== visit.sequence) return false;
      setSnapshot({ scope, value, error: null, loading: false });
      return true;
    } catch (error) {
      if (!visit.active || owner.current !== visit || sequence !== visit.sequence) return false;
      setSnapshot({ scope, value: null, error: error instanceof Error ? error.message : String(error), loading: false });
      return false;
    }
  }, [scope, visit]);
  React.useEffect(() => {
    // Returning to the same library still creates a distinct revocable lifetime.
    // 返回相同库仍创建独立可撤销寿命。
    visit.active = true;
    owner.current = visit;
    void refresh();
    return () => { visit.active = false; };
  }, [refresh, visit]);
  // A changed selector clears the old projection before its new effect runs.
  // 选择器变化在新副作用运行前清空旧投影。
  const visible = snapshot.scope === scope ? snapshot : { scope, value: null, error: null, loading: true };
  return { ...visible, refresh };
}
