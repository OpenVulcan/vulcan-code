import { useI18n } from "@/i18n";
import { cx } from "../behavior/core";

// ProgressRingProps describe a small circular progress mark; value is 0..1 and null means unknown.
// ProgressRingProps 描述小型环形进度标记；value 取 0..1，null 表示未知。
export interface ProgressRingProps {
  "aria-label"?: string;
  className?: string;
  size?: number;
  strokeWidth?: number;
  value?: number | null;
}

// ProgressRing renders a ring whose arc follows the value; unknown progress shows a short arc.
// ProgressRing 渲染随数值变化的圆环；未知进度显示一段短弧。
export function ProgressRing(props: ProgressRingProps) {
  const { t } = useI18n();
  const size = props.size ?? 14;
  const strokeWidth = props.strokeWidth ?? 2;
  const center = size / 2;
  const radius = Math.max(0, (size - strokeWidth) / 2);
  const circumference = 2 * Math.PI * radius;
  const progressValue = typeof props.value === "number" ? clampProgressValue(props.value) : 0.18;
  const dashOffset = circumference * (1 - progressValue);
  return (
    <svg
      aria-label={props["aria-label"] ?? t("global.progress")}
      className={cx("vk-ring", props.className)}
      data-indeterminate={typeof props.value === "number" ? undefined : ""}
      height={size}
      role="img"
      viewBox={`0 0 ${size} ${size}`}
      width={size}
    >
      <circle className="vk-ring-track" cx={center} cy={center} fill="none" r={radius} strokeWidth={strokeWidth} />
      <circle
        className="vk-ring-value"
        cx={center}
        cy={center}
        fill="none"
        r={radius}
        strokeDasharray={circumference}
        strokeDashoffset={dashOffset}
        strokeWidth={strokeWidth}
        transform={`rotate(-90 ${center} ${center})`}
      />
    </svg>
  );
}

// clampProgressValue keeps a finite value inside 0..1.
// clampProgressValue 将有限数值限制在 0..1 之间。
function clampProgressValue(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}
