import * as React from "react";
import { ChevronDown } from "lucide-react";
import { cx } from "../behavior/core";
import { Collapsible, CollapsibleContent, CollapsibleTrigger, type CollapsibleProps } from "./collapsible";

// DisclosureSectionProps describe a bordered section whose body can be expanded.
// DisclosureSectionProps 描述正文可展开的带边框分区。
export interface DisclosureSectionProps extends Omit<CollapsibleProps, "title"> {
  title: React.ReactNode;
  description?: React.ReactNode;
  meta?: React.ReactNode;
  // keepMounted preserves the body's state (drafts, scroll) while it is collapsed.
  // keepMounted 在收起时保留正文状态（草稿、滚动位置）。
  keepMounted?: boolean;
}

// DisclosureSection renders a titled, collapsible block for optional or advanced content.
// DisclosureSection 渲染带标题的可折叠区块，用于可选或高级内容。
export function DisclosureSection({ title, description, meta, keepMounted, className, children, ...props }: DisclosureSectionProps) {
  return (
    <Collapsible className={cx("vk-disclosure-section", className)} data-slot="disclosure-section" {...props}>
      <CollapsibleTrigger className="vk-disclosure-trigger" data-slot="disclosure-trigger">
        <span className="vk-disclosure-copy">
          <span className="vk-disclosure-title">{title}</span>
          {description ? <span className="vk-disclosure-description">{description}</span> : null}
        </span>
        {meta ? <span className="vk-disclosure-meta">{meta}</span> : null}
        <ChevronDown aria-hidden="true" className="vk-disclosure-chevron" />
      </CollapsibleTrigger>
      <CollapsibleContent className="vk-disclosure-content" data-slot="disclosure-content" forceMount={keepMounted}>
        <div className="vk-disclosure-body" data-slot="disclosure-body">{children}</div>
      </CollapsibleContent>
    </Collapsible>
  );
}

// InlineDisclosureProps describe a small text toggle that reveals content below it.
// InlineDisclosureProps 描述在下方展开内容的小型文字开关。
export interface InlineDisclosureProps extends CollapsibleProps {
  label: React.ReactNode;
  // openLabel replaces the label while open (for example "Hide details").
  // openLabel 在展开时替换标签（例如“隐藏详情”）。
  openLabel?: React.ReactNode;
  keepMounted?: boolean;
}

// InlineDisclosure renders a quiet "show more" toggle with its content.
// InlineDisclosure 渲染安静的“显示更多”开关及其内容。
export function InlineDisclosure({ label, openLabel, keepMounted, className, children, open, defaultOpen, onOpenChange, ...props }: InlineDisclosureProps) {
  const [expanded, setExpanded] = React.useState(defaultOpen ?? false);
  const isOpen = open ?? expanded;
  return (
    <Collapsible
      className={cx("vk-inline-disclosure", className)}
      data-slot="inline-disclosure"
      open={isOpen}
      onOpenChange={(next) => {
        setExpanded(next);
        onOpenChange?.(next);
      }}
      {...props}
    >
      <CollapsibleTrigger className="vk-inline-disclosure-trigger" data-slot="inline-disclosure-trigger">
        {isOpen && openLabel ? openLabel : label}
        <ChevronDown aria-hidden="true" className="vk-disclosure-chevron" />
      </CollapsibleTrigger>
      <CollapsibleContent className="vk-disclosure-content" data-slot="inline-disclosure-content" forceMount={keepMounted}>
        <div className="vk-inline-disclosure-body" data-slot="inline-disclosure-body">{children}</div>
      </CollapsibleContent>
    </Collapsible>
  );
}
