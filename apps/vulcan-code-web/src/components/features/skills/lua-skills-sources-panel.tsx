import * as React from "react"
import { ExternalLink, Gauge, Plus, Trash2 } from "lucide-react"

import { Badge } from "@/ui-kit/primitives/badge"
import { Button, IconButton } from "@/ui-kit/primitives/button"
import { Input } from "@/ui-kit/primitives/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { Switch } from "@/ui-kit/primitives/switch"
import { Textarea } from "@/ui-kit/primitives/textarea"
import { PageBody, PageNotice, PageRow, PageSection, PageTable } from "@/components/page"
import { formatMessage, useI18n } from "@/i18n"
import { openExternalUrl } from "@/service/project-actions-service"
import { listLuaSkillsGithubMirrors, probeLuaSkillsGithubMirrors } from "@/service/skills-actions-service"
import type {
  LuaSkillsCatalogSummary,
  LuaSkillsGithubMirror,
  LuaSkillsGithubMirrorProbeResult,
  LuaSkillsNetworkSettings,
} from "@/types"

import { LuaSkillsHubRegionNotice } from "./lua-skills-hub-region-notice"
import {
  LUASKILLS_GITHUB_TOKEN_PAGE,
  LUASKILLS_HUB_SITE_URLS,
  activeMirrorId,
  customMirrorId,
  effectiveHubBaseUrl,
  hubSiteOf,
  mirrorChoices,
  trimmedUrl,
  withNetwork,
  type LuaSkillsHubSiteId,
  type LuaSkillsMirrorChoice,
} from "./lua-skills-network-sources"
import type { SkillActionProps } from "./skill-area-types"
import "@/ui-kit/styles/features/settings-forms.css"

// errorText turns one rejected promise value into displayable text.
// errorText 将一个被拒绝的 Promise 值转换为可显示文本。
function errorText(caught: unknown): string {
  return caught instanceof Error ? caught.message : String(caught)
}

// formatSpeed renders one throughput figure with a readable unit.
// formatSpeed 以易读单位渲染一个吞吐数值。
function formatSpeed(bytesPerSecond: number): string {
  if (bytesPerSecond >= 1_048_576) return `${(bytesPerSecond / 1_048_576).toFixed(1)} MB/s`
  return `${Math.max(1, Math.round(bytesPerSecond / 1024))} KB/s`
}

// fastestMirror picks the successful probe with the highest throughput, breaking ties by first byte.
// fastestMirror 选出吞吐最高的成功测速结果，相同时按首字节耗时取舍。
function fastestMirror(results: ReadonlyMap<string, LuaSkillsGithubMirrorProbeResult>): string | null {
  let best: LuaSkillsGithubMirrorProbeResult | null = null
  for (const result of results.values()) {
    if (!result.ok || result.bytesPerSecond == null) continue
    if (
      best === null
      || result.bytesPerSecond > (best.bytesPerSecond ?? 0)
      || (result.bytesPerSecond === best.bytesPerSecond && (result.ttfbMs ?? Infinity) < (best.ttfbMs ?? Infinity))
    ) {
      best = result
    }
  }
  return best?.id ?? null
}

// AdvancedNetworkDraft holds the hand-edited network fields saved together by the advanced section.
// AdvancedNetworkDraft 保存高级区块一并保存的手工编辑网络字段。
type AdvancedNetworkDraft = Pick<
  LuaSkillsNetworkSettings,
  "allowDownloads" | "githubBaseUrl" | "githubApiBaseUrl" | "enablePrivateUrlSkillInstall" | "privateSkillSourceAllowlist"
>

// advancedDraftOf copies the advanced fields out of the committed network block.
// advancedDraftOf 从已提交网络块复制高级字段。
function advancedDraftOf(network: LuaSkillsNetworkSettings): AdvancedNetworkDraft {
  return {
    allowDownloads: network.allowDownloads,
    githubBaseUrl: network.githubBaseUrl ?? "",
    githubApiBaseUrl: network.githubApiBaseUrl ?? "",
    enablePrivateUrlSkillInstall: network.enablePrivateUrlSkillInstall,
    privateSkillSourceAllowlist: [...network.privateSkillSourceAllowlist],
  }
}

