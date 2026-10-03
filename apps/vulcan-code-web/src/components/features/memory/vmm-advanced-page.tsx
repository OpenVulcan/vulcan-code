import * as React from "react"
import { Database, Save, Settings2, Stethoscope, Unplug } from "lucide-react"

import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion"
import { StreamPlaceholder } from "@/components/ui/stream-placeholder"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Combobox } from "@/components/ui/combobox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useI18n } from "@/i18n"
import { isOptionalUnsignedDraft } from "./vmm-draft-number"
import type { VmmConfigDraftSummary, VmmMemoryActionResult, VmmMemorySnapshot, VmmNativeProjectCatalogSummary } from "@/types"

// VmmAdvancedPageProps defines global native or external settings and diagnostics.
// VmmAdvancedPageProps 定义全局原生或外置设置与诊断。
interface VmmAdvancedPageProps {
  busy: boolean
  draft: VmmConfigDraftSummary
  snapshot: VmmMemorySnapshot
  notice: string
  onChange: (patch: Partial<VmmConfigDraftSummary>) => void
  onSave: () => Promise<boolean>
  onTestManagement: () => Promise<boolean>
  onRunAction: (
    action: string,
    payload?: Record<string, unknown>,
  ) => Promise<VmmMemoryActionResult | null>
}

