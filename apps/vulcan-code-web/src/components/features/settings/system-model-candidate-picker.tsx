import * as React from "react"
import { LoaderCircle, Plus, RefreshCw } from "lucide-react"

import { useI18n } from "@/i18n"
import { fetchPublicAiModelCandidates } from "@/service/settings-actions-service"
import type { PublicAiCustomModelDefinition, PublicAiModelCandidate, PublicAiModelCandidatesRecord, PublicAiServiceEntry, PublicAiSettingsSnapshot } from "@/types"
import { Button } from "@/ui-kit/primitives/button"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"

// AddableSystemModelCandidate is a verified conversation candidate with the fields needed to open its draft.
// AddableSystemModelCandidate 是带有草稿所需字段的已验证会话候选。
export type AddableSystemModelCandidate = PublicAiModelCandidate & {
  prefill: NonNullable<PublicAiModelCandidate["prefill"]>
}

// SystemModelCandidatePickerProps binds discovery to one system provider and hands off one selected draft.
// SystemModelCandidatePickerProps 将发现限定在单个系统供应商内，并交付一条选中的草稿。
interface SystemModelCandidatePickerProps {
  providerId: string
  entries: readonly PublicAiServiceEntry[]
  snapshot: PublicAiSettingsSnapshot
  savedModels: readonly PublicAiCustomModelDefinition[]
  hidden: boolean
  busy: boolean
  onChoose: (entryId: string, candidate: AddableSystemModelCandidate) => void
}

// SystemModelCandidatePicker fetches candidates for a selected service and account, and renders unsaved conversation rows.
// SystemModelCandidatePicker 按选中的服务入口及账号拉取候选，并渲染尚未保存的会话模型行。
export function SystemModelCandidatePicker(props: SystemModelCandidatePickerProps) {
  const { t } = useI18n()
  // The account selection is scoped to one entry and never reused for another entry's fetch.
  // 账号选择属于单个入口，拉取其他入口时不会复用。
  const [entryId, setEntryId] = React.useState(props.entries[0]?.id ?? "")
  const [connectionId, setConnectionId] = React.useState("")
  // Own the latest client request so closing the dialog or switching accounts discards its response.
  // 持有最近一次客户端请求，关闭弹窗或切换账号时丢弃其响应。
  const pending = React.useRef<AbortController | null>(null)
  const [fetching, setFetching] = React.useState(false)
  const [result, setResult] = React.useState<{ connectionId: string; catalog: PublicAiModelCandidatesRecord } | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  React.useEffect(() => () => pending.current?.abort(), [])

  // Only enabled and validated accounts for the selected entry can supply a catalog credential.
  // 只有所选入口下已启用且已验证的账号可以提供目录凭据。
  const accounts = props.snapshot.connections.filter((account) => (
    account.provider_id === props.providerId && account.service_entry_id === entryId && account.enabled && !account.pending_validation
  ))
  const account = accounts.find((item) => item.id === connectionId) ?? accounts[0]
  const remote = props.snapshot.connectionModelAccessPolicyRecords.some((record) => (
    record.connectionId === account?.id && record.directoryKind === "dynamic"
  ))
  const catalog = remote && result?.connectionId === account?.id && result?.catalog.serviceEntryId === entryId
    && result.catalog.providerId === props.providerId
    ? result.catalog : null
  // Filter out locally saved rows as soon as one candidate is confirmed, without fetching again.
  // 单个候选确认后立即从结果中滤掉，无需重新拉取。
  const savedIds = new Set(props.savedModels.map((model) => model.id))
  const candidates = catalog?.candidates.filter((candidate): candidate is AddableSystemModelCandidate => (
    candidate.kind === "conversation" && candidate.addable && candidate.prefill !== undefined && !savedIds.has(candidate.modelId)
  )) ?? []

  // resetFetch cancels the previous selection's request and clears its transient view.
  // resetFetch 取消上次选择的请求，并清除其临时视图。
  const resetFetch = () => {
    pending.current?.abort()
    pending.current = null
    setFetching(false)
    setResult(null)
    setError(null)
  }

  // fetchCatalog reads the selected account's candidates; its result is discarded on cancellation.
  // fetchCatalog 读取所选账号的候选；取消后丢弃结果。
  const fetchCatalog = async () => {
    if (!account || !remote || pending.current) return
    // Use a fresh controller for each pull so a late result cannot replace a newer selection.
    // 每次拉取使用独立控制器，避免迟到结果覆盖新的选择。
    const controller = new AbortController()
    pending.current = controller
    setFetching(true)
    setResult(null)
    setError(null)
    try {
      const fetched = await fetchPublicAiModelCandidates(account.id, controller.signal)
      if (!controller.signal.aborted) setResult({ connectionId: account.id, catalog: fetched })
    } catch (reason) {
      if (!controller.signal.aborted) setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      if (pending.current === controller) {
        pending.current = null
        setFetching(false)
      }
    }
  }

  if (props.hidden || props.entries.length === 0) return null
  return (
    <div className="vk-form-box vk-form-stack">
      <div className="vk-form-grid">
        <label className="vk-field">
          <span>{t("settings.systemManualModels.entry")}</span>
          <Select disabled={props.busy} value={entryId} onValueChange={(value) => { resetFetch(); setEntryId(value); setConnectionId("") }}>
            <SelectTrigger aria-label={t("settings.systemManualModels.entry")}><SelectValue /></SelectTrigger>
            <SelectContent>{props.entries.map((entry) => <SelectItem key={entry.id} value={entry.id}>{entry.display_name}</SelectItem>)}</SelectContent>
          </Select>
        </label>
        <label className="vk-field">
          <span>{t("settings.inference.selectAccount")}</span>
          <Select disabled={props.busy || accounts.length === 0} value={account?.id ?? ""} onValueChange={(value) => { resetFetch(); setConnectionId(value) }}>
            <SelectTrigger aria-label={t("settings.inference.selectAccount")}>
              <SelectValue placeholder={t("settings.customModels.noAccounts")} />
            </SelectTrigger>
            <SelectContent>{accounts.map((item) => <SelectItem key={item.id} value={item.id}>{item.display_name}</SelectItem>)}</SelectContent>
          </Select>
        </label>
      </div>
      <div className="vk-form-inline">
        <Button disabled={props.busy || fetching || !remote} onClick={() => void fetchCatalog()} type="button" variant="outline">
          {fetching ? <LoaderCircle aria-hidden="true" className="vk-icon vk-spin" /> : <RefreshCw aria-hidden="true" className="vk-icon" />}
          {t("settings.systemManualModels.fetch")}
        </Button>
        {account && !remote ? <p className="vk-form-text">{t("settings.systemManualModels.remoteUnavailable")}</p> : null}
      </div>
      {error ? <p className="vk-form-fault" role="alert">{error}</p> : null}
      {catalog && candidates.length === 0 ? <p className="vk-form-text" role="status">{t("settings.systemManualModels.noCandidates")}</p> : null}
      {candidates.map((candidate) => (
        <div className="vk-form-row vk-form-framed" key={candidate.modelId}>
          <div className="vk-form-cell"><p className="vk-form-name">{candidate.displayName}</p><p className="vk-form-id">{candidate.modelId}</p></div>
          <Button disabled={props.busy} onClick={() => props.onChoose(entryId, candidate)} size="sm" type="button" variant="outline">
            <Plus aria-hidden="true" className="vk-icon" />{t("settings.systemManualModels.addCandidate")}
          </Button>
        </div>
      ))}
    </div>
  )
}
