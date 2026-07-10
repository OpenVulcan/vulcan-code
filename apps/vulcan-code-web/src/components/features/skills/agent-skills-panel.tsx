import * as React from "react"
import { FileText, Power } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { StatusPill } from "@/components/shared/status"
import { useI18n } from "@/i18n"
import { localMediaSourceUrl } from "@/resources/localMedia"
import type { AgentSkillSummary } from "@/types"

import { localizedAgentSkillScope, type SkillActionProps } from "./skill-area-types"

// AgentSkillsPanel renders prompt-based skill controls and detail previews.
// AgentSkillsPanel 渲染基于提示词的技能控件与详情预览。
export function AgentSkillsPanel(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps
  // Non-fatal loading errors.
  // 非致命加载错误。
  errors: Array<{ path: string; message: string }>
  // Agent Skill rows.
  // Agent Skill 行。
  skills: AgentSkillSummary[]
}) {
  const { t } = useI18n()
  const [busyAction, setBusyAction] = React.useState<string | null>(null)
  // actionRunningRef closes the synchronous repeat-click window before React commits busy state.
  // actionRunningRef 在 React 提交忙碌状态前关闭同步重复点击窗口。
  const actionRunningRef = React.useRef(false)
  // detailSkill stores the Agent Skill selected for the details dialog.
  // detailSkill 保存当前打开详情弹窗的 Agent Skill。
  const [detailSkill, setDetailSkill] = React.useState<AgentSkillSummary | null>(null)
  // runAction executes one page-exclusive Agent Skills mutation.
  // runAction 执行单个页面互斥的 Agent Skills 变更。
  //
  // Parameters: label identifies the busy indicator and action performs the service mutation.
  // 参数：label 标识忙碌提示，action 执行服务变更。
  // Returns: a promise that settles after the accepted action releases the synchronous guard.
  // 返回值：在已接受动作释放同步守卫后结束的 Promise。
  const runAction = React.useCallback(async (label: string, action: () => Promise<void>) => {
    if (actionRunningRef.current) {
      return
    }
    actionRunningRef.current = true
    setBusyAction(label)
    try {
      await action()
    } finally {
      actionRunningRef.current = false
      setBusyAction(null)
    }
  }, [])

  return (
    <div className="skill-page-body agent-skill-page-body">
      {props.errors.length > 0 ? (
        <div className="skill-warning-list">
          {props.errors.map((error) => (
            <article className="skill-warning-row" key={`${error.path}-${error.message}`}>
              <strong>{error.path}</strong>
              <span>{error.message}</span>
            </article>
          ))}
        </div>
      ) : null}
      <div className="skill-card-list agent-skill-card-list">
        {props.skills.length === 0 ? (
          <article className="skill-empty-card">
            <StatusPill status="pending" label="pending" />
            <strong>{t("skills.agent.emptyTitle")}</strong>
            <span>{t("skills.agent.emptyDetail")}</span>
          </article>
        ) : props.skills.map((skill) => (
          <AgentSkillCard
            actions={props.actions}
            busy={busyAction !== null}
            key={skill.name}
            onOpenDetail={setDetailSkill}
            onRunAction={runAction}
            skill={skill}
          />
        ))}
      </div>
      <AgentSkillDetailDialog
        open={detailSkill !== null}
        skill={detailSkill}
        onOpenChange={(open) => {
          if (!open) {
            setDetailSkill(null)
          }
        }}
      />
    </div>
  )
}

