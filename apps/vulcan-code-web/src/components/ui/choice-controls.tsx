"use client"

import * as React from "react"
import { RadioGroup as RadioGroupPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"

// SegmentedControlContextValue shares controlled selection with child items.
// SegmentedControlContextValue 与子选项共享受控选择状态。
interface SegmentedControlContextValue {
  // Whether every option is disabled by the parent control.
  // 父级选择器是否禁用全部选项。
  disabled: boolean
  // Currently selected option value.
  // 当前选中的选项值。
  value: string
}

// SEGMENTED_CONTROL_EMPTY_CONTEXT protects standalone items from implicit state ownership.
// SEGMENTED_CONTROL_EMPTY_CONTEXT 防止独立选项隐式持有选择状态。
const SEGMENTED_CONTROL_EMPTY_CONTEXT: SegmentedControlContextValue = {
  disabled: false,
  value: "",
}

// SegmentedControlContext carries selection state without another UI runtime dependency.
// SegmentedControlContext 在不引入额外 UI 运行时依赖的情况下传递选择状态。
const SegmentedControlContext = React.createContext(SEGMENTED_CONTROL_EMPTY_CONTEXT)

// SegmentedControlProps describes one mutually exclusive compact choice group.
// SegmentedControlProps 描述一个互斥的紧凑选项组。
export interface SegmentedControlProps extends Omit<React.ComponentProps<typeof RadioGroupPrimitive.Root>, "defaultValue" | "onChange"> {
  // Initial value used when the control owns its state.
  // 选择器自行持有状态时使用的初始值。
  defaultValue?: string
  // Whether every option is disabled.
  // 是否禁用全部选项。
  disabled?: boolean
  // Called after a user selects a different value.
  // 用户选择不同值后调用。
  onValueChange?: (value: string) => void
  // Controlled selected value.
  // 受控的选中值。
  value?: string
}

// SegmentedControl renders keyboard-navigable mutually exclusive choices.
// SegmentedControl 渲染支持键盘导航的互斥选项。
// Parameters: props forwards controlled value, change callback, orientation, and native group attributes.
// 参数：props 转发受控值、变更回调、方向及原生分组属性。
// Returns a lightweight single-selection group with unified density and focus behavior.
// 返回使用统一密度与焦点行为的轻量单选组。
export function SegmentedControl({
  className,
  defaultValue = "",
  disabled = false,
  onKeyDown,
  onValueChange,
  value: controlledValue,
  ...props
}: SegmentedControlProps) {
  // uncontrolledValue stores selection only when the caller does not provide a controlled value.
  // uncontrolledValue 仅在调用方未提供受控值时保存选择状态。
  const [uncontrolledValue, setUncontrolledValue] = React.useState(defaultValue)
  // value resolves the single authoritative selection for child items.
  // value 为子选项解析唯一权威选择值。
  const value = controlledValue ?? uncontrolledValue
  // selectValue updates local state when needed and notifies the caller exactly once.
  // selectValue 在需要时更新本地状态，并仅通知调用方一次。
  const selectValue = React.useCallback((nextValue: string) => {
    if (controlledValue === undefined) {
      setUncontrolledValue(nextValue)
    }
    onValueChange?.(nextValue)
  }, [controlledValue, onValueChange])

  return (
    <SegmentedControlContext.Provider value={{ disabled, value }}>
      <RadioGroupPrimitive.Root
      className={cn("inline-flex w-fit items-center rounded-lg bg-muted p-0.5", className)}
      data-slot="segmented-control"
      disabled={disabled}
      value={value}
      onValueChange={selectValue}
      onKeyDown={onKeyDown}
      {...props}
      />
    </SegmentedControlContext.Provider>
  )
}

// SegmentedControlItemProps describes one value in a mutually exclusive group.
// SegmentedControlItemProps 描述互斥选项组中的一个值。
export interface SegmentedControlItemProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  // Unique value selected by this item.
  // 当前选项对应的唯一值。
  value: string
}

