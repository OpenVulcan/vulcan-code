import * as React from "react"
import { CheckCircle2, CircleAlert, CircleDashed, ExternalLink, LoaderCircle, X } from "lucide-react"

import { localizedServiceEntryPresentation } from "@/components/features/settings/provider-presentation"
import { Button } from "@/ui-kit/primitives/button"
import { useI18n } from "@/i18n"
import { cx } from "@/ui-kit/behavior/core";
import { queryPublicAiConnectionUsage, syncPublicAiConnection } from "@/service/settings-actions-service"
import type {
  PublicAiAccountUsage,
  PublicAiConnection,
  PublicAiProviderDescriptor,
  PublicAiSettingsSnapshot,
} from "@/types"

import { billingLabel, ProviderIntroCard } from "./provider-intro"
import type { SnapshotMutations } from "./use-snapshot-mutations"
import "@/ui-kit/styles/features/settings-forms.css"

// ProviderConnectPanel is the workspace of a provider without accounts: its introduction beside a choice of entry.
// ProviderConnectPanel 是尚无账号的供应商的工作区：介绍与接入方式选择并排。
export function ProviderConnectPanel(props: {
  provider: PublicAiProviderDescriptor
  busy: boolean
  // onConnect opens the credential dialog on the chosen service entry.
  // onConnect 在所选服务入口上打开凭据弹窗。
  onConnect: (entryId: string) => void
}) {
  const { t } = useI18n()
  const { provider } = props
  const [entryId, setEntryId] = React.useState(provider.service_entries[0]?.id ?? "")
  React.useEffect(() => setEntryId(provider.service_entries[0]?.id ?? ""), [provider.id, provider.service_entries])
  return (
    <div className="vk-ms-connect-panel">
      <section className="vk-ms-panel">
        <ProviderIntroCard compact provider={provider} />
      </section>
      <section className="vk-ms-panel">
        <h3>{t("settings.modelServices.connectPanel.entries")}</h3>
        <div className="vk-ms-entries" role="radiogroup" aria-label={t("settings.modelServices.connectPanel.entries")}>
          {provider.service_entries.map((entry) => {
            const presentation = localizedServiceEntryPresentation(provider.id, entry, t)
            const billing = Array.from(new Set(entry.credential_profiles.map((profile) => profile.billing_kind)))
            return (
              <button
                aria-checked={entry.id === entryId}
                className="vk-ms-entry"
                key={entry.id}
                onClick={() => setEntryId(entry.id)}
                role="radio"
                type="button"
              >
                <strong>{presentation.title}</strong>
                <small>{[...presentation.tags, ...billing.map((kind) => billingLabel(kind, t))].join(" · ")}</small>
              </button>
            )
          })}
        </div>
        <p className="vk-ms-panel-hint">{t("settings.modelServices.connectPanel.hint")}</p>
        <Button
          className="vk-form-end"
          disabled={props.busy || !entryId}
          onClick={() => props.onConnect(entryId)}
          type="button"
        >{t("settings.modelServices.connectPanel.connect")}</Button>
      </section>
    </div>
  )
}

// ConnectStepState is the visual state of one connect step.
// ConnectStepState 是一个接入步骤的视觉状态。
type ConnectStepState = "done" | "running" | "warning" | "failed" | "skipped"

// ConnectProgressProps carries the just-added account and the actions its steps offer.
// ConnectProgressProps 承载刚添加的账号及其步骤提供的操作。
interface ConnectProgressProps {
  snapshot: PublicAiSettingsSnapshot
  connection: PublicAiConnection
  usage: PublicAiAccountUsage | undefined
  signupUrl: string | undefined
  mutations: SnapshotMutations
  onUsage: (connectionId: string, usage: PublicAiAccountUsage) => void
  onAdjust: () => void
  onDismiss: () => void
}

