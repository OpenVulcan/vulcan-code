import * as React from "react";
import { Check, ChevronsUpDown } from "lucide-react";
import { cx } from "../behavior/core";
import { Button } from "./button";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "./command";
import { Popover, PopoverContent, PopoverTrigger } from "./popover";

// Combobox picks one value from a long list by searching it. Disabling it closes an open list, and it stays
// closed when it becomes enabled again, so a busy parent never leaves stale rows interactive.
// 组合框通过搜索从长列表中选取一个值。禁用会关闭已打开的列表，重新启用后也保持关闭，使忙碌的父组件不会留下仍可
// 交互的陈旧行。

// ComboboxOption is one choosable entry; description and keywords also match the search.
// ComboboxOption 是一个可选条目；说明与关键词同样参与搜索匹配。
interface ComboboxOption {
  value: string;
  label: string;
  description?: string;
  keywords?: string[];
  disabled?: boolean;
}

// ComboboxProps configure the trigger copy and the options.
// ComboboxProps 配置触发器文案与选项。
interface ComboboxProps {
  value?: string;
  options: ComboboxOption[];
  placeholder: string;
  searchPlaceholder: string;
  emptyText: string;
  ariaLabel: string;
  disabled?: boolean;
  className?: string;
  contentClassName?: string;
  onValueChange: (value: string) => void;
}

// Combobox renders the trigger button and the searchable list.
// Combobox 渲染触发按钮与可搜索列表。
function Combobox({ value, options, placeholder, searchPlaceholder, emptyText, ariaLabel, disabled = false, className, contentClassName, onValueChange }: ComboboxProps) {
  const [open, setOpen] = React.useState(false);
  const selectedOption = options.find((option) => option.value === value);
  React.useEffect(() => {
    if (disabled) setOpen(false);
  }, [disabled]);
  return (
    <Popover open={open && !disabled} onOpenChange={(nextOpen) => setOpen(nextOpen && !disabled)}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          role="combobox"
          aria-label={ariaLabel}
          aria-expanded={open && !disabled}
          title={selectedOption?.label}
          disabled={disabled}
          data-placeholder={selectedOption ? undefined : ""}
          className={cx("vk-combobox", className)}
        >
          <span className="vk-combobox-value">{selectedOption?.label ?? placeholder}</span>
          <ChevronsUpDown className="vk-select-chevron" aria-hidden="true" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" width="content" className={cx("vk-combobox-pop", contentClassName)}>
        <Command>
          <CommandInput aria-label={searchPlaceholder} placeholder={searchPlaceholder} />
          <CommandList>
            <CommandEmpty>{emptyText}</CommandEmpty>
            <CommandGroup>
              {options.map((option) => (
                <CommandItem
                  key={option.value}
                  value={option.value}
                  keywords={[option.label, ...(option.description ? [option.description] : []), ...(option.keywords ?? [])]}
                  disabled={disabled || option.disabled}
                  onSelect={() => {
                    if (disabled || option.disabled) return;
                    onValueChange(option.value);
                    setOpen(false);
                  }}
                >
                  <Check className="vk-combobox-check" data-visible={option.value === value || undefined} aria-hidden="true" />
                  <span className="vk-combobox-copy">
                    <span>{option.label}</span>
                    {option.description ? <span className="vk-combobox-description">{option.description}</span> : null}
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

export { Combobox };
export type { ComboboxOption, ComboboxProps };
