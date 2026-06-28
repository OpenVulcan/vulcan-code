import React from "react";

import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";

// ProgressRingProps describes the compact static progress ring contract.
// ProgressRingProps 描述紧凑静态进度圆环的使用契约。
export interface ProgressRingProps {
  // Optional accessible label for the progress ring.
  // 进度圆环的可选无障碍标签。
  "aria-label"?: string;
  // Optional CSS class name merged onto the SVG element.
  // 合并到 SVG 元素上的可选 CSS 类名。
  className?: string;
  // Visual square size in CSS pixels.
  // 视觉方形尺寸，单位为 CSS 像素。
  size?: number;
  // Stroke width in CSS pixels.
  // 描边宽度，单位为 CSS 像素。
  strokeWidth?: number;
  // Progress value between 0 and 1; omitted values render a stable running crescent.
  // 0 到 1 之间的进度值；省略时渲染稳定的运行中月牙。
  value?: number | null;
}

// ProgressRing renders a non-animated circular progress crescent.
// ProgressRing 渲染一个非动画的圆形进度月牙。
export function ProgressRing(props: ProgressRingProps) {
  const { t } = useI18n();
  const size = props.size ?? 14;
  const strokeWidth = props.strokeWidth ?? 2;
  const center = size / 2;
  const radius = Math.max(0, (size - strokeWidth) / 2);
  const circumference = 2 * Math.PI * radius;
  const progressValue = typeof props.value === "number" ? clampProgressValue(props.value) : 0.18;
  const dashOffset = circumference * (1 - progressValue);
  const ariaLabel = props["aria-label"] ?? t("global.progress");
  return (
    <svg
      aria-label={ariaLabel}
      className={cn("progress-ring", props.className)}
      height={size}
      role="img"
      viewBox={`0 0 ${size} ${size}`}
      width={size}
    >
      <circle
        className="progress-ring-track"
        cx={center}
        cy={center}
        fill="none"
        r={radius}
        strokeWidth={strokeWidth}
      />
      <circle
        className="progress-ring-value"
        cx={center}
        cy={center}
        fill="none"
        r={radius}
        strokeDasharray={circumference}
        strokeDashoffset={dashOffset}
        strokeLinecap="round"
        strokeWidth={strokeWidth}
        transform={`rotate(-90 ${center} ${center})`}
      />
    </svg>
  );
}

// clampProgressValue keeps the ring display inside valid visual bounds.
// clampProgressValue 将圆环显示限制在有效的视觉范围内。
function clampProgressValue(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.min(1, Math.max(0, value));
}
