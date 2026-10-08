import * as React from "react"
import { LoaderCircle } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { StreamPlaceholder } from "@/components/ui/stream-placeholder"
import { Textarea } from "@/components/ui/textarea"
import { formatMessage, useI18n } from "@/i18n"
import {
  loadSystemPromptSettings,
  resetSystemPrompt,
  saveSystemPrompt,
} from "@/service/settings-actions-service"
import type { ProjectSummary, SystemPromptSettingsSnapshot, SystemPromptSource } from "@/types"

// USER_SCOPE_VALUE is the select value of the user-level scope; project scopes use their project id.
// USER_SCOPE_VALUE 是用户级作用域的选择值；项目作用域使用其项目标识。
const USER_SCOPE_VALUE = "__user__"

// SOURCE_LABEL_KEYS binds each effective source to one localized label.
// SOURCE_LABEL_KEYS 将每个生效来源绑定到一个本地化标签。
const SOURCE_LABEL_KEYS: Record<
  SystemPromptSource,
  | "settings.systemPrompt.sourceBuiltIn"
  | "settings.systemPrompt.sourceUser"
  | "settings.systemPrompt.sourceProject"
> = {
  builtIn: "settings.systemPrompt.sourceBuiltIn",
  user: "settings.systemPrompt.sourceUser",
  project: "settings.systemPrompt.sourceProject",
}

// inheritedText returns the prompt a scope currently uses, which is the editor's saved baseline.
// inheritedText 返回某作用域当前使用的提示词，即编辑器的已保存基线。
// Parameters: snapshot is the loaded layer set and projectId selects the project scope; returns the effective text.
// 参数：snapshot 是已加载的层级集合，projectId 选择项目作用域；返回生效正文。
function inheritedText(snapshot: SystemPromptSettingsSnapshot, projectId: string | null): string {
  if (projectId && snapshot.projectText !== null) return snapshot.projectText
  return snapshot.userText ?? snapshot.builtInText
}

