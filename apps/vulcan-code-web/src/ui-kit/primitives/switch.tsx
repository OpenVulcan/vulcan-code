import * as React from "react";
import { cx, useControllableState } from "../behavior/core";

// SwitchProps follow the Radix switch: checked, defaultChecked and onCheckedChange on a button.
// SwitchProps 沿用 Radix 开关：在按钮上提供 checked、defaultChecked 与 onCheckedChange。
export interface SwitchProps extends Omit<React.ComponentProps<"button">, "value" | "onChange"> {
  checked?: boolean;
  defaultChecked?: boolean;
  onCheckedChange?: (checked: boolean) => void;
  required?: boolean;
  name?: string;
  value?: string;
}

// Switch renders an on/off toggle for settings that apply immediately.
// Switch 渲染用于即时生效设置的开关。
export function Switch({ className, checked, defaultChecked = false, onCheckedChange, onClick, name, value = "on", required, ...props }: SwitchProps) {
  const [on, setOn] = useControllableState(checked, defaultChecked, onCheckedChange);
  return (
    <>
      <button
        type="button"
        role="switch"
        aria-checked={on}
        aria-required={required}
        data-state={on ? "checked" : "unchecked"}
        data-slot="switch"
        className={cx("vk-switch", className)}
        onClick={(event) => {
          onClick?.(event);
          if (!event.defaultPrevented) setOn(!on);
        }}
        {...props}
      />
      {name ? <input type="hidden" name={name} value={on ? value : ""} /> : null}
    </>
  );
}
