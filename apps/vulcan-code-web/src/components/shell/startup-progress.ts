import type { StartupSnapshot, StartupStepSnapshot } from "@/types";

// activeStartupStep prioritizes a failed step, then the actual running or pending headline.
// activeStartupStep 优先展示失败步骤，再选择实际运行中或待处理标题。
// steps is the service-owned ordered list; returns the matching step or null.
// steps 是服务持有的有序列表；返回匹配步骤或 null。
export function activeStartupStep(steps: StartupStepSnapshot[]): StartupStepSnapshot | null {
  return steps.find((step) => step.status === "error")
    ?? steps.find((step) => step.status === "running")
    ?? steps.find((step) => step.status === "pending")
    ?? null;
}

// startupProgressPercent preserves measured step progress without estimating step durations.
// startupProgressPercent 保留实测步骤进度，不估算步骤耗时。
// startup is the latest service snapshot; returns a bounded measurement or null for indeterminate progress.
// startup 是最新服务快照；返回有界实测值，或以 null 表示不定进度。
export function startupProgressPercent(startup: StartupSnapshot | null): number | null {
  if (startup?.ready) return 100;
  const step = startup ? activeStartupStep(startup.steps) : null;
  return step?.status === "running" && step.progressPercent != null && Number.isFinite(step.progressPercent)
    ? Math.max(0, Math.min(100, step.progressPercent)) : null;
}
