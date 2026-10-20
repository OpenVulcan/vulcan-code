import * as React from "react";
import { cx } from "../behavior/core";

// TextareaDensity selects the minimum height and padding of a multi-line field.
// TextareaDensity 选择多行输入框的最小高度与内边距。
export type TextareaDensity = "compact" | "default" | "large";

// TextareaProps are native textarea attributes plus density.
// TextareaProps 是原生多行输入框属性加上密度。
export type TextareaProps = React.ComponentProps<"textarea"> & {
  density?: TextareaDensity;
};

// Textarea renders the shared multi-line text field; it grows with its content.
// Textarea 渲染共享的多行文本输入框，随内容增高。
export function Textarea({ className, density = "default", ...props }: TextareaProps) {
  return <textarea data-slot="textarea" data-density={density} className={cx("vk-textarea", className)} {...props} />;
}