// LuaSkillsSourcesPanel manages where LuaSkills downloads from: the Hub site, the GitHub mirror and the token.
// LuaSkillsSourcesPanel 管理 LuaSkills 的下载来源：Hub 站点、GitHub 镜像与令牌。
//
// Every change here applies to the running engine at once, except the private manifest policy, which
// keeps its restart requirement.
// 此处的每项更改都会立即应用到运行中的引擎；私有清单策略除外，它仍需重启。
export function LuaSkillsSourcesPanel(props: {
  // Current LuaSkills catalog including committed settings and token state.
  // 当前 LuaSkills 目录，包含已提交设置与令牌状态。
  catalog: LuaSkillsCatalogSummary
  // Service-backed LuaSkills page actions.
  // 服务端支持的 LuaSkills 页面动作。
  actions: SkillActionProps
}) {
  const { t } = useI18n()
  const settings = props.catalog.runtimeSettings
  const network = settings.network
  // saving serializes every settings or token write issued from this page.
  // saving 串行化本页发出的所有设置或令牌写入。
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [feedback, setFeedback] = React.useState<string | null>(null)

  // run executes one write with shared busy, error and success feedback.
  // run 执行一次写入，共享忙碌、错误与成功反馈。
  const run = React.useCallback(async (write: () => Promise<void>, success: string) => {
    setSaving(true)
    setError(null)
    setFeedback(null)
    try {
      await write()
      setFeedback(success)
      return true
    } catch (caught) {
      setError(errorText(caught))
      return false
    } finally {
      setSaving(false)
    }
  }, [])

  // saveNetwork saves the committed document with one network change.
  // saveNetwork 保存带有一项网络变更的已提交文档。
  const saveNetwork = React.useCallback((change: Partial<LuaSkillsNetworkSettings>) => (
    run(() => props.actions.onSaveLuaSkillsRuntimeSettings(withNetwork(settings, change)), t("skills.luaSources.saved"))
  ), [props.actions, run, settings, t])

  return (
    <PageBody className="lua-settings-page">
      <div className="lua-settings-content">
        <LuaSkillsHubRegionNotice settings={settings} onSave={props.actions.onSaveLuaSkillsRuntimeSettings} />
        {error ? <PageNotice tone="error" title={t("skills.luaSources.saveFailed")} details={error} detailsLabel={t("skills.lua.errorDetails")} /> : null}
        {feedback ? <PageNotice tone="success" title={feedback} /> : null}
        <HubSiteSection network={network} saving={saving} onSave={saveNetwork} />
        <MirrorSection network={network} saving={saving} onSave={saveNetwork} />
        <TokenSection
          configured={props.catalog.githubTokenConfigured}
          saving={saving}
          onSet={(token) => run(
            () => props.actions.onSetLuaSkillsGithubToken(token),
            t(token === null ? "skills.luaSources.tokenCleared" : "skills.luaSources.tokenSaved"),
          )}
        />
        <AdvancedNetworkSection network={network} saving={saving} onSave={saveNetwork} />
      </div>
    </PageBody>
  )
}

