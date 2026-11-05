import type { CSSProperties, ReactNode } from "react"

import { Badge } from "@/ui-kit/primitives/badge"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/ui-kit/primitives/card"
import { Spinner } from "@/ui-kit/primitives/spinner"
import { cx } from "@/ui-kit/behavior/core"

// UsagePanelState is the closed set of presentation states supported by a usage surface.
// UsagePanelState 是用量表面支持的封闭展示状态集合。
export type UsagePanelState = "ready" | "empty" | "loading" | "partial" | "error"

// UsageMetricViewModel contains one already-formatted usage fact.
// UsageMetricViewModel 包含一项已经格式化的用量事实。
export interface UsageMetricViewModel {
  // description explains the metric's exact accounting boundary.
  // description 说明指标的精确计量边界。
  description?: string
  // id is the stable render identity supplied by the business adapter.
  // id 是业务适配层提供的稳定渲染标识。
  id: string
  // label is the localized metric name.
  // label 是已本地化的指标名称。
  label: string
  // value is the exact preformatted value and is never numerically coerced here.
  // value 是精确的预格式化值，本组件绝不再次进行数值转换。
  value: string
}

// UsageMetricProps configures one reusable metric presentation.
// UsageMetricProps 配置一项可复用指标展示。
export interface UsageMetricProps extends UsageMetricViewModel {
  // compact removes the card shell for dense embedded contexts.
  // compact 为紧凑嵌入场景移除卡片外壳。
  compact?: boolean
}

// UsageQuotaMetricViewModel contains one exact quota value and normalized visual percentage.
// UsageQuotaMetricViewModel 包含一个精确额度值与规范化视觉百分比。
export interface UsageQuotaMetricViewModel {
  // id is the stable render identity supplied by the business adapter.
  // id 是业务适配层提供的稳定渲染标识。
  id: string
  // label is the localized quota-window name.
  // label 是已本地化的额度周期名称。
  label: string
  // percentage is the already-normalized remaining percentage from zero to one hundred.
  // percentage 是已经规范化到零至一百的剩余百分比。
  percentage: number
  // value preserves the provider's exact formatted quota semantics.
  // value 保留供应商精确的格式化额度语义。
  value: string
}

// TokenUsageBreakdownViewModel contains exact preformatted token categories.
// TokenUsageBreakdownViewModel 包含精确的预格式化 Token 分类。
export interface TokenUsageBreakdownViewModel {
  // items are visible token categories in business-defined order.
  // items 是按业务定义顺序展示的 Token 分类。
  items: UsageMetricViewModel[]
  // title is the localized breakdown heading.
  // title 是已本地化的分解标题。
  title: string
}

// CostSummaryViewModel contains an exact decimal cost and its provenance.
// CostSummaryViewModel 包含精确十进制费用及其来源。
export interface CostSummaryViewModel {
  // amount is the exact preformatted decimal value.
  // amount 是精确的预格式化十进制值。
  amount: string
  // description explains the accounting or estimation boundary.
  // description 说明计费或估算边界。
  description?: string
  // label is the localized cost heading.
  // label 是已本地化的费用标题。
  label: string
  // source is an already-localized provenance label.
  // source 是已经本地化的来源标签。
  source?: string
}

// ModelUsageViewModel contains one already-aggregated model usage row.
// ModelUsageViewModel 包含一条已经聚合的模型用量行。
export interface ModelUsageViewModel {
  // cost is an optional exact preformatted cost.
  // cost 是可选的精确预格式化费用。
  cost?: string
  // id is the stable business identity.
  // id 是稳定业务标识。
  id: string
  // label is the visible model name.
  // label 是可见模型名称。
  label: string
  // provider is the visible provider name.
  // provider 是可见供应商名称。
  provider?: string
  // requests is the optional preformatted request count.
  // requests 是可选的预格式化请求次数。
  requests?: string
  // tokens is the optional preformatted token total.
  // tokens 是可选的预格式化 Token 合计。
  tokens?: string
}

// UsagePanelViewModel defines one service-independent product usage surface.
// UsagePanelViewModel 定义一个独立于服务层的产品用量表面。
export interface UsagePanelViewModel {
  // cost is the optional exact cost summary.
  // cost 是可选的精确费用摘要。
  cost?: CostSummaryViewModel
  // description explains the panel's accounting scope.
  // description 说明面板计量范围。
  description?: string
  // metrics are primary already-formatted usage facts.
  // metrics 是主要的已格式化用量事实。
  metrics?: UsageMetricViewModel[]
  // models are optional already-aggregated model rows.
  // models 是可选的已聚合模型行。
  models?: ModelUsageViewModel[]
  // modelsTitle is the localized model-list heading.
  // modelsTitle 是已本地化的模型列表标题。
  modelsTitle?: string
  // state selects the explicit display state.
  // state 选择明确的展示状态。
  state: UsagePanelState
  // stateLabel is the localized compact label for a partial state.
  // stateLabel 是部分可用状态的已本地化紧凑标签。
  stateLabel?: string
  // stateMessage is the localized empty, loading, partial, or error explanation.
  // stateMessage 是已本地化的空、加载、部分可用或错误说明。
  stateMessage?: string
  // title is the localized panel heading.
  // title 是已本地化的面板标题。
  title: string
  // tokenBreakdown is the optional exact token-category summary.
  // tokenBreakdown 是可选的精确 Token 分类摘要。
  tokenBreakdown?: TokenUsageBreakdownViewModel
}

