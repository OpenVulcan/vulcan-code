import * as React from "react"

import { CircleAlert, Eye, RefreshCw, RotateCcw, ShieldCheck } from "lucide-react"

import {
  getProviderManifestStatus,
  previewProviderManifest,
  rollbackProviderManifest,
  syncProviderManifest,
} from "@/service/settings-actions-service"
import type {
  ManifestFieldProvenance,
  ManifestActiveLayer,
  ManifestModelBindingStatus,
  ManifestModelSourceStatus,
  ManifestProvenanceStatus,
  ManifestToolTargetTemplateInstanceStatus,
  ProviderManifestChangeKind,
  ProviderManifestStatus,
  ProviderManifestCandidatePreview,
  ProviderSyncReport,
} from "@/types"
import { Badge } from "@/components/ui/badge"
import { StreamPlaceholder } from "@/components/ui/stream-placeholder"
import { Button } from "@/components/ui/button"
import { DisclosureSection } from "@/components/ui/disclosure"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"

// errorMessage normalizes service failures for inline rendering.
// errorMessage 归一化服务失败以便行内渲染。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}

// activeLayerLabel renders one stable active-layer label.
// activeLayerLabel 渲染一个稳定的 active 层标签。
export function activeLayerLabel(layer: ManifestActiveLayer): string {
  switch (layer) {
    case "bundled":
      return "Bundled"
    case "active":
      return "Active"
    case "custom":
      return "Custom Local"
    case "previous":
      return "Previous"
  }
}

// manifestProvenanceStatusLabel renders one closed manifest verification-state label.
// manifestProvenanceStatusLabel 渲染一个封闭 Manifest 验证状态标签。
export function manifestProvenanceStatusLabel(status: ManifestProvenanceStatus): string {
  switch (status) {
    case "verified":
      return "已验证"
    case "migrated":
      return "已迁移"
    case "example_only":
      return "仅示例"
  }
}

// shortHash renders the first 12 characters of one content hash.
// shortHash 渲染一个内容摘要的前 12 个字符。
export function shortHash(value: string | undefined): string {
  if (!value) {
    return "—"
  }
  return value.length > 12 ? `${value.slice(0, 12)}…` : value
}

// manifestFieldSourceLabel renders one stable inheritance-source label.
// manifestFieldSourceLabel 渲染一个稳定的继承来源标签。
export function manifestFieldSourceLabel(source: ManifestFieldProvenance["source"]): string {
  switch (source) {
    case "provider_defaults":
      return "供应商默认"
    case "service_entry_override":
      return "入口覆盖"
    case "model_rule_exact":
      return "精确规则"
    case "model_rule_prefix":
      return "前缀规则"
  }
}

// manifestModelSourceLabel renders one closed backend-owned model inventory source.
// manifestModelSourceLabel 渲染一个由后端持有的封闭模型目录来源。
export function manifestModelSourceLabel(source: ManifestModelSourceStatus): string {
  switch (source) {
    case "manifest":
      return "Manifest"
    case "discovery":
      return "发现"
    case "manual":
      return "手工"
    case "merged":
      return "合并"
  }
}

// manifestModelBindingIdentity serializes the exact backend-owned binding identity without
// interpreting any routing or capability field.
// manifestModelBindingIdentity 序列化后端持有的精确绑定身份，不解释任何路由或能力字段。
export function manifestModelBindingIdentity(
  binding: Pick<ManifestModelBindingStatus, "service_entry_id" | "credential_profile_id" | "connection_id" | "model_id">,
): string {
  return JSON.stringify([
    binding.service_entry_id,
    binding.credential_profile_id ?? null,
    binding.connection_id ?? null,
    binding.model_id,
  ])
}

// manifestModelBindingChangeLabel renders one backend-classified model lifecycle change.
// manifestModelBindingChangeLabel 渲染一条由后端分类的模型生命周期变化。
export function manifestModelBindingChangeLabel(change: ProviderManifestChangeKind): string {
  switch (change) {
    case "added":
      return "新增"
    case "changed":
      return "变更"
    case "removed":
      return "退役"
  }
}

