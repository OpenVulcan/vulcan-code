import React from "react";
import { cx } from "@/ui-kit/behavior/core";

// SessionSettingsRow lays out one setting: its name and a short explanation on the left, its control on the right.
// SessionSettingsRow 排布一项设置：左侧为名称与简短说明，右侧为控件。
// Parameters: props carry the title, optional description, the section a slash command may target and the control; returns the row.
// 参数：props 携带标题、可选说明、slash 指令可定位的区块与控件；返回该行。
export function SessionSettingsRow(props: {
  title: React.ReactNode;
  description?: React.ReactNode;
  section?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="vk-ss-row" data-ss-section={props.section}>
      <div>
        <strong>{props.title}</strong>
        {props.description ? <small>{props.description}</small> : null}
      </div>
      {props.children}
    </div>
  );
}

// SessionSettingsCheck is one checkable line of a settings list; the whole line toggles, and the box only shows the state.
// SessionSettingsCheck 是设置列表中可勾选的一行；整行可切换，方框只用于显示状态。
// Parameters: props carry the checked state, an optional leading icon and the change callback; returns an accessible checkbox row.
// 参数：props 携带勾选状态、可选前置图标与变更回调；返回可访问的复选行。
export function SessionSettingsCheck({
  checked,
  icon,
  onCheckedChange,
  className,
  children,
  ...props
}: Omit<React.ComponentProps<"button">, "onChange"> & {
  checked: boolean;
  icon?: React.ReactNode;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      className={cx("vk-ss-check", className)}
      onClick={() => onCheckedChange(!checked)}
      {...props}
    >
      <span aria-hidden="true" className="vk-check" data-state={checked ? "checked" : "unchecked"} />
      {icon}
      <span className="vk-ss-check-label">{children}</span>
    </button>
  );
}
