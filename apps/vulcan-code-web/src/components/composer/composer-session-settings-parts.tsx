import React from "react";
import { cx } from "@/ui-kit/behavior/core";

// SessionSettingsGroup is one item of the session section: a heading line with an icon, the item name, an optional
// count and an optional go-to action, followed by its controls. Consecutive groups are divided by a rule.
// SessionSettingsGroup 是会话区块中的一项：带图标、名称、可选计数与可选前往操作的标题行，其下为控件。相邻项之间以分隔线划分。
// Parameters: props carry the icon, title, the section a slash command may target, count, action and body; returns the group.
// 参数：props 携带图标、标题、slash 指令可定位的区块、计数、操作与正文；返回该项。
export function SessionSettingsGroup(props: {
  icon: React.ReactNode;
  title: React.ReactNode;
  section?: string;
  count?: number;
  action?: React.ReactNode;
  busy?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div aria-busy={props.busy} className="vk-ss-block" data-ss-section={props.section}>
      <div className="vk-ss-sub">
        {props.icon}
        <span>{props.title}</span>
        {props.count ? <span className="vk-ss-count vk-num">{props.count}</span> : null}
        {props.action}
      </div>
      {props.children}
    </div>
  );
}

// SessionSettingsRow lays out one setting: its name and a short explanation on the left, its control on the right.
// Inside a group whose heading already names the setting, the row carries only the explanation.
// SessionSettingsRow 排布一项设置：左侧为名称与简短说明，右侧为控件。位于标题已命名该设置的分组内时，该行只带说明。
// Parameters: props carry the optional title and description, the section a slash command may target and the control; returns the row.
// 参数：props 携带可选标题与说明、slash 指令可定位的区块与控件；返回该行。
export function SessionSettingsRow(props: {
  title?: React.ReactNode;
  description?: React.ReactNode;
  section?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="vk-ss-row" data-ss-section={props.section}>
      <div>
        {props.title ? <strong>{props.title}</strong> : null}
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
