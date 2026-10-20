import * as React from "react";
import { cx } from "../behavior/core";

// NavItemProps are button attributes plus selection and density.
// NavItemProps 是按钮属性加上选中状态与密度。
export interface NavItemProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  selected?: boolean | null;
  density?: "compact" | "default" | null;
}

// navigationItemVariants returns the kit classes for a navigation row (kept for callers styling links).
// navigationItemVariants 返回导航行的 kit 类名（为给链接套用样式的调用方保留）。
export function navigationItemVariants(_options: { selected?: boolean | null; density?: "compact" | "default" | null } = {}): string {
  return "vk-nav-item";
}

// NavItem renders one selectable navigation row.
// NavItem 渲染一个可选中的导航行。
export function NavItem({ className, density, selected, type = "button", ...props }: NavItemProps) {
  return (
    <button
      aria-current={selected ? "page" : undefined}
      className={cx("vk-nav-item", className)}
      data-density={density ?? "default"}
      data-selected={selected ? "true" : "false"}
      data-slot="nav-item"
      type={type}
      {...props}
    />
  );
}

// TreeRowProps add the nesting depth.
// TreeRowProps 增加嵌套深度。
export interface TreeRowProps extends NavItemProps {
  depth?: number;
}

// TreeRow renders a navigation row indented by its depth.
// TreeRow 渲染按深度缩进的导航行。
export function TreeRow({ className, depth = 0, style, ...props }: TreeRowProps) {
  return (
    <NavItem
      className={cx("vk-indent-row", className)}
      data-depth={depth}
      data-slot="tree-row"
      role="treeitem"
      style={{ paddingInlineStart: `calc(var(--vk-u) * 2 + ${depth} * var(--vk-u) * 4)`, ...style }}
      {...props}
    />
  );
}
