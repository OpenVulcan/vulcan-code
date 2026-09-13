"use client"

import * as React from "react"
import { Check, ChevronsUpDown } from "lucide-react"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"

// ComboboxOption describes one searchable, selectable option.
// ComboboxOption 描述一个可搜索、可选择的选项。
interface ComboboxOption {
  value: string
  label: string
  description?: string
  keywords?: string[]
  disabled?: boolean
}

// ComboboxProps defines the controlled value, options, labels, and change callback.
// ComboboxProps 定义受控值、选项、文案与变更回调。
interface ComboboxProps {
  value?: string
  options: ComboboxOption[]
  placeholder: string
  searchPlaceholder: string
  emptyText: string
  ariaLabel: string
  disabled?: boolean
  className?: string
  contentClassName?: string
  onValueChange: (value: string) => void
}

// Combobox renders a shadcn-style searchable selector using Radix Popover and cmdk.
// Combobox 使用 Radix Popover 与 cmdk 渲染符合 shadcn 风格的可搜索选择器。
function Combobox({
  value,
  options,
  placeholder,
  searchPlaceholder,
  emptyText,
  ariaLabel,
  disabled = false,
  className,
  contentClassName,
  onValueChange,
}: ComboboxProps) {
  // open owns only the popup, while the selected identity remains controlled by the caller.
  // open 仅持有弹层状态，选中标识仍由调用方控制。
  const [open, setOpen] = React.useState(false)
  // selectedOption resolves the exact current identity without guessing a replacement.
  // selectedOption 解析准确的当前标识，不猜测替代选项。
  const selectedOption = options.find((option) => option.value === value)

  React.useEffect(() => {
    if (disabled) setOpen(false)
  }, [disabled])

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
          className={cn(
            // Let grid and flex parents constrain long labels instead of overlapping adjacent fields.
            // 允许网格和弹性容器约束长名称，避免选择框覆盖相邻字段。
            "w-full min-w-0 max-w-full justify-between gap-2 font-normal",
            !selectedOption && "text-muted-foreground",
            className,
          )}
        >
          <span className="min-w-0 flex-1 truncate text-left">
            {selectedOption?.label ?? placeholder}
          </span>
          <ChevronsUpDown className="size-4 shrink-0 opacity-50" aria-hidden="true" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className={cn("w-[max(18rem,var(--radix-popover-trigger-width))] max-w-[calc(100vw-2rem)] p-0", contentClassName)}
      >
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
                    if (disabled || option.disabled) return
                    onValueChange(option.value)
                    setOpen(false)
                  }}
                >
                  <Check
                    className={cn(
                      "size-4 shrink-0",
                      option.value === value ? "opacity-100" : "opacity-0",
                    )}
                    aria-hidden="true"
                  />
                  <span className="min-w-0 flex-1">
                    <span className="block whitespace-normal break-words">{option.label}</span>
                    {option.description ? (
                      <span className="block whitespace-normal break-words text-xs text-muted-foreground">
                        {option.description}
                      </span>
                    ) : null}
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}

export { Combobox }
export type { ComboboxOption, ComboboxProps }
