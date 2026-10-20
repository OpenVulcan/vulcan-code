import * as React from "react";
import { cx } from "../behavior/core";

// Label renders a form label; a double click on it never selects its text.
// Label 渲染表单标签；双击标签不会选中其文字。
export function Label({ className, onMouseDown, ...props }: React.ComponentProps<"label">) {
  return (
    <label
      data-slot="label"
      className={cx("vk-form-label", className)}
      onMouseDown={(event) => {
        onMouseDown?.(event);
        if (!event.defaultPrevented && event.detail > 1) event.preventDefault();
      }}
      {...props}
    />
  );
}
