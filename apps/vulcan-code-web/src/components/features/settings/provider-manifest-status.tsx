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
import { Badge } from "@/ui-kit/primitives/badge"
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder"
import { Button } from "@/ui-kit/primitives/button"
import { DisclosureSection } from "@/ui-kit/primitives/disclosure"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/ui-kit/primitives/table"
import { PageEmptyState } from "@/components/page"
import { formatMessage, useI18n, type Translate } from "@/i18n"
import "@/ui-kit/styles/features/settings-forms.css"
import "@/ui-kit/styles/features/provider-manifest.css"
import { formatDateTime, errorMessage } from "@/lib/display-format"


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
export function manifestProvenanceStatusLabel(status: ManifestProvenanceStatus, t: Translate): string {
  switch (status) {
    case "verified":
      return t("settings.manifest.provenance.verified")
    case "migrated":
      return t("settings.manifest.provenance.migrated")
    case "example_only":
      return t("settings.manifest.provenance.exampleOnly")
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
export function manifestFieldSourceLabel(source: ManifestFieldProvenance["source"], t: Translate): string {
  switch (source) {
    case "provider_defaults":
      return t("settings.manifest.fieldSource.providerDefaults")
    case "service_entry_override":
      return t("settings.manifest.fieldSource.entryOverride")
    case "model_rule_exact":
      return t("settings.manifest.fieldSource.exactRule")
    case "model_rule_prefix":
      return t("settings.manifest.fieldSource.prefixRule")
  }
}

// manifestModelSourceLabel renders one closed backend-owned model inventory source.
// manifestModelSourceLabel 渲染一个由后端持有的封闭模型目录来源。
export function manifestModelSourceLabel(source: ManifestModelSourceStatus, t: Translate): string {
  switch (source) {
    case "manifest":
      return "Manifest"
    case "discovery":
      return t("settings.manifest.modelSource.discovery")
    case "manual":
      return t("settings.manifest.modelSource.manual")
    case "merged":
      return t("settings.manifest.modelSource.merged")
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
export function manifestModelBindingChangeLabel(change: ProviderManifestChangeKind, t: Translate): string {
  switch (change) {
    case "added":
      return t("settings.manifest.change.added")
    case "changed":
      return t("settings.manifest.change.changed")
    case "removed":
      return t("settings.manifest.change.removed")
  }
}

// ruleProvenanceSummary explains the selected-protocol winner for one model rule.
// ruleProvenanceSummary 解释一条模型规则的生效协议胜者。
export function ruleProvenanceSummary(
  provenance: ManifestFieldProvenance | undefined,
  t: Translate,
): string {
  if (!provenance) {
    return t("settings.manifest.noDeclaredProtocol")
  }
  const rule = provenance.rule_id ? ` · ${provenance.rule_id}` : ""
  return `${manifestFieldSourceLabel(provenance.source, t)}${rule} · r${provenance.manifest_revision}`
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
export function syncOutcomeLabel(outcome: ProviderSyncReport["outcome"], t: Translate): string {
  switch (outcome) {
    case "not_configured":
      return t("settings.manifest.outcome.notConfigured")
    case "up_to_date":
      return t("settings.manifest.outcome.upToDate")
    case "activated":
      return t("settings.manifest.outcome.activated")
    case "rejected":
      return t("settings.manifest.outcome.rejected")
    case "not_pinned":
      return t("settings.manifest.outcome.notPinned")
  }
}

// syncStatusSummary renders the row-level sync status for one provider: unconfigured,
// rejected with its classified kind, or last-synced. Quarantine counts are appended when
// any rejected candidate exists (plan §R10).
// syncStatusSummary 渲染一个供应商的行级同步状态：未配置、带分类类型的已拒绝，或最近
// 同步时间。存在被拒候选时附加隔离计数（方案 §R10）。
export function syncStatusSummary(
  status: Pick<ProviderManifestStatus, "sync_configured" | "sync_state" | "sync_check_state" | "quarantine">,
  t: Translate,
  locale: string,
): { text: string; kind: "unconfigured" | "rejected" | "ok" | "quarantine"; quarantineCount: number } {
  if (!status.sync_configured) {
    return { text: t("settings.manifest.outcome.notConfigured"), kind: "unconfigured", quarantineCount: status.quarantine.length }
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
      ? formatMessage(t("settings.manifest.syncCheckedActivated"), {
        checked: formatDateTime(lastChecked, locale) ?? String(lastChecked),
        activated: lastSuccessful > 0 ? formatDateTime(lastSuccessful, locale) ?? String(lastSuccessful) : t("settings.manifest.none"),
      })
      : (lastSuccessful > 0
        ? formatMessage(t("settings.manifest.syncActivated"), { activated: formatDateTime(lastSuccessful, locale) ?? String(lastSuccessful) })
        : t("settings.manifest.syncNever")),
    kind: "ok",
    quarantineCount: status.quarantine.length,
  }
}

// ProviderManifestStatusPanel shows the typed per-provider manifest status (plan §R10):
// which manifest is active, its source layer, sync state, and rollback/sync actions.
// ProviderManifestStatusPanel 展示 typed 逐供应商清单状态（方案 §R10）：当前 active 清单、
// 来源层、同步状态与同步/回滚操作。
export function ProviderManifestStatusPanel() {
  const { t, locale } = useI18n()
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
          ? formatMessage(t("settings.manifest.failureDetail"), { kind: report.failure_kind ?? "sync_failed", message: report.failure_message })
          : (report.failure_kind ?? t("settings.manifest.unknownReason"))
        setError(formatMessage(t("settings.manifest.syncRejected"), { provider: providerId, reason }))
      }
      setStatuses(await getProviderManifestStatus())
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      actionRunningRef.current = false
      setBusyProviderId(null)
    }
  }, [t])

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

  // none is the shared placeholder for every empty reference list below.
  // none 是下方所有空引用列表共用的占位文本。
  const none = t("settings.manifest.none")

  return (
    <section className="vk-form-box vk-mf">
      <div className="vk-form-head">
        <div>
          <h3 className="vk-mf-title">
            <ShieldCheck aria-hidden="true" />
            {t("settings.manifest.title")}
          </h3>
          <p>
            {t("settings.manifest.description")}
          </p>
        </div>
        <Button
          aria-label={t("settings.manifest.refreshLabel")}
          disabled={refreshing || busyProviderId !== null}
          onClick={() => void refresh()}
          size="sm"
          variant="outline"
        >
          <RefreshCw aria-hidden="true" />
          {t("settings.manifest.refresh")}
        </Button>
      </div>

      {error ? (
        <div className="vk-form-fault" role="alert">
          {error}
        </div>
      ) : null}

      {!statuses ? (
        <StreamPlaceholder label={t("ui.loading")} shape="rows" count={3} />
      ) : statuses.length === 0 ? (
        <PageEmptyState title={t("settings.manifest.empty")} />
      ) : (
        <div className="vk-mf-body">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("settings.manifest.column.provider")}</TableHead>
                <TableHead>{t("settings.manifest.column.layer")}</TableHead>
                <TableHead>{t("settings.manifest.column.revision")}</TableHead>
                <TableHead>{t("settings.manifest.column.hash")}</TableHead>
                <TableHead>{t("settings.manifest.column.sync")}</TableHead>
                <TableHead className="vk-mf-end">{t("settings.manifest.column.actions")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {statuses.map((status) => (
                <TableRow key={status.provider_id}>
                  <TableCell className="vk-mf-name">
                    <strong>{status.provider_id}</strong>
                    {status.critical_error ? (
                      <small className="vk-mf-flag" data-tone="bad" title={status.critical_error.message}>
                        {formatMessage(t("settings.manifest.criticalError"), { kind: status.critical_error.failure_kind, message: status.critical_error.message })}
                      </small>
                    ) : status.status_error ? (
                      <small className="vk-mf-flag" data-tone="bad" title={status.status_error.message}>
                        {formatMessage(t("settings.manifest.statusIncomplete"), { kind: status.status_error.failure_kind, message: status.status_error.message })}
                      </small>
                    ) : status.bundled_failure ? (
                      <small className="vk-mf-flag" data-tone="warn" title={status.bundled_failure.message}>
                        {formatMessage(t("settings.manifest.bundledUnavailable"), { kind: status.bundled_failure.failure_kind })}
                      </small>
                    ) : null}
                  </TableCell>
                  <TableCell className="vk-mf-name">
                    <strong data-layer={status.active?.layer}>{status.active ? activeLayerLabel(status.active.layer) : "—"}</strong>
                    {status.active ? (
                      <small className="vk-mf-flag">
                        {formatMessage(t("settings.manifest.engineBadge"), { engine: status.active.minimum_manifest_engine, provenance: manifestProvenanceStatusLabel(status.active.provenance_status, t), count: status.active.evidence_source_count })}
                      </small>
                    ) : null}
                  </TableCell>
                  <TableCell>
                    {status.active?.revision ?? status.bundled?.revision ?? "—"}
                    {status.previous ? formatMessage(t("settings.manifest.previousRevision"), { revision: status.previous.revision }) : ""}
                  </TableCell>
                  <TableCell className="vk-mono vk-faint">
                    {shortHash(status.active?.sha256 ?? status.bundled?.sha256)}
                  </TableCell>
                  <TableCell className="vk-mf-name vk-mf-sync">
                    {(() => {
                      const summary = syncStatusSummary(status, t, locale)
                      if (summary.kind === "unconfigured") {
                        return <small className="vk-mf-flag">{summary.text}</small>
                      }
                      if (summary.kind === "rejected") {
                        return (
                          <small className="vk-mf-flag" data-tone="bad">
                            <CircleAlert aria-hidden="true" />
                            {summary.text}
                          </small>
                        )
                      }
                      return <small className="vk-mf-flag">{summary.text}</small>
                    })()}
                    {status.quarantine.length > 0 ? (
                      <small className="vk-mf-flag" data-tone="warn">
                        {formatMessage(t("settings.manifest.quarantined"), { count: status.quarantine.length })}
                      </small>
                    ) : null}
                  </TableCell>
                  <TableCell className="vk-mf-end">
                    <div className="vk-mf-acts">
                      {/* Row actions are icon buttons named by their labels, so the table keeps room for its data columns. */}
                      {/* 行操作为以标签命名的图标按钮，使表格为数据列保留空间。 */}
                      <Button
                        aria-label={t("settings.manifest.preview")}
                        disabled={refreshing || busyProviderId !== null || !status.sync_configured}
                        onClick={() => void previewOne(status.provider_id)}
                        size="icon-sm"
                        variant="ghost"
                        title={status.sync_configured ? t("settings.manifest.previewHint") : t("settings.manifest.syncNotConfigured")}
                      >
                        <Eye aria-hidden="true" />
                      </Button>
                      <Button
                        aria-label={t("settings.manifest.sync")}
                        disabled={refreshing || busyProviderId !== null}
                        onClick={() => void syncOne(status.provider_id)}
                        size="icon-sm"
                        variant="ghost"
                        title={t("settings.manifest.sync")}
                      >
                        <RefreshCw aria-hidden="true" />
                      </Button>
                      <Button
                        aria-label={t("settings.manifest.rollback")}
                        disabled={refreshing || busyProviderId !== null || !status.previous}
                        onClick={() => void rollbackOne(status.provider_id)}
                        size="icon-sm"
                        variant="ghost"
                        title={status.previous ? t("settings.manifest.rollbackHint") : t("settings.manifest.noPrevious")}
                      >
                        <RotateCcw aria-hidden="true" />
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          {statuses.some((status) => !status.sync_configured) ? (
            <p className="vk-field-hint">
              {t("settings.manifest.syncNotConfiguredNote")}
            </p>
          ) : null}
          {preview ? (
            <div className="vk-mf-preview">
              <div className="vk-mf-entry-head">
                <b>{formatMessage(t("settings.manifest.candidatePreview"), { provider: preview.provider_id })}</b>
                <Badge variant={preview.executable ? "default" : "destructive"}>
                  {preview.outcome}
                </Badge>
                <code>
                  r{preview.current_revision ?? "—"} → r{preview.candidate_revision ?? "—"}
                </code>
              </div>
              {preview.failure_kind ? (
                <p className="vk-mf-flag" data-tone="bad">
                  {preview.failure_message ? formatMessage(t("settings.manifest.failureDetail"), { kind: preview.failure_kind, message: preview.failure_message }) : preview.failure_kind}
                </p>
              ) : null}
              <p>
                {formatMessage(t("settings.manifest.previewSummary"), {
                  bindings: preview.model_binding_change_total_count,
                  bindingsTruncated: preview.model_binding_changes_truncated ? t("settings.manifest.detailsTruncated") : "",
                  operations: preview.operation_change_total_count,
                  operationsTruncated: preview.operation_changes_truncated ? t("settings.manifest.detailsTruncated") : "",
                })}
              </p>
              <div className="vk-mf-lines">
                {preview.model_binding_changes.slice(0, 20).map((delta, index) => {
                  // binding is whichever side owns the concrete identity for this delta.
                  // binding 是此差异中持有具体身份的任一侧。
                  const binding = delta.candidate ?? delta.current
                  return binding ? (
                    <div key={`preview-binding:${binding.service_entry_id}:${binding.credential_profile_id ?? "all"}:${binding.connection_id ?? "static"}:${binding.model_id}:${index}`}>
                      {delta.change} · {binding.service_entry_id}/{binding.model_id} · {binding.connection_id ?? t("settings.manifest.staticScope")} · {delta.current?.effective_protocol ?? "—"} → {delta.candidate?.effective_protocol ?? "—"}
                    </div>
                  ) : null
                })}
                {preview.operation_changes.slice(0, 20).map((delta, index) => {
                  // operation is whichever side owns the semantic identity for this delta.
                  // operation 是此差异中持有语义身份的任一侧。
                  const operation = delta.candidate ?? delta.current
                  return operation ? (
                    <div key={`preview-operation:${operation.operation_id}:${index}`}>
                      {delta.change} · operation {operation.operation_id} · {operation.carrier} · {operation.asynchronous ? t("settings.manifest.asynchronous") : t("settings.manifest.synchronous")}
                    </div>
                  ) : null
                })}
              </div>
            </div>
          ) : null}
          <p className="vk-field-hint">
            {t("settings.manifest.activationNote")}
          </p>
          <div className="vk-mf-providers">
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
                    {formatMessage(t("settings.manifest.counts"), { bindings: status.model_binding_total_count, rules: status.model_rules.length, operations: status.operations.length, templates: status.tool_target_template_instances.length })}
                  </>}
                  key={`${status.provider_id}-rules`}
                  meta={status.active_model_binding_change_baseline
                    ? formatMessage(t("settings.manifest.changeCount"), { count: status.active_model_binding_change_total_count })
                    : undefined}
                  title={status.provider_id}
                >
                <div className="vk-mf-ledger">
                  {status.active_model_binding_change_baseline ? (
                    <p className="vk-field-hint">
                      {formatMessage(t("settings.manifest.changeBaseline"), {
                        layer: activeLayerLabel(status.active_model_binding_change_baseline.layer),
                        revision: status.active_model_binding_change_baseline.revision,
                        count: status.active_model_binding_change_total_count,
                        truncated: status.active_model_binding_changes_truncated ? t("settings.manifest.changeTruncated") : "",
                      })}
                    </p>
                  ) : null}
                  {status.model_rules.length === 0 ? (
                    <p className="vk-field-hint">{t("settings.manifest.noRules")}</p>
                  ) : (
                    status.model_rules.map((rule) => (
                      <div
                        className="vk-mf-entry"
                        data-kind="rule"
                        key={`${rule.service_entry_id}:${rule.rule_id}`}
                      >
                        <div className="vk-mf-entry-head">
                          <b>{rule.matcher.kind}:{rule.matcher.value}</b>
                          <span>{formatMessage(t("settings.manifest.ruleEntry"), { entry: rule.service_entry_id })}</span>
                          <code>{rule.selected_protocol ?? t("settings.manifest.noEffectiveProtocol")}</code>
                          <span>
                            {ruleProvenanceSummary(rule.field_provenance.selected_protocol, t)}
                          </span>
                        </div>
                        <div>
                          {formatMessage(t("settings.manifest.ruleCandidates"), {
                            protocols: rule.supported_protocols.join(", ") || none,
                            defaultState: rule.default_enabled ? t("settings.manifest.defaultEnabled") : t("settings.manifest.defaultDisabled"),
                          })}
                          {rule.model_parameter_ids.length > 0
                            ? formatMessage(t("settings.manifest.ruleParameters"), { ids: rule.model_parameter_ids.join(", ") })
                            : ""}
                        </div>
                        <div>
                          usage [{rule.usage_profile_refs.join(", ") || none}] · request [{rule.request_profile_refs.join(", ") || none}] · response [{rule.response_profile_refs.join(", ") || none}] · operation [{rule.operation_refs.join(", ") || none}]
                        </div>
                        <div>
                          {formatMessage(t("settings.manifest.ruleEvidence"), { status: manifestProvenanceStatusLabel(rule.provenance.provenance_status, t), revision: rule.provenance.manifest_revision })}
                          {rule.provenance.provenance_ref ? ` · ${rule.provenance.provenance_ref}` : t("settings.manifest.inheritsManifestSource")}
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
                        className="vk-mf-entry"
                        data-kind="binding"
                        key={`binding:${manifestModelBindingIdentity(binding)}`}
                      >
                        <div className="vk-mf-entry-head">
                          <b>{binding.display_name}</b>
                          <code>{binding.model_id}</code>
                          <Badge variant="outline">{manifestModelSourceLabel(binding.model_source, t)}</Badge>
                          {activeChange ? (
                            <Badge variant={activeChange === "added" ? "secondary" : "outline"}>
                              {manifestModelBindingChangeLabel(activeChange, t)}
                            </Badge>
                          ) : null}
                          <code>{binding.effective_protocol ?? t("settings.manifest.notExecutable")}</code>
                          <Badge variant={binding.executable ? "secondary" : "destructive"}>
                            {binding.executable ? t("settings.manifest.executable") : t("settings.manifest.blocked")}
                          </Badge>
                        </div>
                        <div>
                          {formatMessage(t("settings.manifest.bindingScope"), {
                            entry: binding.service_entry_id,
                            profile: binding.credential_profile_id ?? t("settings.manifest.allProfiles"),
                            connection: binding.connection_id ?? t("settings.manifest.staticConnection"),
                            supported: binding.supported_protocols.join(", ") || none,
                            available: binding.available_protocols.join(", ") || t("settings.manifest.notObserved"),
                          })}
                        </div>
                        <div>
                          {formatMessage(t("settings.manifest.bindingSource"), {
                            source: binding.binding_source ?? none,
                            rule: binding.binding_rule_id ?? t("settings.manifest.fieldSource.providerDefaults"),
                            revision: binding.manifest_revision,
                            catalog: binding.catalog_observed
                              ? (binding.catalog_enabled ? t("settings.manifest.catalogEnabled") : t("settings.manifest.catalogObservedDisabled"))
                              : t("settings.manifest.notObserved"),
                          })}
                        </div>
                        <div>
                          {formatMessage(t("settings.manifest.capabilities"), {
                            operations: binding.capabilities?.operations?.join(", ") || none,
                            inputs: binding.capabilities?.input_resources?.join(", ") || none,
                            outputs: binding.capabilities?.output_resources?.join(", ") || none,
                            context: binding.capabilities?.context_limit ?? t("settings.manifest.unknown"),
                            output: binding.capabilities?.output_limit ?? t("settings.manifest.unknown"),
                            evidence: binding.capabilities?.evidence_revision ?? none,
                            fields: Object.keys(binding.capability_provenance).length,
                          })}
                        </div>
                        <div>
                          operation [{binding.operation_refs.join(", ") || none}]
                          {binding.unavailable_reason ? ` · ${binding.unavailable_reason}` : ""}
                        </div>
                      </div>
                    )
                  })}
                  {status.model_binding_total_count > Math.min(status.model_bindings.length, 200) ? (
                    <p className="vk-field-hint">
                      {formatMessage(t("settings.manifest.renderedBindings"), {
                        shown: Math.min(status.model_bindings.length, 200),
                        returned: status.model_bindings.length,
                        total: status.model_binding_total_count,
                        truncated: status.model_bindings_truncated ? t("settings.manifest.responseTruncated") : "",
                      })}
                    </p>
                  ) : null}
                  {retiredChanges.length > 0 ? (
                    <div className="vk-mf-entry" data-kind="retired">
                      <b>{t("settings.manifest.retiredModels")}</b>
                      {retiredChanges.map((delta) => {
                        // retiredBinding is guaranteed by the filtered backend delta set above.
                        // retiredBinding 已由上方后端差异过滤保证存在。
                        const retiredBinding = delta.current!
                        return (
                          <div
                            className="vk-mf-entry-head"
                            key={`retired:${manifestModelBindingIdentity(retiredBinding)}`}
                          >
                            <Badge variant="destructive">{manifestModelBindingChangeLabel(delta.change, t)}</Badge>
                            <span>{retiredBinding.display_name}</span>
                            <code>{retiredBinding.model_id}</code>
                            <span>{retiredBinding.service_entry_id}</span>
                            <span>{retiredBinding.effective_protocol ?? t("settings.manifest.notExecutable")}</span>
                          </div>
                        )
                      })}
                    </div>
                  ) : null}
                  {status.operations.map((operation) => (
                    <div
                      className="vk-mf-entry"
                      data-kind="operation"
                      key={`operation:${operation.operation_id}`}
                    >
                      <div>
                        {formatMessage(t("settings.manifest.operationLine"), {
                          id: operation.operation_id,
                          operation: operation.operation,
                          carrier: operation.carrier,
                          mode: operation.asynchronous ? t("settings.manifest.asynchronous") : t("settings.manifest.synchronous"),
                          timeout: operation.timeout_ms ?? t("settings.manifest.undeclared"),
                        })}
                      </div>
                      {operation.asynchronous ? (
                        <div>
                          {formatMessage(t("settings.manifest.operationPolling"), {
                            poll: operation.poll_interval_ms ?? t("settings.manifest.undeclared"),
                            backoff: operation.max_backoff_ms ?? t("settings.manifest.undeclared"),
                            cancel: operation.cancellation_supported ? t("settings.manifest.supported") : t("settings.manifest.unsupported"),
                          })}
                        </div>
                      ) : null}
                      <div>
                        {formatMessage(t("settings.manifest.operationIo"), {
                          inputs: operation.input_resources.join(", ") || none,
                          inputMimes: operation.input_mime_types.join(", ") || none,
                          outputs: operation.output_resources.join(", ") || none,
                          outputMimes: operation.output_mime_types.join(", ") || none,
                        })}
                      </div>
                      <div>
                        {formatMessage(t("settings.manifest.operationRefs"), {
                          uploads: operation.upload_channel_refs.join(", ") || none,
                          tools: operation.tool_target_refs.join(", ") || none,
                          templates: operation.tool_target_template_refs.join(", ") || none,
                          count: operation.matching_model_total_count,
                          truncated: operation.matching_models_truncated ? t("settings.manifest.summaryTruncated") : "",
                        })}
                      </div>
                    </div>
                  ))}
                  {status.tool_target_template_instances.map((instance) => (
                    <div
                      className="vk-mf-entry"
                      data-kind="operation"
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
