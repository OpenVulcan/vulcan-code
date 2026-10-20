import React from "react";
import { composeRefs, cx } from "./core";

// SlotProps are the props a primitive forwards onto its single child when rendered with asChild.
// SlotProps 是原语以 asChild 渲染时转发到唯一子元素的属性。
type SlotProps = React.HTMLAttributes<HTMLElement> & { ref?: React.Ref<HTMLElement>; [key: string]: unknown };

// mergeSlotProps merges primitive props into the child's props: handlers run child first, then the
// primitive unless the child prevented default; classes join; styles merge with the child winning.
// mergeSlotProps 将原语属性合并到子元素属性：处理器先运行子元素的，子元素未阻止默认行为时再运行原语的；
// 类名拼接；样式合并且子元素优先。
function mergeSlotProps(slotProps: Record<string, unknown>, childProps: Record<string, unknown>): Record<string, unknown> {
  const merged: Record<string, unknown> = { ...slotProps, ...childProps };
  for (const key of Object.keys(slotProps)) {
    const slotValue = slotProps[key];
    const childValue = childProps[key];
    if (/^on[A-Z]/.test(key) && typeof slotValue === "function" && typeof childValue === "function") {
      merged[key] = (event: { defaultPrevented?: boolean }, ...rest: unknown[]) => {
        (childValue as (...args: unknown[]) => void)(event, ...rest);
        if (!event?.defaultPrevented) (slotValue as (...args: unknown[]) => void)(event, ...rest);
      };
    } else if (key === "className") {
      merged.className = cx(slotValue as string, childValue as string);
    } else if (key === "style") {
      merged.style = { ...(slotValue as object), ...(childValue as object) };
    }
  }
  return merged;
}

// Slot renders its only child element with the primitive's props and ref merged in (Radix-style asChild).
// Slot 以合并原语属性与引用后的形式渲染唯一子元素（Radix 风格的 asChild）。
export const Slot = React.forwardRef<HTMLElement, SlotProps>(function Slot({ children, ...slotProps }, forwardedRef) {
  if (!React.isValidElement(children)) return React.Children.count(children) > 1 ? React.Children.only(null) : null;
  const child = children as React.ReactElement<Record<string, unknown> & { ref?: React.Ref<HTMLElement> }>;
  const childRef = child.props.ref;
  return React.cloneElement(child, {
    ...mergeSlotProps(slotProps, child.props),
    ref: forwardedRef ? composeRefs(forwardedRef, childRef) : childRef,
  });
});
