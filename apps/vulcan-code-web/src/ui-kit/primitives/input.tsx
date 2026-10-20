import * as React from "react";
import { cx } from "../behavior/core";

// InputDensity selects the control height: compact for dense tables, large for prominent forms.
// InputDensity 选择控件高度：密集表格用 compact，醒目表单用 large。
export type InputDensity = "compact" | "default" | "large";

// InputProps are native input attributes plus density.
// InputProps 是原生输入框属性加上密度。
export type InputProps = React.ComponentProps<"input"> & {
  density?: InputDensity;
};

// Input renders the shared single-line text field.
// Input 渲染共享的单行文本输入框。
export function Input({ className, density = "default", ...props }: InputProps) {
  return <input data-slot="input" data-density={density} className={cx("vk-input", className)} {...props} />;
}
