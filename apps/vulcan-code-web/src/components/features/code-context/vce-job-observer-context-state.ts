import * as React from "react";
import type { VceActivityObserverStore } from "./vce-activity-observer-store";
import type { VceJobObserverStore } from "./vce-job-observer-store";

// VceJobObserverContext is kept in a stable module so provider and hook updates share one identity.
// VceJobObserverContext 保存在稳定模块中，确保 Provider 与 Hook 更新后仍共享同一身份。
export const VceJobObserverContext = React.createContext<VceJobObserverStore | null>(null);

// VceActivityObserverContext shares the lightweight project observer with the job provider lifetime.
// VceActivityObserverContext 在作业 Provider 生命周期内共享轻量项目观察器。
export const VceActivityObserverContext = React.createContext<VceActivityObserverStore | null>(null);
