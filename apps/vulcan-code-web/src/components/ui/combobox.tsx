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
  const [open, setOpen] = React.useState(false)
  const selectedOption = options.find((option) => option.value === value)

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          role="combobox"
          aria-label={ariaLabel}
          aria-expanded={open}
          disabled={disabled}
          className={cn(
            "w-full justify-between gap-2 font-normal",
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
        className={cn("w-[var(--radix-popover-trigger-width)] p-0", contentClassName)}
      >
        <Command>
          <CommandInput placeholder={searchPlaceholder} />
          <CommandList>
            <CommandEmpty>{emptyText}</CommandEmpty>
            <CommandGroup>
              {options.map((option) => (
                <CommandItem
                  key={option.value}
                  value={[option.label, option.description, ...(option.keywords ?? [])]
                    .filter(Boolean)
                    .join(" ")}
                  disabled={option.disabled}
                  onSelect={() => {
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
                    <span className="block truncate">{option.label}</span>
                    {option.description ? (
                      <span className="block truncate text-xs text-muted-foreground">
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
