import * as React from "react"
import { ChevronDown } from "lucide-react"
import { Collapsible as CollapsiblePrimitive } from "radix-ui"

import { cn } from "@/lib/utils"

// DISCLOSURE_MOTION_CLASS_NAME animates folded content to and from its measured height; padding lives on an inner wrapper so the height reaches zero cleanly.
// DISCLOSURE_MOTION_CLASS_NAME 让收起内容在实测高度与零之间过渡；内边距放在内层容器，使高度能干净地收到零。
const DISCLOSURE_MOTION_CLASS_NAME =
  "min-w-0 overflow-hidden data-[state=open]:animate-[vcn-collapsible-down_var(--vcn-duration-slow)_var(--vcn-ease-out)] data-[state=closed]:animate-[vcn-collapsible-up_var(--vcn-duration-base)_var(--vcn-ease-in-out)]"

// DisclosureSectionProps describes an optional block of settings that stays folded until the user needs it.
// DisclosureSectionProps 描述一组在用户需要前保持收起的可选设置。
export interface DisclosureSectionProps extends Omit<React.ComponentProps<typeof CollapsiblePrimitive.Root>, "title"> {
  // title names what the folded block contains.
  // title 说明收起区块包含的内容。
  title: React.ReactNode
  // description explains in one line why someone would open it.
  // description 用一句话说明为何需要展开。
  description?: React.ReactNode
  // meta shows a short current-state summary on the right, such as a count or "已配置".
  // meta 在右侧显示简短的当前状态摘要，例如数量或「已配置」。
  meta?: React.ReactNode
  // keepMounted keeps folded content in the DOM so unsaved drafts survive collapsing.
  // keepMounted 让收起的内容保留在 DOM 中，未保存的草稿在收起后不会丢失。
  keepMounted?: boolean
}

// DisclosureSection renders a bordered card whose header row reveals advanced or rarely used content.
// DisclosureSection 渲染带边框的卡片，其标题行用于展开高级或不常用的内容。
// Parameters: props provides the header copy, optional state summary, Radix root props and the folded content.
// 参数：props 提供标题文案、可选状态摘要、Radix 根节点属性与收起的内容。
// Returns a Collapsible root; the header is a real button so keyboard and screen readers work unchanged.
// 返回 Collapsible 根节点；标题行是真实按钮，键盘与读屏行为保持一致。
export function DisclosureSection({ title, description, meta, keepMounted, className, children, ...props }: DisclosureSectionProps) {
  return (
    <CollapsiblePrimitive.Root
      className={cn("group/disclosure min-w-0 rounded-lg border border-border bg-card shadow-(--vcn-shadow-raised)", className)}
      data-slot="disclosure-section"
      {...props}
    >
      <CollapsiblePrimitive.Trigger
        className="flex w-full min-w-0 items-center gap-3 rounded-lg px-3.5 py-3 text-left outline-none transition-colors duration-(--vcn-duration-fast) ease-(--vcn-ease-out) hover:bg-muted/50 focus-visible:ring-1 focus-visible:ring-ring/35 group-data-[state=open]/disclosure:rounded-b-none"
        data-slot="disclosure-trigger"
      >
        <span className="grid min-w-0 flex-1 gap-0.5">
          <span className="text-[13px] font-medium leading-5 text-foreground">{title}</span>
          {description ? <span className="text-xs leading-[18px] text-muted-foreground">{description}</span> : null}
        </span>
        {meta ? <span className="shrink-0 text-xs text-muted-foreground">{meta}</span> : null}
        <ChevronDown
          aria-hidden="true"
          className="size-4 shrink-0 text-muted-foreground transition-transform duration-(--vcn-duration-base) ease-(--vcn-ease-out) group-data-[state=open]/disclosure:rotate-180"
        />
      </CollapsiblePrimitive.Trigger>
      <CollapsiblePrimitive.Content
        className={cn(DISCLOSURE_MOTION_CLASS_NAME, keepMounted && "data-[state=closed]:hidden")}
        data-slot="disclosure-content"
        forceMount={keepMounted ? true : undefined}
      >
        <div className="min-w-0 border-t border-border px-3.5 py-3.5" data-slot="disclosure-body">{children}</div>
      </CollapsiblePrimitive.Content>
    </CollapsiblePrimitive.Root>
  )
}

// InlineDisclosureProps describes secondary content revealed by a quiet text button.
// InlineDisclosureProps 描述由低调文字按钮展开的次要内容。
export interface InlineDisclosureProps extends React.ComponentProps<typeof CollapsiblePrimitive.Root> {
  // label is the button text while folded.
  // label 是收起时的按钮文字。
  label: React.ReactNode
  // openLabel replaces the label while open; defaults to label.
  // openLabel 在展开时替换按钮文字，默认沿用 label。
  openLabel?: React.ReactNode
  // keepMounted keeps folded content in the DOM (hidden) so it stays searchable and copyable.
  // keepMounted 让收起的内容隐藏保留在 DOM 中，便于查找与复制。
  keepMounted?: boolean
}

// InlineDisclosure renders a text button with a turning chevron for technical details, raw data or long text.
// InlineDisclosure 渲染带旋转箭头的文字按钮，用于技术细节、原始数据或长文本。
// Parameters: props provides the button labels, Radix root props and the revealed content.
// 参数：props 提供按钮文字、Radix 根节点属性与展开的内容。
// Returns a Collapsible root without its own surface, so it sits inside cards, alerts and table cells.
// 返回不带自身表面的 Collapsible 根节点，可放入卡片、提示与表格单元格。
export function InlineDisclosure({ label, openLabel, keepMounted, className, children, open, defaultOpen, onOpenChange, ...props }: InlineDisclosureProps) {
  // expanded mirrors the Radix state so the label can change while staying controlled or uncontrolled.
  // expanded 镜像 Radix 状态，使按钮文字在受控或非受控模式下都能切换。
  const [expanded, setExpanded] = React.useState(defaultOpen ?? false)
  const isOpen = open ?? expanded
  return (
    <CollapsiblePrimitive.Root
      className={cn("group/inline-disclosure min-w-0", className)}
      data-slot="inline-disclosure"
      open={isOpen}
      onOpenChange={(next) => { setExpanded(next); onOpenChange?.(next) }}
      {...props}
    >
      <CollapsiblePrimitive.Trigger
        className="inline-flex items-center gap-1 rounded-md text-xs font-medium text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-1 focus-visible:ring-ring/35"
        data-slot="inline-disclosure-trigger"
      >
        {isOpen && openLabel ? openLabel : label}
        <ChevronDown aria-hidden="true" className="size-3.5 transition-transform duration-(--vcn-duration-base) ease-(--vcn-ease-out) group-data-[state=open]/inline-disclosure:rotate-180" />
      </CollapsiblePrimitive.Trigger>
      <CollapsiblePrimitive.Content className={cn(DISCLOSURE_MOTION_CLASS_NAME, keepMounted && "data-[state=closed]:hidden")} data-slot="inline-disclosure-content" forceMount={keepMounted ? true : undefined}>
        <div className="min-w-0 pt-2" data-slot="inline-disclosure-body">{children}</div>
      </CollapsiblePrimitive.Content>
    </CollapsiblePrimitive.Root>
  )
}