// UsagePanelProps combines a determined view model with optional specialized quota content.
// UsagePanelProps 将确定的视图模型与可选的专用额度内容组合。
export interface UsagePanelProps {
  // children contains provider-specific content already adapted by the business layer.
  // children 包含已经由业务层适配的供应商专用内容。
  children?: ReactNode
  // className extends layout only without redefining component appearance.
  // className 仅扩展布局，不重新定义组件外观。
  className?: string
  // model is the complete service-independent usage view model.
  // model 是完整且独立于服务层的用量视图模型。
  model: UsagePanelViewModel
}

// ModelUsageListProps contains an already-localized model list.
// ModelUsageListProps 包含一个已经本地化的模型列表。
export interface ModelUsageListProps {
  // models are already-aggregated display rows.
  // models 是已经聚合的展示行。
  models: ModelUsageViewModel[]
  // title is the localized list heading.
  // title 是已本地化的列表标题。
  title: string
}

// USAGE_METER_CELLS is the number of discrete cells in one usage gauge.
// USAGE_METER_CELLS 是单个用量仪表的离散格数。
const USAGE_METER_CELLS = 20

// UsageMeterTone is the urgency band of a remaining-quota share.
// UsageMeterTone 是剩余额度比例的紧迫程度分档。
export type UsageMeterTone = "ok" | "warn" | "bad"

// UsageMeterProps configures one segmented remaining-quota gauge.
// UsageMeterProps 配置一个分段式剩余额度仪表。
export interface UsageMeterProps {
  // label is the accessible name, normally "window: value".
  // label 是无障碍名称，通常为“周期: 数值”。
  label: string
  // percentage is the remaining share from zero to one hundred.
  // percentage 是零至一百的剩余比例。
  percentage: number
  // tone selects the urgency color; the caller owns its thresholds.
  // tone 选择紧迫程度颜色；阈值由调用方持有。
  tone: UsageMeterTone
  // cells overrides the cell count for gauges too narrow for the standard resolution, such as status-bar items.
  // cells 为过窄而无法容纳标准分辨率的仪表（如状态栏项）覆盖格数。
  cells?: number
}

// UsageMeter renders a remaining share as discrete instrument cells with synchronized ARIA values.
// UsageMeter 以离散仪表格渲染剩余比例，并同步 ARIA 数值。
export function UsageMeter(props: UsageMeterProps) {
  // percentage prevents invalid geometry while the caller keeps the exact text value.
  // percentage 防止无效几何，精确文本值由调用方保留。
  const percentage = Math.max(0, Math.min(100, props.percentage))
  // cells is the rendered cell count; the stylesheet's column count follows it through --cells.
  // cells 是渲染的格数；样式表的列数通过 --cells 跟随该值。
  const cells = props.cells !== undefined && Number.isSafeInteger(props.cells) && props.cells > 0 ? props.cells : USAGE_METER_CELLS
  // lit is the count of filled cells; any non-zero share keeps at least one cell visible.
  // lit 是点亮格数；任何非零比例至少保留一格可见。
  const lit = percentage > 0 ? Math.max(1, Math.round((percentage / 100) * cells)) : 0
  return (
    <div
      aria-label={props.label}
      aria-valuemax={100}
      aria-valuemin={0}
      aria-valuenow={Math.round(percentage)}
      className="vk-meter"
      data-tone={props.tone}
      role="progressbar"
      style={cells === USAGE_METER_CELLS ? undefined : { "--cells": cells } as CSSProperties}
    >
      {Array.from({ length: cells }, (_, index) => <i data-on={index < lit ? "" : undefined} key={index} />)}
    </div>
  )
}

// UsageMetric renders one stable KPI treatment for both standalone and embedded contexts.
// UsageMetric 为独立与嵌入场景渲染统一的 KPI 样式。
export function UsageMetric(props: UsageMetricProps) {
  // content contains the shared metric hierarchy used by both density modes.
  // content 包含两种密度模式共享的指标层级。
  const content = (
    <>
      <span>{props.label}</span>
      <strong>{props.value}</strong>
      {props.description ? <small>{props.description}</small> : null}
    </>
  )
  return props.compact
    ? <div className="vk-usage-metric">{content}</div>
    : <Card><CardContent className="vk-usage-metric" data-size="lg">{content}</CardContent></Card>
}

