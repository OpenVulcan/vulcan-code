import * as React from "react";
import {
  ChevronDown,
  FileText,
  Power,
  RotateCw,
  Trash2,
  Wrench,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { formatSkillText } from "@/components/features/featureText";
import { useI18n } from "@/i18n";
import type { LuaSkillSummary, LuaSkillToolSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";

// LuaSkillCard renders one Lua Skill package and its tool summary.
// LuaSkillCard 渲染单个 Lua Skill 包及其工具摘要。
export function LuaSkillCard(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps;
  // Whether another skill action is running.
  // 是否已有其他技能动作正在运行。
  busy: boolean;
  // Opens the details dialog for this Lua Skill.
  // 打开当前 Lua Skill 的详情弹窗。
  onOpenDetail: (skill: LuaSkillSummary) => void;
  // Opens the tools dialog for this Lua Skill.
  // 打开当前 Lua Skill 的工具弹窗。
  onOpenTools: (skill: LuaSkillSummary) => void;
  // Runs a labeled async action with page-level busy state.
  // 使用页面级忙碌状态运行带标签的异步动作。
  onRunAction: (label: string, action: () => Promise<void>) => Promise<void>;
  // Lua Skill row rendered by this card.
  // 当前卡片渲染的 Lua Skill 行。
  skill: LuaSkillSummary;
}) {
  const { t } = useI18n();
  const source = props.skill.sourceLocator ?? t("skills.source.unknown");
  const sourceTypeLabel = (props.skill.sourceType ?? "source").toUpperCase();
  const explicitToolCount = props.skill.tools.filter((tool) => tool.registrationState === "explicit").length;
  const implicitToolCount = props.skill.tools.filter((tool) => tool.registrationState === "implicit").length;
  const disabledToolCount = props.skill.tools.filter((tool) => tool.registrationState === "disabled").length;

  return (
    <article className="skill-card lua-skill-card">
      <header className="skill-card-header">
        <div className="skill-card-title">
          <div className="skill-card-title-line">
            <strong>{props.skill.displayName}</strong>
            <div className="skill-card-title-meta">
              <Badge variant="secondary">v{props.skill.version}</Badge>
            </div>
          </div>
          <div className="skill-card-badges">
            {props.skill.enabled ? (
              <Badge className="skill-card-enabled-badge">{t("skills.status.enabled")}</Badge>
            ) : (
              <Badge variant="destructive">{t("skills.status.stopped")}</Badge>
            )}
            <Badge variant="outline">{sourceTypeLabel}</Badge>
          </div>
        </div>
      </header>
      <div className="skill-card-summary">
        <span>{source}</span>
      </div>
      <div className="skill-card-metrics">
        <span>{props.skill.rootName === "ROOT" ? t("skills.label.system") : props.skill.rootName}</span>
        {props.skill.enabled ? (
          <>
            <span>{t("skills.label.explicit")} {explicitToolCount}</span>
            <span>{t("skills.label.implicit")} {implicitToolCount}</span>
            {disabledToolCount > 0 ? <span>{t("skills.label.closed")} {disabledToolCount}</span> : null}
          </>
        ) : null}
      </div>
      <div className="skill-actions-row">
        <button className="skill-action-button secondary" onClick={() => props.onOpenDetail(props.skill)} type="button">
          <FileText size={15} />
          <span>{t("skills.action.details")}</span>
        </button>
        <button className="skill-action-button secondary" onClick={() => props.onOpenTools(props.skill)} type="button">
          <Wrench size={15} />
          <span>{t("skills.action.tools")} {props.skill.tools.length}</span>
        </button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button className="skill-action-button secondary" disabled={props.busy} type="button">
              <span>{t("skills.action.action")}</span>
              <ChevronDown size={14} />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="lua-skill-action-menu">
            <DropdownMenuItem onSelect={() => void props.onRunAction("toggle", () => props.actions.onSetLuaSkillEnabled(props.skill.skillId, !props.skill.enabled))}>
              <Power size={15} />
              <span>{props.skill.enabled ? t("skills.action.disable") : t("skills.action.enable")}</span>
            </DropdownMenuItem>
            {props.skill.managed ? (
              <DropdownMenuItem onSelect={() => void props.onRunAction("update", () => props.actions.onUpdateLuaSkill(props.skill.skillId))}>
                <RotateCw size={15} />
                <span>{t("skills.action.update")}</span>
              </DropdownMenuItem>
            ) : null}
            {!props.skill.systemOwned ? (
              <DropdownMenuItem variant="destructive" onSelect={() => void props.onRunAction("uninstall", () => props.actions.onUninstallLuaSkill(props.skill.skillId))}>
                <Trash2 size={15} />
                <span>{t("skills.action.delete")}</span>
              </DropdownMenuItem>
            ) : null}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </article>
  );
}

// LuaSkillDetailDialog renders package metadata for one Lua Skill.
// LuaSkillDetailDialog 渲染单个 Lua Skill 的包元数据。
export function LuaSkillDetailDialog(props: {
  // Whether the details dialog is open.
  // 详情弹窗当前是否打开。
  open: boolean;
  // Lua Skill selected for details.
  // 当前查看详情的 Lua Skill。
  skill: LuaSkillSummary | null;
  // Updates the details dialog open state.
  // 更新详情弹窗打开状态。
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useI18n();
  const skill = props.skill;
  const source = skill?.sourceLocator ? `${skill.sourceType ?? "source"} · ${skill.sourceLocator}` : t("skills.source.unknown");
  return (
    <Dialog open={props.open && skill !== null} onOpenChange={props.onOpenChange}>
      <DialogContent className="lua-skill-detail-dialog">
        <DialogHeader>
          <DialogTitle>{skill?.displayName ?? t("skills.lua.detailsTitle")}</DialogTitle>
          <DialogDescription>{skill ? `${skill.skillId} · v${skill.version}` : t("skills.lua.detailsDescription")}</DialogDescription>
        </DialogHeader>
        {skill ? (
          <div className="lua-skill-detail-grid">
            <div>
              <span>{t("skills.label.currentStatus")}</span>
              <strong>{skill.enabled ? t("skills.status.enabled") : t("skills.status.stopped")}</strong>
            </div>
            <div>
              <span>{t("skills.label.installLayer")}</span>
              <strong>{skill.rootName === "ROOT" ? t("skills.label.system") : skill.rootName}</strong>
            </div>
            <div>
              <span>{t("skills.label.installManagement")}</span>
              <strong>{skill.managed ? t("skills.label.managedInstall") : t("skills.label.unmanagedInstall")}</strong>
            </div>
            <div>
              <span>{t("skills.label.runtimeEntries")}</span>
              <strong>{skill.entryCount}</strong>
            </div>
            <div className="wide">
              <span>{t("skills.label.source")}</span>
              <strong>{source}</strong>
            </div>
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

// LuaSkillToolsDialog renders runtime tool registration controls in a dialog.
// LuaSkillToolsDialog 在弹窗中渲染运行时工具注册控件。
export function LuaSkillToolsDialog(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps;
  // Whether another skill action is running.
  // 是否已有其他技能动作正在运行。
  busy: boolean;
  // Whether the tools dialog is open.
  // 工具弹窗当前是否打开。
  open: boolean;
  // Lua Skill selected for tool management.
  // 当前管理工具的 Lua Skill。
  skill: LuaSkillSummary | null;
  // Updates the tools dialog open state.
  // 更新工具弹窗打开状态。
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useI18n();
  const skill = props.skill;
  return (
    <Dialog open={props.open && skill !== null} onOpenChange={props.onOpenChange}>
      <DialogContent className="lua-skill-tools-dialog">
        <DialogHeader>
          <DialogTitle>{skill ? `${skill.displayName} ${t("skills.action.tools")}` : t("skills.lua.toolsTitle")}</DialogTitle>
          <DialogDescription>{skill ? `${skill.skillId} · ${skill.tools.length} ${t("skills.label.runtimeTools")}` : t("skills.lua.toolsDescription")}</DialogDescription>
        </DialogHeader>
        <div className="skill-tool-list dialog-tool-list">
          {skill && skill.tools.length === 0 ? (
            <p>{t("skills.lua.toolsEmpty")}</p>
          ) : skill?.tools.map((tool) => (
            <LuaSkillToolRow
              actions={props.actions}
              busy={props.busy}
              key={`${skill.skillId}-${tool.localName}`}
              skillId={skill.skillId}
              tool={tool}
            />
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}

// LuaSkillToolRow renders one runtime tool registration selector.
// LuaSkillToolRow 渲染单个运行时工具注册选择器。
function LuaSkillToolRow(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps;
  // Whether another skill action is running.
  // 是否已有其他技能动作正在运行。
  busy: boolean;
  // Parent Lua Skill identifier.
  // 父级 Lua Skill 标识。
  skillId: string;
  // Runtime tool row.
  // 运行时工具行。
  tool: LuaSkillToolSummary;
}) {
  const { t } = useI18n();
  // selectedState mirrors the select value immediately before backend synchronization completes.
  // selectedState 在后端同步完成前立即镜像选择框取值。
  const [selectedState, setSelectedState] = React.useState(props.tool.registrationState);
  // syncing tracks the background persistence request for this single tool row.
  // syncing 跟踪当前工具行的后台持久化请求。
  const [syncing, setSyncing] = React.useState(false);

  React.useEffect(() => {
    setSelectedState(props.tool.registrationState);
  }, [props.tool.registrationState]);

  // handleRegistrationChange applies an optimistic local switch and reverts on persistence failure.
  // handleRegistrationChange 先执行乐观本地切换，并在持久化失败时回滚。
  const handleRegistrationChange = React.useCallback(async (nextState: LuaSkillToolSummary["registrationState"]) => {
    const previousState = selectedState;
    if (nextState === previousState || syncing || props.busy) {
      return;
    }
    setSelectedState(nextState);
    setSyncing(true);
    try {
      await props.actions.onSetLuaSkillToolRegistrationState(props.skillId, props.tool.localName, nextState);
    } catch (error) {
      setSelectedState(previousState);
      console.warn("sync LuaSkill tool registration state failed", error);
    } finally {
      setSyncing(false);
    }
  }, [props.actions, props.busy, props.skillId, props.tool.localName, selectedState, syncing]);

  return (
    <article className="skill-tool-row">
      <div className="skill-tool-row-header">
        <div>
          <strong>{props.tool.localName}</strong>
          <span>{props.tool.canonicalName}</span>
        </div>
        <div className="skill-tool-control">
          <div className="skill-tool-segmented" role="group" aria-label={formatSkillText(t("skills.lua.toolModeAria"), { name: props.tool.localName })}>
            <button
              className={selectedState === "explicit" ? "active" : ""}
              disabled={props.busy || syncing}
              onClick={() => handleRegistrationChange("explicit")}
              type="button"
            >
              {t("skills.lua.toolExplicit")}
            </button>
            <button
              className={selectedState === "implicit" ? "active" : ""}
              disabled={props.busy || syncing}
              onClick={() => handleRegistrationChange("implicit")}
              type="button"
            >
              {t("skills.lua.toolImplicit")}
            </button>
            <button
              className={selectedState === "disabled" ? "active" : ""}
              disabled={props.busy || syncing}
              onClick={() => handleRegistrationChange("disabled")}
              type="button"
            >
              {t("skills.lua.toolDisabled")}
            </button>
          </div>
          {syncing ? <small className="skill-sync-status">{t("skills.lua.syncing")}</small> : null}
        </div>
      </div>
      <div className="skill-tool-description">
        <p>{props.tool.description || props.tool.registrationHelp}</p>
      </div>
    </article>
  );
}
