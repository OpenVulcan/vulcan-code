import { DirectoryRow, WorkspaceDirectoryLayout, WorkspaceDirectoryMenu } from "@/components/workspace/workspace-directory-layout";
import * as React from "react"
import { ArrowUpRight, LoaderCircle, RotateCcw } from "lucide-react"
import { toast } from "@/ui-kit/primitives/toast"

import { PageActionBar } from "@/components/page"
import { ContextWindowPolicyEditor } from "@/components/features/settings/context-window-policy-editor"
import { ProviderRetrySettingsArea } from "@/components/features/settings/provider-retry-settings"
import { MediaDeliveryPolicyCard } from "@/components/features/settings/media-delivery-policy-card"
import { TerminalShellSettingsCard } from "@/components/features/settings/terminal-shell-settings"
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import { Button } from "@/ui-kit/primitives/button"
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice"
import { Input } from "@/ui-kit/primitives/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { useI18n } from "@/i18n"
import { formatTokenCount, tokenUnitLabel } from "@/lib/usage-format"
import {
  loadPublicAiSettings,
  savePublicAiContextCompactionWindowNodes,
  savePublicAiNewSessionContextWindowDefault,
  savePublicAiContextWindowPolicyOverrides,
  savePublicAiUsageRefreshSettings,
} from "@/service/settings-actions-service"
import type {
  CommandShellSettingsSummary,
  ContextCompactionWindowNodeId,
  PublicAiContextWindowPolicy,
  PublicAiContextWindowPolicyOverrides,
  PublicAiContextCompactionWindowConfiguration,
  PublicAiSettingsSnapshot,
  ProviderRetrySettingsSummary,
} from "@/types"
import "@/ui-kit/styles/features/settings-forms.css"

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

// ContextBasicsHeader titles one settings concern with its explanation and optional section command.
// ContextBasicsHeader 为一个设置事项提供标题、说明与可选的分区命令。
function ContextBasicsHeader(props: { title: string; children: React.ReactNode; action?: React.ReactNode }) {
  return (
    <header className="context-basics-head">
      <div>
        <h3>{props.title}</h3>
        {props.children}
      </div>
      {props.action}
    </header>
  )
}

