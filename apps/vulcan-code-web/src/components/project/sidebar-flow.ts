import React from "react";

// SidebarFlow names the two ways the navigation panel lists sessions: the current project's sessions by time,
// or every project as an expandable tree.
// SidebarFlow 命名导航面板列出会话的两种方式：当前项目的会话按时间排列，或以可展开树浏览全部项目。
export type SidebarFlow = "session" | "code";

// SIDEBAR_FLOW_STORAGE_KEY keeps the viewer's last flow choice in this browser only.
// SIDEBAR_FLOW_STORAGE_KEY 只在当前浏览器中保存查看者上次选择的流。
const SIDEBAR_FLOW_STORAGE_KEY = "vulcan.gui.sidebar-flow.v1";

// currentFlow is the shared in-memory value; the panel and the rail flyout both read it.
// currentFlow 是共享的内存值；面板与轨道浮层都读取它。
let currentFlow: SidebarFlow = readStoredFlow();
// listeners are notified after the flow changes.
// listeners 在流变化后收到通知。
const listeners = new Set<() => void>();

// readStoredFlow restores the stored choice, falling back to the session flow when storage is unavailable.
// readStoredFlow 恢复已保存的选择；存储不可用时回退到会话流。
function readStoredFlow(): SidebarFlow {
  try {
    return globalThis.localStorage?.getItem(SIDEBAR_FLOW_STORAGE_KEY) === "code" ? "code" : "session";
  } catch {
    return "session";
  }
}

// setSidebarFlow switches every navigation surface to one flow and remembers it for this browser.
// setSidebarFlow 将所有导航界面切换到同一流，并在当前浏览器中记住该选择。
export function setSidebarFlow(flow: SidebarFlow): void {
  if (flow === currentFlow) return;
  currentFlow = flow;
  try {
    globalThis.localStorage?.setItem(SIDEBAR_FLOW_STORAGE_KEY, flow);
  } catch {
    // Storage is a convenience; the in-memory choice still applies for this page.
    // 存储只是便利功能；内存中的选择在本页面内仍然生效。
  }
  listeners.forEach((listener) => listener());
}

// subscribe registers one listener for flow changes.
// subscribe 注册一个流变化监听器。
function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

// readFlow returns the current shared flow.
// readFlow 返回当前共享的流。
function readFlow(): SidebarFlow {
  return currentFlow;
}

// useSidebarFlow binds a component to the shared navigation flow.
// useSidebarFlow 将组件绑定到共享的导航流。
export function useSidebarFlow(): SidebarFlow {
  return React.useSyncExternalStore(subscribe, readFlow, readFlow);
}