// VmmAdvancedPage renders mutually exclusive backend settings without removed sidecar controls.
// VmmAdvancedPage 渲染互斥后端设置且不包含已移除的 Sidecar 控制。
function VmmAdvancedPage({ busy, draft, snapshot, notice, onChange, onSave, onTestManagement, onRunAction }: VmmAdvancedPageProps) {
  const { t } = useI18n()
  const native = draft.backendMode === "native"
  // Only the active external branch submits optional u64 transport fields.
  // 只有当前外链分支提交可选 u64 传输字段。
  const numbersValid = native || [draft.managementTimeoutMs, draft.receiveTimeoutMs, draft.handshakeTimeoutMs, draft.grpcKeepaliveTimeMs, draft.grpcKeepaliveTimeoutMs].every((value) => isOptionalUnsignedDraft(value, 64))

  return (
    <div className="flex w-full min-w-0 flex-col gap-4">
      <div className="vmm-page-toolbar">
        <Button disabled={busy || !numbersValid} onClick={() => void onSave()} type="button"><Save className="size-4" aria-hidden="true" />{t("memory.advanced.save")}</Button>
      </div>

      {notice ? <p className="rounded-lg border bg-muted/30 px-3 py-2 text-sm text-muted-foreground">{notice}</p> : null}
      {!numbersValid ? <p role="alert" className="text-sm text-destructive">{t("memory.advanced.invalidNumbers")}</p> : null}

      <fieldset className="min-w-0" disabled={busy}>
      <Card>
        <CardHeader><CardTitle className="flex items-center gap-2"><Settings2 className="size-4" aria-hidden="true" />{t("memory.advanced.deployment")}</CardTitle><CardDescription>{t("memory.advanced.description")}</CardDescription></CardHeader>
        <CardContent className="space-y-4">
          <RadioGroup disabled={busy} className="grid gap-3 md:grid-cols-2" onValueChange={(value) => onChange({ backendMode: value as VmmConfigDraftSummary["backendMode"] })} value={draft.backendMode}>
            <label className="flex cursor-pointer items-start gap-3 rounded-lg border p-4"><RadioGroupItem value="native" /><span><span className="block text-sm font-medium">{t("memory.advanced.native")}</span><span className="mt-1 block text-xs text-muted-foreground">{t("memory.advanced.nativeDescription")}</span></span></label>
            <label className="flex cursor-pointer items-start gap-3 rounded-lg border p-4"><RadioGroupItem value="external_grpc" /><span><span className="block text-sm font-medium">{t("memory.advanced.external")}</span><span className="mt-1 block text-xs text-muted-foreground">{t("memory.advanced.externalDescription")}</span></span></label>
          </RadioGroup>

          {native ? (
            <div className="grid gap-4 sm:grid-cols-2">
              <BindingModeSelect disabled={busy} label={t("memory.advanced.defaultBinding")} onChange={(nativeProjectBindingMode) => onChange({ nativeProjectBindingMode })} value={draft.nativeProjectBindingMode} />
              {draft.nativeProjectBindingMode === "fixed" ? <NativeFixedProjectSelect disabled={busy} onRunAction={onRunAction} label={t("memory.advanced.nativeFixedProject")} onChange={(nativeFixedProjectId) => onChange({ nativeFixedProjectId })} value={draft.nativeFixedProjectId} /> : null}
            </div>
          ) : (
            <div className="grid gap-4 sm:grid-cols-2">
              <AdvancedTextField label={t("memory.advanced.endpoint")} onChange={(grpcTarget) => onChange({ grpcTarget })} placeholder="127.0.0.1:17625" value={draft.grpcTarget} />
              <AdvancedSecretField label={t("memory.advanced.apiKey")} onChange={(grpcApiKey) => onChange({ grpcApiKey })} value={draft.grpcApiKey} />
              <AdvancedTextField label={t("memory.advanced.managementEndpoint")} onChange={(managementBaseUrl) => onChange({ managementBaseUrl })} placeholder="http://127.0.0.1:17635" value={draft.managementBaseUrl} />
              <AdvancedSecretField label={t("memory.advanced.managementToken")} onChange={(managementBearerToken) => onChange({ managementBearerToken })} value={draft.managementBearerToken} />
              <BindingModeSelect disabled={busy} label={t("memory.advanced.defaultBinding")} onChange={(externalProjectBindingMode) => onChange({ externalProjectBindingMode })} value={draft.externalProjectBindingMode} />
              <AdvancedTextField label={t("memory.advanced.externalPrefix")} onChange={(externalProjectPrefix) => onChange({ externalProjectPrefix })} placeholder="vulcan/space" value={draft.externalProjectPrefix} />
              <div className="flex items-end sm:col-span-2"><Button disabled={busy || !numbersValid} onClick={() => void onTestManagement()} type="button" variant="outline"><Unplug className="size-4" aria-hidden="true" />{t("memory.advanced.testManagement")}</Button></div>
            </div>
          )}
        </CardContent>
      </Card>
      </fieldset>

      {native ? (
        <>
          <Card>
            <CardHeader><CardTitle className="flex items-center gap-2"><Database className="size-4" aria-hidden="true" />{t("memory.advanced.data")}</CardTitle></CardHeader>
            <CardContent><Alert><Database aria-hidden="true" /><AlertTitle>{t("memory.advanced.nativeStorage")}</AlertTitle><AlertDescription>{t("memory.advanced.nativeStorageDescription")}</AlertDescription></Alert></CardContent>
          </Card>
        </>
      ) : null}

      <Accordion className="rounded-xl border px-4" collapsible type="single">
        {!native ? (
          <AccordionItem value="transport">
            <AccordionTrigger>{t("memory.advanced.transport")}</AccordionTrigger>
            <AccordionContent>
            <fieldset disabled={busy} className="grid min-w-0 gap-4 pb-4 sm:grid-cols-2">
              <AdvancedNumberField label={t("memory.advanced.managementTimeout")} onChange={(managementTimeoutMs) => onChange({ managementTimeoutMs })} value={draft.managementTimeoutMs} />
              <AdvancedNumberField label={t("vmm.settings.receiveTimeout")} onChange={(receiveTimeoutMs) => onChange({ receiveTimeoutMs })} value={draft.receiveTimeoutMs} />
              <AdvancedNumberField label={t("vmm.settings.handshakeTimeout")} onChange={(handshakeTimeoutMs) => onChange({ handshakeTimeoutMs })} value={draft.handshakeTimeoutMs} />
              <AdvancedNumberField label={t("vmm.settings.keepaliveInterval")} onChange={(grpcKeepaliveTimeMs) => onChange({ grpcKeepaliveTimeMs })} value={draft.grpcKeepaliveTimeMs} />
              <AdvancedNumberField label={t("vmm.settings.keepaliveTimeout")} onChange={(grpcKeepaliveTimeoutMs) => onChange({ grpcKeepaliveTimeoutMs })} value={draft.grpcKeepaliveTimeoutMs} />
            </fieldset>
            </AccordionContent>
          </AccordionItem>
        ) : null}
        <AccordionItem value="diagnostics">
          <AccordionTrigger><span className="flex items-center gap-2"><Stethoscope className="size-4" aria-hidden="true" />{t("memory.advanced.diagnostics")}</span></AccordionTrigger>
          <AccordionContent className="space-y-2 pb-4 text-sm">
            <DiagnosticRow label={t("memory.advanced.diagnosticBackend")} value={draft.backendMode} />
            <DiagnosticRow label={t("memory.advanced.diagnosticRuntime")} value={snapshot.effective.diagnostic} />
            {!native ? <DiagnosticRow label={t("memory.advanced.diagnosticGrpc")} value={draft.grpcTarget || "-"} /> : null}
          </AccordionContent>
        </AccordionItem>
      </Accordion>
    </div>
  )
}