// ConnectProgress shows what happened after an account was saved: discovery, default access, usage.
// It reads the saved state only; the one automatic action is a usage read for providers that support it.
// ConnectProgress 显示账号保存之后发生的事情：模型发现、默认访问与用量。
// 它只读取已保存状态；唯一的自动动作是为支持的供应商读取一次用量。
export function ConnectProgress(props: ConnectProgressProps) {
  const { t } = useI18n()
  const { snapshot, connection, mutations } = props
  const usageSupported = connection.usage_capability !== "unsupported"
    && (!connection.usage_requires_management_key || connection.has_management_key)
  // usageRequestedRef keeps the automatic usage read to exactly one attempt per account.
  // usageRequestedRef 保证每个账号的自动用量读取只尝试一次。
  const usageRequestedRef = React.useRef<string | null>(null)
  React.useEffect(() => {
    if (!usageSupported || usageRequestedRef.current === connection.id || mutations.pendingKey !== null) return
    usageRequestedRef.current = connection.id
    void mutations.run(`usage/${connection.id}`, async () => {
      props.onUsage(connection.id, await queryPublicAiConnectionUsage(connection.id))
    })
  }, [connection.id, mutations, props, usageSupported])

  const status = snapshot.catalogStatuses[connection.id]
  const catalogError = snapshot.catalogErrors[connection.id]
  const modelCount = snapshot.modelSupportCatalogs[connection.id]?.models.length ?? status?.model_count ?? 0
  const enabledCount = snapshot.catalogs[connection.id]?.models.length ?? 0
  const discovery: ConnectStepState = catalogError ? "failed"
    : status?.state === "ready" ? "done"
      : status?.state === "not_checked" || status === undefined ? "running"
        : "warning"
  const usage = props.usage ?? snapshot.usageCache[connection.id]
  const usageState: ConnectStepState = !usageSupported ? "skipped"
    : mutations.pendingKey === `usage/${connection.id}` ? "running"
      : usage ? "done" : "warning"

  const steps: { key: string; state: ConnectStepState; title: string; detail?: React.ReactNode; action?: React.ReactNode }[] = [
    {
      key: "saved",
      state: connection.pending_validation ? "warning" : "done",
      title: t("settings.modelServices.connect.saved"),
      detail: connection.pending_validation ? t("settings.modelServices.connect.pendingValidation") : undefined,
    },
    {
      key: "discovery",
      state: discovery,
      title: discovery === "done"
        ? t("settings.modelServices.connect.discovered").replace("{count}", String(modelCount))
        : t("settings.modelServices.connect.discovering"),
      detail: catalogError ?? status?.message,
      action: discovery === "done" ? undefined : (
        <Button
          disabled={mutations.pendingKey !== null}
          onClick={() => void mutations.run(`account/${connection.id}`, () => syncPublicAiConnection(connection.id))}
          size="sm"
          type="button"
          variant="outline"
        >{t("settings.modelServices.connect.retry")}</Button>
      ),
    },
    {
      key: "enabled",
      state: discovery === "done" ? "done" : "skipped",
      title: t("settings.modelServices.connect.enabled").replace("{count}", String(enabledCount)),
      action: discovery === "done" ? (
        <Button onClick={props.onAdjust} size="sm" type="button" variant="outline">{t("settings.modelServices.connect.adjust")}</Button>
      ) : undefined,
    },
    {
      key: "usage",
      state: usageState,
      title: usageSupported ? t("settings.modelServices.connect.usage")
        : connection.usage_capability !== "unsupported" ? t("settings.modelServices.connect.usageNeedsKey")
          : t("settings.modelServices.connect.usageUnsupported"),
      action: connection.usage_capability === "unsupported" && props.signupUrl ? (
        <a className="vk-ms-official-link" href={props.signupUrl} rel="noreferrer" target="_blank">
          <ExternalLink aria-hidden="true" className="vk-icon" />
          {t("settings.modelServices.usage.officialSite")}
        </a>
      ) : undefined,
    },
  ]

  return (
    <section aria-live="polite" className="vk-ms-connect">
      <div className="vk-ms-connect-head">
        <strong>{t("settings.modelServices.connect.title").replace("{account}", connection.display_name)}</strong>
        <Button aria-label={t("settings.modelServices.connect.dismiss")} onClick={props.onDismiss} size="icon-xs" type="button" variant="ghost"><X /></Button>
      </div>
      <ol>
        {steps.map((step) => (
          <li className={cx("vk-ms-connect-step")} data-state={step.state} key={step.key}>
            <ConnectStepIcon state={step.state} />
            <span className="vk-ms-connect-copy">
              <span>{step.title}</span>
              {step.detail ? <small>{step.detail}</small> : null}
            </span>
            {step.action ?? null}
          </li>
        ))}
      </ol>
    </section>
  )
}

// ConnectStepIcon renders the state glyph of one connect step.
// ConnectStepIcon 渲染一个接入步骤的状态图标。
function ConnectStepIcon(props: { state: ConnectStepState }) {
  switch (props.state) {
    case "done": return <CheckCircle2 aria-hidden="true" className="vk-icon" />
    case "running": return <LoaderCircle aria-hidden="true" className="vk-spin" />
    case "warning":
    case "failed": return <CircleAlert aria-hidden="true" className="vk-icon" />
    case "skipped": return <CircleDashed aria-hidden="true" className="vk-icon" />
  }
}