// UsageQuotaMetric renders one exact quota value with a normalized semantic gauge.
// UsageQuotaMetric 使用规范化语义仪表渲染一个精确额度值。
export function UsageQuotaMetric(props: UsageQuotaMetricViewModel) {
  // tone communicates remaining-quota urgency without changing the exact value.
  // tone 在不改变精确数值的前提下表达剩余额度紧迫程度。
  const tone: UsageMeterTone = props.percentage <= 10 ? "bad" : props.percentage <= 30 ? "warn" : "ok"
  return (
    <div className="vk-usage-row" data-tone={tone}>
      <span className="vk-usage-name vk-truncate">{props.label}</span>
      <span className="vk-usage-value">{props.value}</span>
      <UsageMeter label={`${props.label}: ${props.value}`} percentage={props.percentage} tone={tone} />
    </div>
  )
}

// TokenUsageBreakdown renders exact token categories without deriving totals.
// TokenUsageBreakdown 渲染精确 Token 分类且不自行推导合计。
export function TokenUsageBreakdown(props: TokenUsageBreakdownViewModel) {
  return (
    <section className="vk-usage-group">
      <h4 className="vk-label">{props.title}</h4>
      <div className="vk-usage-metrics">
        {props.items.map((item) => <UsageMetric compact key={item.id} {...item} />)}
      </div>
    </section>
  )
}

// CostSummary renders an exact cost string and explicit provenance.
// CostSummary 渲染精确费用字符串与明确来源。
export function CostSummary(props: CostSummaryViewModel) {
  return (
    <section className="vk-usage-cost">
      <div>
        <h4 className="vk-label">{props.label}</h4>
        <strong>{props.amount}</strong>
      </div>
      {props.source ? <p>{props.source}</p> : null}
      {props.description ? <small>{props.description}</small> : null}
    </section>
  )
}

// ModelUsageList renders already-aggregated model usage without recomputing accounting facts.
// ModelUsageList 渲染已聚合模型用量且不重新计算计量事实。
export function ModelUsageList(props: ModelUsageListProps) {
  return (
    <section className="vk-usage-group">
      <h4 className="vk-label">{props.title}</h4>
      <ul className="vk-usage-models">
        {props.models.map((model) => (
          <li key={model.id}>
            <span>
              <strong className="vk-truncate">{model.label}</strong>
              {model.provider ? <small className="vk-truncate">{model.provider}</small> : null}
            </span>
            <span>
              {model.tokens ? <b>{model.tokens}</b> : null}
              {model.requests ? <small>{model.requests}</small> : null}
              {model.cost ? <small>{model.cost}</small> : null}
            </span>
          </li>
        ))}
      </ul>
    </section>
  )
}

// UsagePanel renders one complete usage product surface from an already-determined view model.
// UsagePanel 根据已经确定的视图模型渲染一个完整的产品用量表面。
export function UsagePanel(props: UsagePanelProps) {
  // showBody preserves partial facts while explicit empty, loading, and error states remain visible.
  // showBody 在明确展示空、加载与错误状态时仍保留部分可用事实。
  const showBody = props.model.state === "ready" || props.model.state === "partial"
  return (
    <Card className={cx("vk-usage-panel", props.className)} data-state={props.model.state} data-slot="usage-panel">
      <CardHeader>
        <div className="vk-usage-head">
          <span>
            <CardTitle>{props.model.title}</CardTitle>
            {props.model.description ? <CardDescription>{props.model.description}</CardDescription> : null}
          </span>
          {props.model.state === "partial" && props.model.stateLabel ? <Badge variant="outline">{props.model.stateLabel}</Badge> : null}
        </div>
      </CardHeader>
      <CardContent className="vk-usage">
        {props.model.state === "loading" ? (
          <p className="vk-usage-state"><Spinner />{props.model.stateMessage}</p>
        ) : null}
        {props.model.state === "empty" || props.model.state === "error" ? (
          <p className="vk-usage-state" data-tone={props.model.state === "error" ? "bad" : undefined}>{props.model.stateMessage}</p>
        ) : null}
        {showBody ? (
          <>
            {props.model.stateMessage ? <p className="vk-usage-note">{props.model.stateMessage}</p> : null}
            {/* Fit metrics to their actual panel width instead of the application's viewport. */}
            {/* 根据实际面板宽度排列指标，避免依据整个应用视口挤压嵌入面板。 */}
            {props.model.metrics?.length ? <div className="vk-usage-metrics">{props.model.metrics.map((metric) => <UsageMetric compact key={metric.id} {...metric} />)}</div> : null}
            {props.model.tokenBreakdown ? <TokenUsageBreakdown {...props.model.tokenBreakdown} /> : null}
            {props.model.cost ? <CostSummary {...props.model.cost} /> : null}
            {props.model.models?.length && props.model.modelsTitle ? <ModelUsageList models={props.model.models} title={props.model.modelsTitle} /> : null}
            {props.children}
          </>
        ) : null}
      </CardContent>
    </Card>
  )
}
