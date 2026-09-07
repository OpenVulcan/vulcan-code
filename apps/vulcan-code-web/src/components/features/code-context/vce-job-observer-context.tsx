import * as React from "react";
import { cancelVceJob, loadVceJob, subscribeVceJobChanged, loadVceProjectActivity, subscribeVceProjectChanged } from "@/service/vce-service";
import { VceJobObserverStore } from "./vce-job-observer-store";
import { VceActivityObserverStore } from "./vce-activity-observer-store";

// Scope job observations to one workbench service lifetime, never a process-wide singleton.
// 将作业观察限定到一个工作台服务生命周期，绝不使用进程级单例。
const VceJobObserverContext = React.createContext<VceJobObserverStore | null>(null);
// Lightweight activity shares the same workbench/service lifetime as job observation.
// 轻量活动与作业观察共享同一工作台及服务生命周期。
const VceActivityObserverContext = React.createContext<VceActivityObserverStore | null>(null);

// Share host job observation with all workspace surfaces without remounting their React children.
// 与全部工作区视图共享宿主作业观察，不重新挂载其 React 子节点。
// serviceEpoch is the existing host restart generation; children retain UI identity across changes.
// serviceEpoch 为既有宿主重启代次；children 在变化期间保留界面身份。
// Returns the provider; construction is inert and only mounted subscribers initiate metadata IO.
// 返回提供者；构造保持静止，只有已挂载订阅者才发起元数据输入输出。
export function VceJobObserverProvider(props: { serviceEpoch: number; children: React.ReactNode }) {
  // store is replaced on host restart, invalidating old receipts without resetting chat components.
  // store 在宿主重启时替换，使旧回执失效而不重置聊天组件。
  const store = React.useMemo(() => new VceJobObserverStore({
    load: loadVceJob, cancel: cancelVceJob, subscribe: subscribeVceJobChanged,
  }), [props.serviceEpoch]);
  // activityStore is inert until a surface observes one exact project, not all historical projects.
  // activityStore 在界面观察一个精确项目之前保持静止，不观察全部历史项目。
  const activityStore = React.useMemo(() => new VceActivityObserverStore({
    load: (projectId) => loadVceProjectActivity(projectId), subscribe: (listener) => subscribeVceProjectChanged(listener),
  }), [props.serviceEpoch]);
  return <VceJobObserverContext.Provider value={store}><VceActivityObserverContext.Provider value={activityStore}>
    {props.children}
  </VceActivityObserverContext.Provider></VceJobObserverContext.Provider>;
}

// Resolve the workbench-owned activity cache; no arguments or IO, and no fallback singleton.
// 获取工作台持有的活动缓存；无参数或输入输出，不使用兜底单例。
export function useVceActivityObserverStore(): VceActivityObserverStore {
  // store is supplied only by the existing workbench composition.
  // store 仅由既有工作台组合提供。
  const store = React.useContext(VceActivityObserverContext);
  if (store === null) throw new Error("VceJobObserverProvider is missing");
  return store;
}

// Resolve the required workbench-owned store; takes no arguments and returns its exact instance.
// 获取必需的工作台所属仓库；无参数，返回其精确实例。
// Missing composition fails explicitly instead of silently creating a duplicate polling owner.
// 缺少组合时明确失败，不静默创建重复轮询所有者。
export function useVceJobObserverStore(): VceJobObserverStore {
  // store is authoritative only through the matching workbench provider.
  // store 仅通过匹配的工作台提供者成为权威。
  const store = React.useContext(VceJobObserverContext);
  if (store === null) throw new Error("VceJobObserverProvider is missing");
  return store;
}
