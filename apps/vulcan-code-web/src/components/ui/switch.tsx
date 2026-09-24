"use client"

import * as React from "react"
import { Switch as SwitchPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"

// Switch renders the shared shadcn-style boolean control.
// Switch 渲染共享的 shadcn 风格布尔控件。
function Switch({
  className,
  ...props
}: React.ComponentProps<typeof SwitchPrimitive.Root>) {
  return (
    <SwitchPrimitive.Root
      data-slot="switch"
      className={cn(
        "peer inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border border-transparent bg-input shadow-[inset_0_1px_2px_rgb(0_0_0/0.08)] transition-[background-color,box-shadow] duration-(--vcn-duration-base) ease-(--vcn-ease-out) outline-none focus-visible:border-ring focus-visible:ring-1 focus-visible:ring-ring/35 disabled:cursor-not-allowed disabled:opacity-60 data-[state=checked]:bg-primary disabled:data-[state=checked]:bg-(--vcn-disabled-foreground) disabled:shadow-none",
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb
        data-slot="switch-thumb"
        className="pointer-events-none block size-4 rounded-full bg-background shadow-[0_1px_2px_rgb(0_0_0/0.22),0_0_0_0.5px_rgb(0_0_0/0.06)] ring-0 transition-transform duration-(--vcn-duration-base) ease-(--vcn-ease-spring) data-[state=checked]:translate-x-4 data-[state=unchecked]:translate-x-0"
      />
    </SwitchPrimitive.Root>
  )
}

export { Switch }
