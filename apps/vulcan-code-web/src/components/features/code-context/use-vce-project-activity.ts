import * as React from "react";
import { useVceActivityObserverStore } from "./vce-job-observer-context";
import { EMPTY_VCE_ACTIVITY } from "./vce-activity-observer-store";

// Observe projectId's shared lightweight metadata; revisionKey invalidates changed host authority.
// 观察 projectId 的共享轻量元数据；revisionKey 使变化后的宿主权威失效。
// Returns the immutable activity/error plus an exact-scope read-only refresh action.
// 返回不可变活动及错误，以及精确作用域的只读刷新动作。
export function useVceProjectActivity(projectId: string | null, revisionKey: string) {
  // store is replaced on a service restart without remounting conversations.
  // store 在服务重启时替换，不重新挂载会话。
  const store = useVceActivityObserverStore();
  // subscribe and getSnapshot retain identity across progress-only renders.
  // subscribe 与 getSnapshot 在仅进度变化的渲染间保留身份。
  const subscribe = React.useCallback((listener: () => void) => store.subscribe(projectId, listener), [projectId, store]);
  const getSnapshot = React.useCallback(() => store.snapshot(projectId), [projectId, store]);
  // snapshot is the same object for every current observer of this exact project.
  // snapshot 对此精确项目的每个当前观察者都是相同对象。
  const snapshot = React.useSyncExternalStore(subscribe, getSnapshot, () => EMPTY_VCE_ACTIVITY);
  // scope fences callbacks after navigation, authority changes, or a service restart.
  // scope 在导航、权威变化或服务重启后隔离回调。
  const scope = React.useMemo(() => ({ projectId, store, revisionKey }), [projectId, store, revisionKey]);
  // mounted records the exact committed hook lifetime.
  // mounted 记录精确的已提交钩子生命周期。
  const mounted = React.useRef<object | null>(null);
  // previous distinguishes an authority change from an initial subscription that already loads state.
  // previous 区分权威变化与已经加载状态的首次订阅。
  const previous = React.useRef(scope);
  React.useEffect(() => {
    mounted.current = scope;
    if (previous.current.projectId === projectId && previous.current.store === store
      && previous.current.revisionKey !== revisionKey) void store.refresh(projectId);
    previous.current = scope;
    return () => { if (mounted.current === scope) mounted.current = null; };
  }, [projectId, store, revisionKey, scope]);
  // reload refreshes shared state only while this exact UI scope is still mounted.
  // reload 仅在此精确界面作用域仍挂载时刷新共享状态。
  const reload = React.useCallback((): Promise<void> => mounted.current === scope
    ? store.refresh(projectId) : Promise.resolve(), [projectId, store, scope]);
  return { ...snapshot, reload };
}
