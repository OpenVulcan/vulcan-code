import * as React from "react";
import { cx } from "../behavior/core";
import { useUiText } from "./ui-text";

// Spinner renders an indeterminate busy indicator with an accessible loading label.
// Spinner 渲染带可访问加载名称的不确定忙碌指示器。
export function Spinner({ className, ...props }: React.ComponentProps<"span">) {
  const text = useUiText();
  return <span role="status" aria-label={text.loading} data-slot="spinner" className={cx("vk-spinner", className)} {...props} />;
}
