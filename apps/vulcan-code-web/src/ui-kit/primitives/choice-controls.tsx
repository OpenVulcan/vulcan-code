import * as React from "react";
import { cx } from "../behavior/core";
import { RadioGroupRoot, RadioItem, useRadioGroup, type RadioGroupProps, type RadioItemProps } from "./radio-group";

// SegmentedControlProps choose one value among a few mutually exclusive options.
// SegmentedControlProps 在少量互斥选项中选择一个值。
export type SegmentedControlProps = RadioGroupProps;

// SegmentedControl renders a compact single-choice switcher with radio semantics.
// SegmentedControl 渲染具备单选语义的紧凑切换器。
export function SegmentedControl({ className, ...props }: SegmentedControlProps) {
  return <RadioGroupRoot data-slot="segmented-control" className={cx("vk-seg", className)} {...props} />;
}

// SegmentedControlItemProps are button attributes plus the option value.
// SegmentedControlItemProps 是按钮属性加上选项值。
export type SegmentedControlItemProps = RadioItemProps;

// SegmentedControlItem renders one segment; it reports on/off like a toggle.
// SegmentedControlItem 渲染一个分段；以开关方式报告 on/off。
export function SegmentedControlItem({ className, value, ...props }: SegmentedControlItemProps) {
  const group = useRadioGroup();
  return (
    <RadioItem
      data-slot="segmented-control-item"
      data-state={group.value === value ? "on" : "off"}
      className={cx("vk-seg-item", className)}
      value={value}
      {...props}
    />
  );
}

// FilterChipGroupProps require a label that names what the chips filter.
// FilterChipGroupProps 要求提供说明筛选对象的标签。
export interface FilterChipGroupProps extends React.ComponentProps<"div"> {
  "aria-label": string;
}

// FilterChipGroup lays out filter chips that toggle independently.
// FilterChipGroup 排布彼此独立切换的筛选标签。
export function FilterChipGroup({ className, ...props }: FilterChipGroupProps) {
  return <div className={cx("vk-chip-group", className)} data-slot="filter-chip-group" role="group" {...props} />;
}

// FilterChipProps are button attributes plus the pressed state.
// FilterChipProps 是按钮属性加上按下状态。
export interface FilterChipProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  selected?: boolean;
}

// FilterChip renders one toggleable filter.
// FilterChip 渲染一个可切换的筛选项。
export function FilterChip({ className, selected = false, type = "button", ...props }: FilterChipProps) {
  return (
    <button
      aria-pressed={selected}
      className={cx("vk-chip", className)}
      data-selected={selected ? "true" : "false"}
      data-slot="filter-chip"
      type={type}
      {...props}
    />
  );
}