// SegmentedControlItem renders one option inside a segmented control.
// SegmentedControlItem 渲染分段选择器中的单个选项。
// Parameters: props forwards the required value and native Radix toggle-item attributes.
// 参数：props 转发必需的值及 Radix 切换项原生属性。
// Returns one compact item with selected, hover, disabled, and focus-visible states.
// 返回带有选中、悬停、禁用及键盘焦点状态的紧凑选项。
export function SegmentedControlItem({
  className,
  disabled,
  onClick,
  type = "button",
  value,
  ...props
}: SegmentedControlItemProps) {
  // context supplies the group-owned selected value and change callback.
  // context 提供由分组持有的选中值与变更回调。
  const context = React.useContext(SegmentedControlContext)
  // selected determines radio semantics and the only tab stop in the group.
  // selected 决定单选语义及分组中的唯一 Tab 停靠点。
  const selected = context.value === value

  return (
    <RadioGroupPrimitive.Item
      className={cn(
        "inline-flex h-7 min-w-0 items-center justify-center gap-1 rounded-md px-2.5 text-xs font-medium whitespace-nowrap text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/35 data-[state=on]:bg-background data-[state=on]:text-foreground data-[state=on]:shadow-sm disabled:pointer-events-none disabled:opacity-50",
        className,
      )}
      data-state={selected ? "on" : "off"}
      data-slot="segmented-control-item"
      disabled={context.disabled || disabled}
      onClick={onClick}
      value={value}
      type={type}
      {...props}
    />
  )
}

// FilterChipGroupProps describes a wrapping group of related filter chips.
// FilterChipGroupProps 描述一组可换行的相关筛选标签。
export interface FilterChipGroupProps extends React.ComponentProps<"div"> {
  // Accessible name describing the filter dimension.
  // 描述筛选维度的无障碍名称。
  "aria-label": string
}

// FilterChipGroup renders a labelled group without owning filter state.
// FilterChipGroup 渲染带标签的分组，但不持有筛选状态。
// Parameters: props supplies the accessible label, child chips, and native div attributes.
// 参数：props 提供无障碍标签、子筛选项及原生 div 属性。
// Returns a wrapping toolbar-like group that preserves caller-owned state.
// 返回可换行的工具栏式分组，并保留调用方状态所有权。
export function FilterChipGroup({ className, ...props }: FilterChipGroupProps) {
  return (
    <div
      className={cn("flex min-w-0 flex-wrap items-center gap-1", className)}
      data-slot="filter-chip-group"
      role="group"
      {...props}
    />
  )
}

// FilterChipProps describes one independently selectable filter value.
// FilterChipProps 描述一个可独立选择的筛选值。
export interface FilterChipProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  // Whether this filter value is currently active.
  // 当前筛选值是否处于启用状态。
  selected?: boolean
}

// FilterChip renders one compact filter value with an explicit pressed state.
// FilterChip 渲染一个具有明确按下状态的紧凑筛选值。
// Parameters: props supplies selection, click behavior, content, and native button attributes.
// 参数：props 提供选中状态、点击行为、内容及原生按钮属性。
// Returns a semantic toggle button without changing caller-owned filter logic.
// 返回语义切换按钮，不改变调用方持有的筛选逻辑。
export function FilterChip({ className, selected = false, type = "button", ...props }: FilterChipProps) {
  return (
    <button
      aria-pressed={selected}
      className={cn(
        "inline-flex h-7 items-center justify-center gap-1 rounded-full border border-border bg-background px-2.5 text-xs font-medium whitespace-nowrap text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/35 disabled:pointer-events-none disabled:opacity-50",
        selected && "border-foreground/15 bg-accent text-accent-foreground",
        className,
      )}
      data-selected={selected ? "true" : "false"}
      data-slot="filter-chip"
      type={type}
      {...props}
    />
  )
}
