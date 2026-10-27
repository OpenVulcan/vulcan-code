import * as React from "react"

import { Button } from "@/ui-kit/primitives/button"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog"
import { formatMessage, useI18n } from "@/i18n"
import type { LuaSkillSummary } from "@/types"
import type { LuaSkillReplacementSource } from "@/service/skills-actions-service"

import { LuaSkillHubPicker } from "./lua-skill-hub-picker"

// LuaSkillChangeSourceType lists the install sources an update may switch to.
// LuaSkillChangeSourceType 列出更新时可切换到的安装来源。
type LuaSkillChangeSourceType = "official_hub" | "github"

// LuaSkillChangeSourceDialog picks a new install source for one managed skill.
// LuaSkillChangeSourceDialog 为一个受管技能选择新的安装来源。
//
// Confirming hands the choice to the caller, which runs it as an update so the shared progress dialog
// reports it; the skill keeps its identifier and only the recorded source changes.
// 确认后将选择交给调用方，由调用方以更新的形式执行，从而由共享进度弹窗报告；技能标识保持不变，
// 只改变已记录的来源。
export function LuaSkillChangeSourceDialog(props: {
  // Skill whose source is being changed, or null when closed.
  // 正在更换来源的技能；关闭时为 null。
  skill: LuaSkillSummary | null
  // Closes the dialog without changes.
  // 不做更改地关闭弹窗。
  onClose: () => void
  // Starts the source-changing update.
  // 启动更换来源的更新。
  onConfirm: (skill: LuaSkillSummary, replacement: LuaSkillReplacementSource) => void
}) {
  const { t } = useI18n()
  const [sourceType, setSourceType] = React.useState<LuaSkillChangeSourceType>("official_hub")
  const [source, setSource] = React.useState("")
  const formId = React.useId()
  const skillId = props.skill?.skillId ?? null

  React.useEffect(() => {
    if (skillId !== null) {
      setSourceType("official_hub")
      setSource("")
    }
  }, [skillId])

  return (
    <Dialog open={props.skill !== null} onOpenChange={(open) => { if (!open) props.onClose() }}>
      <DialogContent size="md" scrollMode="fixed-footer">
        <DialogHeader>
          <DialogTitle>{t("skills.luaSources.changeSourceTitle")}</DialogTitle>
          <DialogDescription>
            {props.skill ? formatMessage(t("skills.luaSources.changeSourceDescription"), {
              displayName: props.skill.displayName,
              source: props.skill.sourceLocator ?? t("skills.source.unknown"),
            }) : ""}
          </DialogDescription>
        </DialogHeader>
        <form
          id={formId}
          className="lua-skill-install-form vk-dialog-body"
          onSubmit={(event) => {
            event.preventDefault()
            if (props.skill && source.trim()) props.onConfirm(props.skill, { sourceType, source: source.trim() })
          }}
        >
          <label className="lua-skill-install-field">
            <span>{t("skills.lua.sourceType")}</span>
            <Select value={sourceType} onValueChange={(value) => { setSourceType(value as LuaSkillChangeSourceType); setSource("") }}>
              <SelectTrigger className="vk-fill"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="official_hub">{t("skills.lua.sourceHub")}</SelectItem>
                <SelectItem value="github">GitHub</SelectItem>
              </SelectContent>
            </Select>
          </label>
          {sourceType === "official_hub" ? (
            <LuaSkillHubPicker disabled={false} selectedSkillId={source} onSelect={setSource} />
          ) : (
            <label className="lua-skill-install-field">
              <span>{t("skills.lua.installSource")}</span>
              <input
                autoFocus
                placeholder={t("skills.lua.placeholderGithub")}
                value={source}
                onChange={(event) => setSource(event.target.value)}
              />
            </label>
          )}
        </form>
        <DialogFooter>
          <Button onClick={props.onClose} type="button" variant="secondary">{t("skills.action.cancel")}</Button>
          <Button disabled={source.trim().length === 0} form={formId} type="submit">{t("skills.luaSources.changeSourceConfirm")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
