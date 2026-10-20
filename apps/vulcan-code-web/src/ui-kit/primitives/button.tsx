import * as React from "react";
import { cx } from "../behavior/core";
import { Slot } from "../behavior/slot";

// ButtonVariant is the visual hierarchy of an ordinary action. The last three names are the earlier
// vocabulary and resolve to their kit equivalents, so existing call sites keep working.
// ButtonVariant 是普通操作的视觉层级。最后三个名称是早期词汇，会解析为对应的 kit 层级，使既有调用处继续可用。
export type ButtonVariant =
  | "primary" | "outline" | "quiet" | "ghost" | "danger" | "link" | "ember"
  | "default" | "secondary" | "destructive";

// ButtonSize is the height of a text button, or the square size of an icon-only button.
// ButtonSize 是文本按钮的高度，或纯图标按钮的方形尺寸。
export type ButtonSize = "default" | "xs" | "sm" | "lg" | "icon" | "icon-xs" | "icon-sm" | "icon-lg";

// VARIANT_ALIASES maps the earlier names onto the kit hierarchy.
// VARIANT_ALIASES 将早期名称映射到 kit 层级。
const VARIANT_ALIASES: Partial<Record<ButtonVariant, ButtonVariant>> = {
  default: "primary",
  secondary: "quiet",
  destructive: "danger",
};

// resolveButtonVariant returns the kit name written to data-variant.
// resolveButtonVariant 返回写入 data-variant 的 kit 名称。
export function resolveButtonVariant(variant: ButtonVariant): ButtonVariant {
  return VARIANT_ALIASES[variant] ?? variant;
}

// ButtonProps are native button attributes plus hierarchy, size and asChild.
// ButtonProps 是原生按钮属性加上层级、尺寸与 asChild。
export interface ButtonProps extends React.ComponentProps<"button"> {
  variant?: ButtonVariant | null;
  size?: ButtonSize | null;
  asChild?: boolean;
}

// Button renders the shared action control; the kit stylesheet owns every visual state.
// Button 渲染共享操作控件；所有视觉状态由 kit 样式负责。
export function Button({ className, variant, size, asChild = false, ...props }: ButtonProps) {
  const Comp = asChild ? Slot : "button";
  const resolvedSize = size ?? "default";
  return (
    <Comp
      data-slot="button"
      data-variant={resolveButtonVariant(variant ?? "primary")}
      data-size={resolvedSize}
      data-shape={resolvedSize.startsWith("icon") ? "square" : undefined}
      className={cx("vk-btn", className)}
      {...(props as React.HTMLAttributes<HTMLElement>)}
    />
  );
}

// IconButtonSize identifies the four square action dimensions.
// IconButtonSize 标识四种方形操作尺寸。
export type IconButtonSize = "xs" | "sm" | "md" | "lg";

// IconButtonProps defines an icon-only action without the text-button sizes.
// IconButtonProps 定义纯图标操作，并隐藏文本按钮尺寸。
export interface IconButtonProps extends Omit<ButtonProps, "size"> {
  size?: IconButtonSize;
}

// ICON_BUTTON_SIZES maps semantic square sizes onto Button sizes.
// ICON_BUTTON_SIZES 将语义方形尺寸映射到 Button 尺寸。
const ICON_BUTTON_SIZES: Record<IconButtonSize, ButtonSize> = {
  xs: "icon-xs",
  sm: "icon-sm",
  md: "icon",
  lg: "icon-lg",
};

// IconButton renders icon-only actions through the same visual and state system as Button.
// IconButton 通过与 Button 相同的视觉和状态体系渲染纯图标操作。
export function IconButton({ size = "sm", variant = "ghost", ...props }: IconButtonProps) {
  return <Button size={ICON_BUTTON_SIZES[size]} variant={variant} {...props} />;
}
