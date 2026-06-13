import { StatusIcon } from "@/components/shared/status";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/utils";
import type { StartupSnapshot, StartupStepSnapshot } from "@/types";
import {
  Activity,
  CircleAlert,
} from "lucide-react";

// LoadingScreen renders the earliest frontend loading state.
// LoadingScreen 渲染最早期的前端加载状态。
export function LoadingScreen(props: {
  // Startup snapshot received before the workbench snapshot is ready.
  // 在工作台快照就绪前已经收到的启动快照。
  startup?: StartupSnapshot | null;
}) {
  return (
    <main className="loading-screen">
      {props.startup ? (
        <StartupProgressCard startup={props.startup} />
      ) : (
        <>
          <Activity className="spin" size={28} />
          <span>正在连接 Vulcan Core Service...</span>
        </>
      )}
    </main>
  );
}

// StartupOverlay renders the in-shell bootstrap blocker below the main header.
// StartupOverlay 渲染位于主标题栏下方的壳内启动阻塞层。
export function StartupOverlay(props: {
  // Latest service startup snapshot.
  // 最近一次服务启动快照。
  startup: StartupSnapshot;
}) {
  return (
    <div className="startup-overlay" role="status" aria-live="polite" aria-busy="true">
      <StartupProgressCard startup={props.startup} />
    </div>
  );
}

// StartupProgressCard renders the blocking runtime bootstrap progress.
// StartupProgressCard 渲染阻塞式运行时启动进度。
function StartupProgressCard(props: {
  // Latest service startup snapshot.
  // 最近一次服务启动快照。
  startup: StartupSnapshot;
}) {
  // activeStep selects the current startup row for the progress headline.
  // activeStep 选择当前启动步骤作为进度标题。
  const activeStep = activeStartupStep(props.startup.steps);
  return (
    <section className="startup-progress-card" aria-live="polite" aria-busy={!props.startup.ready}>
      <div className="startup-progress-heading">
        <Spinner className="size-5" />
        <div>
          <strong>{activeStep ? `正在加载 ${activeStep.label}` : "正在加载 Vulcan 运行时"}</strong>
          <span>{activeStep?.message ?? "正在加载 VLDB、LuaSkills 与 Token 预算运行时。"}</span>
        </div>
      </div>
      <div className="startup-progress-steps">
        {props.startup.steps.map((step) => (
          <StartupProgressStep key={step.id} step={step} />
        ))}
      </div>
    </section>
  );
}

// StartupProgressStep renders one runtime bootstrap row.
// StartupProgressStep 渲染单个运行时启动步骤行。
function StartupProgressStep(props: {
  // Step snapshot displayed by this row.
  // 此行展示的步骤快照。
  step: StartupStepSnapshot;
}) {
  return (
    <div className={cn("startup-progress-step", props.step.status)}>
      {props.step.status === "running" ? <Spinner className="size-4" /> : <StatusIcon status={props.step.status} />}
      <div>
        <strong>{props.step.label}</strong>
        <span>{props.step.message ?? "等待状态更新"}</span>
      </div>
    </div>
  );
}

// activeStartupStep returns the first in-progress or pending startup step for the headline.
// activeStartupStep 返回首个进行中或待处理启动步骤，用于标题显示。
function activeStartupStep(steps: StartupStepSnapshot[]): StartupStepSnapshot | null {
  return steps.find((step) => step.status === "running")
    ?? steps.find((step) => step.status === "pending")
    ?? null;
}

// ErrorScreen renders a startup failure.
// ErrorScreen 渲染启动失败状态。
export function ErrorScreen(props: {
  // Error message rendered after startup fails.
  // 启动失败后展示的错误消息。
  error: string;
}) {
  return (
    <main className="loading-screen error">
      <CircleAlert size={28} />
      <span>启动失败：{props.error}</span>
    </main>
  );
}
