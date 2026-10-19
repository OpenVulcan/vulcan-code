import * as React from "react"

import { cx } from "@/ui-kit/behavior/core"
import { InlineDisclosure } from "@/ui-kit/primitives/disclosure"

// PageSectionProps describes one titled card section, the shared block of every Vulcan page.
// PageSectionProps 描述一个带标题的卡片区块，即所有 Vulcan 页面的共享区块。
export interface PageSectionProps extends Omit<React.ComponentProps<"div">, "title"> {
  // title names the concern of this section.
  // title 命名此区块关注的事项。
  title: React.ReactNode
  // description states the purpose in one sentence.
  // description 用一句话说明用途。
  description?: React.ReactNode
  // status renders beside the title, typically a Badge or StatusPill.
  // status 显示在标题旁，通常是 Badge 或 StatusPill。
  status?: React.ReactNode
  // actions are section-level commands aligned to the header end.
  // actions 是对齐到标题栏末端的区块级命令。
  actions?: React.ReactNode
  // guide explains when and how to use the section; collapsed folds it behind guideLabel.
  // guide 说明何时及如何使用此区块；collapsed 时收进 guideLabel 链接。
  guide?: React.ReactNode
  // guideLabel is the disclosure label used when the guide is collapsed.
  // guideLabel 是说明收起时使用的展开标签。
  guideLabel?: React.ReactNode
  // guideCollapsed folds the guide, for drawers and other narrow surfaces.
  // guideCollapsed 收起说明，用于抽屉等窄界面。
  guideCollapsed?: boolean
  // footer holds the commit action or outbound links of the section.
  // footer 放置区块的提交操作或外部链接。
  footer?: React.ReactNode
  // layout "rows" makes PageRow/PageItem children flush with dividers; "padded" keeps free content padded.
  // layout 为 "rows" 时 PageRow/PageItem 子项贴边并以分隔线排列；"padded" 为自由内容保留内边距。
  layout?: "padded" | "rows"
  // headingLevel keeps the document outline correct when sections nest under a page heading.
  // headingLevel 在区块嵌套于页面标题下时保持文档大纲正确。
  headingLevel?: 2 | 3 | 4
}

// PageSection renders one square panel with a labelled header, optional guide, body and footer.
// PageSection 渲染一个方形面板，含带标签标题栏、可选说明、正文与页脚。
export function PageSection({
  title, description, status, actions, guide, guideLabel, guideCollapsed, footer, layout = "padded", headingLevel = 3,
  className, children, ...props
}: PageSectionProps) {
  // headingId labels the region for assistive technology.
  // headingId 为辅助技术标注区域。
  const headingId = React.useId()
  const Heading = `h${headingLevel}` as const
  return (
    <section aria-labelledby={headingId} className={cx("vk-page-section", className)} data-layout={layout} data-slot="page-section" role="region" {...props}>
      <header className="vk-page-section-head">
        <div className="vk-page-section-title">
          <Heading id={headingId}>{title}</Heading>
          {status}
        </div>
        {description ? <p>{description}</p> : null}
        {actions ? <div className="vk-page-actions">{actions}</div> : null}
      </header>
      {guide && !guideCollapsed ? <div className="vk-page-section-guide">{guide}</div> : null}
      {guide && guideCollapsed ? (
        <InlineDisclosure className="vk-page-section-guide" label={guideLabel}>{guide}</InlineDisclosure>
      ) : null}
      {children === undefined || children === null || children === false ? null : (
        <div className="vk-page-section-body">{children}</div>
      )}
      {footer ? <footer className="vk-page-section-foot">{footer}</footer> : null}
    </section>
  )
}

// PageRowProps describes one labelled setting or fact with its controls.
// PageRowProps 描述一个带标签的设置或事实及其控件。
export interface PageRowProps extends Omit<React.ComponentProps<"div">, "title"> {
  // label names the setting; htmlFor links it to a single native control.
  // label 命名设置；htmlFor 将其关联到单个原生控件。
  label: React.ReactNode
  htmlFor?: string
  // description explains the effect of the setting.
  // description 说明设置的效果。
  description?: React.ReactNode
  // stacked places controls below the text, for wide editors such as checklists.
  // stacked 将控件放在文字下方，用于清单等宽编辑器。
  stacked?: boolean
}

// PageRow puts the name and explanation on the start side and its controls on the end side, wrapping when narrow.
// PageRow 将名称与说明放在起始侧、控件放在结束侧，窄布局时自动换行。
export function PageRow({ label, htmlFor, description, stacked, className, children, ...props }: PageRowProps) {
  return (
    <div className={cx("vk-page-row", className)} data-slot="page-row" data-stacked={stacked ? "" : undefined} {...props}>
      <div className="vk-page-row-text">
        {htmlFor ? <label className="vk-page-row-label" htmlFor={htmlFor}>{label}</label> : <span className="vk-page-row-label">{label}</span>}
        {description ? <p>{description}</p> : null}
      </div>
      {children === undefined || children === null ? null : <div className="vk-page-actions">{children}</div>}
    </div>
  )
}

// PageGuideItem is one labelled answer of a guide, such as "when", "how" or "effect".
// PageGuideItem 是说明中的一项带标签回答，例如「何时」「如何」「效果」。
export interface PageGuideItem {
  term: React.ReactNode
  detail: React.ReactNode
}

// PageGuide renders guide answers as a compact definition list.
// PageGuide 以紧凑的定义列表渲染说明回答。
export function PageGuide(props: { items: readonly PageGuideItem[]; className?: string }) {
  return (
    <dl className={cx("vk-page-guide", props.className)}>
      {props.items.map((item, index) => <div key={index}><dt>{item.term}</dt><dd>{item.detail}</dd></div>)}
    </dl>
  )
}

// PageHint renders one muted explanatory sentence, such as a footnote or a bounds hint.
// PageHint 渲染一句弱化的说明，例如脚注或边界提示。
export function PageHint({ className, ...props }: React.ComponentProps<"p">) {
  return <p className={cx("vk-page-hint", className)} {...props} />
}
