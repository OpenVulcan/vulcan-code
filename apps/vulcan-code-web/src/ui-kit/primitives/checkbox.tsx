import * as React from "react";
import { cx, useControllableState } from "../behavior/core";

// CheckedState is a checkbox value; "indeterminate" marks a partially selected group.
// CheckedState 是复选框的值；"indeterminate" 表示部分选中的组。
export type CheckedState = boolean | "indeterminate";

// CheckboxProps follow the Radix checkbox: checked, defaultChecked and onCheckedChange on a button.
// CheckboxProps 沿用 Radix 复选框：在按钮上提供 checked、defaultChecked 与 onCheckedChange。
export interface CheckboxProps extends Omit<React.ComponentProps<"button">, "value" | "onChange" | "checked" | "defaultChecked"> {
  checked?: CheckedState;
  defaultChecked?: CheckedState;
  onCheckedChange?: (checked: CheckedState) => void;
  required?: boolean;
  name?: string;
  value?: string;
}

// Checkbox renders a tri-state check box; activating an indeterminate box checks it.
// Checkbox 渲染三态复选框；激活不确定状态的复选框会将其选中。
export function Checkbox({ className, checked, defaultChecked = false, onCheckedChange, onClick, onKeyDown, name, value = "on", required, ...props }: CheckboxProps) {
  const [state, setState] = useControllableState<CheckedState>(checked, defaultChecked, onCheckedChange);
  const dataState = state === "indeterminate" ? "indeterminate" : state ? "checked" : "unchecked";
  return (
    <>
      <button
        type="button"
        role="checkbox"
        aria-checked={state === "indeterminate" ? "mixed" : state}
        aria-required={required}
        data-state={dataState}
        data-slot="checkbox"
        className={cx("vk-check", className)}
        onClick={(event) => {
          onClick?.(event);
          if (!event.defaultPrevented) setState(state === "indeterminate" ? true : !state);
        }}
        onKeyDown={(event) => {
          // Enter never toggles a checkbox, matching native behavior.
          // 与原生行为一致，Enter 不切换复选框。
          onKeyDown?.(event);
          if (event.key === "Enter") event.preventDefault();
        }}
        {...props}
      />
      {name ? <input type="hidden" name={name} value={state === true ? value : ""} /> : null}
    </>
  );
}
