import * as React from "react"
import { LoaderCircle } from "lucide-react"

import { ContextWindowPolicyEditor } from "@/components/features/settings/public-ai-model-capability-dialog"
import { ProviderRetrySettingsArea } from "@/components/features/settings/provider-retry-settings"
import { ProviderUsageSettingsArea } from "@/components/features/settings/provider-usage-settings"
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import { Button } from "@/components/ui/button"
import { useI18n } from "@/i18n"
import {
  loadPublicAiSettings,
  savePublicAiContextWindowPolicyOverrides,
} from "@/service/settings-actions-service"
import type {
  PublicAiContextWindowPolicyOverrides,
  PublicAiSettingsSnapshot,
  ProviderRetrySettingsSummary,
} from "@/types"

// SystemContextWindowPolicyCard edits the process-wide context-window policy.
// SystemContextWindowPolicyCard 编辑进程级上下文窗口策略。
function SystemContextWindowPolicyCard(props: {
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  policy: PublicAiContextWindowPolicyOverrides
}) {
  const { t } = useI18n()
  // draft is the current system-level sparse policy being edited.
  // draft 是当前正在编辑的系统级稀疏策略。
  const [draft, setDraft] = React.useState<PublicAiContextWindowPolicyOverrides | undefined>(props.policy)
  // saving prevents overlapping policy mutations from the same settings card.
  // saving 防止同一卡片发起重叠策略变更。
  const [saving, setSaving] = React.useState(false)
  // error contains one actionable system-policy save failure.
  // error 保存一条可操作的系统策略保存失败信息。
  const [error, setError] = React.useState<string | null>(null)

  React.useEffect(() => {
    setDraft(Object.keys(props.policy).length > 0 ? props.policy : undefined)
  }, [props.policy])

  // save persists the sparse system layer and updates the authoritative snapshot.
  // save 持久化系统稀疏层并更新权威设置快照。
  const save = React.useCallback(async () => {
    setSaving(true)
    setError(null)
    try {
      props.onSaved(await savePublicAiContextWindowPolicyOverrides(draft ?? {}))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setSaving(false)
    }
  }, [draft, props.onSaved])

  return (
    <section className="space-y-3 rounded-xl border p-4">
      <div>
        <h3 className="text-sm font-semibold">{t("settings.contextBasics.title")}</h3>
        <p className="text-xs text-muted-foreground">{t("settings.contextWindowPolicy.description")}</p>
      </div>
      <ContextWindowPolicyEditor onChange={setDraft} value={draft} />
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      <Button disabled={saving} onClick={() => void save()} type="button">
        {saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
        {t("settings.contextBasics.saveSystemDefault")}
      </Button>
    </section>
  )
}

// ContextBasicsSettingsArea groups context, retry, and usage settings under the model settings node.
// ContextBasicsSettingsArea 将上下文、重试和用量设置统一放入模型设置基础节点。
export function ContextBasicsSettingsArea(props: {
  // Service-backed settings actions used by request retry settings.
  // 请求重试设置使用的服务端设置操作。
  actions: SettingsActionProps
  // Workbench-projected request retry settings.
  // 工作台投影出的请求重试设置。
  retrySettings?: ProviderRetrySettingsSummary
}) {
  const { t } = useI18n()
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  const [error, setError] = React.useState<string | null>(null)

  // refresh loads the authoritative system context policy before rendering its editor.
  // refresh 在渲染编辑器前加载权威系统上下文策略。
  const refresh = React.useCallback(async () => {
    setError(null)
    try {
      setSnapshot(await loadPublicAiSettings())
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }, [])

  React.useEffect(() => {
    void refresh()
  }, [refresh])

  return (
    <div className="settings-direct-content context-basics-surface space-y-6 px-4 lg:px-6">
      <div>
        <h2 className="text-xl font-semibold">{t("settings.contextBasics.title")}</h2>
        <p className="text-sm text-muted-foreground">{t("settings.contextBasics.description")}</p>
      </div>
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      {snapshot ? (
        <SystemContextWindowPolicyCard
          onSaved={setSnapshot}
          policy={snapshot.contextWindowPolicyOverrides}
        />
      ) : (
        <p className="settings-muted-line">{t("settings.contextBasics.loading")}</p>
      )}
      <ProviderRetrySettingsArea actions={props.actions} embedded settings={props.retrySettings} />
      <ProviderUsageSettingsArea />
    </div>
  )
}
