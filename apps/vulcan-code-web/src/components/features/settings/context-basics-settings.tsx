import * as React from "react"
import { LoaderCircle } from "lucide-react"

import { ContextWindowPolicyEditor } from "@/components/features/settings/public-ai-model-capability-dialog"
import { ProviderRetrySettingsArea } from "@/components/features/settings/provider-retry-settings"
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useI18n } from "@/i18n"
import {
  loadPublicAiSettings,
  savePublicAiContextCompactionWindowNodes,
  savePublicAiContextWindowPolicyOverrides,
  savePublicAiUsageRefreshSettings,
} from "@/service/settings-actions-service"
import type {
  ContextCompactionWindowNodeId,
  PublicAiContextWindowPolicyOverrides,
  PublicAiContextCompactionWindowConfiguration,
  PublicAiSettingsSnapshot,
  ProviderRetrySettingsSummary,
} from "@/types"

// CONTEXT_COMPACTION_NODE_LABEL_KEYS binds every fixed node identity to one localized label.
// CONTEXT_COMPACTION_NODE_LABEL_KEYS 将每个固定节点身份绑定到一个本地化标签。
const CONTEXT_COMPACTION_NODE_LABEL_KEYS: Record<
  ContextCompactionWindowNodeId,
  | "settings.contextCompactionNodes.node1"
  | "settings.contextCompactionNodes.node2"
  | "settings.contextCompactionNodes.node3"
  | "settings.contextCompactionNodes.node4"
  | "settings.contextCompactionNodes.node5"
> = {
  "compression-window-1": "settings.contextCompactionNodes.node1",
  "compression-window-2": "settings.contextCompactionNodes.node2",
  "compression-window-3": "settings.contextCompactionNodes.node3",
  "compression-window-4": "settings.contextCompactionNodes.node4",
  "compression-window-5": "settings.contextCompactionNodes.node5",
}

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

// cloneContextCompactionWindowConfiguration creates an editable copy of the exact five-node tuple.
// cloneContextCompactionWindowConfiguration 创建恰好五节点元组的可编辑副本。
function cloneContextCompactionWindowConfiguration(
  configuration: PublicAiContextCompactionWindowConfiguration,
): PublicAiContextCompactionWindowConfiguration {
  return {
    nodes: [
      { ...configuration.nodes[0] },
      { ...configuration.nodes[1] },
      { ...configuration.nodes[2] },
      { ...configuration.nodes[3] },
      { ...configuration.nodes[4] },
    ],
  }
}

// ContextCompactionWindowNodesCard edits values while keeping node identity, count, and order immutable.
// ContextCompactionWindowNodesCard 编辑节点值，同时保持节点身份、数量与顺序不可变。
function ContextCompactionWindowNodesCard(props: {
  // nodes is the latest backend-authoritative complete node configuration.
  // nodes 是后台最新权威完整节点配置。
  nodes: PublicAiContextCompactionWindowConfiguration
  // onSaved receives the authoritative settings snapshot after a successful save.
  // onSaved 在保存成功后接收权威设置快照。
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
}) {
  const { t } = useI18n()
  // draft is the complete five-node edit buffer; identities are never editable.
  // draft 是完整五节点编辑缓冲；节点身份始终不可编辑。
  const [draft, setDraft] = React.useState(() => cloneContextCompactionWindowConfiguration(props.nodes))
  // saving prevents overlapping global node configuration writes.
  // saving 防止全局节点配置写入重叠执行。
  const [saving, setSaving] = React.useState(false)
  // error contains one actionable validation or persistence failure.
  // error 保存一条可操作的校验或持久化失败信息。
  const [error, setError] = React.useState<string | null>(null)

  React.useEffect(() => {
    setDraft(cloneContextCompactionWindowConfiguration(props.nodes))
  }, [props.nodes])

  // validationError mirrors backend positive-safe-integer and strict-ascending constraints.
  // validationError 镜像后台正安全整数与严格递增约束。
  const validationError = React.useMemo(() => {
    if (draft.nodes.some((node) => !Number.isSafeInteger(node.tokens) || node.tokens <= 0)) {
      return t("settings.contextCompactionNodes.invalidPositive")
    }
    if (draft.nodes.some((node, index) => index > 0 && draft.nodes[index - 1].tokens >= node.tokens)) {
      return t("settings.contextCompactionNodes.invalidAscending")
    }
    return null
  }, [draft.nodes, t])

  // updateNodeTokens changes only one token value at its immutable tuple index.
  // updateNodeTokens 仅修改不可变元组索引处的一个 Token 值。
  const updateNodeTokens = React.useCallback((index: number, tokens: number) => {
    setDraft((current) => {
      const next = cloneContextCompactionWindowConfiguration(current)
      next.nodes[index] = { ...next.nodes[index], tokens }
      return next
    })
  }, [])

  // save submits all five nodes atomically and adopts the returned authoritative snapshot.
  // save 原子提交全部五个节点并采用返回的权威快照。
  const save = React.useCallback(async () => {
    if (validationError) {
      setError(validationError)
      return
    }
    setSaving(true)
    setError(null)
    try {
      props.onSaved(await savePublicAiContextCompactionWindowNodes(draft))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setSaving(false)
    }
  }, [draft, props, validationError])

  return (
    <section className="space-y-4 rounded-xl border p-4">
      <div>
        <h3 className="text-sm font-semibold">{t("settings.contextCompactionNodes.title")}</h3>
        <p className="text-xs text-muted-foreground">{t("settings.contextCompactionNodes.description")}</p>
      </div>
      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-5">
        {draft.nodes.map((node, index) => (
          <label className="grid gap-1.5 text-sm" key={node.id}>
            <span className="font-medium">{t(CONTEXT_COMPACTION_NODE_LABEL_KEYS[node.id])}</span>
            <Input
              disabled={saving}
              inputMode="numeric"
              min={1}
              onChange={(event) => updateNodeTokens(index, Number.parseInt(event.target.value.replace(/\D/g, "") || "0", 10))}
              step={1000}
              type="number"
              value={node.tokens}
            />
            <span className="font-mono text-[11px] text-muted-foreground">{node.id}</span>
          </label>
        ))}
      </div>
      <p className="text-xs text-amber-600 dark:text-amber-400">{t("settings.contextCompactionNodes.warning")}</p>
      {validationError || error ? <p className="text-sm text-destructive">{error ?? validationError}</p> : null}
      <Button disabled={saving || validationError !== null} onClick={() => void save()} type="button">
        {saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
        {t("settings.contextCompactionNodes.save")}
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
          <ContextCompactionWindowNodesCard
            nodes={snapshot.contextCompactionWindowNodes}
            onSaved={setSnapshot}
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
