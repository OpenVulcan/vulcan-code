import * as React from "react"

import { Button } from "@/ui-kit/primitives/button"
import { PageNotice } from "@/components/page"
import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context"
import { formatMessage, useI18n } from "@/i18n"
import type { LuaSkillsRuntimeSettings } from "@/types"

import {
  LUASKILLS_HUB_SITE_URLS,
  effectiveHubBaseUrl,
  hubRegionPromptTarget,
  withNetwork,
} from "./lua-skills-network-sources"

// LuaSkillsHubRegionNotice offers to move the Hub to the signed-in account's region.
// LuaSkillsHubRegionNotice 提议将 Hub 切换到已登录账号所在区域。
//
// Nothing changes until the user answers: "switch" saves the regional Hub, "keep" records the region as
// dismissed so the prompt does not return for it. Both answers go through the same settings save.
// 用户回答前不做任何更改：“切换”保存对应区域的 Hub，“保留”将该区域记为已拒绝，此后不再为它提示。
// 两种回答都走同一个设置保存入口。
export function LuaSkillsHubRegionNotice(props: {
  // Committed LuaSkills settings document.
  // 已提交的 LuaSkills 设置文档。
  settings: LuaSkillsRuntimeSettings
  // Saves one complete settings document.
  // 保存一份完整设置文档。
  onSave: (settings: LuaSkillsRuntimeSettings) => Promise<void>
}) {
  const { t } = useI18n()
  // account supplies the signed-in OpenVulcan region; the notice stays hidden while signed out.
  // account 提供已登录的 OpenVulcan 区域；未登录时提示保持隐藏。
  const account = React.useContext(OpenVulcanAccountContext)
  const region = account?.snapshot?.connected ? account.snapshot.region : null
  const target = hubRegionPromptTarget(props.settings.network, region)
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)

  if (!target) return null

  // answer saves the user's choice for this region.
  // answer 保存用户对该区域的选择。
  const answer = async (switchHub: boolean) => {
    setSaving(true)
    setError(null)
    try {
      await props.onSave(withNetwork(props.settings, switchHub
        ? { officialSkillHubBaseUrl: LUASKILLS_HUB_SITE_URLS[target] }
        : { dismissedHubRegionPrompts: [...props.settings.network.dismissedHubRegionPrompts, target] }))
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      setSaving(false)
    }
  }

  return (
    <PageNotice
      tone="info"
      title={t(target === "china" ? "skills.luaSources.regionPromptChina" : "skills.luaSources.regionPromptGlobal")}
      details={error ?? undefined}
      detailsLabel={error ? t("skills.lua.errorDetails") : undefined}
      actions={(
        <>
          <Button size="sm" disabled={saving} onClick={() => void answer(true)}>
            {t("skills.luaSources.regionPromptSwitch")}
          </Button>
          <Button size="sm" variant="outline" disabled={saving} onClick={() => void answer(false)}>
            {t("skills.luaSources.regionPromptKeep")}
          </Button>
        </>
      )}
    >
      {formatMessage(t("skills.luaSources.regionPromptDetail"), {
        current: effectiveHubBaseUrl(props.settings.network),
        target: LUASKILLS_HUB_SITE_URLS[target],
      })}
    </PageNotice>
  )
}
