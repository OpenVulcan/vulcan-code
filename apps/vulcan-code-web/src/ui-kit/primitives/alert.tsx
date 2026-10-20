import * as React from "react";
import { cx } from "../behavior/core";

// AlertVariant is the tone of an inline notice.
// AlertVariant 是内联提示的色调。
export type AlertVariant = "default" | "info" | "success" | "warning" | "destructive";

// ALERT_TONES maps alert variants onto kit tones.
// ALERT_TONES 将提示变体映射到 kit 色调。
const ALERT_TONES: Record<AlertVariant, string | undefined> = {
  default: undefined,
  info: "info",
  success: "ok",
  warning: "warn",
  destructive: "bad",
};

// Alert renders an inline notice; a leading icon child takes the first column.
// Alert 渲染内联提示；首个图标子元素占据第一列。
export function Alert({ className, variant = "default", ...props }: React.ComponentProps<"div"> & { variant?: AlertVariant | null }) {
  const resolved = variant ?? "default";
  return (
    <div
      data-slot="alert"
      data-variant={resolved}
      data-tone={ALERT_TONES[resolved]}
      role="alert"
      className={cx("vk-alert", className)}
      {...props}
    />
  );
}

// AlertTitle is the notice heading.
// AlertTitle 是提示标题。
export function AlertTitle({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="alert-title" className={cx("vk-alert-title", className)} {...props} />;
}

// AlertDescription is the notice body.
// AlertDescription 是提示正文。
export function AlertDescription({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="alert-description" className={cx("vk-alert-description", className)} {...props} />;
}

// AlertAction pins an action to the notice's top-right corner.
// AlertAction 将操作固定在提示右上角。
export function AlertAction({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="alert-action" className={cx("vk-alert-action", className)} {...props} />;
}
