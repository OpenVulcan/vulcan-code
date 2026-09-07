import * as React from "react";
import { useVceJobObserverStore } from "./vce-job-observer-context";

// Retain action ownership for one mounted project and workbench service lifetime, without starting IO.
// 为一个已挂载项目和工作台服务生命周期保留操作归属，不启动输入输出。
// projectId is the authoritative UI project; null represents no admitted project.
// projectId 为权威界面项目；null 表示没有准入项目。
// Returns a stable predicate captured by each action; retired predicates remain false after A-B-A navigation.
// 返回供每次操作捕获的稳定判断函数；甲乙甲导航后，退役判断函数仍返回否。
export function useVceActionScope(projectId: string | null): () => boolean {
  // The existing store identity changes on service restart without remounting the conversation tree.
  // 既有仓库身份在服务重启时变化，不重新挂载会话树。
  const store = useVceJobObserverStore();
  // A fresh object distinguishes repeated visits to the same project from an earlier visit.
  // 新对象区分同一项目的重复访问与较早访问。
  const scope = React.useMemo(() => ({ projectId, store }), [projectId, store]);
  // Only a committed layout can accept responses; cleanup revokes ownership before passive effects run.
  // 只有已提交布局可接收响应；清理在被动副作用运行前撤销归属。
  const mounted = React.useRef<object | null>(null);
  React.useLayoutEffect(() => {
    mounted.current = scope;
    return () => { if (mounted.current === scope) mounted.current = null; };
  }, [scope]);
  return React.useCallback(() => projectId !== null && mounted.current === scope, [projectId, scope]);
}