// HubSiteSection chooses the LuaSkills Hub: the global site, the China site or a custom address.
// HubSiteSection 选择 LuaSkills Hub：国际站、中国站或自定义地址。
function HubSiteSection(props: {
  network: LuaSkillsNetworkSettings
  saving: boolean
  onSave: (change: Partial<LuaSkillsNetworkSettings>) => Promise<boolean>
}) {
  const { t } = useI18n()
  const current = effectiveHubBaseUrl(props.network)
  const [site, setSite] = React.useState<LuaSkillsHubSiteId>(() => hubSiteOf(current))
  const [customUrl, setCustomUrl] = React.useState(() => (hubSiteOf(current) === "custom" ? current : ""))
  React.useEffect(() => {
    setSite(hubSiteOf(current))
    if (hubSiteOf(current) === "custom") setCustomUrl(current)
  }, [current])
  const target = site === "custom" ? trimmedUrl(customUrl) : LUASKILLS_HUB_SITE_URLS[site]
  return (
    <PageSection
      title={t("skills.luaSources.hubTitle")}
      description={formatMessage(t("skills.luaSources.hubDescription"), { url: current })}
      footer={(
        <Button
          type="button"
          disabled={props.saving || target === null || target === current}
          onClick={() => void props.onSave({ officialSkillHubBaseUrl: target })}
        >
          {t("skills.luaSources.apply")}
        </Button>
      )}
    >
      <label className="vk-field">
        <span>{t("skills.luaSources.hubSite")}</span>
        <Select disabled={props.saving} value={site} onValueChange={(value) => setSite(value as LuaSkillsHubSiteId)}>
          <SelectTrigger className="vk-fill"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="global">{t("skills.luaSources.hubGlobal")} · {LUASKILLS_HUB_SITE_URLS.global}</SelectItem>
            <SelectItem value="china">{t("skills.luaSources.hubChina")} · {LUASKILLS_HUB_SITE_URLS.china}</SelectItem>
            <SelectItem value="custom">{t("skills.luaSources.hubCustom")}</SelectItem>
          </SelectContent>
        </Select>
      </label>
      {site === "custom" ? (
        <label className="vk-field">
          <span>{t("skills.luaSettings.skillHubBaseUrl")}</span>
          <Input type="url" disabled={props.saving} value={customUrl} placeholder="https://" onChange={(event) => setCustomUrl(event.currentTarget.value)} />
        </label>
      ) : null}
    </PageSection>
  )
}

