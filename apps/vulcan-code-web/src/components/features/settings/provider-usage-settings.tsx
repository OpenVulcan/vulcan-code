import * as React from "react"
import { FileUp, LoaderCircle, RefreshCw } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { useI18n } from "@/i18n"
import {
  importPublicAiCcSwitchPricing,
  loadProviderUsageDashboard,
} from "@/service/settings-actions-service"
import type {
  ProviderUsageAttemptSummary,
  ProviderUsageBillingDetail,
  ProviderUsageDashboard,
  ProviderUsageDailySummary,
} from "@/types"

// ALL_CONSUMERS identifies the unfiltered server-bound consumer view.
// ALL_CONSUMERS 标识未筛选的服务端绑定消费者视图。
const ALL_CONSUMERS = "__all_consumers__"

// ProviderUsageSettingsArea renders native egress accounting without session-only statistics.
// ProviderUsageSettingsArea 渲染原生出口侧计量，不混入仅会话统计。
export function ProviderUsageSettingsArea() {
  const { locale } = useI18n()
  // dashboard is the latest bounded VLDB-backed accounting snapshot.
  // dashboard 是最新有界 VLDB 计量快照。
  const [dashboard, setDashboard] = React.useState<ProviderUsageDashboard | null>(null)
  // loading prevents overlapping dashboard reads.
  // loading 防止仪表盘读取重叠。
  const [loading, setLoading] = React.useState(false)
  // error contains one actionable native-query failure.
  // error 保存一条可操作的原生查询失败。
  const [error, setError] = React.useState<string | null>(null)
  // importing prevents overlapping pricing document mutations.
  // importing 防止价格文档变更重叠执行。
  const [importing, setImporting] = React.useState(false)
  // importNotice reports the exact active row count after one successful import.
  // importNotice 报告一次成功导入后的精确有效行数。
  const [importNotice, setImportNotice] = React.useState<string | null>(null)
  // consumerFilter isolates WebUI, VMM, and external-consumer billing facts.
  // consumerFilter 隔离 WebUI、VMM 与外部消费者的计费事实。
  const [consumerFilter, setConsumerFilter] = React.useState(ALL_CONSUMERS)
  // pricingFileInput owns the browser-local file selection without exposing a host path.
  // pricingFileInput 持有浏览器本地文件选择，不暴露宿主路径。
  const pricingFileInput = React.useRef<HTMLInputElement | null>(null)

  // refresh loads one coherent dashboard revision from the existing application binding.
  // refresh 从现有应用绑定加载一个一致的仪表盘修订版。
  const refresh = React.useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      setDashboard(await loadProviderUsageDashboard())
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    void refresh()
  }, [refresh])

  // importPricingFile validates one bounded UTF-8 document before the backend performs strict cc-switch parsing.
  // importPricingFile 在后端执行严格 cc-switch 解析前校验一份有界 UTF-8 文档。
  const importPricingFile = React.useCallback(async (file: File) => {
    const maxBytes = 4 * 1024 * 1024
    setError(null)
    setImportNotice(null)
    if (file.size > maxBytes) {
      setError(`cc-switch model-pricing.json exceeds ${maxBytes} bytes.`)
      return
    }
    setImporting(true)
    try {
      const importedCount = await importPublicAiCcSwitchPricing(file.name, await file.text())
      setImportNotice(
        locale === "zh-CN"
          ? `已导入 ${formatInteger(importedCount)} 条有效模型价格。`
          : `Imported ${formatInteger(importedCount)} active model prices.`,
      )
      await refresh()
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setImporting(false)
      if (pricingFileInput.current) {
        pricingFileInput.current.value = ""
      }
    }
  }, [locale, refresh])

  // consumerIds contains exact server-bound identities observed in either dashboard projection.
  // consumerIds 包含任一仪表盘投影中观察到的精确服务端绑定身份。
  const consumerIds = React.useMemo(() => Array.from(new Set([
    ...(dashboard?.daily.map((row) => row.consumerId) ?? []),
    ...(dashboard?.recentAttempts.map((row) => row.consumerId) ?? []),
  ])).sort(), [dashboard])
  React.useEffect(() => {
    if (consumerFilter !== ALL_CONSUMERS && !consumerIds.includes(consumerFilter)) {
      setConsumerFilter(ALL_CONSUMERS)
    }
  }, [consumerFilter, consumerIds])
  // filteredDaily applies the selected consumer without changing persisted aggregates.
  // filteredDaily 应用选定消费者，且不改变持久化聚合。
  const filteredDaily = dashboard?.daily.filter(
    (row) => consumerFilter === ALL_CONSUMERS || row.consumerId === consumerFilter,
  ) ?? []
  // filteredRecentAttempts applies the same source boundary to exact attempt evidence.
  // filteredRecentAttempts 对精确 attempt 证据应用同一来源边界。
  const filteredRecentAttempts = dashboard?.recentAttempts.filter(
    (row) => consumerFilter === ALL_CONSUMERS || row.consumerId === consumerFilter,
  ) ?? []
  // attemptCount is the physical upstream-attempt total in the bounded dashboard window.
  // attemptCount 是有界仪表盘窗口内真实上游 attempt 总数。
  const attemptCount = filteredDaily.reduce((sum, row) => sum + row.attemptCount, 0)
  // reportedCount measures direct provider usage coverage rather than fabricated estimates.
  // reportedCount 衡量供应商直接报告用量的覆盖量，不包含伪造估算。
  const reportedCount = filteredDaily.reduce(
    (sum, row) => sum + row.reportedUsageAttemptCount,
    0,
  ) ?? 0
  // totalTokens sums only known daily token totals; unknown values remain absent in row detail.
  // totalTokens 仅汇总已知每日 Token；未知值仍在行明细中保持缺失。
  const totalTokens = filteredDaily.reduce((sum, row) => sum + (row.totalTokens ?? 0), 0)
  const zh = locale === "zh-CN"

  return (
    <div className="space-y-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold">{zh ? "用量与计费" : "Usage and billing"}</h2>
          <p className="text-sm text-muted-foreground">
            {zh
              ? "数据来自协议出口的真实 attempt，并通过现有 VLDB Controller 持久化。"
              : "Facts come from physical protocol-egress attempts and the existing VLDB controller."}
          </p>
        </div>
        <Button disabled={loading} onClick={() => void refresh()} size="sm" type="button" variant="outline">
          {loading ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : <RefreshCw className="mr-2 size-4" />}
          {zh ? "刷新" : "Refresh"}
        </Button>
      </div>

      {error ? (
        <Alert variant="destructive">
          <AlertTitle>{zh ? "读取失败" : "Read failed"}</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}

      {importNotice ? (
        <Alert>
          <AlertTitle>{zh ? "价格目录已更新" : "Pricing catalog updated"}</AlertTitle>
          <AlertDescription>{importNotice}</AlertDescription>
        </Alert>
      ) : null}

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">{zh ? "cc-switch 价格配置" : "cc-switch pricing configuration"}</CardTitle>
          <CardDescription>
            {zh
              ? "已内置 cc-switch 固定版本的 189 条默认价格。导入标准 model-pricing.json 会原子替换用户价格层；价格与删除标记高于内置默认、低于 Provider、模型和路由级 Vulcan 配置。"
              : "A frozen cc-switch snapshot provides 189 built-in defaults. Importing model-pricing.json atomically replaces the user pricing layer; its prices and tombstones override built-ins but remain below Vulcan provider, model, and route scopes."}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <input
            accept="application/json,.json"
            className="hidden"
            onChange={(event) => {
              const file = event.currentTarget.files?.[0]
              if (file) {
                void importPricingFile(file)
              }
            }}
            ref={pricingFileInput}
            type="file"
          />
          <Button
            disabled={importing}
            onClick={() => pricingFileInput.current?.click()}
            size="sm"
            type="button"
            variant="outline"
          >
            {importing ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : <FileUp className="mr-2 size-4" />}
            {zh ? "导入 model-pricing.json" : "Import model-pricing.json"}
          </Button>
        </CardContent>
      </Card>

      <div className="flex flex-wrap items-center gap-3">
        <span className="text-sm font-medium">{zh ? "费用来源" : "Cost source"}</span>
        <Select onValueChange={setConsumerFilter} value={consumerFilter}>
          <SelectTrigger className="w-[240px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={ALL_CONSUMERS}>{zh ? "全部消费者" : "All consumers"}</SelectItem>
            {consumerIds.map((consumerId) => (
              <SelectItem key={consumerId} value={consumerId}>{consumerId}</SelectItem>
            ))}
          </SelectContent>
        </Select>
        <span className="text-xs text-muted-foreground">
          {zh ? "身份由服务端认证绑定，不采用请求自报值。" : "Identity is server-bound and never trusted from a request claim."}
        </span>
      </div>

      <div className="grid gap-3 md:grid-cols-3">
        <MetricCard description={zh ? "包含重试与失败" : "Includes retries and failures"} title={zh ? "真实调用" : "Physical attempts"} value={formatInteger(attemptCount)} />
        <MetricCard description={zh ? "供应商直接报告" : "Directly provider-reported"} title={zh ? "用量覆盖率" : "Usage coverage"} value={attemptCount > 0 ? `${((reportedCount / attemptCount) * 100).toFixed(1)}%` : "—"} />
        <MetricCard description={zh ? "仅汇总已知值" : "Known values only"} title={zh ? "Token 合计" : "Total tokens"} value={formatInteger(totalTokens)} />
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">{zh ? "每日出口明细" : "Daily egress detail"}</CardTitle>
          <CardDescription>
            {zh ? "费用明确区分供应商上报与本地估算；未知费用显示为 —。" : "Provider-reported and locally estimated costs stay distinct; unknown cost is shown as —."}
          </CardDescription>
        </CardHeader>
        <CardContent className="overflow-x-auto">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{zh ? "日期" : "Date"}</TableHead>
                <TableHead>{zh ? "供应商 / 模型" : "Provider / model"}</TableHead>
                <TableHead>{zh ? "来源 / 操作" : "Consumer / operation"}</TableHead>
                <TableHead className="text-right">{zh ? "调用" : "Attempts"}</TableHead>
                <TableHead className="text-right">Token</TableHead>
                <TableHead className="text-right">{zh ? "费用" : "Cost"}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredDaily.length ? filteredDaily.map((row) => (
                <ProviderUsageRow key={dailyRowKey(row)} row={row} zh={zh} />
              )) : (
                <TableRow>
                  <TableCell className="py-8 text-center text-muted-foreground" colSpan={6}>
                    {loading ? (zh ? "正在读取…" : "Loading…") : (zh ? "暂无原生用量记录" : "No native usage records")}
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">{zh ? "近期 attempt 计费证据" : "Recent attempt billing evidence"}</CardTitle>
          <CardDescription>
            {zh
              ? "展示最近 200 次物理调用的供应商计费分区与冻结价格来源；缺失事实保持为空。"
              : "Shows provider billing partitions and frozen pricing provenance for the latest 200 physical attempts; absent facts remain empty."}
          </CardDescription>
        </CardHeader>
        <CardContent className="overflow-x-auto">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{zh ? "时间" : "Time"}</TableHead>
                <TableHead>{zh ? "消费者 / 操作" : "Consumer / operation"}</TableHead>
                <TableHead>{zh ? "供应商 / 模型" : "Provider / model"}</TableHead>
                <TableHead>{zh ? "状态 / 用量" : "Status / usage"}</TableHead>
                <TableHead>{zh ? "计费分区" : "Billing partition"}</TableHead>
                <TableHead className="text-right">{zh ? "费用 / 价格来源" : "Cost / pricing source"}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredRecentAttempts.length ? filteredRecentAttempts.map((attempt) => (
                <ProviderUsageAttemptRow attempt={attempt} key={attempt.attemptId} locale={locale} zh={zh} />
              )) : (
                <TableRow>
                  <TableCell className="py-8 text-center text-muted-foreground" colSpan={6}>
                    {loading ? (zh ? "正在读取…" : "Loading…") : (zh ? "暂无匹配的 attempt 证据" : "No matching attempt evidence")}
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">{zh ? "账户额度与套餐" : "Account quotas and plans"}</CardTitle>
          <CardDescription>{zh ? "独立快照，不通过余额差反推单次调用费用。" : "Independent snapshots; balance deltas never infer per-request cost."}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-2">
          {dashboard?.accounts.length ? dashboard.accounts.map((account) => (
            <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border px-3 py-2" key={`${account.connectionId}:${account.snapshotKind}`}>
              <div>
                <p className="text-sm font-medium">{account.providerId} · {account.connectionId}</p>
                <p className="text-xs text-muted-foreground">{account.payload.plan_name ?? account.snapshotKind}</p>
              </div>
              <div className="text-right text-sm">
                <p>{account.payload.remaining_amount ?? account.payload.balance_amount ?? "—"} {account.payload.currency ?? ""}</p>
                <p className="text-xs text-muted-foreground">{new Date(account.observedAtMs).toLocaleString()}</p>
              </div>
            </div>
          )) : <p className="text-sm text-muted-foreground">{zh ? "暂无受支持账户快照" : "No supported account snapshots"}</p>}
        </CardContent>
      </Card>

      {dashboard ? (
        <p className="text-xs text-muted-foreground">
          schema v{dashboard.schemaRevision} · aggregate {dashboard.aggregateRevision} · {dashboard.healthStatus}
        </p>
      ) : null}
    </div>
  )
}

// MetricCard renders one compact native-accounting metric.
// MetricCard 渲染一项紧凑原生计量指标。
function MetricCard(props: { description: string; title: string; value: string }) {
  return (
    <Card>
      <CardHeader className="pb-2">
        <CardDescription>{props.title}</CardDescription>
        <CardTitle className="text-2xl">{props.value}</CardTitle>
      </CardHeader>
      <CardContent className="text-xs text-muted-foreground">{props.description}</CardContent>
    </Card>
  )
}

// ProviderUsageRow renders one exact daily aggregate without converting decimal cost to floating point.
// ProviderUsageRow 渲染一条精确每日聚合，不把十进制费用转换为浮点数。
function ProviderUsageRow(props: { row: ProviderUsageDailySummary; zh: boolean }) {
  const { row } = props
  const cost = row.reportedCost ?? row.estimatedCost
  return (
    <TableRow>
      <TableCell>{row.calendarDate}</TableCell>
      <TableCell><div className="font-medium">{row.providerId}</div><div className="text-xs text-muted-foreground">{row.modelId}</div></TableCell>
      <TableCell><div>{row.consumerId}</div><div className="text-xs text-muted-foreground">{row.operation}</div></TableCell>
      <TableCell className="text-right"><div>{formatInteger(row.attemptCount)}</div><div className="text-xs text-muted-foreground">{row.reportedUsageAttemptCount}/{row.attemptCount} usage</div></TableCell>
      <TableCell className="text-right">{row.totalTokens == null ? "—" : formatInteger(row.totalTokens)}</TableCell>
      <TableCell className="text-right">
        {cost == null ? "—" : <div className="flex items-center justify-end gap-2"><Badge variant="outline">{row.reportedCost != null ? (props.zh ? "上报" : "reported") : (props.zh ? "估算" : "estimated")}</Badge><span>{cost} {row.currency}</span></div>}
      </TableCell>
    </TableRow>
  )
}

// ProviderUsageAttemptRow renders one immutable attempt with billing and pricing provenance.
// ProviderUsageAttemptRow 渲染一条带有计费与价格来源的不可变 attempt。
function ProviderUsageAttemptRow(props: {
  attempt: ProviderUsageAttemptSummary;
  locale: string;
  zh: boolean;
}) {
  // cost preserves the provider-reported value over the local estimate.
  // cost 优先保留供应商报告值，其次才使用本地估算值。
  const cost = props.attempt.reportedCost ?? props.attempt.estimatedCost
  // pricingSource describes the exact frozen scope and source identity when available.
  // pricingSource 在可用时描述精确冻结作用域与来源身份。
  const pricingSource = [props.attempt.pricingSourceKind, props.attempt.pricingSourceId]
    .filter((value): value is string => Boolean(value))
    .join(" · ")
  return (
    <TableRow>
      <TableCell className="whitespace-nowrap">{new Date(props.attempt.terminalAtMs).toLocaleString(props.locale)}</TableCell>
      <TableCell><div>{props.attempt.consumerId}</div><div className="text-xs text-muted-foreground">{props.attempt.operation}</div></TableCell>
      <TableCell>
        <div className="font-medium">{props.attempt.providerId}</div>
        <div className="text-xs text-muted-foreground">{props.attempt.modelId}</div>
        {props.attempt.transport ? <div className="text-xs text-muted-foreground">{props.attempt.transport}</div> : null}
      </TableCell>
      <TableCell>
        <div>{props.attempt.status}</div>
        <div className="text-xs text-muted-foreground">{props.attempt.usageAvailability} · Token {props.attempt.totalTokens == null ? "—" : formatInteger(props.attempt.totalTokens)}</div>
        {props.attempt.httpStatus != null ? <div className="text-xs text-muted-foreground">HTTP {props.attempt.httpStatus}</div> : null}
        {props.attempt.websocketCloseCode != null ? <div className="text-xs text-muted-foreground">WS {props.attempt.websocketCloseCode}</div> : null}
        {props.attempt.errorCategory || props.attempt.errorCode ? (
          <div className="text-xs text-muted-foreground">{[props.attempt.errorCategory, props.attempt.errorCode].filter(Boolean).join(" · ")}</div>
        ) : null}
      </TableCell>
      <TableCell className="min-w-[240px]">
        {props.attempt.billingDetails.length
          ? props.attempt.billingDetails.map((detail, index) => (
              <div className="text-xs" key={`${detail.billing_type}:${index}`}>{formatBillingDetail(detail)}</div>
            ))
          : <span className="text-muted-foreground">—</span>}
      </TableCell>
      <TableCell className="min-w-[240px] text-right">
        <div>
          {cost == null ? "—" : `${cost} ${props.attempt.currency ?? ""}`}
          {cost != null ? <Badge className="ml-2" variant="outline">{props.attempt.reportedCost != null ? (props.zh ? "上报" : "reported") : (props.zh ? "估算" : "estimated")}</Badge> : null}
        </div>
        <div className="mt-1 break-all text-xs text-muted-foreground">{pricingSource || "—"}</div>
        {props.attempt.pricingRevision ? <div className="break-all text-xs text-muted-foreground">rev {props.attempt.pricingRevision}</div> : null}
      </TableCell>
    </TableRow>
  )
}

// formatBillingDetail formats only provider-reported fields that are actually present.
// formatBillingDetail 仅格式化供应商实际报告的字段。
function formatBillingDetail(detail: ProviderUsageBillingDetail): string {
  // values retains exact field names so vendor-specific partitions stay auditable.
  // values 保留精确字段名，使供应商专属分区可审计。
  const values = [
    ["input", detail.input_tokens],
    ["output", detail.output_tokens],
    ["total", detail.total_tokens],
    ["text", detail.text_tokens],
    ["image", detail.image_tokens],
    ["reasoning", detail.reasoning_tokens],
  ].filter((entry): entry is [string, number] => entry[1] != null)
  return `${detail.billing_type}${values.length ? ` · ${values.map(([name, value]) => `${name} ${formatInteger(value)}`).join(" · ")}` : ""}`
}

// dailyRowKey returns the compound primary-key projection used by the daily table.
// dailyRowKey 返回每日表使用的复合主键投影。
function dailyRowKey(row: ProviderUsageDailySummary): string {
  return [row.calendarDate, row.providerId, row.serviceEntryId, row.consumerId, row.operation, row.modelId, row.currency].join("\u0000")
}

// formatInteger formats one exact safe JavaScript integer for display.
// formatInteger 格式化一个精确且在 JavaScript 安全范围内的整数用于展示。
function formatInteger(value: number): string {
  return new Intl.NumberFormat().format(value)
}
