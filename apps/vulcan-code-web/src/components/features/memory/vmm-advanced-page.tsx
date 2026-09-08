import * as React from "react"
import { Database, Save, Settings2, Stethoscope, Unplug } from "lucide-react"

import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useI18n } from "@/i18n"
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
  const [nativeCatalog, setNativeCatalog] = React.useState<VmmNativeProjectCatalogSummary | null>(null)

  React.useEffect(() => {
    if (!native) {
      setNativeCatalog(null)
      return
    }
    let active = true
    void onRunAction("native_project_catalog").then((result) => {
      if (active && result?.data.kind === "native_project_catalog") {
        setNativeCatalog(result.data.value)
      }
    })
    return () => {
      active = false
    }
  }, [native, onRunAction])

  return (
    <div className="flex w-full min-w-0 flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("memory.advanced.title")}</h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">{t("memory.advanced.description")}</p>
        </div>
        <Button disabled={busy} onClick={() => void onSave()} type="button"><Save className="size-4" aria-hidden="true" />{t("memory.advanced.save")}</Button>
      </div>

      {notice ? <p className="rounded-lg border bg-muted/30 px-3 py-2 text-sm text-muted-foreground">{notice}</p> : null}

      <Card>
        <CardHeader><CardTitle className="flex items-center gap-2"><Settings2 className="size-4" aria-hidden="true" />{t("memory.advanced.deployment")}</CardTitle><CardDescription>{t("memory.advanced.description")}</CardDescription></CardHeader>
        <CardContent className="space-y-4">
          <RadioGroup className="grid gap-3 md:grid-cols-2" onValueChange={(value) => onChange({ backendMode: value as VmmConfigDraftSummary["backendMode"] })} value={draft.backendMode}>
            <label className="flex cursor-pointer items-start gap-3 rounded-lg border p-4"><RadioGroupItem value="native" /><span><span className="block text-sm font-medium">{t("memory.advanced.native")}</span><span className="mt-1 block text-xs text-muted-foreground">{t("memory.advanced.nativeDescription")}</span></span></label>
            <label className="flex cursor-pointer items-start gap-3 rounded-lg border p-4"><RadioGroupItem value="external_grpc" /><span><span className="block text-sm font-medium">{t("memory.advanced.external")}</span><span className="mt-1 block text-xs text-muted-foreground">{t("memory.advanced.externalDescription")}</span></span></label>
          </RadioGroup>

          {native ? (
            <div className="grid gap-4 sm:grid-cols-2">
              <BindingModeSelect label={t("memory.advanced.defaultBinding")} onChange={(nativeProjectBindingMode) => onChange({ nativeProjectBindingMode })} value={draft.nativeProjectBindingMode} />
              {draft.nativeProjectBindingMode === "fixed" ? <NativeFixedProjectSelect catalog={nativeCatalog} disabled={busy} label={t("memory.advanced.nativeFixedProject")} onChange={(nativeFixedProjectId) => onChange({ nativeFixedProjectId })} value={draft.nativeFixedProjectId} /> : null}
            </div>
          ) : (
            <div className="grid gap-4 sm:grid-cols-2">
              <AdvancedTextField label={t("memory.advanced.endpoint")} onChange={(grpcTarget) => onChange({ grpcTarget })} placeholder="127.0.0.1:17625" value={draft.grpcTarget} />
              <AdvancedSecretField label={t("memory.advanced.apiKey")} onChange={(grpcApiKey) => onChange({ grpcApiKey })} value={draft.grpcApiKey} />
              <AdvancedTextField label={t("memory.advanced.managementEndpoint")} onChange={(managementBaseUrl) => onChange({ managementBaseUrl })} placeholder="http://127.0.0.1:17635" value={draft.managementBaseUrl} />
              <AdvancedSecretField label={t("memory.advanced.managementToken")} onChange={(managementBearerToken) => onChange({ managementBearerToken })} value={draft.managementBearerToken} />
              <BindingModeSelect label={t("memory.advanced.defaultBinding")} onChange={(externalProjectBindingMode) => onChange({ externalProjectBindingMode })} value={draft.externalProjectBindingMode} />
              <AdvancedTextField label={t("memory.advanced.externalPrefix")} onChange={(externalProjectPrefix) => onChange({ externalProjectPrefix })} placeholder="vulcan/space" value={draft.externalProjectPrefix} />
              <div className="flex items-end sm:col-span-2"><Button disabled={busy} onClick={() => void onTestManagement()} type="button" variant="outline"><Unplug className="size-4" aria-hidden="true" />{t("memory.advanced.testManagement")}</Button></div>
            </div>
          )}
        </CardContent>
      </Card>

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
            <AccordionContent className="grid gap-4 pb-4 sm:grid-cols-2">
              <AdvancedNumberField label={t("memory.advanced.managementTimeout")} onChange={(managementTimeoutMs) => onChange({ managementTimeoutMs })} value={draft.managementTimeoutMs} />
              <AdvancedNumberField label={t("vmm.settings.receiveTimeout")} onChange={(receiveTimeoutMs) => onChange({ receiveTimeoutMs })} value={draft.receiveTimeoutMs} />
              <AdvancedNumberField label={t("vmm.settings.handshakeTimeout")} onChange={(handshakeTimeoutMs) => onChange({ handshakeTimeoutMs })} value={draft.handshakeTimeoutMs} />
              <AdvancedNumberField label={t("vmm.settings.keepaliveInterval")} onChange={(grpcKeepaliveTimeMs) => onChange({ grpcKeepaliveTimeMs })} value={draft.grpcKeepaliveTimeMs} />
              <AdvancedNumberField label={t("vmm.settings.keepaliveTimeout")} onChange={(grpcKeepaliveTimeoutMs) => onChange({ grpcKeepaliveTimeoutMs })} value={draft.grpcKeepaliveTimeoutMs} />
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

// NativeFixedProjectSelect renders stable project IDs as user-facing project names and tags.
// NativeFixedProjectSelect 将稳定项目 ID 渲染为面向用户的项目名称与标签。
function NativeFixedProjectSelect({ catalog, disabled, label, onChange, value }: { catalog: VmmNativeProjectCatalogSummary | null; disabled: boolean; label: string; onChange: (value: string) => void; value: string }) {
  const inputId = React.useId()
  return <div className="space-y-2"><Label htmlFor={inputId}>{label}</Label><Select disabled={disabled || !catalog} onValueChange={onChange} value={value}><SelectTrigger id={inputId}><SelectValue /></SelectTrigger><SelectContent>{catalog?.projects.map((project) => <SelectItem key={project.projectId} value={String(project.projectId)}>{project.name} · {project.tag}</SelectItem>)}</SelectContent></Select></div>
}

// BindingModeSelect renders the exact fixed or per-project global default.
// BindingModeSelect 渲染准确的固定或逐项目全局默认值。
function BindingModeSelect({ label, onChange, value }: {
  label: string
  onChange: (value: "fixed" | "per_project") => void
  value: "fixed" | "per_project"
}) {
  const { t } = useI18n()
  const inputId = React.useId()
  return <div className="space-y-2"><Label htmlFor={inputId}>{label}</Label><Select onValueChange={(next) => onChange(next as "fixed" | "per_project")} value={value}><SelectTrigger id={inputId}><SelectValue /></SelectTrigger><SelectContent><SelectItem value="fixed">{t("vmm.setup.binding.fixed")}</SelectItem><SelectItem value="per_project">{t("vmm.setup.binding.separate")}</SelectItem></SelectContent></Select></div>
}

// AdvancedNumberField renders one direct positive numeric setting.
// AdvancedNumberField 渲染一个直接的正数设置。
function AdvancedNumberField({ label, onChange, value }: { label: string; onChange: (value: string) => void; value: string }) {
  const inputId = React.useId()
  return <div className="space-y-2"><Label htmlFor={inputId}>{label}</Label><Input id={inputId} inputMode="numeric" onChange={(event) => onChange(event.target.value.replace(/\D+/g, ""))} value={value} /></div>
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