// NativeFixedProjectSelect waits for the shared channel and renders searchable stable native identities.
// NativeFixedProjectSelect 等待共享通道空闲并渲染可搜索的稳定原生标识。
// Parameters carry shared occupancy, the native action channel, current ID, label, and draft callback.
// 参数提供共享占用状态、原生动作通道、当前标识、标签及草稿回调。
// Returns a bounded picker with explicit loading/retry feedback; reading never rewrites the selection.
// 返回有边界的选择器及明确加载/重试反馈；读取不改写已选标识。
export function NativeFixedProjectSelect({ disabled, label, onChange, onRunAction, value }: {
  disabled: boolean; label: string; onChange: (value: string) => void;
  onRunAction: VmmAdvancedPageProps["onRunAction"]; value: string;
}) {
  const { t } = useI18n()
  // The picker lifetime is scoped to the visible native fixed-binding branch.
  // 选择器生命周期限定于当前可见的原生固定绑定分支。
  const [catalog, setCatalog] = React.useState<VmmNativeProjectCatalogSummary | null>(null)
  const [loading, setLoading] = React.useState(false)
  const [failed, setFailed] = React.useState(false)
  // Refs prevent duplicate reads and ignore completion after leaving this backend branch.
  // 引用用于防止重复读取，并忽略离开此后端分支后的完成结果。
  const mounted = React.useRef(false)
  const started = React.useRef(false)
  const pending = React.useRef(false)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  // One automatic read follows channel availability; failure requires an explicit retry.
  // 通道可用后自动读取一次；失败后仅通过显式重试再次读取。
  const load = React.useCallback(async () => {
    if (disabled || pending.current) return
    pending.current = true
    setLoading(true)
    setFailed(false)
    try {
      const result = await onRunAction("native_project_catalog")
      if (!mounted.current) return
      if (result?.data.kind !== "native_project_catalog") { setFailed(true); return }
      setCatalog(result.data.value)
    } catch {
      if (mounted.current) setFailed(true)
    } finally {
      pending.current = false
      if (mounted.current) setLoading(false)
    }
  }, [disabled, onRunAction])
  React.useEffect(() => {
    if (disabled || started.current) return
    started.current = true
    void load()
  }, [disabled, load])
  // Search presentation names and tags while submitting only the stable native identity.
  // 搜索展示名称与标签，提交时仍仅使用稳定的原生标识。
  const options = catalog === null ? [] : catalog.projects.map((project) => ({
    value: String(project.projectId), label: `${project.name} · ${project.tag}`,
  }))
  return <div className="min-w-0 space-y-2">
    <Label>{label}</Label>
    <Combobox ariaLabel={label} disabled={disabled || loading || catalog === null} options={options}
      placeholder={t("memory.profile.selectProject")} searchPlaceholder={t("memory.profile.searchProject")}
      emptyText={t("memory.profile.noProject")} onValueChange={onChange} value={value} />
    {loading ? <StreamPlaceholder label={t("memory.advanced.catalogLoading")} count={2} /> : null}
    {failed ? <div className="space-y-2">
      <p role="alert" className="text-sm text-destructive">{t("memory.advanced.catalogFailed")}</p>
      <Button type="button" variant="outline" disabled={disabled || loading} onClick={() => void load()}>{t("memory.advanced.catalogRetry")}</Button>
    </div> : null}
  </div>
}

