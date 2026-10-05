import * as React from "react"
import { Activity, Bot, Calendar, CheckCircle2, Clock3, Database, MessageSquareText, Network, User } from "lucide-react"

import { InlineDisclosure } from "@/components/ui/disclosure"
import { cn } from "@/lib/utils"

// PageFact is one labelled value of a facts list.
// PageFact 是事实列表中的一个带标签值。
export interface PageFact {
  // key keeps identity stable when labels repeat across locales.
  // key 在不同语言标签重复时保持身份稳定。
  key?: string
  label: React.ReactNode
  value: React.ReactNode
  // tone highlights a tile value that needs attention.
  // tone 高亮需要关注的磁贴数值。
  tone?: "neutral" | "attention"
}

// PageFactsProps describes a definition list of read-only facts.
// PageFactsProps 描述只读事实的定义列表。
export interface PageFactsProps extends Omit<React.ComponentProps<"dl">, "children"> {
  items: readonly PageFact[]
  // layout "grid" flows facts into responsive columns, "list" keeps one pair per line, "tiles" boxes summary counts.
  // layout 为 "grid" 时事实流入响应式列，"list" 每行一对，"tiles" 将汇总计数装入磁贴。
  layout?: "grid" | "list" | "tiles"
}

// PageFacts renders read-only facts with muted labels and wrapping values.
// PageFacts 以弱化标签和可换行数值渲染只读事实。
export function PageFacts({ items, layout = "grid", className, ...props }: PageFactsProps) {
  return (
    <dl className={cn("page-facts", className)} data-layout={layout} {...props}>
      {items.map((item, index) => (
        <div data-tone={item.tone} key={item.key ?? index}>
          <dt>{item.label}</dt>
          <dd>{item.value}</dd>
        </div>
      ))}
    </dl>
  )
}

// PageDetailsProps describes raw identifiers folded behind a quiet disclosure.
// PageDetailsProps 描述收进低调展开项的原始标识。
export interface PageDetailsProps {
  // label is the disclosure trigger text, such as "Technical details".
  // label 是展开触发文字，例如「技术详情」。
  label: React.ReactNode
  items?: readonly PageFact[]
  // children append free-form evidence such as a JSON block.
  // children 追加自由格式证据，例如 JSON 区块。
  children?: React.ReactNode
  className?: string
}

// PageDetails keeps its content mounted while folded so identifiers stay searchable and copyable.
// PageDetails 收起时保持内容挂载，使标识仍可查找与复制。
export function PageDetails(props: PageDetailsProps) {
  return (
    <InlineDisclosure className={cn("page-details", props.className)} keepMounted label={props.label}>
      {props.items && props.items.length > 0 ? <PageFacts items={props.items} layout="list" /> : null}
      {props.children}
    </InlineDisclosure>
  )
}

// METRIC_ICONS is the bounded icon vocabulary shared by host and plugin metric cards.
// METRIC_ICONS 是宿主与插件指标卡片共享的有界图标词表。
const METRIC_ICONS = {
  activity: Activity,
  agent: User,
  calendar: Calendar,
  duration: Clock3,
  internal: Bot,
  model: Bot,
  requests: MessageSquareText,
  success: CheckCircle2,
  subagent: Network,
  tokens: Database,
} as const

// MetricCardIconName is one stable icon identifier supported by metric cards.
// MetricCardIconName 是指标卡片支持的一个稳定图标标识。
export type MetricCardIconName = keyof typeof METRIC_ICONS

// MetricCardProps describes one theme-inheriting KPI card.
// MetricCardProps 描述一个继承主题的 KPI 卡片。
export interface MetricCardProps {
  // label is the short metric name.
  // label 是简短指标名称。
  label: string
  // value is the formatted primary value.
  // value 是已格式化的主要数值。
  value: React.ReactNode
  // icon is an optional glyph from the bounded vocabulary.
  // icon 是有界词表中的可选图标。
  icon?: MetricCardIconName
  // detail is optional supporting text such as a comparison.
  // detail 是可选辅助文字，例如对比说明。
  detail?: React.ReactNode
}