// AgentSkillCard renders one prompt skill and optional body details.
// AgentSkillCard 渲染单个提示词技能及可选正文详情。
function AgentSkillCard(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps
  // Whether another skill action is running.
  // 是否已有其他技能动作正在运行。
  busy: boolean
  // Opens the details dialog for this Agent Skill.
  // 打开当前 Agent Skill 的详情弹窗。
  onOpenDetail: (skill: AgentSkillSummary) => void
  // Runs a labeled async action with page-level busy state.
  // 使用页面级忙碌状态运行带标签的异步动作。
  onRunAction: (label: string, action: () => Promise<void>) => Promise<void>
  // Agent Skill row rendered by this card.
  // 当前卡片渲染的 Agent Skill 行。
  skill: AgentSkillSummary
}) {
  const { t } = useI18n()
  const fallbackIcon = props.skill.displayName.trim().charAt(0).toUpperCase() || "*"
  const scopeLabel = localizedAgentSkillScope(props.skill.scope, t)

  return (
    <article className="skill-card agent-skill-card">
      <header className="skill-card-header">
        <div className="agent-skill-title">
          <AgentSkillIcon fallback={fallbackIcon} iconPath={props.skill.iconPath} />
          <div>
            <strong>{props.skill.displayName}</strong>
            <div className="skill-card-badges">
              {props.skill.enabled ? (
                <Badge className="skill-card-enabled-badge">{t("skills.status.enabled")}</Badge>
              ) : (
                <Badge variant="destructive">{t("skills.status.stopped")}</Badge>
              )}
              <Badge variant="outline">{scopeLabel}</Badge>
            </div>
          </div>
        </div>
      </header>
      <p>{props.skill.description}</p>
      <div className="skill-actions-row">
        <button className="skill-action-button" disabled={props.busy} onClick={() => props.onRunAction("agent-toggle", () => props.actions.onSetAgentSkillEnabled(props.skill.name, !props.skill.enabled))} type="button">
          <Power size={15} />
          <span>{props.skill.enabled ? t("skills.action.disable") : t("skills.action.enable")}</span>
        </button>
        <button className="skill-action-button secondary" onClick={() => props.onOpenDetail(props.skill)} type="button">
          <FileText size={15} />
          <span>{t("skills.action.details")}</span>
        </button>
      </div>
    </article>
  )
}

// AgentSkillIcon renders the resolved Agent Skill icon with a text fallback.
// AgentSkillIcon 渲染解析后的 Agent Skill 图标，并提供文本兜底。
function AgentSkillIcon(props: {
  // Fallback text used when the icon cannot be loaded.
  // 图标无法加载时使用的兜底文本。
  fallback: string
  // Optional local icon path resolved by the service.
  // 服务端解析出的可选本地图标路径。
  iconPath?: string | null
}) {
  // failed tracks image loading failure so the fallback stays visible.
  // failed 跟踪图片加载失败以持续展示兜底。
  const [failed, setFailed] = React.useState(false)
  React.useEffect(() => {
    setFailed(false)
  }, [props.iconPath])

  if (!props.iconPath || failed) {
    return <span className="agent-skill-icon fallback">{props.fallback}</span>
  }
  return (
    <span className="agent-skill-icon">
      <img alt="" src={localMediaSourceUrl(props.iconPath)} onError={() => setFailed(true)} />
    </span>
  )
}

// AgentSkillDetailDialog renders Agent Skill metadata and Markdown body in a dialog.
// AgentSkillDetailDialog 在弹窗中渲染 Agent Skill 元数据与 Markdown 正文。
function AgentSkillDetailDialog(props: {
  // Whether the details dialog is open.
  // 详情弹窗当前是否打开。
  open: boolean
  // Agent Skill selected for details.
  // 当前查看详情的 Agent Skill。
  skill: AgentSkillSummary | null
  // Updates the details dialog open state.
  // 更新详情弹窗打开状态。
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useI18n()
  const skill = props.skill
  const scopeLabel = skill ? localizedAgentSkillScope(skill.scope, t) : ""
  return (
    <Dialog open={props.open && skill !== null} onOpenChange={props.onOpenChange}>
      <DialogContent className="agent-skill-detail-dialog">
        <DialogHeader>
          <DialogTitle>{skill?.displayName ?? t("skills.agent.detailsTitle")}</DialogTitle>
          <DialogDescription>{skill ? `${scopeLabel} · ${skill.enabled ? t("skills.status.enabled") : t("skills.status.stopped")}` : t("skills.agent.detailsDescription")}</DialogDescription>
        </DialogHeader>
        {skill ? (
          <div className="agent-skill-detail-body">
            <div className="lua-skill-detail-grid">
              <div>
                <span>{t("skills.label.currentStatus")}</span>
                <strong>{skill.enabled ? t("skills.status.enabled") : t("skills.status.stopped")}</strong>
              </div>
              <div>
                <span>{t("skills.label.scope")}</span>
                <strong>{scopeLabel}</strong>
              </div>
              <div className="wide">
                <span>{t("skills.label.skillFile")}</span>
                <strong>{skill.skillFile}</strong>
              </div>
              <div className="wide">
                <span>{t("skills.label.skillDir")}</span>
                <strong>{skill.baseDir}</strong>
              </div>
              {skill.iconPath ? (
                <div className="wide">
                  <span>{t("skills.label.iconPath")}</span>
                  <strong>{skill.iconPath}</strong>
                </div>
              ) : null}
            </div>
            <pre className="skill-body-preview">{skill.body}</pre>
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  )
}
