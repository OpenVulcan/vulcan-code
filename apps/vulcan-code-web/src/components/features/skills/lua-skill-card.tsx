import * as React from "react";
import {
  ChevronDown,
  Package,
  RotateCw,
  Settings2,
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

// hasLuaSkillConfigOptions determines whether one catalog row exposes actual package configuration options.
// hasLuaSkillConfigOptions 判断单个目录行是否暴露实际技能包配置选项。
//
// The skill parameter supplies the projected configuration summary.
// skill 参数提供投影后的配置摘要。
//
// Returns true only when the package declares at least one configuration item.
// 仅当技能包至少声明一个配置项时返回 true。
export function hasLuaSkillConfigOptions(skill: Pick<LuaSkillSummary, "config">): boolean {
  return Boolean(skill.config && skill.config.itemCount > 0);
}

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
  // Opens the package configuration dialog for this Lua Skill.
  // 打开当前 Lua Skill 的技能包配置弹窗。
  onOpenConfig: (skill: LuaSkillSummary) => void;
  // Runs a labeled async action with page-level busy state.
  // 使用页面级忙碌状态运行带标签的异步动作。
  onRunAction: (label: string, action: () => Promise<void>) => Promise<void>;
  // Lua Skill row rendered by this card.
  // 当前卡片渲染的 Lua Skill 行。
  skill: LuaSkillSummary;
}) {
  const { t } = useI18n();
  // source is the compact source description shown below the unified label line.
  // source 是统一标签行下方显示的紧凑来源说明。
  const source = props.skill.sourceLocator ?? t("skills.source.unknown");
  // sourceTypeLabel is the normalized installation source badge.
  // sourceTypeLabel 是规范化后的安装来源标签。
  const sourceTypeLabel = (props.skill.sourceType ?? "source").toUpperCase();
  // explicitToolCount counts tools registered directly with the provider.
  // explicitToolCount 统计直接注册到 Provider 的工具。
  const explicitToolCount = props.skill.tools.filter((tool) => tool.registrationState === "explicit").length;
  // implicitToolCount counts tools available through the aggregate LuaSkills entry.
  // implicitToolCount 统计通过 LuaSkills 聚合入口提供的工具。
  const implicitToolCount = props.skill.tools.filter((tool) => tool.registrationState === "implicit").length;
  // disabledToolCount counts tools explicitly hidden from the model.
  // disabledToolCount 统计被明确隐藏于模型的工具。
  const disabledToolCount = props.skill.tools.filter((tool) => tool.registrationState === "disabled").length;
  // hasConfigOptions controls both configuration status and the configuration action.
  // hasConfigOptions 同时控制配置状态与配置操作入口。
  const hasConfigOptions = hasLuaSkillConfigOptions(props.skill);

  return (
    <article className="agent-definition-row lua-skill-definition-row">
      <button
        className="agent-definition-row-icon"
        onClick={() => props.onOpenDetail(props.skill)}
        title={t("skills.action.details")}
        type="button"
      >
        <span className="agent-definition-row-glyph"><Package size={17} /></span>
      </button>
      <div className="agent-definition-row-main">
        <div className="agent-definition-row-title-line">
          <button
            className="agent-definition-title-button"
            onClick={() => props.onOpenDetail(props.skill)}
            title={t("skills.action.details")}
            type="button"
          >
            {props.skill.displayName}
          </button>
          <div className="agent-definition-row-badges">
            <Badge variant="outline">{sourceTypeLabel}</Badge>
            <Badge variant="secondary">v{props.skill.version}</Badge>
            <Badge variant="outline">{props.skill.rootName === "ROOT" ? t("skills.label.system") : props.skill.rootName}</Badge>
            <Badge variant="outline">{t("skills.label.explicit")} {explicitToolCount}</Badge>
            <Badge variant="outline">{t("skills.label.implicit")} {implicitToolCount}</Badge>
            {disabledToolCount > 0 ? <Badge variant="outline">{t("skills.label.closed")} {disabledToolCount}</Badge> : null}
            {hasConfigOptions && props.skill.config ? (
              <Badge variant={props.skill.config.complete ? "secondary" : "outline"}>
                {props.skill.config.complete ? t("skills.luaConfig.complete") : t("skills.luaConfig.incomplete")}
              </Badge>
            ) : null}
          </div>
        </div>
        <p>{source}</p>
        <span className="agent-definition-row-path">{props.skill.skillDir}</span>
      </div>
      <div className="agent-definition-row-actions">
        <button
          aria-checked={props.skill.enabled}
          aria-label={props.skill.enabled ? t("skills.action.disable") : t("skills.action.enable")}
          className={`agent-definition-switch ${props.skill.enabled ? "on" : ""}`}
          disabled={props.busy}
          onClick={() => void props.onRunAction("toggle", () => props.actions.onSetLuaSkillEnabled(props.skill.skillId, !props.skill.enabled))}
          role="switch"
          title={props.skill.enabled ? t("skills.action.disable") : t("skills.action.enable")}
          type="button"
        >
          <span />
        </button>
        <button
          className="agent-definition-icon-button"
          onClick={() => props.onOpenTools(props.skill)}
          title={`${t("skills.action.tools")} (${props.skill.tools.length})`}
          type="button"
        >
          <Wrench size={14} />
        </button>
        {hasConfigOptions ? (
          <button
            className="agent-definition-icon-button"
            onClick={() => props.onOpenConfig(props.skill)}
            title={t("skills.luaConfig.title")}
            type="button"
          >
            <Settings2 size={14} />
          </button>
        ) : null}
        {props.skill.managed ? (
          <button
            className="agent-definition-icon-button"
            disabled={props.busy}
            onClick={() => void props.onRunAction("update", () => props.actions.onUpdateLuaSkill(props.skill.skillId))}
            title={t("skills.action.update")}
            type="button"
          >
            <RotateCw size={14} />
          </button>
        ) : null}
        {!props.skill.systemOwned ? (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button className="agent-definition-icon-button" disabled={props.busy} title={t("skills.action.action")} type="button">
                <ChevronDown size={14} />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="lua-skill-action-menu">
              <DropdownMenuItem variant="destructive" onSelect={() => void props.onRunAction("uninstall", () => props.actions.onUninstallLuaSkill(props.skill.skillId))}>
                <Trash2 size={15} />
                <span>{t("skills.action.delete")}</span>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        ) : null}
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