// SystemPromptSettingsCard edits the user-level or one project-level base system prompt override.
// SystemPromptSettingsCard 编辑用户级或单个项目级的基础系统提示词覆盖。
// Parameters: props supplies the selectable projects; returns a self-loading editor with save and restore actions.
// 参数：props 提供可选择的项目；返回带保存与恢复操作的自加载编辑器。
export function SystemPromptSettingsCard(props: { projects: ProjectSummary[] }) {
  const { t } = useI18n()
  // scopeValue is USER_SCOPE_VALUE or the selected project id.
  // scopeValue 为 USER_SCOPE_VALUE 或所选项目标识。
  const [scopeValue, setScopeValue] = React.useState(USER_SCOPE_VALUE)
  const projectId = scopeValue === USER_SCOPE_VALUE ? null : scopeValue
  // snapshot is the authoritative layer set for the selected scope.
  // snapshot 是所选作用域的权威层级集合。
  const [snapshot, setSnapshot] = React.useState<SystemPromptSettingsSnapshot | null>(null)
  // draft is the editable prompt body.
  // draft 是可编辑的提示词正文。
  const [draft, setDraft] = React.useState("")
  // busy blocks overlapping writes; savingRef locks synchronously before React commits.
  // busy 阻止重叠写入；savingRef 在 React 提交前同步加锁。
  const [busy, setBusy] = React.useState(false)
  const savingRef = React.useRef(false)
  // saved shows the confirmation only after an authoritative write.
  // saved 仅在权威写入完成后显示确认。
  const [saved, setSaved] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  // requestRef discards late responses from a previously selected scope.
  // requestRef 丢弃先前所选作用域的迟到响应。
  const requestRef = React.useRef(0)

  // adopt applies one snapshot and resets the draft to the scope's saved baseline.
  // adopt 应用一个快照，并将草稿重置为该作用域的已保存基线。
  const adopt = React.useCallback((next: SystemPromptSettingsSnapshot, scopeProjectId: string | null) => {
    setSnapshot(next)
    setDraft(inheritedText(next, scopeProjectId))
  }, [])

  React.useEffect(() => {
    const request = ++requestRef.current
    setSnapshot(null)
    setSaved(false)
    setError(null)
    void loadSystemPromptSettings(projectId).then((next) => {
      if (requestRef.current === request) adopt(next, projectId)
    }).catch((reason: unknown) => {
      if (requestRef.current === request) setError(reason instanceof Error ? reason.message : String(reason))
    })
  }, [projectId, adopt])

  // run performs one write and adopts its returned snapshot.
  // run 执行一次写入并采用其返回的快照。
  const run = React.useCallback(async (write: () => Promise<SystemPromptSettingsSnapshot>) => {
    if (savingRef.current) return
    savingRef.current = true
    const request = requestRef.current
    setBusy(true)
    setSaved(false)
    setError(null)
    try {
      const next = await write()
      if (requestRef.current === request) {
        adopt(next, projectId)
        setSaved(true)
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      savingRef.current = false
      setBusy(false)
    }
  }, [adopt, projectId])

  const scope = projectId ? "project" : "user"
  const baseline = snapshot ? inheritedText(snapshot, projectId) : ""
  const empty = draft.trim().length === 0
  const dirty = snapshot !== null && draft.trim() !== baseline.trim()
  const overridePresent = snapshot !== null && (projectId ? snapshot.projectText !== null : snapshot.userText !== null)
  const overridePath = snapshot ? (projectId ? snapshot.projectPath : snapshot.userPath) : null

  return (
    <section className="space-y-3 rounded-xl border p-4">
      <div>
        <h3 className="text-sm font-semibold">{t("settings.modelExperience.systemPrompt")}</h3>
        <p className="text-xs text-muted-foreground">{t("settings.systemPrompt.description")}</p>
      </div>
      <label className="flex items-center gap-3 text-sm">
        <span>{t("settings.systemPrompt.scope")}</span>
        <Select value={scopeValue} onValueChange={setScopeValue} disabled={busy}>
          <SelectTrigger className="w-72"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value={USER_SCOPE_VALUE}>{t("settings.systemPrompt.scopeUser")}</SelectItem>
            {props.projects.map((project) => (
              <SelectItem key={project.id} value={project.id}>
                {formatMessage(t("settings.systemPrompt.scopeProject"), { name: project.name })}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </label>
      {snapshot ? (
        <>
          <p className="text-xs text-muted-foreground" role="status">{t(SOURCE_LABEL_KEYS[snapshot.effectiveSource])}</p>
          <Textarea
            aria-label={t("settings.systemPrompt.textLabel")}
            className="min-h-64 font-mono text-xs"
            disabled={busy}
            onChange={(event) => { setDraft(event.currentTarget.value); setSaved(false) }}
            value={draft}
          />
          {overridePath ? <p className="text-xs text-muted-foreground">{formatMessage(t("settings.systemPrompt.savedTo"), { path: overridePath })}</p> : null}
          {empty ? <p className="text-sm text-destructive" role="alert">{t("settings.systemPrompt.empty")}</p> : null}
          <p className="text-xs text-muted-foreground" role="status">
            {busy ? t("settings.systemPrompt.saving") : dirty ? t("settings.systemPrompt.dirty") : saved ? t("settings.systemPrompt.saved") : null}
          </p>
          <div className="flex flex-wrap gap-2">
            <Button disabled={busy || !dirty || empty} onClick={() => void run(() => saveSystemPrompt(scope, projectId, draft))} type="button">
              {busy ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
              {t("settings.systemPrompt.save")}
            </Button>
            <Button disabled={busy || !overridePresent} onClick={() => void run(() => resetSystemPrompt(scope, projectId))} type="button" variant="outline">
              {t(projectId ? "settings.systemPrompt.resetProject" : "settings.systemPrompt.resetUser")}
            </Button>
          </div>
        </>
      ) : !error ? <StreamPlaceholder label={t("settings.systemPrompt.loading")} shape="fields" count={3} /> : null}
      {error ? <p className="text-sm text-destructive" role="alert">{error}</p> : null}
    </section>
  )
}
