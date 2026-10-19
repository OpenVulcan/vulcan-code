import * as React from "react";
import { cx } from "../behavior/core";
import { Button, type ButtonProps } from "./button";
import { Input } from "./input";
import { Textarea } from "./textarea";

// InputGroup frames one text control with leading or trailing addons inside a single border.
// InputGroup 在同一边框内为一个文本控件配置前置或后置附加内容。
export function InputGroup({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="input-group" role="group" className={cx("vk-input-group", className)} {...props} />;
}

// InputGroupAddonAlign places an addon before, after, above or below the control.
// InputGroupAddonAlign 将附加内容放在控件之前、之后、上方或下方。
export type InputGroupAddonAlign = "inline-start" | "inline-end" | "block-start" | "block-end";

// InputGroupAddon holds icons, text or buttons; clicking its non-button area focuses the control.
// InputGroupAddon 承载图标、文字或按钮；点击其非按钮区域会聚焦控件。
export function InputGroupAddon({
  className,
  align = "inline-start",
  onClick,
  ...props
}: React.ComponentProps<"div"> & { align?: InputGroupAddonAlign | null }) {
  return (
    <div
      role="group"
      data-slot="input-group-addon"
      data-align={align ?? "inline-start"}
      className={cx("vk-input-group-addon", className)}
      onClick={(event) => {
        onClick?.(event);
        if (event.defaultPrevented || (event.target as HTMLElement).closest("button")) return;
        event.currentTarget.parentElement?.querySelector<HTMLInputElement | HTMLTextAreaElement>("input, textarea")?.focus();
      }}
      {...props}
    />
  );
}

// InputGroupButton is a compact ghost button that sits inside an addon.
// InputGroupButton 是位于附加区内的紧凑幽灵按钮。
export function InputGroupButton({
  type = "button",
  variant = "ghost",
  size = "xs",
  ...props
}: Omit<ButtonProps, "size"> & { size?: "xs" | "sm" | "icon-xs" | "icon-sm" | null }) {
  return <Button type={type} variant={variant} size={size ?? "xs"} {...props} />;
}

// InputGroupText is plain text inside an addon.
// InputGroupText 是附加区内的纯文本。
export function InputGroupText({ className, ...props }: React.ComponentProps<"span">) {
  return <span className={cx("vk-input-group-text", className)} {...props} />;
}

// InputGroupInput is the borderless single-line control of the group.
// InputGroupInput 是组内无边框的单行控件。
export function InputGroupInput(props: React.ComponentProps<"input">) {
  return <Input data-slot="input-group-control" {...props} />;
}

// InputGroupTextarea is the borderless multi-line control of the group.
// InputGroupTextarea 是组内无边框的多行控件。
export function InputGroupTextarea(props: React.ComponentProps<"textarea">) {
  return <Textarea data-slot="input-group-control" {...props} />;
}