// SystemContextWindowPolicyCard edits the process-wide context-window policy.
// SystemContextWindowPolicyCard 编辑进程级上下文窗口策略。
// Parameters: props supplies the policy snapshot and save result callback; returns the independent context policy draft editor.
// 参数：props 提供策略快照及保存结果回调；返回独立的上下文策略草稿编辑器。
function SystemContextWindowPolicyCard(props: {
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  policy: PublicAiContextWindowPolicyOverrides
  // Built-in values shown in every field nobody overrides.
  // 无人覆盖的字段中显示的内置值。
  defaults: PublicAiContextWindowPolicy
}) {
  const { t } = useI18n()
  // draft is the current system-level sparse policy being edited.
  // draft 是当前正在编辑的系统级稀疏策略。
  const [draft, setDraft] = React.useState<PublicAiContextWindowPolicyOverrides | undefined>(props.policy)
  // saving prevents overlapping policy mutations from the same settings card.
  // saving 防止同一卡片发起重叠策略变更。
  const [saving, setSaving] = React.useState(false)
  // Lock synchronously before awaiting this independent section write.
  // 等待独立分区写入前同步加锁。
  const savingRef = React.useRef(false)
  // error contains one actionable system-policy save failure.
  // error 保存一条可操作的系统策略保存失败信息。
  const [error, setError] = React.useState<string | null>(null)
  // policyKey avoids discarding another section's draft when a full snapshot repeats the same policy.
  // policyKey 避免完整快照重复相同策略时丢弃另一分区的草稿。
  const policyKey = JSON.stringify(props.policy)
  // dirty distinguishes the editable buffer from the saved system layer.
  // dirty 区分可编辑缓冲与已保存系统层。
  const dirty = JSON.stringify(draft ?? {}) !== policyKey

  React.useEffect(() => {
    setDraft(Object.keys(props.policy).length > 0 ? props.policy : undefined)
  }, [policyKey])

  // save persists the sparse system layer and updates the authoritative snapshot.
  // save 持久化系统稀疏层并更新权威设置快照。
  const save = React.useCallback(async () => {
    if (savingRef.current) return
    savingRef.current = true
    setSaving(true)
    setError(null)
    try {
      props.onSaved(await savePublicAiContextWindowPolicyOverrides(draft ?? {}))
      toast.success(t("settings.modelExperience.savedSystem"))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      savingRef.current = false
      setSaving(false)
    }
  }, [draft, props.onSaved, t])

  return (
    <section className="context-basics-section">
      <ContextBasicsHeader title={t("settings.modelExperience.context")} action={
        <Button disabled={saving || !draft} onClick={() => setDraft(undefined)} size="sm" type="button" variant="ghost">
          <RotateCcw aria-hidden="true" />{t("settings.contextWindowPolicy.clear")}
        </Button>
      }>
        <p>{t("settings.modelExperience.scopeHint")}</p>
        <p className="vk-form-note">{t("settings.contextWindowPolicy.description")}</p>
      </ContextBasicsHeader>
      <ContextWindowPolicyEditor defaults={props.defaults} disabled={saving} onChange={setDraft} value={draft} inheritanceHints={{
        effective_context_window_percent: t("settings.modelExperience.inheritWindow"),
        auto_compact_percent: t("settings.modelExperience.inheritCompact"),
        auto_compact_token_limit: t("settings.modelExperience.inheritToken"),
        auto_compact_scope: t("settings.modelExperience.inheritScope"),
        fallback_buffer_tokens: t("settings.modelExperience.inheritBuffer"),
      }} />
      {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
      {dirty || saving ? (
        <PageActionBar pending={dirty} actions={
          <Button disabled={saving || !dirty} onClick={() => void save()} size="sm" type="button">
            {saving ? <LoaderCircle className="vk-spin" /> : null}
            {t(saving ? "settings.modelExperience.savingSystem" : "settings.contextBasics.saveSystemDefault")}
          </Button>
        }>{t("settings.modelExperience.dirtySystem")}</PageActionBar>
      ) : null}
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
// Parameters: props supplies fixed-identity nodes and save callbacks; returns editable token values while preserving node count and order.
// 参数：props 提供固定身份节点与保存回调；返回可编辑令牌值，同时保留节点数量与顺序。
function ContextCompactionWindowNodesCard(props: {
  // nodes is the latest backend-authoritative complete node configuration.
  // nodes 是后台最新权威完整节点配置。
  nodes: PublicAiContextCompactionWindowConfiguration
  // onSaved receives the authoritative settings snapshot after a successful save.
  // onSaved 在保存成功后接收权威设置快照。
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  // children renders after the node rows, before the save bar.
  // children 渲染在节点行之后、保存栏之前。
  children?: React.ReactNode
}) {
  const { locale, t } = useI18n()
  // draft is the complete five-node edit buffer; identities are never editable.
  // draft 是完整五节点编辑缓冲；节点身份始终不可编辑。
  const [draft, setDraft] = React.useState(() => cloneContextCompactionWindowConfiguration(props.nodes))
  // saving prevents overlapping global node configuration writes.
  // saving 防止全局节点配置写入重叠执行。
  const [saving, setSaving] = React.useState(false)
  // Lock synchronously before awaiting this independent section write.
  // 等待独立分区写入前同步加锁。
  const savingRef = React.useRef(false)
  // error contains one actionable validation or persistence failure.
  // error 保存一条可操作的校验或持久化失败信息。
  const [error, setError] = React.useState<string | null>(null)
  // nodesKey preserves unsaved node edits when another section saves an unchanged node snapshot.
  // nodesKey 在另一分区保存未改变节点的快照时保留未保存节点编辑。
  const nodesKey = JSON.stringify(props.nodes)
  // dirty compares the complete fixed tuple with its persisted baseline.
  // dirty 将完整固定元组与持久化基线比较。
  const dirty = JSON.stringify(draft) !== nodesKey

  React.useEffect(() => {
    setDraft(cloneContextCompactionWindowConfiguration(props.nodes))
  }, [nodesKey])

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
    if (savingRef.current) return
    if (validationError) {
      setError(validationError)
      return
    }
    savingRef.current = true
    setSaving(true)
    setError(null)
    try {
      props.onSaved(await savePublicAiContextCompactionWindowNodes(draft))
      toast.success(t("settings.modelExperience.savedNodes"))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      savingRef.current = false
      setSaving(false)
    }
  }, [draft, props, validationError, t])

  return (
    <section className="context-basics-section">
      <ContextBasicsHeader title={t("settings.contextCompactionNodes.title")}>
        <p>{t("settings.contextCompactionNodes.description")}</p>
        <p className="vk-form-note">{t("settings.contextCompactionNodes.warning")}</p>
      </ContextBasicsHeader>
      <fieldset className="vk-form-prefs context-basics-rows" disabled={saving}>
        {draft.nodes.map((node, index) => (
          <label className="vk-form-pref" key={node.id}>
            <span className="vk-form-pref-text"><strong>{t(CONTEXT_COMPACTION_NODE_LABEL_KEYS[node.id])}</strong></span>
            <span className="context-basics-unit">
              <Input
                className="vk-form-w-md"
                inputMode="numeric"
                min={1}
                onChange={(event) => updateNodeTokens(index, Number.parseInt(event.target.value.replace(/\D/g, "") || "0", 10))}
                step={1000}
                type="number"
                value={node.tokens}
              />
              <small>{tokenUnitLabel(locale)}</small>
            </span>
          </label>
        ))}
        {props.children}
      </fieldset>
      {validationError || error ? <p className="vk-field-error" role="alert">{error ?? validationError}</p> : null}
      {dirty || saving ? (
        <PageActionBar pending={dirty} actions={
          <Button disabled={saving || !dirty || validationError !== null} onClick={() => void save()} size="sm" type="button">
            {saving ? <LoaderCircle className="vk-spin" /> : null}
            {t(saving ? "settings.modelExperience.savingNodes" : "settings.contextCompactionNodes.save")}
          </Button>
        }>{t("settings.modelExperience.dirtyNodes")}</PageActionBar>
      ) : null}
    </section>
  )
}

// NewSessionContextWindowDefaultRow saves the default applied at conversation creation as soon as it is chosen.
// NewSessionContextWindowDefaultRow 在选择后立即保存创建会话时应用的默认窗口。
// Parameters: props supplies the saved node identity, node values and snapshot callback.
// 参数：props 提供已保存节点身份、节点数值和快照回调。
function NewSessionContextWindowDefaultRow(props: {
  // nodeId is null when new conversations follow the selected model.
  // nodeId 为空时新会话遵循所选模型。
  nodeId: ContextCompactionWindowNodeId | null
  // nodes provides the five current system window values.
  // nodes 提供五个当前系统窗口数值。
  nodes: PublicAiContextCompactionWindowConfiguration
  // onSaved adopts the backend-authoritative result.
  // onSaved 采用后端权威结果。
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
}) {
  const { locale, t } = useI18n()
  // pending keeps the chosen value visible while its write is in flight.
  // pending 在写入进行中保持所选值可见。
  const [pending, setPending] = React.useState<ContextCompactionWindowNodeId | null | undefined>(undefined)
  // error displays one persistence failure.
  // error 展示一条持久化失败。
  const [error, setError] = React.useState<string | null>(null)
  // save writes only the new-session default and leaves existing sessions untouched.
  // save 只写入新会话默认值，不改变已有会话。
  const save = React.useCallback(async (nodeId: ContextCompactionWindowNodeId | null) => {
    if (pending !== undefined || nodeId === props.nodeId) return
    setPending(nodeId)
    setError(null)
    try {
      props.onSaved(await savePublicAiNewSessionContextWindowDefault(nodeId))
      toast.success(t("settings.contextCompactionNodes.defaultSaved"))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending(undefined)
    }
  }, [pending, props, t])
  // shown is the in-flight choice, else the saved one.
  // shown 是进行中的选择，否则为已保存值。
  const shown = pending === undefined ? props.nodeId : pending

  return (
    <div className="vk-form-pref">
      <span className="vk-form-pref-text">
        <strong>{t("settings.contextCompactionNodes.defaultTitle")}</strong>
        <small className="vk-form-note">{t("settings.contextCompactionNodes.defaultDescription")} {t("settings.contextCompactionNodes.defaultLimitHint")}</small>
      </span>
      <Select disabled={pending !== undefined} onValueChange={(value) => void save(value === "follow-model" ? null : value as ContextCompactionWindowNodeId)} value={shown ?? "follow-model"}>
        <SelectTrigger aria-label={t("settings.contextCompactionNodes.defaultTitle")} className="vk-form-w-lg"><SelectValue /></SelectTrigger>
        <SelectContent>
          <SelectItem value="follow-model">{t("settings.contextCompactionNodes.followModel")}</SelectItem>
          {props.nodes.nodes.map((node) => (
            <SelectItem key={node.id} value={node.id}>
              {t(CONTEXT_COMPACTION_NODE_LABEL_KEYS[node.id])} · {formatTokenCount(node.tokens)} {tokenUnitLabel(locale)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
    </div>
  )
}

// AccountUsageRefreshCard edits the persisted background account-usage cadence.
// AccountUsageRefreshCard 编辑持久化的后台账号用量刷新周期。
// Parameters: props supplies the persisted interval, save callback and optional statistics link; returns the validated background-usage refresh editor.
// 参数：props 提供持久化间隔、保存回调与可选统计链接；返回经过校验的后台用量刷新编辑器。
function AccountUsageRefreshCard(props: {
  // onSaved receives the authoritative post-save settings snapshot.
  // onSaved 接收保存后的权威设置快照。
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  // intervalSeconds is the currently persisted refresh cadence.
  // intervalSeconds 是当前持久化刷新周期。
  intervalSeconds: number
  // onOpenUsageStatistics opens an installed statistics mount; absence hides the link.
  // onOpenUsageStatistics 打开已安装的统计挂载；缺少时隐藏链接。
  onOpenUsageStatistics?: () => void
}) {
  const { t } = useI18n()
  // saving prevents overlapping interval mutations.
  // saving 防止刷新周期变更重叠执行。
  const [saving, setSaving] = React.useState(false)
  // Lock synchronously before awaiting this independent section write.
  // 等待独立分区写入前同步加锁。
  const savingRef = React.useRef(false)
  // error contains one actionable interval save failure.
  // error 保存一条可操作的刷新周期保存失败。
  const [error, setError] = React.useState<string | null>(null)
  // save persists one selected interval and publishes the resulting settings snapshot.
  // save 持久化一个选定周期并发布生成的设置快照。
  const save = React.useCallback(async (intervalSeconds: number) => {
    if (savingRef.current) return
    savingRef.current = true
    setSaving(true)
    setError(null)
    try {
      props.onSaved(await savePublicAiUsageRefreshSettings(intervalSeconds))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      savingRef.current = false
      setSaving(false)
    }
  }, [props])
  return (
    <div className="vk-form-prefs context-basics-rows">
      <div className="vk-form-pref">
        <span className="vk-form-pref-text">
          <strong>{t("settings.usageRefresh.title")}</strong>
          <small className="vk-form-note">{t("settings.usageRefresh.description")}</small>
          {props.onOpenUsageStatistics ? (
            <Button className="vk-form-fit" onClick={props.onOpenUsageStatistics} size="sm" type="button" variant="link">
              {t("settings.modelExperience.statistics")}<ArrowUpRight aria-hidden="true" />
            </Button>
          ) : null}
        </span>
        <Select disabled={saving} onValueChange={(value) => void save(Number(value))} value={String(props.intervalSeconds)}>
          <SelectTrigger aria-label={t("settings.usageRefresh.title")} className="vk-form-w-md"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="60">{t("settings.usageRefresh.minute").replace("{count}", "1")}</SelectItem>
            <SelectItem value="300">{t("settings.usageRefresh.minutes").replace("{count}", "5")}</SelectItem>
            <SelectItem value="900">{t("settings.usageRefresh.minutes").replace("{count}", "15")}</SelectItem>
            <SelectItem value="1800">{t("settings.usageRefresh.minutes").replace("{count}", "30")}</SelectItem>
            <SelectItem value="3600">{t("settings.usageRefresh.hour").replace("{count}", "1")}</SelectItem>
          </SelectContent>
        </Select>
        {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
      </div>
    </div>
  )
}

// SNAPSHOT_SECTIONS lists the categories whose editors are built from the AI settings snapshot.
// SNAPSHOT_SECTIONS 列出编辑器依赖 AI 设置快照构建的分类。
const SNAPSHOT_SECTIONS: ReadonlySet<string> = new Set(["context", "nodes", "media", "usage"])

// ContextBasicsSettingsArea groups independent settings editors by user task while retaining their save ownership.
// ContextBasicsSettingsArea 按用户任务组织独立设置编辑器，同时保留各自保存归属。
// Parameters: props supplies optional output-style content and the real statistics navigation callback; returns task-grouped settings with independent save ownership.
// 参数：props 提供可选输出风格内容与真实统计导航回调；返回按任务分区且保存归属独立的设置页。
export function ContextBasicsSettingsArea(props: {
  // Service-backed settings actions used by request retry settings.
  // 请求重试设置使用的服务端设置操作。
  actions: SettingsActionProps
  // Workbench-projected request retry settings.
  // 工作台投影出的请求重试设置。
  retrySettings?: ProviderRetrySettingsSummary
  // Conversation-default editor supplied by the settings composition root.
  // 由设置组合入口提供的会话默认编辑器。
  outputStyleSettings?: React.ReactNode
  // Base system prompt editor supplied by the settings composition root.
  // 由设置组合入口提供的基础系统提示词编辑器。
  systemPromptSettings?: React.ReactNode
  // Default shell projection of the integrated terminal; absent before the service is ready.
  // 集成终端的默认 Shell 投影；服务就绪前缺省。
  commandShell?: CommandShellSettingsSummary
  // Opens an actually installed statistics mount; absence leaves the optional link hidden.
  // 打开实际安装的统计挂载；缺少挂载时不显示可选链接。
  onOpenUsageStatistics?: () => void
}) {
  // t resolves localized settings copy.
  // t 解析本地化设置文案。
  const { t } = useI18n()
  // Keep category selection independent of the mounted editors so unsaved drafts survive navigation.
  // 分类选择独立于已挂载编辑器，使未保存草稿在导航切换后保留。
  const [activeSection, setActiveSection] = React.useState(props.outputStyleSettings ? "conversation" : "context")
  // Localized category entries retain the existing feature availability and save ownership.
  // 本地化分类保持现有功能可用性及保存归属。
  const sections = [
    ...(props.outputStyleSettings ? [{ id: "conversation", label: t("settings.modelExperience.conversation") }] : []),
    ...(props.systemPromptSettings ? [{ id: "system-prompt", label: t("settings.modelExperience.systemPrompt") }] : []),
    { id: "context", label: t("settings.modelExperience.context") },
    { id: "nodes", label: t("settings.modelExperience.nodes") },
    { id: "retry", label: t("settings.modelExperience.retry") },
    { id: "terminal", label: t("settings.modelExperience.terminal") },
    { id: "media", label: t("settings.modelExperience.media") },
    { id: "usage", label: t("settings.modelExperience.usage") },
  ]
  // snapshot holds the authoritative configuration shared by the independent save sections.
  // snapshot 保存各独立保存分区共享的权威配置。
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  // error exposes configuration loading failures.
  // error 展示配置加载失败。
  const [error, setError] = React.useState<string | null>(null)
  // loadGeneration lets only the latest read, and only while mounted, publish its result.
  // loadGeneration 只允许最近一次且仍处于挂载状态的读取发布结果。
  const loadGeneration = React.useRef(0)
  const loadSnapshot = React.useCallback(() => {
    const generation = ++loadGeneration.current
    setError(null)
    void loadPublicAiSettings().then((next) => {
      if (generation === loadGeneration.current) setSnapshot(next)
    }).catch((reason: unknown) => {
      if (generation === loadGeneration.current) setError(reason instanceof Error ? reason.message : String(reason))
    })
  }, [])
  React.useEffect(() => {
    loadSnapshot()
    return () => { loadGeneration.current += 1 }
  }, [loadSnapshot])
  // Only these sections read the AI settings snapshot; the others never wait on it or show its failure.
  // 只有这些分区读取 AI 设置快照；其他分区既不等待它，也不显示它的失败。
  const snapshotSection = SNAPSHOT_SECTIONS.has(activeSection)

  return (
    <WorkspaceDirectoryLayout contentClassName="context-basics-surface vk-form-loose" navigationMode="tabs" navigation={
      <WorkspaceDirectoryMenu ariaLabel={t("settings.contextBasics.title")} heading={t("workspaceToolPanel.category")}>
        {sections.map((section) => <DirectoryRow active={activeSection === section.id} key={section.id} label={section.label} onSelect={() => setActiveSection(section.id)} />)}
      </WorkspaceDirectoryMenu>
    }>
      {error && snapshotSection ? (
        <SettingsErrorNotice
          action={<Button onClick={loadSnapshot} size="sm" type="button" variant="outline">{t("settings.modelExperience.retryLoad")}</Button>}
          detail={error}
          message={t("settings.contextBasics.loadFailed")}
          onServiceReconnected={loadSnapshot}
        />
      ) : null}
      <div className="context-basics-tabs">
        {props.outputStyleSettings ? <section className="vk-form-prefs" hidden={activeSection !== "conversation"}>{props.outputStyleSettings}</section> : null}
        {props.systemPromptSettings ? <section hidden={activeSection !== "system-prompt"}>{props.systemPromptSettings}</section> : null}
        {snapshot ? (
          <>
            {/* Adopt only the saved section so late full snapshots cannot regress another independent editor. */}
            {/* 仅采用已保存分区，避免迟到的完整快照使另一独立编辑器倒退。 */}
            <section hidden={activeSection !== "context"}>
              <SystemContextWindowPolicyCard onSaved={(next) => setSnapshot((current) => current ? { ...current, contextWindowPolicyOverrides: next.contextWindowPolicyOverrides } : next)} policy={snapshot.contextWindowPolicyOverrides} defaults={snapshot.contextWindowPolicyDefaults} />
            </section>
            <section hidden={activeSection !== "nodes"}>
              <ContextCompactionWindowNodesCard nodes={snapshot.contextCompactionWindowNodes} onSaved={(next) => setSnapshot((current) => current ? { ...current, contextCompactionWindowNodes: next.contextCompactionWindowNodes } : next)}>
                <NewSessionContextWindowDefaultRow nodeId={snapshot.newSessionContextCompactionWindowNodeId} nodes={snapshot.contextCompactionWindowNodes} onSaved={(next) => setSnapshot((current) => current ? { ...current, newSessionContextCompactionWindowNodeId: next.newSessionContextCompactionWindowNodeId } : next)} />
              </ContextCompactionWindowNodesCard>
            </section>
            <section hidden={activeSection !== "media"}>
              <MediaDeliveryPolicyCard settings={snapshot.mediaDelivery} onSaved={(next) => setSnapshot((current) => current ? { ...current, mediaDelivery: next.mediaDelivery } : next)} />
            </section>
            <section hidden={activeSection !== "usage"}>
              <AccountUsageRefreshCard intervalSeconds={snapshot.usageRefreshIntervalSeconds} onOpenUsageStatistics={props.onOpenUsageStatistics} onSaved={(next) => setSnapshot((current) => current ? { ...current, usageRefreshIntervalSeconds: next.usageRefreshIntervalSeconds } : next)} />
            </section>
          </>
        ) : !error && snapshotSection ? <StreamPlaceholder label={t("settings.contextBasics.loading")} shape="fields" count={4} /> : null}
        <section hidden={activeSection !== "retry"}>
          <ProviderRetrySettingsArea actions={props.actions} settings={props.retrySettings} />
        </section>
        <section hidden={activeSection !== "terminal"}>
          <TerminalShellSettingsCard onSave={props.actions.onSaveDefaultCommandShell} settings={props.commandShell} />
        </section>
      </div>
    </WorkspaceDirectoryLayout>
  )
}
