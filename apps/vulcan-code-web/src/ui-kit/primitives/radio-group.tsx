import * as React from "react";
import { composeHandlers, cx, useComposedRefs, useControllableState } from "../behavior/core";
import { useRovingGroup } from "../behavior/roving-group";

// RadioGroupContextValue shares the current value and the selection callback with the items.
// RadioGroupContextValue 与各项共享当前值与选择回调。
interface RadioGroupContextValue {
  value: string;
  disabled: boolean;
  name?: string;
  select: (value: string) => void;
}

const RadioGroupContext = React.createContext<RadioGroupContextValue | null>(null);

// useRadioGroup reads the enclosing radio group.
// useRadioGroup 读取外层单选组。
export function useRadioGroup(): RadioGroupContextValue {
  const context = React.useContext(RadioGroupContext);
  if (!context) throw new Error("Radio items must be rendered inside a radio group.");
  return context;
}

// RadioGroupProps follow the Radix radio group.
// RadioGroupProps 沿用 Radix 单选组。
export interface RadioGroupProps extends Omit<React.ComponentProps<"div">, "defaultValue" | "onChange" | "dir"> {
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  disabled?: boolean;
  name?: string;
  required?: boolean;
  orientation?: "horizontal" | "vertical";
  loop?: boolean;
  dir?: "ltr" | "rtl";
}

// RadioGroupRoot is the unstyled group; RadioGroup and SegmentedControl dress it.
// RadioGroupRoot 是无样式的单选组；RadioGroup 与 SegmentedControl 为其提供外观。
export function RadioGroupRoot({
  value: controlled,
  defaultValue = "",
  onValueChange,
  disabled = false,
  name,
  required,
  orientation,
  loop = true,
  dir,
  ref,
  onKeyDown,
  onFocus,
  ...props
}: RadioGroupProps) {
  const element = React.useRef<HTMLDivElement | null>(null);
  const composed = useComposedRefs(element, ref);
  const [value, setValue] = useControllableState(controlled, defaultValue, onValueChange);
  const roving = useRovingGroup(element, '[role="radio"]', orientation ?? "both", {
    loop,
    dir,
    // Arrow keys check the newly focused radio, like native radio buttons.
    // 与原生单选按钮一致，方向键会选中新获得焦点的单选项。
    onFocusItem: (item) => item.click(),
  });
  const context = React.useMemo(() => ({ value, disabled, name, select: setValue }), [value, disabled, name, setValue]);
  return (
    <RadioGroupContext.Provider value={context}>
      <div
        ref={composed}
        role="radiogroup"
        aria-required={required}
        aria-orientation={orientation}
        data-disabled={disabled ? "" : undefined}
        data-vk-roving=""
        tabIndex={roving.tabIndex}
        onFocus={composeHandlers(onFocus, roving.onFocus)}
        onKeyDown={composeHandlers(onKeyDown, roving.onKeyDown)}
        {...props}
      />
      {name ? <input type="hidden" name={name} value={value} /> : null}
    </RadioGroupContext.Provider>
  );
}

// RadioGroup renders a vertical list of radio choices.
// RadioGroup 渲染纵向排列的单选项。
export function RadioGroup({ className, ...props }: RadioGroupProps) {
  return <RadioGroupRoot data-slot="radio-group" className={cx("vk-radio-group", className)} {...props} />;
}

// RadioItemProps are button attributes plus the item value.
// RadioItemProps 是按钮属性加上项值。
export interface RadioItemProps extends Omit<React.ComponentProps<"button">, "value"> {
  value: string;
}

// RadioItem is the unstyled radio button bound to the enclosing group.
// RadioItem 是绑定外层单选组的无样式单选按钮。
export function RadioItem({ value, disabled, onClick, onKeyDown, ...props }: RadioItemProps) {
  const group = useRadioGroup();
  const checked = group.value === value;
  const isDisabled = group.disabled || disabled;
  return (
    <button
      type="button"
      role="radio"
      aria-checked={checked}
      data-state={checked ? "checked" : "unchecked"}
      data-disabled={isDisabled ? "" : undefined}
      disabled={isDisabled}
      value={value}
      onClick={composeHandlers(onClick, () => group.select(value))}
      onKeyDown={composeHandlers(onKeyDown, (event) => {
        // Enter does not check a radio, matching native behavior.
        // 与原生行为一致，Enter 不会选中单选项。
        if (event.key === "Enter") event.preventDefault();
      })}
      {...props}
    />
  );
}

// RadioGroupItem renders one radio dot.
// RadioGroupItem 渲染一个单选圆点。
export function RadioGroupItem({ className, ...props }: RadioItemProps) {
  return <RadioItem data-slot="radio-group-item" className={cx("vk-radio", className)} {...props} />;
}