// ruleProvenanceSummary explains the selected-protocol winner for one model rule.
// ruleProvenanceSummary 解释一条模型规则的生效协议胜者。
export function ruleProvenanceSummary(
  provenance: ManifestFieldProvenance | undefined,
): string {
  if (!provenance) {
    return "未声明有效协议"
  }
  const rule = provenance.rule_id ? ` · ${provenance.rule_id}` : ""
  return `${manifestFieldSourceLabel(provenance.source)}${rule} · r${provenance.manifest_revision}`
}

// templateInstanceSummary renders one immutable template instance; instance supplies the exact
// template, service entry, model identity, display name, and generated target identity.
// templateInstanceSummary 渲染一个不可变模板实例；instance 提供精确模板、服务入口、模型身份、
// 显示名与生成后的目标身份。
export function templateInstanceSummary(
  instance: ManifestToolTargetTemplateInstanceStatus,
): string {
  return `template ${instance.template_id} · ${instance.service_entry_id}/${instance.model_id} · ${instance.model_display_name} · target ${instance.target_id}`
}

// syncOutcomeLabel renders one stable sync-outcome label.
// syncOutcomeLabel 渲染一个稳定的同步结果标签。
export function syncOutcomeLabel(outcome: ProviderSyncReport["outcome"]): string {
  switch (outcome) {
    case "not_configured":
      return "未配置"
    case "up_to_date":
      return "已最新"
    case "activated":
      return "已激活"
    case "rejected":
      return "已拒绝"
    case "not_pinned":
      return "未固定"
  }
}

// syncStatusSummary renders the row-level sync status for one provider: unconfigured,
// rejected with its classified kind, or last-synced. Quarantine counts are appended when
// any rejected candidate exists (plan §R10).
// syncStatusSummary 渲染一个供应商的行级同步状态：未配置、带分类类型的已拒绝，或最近
// 同步时间。存在被拒候选时附加隔离计数（方案 §R10）。
export function syncStatusSummary(
  status: Pick<ProviderManifestStatus, "sync_configured" | "sync_state" | "sync_check_state" | "quarantine">,
): { text: string; kind: "unconfigured" | "rejected" | "ok" | "quarantine"; quarantineCount: number } {
  if (!status.sync_configured) {
    return { text: "未配置", kind: "unconfigured", quarantineCount: status.quarantine.length }
  }
  // latestQuarantine preserves a visible failure even before the first successful sync creates state.
  // latestQuarantine 在首次成功同步创建状态前仍保留可见失败。
  const latestQuarantine = status.quarantine.reduce<(typeof status.quarantine)[number] | undefined>(
    (latest, candidate) =>
      !latest || candidate.quarantined_at_unix_ms > latest.quarantined_at_unix_ms ? candidate : latest,
    undefined,
  )
  const failureKind = status.sync_check_state?.last_error_kind
    ?? status.sync_state?.last_error_kind
    ?? latestQuarantine?.failure_kind
  if (failureKind) {
    return {
      text: failureKind,
      kind: "rejected",
      quarantineCount: status.quarantine.length,
    }
  }
  // lastChecked and lastSuccessful stay independent: a 304/failure advances only lastChecked.
  // lastChecked 与 lastSuccessful 保持独立：304/失败只推进 lastChecked。
  const lastChecked = status.sync_check_state?.checked_at_unix_ms ?? 0
  const lastSuccessful = status.sync_check_state?.last_successful_at_unix_ms
    ?? status.sync_state?.synced_at_unix_ms
    ?? 0
  return {
    text: lastChecked > 0
      ? `最近检查 ${new Date(lastChecked).toLocaleString()} · 最近激活 ${lastSuccessful > 0 ? new Date(lastSuccessful).toLocaleString() : "无"}`
      : (lastSuccessful > 0 ? `最近激活 ${new Date(lastSuccessful).toLocaleString()}` : "从未同步"),
    kind: "ok",
    quarantineCount: status.quarantine.length,
  }
}

