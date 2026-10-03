import type { StartupStepSnapshot } from "@/types";
import { formatBytes } from "@/lib/display-format";
import { StartupProgressBar } from "./startup-progress-bar";

// StartupDownloadProgressProps contains the active startup step and localized label.
// StartupDownloadProgressProps 包含当前启动步骤与本地化标签。
interface StartupDownloadProgressProps {
  // Active startup step that may carry byte-level download progress.
  // 可能携带字节级下载进度的当前启动步骤。
  step: StartupStepSnapshot | null;
  // Localized heading displayed above the download bar.
  // 显示在下载进度条上方的本地化标题。
  label: string;
}

// StartupDownloadProgress renders byte progress only while one download is incomplete.
// StartupDownloadProgress 仅在某个下载未完成时渲染字节进度。
// Parameters: props supplies the current optional download step and accessible label; returns measured or indeterminate download progress when active.
// 参数：props 提供当前可选下载步骤及可访问标签；活动时返回实测或不定下载进度。
export function StartupDownloadProgress(props: StartupDownloadProgressProps) {
  // progress contains a validated active byte-transfer snapshot or null outside downloads.
  // progress 包含已校验的活动字节传输快照，非下载阶段为 null。
  const progress = activeDownloadProgress(props.step);
  if (!progress) {
    return null;
  }
  return (
    <div className="startup-download-progress" role="group" aria-label={props.label}>
      <div className="startup-download-progress-heading">
        <span>{props.label}</span>
        <span title={progress.kind === "indeterminate" ? props.step?.message ?? undefined : undefined}>{progress.kind === "determinate" ? `${formatBytes(progress.completedBytes)} / ${formatBytes(progress.totalBytes)}` : props.step?.message}</span>
      </div>
      <StartupProgressBar percent={progress.kind === "determinate" ? progress.percent : null} label={props.label} compact />
    </div>
  );
}

// ActiveDownloadProgress is the normalized byte-transfer state consumed by the visual component.
// ActiveDownloadProgress 是可视化组件消费的规范字节传输状态。
type ActiveDownloadProgress = DeterminateDownloadProgress | IndeterminateDownloadProgress;

// DeterminateDownloadProgress contains an exact byte ratio.
// DeterminateDownloadProgress 包含精确的字节比例。
interface DeterminateDownloadProgress {
  kind: "determinate";
  // Completed byte count clamped to the advertised total.
  // 限制在已声明总量内的已完成字节数。
  completedBytes: number;
  // Positive advertised total byte count.
  // 正数的已声明总字节数。
  totalBytes: number;
  // Bounded transfer percentage.
  // 有界的传输百分比。
  percent: number;
}

// IndeterminateDownloadProgress identifies a download whose upstream total is unavailable.
// IndeterminateDownloadProgress 标识上游总量不可用的下载。
interface IndeterminateDownloadProgress {
  kind: "indeterminate";
}

// activeDownloadProgress accepts a running download and selects exact or indeterminate presentation.
// activeDownloadProgress 接受正在运行的下载，并选择精确或不定进度展示。
function activeDownloadProgress(step: StartupStepSnapshot | null): ActiveDownloadProgress | null {
  if (step?.status !== "running" || step.progressPhase !== "downloading") {
    return null;
  }
  if (step.completedBytes == null || step.totalBytes == null || step.totalBytes <= 0) {
    return { kind: "indeterminate" };
  }
  // completedBytes prevents malformed upstream values from overflowing the progress presentation.
  // completedBytes 防止异常上游值溢出进度展示。
  const completedBytes = Math.max(0, Math.min(step.completedBytes, step.totalBytes));
  if (completedBytes >= step.totalBytes) {
    return null;
  }
  return {
    kind: "determinate",
    completedBytes,
    totalBytes: step.totalBytes,
    percent: (completedBytes / step.totalBytes) * 100,
  };
}
