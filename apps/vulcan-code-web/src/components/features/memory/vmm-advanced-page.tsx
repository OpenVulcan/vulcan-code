import * as React from "react"
import { Database, Save, Settings2, ShieldAlert, Stethoscope } from "lucide-react"

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { useI18n } from "@/i18n"
import type { TranslationKey } from "@/i18n-messages"
import type {
  VmmConfigDraftSummary,
  VmmMaintenanceSummary,
  VmmMemorySnapshot,
} from "@/types"

import { VmmMaintenanceDialog } from "./vmm-maintenance-dialog"

// VmmAdvancedPageProps defines deployment settings, diagnostics, and data recovery actions.
// VmmAdvancedPageProps 定义部署设置、诊断与数据恢复操作。
interface VmmAdvancedPageProps {
  busy: boolean
  draft: VmmConfigDraftSummary
  snapshot: VmmMemorySnapshot
  notice: string
  onChange: (patch: Partial<VmmConfigDraftSummary>) => void
  onSave: () => Promise<boolean>
  onMaintenance: (
    action: "managed_maintenance_start" | "managed_maintenance_status" | "managed_maintenance_cancel",
  ) => Promise<VmmMaintenanceSummary | null>
  onStartRuntime: () => Promise<boolean>
}

// VmmAdvancedPage renders progressive disclosure for deployment and technical details.
// VmmAdvancedPage 渲染部署与技术详情的渐进式披露界面。
function VmmAdvancedPage({
  busy,
  draft,
  snapshot,
  notice,
  onChange,
  onSave,
  onMaintenance,
  onStartRuntime,
}: VmmAdvancedPageProps) {
  const { t } = useI18n()
  const dataBlocked = [
    "unmarked_history",
    "fingerprint_changed",
    "marker_corrupt",
    "marker_version_unsupported",
    "schema_migration_required",
  ].includes(snapshot.dataSpaceState.state)

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("memory.advanced.title")}</h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
            {t("memory.advanced.description")}
          </p>
        </div>
        <Button disabled={busy} onClick={() => void onSave()} type="button">
          <Save className="size-4" aria-hidden="true" />
          {t("memory.advanced.save")}
        </Button>
      </div>

      {notice ? (
        <p className="rounded-lg border bg-muted/30 px-3 py-2 text-sm text-muted-foreground">
          {notice}
        </p>
      ) : null}

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Settings2 className="size-4" aria-hidden="true" />
            {t("memory.advanced.deployment")}
          </CardTitle>
          <CardDescription>{t("memory.advanced.description")}</CardDescription>
        </CardHeader>
        <CardContent>
          <RadioGroup
            className="grid gap-3 md:grid-cols-2"
            onValueChange={(value) => onChange({
              managedInstallEnabled: value === "managed",
            })}
            value={draft.managedInstallEnabled ? "managed" : "external"}
          >
            <label className="flex cursor-pointer items-start gap-3 rounded-lg border p-4">
              <RadioGroupItem value="managed" />
              <span>
                <span className="block text-sm font-medium">{t("memory.advanced.managed")}</span>
                <span className="mt-1 block text-xs text-muted-foreground">
                  {t("memory.advanced.managedDescription")}
                </span>
              </span>
            </label>
            <label className="flex cursor-pointer items-start gap-3 rounded-lg border p-4">
              <RadioGroupItem value="external" />
              <span>
                <span className="block text-sm font-medium">{t("memory.advanced.external")}</span>
                <span className="mt-1 block text-xs text-muted-foreground">
                  {t("memory.advanced.externalDescription")}
                </span>
              </span>
            </label>
          </RadioGroup>

          {draft.managedInstallEnabled ? (
            <div className="mt-4 max-w-sm space-y-2">
              <Label>{t("memory.advanced.runtime")}</Label>
              <Select
                onValueChange={(autoStartPolicy) => onChange({ autoStartPolicy })}
                value={draft.autoStartPolicy || "on_start"}
              >
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="on_start">{t("memory.advanced.autoStart")}</SelectItem>
                  <SelectItem value="manual">{t("memory.advanced.manualStart")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
          ) : (
            <div className="mt-4 grid gap-4 sm:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="vmm-external-endpoint">{t("memory.advanced.endpoint")}</Label>
                <Input
                  id="vmm-external-endpoint"
                  onChange={(event) => onChange({ grpcTarget: event.target.value })}
                  placeholder="127.0.0.1:17625"
                  value={draft.grpcTarget}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="vmm-external-key">{t("memory.advanced.apiKey")}</Label>
                <Input
                  id="vmm-external-key"
                  onChange={(event) => onChange({ grpcApiKey: event.target.value })}
                  type="password"
                  value={draft.grpcApiKey}
                />
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Database className="size-4" aria-hidden="true" />
            {t("memory.advanced.data")}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Alert variant={dataBlocked ? "destructive" : "default"}>
            {dataBlocked ? <ShieldAlert aria-hidden="true" /> : <Database aria-hidden="true" />}
            <AlertTitle>{t(snapshot.dataSpaceState.titleKey as TranslationKey)}</AlertTitle>
            <AlertDescription className="space-y-3">
              <p>{t(snapshot.dataSpaceState.descriptionKey as TranslationKey)}</p>
              {dataBlocked ? (
                <VmmMaintenanceDialog
                  busy={busy}
                  maintenance={snapshot.maintenance}
                  onRun={onMaintenance}
                  onStartRuntime={onStartRuntime}
                />
              ) : null}
            </AlertDescription>
          </Alert>
        </CardContent>
      </Card>

      <Accordion className="rounded-xl border px-4" collapsible type="single">
        <AccordionItem value="transport">
          <AccordionTrigger>{t("memory.advanced.transport")}</AccordionTrigger>
          <AccordionContent className="grid gap-4 pb-4 sm:grid-cols-2">
            <AdvancedNumberField
              label={t("vmm.settings.receiveTimeout")}
              onChange={(receiveTimeoutMs) => onChange({ receiveTimeoutMs })}
              value={draft.receiveTimeoutMs}
            />
            <AdvancedNumberField
              label={t("vmm.settings.handshakeTimeout")}
              onChange={(handshakeTimeoutMs) => onChange({ handshakeTimeoutMs })}
              value={draft.handshakeTimeoutMs}
            />
            <AdvancedNumberField
              label={t("vmm.settings.keepaliveInterval")}
              onChange={(grpcKeepaliveTimeMs) => onChange({ grpcKeepaliveTimeMs })}
              value={draft.grpcKeepaliveTimeMs}
            />
            <AdvancedNumberField
              label={t("vmm.settings.keepaliveTimeout")}
              onChange={(grpcKeepaliveTimeoutMs) => onChange({ grpcKeepaliveTimeoutMs })}
              value={draft.grpcKeepaliveTimeoutMs}
            />
          </AccordionContent>
        </AccordionItem>
        <AccordionItem value="diagnostics">
          <AccordionTrigger>
            <span className="flex items-center gap-2">
              <Stethoscope className="size-4" aria-hidden="true" />
              {t("memory.advanced.diagnostics")}
            </span>
          </AccordionTrigger>
          <AccordionContent className="space-y-2 pb-4 text-sm">
            <DiagnosticRow
              label={t("memory.advanced.diagnosticRuntime")}
              value={snapshot.runtime.managedPhase}
            />
            <DiagnosticRow
              label={t("memory.advanced.diagnosticGrpc")}
              value={snapshot.runtime.managedGrpcTarget ?? "-"}
            />
            <DiagnosticRow
              label={t("memory.advanced.diagnosticController")}
              value={snapshot.runtime.managedControllerEndpoint || "-"}
            />
            <DiagnosticRow
              label={t("memory.advanced.diagnosticData")}
              value={snapshot.runtime.managedDataRoot || "-"}
            />
            <DiagnosticRow
              label={t("memory.advanced.diagnosticLog")}
              value={snapshot.runtime.managedLogPath || "-"}
            />
            {snapshot.runtime.managedLastError ? (
              <p className="whitespace-pre-wrap rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-destructive">
                {snapshot.runtime.managedLastError}
              </p>
            ) : null}
          </AccordionContent>
        </AccordionItem>
      </Accordion>
    </div>
  )
}

// AdvancedNumberField renders one direct numeric transport setting.
// AdvancedNumberField 渲染一个直接数值传输设置。
function AdvancedNumberField({
  label,
  onChange,
  value,
}: {
  label: string
  onChange: (value: string) => void
  value: string
}) {
  const inputId = React.useId()
  return (
    <div className="space-y-2">
      <Label htmlFor={inputId}>{label}</Label>
      <Input
        id={inputId}
        inputMode="numeric"
        onChange={(event) => onChange(event.target.value.replace(/\D+/g, ""))}
        value={value}
      />
    </div>
  )
}

// DiagnosticRow renders one technical value only inside the collapsed diagnostics section.
// DiagnosticRow 仅在折叠诊断区内渲染一个技术值。
function DiagnosticRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid gap-1 rounded-lg border bg-muted/20 px-3 py-2 sm:grid-cols-[8rem_minmax(0,1fr)]">
      <span className="text-muted-foreground">{label}</span>
      <span className="break-all font-mono text-xs">{value}</span>
    </div>
  )
}

export { VmmAdvancedPage }
export type { VmmAdvancedPageProps }
