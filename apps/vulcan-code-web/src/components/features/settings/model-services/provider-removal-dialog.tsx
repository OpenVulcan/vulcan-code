import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog"
import { useI18n } from "@/i18n"
import { removePublicAiCustomProvider } from "@/service/settings-actions-service"
import type { PublicAiProviderDescriptor, PublicAiSettingsSnapshot } from "@/types"

import type { SnapshotMutations } from "./use-snapshot-mutations"
import "@/ui-kit/styles/features/settings-forms.css"

// providerRemovalCustomizationCount counts the provider-owned settings the cascade removal deletes.
// providerRemovalCustomizationCount 统计级联移除会删除的供应商专属设置数量。
function providerRemovalCustomizationCount(snapshot: PublicAiSettingsSnapshot, providerId: string): number {
  const connectionIds = new Set(snapshot.connections
    .filter((connection) => connection.provider_id === providerId)
    .map((connection) => connection.id))
  return [
    Object.keys(snapshot.customModels?.[providerId] ?? {}).length,
    snapshot.customProviderModelDefaults?.[providerId] ? 1 : 0,
    snapshot.customModelTemplateDecisionRecords?.filter((record) => record.providerId === providerId).length ?? 0,
    snapshot.modelAccessRecords.filter((record) => record.providerId === providerId && record.enabled !== record.defaultEnabled).length,
    snapshot.modelCapabilityOverrideRecords?.filter((record) => record.providerId === providerId).length ?? 0,
    snapshot.customModelConnectionAvailabilityRecords?.filter((record) => connectionIds.has(record.connectionId)).length ?? 0,
    snapshot.connectionModelAccessPolicyRecords.filter((record) => (
      connectionIds.has(record.connectionId)
      && (!record.defaultEnabled || Object.keys(record.modelEnabledOverrides).length > 0)
    )).length,
    Object.values(snapshot.inferenceProfiles).reduce((count, profile) => (
      count
      + Object.values(profile.llm_routes ?? {}).filter((route) => route.provider_id === providerId).length
      + Number(profile.embedding?.provider_id === providerId)
      + Number(profile.rerank?.provider_id === providerId)
    ), 0),
  ].reduce((total, count) => total + count, 0)
}

// ProviderRemovalDialog confirms removing one custom provider and discloses every known cascade effect.
// ProviderRemovalDialog 确认移除一个自定义供应商，并披露全部已知的级联影响。
export function ProviderRemovalDialog(props: {
  provider: PublicAiProviderDescriptor | null
  snapshot: PublicAiSettingsSnapshot | null
  mutations: SnapshotMutations
  onClose: () => void
  onRemoved: () => void
}) {
  const { t } = useI18n()
  const { provider, snapshot, mutations } = props
  const busy = mutations.pendingKey !== null
  const lines: string[] = []
  if (provider && snapshot) {
    const connectionCount = snapshot.connections.filter((connection) => connection.provider_id === provider.id).length
    const customizationCount = providerRemovalCustomizationCount(snapshot, provider.id)
    lines.push(t("settings.customProviders.removeDescription").replace("{name}", provider.display_name))
    // Backend-owned settings the browser cannot count still require the generic warning.
    // 浏览器无法计数的后端专属设置仍需显示通用警告。
    if (snapshot.providersWithModelCustomizations?.includes(provider.id)) {
      lines.push(customizationCount > 0
        ? t("settings.customProviders.removeModelsWarning").replace("{count}", String(customizationCount))
        : t("settings.customProviders.removeModelsWarningGeneric"))
    }
    if (connectionCount > 0) {
      lines.push(t("settings.customProviders.removeCredentialsWarning").replace("{count}", String(connectionCount)))
    }
    lines.push(t("settings.customProviders.removeSessionsWarning"))
  }
  return (
    <AlertDialog onOpenChange={(open) => !open && !busy && props.onClose()} open={provider !== null}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("settings.customProviders.removeTitle")}</AlertDialogTitle>
          <AlertDialogDescription>
            <span className="vk-form-lines">
              {lines.map((line) => <span key={line}>{line}</span>)}
            </span>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy}>{t("memory.common.cancel")}</AlertDialogCancel>
          <AlertDialogAction
            disabled={busy || provider === null}
            onClick={(event) => {
              event.preventDefault()
              if (!provider) return
              void mutations.run(`provider/${provider.id}`, () => removePublicAiCustomProvider(provider.id))
                .then((succeeded) => succeeded && props.onRemoved())
            }}
            variant="destructive"
          >
            {t("settings.customProviders.removeTitle")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
