import * as React from "react";
import { PageNotice } from "@/components/page";
import {
  ChevronDown,
  Package,
  RotateCw,
  Settings2,
  Trash2,
  Wrench,
} from "lucide-react";
import { Badge } from "@/ui-kit/primitives/badge";
import { IconButton } from "@/ui-kit/primitives/button";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { Switch } from "@/ui-kit/primitives/switch";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";
import { formatSkillText } from "@/components/features/featureText";
import { useI18n } from "@/i18n";
import type { LuaSkillSummary, LuaSkillToolSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";

// SKILL_SOURCE_TYPE_NAMES spells install source kinds the way their owners write them; the kinds are protocol
// identifiers and proper names, so they are the same in every language.
// SKILL_SOURCE_TYPE_NAMES 按来源方自己的写法拼写安装来源类型；这些类型是协议标识与专有名称，因此各语言相同。
const SKILL_SOURCE_TYPE_NAMES: Readonly<Record<string, string>> = { github: "GitHub", officialhub: "LuaSkills Hub", url: "URL" };

// skillSourceTypeLabel names a recorded install source kind, or returns null when none was recorded.
// skillSourceTypeLabel 给出已记录安装来源类型的名称；未记录时返回 null。
function skillSourceTypeLabel(sourceType: string | null | undefined): string | null {
  const kind = sourceType?.trim();
  return kind ? SKILL_SOURCE_TYPE_NAMES[kind.toLowerCase()] ?? kind : null;
}

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
  // Requests shared confirmation before removing this user-owned package.
  // 删除此用户包之前请求共享确认。
  onRequestUninstall: (skill: LuaSkillSummary) => void;
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
  const sourceTypeLabel = skillSourceTypeLabel(props.skill.sourceType);
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
            {sourceTypeLabel ? <Badge variant="outline">{sourceTypeLabel}</Badge> : null}
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
        <Switch
          checked={props.skill.enabled}
          aria-label={props.skill.enabled ? t("skills.action.disable") : t("skills.action.enable")}
          disabled={props.busy}
          onCheckedChange={() => void props.onRunAction("toggle", () => props.actions.onSetLuaSkillEnabled(props.skill.skillId, !props.skill.enabled))}
          title={props.skill.enabled ? t("skills.action.disable") : t("skills.action.enable")}
        />
        <IconButton
          aria-label={`${t("skills.action.tools")} (${props.skill.tools.length})`}
          onClick={() => props.onOpenTools(props.skill)}
          title={`${t("skills.action.tools")} (${props.skill.tools.length})`}
          type="button"
        >
          <Wrench size={14} />
        </IconButton>
        {hasConfigOptions ? (
          <IconButton
            aria-label={t("skills.luaConfig.title")}
            onClick={() => props.onOpenConfig(props.skill)}
            title={t("skills.luaConfig.title")}
            type="button"
          >
            <Settings2 size={14} />
          </IconButton>
        ) : null}
        {props.skill.managed ? (
          <IconButton
            aria-label={t("skills.action.update")}
            disabled={props.busy}
            onClick={() => void props.onRunAction("update", () => props.actions.onUpdateLuaSkill(props.skill.skillId))}
            title={t("skills.action.update")}
            type="button"
          >
            <RotateCw size={14} />
          </IconButton>
        ) : null}
        {!props.skill.systemOwned ? (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <IconButton aria-label={t("skills.action.action")} disabled={props.busy} title={t("skills.action.action")} type="button">
                <ChevronDown size={14} />
              </IconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="lua-skill-action-menu">
              <DropdownMenuItem variant="destructive" onSelect={() => props.onRequestUninstall(props.skill)}>
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
  const source = skill?.sourceLocator ? [skillSourceTypeLabel(skill.sourceType), skill.sourceLocator].filter(Boolean).join(" · ") : t("skills.source.unknown");
  return (
    <Dialog open={props.open && skill !== null} onOpenChange={props.onOpenChange}>
      <DialogContent className="lua-skill-detail-dialog" size="md" scrollMode="body-scroll">
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
  // writingRef protects this dialog until its accepted registration write settles.
  // writingRef 在已接受的注册写入结束前保护此弹窗。
  const writingRef = React.useRef(false);
  const [writing, setWriting] = React.useState(false);
  // onWritingChange synchronously guards dismissal and disables all sibling tool controls.
  // onWritingChange 同步保护关闭，并禁用所有同级工具控件。
  const onWritingChange = React.useCallback((value: boolean) => {
    if (value && writingRef.current) return false;
    writingRef.current = value;
    setWriting(value);
    return true;
  }, []);
  return (
    <Dialog open={props.open && skill !== null} onOpenChange={(open) => { if (!writingRef.current) props.onOpenChange(open); }}>
      <DialogContent className="lua-skill-tools-dialog" size="lg" scrollMode="content-scroll">
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
              busy={props.busy || writing}
              onWritingChange={onWritingChange}
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
  // Reports the accepted write lifetime to the containing dialog.
  // 将已接受写入的生命周期报告给所属弹窗。
  onWritingChange: (writing: boolean) => boolean;
}) {
  const { t } = useI18n();
  // selectedState mirrors the select value immediately before backend synchronization completes.
  // selectedState 在后端同步完成前立即镜像选择框取值。
  const [selectedState, setSelectedState] = React.useState(props.tool.registrationState);
  // syncing tracks the background persistence request for this single tool row.
  // syncing 跟踪当前工具行的后台持久化请求。
  const [syncing, setSyncing] = React.useState(false);
  // syncingRef rejects repeat writes before the busy render; persistedState tracks current host authority.
  // syncingRef 在忙碌渲染前拒绝重复写入；persistedState 跟踪当前宿主权威。
  const syncingRef = React.useRef(false);
  const persistedState = React.useRef(props.tool.registrationState);
  persistedState.current = props.tool.registrationState;
  // syncError presents failed registration without hiding the retained current state.
  // syncError 展示注册失败，不隐藏保留的当前状态。
  const [syncError, setSyncError] = React.useState("");

  React.useEffect(() => {
    setSelectedState(props.tool.registrationState);
  }, [props.tool.registrationState]);

  // handleRegistrationChange applies an optimistic local switch and reverts on persistence failure.
  // handleRegistrationChange 先执行乐观本地切换，并在持久化失败时回滚。
  const handleRegistrationChange = React.useCallback(async (nextState: LuaSkillToolSummary["registrationState"]) => {
    const previousState = selectedState;
    if (nextState === previousState || syncingRef.current || props.busy) {
      return;
    }
    if (!props.onWritingChange(true)) return;
    setSelectedState(nextState);
    syncingRef.current = true;
    setSyncing(true);
    setSyncError("");
    try {
      await props.actions.onSetLuaSkillToolRegistrationState(props.skillId, props.tool.localName, nextState);
    } catch (error) {
      setSelectedState(persistedState.current);
      setSyncError(error instanceof Error ? error.message : String(error));
    } finally {
      syncingRef.current = false;
      props.onWritingChange(false);
      setSyncing(false);
    }
  }, [props.actions, props.busy, props.skillId, props.tool.localName, props.onWritingChange, selectedState]);

  return (
    <article className="skill-tool-row">
      <div className="skill-tool-row-header">
        <div>
          <strong>{props.tool.localName}</strong>
          <span>{props.tool.canonicalName}</span>
        </div>
        <div className="skill-tool-control">
          <SegmentedControl
            aria-label={formatSkillText(t("skills.lua.toolModeAria"), { name: props.tool.localName })}
            disabled={props.busy || syncing}
            onValueChange={(value) => {
              if (value) {
                void handleRegistrationChange(value as LuaSkillToolSummary["registrationState"])
              }
            }}
            value={selectedState}
          >
            <SegmentedControlItem value="explicit">
              {t("skills.lua.toolExplicit")}
            </SegmentedControlItem>
            <SegmentedControlItem value="implicit">
              {t("skills.lua.toolImplicit")}
            </SegmentedControlItem>
            <SegmentedControlItem value="disabled">
              {t("skills.lua.toolDisabled")}
            </SegmentedControlItem>
          </SegmentedControl>
          {syncing ? <small className="skill-sync-status">{t("skills.lua.syncing")}</small> : null}
        </div>
      </div>
      <div className="skill-tool-description">
        <p>{props.tool.description || props.tool.registrationHelp}</p>
      </div>
      {syncError ? <PageNotice details={syncError} detailsLabel={t("skills.lua.errorDetails")} tone="error">{t("skills.card.toolStateCouldNot")}</PageNotice> : null}
    </article>
  );
}
