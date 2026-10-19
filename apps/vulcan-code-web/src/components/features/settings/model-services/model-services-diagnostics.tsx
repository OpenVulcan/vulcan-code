import * as React from "react"
import { Trash2 } from "lucide-react"

import { ProviderManifestStatusPanel } from "@/components/features/settings/provider-manifest-status"
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
import { Button } from "@/ui-kit/primitives/button"
import { useI18n } from "@/i18n"
import { removePublicAiCustomProvider } from "@/service/settings-actions-service"
import type { PublicAiSettingsSnapshot } from "@/types"

import type { SnapshotMutations } from "./use-snapshot-mutations"

// ModelServicesDiagnostics holds the maintenance views: manifest status and isolated custom providers.
// ModelServicesDiagnostics 承载维护视图：清单状态与被隔离的自定义供应商。
export function ModelServicesDiagnostics(props: {
  snapshot: PublicAiSettingsSnapshot | null
  mutations: SnapshotMutations
}) {
  const { t } = useI18n()
  const { snapshot, mutations } = props
  // removing is the exact failed provider reference awaiting confirmation; no descriptor is fabricated.
  // removing 是等待确认的精确失败供应商引用；不伪造描述。
  const [removing, setRemoving] = React.useState<string | null>(null)
  const failures = snapshot?.customProviderManifestFailures ?? []
  // The total stays truthful when the backend truncated the detail list.
  // 后端截断详情列表时，总数仍保持真实。
  const failureTotal = Math.max(snapshot?.customProviderManifestFailureTotalCount ?? 0, failures.length)
  const busy = mutations.pendingKey !== null

  return (
    <div className="model-services-diagnostics">
      {failureTotal > 0 ? (
        <section className="model-services-isolated" role="alert">
          <p className="font-semibold text-destructive">
            {t("settings.providers.isolatedCount").replace("{count}", String(failureTotal))}
          </p>
          <p className="text-muted-foreground">{t("settings.providers.isolatedDescription")}</p>
          <ul>
            {failures.map((failure, index) => (
              <li key={`${failure.providerId}:${index}`}>
                <span className="min-w-0 flex-1 break-all font-mono">{failure.providerId}: {failure.message}</span>
                <Button disabled={busy} onClick={() => setRemoving(failure.providerId)} size="sm" type="button" variant="destructive">
                  <Trash2 aria-hidden="true" />
                  {t("settings.providers.removeIsolated")}
                </Button>
              </li>
            ))}
          </ul>
          {snapshot?.customProviderManifestFailuresTruncated ? (
            <p className="text-xs text-muted-foreground">{t("settings.providers.diagnosticsOmitted")}</p>
          ) : null}
        </section>
      ) : null}
      <ProviderManifestStatusPanel />
      <AlertDialog onOpenChange={(open) => !open && !busy && setRemoving(null)} open={removing !== null}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.providers.removeIsolatedTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              <span className="block space-y-2">
                <span className="block">{t("settings.providers.removeIsolatedDescription")}</span>
                <span className="block break-all font-mono text-foreground">{removing}</span>
              </span>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>{t("memory.common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={busy || removing === null}
              onClick={(event) => {
                event.preventDefault()
                if (!removing) return
                const providerId = removing
                void mutations.run(`provider/${providerId}`, () => removePublicAiCustomProvider(providerId))
                  .then((succeeded) => succeeded && setRemoving(null))
              }}
              variant="destructive"
            >
              {t("settings.providers.permanentlyDelete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