// MirrorSection lists built-in and user mirrors, speed-tests them and selects the one downloads use.
// MirrorSection 列出内置与用户镜像，对其测速并选择下载使用的镜像。
function MirrorSection(props: {
  network: LuaSkillsNetworkSettings
  saving: boolean
  onSave: (change: Partial<LuaSkillsNetworkSettings>) => Promise<boolean>
}) {
  const { t } = useI18n()
  const [builtins, setBuiltins] = React.useState<LuaSkillsGithubMirror[]>([])
  const [loadError, setLoadError] = React.useState<string | null>(null)
  // resultsByUrl keys probe results by the probed address: user mirror ids are reused after deletion, so an id alone
  // could attach an old result to a different mirror.
  // resultsByUrl 按测速地址保存结果：用户镜像标识在删除后会被复用，仅凭标识可能把旧结果挂到另一个镜像上。
  const [resultsByUrl, setResultsByUrl] = React.useState<ReadonlyMap<string, LuaSkillsGithubMirrorProbeResult>>(new Map())
  const [probing, setProbing] = React.useState(false)
  const [probeError, setProbeError] = React.useState<string | null>(null)
  const [newLabel, setNewLabel] = React.useState("")
  const [newUrl, setNewUrl] = React.useState("")

  React.useEffect(() => {
    let live = true
    listLuaSkillsGithubMirrors()
      .then((mirrors) => { if (live) setBuiltins(mirrors) })
      .catch((caught: unknown) => { if (live) setLoadError(errorText(caught)) })
    return () => { live = false }
  }, [])

  const choices = mirrorChoices(builtins, props.network.customGithubMirrors)
  const activeId = activeMirrorId(choices, props.network)
  const results: ReadonlyMap<string, LuaSkillsGithubMirrorProbeResult> = new Map(choices.flatMap((choice) => {
    const result = resultsByUrl.get(choice.githubBaseUrl ?? "")
    return result ? [[choice.id, result] as const] : []
  }))
  const fastestId = fastestMirror(results)

  // probeAll speed-tests every listed mirror in one parallel request.
  // probeAll 以一次并行请求对全部已列出镜像测速。
  const probeAll = async () => {
    setProbing(true)
    setProbeError(null)
    try {
      const probedChoices = choices.map((choice) => ({ id: choice.id, githubBaseUrl: choice.githubBaseUrl }))
      const probed = await probeLuaSkillsGithubMirrors(probedChoices)
      const urlById = new Map(probedChoices.map((choice) => [choice.id, choice.githubBaseUrl ?? ""]))
      setResultsByUrl(new Map(probed.flatMap((result) => {
        const url = urlById.get(result.id)
        return url === undefined ? [] : [[url, result] as const]
      })))
    } catch (caught) {
      setProbeError(errorText(caught))
    } finally {
      setProbing(false)
    }
  }

  // applyMirror applies one mirror's download address; the API address stays an advanced setting.
  // applyMirror 应用一个镜像的下载地址；API 地址仍属于高级设置。
  const applyMirror = (choice: LuaSkillsMirrorChoice) => props.onSave({ githubBaseUrl: choice.githubBaseUrl })

  // removeMirror deletes one user mirror and falls back to direct GitHub when it was in use.
  // removeMirror 删除一个用户镜像；若它正在使用，则回到直连 GitHub。
  const removeMirror = (id: string) => props.onSave({
    customGithubMirrors: props.network.customGithubMirrors.filter((mirror) => mirror.id !== id),
    ...(activeId === id ? { githubBaseUrl: null } : {}),
  })

  // addMirror saves one user mirror; the service normalizes and validates the address.
  // addMirror 保存一个用户镜像；地址由服务端规范化并校验。
  const addMirror = async () => {
    const saved = await props.onSave({
      customGithubMirrors: [
        ...props.network.customGithubMirrors,
        { id: customMirrorId(choices), label: newLabel.trim(), githubBaseUrl: newUrl.trim() },
      ],
    })
    if (saved) {
      setNewLabel("")
      setNewUrl("")
    }
  }

  // speedCell describes one probe result.
  // speedCell 描述一个测速结果。
  const speedCell = (id: string): React.ReactNode => {
    const result = results.get(id)
    if (!result) return probing ? t("skills.luaSources.probing") : "—"
    if (!result.ok) return <span title={result.error ?? undefined}>{t("skills.luaSources.probeFailed")}</span>
    return [
      result.bytesPerSecond != null ? formatSpeed(result.bytesPerSecond) : null,
      result.ttfbMs != null ? formatMessage(t("skills.luaSources.firstByte"), { ms: result.ttfbMs }) : null,
    ].filter(Boolean).join(" · ")
  }

  return (
    <PageSection
      title={t("skills.luaSources.mirrorTitle")}
      description={t("skills.luaSources.mirrorDescription")}
      actions={(
        <>
          <Button type="button" size="sm" variant="outline" disabled={probing || choices.length === 0} onClick={() => void probeAll()}>
            <Gauge size={14} />
            <span>{probing ? t("skills.luaSources.probing") : t("skills.luaSources.probeAll")}</span>
          </Button>
          <Button
            type="button"
            size="sm"
            disabled={props.saving || probing || fastestId === null || fastestId === activeId}
            onClick={() => {
              const fastest = choices.find((choice) => choice.id === fastestId)
              if (fastest) void applyMirror(fastest)
            }}
          >
            {t("skills.luaSources.useFastest")}
          </Button>
        </>
      )}
    >
      {loadError ? <PageNotice tone="error" title={t("skills.luaSources.mirrorLoadFailed")} details={loadError} detailsLabel={t("skills.lua.errorDetails")} /> : null}
      {probeError ? <PageNotice tone="error" title={t("skills.luaSources.probeFailed")} details={probeError} detailsLabel={t("skills.lua.errorDetails")} /> : null}
      {activeId === null && trimmedUrl(props.network.githubBaseUrl) !== null ? (
        <PageNotice tone="info" title={formatMessage(t("skills.luaSources.manualMirror"), { url: props.network.githubBaseUrl ?? "" })} />
      ) : null}
      <PageTable
        label={t("skills.luaSources.mirrorTitle")}
        columns={[
          { key: "name", label: t("skills.luaSources.mirrorName") },
          { key: "address", label: t("skills.luaSources.mirrorAddress"), wrap: true },
          { key: "speed", label: t("skills.luaSources.mirrorSpeed") },
          { key: "actions", label: "", width: "11rem" },
        ]}
        rows={choices.map((choice) => ({
          key: choice.id,
          selected: choice.id === activeId,
          cells: {
            name: (
              <span className="lua-mirror-name">
                <span>{choice.label}</span>
                {choice.builtin ? (
                  <Badge variant="outline">{t(choice.builtin.region === "china" ? "skills.luaSources.regionChina" : "skills.luaSources.regionGlobal")}</Badge>
                ) : (
                  <Badge variant="outline">{t("skills.luaSources.mirrorCustom")}</Badge>
                )}
                {choice.id === fastestId ? <Badge variant="secondary">{t("skills.luaSources.fastest")}</Badge> : null}
              </span>
            ),
            address: choice.githubBaseUrl ?? "https://github.com",
            speed: speedCell(choice.id),
            actions: (
              <span className="lua-mirror-actions">
                {choice.id === activeId ? (
                  <Badge variant="secondary">{t("skills.luaSources.inUse")}</Badge>
                ) : (
                  <Button type="button" size="sm" variant="outline" disabled={props.saving} onClick={() => void applyMirror(choice)}>
                    {t("skills.luaSources.use")}
                  </Button>
                )}
                {choice.builtin === null ? (
                  <IconButton
                    aria-label={t("skills.action.delete")}
                    title={t("skills.action.delete")}
                    disabled={props.saving}
                    onClick={() => void removeMirror(choice.id)}
                    type="button"
                  >
                    <Trash2 size={14} />
                  </IconButton>
                ) : null}
              </span>
            ),
          },
        }))}
      />
      <div className="lua-settings-grid">
        <label className="vk-field">
          <span>{t("skills.luaSources.mirrorLabel")}</span>
          <Input disabled={props.saving} maxLength={64} value={newLabel} onChange={(event) => setNewLabel(event.currentTarget.value)} />
        </label>
        <label className="vk-field">
          <span>{t("skills.luaSources.mirrorUrl")}</span>
          <Input type="url" disabled={props.saving} placeholder="https://" value={newUrl} onChange={(event) => setNewUrl(event.currentTarget.value)} />
          <small className="vk-field-hint">{t("skills.luaSources.mirrorUrlHint")}</small>
        </label>
      </div>
      <div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={props.saving || newLabel.trim() === "" || newUrl.trim() === ""}
          onClick={() => void addMirror()}
        >
          <Plus size={14} />
          <span>{t("skills.luaSources.addMirror")}</span>
        </Button>
      </div>
    </PageSection>
  )
}

