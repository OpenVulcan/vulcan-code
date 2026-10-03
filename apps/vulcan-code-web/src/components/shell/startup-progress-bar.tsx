import type { CSSProperties } from "react";

// StartupProgressBar renders one startup meter: a measured fill when percent is known, otherwise a sweeping segment
// that never pretends to be a numeric value.
// StartupProgressBar 渲染一条启动进度条：已知百分比时显示实测填充，否则显示扫动片段，绝不伪装成数值。
export function StartupProgressBar(props: {
  // Bounded measured percentage, or null for indeterminate progress.
  // 有界实测百分比；不定进度时为 null。
  percent: number | null;
  // Accessible name of the measured operation.
  // 被测操作的可访问名称。
  label: string;
  // Optional size variant; the compact bar belongs to secondary transfers such as downloads.
  // 可选尺寸变体；紧凑进度条用于下载等次要传输。
  compact?: boolean;
}) {
  // fillStyle scales the fill instead of animating width, keeping updates on the compositor.
  // fillStyle 通过缩放而非宽度动画呈现填充，使更新留在合成层。
  const fillStyle = props.percent !== null
    ? ({ "--startup-progress-ratio": String(props.percent / 100) } as CSSProperties)
    : undefined;
  return (
    <div
      className={`startup-progress-track${props.compact ? " compact" : ""}`}
      role="progressbar"
      aria-label={props.label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={props.percent !== null ? Math.round(props.percent) : undefined}
    >
      <div className={`startup-progress-fill${props.percent === null ? " indeterminate" : ""}`} style={fillStyle} />
    </div>
  );
}