// ProviderManifestStatusPanel shows the typed per-provider manifest status (plan §R10):
// which manifest is active, its source layer, sync state, and rollback/sync actions.
// ProviderManifestStatusPanel 展示 typed 逐供应商清单状态（方案 §R10）：当前 active 清单、
// 来源层、同步状态与同步/回滚操作。
export function ProviderManifestStatusPanel() {
  const [statuses, setStatuses] = React.useState<ProviderManifestStatus[] | null>(null)
  const [busyProviderId, setBusyProviderId] = React.useState<string | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [preview, setPreview] = React.useState<ProviderManifestCandidatePreview | null>(null)
  // Reads and mutations share one lock so late catalog reads cannot regress a completed mutation.
  // 读取和变更共享锁，避免迟到的目录读取覆盖已完成的变更。
  const actionRunningRef = React.useRef(false)
  const [refreshing, setRefreshing] = React.useState(false)

  const refresh = React.useCallback(async () => {
    if (actionRunningRef.current) return
    actionRunningRef.current = true
    setRefreshing(true)
    // A refreshed backend snapshot invalidates every previously rendered candidate delta.
    // 刷新后端快照会使之前渲染的所有候选差异失效。
    setPreview(null)
    setError(null)
    try {
      setStatuses(await getProviderManifestStatus())
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      actionRunningRef.current = false
      setRefreshing(false)
    }
  }, [])

  React.useEffect(() => {
    void refresh()
  }, [refresh])

  // syncOne synchronizes exactly one provider and refreshes the snapshot.
  // syncOne 同步精确一个供应商并刷新快照。
  const syncOne = React.useCallback(async (providerId: string) => {
    if (actionRunningRef.current) return
    actionRunningRef.current = true
    setBusyProviderId(providerId)
    setError(null)
    setPreview(null)
    try {
      const report = await syncProviderManifest(providerId)
      if (report.outcome === "rejected") {
        const reason = report.failure_message
          ? `${report.failure_kind ?? "sync_failed"}：${report.failure_message}`
          : (report.failure_kind ?? "未知原因")
        setError(`供应商 ${providerId} 同步被拒绝：${reason}`)
      }
      setStatuses(await getProviderManifestStatus())
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      actionRunningRef.current = false
      setBusyProviderId(null)
    }
  }, [])

  // rollbackOne explicitly rolls one provider back to its previous layer.
  // rollbackOne 显式把一个供应商回滚到 previous 层。
  const rollbackOne = React.useCallback(async (providerId: string) => {
    if (actionRunningRef.current) return
    actionRunningRef.current = true
    setBusyProviderId(providerId)
    setError(null)
    setPreview(null)
    try {
      await rollbackProviderManifest(providerId)
      setStatuses(await getProviderManifestStatus())
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      actionRunningRef.current = false
      setBusyProviderId(null)
    }
  }, [])

  // previewOne verifies one candidate and renders backend-owned semantic deltas without writes.
  // previewOne 验证一个候选并渲染后端持有的语义差异，且不执行写入。
  const previewOne = React.useCallback(async (providerId: string) => {
    if (actionRunningRef.current) return
    actionRunningRef.current = true
    setBusyProviderId(providerId)
    setError(null)
    // A failed new preview must never leave another provider's stale delta visible.
    // 新预览失败时绝不得继续显示另一供应商的过期差异。
    setPreview(null)
    try {
      setPreview(await previewProviderManifest(providerId))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      actionRunningRef.current = false
      setBusyProviderId(null)
    }
  }, [])

  return (
    <section className="rounded-xl border p-4">
      <div className="flex items-center justify-between gap-2">
        <div>
          <h3 className="flex items-center gap-2 text-sm font-semibold">
            <ShieldCheck className="size-4" />
            Manifest 状态
          </h3>
          <p className="text-xs text-muted-foreground">
            当前生效清单、来源层、同步状态与回滚信息；无需读取日志即可判断。
          </p>
        </div>
        <Button
          aria-label="刷新 Manifest 状态"
          disabled={refreshing || busyProviderId !== null}
          onClick={() => void refresh()}
          size="sm"
          variant="outline"
        >
          <RefreshCw className="size-3.5" />
          刷新
        </Button>
      </div>

      {error ? (
        <div className="mt-3 rounded-lg border border-destructive/40 p-3 text-sm text-destructive" role="alert">
          {error}
        </div>
      ) : null}

      {!statuses ? (
        <StreamPlaceholder className="mt-3" label="加载中…" shape="rows" count={3} />
      ) : statuses.length === 0 ? (
        <div className="mt-3 text-sm text-muted-foreground">暂无系统供应商。</div>
      ) : (
        <div className="mt-3 overflow-x-auto">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>供应商</TableHead>
                <TableHead>生效层</TableHead>
                <TableHead>版本</TableHead>
                <TableHead>哈希</TableHead>
                <TableHead>同步</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {statuses.map((status) => (
                <TableRow key={status.provider_id}>
                  <TableCell className="font-medium">
                    <div>{status.provider_id}</div>
                    {status.critical_error ? (
                      <div
                        className="mt-1 max-w-72 text-xs font-normal text-destructive"
                        title={status.critical_error.message}
                      >
                        严重：{status.critical_error.failure_kind} · {status.critical_error.message}
                      </div>
                    ) : status.status_error ? (
                      <div
                        className="mt-1 max-w-72 text-xs font-normal text-destructive"
                        title={status.status_error.message}
                      >
                        状态不完整：{status.status_error.failure_kind} · {status.status_error.message}
                      </div>
                    ) : status.bundled_failure ? (
                      <div
                        className="mt-1 max-w-72 text-xs font-normal text-[var(--vcn-content-warning)]"
                        title={status.bundled_failure.message}
                      >
                        内置回退不可用：{status.bundled_failure.failure_kind}
                      </div>
                    ) : null}
                  </TableCell>
                  <TableCell>
                    <div className="flex flex-wrap gap-1">
                      <Badge variant={status.active?.layer === "bundled" ? "secondary" : "default"}>
                        {status.active ? activeLayerLabel(status.active.layer) : "—"}
                      </Badge>
                      {status.active ? (
                        <Badge variant="outline">
                          引擎 v{status.active.minimum_manifest_engine} · {manifestProvenanceStatusLabel(status.active.provenance_status)} · {status.active.evidence_source_count} 证据
                        </Badge>
                      ) : null}
                    </div>
                  </TableCell>
                  <TableCell>
                    {status.active?.revision ?? status.bundled?.revision ?? "—"}
                    {status.previous ? `（前 ${status.previous.revision}）` : ""}
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {shortHash(status.active?.sha256 ?? status.bundled?.sha256)}
                  </TableCell>
                  <TableCell>
                    {(() => {
                      const summary = syncStatusSummary(status)
                      if (summary.kind === "unconfigured") {
                        return <span className="text-xs text-muted-foreground">{summary.text}</span>
                      }
                      if (summary.kind === "rejected") {
                        return (
                          <span className="inline-flex items-center gap-1 text-xs text-destructive">
                            <CircleAlert className="size-3" />
                            {summary.text}
                          </span>
                        )
                      }
                      return <span className="text-xs text-muted-foreground">{summary.text}</span>
                    })()}
                    {status.quarantine.length > 0 ? (
                      <span className="ml-1 text-xs text-[var(--vcn-content-warning)]">
                        {status.quarantine.length} 隔离
                      </span>
                    ) : null}
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="inline-flex gap-1">
                      <Button
                        disabled={refreshing || busyProviderId !== null || !status.sync_configured}
                        onClick={() => void previewOne(status.provider_id)}
                        size="sm"
                        variant="ghost"
                        title={status.sync_configured ? "只读校验并预览候选差异" : "远端同步未配置"}
                      >
                        <Eye className="size-3" />
                        预览
                      </Button>
                      <Button
                        disabled={refreshing || busyProviderId !== null}
                        onClick={() => void syncOne(status.provider_id)}
                        size="sm"
                        variant="ghost"
                      >
                        <RefreshCw className="size-3" />
                        同步
                      </Button>
                      <Button
                        disabled={refreshing || busyProviderId !== null || !status.previous}
                        onClick={() => void rollbackOne(status.provider_id)}
                        size="sm"
                        variant="ghost"
                        title={status.previous ? "回滚到 previous 层" : "无 previous 层可回滚"}
                      >
                        <RotateCcw className="size-3" />
                        回滚
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          {statuses.some((status) => !status.sync_configured) ? (
            <p className="mt-2 text-xs text-muted-foreground">
              远端同步未配置：不会自动检查更新；当前实际生效层以各供应商行显示为准。
            </p>
          ) : null}
          {preview ? (
            <div className="mt-3 rounded-lg border bg-muted/20 p-3 text-xs">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-semibold">{preview.provider_id} 候选预览</span>
                <Badge variant={preview.executable ? "default" : "destructive"}>
                  {preview.outcome}
                </Badge>
                <span className="text-muted-foreground">
                  r{preview.current_revision ?? "—"} → r{preview.candidate_revision ?? "—"}
                </span>
              </div>
              {preview.failure_kind ? (
                <p className="mt-2 text-destructive">
                  {preview.failure_kind}{preview.failure_message ? `：${preview.failure_message}` : ""}
                </p>
              ) : null}
              <p className="mt-2 text-muted-foreground">
                模型绑定变更 {preview.model_binding_change_total_count} 条
                {preview.model_binding_changes_truncated ? "（详情已限界）" : ""} · operation 变更 {preview.operation_change_total_count} 条
                {preview.operation_changes_truncated ? "（详情已限界）" : ""}。预览不写入 staging、Active、Previous 或同步状态。
              </p>
              <div className="mt-2 space-y-1">
                {preview.model_binding_changes.slice(0, 20).map((delta, index) => {
                  // binding is whichever side owns the concrete identity for this delta.
                  // binding 是此差异中持有具体身份的任一侧。
                  const binding = delta.candidate ?? delta.current
                  return binding ? (
                    <div key={`preview-binding:${binding.service_entry_id}:${binding.credential_profile_id ?? "all"}:${binding.connection_id ?? "static"}:${binding.model_id}:${index}`}>
                      {delta.change} · {binding.service_entry_id}/{binding.model_id} · {binding.connection_id ?? "静态作用域"} · {delta.current?.effective_protocol ?? "—"} → {delta.candidate?.effective_protocol ?? "—"}
                    </div>
                  ) : null
                })}
                {preview.operation_changes.slice(0, 20).map((delta, index) => {
                  // operation is whichever side owns the semantic identity for this delta.
                  // operation 是此差异中持有语义身份的任一侧。
                  const operation = delta.candidate ?? delta.current
                  return operation ? (
                    <div key={`preview-operation:${operation.operation_id}:${index}`}>
                      {delta.change} · operation {operation.operation_id} · {operation.carrier} · {operation.asynchronous ? "异步" : "同步"}
                    </div>
                  ) : null
                })}
              </div>
            </div>
          ) : null}
          <p className="mt-2 text-xs text-muted-foreground">
            新 Manifest 激活只影响新会话，或对现有会话显式重新选择模型后的新租约；活动会话不会在中途切换协议、Profile 或上传通道。
          </p>
          <div className="mt-4 space-y-2">
            {statuses.map((status) => {
              // activeChangeByIdentity indexes backend-classified additions and changes for active rows.
              // activeChangeByIdentity 为 active 行索引后端分类的新增与变更。
              const activeChangeByIdentity = new Map(
                status.active_model_binding_changes.flatMap((delta) => delta.candidate
                  ? [[manifestModelBindingIdentity(delta.candidate), delta.change] as const]
                  : []),
              )
              // retiredChanges retain removed identities that no longer have an active row.
              // retiredChanges 保留已不存在 active 行的退役身份。
              const retiredChanges = status.active_model_binding_changes.filter(
                (delta) => delta.change === "removed" && delta.current,
              )
              return (
                <DisclosureSection
                  description={<>
                    {status.model_binding_total_count} 条模型绑定 · {status.model_rules.length} 条模型规则 · {status.operations.length} 个操作 · {status.tool_target_template_instances.length} 个模板实例
                  </>}
                  key={`${status.provider_id}-rules`}
                  meta={status.active_model_binding_change_baseline
                    ? `${status.active_model_binding_change_total_count} 项模型变化`
                    : undefined}
                  title={status.provider_id}
                >
                <div className="space-y-2">
                  {status.active_model_binding_change_baseline ? (
                    <p className="text-xs text-muted-foreground">
                      模型变化由后端对比已认证 {activeLayerLabel(status.active_model_binding_change_baseline.layer)} r{status.active_model_binding_change_baseline.revision} 与当前 active 代际得出；共 {status.active_model_binding_change_total_count} 项
                      {status.active_model_binding_changes_truncated ? "，当前明细已限界" : ""}。
                    </p>
                  ) : null}
                  {status.model_rules.length === 0 ? (
                    <p className="text-xs text-muted-foreground">此清单没有模型规则。</p>
                  ) : (
                    status.model_rules.map((rule) => (
                      <div
                        className="rounded-md bg-muted/40 px-3 py-2 text-xs"
                        key={`${rule.service_entry_id}:${rule.rule_id}`}
                      >
                        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                          <span className="font-medium">{rule.matcher.kind}:{rule.matcher.value}</span>
                          <span className="text-muted-foreground">入口 {rule.service_entry_id}</span>
                          <span>{rule.selected_protocol ?? "无有效协议"}</span>
                          <span className="text-muted-foreground">
                            {ruleProvenanceSummary(rule.field_provenance.selected_protocol)}
                          </span>
                        </div>
                        <div className="mt-1 text-muted-foreground">
                          候选 [{rule.supported_protocols.join(", ") || "无"}] · 默认{rule.default_enabled ? "启用" : "停用"}
                          {rule.model_parameter_ids.length > 0
                            ? ` · 参数 [${rule.model_parameter_ids.join(", ")}]`
                            : ""}
                        </div>
                        <div className="mt-1 text-muted-foreground">
                          usage [{rule.usage_profile_refs.join(", ") || "无"}] · request [{rule.request_profile_refs.join(", ") || "无"}] · response [{rule.response_profile_refs.join(", ") || "无"}] · operation [{rule.operation_refs.join(", ") || "无"}]
                        </div>
                        <div className="mt-1 text-muted-foreground">
                          规则证据 {manifestProvenanceStatusLabel(rule.provenance.provenance_status)} · r{rule.provenance.manifest_revision}
                          {rule.provenance.provenance_ref ? ` · ${rule.provenance.provenance_ref}` : " · 继承整份清单来源"}
                        </div>
                      </div>
                    ))
                  )}
                  {status.model_bindings.slice(0, 200).map((binding) => {
                    // activeChange is the exact backend delta for this active identity, when present.
                    // activeChange 是当前 active 身份对应的精确后端差异（存在时）。
                    const activeChange = activeChangeByIdentity.get(manifestModelBindingIdentity(binding))
                    return (
                      <div
                        className="rounded-md border px-3 py-2 text-xs"
                        key={`binding:${manifestModelBindingIdentity(binding)}`}
                      >
                        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                          <span className="font-medium">{binding.display_name}</span>
                          <span className="font-mono text-muted-foreground">{binding.model_id}</span>
                          <Badge variant="outline">{manifestModelSourceLabel(binding.model_source)}</Badge>
                          {activeChange ? (
                            <Badge variant={activeChange === "added" ? "secondary" : "outline"}>
                              {manifestModelBindingChangeLabel(activeChange)}
                            </Badge>
                          ) : null}
                          <span>{binding.effective_protocol ?? "不可执行"}</span>
                          <Badge variant={binding.executable ? "secondary" : "destructive"}>
                            {binding.executable ? "可执行" : "已阻止"}
                          </Badge>
                        </div>
                        <div className="mt-1 text-muted-foreground">
                          入口 {binding.service_entry_id} · 配置档 {binding.credential_profile_id ?? "全部"} · 连接 {binding.connection_id ?? "未配置（静态作用域）"} · 声明支持 [{binding.supported_protocols.join(", ") || "无"}] · 目录可用 [{binding.available_protocols.join(", ") || "未观测"}]
                        </div>
                        <div className="mt-1 text-muted-foreground">
                          绑定来源 {binding.binding_source ?? "无"} · 规则 {binding.binding_rule_id ?? "供应商默认"} · r{binding.manifest_revision} · 目录{binding.catalog_observed ? (binding.catalog_enabled ? "已启用" : "已观测但未启用") : "未观测"}
                        </div>
                        <div className="mt-1 text-muted-foreground">
                          能力操作 [{binding.capabilities?.operations?.join(", ") || "无"}] · 输入 [{binding.capabilities?.input_resources?.join(", ") || "无"}] · 输出 [{binding.capabilities?.output_resources?.join(", ") || "无"}] · 上下文 {binding.capabilities?.context_limit ?? "未知"} · 输出上限 {binding.capabilities?.output_limit ?? "未知"} · 能力证据 {binding.capabilities?.evidence_revision ?? "无"} · Manifest 来源字段 {Object.keys(binding.capability_provenance).length}
                        </div>
                        <div className="mt-1 text-muted-foreground">
                          operation [{binding.operation_refs.join(", ") || "无"}]
                          {binding.unavailable_reason ? ` · ${binding.unavailable_reason}` : ""}
                        </div>
                      </div>
                    )
                  })}
                  {status.model_binding_total_count > Math.min(status.model_bindings.length, 200) ? (
                    <p className="text-xs text-muted-foreground">
                      当前仅渲染前 {Math.min(status.model_bindings.length, 200)} 条；后端共返回 {status.model_bindings.length} 条、完整数量 {status.model_binding_total_count} 条
                      {status.model_bindings_truncated ? "，后端响应已限界" : ""}。
                    </p>
                  ) : null}
                  {retiredChanges.length > 0 ? (
                    <div className="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-xs">
                      <div className="mb-1 font-medium">退役模型</div>
                      {retiredChanges.map((delta) => {
                        // retiredBinding is guaranteed by the filtered backend delta set above.
                        // retiredBinding 已由上方后端差异过滤保证存在。
                        const retiredBinding = delta.current!
                        return (
                          <div
                            className="flex flex-wrap items-center gap-x-2 gap-y-1 text-muted-foreground"
                            key={`retired:${manifestModelBindingIdentity(retiredBinding)}`}
                          >
                            <Badge variant="destructive">{manifestModelBindingChangeLabel(delta.change)}</Badge>
                            <span>{retiredBinding.display_name}</span>
                            <span className="font-mono">{retiredBinding.model_id}</span>
                            <span>{retiredBinding.service_entry_id}</span>
                            <span>{retiredBinding.effective_protocol ?? "不可执行"}</span>
                          </div>
                        )
                      })}
                    </div>
                  ) : null}
                  {status.operations.map((operation) => (
                    <div
                      className="rounded-md border border-dashed px-3 py-2 text-xs text-muted-foreground"
                      key={`operation:${operation.operation_id}`}
                    >
                      <div>
                        operation {operation.operation_id} · {operation.operation} · {operation.carrier} · {operation.asynchronous ? "异步" : "同步"} · 超时 {operation.timeout_ms ?? "未声明"} ms
                      </div>
                      {operation.asynchronous ? (
                        <div className="mt-1">
                          轮询 {operation.poll_interval_ms ?? "未声明"} ms · 最大退避 {operation.max_backoff_ms ?? "未声明"} ms · 取消 {operation.cancellation_supported ? "支持" : "不支持"}
                        </div>
                      ) : null}
                      <div className="mt-1">
                        输入 [{operation.input_resources.join(", ") || "无"}] MIME [{operation.input_mime_types.join(", ") || "无"}] · 输出 [{operation.output_resources.join(", ") || "无"}] MIME [{operation.output_mime_types.join(", ") || "无"}]
                      </div>
                      <div className="mt-1">
                        upload [{operation.upload_channel_refs.join(", ") || "无"}] · tool [{operation.tool_target_refs.join(", ") || "无"}] · template [{operation.tool_target_template_refs.join(", ") || "无"}] · 匹配模型 {operation.matching_model_total_count}
                        {operation.matching_models_truncated ? "（摘要已限界）" : ""}
                      </div>
                    </div>
                  ))}
                  {status.tool_target_template_instances.map((instance) => (
                    <div
                      className="rounded-md border border-dashed px-3 py-2 text-xs text-muted-foreground"
                      key={`template-instance:${instance.target_id}`}
                    >
                      {templateInstanceSummary(instance)}
                    </div>
                  ))}
                </div>
              </DisclosureSection>
              )
            })}
          </div>
        </div>
      )}
    </section>
  )
}