// TokenSection stores or clears the GitHub token kept in the operating-system credential store.
// TokenSection 保存或清除存放于操作系统凭据库中的 GitHub 令牌。
function TokenSection(props: {
  configured: boolean
  saving: boolean
  onSet: (token: string | null) => Promise<boolean>
}) {
  const { t } = useI18n()
  const [token, setToken] = React.useState("")
  const [openError, setOpenError] = React.useState<string | null>(null)
  const inputId = React.useId()
  return (
    <PageSection
      title={t("skills.luaSources.tokenTitle")}
      description={t("skills.luaSources.tokenDescription")}
      status={<Badge variant={props.configured ? "secondary" : "outline"}>{t(props.configured ? "skills.luaSources.tokenConfigured" : "skills.luaSources.tokenMissing")}</Badge>}
      actions={(
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => {
            setOpenError(null)
            void openExternalUrl(LUASKILLS_GITHUB_TOKEN_PAGE).catch((caught: unknown) => setOpenError(errorText(caught)))
          }}
        >
          <ExternalLink size={14} />
          <span>{t("skills.luaSources.tokenOpenPage")}</span>
        </Button>
      )}
    >
      {openError ? <PageNotice tone="error" title={openError} /> : null}
      <label className="vk-field" htmlFor={inputId}>
        <span>{t("skills.luaSources.tokenInput")}</span>
        <Input
          id={inputId}
          type="password"
          autoComplete="off"
          spellCheck={false}
          disabled={props.saving}
          value={token}
          placeholder={props.configured ? t("skills.luaSources.tokenReplacePlaceholder") : "ghp_…"}
          onChange={(event) => setToken(event.currentTarget.value)}
        />
        <small className="vk-field-hint">{t("skills.luaSources.tokenHint")}</small>
      </label>
      <div className="lua-mirror-actions">
        <Button
          type="button"
          size="sm"
          disabled={props.saving || token.trim() === ""}
          onClick={() => void props.onSet(token).then((saved) => { if (saved) setToken("") })}
        >
          {t("skills.luaSources.tokenSave")}
        </Button>
        {props.configured ? (
          <Button type="button" size="sm" variant="outline" disabled={props.saving} onClick={() => void props.onSet(null)}>
            {t("skills.luaSources.tokenClear")}
          </Button>
        ) : null}
      </div>
    </PageSection>
  )
}

