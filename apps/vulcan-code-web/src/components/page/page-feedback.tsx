import * as React from "react"
import { CircleAlert, CircleCheck, Info, TriangleAlert } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { InlineDisclosure } from "@/components/ui/disclosure"
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { cn } from "@/lib/utils"

// PageNoticeTone is the closed semantic tone set shared by host pages and plugin pages.
// PageNoticeTone 是宿主页面与插件页面共享的封闭语义色调集合。
export type PageNoticeTone = "info" | "success" | "warning" | "error"

// noticeVariant maps each semantic tone to its Alert variant.
// noticeVariant 将每个语义色调映射到对应 Alert 变体。
const noticeVariant = { info: "info", success: "success", warning: "warning", error: "destructive" } as const

// noticeIcon gives every tone one fixed glyph so colour is never the only signal.
// noticeIcon 为每个色调提供固定图标，使颜色不是唯一信号。
const noticeIcon = { info: Info, success: CircleCheck, warning: TriangleAlert, error: CircleAlert } as const

// PageNoticeProps describes one inline status message inside a page or section.
// PageNoticeProps 描述页面或区块内的一条行内状态消息。
export interface PageNoticeProps extends Omit<React.ComponentProps<"div">, "title"> {
  tone: PageNoticeTone
  // title is the short headline; children carry the explanation.
  // title 是简短标题；children 承载说明。
  title?: React.ReactNode
  // actions are recovery commands such as retry; they flow below the text so several buttons never cover it.
  // actions 是重试等恢复命令；它们排在文字下方，多个按钮也不会遮挡文字。
  actions?: React.ReactNode
  // details is raw diagnostic text folded behind detailsLabel; it is rendered as preformatted text.
  // details 是收进 detailsLabel 的原始诊断文本，以预格式化文本呈现。
  details?: string
  detailsLabel?: React.ReactNode
}

// PageNotice renders an Alert with a tone icon; errors announce assertively, other tones politely.
// PageNotice 渲染带色调图标的 Alert；错误以强提示播报，其余色调以礼貌方式播报。
export function PageNotice({ tone, title, actions, details, detailsLabel, className, children, ...props }: PageNoticeProps) {
  const Icon = noticeIcon[tone]
  return (
    <Alert className={cn("page-notice", className)} data-tone={tone} role={tone === "error" ? "alert" : "status"} variant={noticeVariant[tone]} {...props}>
      <Icon aria-hidden="true" />
      {title ? <AlertTitle>{title}</AlertTitle> : null}
      {children || details ? (
        <AlertDescription className="page-notice-description">
          {children}
          {details ? (
            <InlineDisclosure className="page-notice-details" keepMounted label={detailsLabel}>
              <pre>{details}</pre>
            </InlineDisclosure>
          ) : null}
        </AlertDescription>
      ) : null}
      {actions ? <div className="page-notice-actions">{actions}</div> : null}
    </Alert>
  )
}

// PageEmptyStateProps describes the content shown when a page, section or list has nothing to show.
// PageEmptyStateProps 描述页面、区块或列表无内容时显示的内容。
export interface PageEmptyStateProps extends Omit<React.ComponentProps<"div">, "title"> {
  title: React.ReactNode
  description?: React.ReactNode
  // icon is a decorative glyph that names the empty subject.
  // icon 是命名空内容主题的装饰图标。
  icon?: React.ReactNode
  // action is the single next step, such as creating the first item.
  // action 是唯一的下一步，例如创建第一个项目。
  action?: React.ReactNode
  // size "inline" fits inside a section body; "panel" fills a page or pane.
  // size 为 "inline" 时嵌入区块正文；"panel" 填满页面或窗格。
  size?: "inline" | "panel"
}

// PageEmptyState renders the shared Empty surface with a polite status role.
// PageEmptyState 以礼貌状态角色渲染共享 Empty 表面。
export function PageEmptyState({ title, description, icon, action, size = "inline", className, ...props }: PageEmptyStateProps) {
  return (
    <Empty className={cn("page-empty-state", className)} data-size={size} role="status" variant="outlined" {...props}>
      <EmptyHeader>
        {icon ? <EmptyMedia variant="icon">{icon}</EmptyMedia> : null}
        <EmptyTitle>{title}</EmptyTitle>
        {description ? <EmptyDescription>{description}</EmptyDescription> : null}
      </EmptyHeader>
      {action ? <EmptyContent>{action}</EmptyContent> : null}
    </Empty>
  )
}