// textTitle returns a tooltip only for plain text, preserving complete values when cards truncate them.
// textTitle 仅为纯文本返回提示，在卡片截断时保留完整数值。
function textTitle(value: React.ReactNode): string | undefined {
  return typeof value === "string" || typeof value === "number" ? String(value) : undefined
}

// MetricCard renders one compact KPI with host typography and colours.
// MetricCard 使用宿主字体与颜色渲染一个紧凑 KPI。
export function MetricCard(props: MetricCardProps) {
  const Icon = props.icon ? METRIC_ICONS[props.icon] : null
  return (
    <section aria-label={props.label} className="page-metric">
      <div className="page-metric-header">
        {Icon ? <Icon aria-hidden="true" size={14} /> : null}
        <div className="page-metric-label" title={props.label}>{props.label}</div>
      </div>
      <div className="page-metric-value" title={textTitle(props.value)}>{props.value}</div>
      <div className="page-metric-detail" title={textTitle(props.detail)}>{props.detail ?? " "}</div>
    </section>
  )
}

// MetricGrid flows metric cards into responsive equal columns.
// MetricGrid 将指标卡片排入响应式等宽列。
export function MetricGrid({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cn("page-metric-grid", className)} data-slot="metric-grid" {...props} />
}

// PageItemListProps describes one list of rows, optionally headed when a section groups several lists.
// PageItemListProps 描述一组行；区块包含多组列表时可带分组标题。
export interface PageItemListProps extends React.ComponentProps<"div"> {
  // heading names the group, such as the owning plugin of several capacity rows.
  // heading 命名分组，例如多条容量行所属的插件。
  heading?: React.ReactNode
}

// PageItemList stacks PageItem rows with dividers; it collapses when it has no rows.
// PageItemList 以分隔线堆叠 PageItem 行；没有行时自动收起。
export function PageItemList({ heading, className, children, ...props }: PageItemListProps) {
  return (
    <div className={cn("page-item-list", className)} data-slot="page-item-list" {...props}>
      {heading ? <p className="page-item-list-heading">{heading}</p> : null}
      {children}
    </div>
  )
}

// PageItemProps describes one row of a list inside a section, such as a finding, a rule or a resource.
// PageItemProps 描述区块内列表的一行，例如发现项、规则或资源。
export interface PageItemProps extends Omit<React.ComponentProps<"article">, "title"> {
  title: React.ReactNode
  description?: React.ReactNode
  // icon is a decorative glyph coloured by tone.
  // icon 是按色调着色的装饰图标。
  icon?: React.ReactNode
  // meta is secondary inline information such as a badge or a timestamp.
  // meta 是次要行内信息，例如徽标或时间戳。
  meta?: React.ReactNode
  // tone colours the icon: attention for unresolved work, success for healthy state, neutral otherwise.
  // tone 为图标着色：attention 表示未解决工作，success 表示健康状态，其余为 neutral。
  tone?: "neutral" | "attention" | "success" | "danger"
  // actions are row-level commands on the end side.
  // actions 是结束侧的行级命令。
  actions?: React.ReactNode
}

// PageItem renders icon, text and actions; children append details below the description.
// PageItem 渲染图标、文字与操作；children 在说明下方追加详情。
export function PageItem({ title, description, icon, meta, tone = "neutral", actions, className, children, ...props }: PageItemProps) {
  return (
    <article className={cn("page-item", className)} data-slot="page-item" data-tone={tone} {...props}>
      {icon ? <span aria-hidden="true" className="page-item-icon">{icon}</span> : null}
      <div className="page-item-main">
        <div className="page-item-heading">
          <p className="page-item-title">{title}</p>
          {meta ? <div className="page-item-meta">{meta}</div> : null}
        </div>
        {description ? <p className="page-item-description">{description}</p> : null}
        {children}
      </div>
      {actions ? <div className="page-item-actions">{actions}</div> : null}
    </article>
  )
}