// AdvancedNetworkSection edits the raw download switch, addresses and private manifest policy.
// AdvancedNetworkSection 编辑原始下载开关、地址与私有清单策略。
function AdvancedNetworkSection(props: {
  network: LuaSkillsNetworkSettings
  saving: boolean
  onSave: (change: Partial<LuaSkillsNetworkSettings>) => Promise<boolean>
}) {
  const { t } = useI18n()
  const committedKey = JSON.stringify(advancedDraftOf(props.network))
  const [draft, setDraft] = React.useState<AdvancedNetworkDraft>(() => advancedDraftOf(props.network))
  React.useEffect(() => {
    setDraft(JSON.parse(committedKey) as AdvancedNetworkDraft)
  }, [committedKey])
  const dirty = JSON.stringify(draft) !== committedKey
  const update = (change: Partial<AdvancedNetworkDraft>) => setDraft((current) => ({ ...current, ...change }))
  const downloadsId = React.useId()
  const privateId = React.useId()
  return (
    <PageSection
      title={t("skills.luaSources.advancedTitle")}
      description={t("skills.luaSources.advancedDescription")}
      footer={dirty ? (
        <Button
          type="button"
          disabled={props.saving}
          onClick={() => void props.onSave({
            ...draft,
            githubBaseUrl: trimmedUrl(draft.githubBaseUrl),
            githubApiBaseUrl: trimmedUrl(draft.githubApiBaseUrl),
            privateSkillSourceAllowlist: draft.privateSkillSourceAllowlist.map((entry) => entry.trim()).filter(Boolean),
          })}
        >
          {t("skills.luaSettings.save")}
        </Button>
      ) : null}
    >
      <PageRow label={t("skills.luaSettings.allowDownloads")} htmlFor={downloadsId}>
        <Switch id={downloadsId} disabled={props.saving} checked={draft.allowDownloads} onCheckedChange={(checked) => update({ allowDownloads: checked })} />
      </PageRow>
      <div className="lua-settings-grid">
        <label className="vk-field">
          <span>{t("skills.luaSettings.githubBaseUrl")}</span>
          <Input disabled={props.saving} value={draft.githubBaseUrl ?? ""} onChange={(event) => update({ githubBaseUrl: event.currentTarget.value })} />
        </label>
        <label className="vk-field">
          <span>{t("skills.luaSettings.githubApiBaseUrl")}</span>
          <Input disabled={props.saving} value={draft.githubApiBaseUrl ?? ""} onChange={(event) => update({ githubApiBaseUrl: event.currentTarget.value })} />
        </label>
      </div>
      <PageRow label={t("skills.luaSettings.privateUrlInstall")} htmlFor={privateId} description={t("skills.luaSources.privateRestartHint")}>
        <Switch id={privateId} disabled={props.saving} checked={draft.enablePrivateUrlSkillInstall} onCheckedChange={(checked) => update({ enablePrivateUrlSkillInstall: checked })} />
      </PageRow>
      <label className="vk-field">
        <span>{t("skills.luaSettings.privateAllowlist")}</span>
        <Textarea
          disabled={props.saving}
          value={draft.privateSkillSourceAllowlist.join("\n")}
          onChange={(event) => update({ privateSkillSourceAllowlist: event.currentTarget.value.split(/\r?\n/) })}
        />
        <small className="vk-field-hint">{t("skills.luaSettings.privateAllowlistHint")}</small>
      </label>
    </PageSection>
  )
}