// BindingModeSelect renders the exact fixed or per-project global default.
// BindingModeSelect 渲染准确的固定或逐项目全局默认值。
function BindingModeSelect({ disabled, label, onChange, value }: {
  // Explicit occupancy reaches the portalled menu as well as its trigger.
  // 明确占用状态同时传递到浮层菜单及其触发器。
  disabled: boolean
  label: string
  onChange: (value: "fixed" | "per_project") => void
  value: "fixed" | "per_project"
}) {
  const { t } = useI18n()
  const inputId = React.useId()
  return <div className="min-w-0 space-y-2"><Label htmlFor={inputId}>{label}</Label><Select disabled={disabled} onValueChange={(next) => onChange(next as "fixed" | "per_project")} value={value}><SelectTrigger id={inputId} className="w-full min-w-0"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="fixed">{t("vmm.setup.binding.fixed")}</SelectItem><SelectItem value="per_project">{t("vmm.setup.binding.separate")}</SelectItem></SelectContent></Select></div>
}

// AdvancedNumberField renders one direct positive numeric setting.
// AdvancedNumberField 渲染一个直接的正数设置。
function AdvancedNumberField({ label, onChange, value }: { label: string; onChange: (value: string) => void; value: string }) {
  const inputId = React.useId()
  return <div className="space-y-2"><Label htmlFor={inputId}>{label}</Label><Input id={inputId} inputMode="numeric" aria-invalid={!isOptionalUnsignedDraft(value, 64)} onChange={(event) => onChange(event.target.value)} value={value} /></div>
}

// AdvancedTextField renders one plain backend setting.
// AdvancedTextField 渲染一个普通后端设置。
function AdvancedTextField({ label, onChange, placeholder, value }: { label: string; onChange: (value: string) => void; placeholder?: string; value: string }) {
  const inputId = React.useId()
  return <div className="space-y-2"><Label htmlFor={inputId}>{label}</Label><Input id={inputId} onChange={(event) => onChange(event.target.value)} placeholder={placeholder} value={value} /></div>
}

// AdvancedSecretField renders one masked external credential.
// AdvancedSecretField 渲染一个遮罩的外置凭据。
function AdvancedSecretField({ label, onChange, value }: { label: string; onChange: (value: string) => void; value: string }) {
  const inputId = React.useId()
  return <div className="space-y-2"><Label htmlFor={inputId}>{label}</Label><Input id={inputId} onChange={(event) => onChange(event.target.value)} type="password" value={value} /></div>
}

// DiagnosticRow renders one controlled technical value inside diagnostics.
// DiagnosticRow 在诊断区内渲染一个受控技术值。
function DiagnosticRow({ label, value }: { label: string; value: string }) {
  return <div className="grid gap-1 rounded-lg border bg-muted/20 px-3 py-2 sm:grid-cols-[8rem_minmax(0,1fr)]"><span className="text-muted-foreground">{label}</span><span className="break-all font-mono text-xs">{value}</span></div>
}

export { VmmAdvancedPage }
export type { VmmAdvancedPageProps }
