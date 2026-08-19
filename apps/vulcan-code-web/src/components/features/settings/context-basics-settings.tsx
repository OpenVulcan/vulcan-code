import * as React from "react"
import { LoaderCircle } from "lucide-react"

import { ContextWindowPolicyEditor } from "@/components/features/settings/public-ai-model-capability-dialog"
import { ProviderRetrySettingsArea } from "@/components/features/settings/provider-retry-settings"
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import { Button } from "@/components/ui/button"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useI18n } from "@/i18n"
import {
  loadPublicAiSettings,
  savePublicAiContextWindowPolicyOverrides,
  savePublicAiUsageRefreshSettings,
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

// AccountUsageRefreshCard edits the persisted background account-usage cadence.
// AccountUsageRefreshCard 编辑持久化的后台账号用量刷新周期。
function AccountUsageRefreshCard(props: {
  // onSaved receives the authoritative post-save settings snapshot.
  // onSaved 接收保存后的权威设置快照。
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  // intervalSeconds is the currently persisted refresh cadence.
  // intervalSeconds 是当前持久化刷新周期。
  intervalSeconds: number
}) {
  // saving prevents overlapping interval mutations.
  // saving 防止刷新周期变更重叠执行。
  const [saving, setSaving] = React.useState(false)
  // error contains one actionable interval save failure.
  // error 保存一条可操作的刷新周期保存失败。
  const [error, setError] = React.useState<string | null>(null)
  // save persists one selected interval and publishes the resulting settings snapshot.
  // save 持久化一个选定周期并发布生成的设置快照。
  const save = React.useCallback(async (intervalSeconds: number) => {
    setSaving(true)
    setError(null)
    try {
      props.onSaved(await savePublicAiUsageRefreshSettings(intervalSeconds))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setSaving(false)
    }
  }, [props])
  return (
    <section className="space-y-3 rounded-xl border p-4">
      <div>
        <h3 className="text-sm font-semibold">账号用量后台刷新</h3>
        <p className="text-xs text-muted-foreground">
          仅刷新声明支持用量查询且已具备必要凭据的账号；默认每 30 分钟刷新，缓存只保存在内存中。
        </p>
      </div>
      <Select
        disabled={saving}
        onValueChange={(value) => void save(Number(value))}
        value={String(props.intervalSeconds)}
      >
        <SelectTrigger className="w-48"><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value="60">每 1 分钟</SelectItem>
          <SelectItem value="300">每 5 分钟</SelectItem>
          <SelectItem value="900">每 15 分钟</SelectItem>
          <SelectItem value="1800">每 30 分钟</SelectItem>
          <SelectItem value="3600">每 1 小时</SelectItem>
        </SelectContent>
      </Select>
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
    </section>
  )
}

// ContextBasicsSettingsArea groups only model-context and retry policies under the model settings node.
// ContextBasicsSettingsArea 仅在模型设置基础节点中组织模型上下文与重试策略。
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
        <>
          <SystemContextWindowPolicyCard
            onSaved={setSnapshot}
            policy={snapshot.contextWindowPolicyOverrides}
          />
          <AccountUsageRefreshCard
            intervalSeconds={snapshot.usageRefreshIntervalSeconds}
            onSaved={setSnapshot}
          />
        </>
      ) : (
        <p className="settings-muted-line">{t("settings.contextBasics.loading")}</p>
      )}
      <ProviderRetrySettingsArea actions={props.actions} embedded settings={props.retrySettings} />
    </div>
  )
}
