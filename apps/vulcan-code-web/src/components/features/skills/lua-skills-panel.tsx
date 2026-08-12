import * as React from "react";
import { PackagePlus, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  LuaSkillCard,
  LuaSkillDetailDialog,
  LuaSkillToolsDialog,
} from "@/components/features/skills/lua-skill-card";
import { LuaSkillConfigDialog } from "@/components/features/skills/lua-skill-config-dialog";
import { useI18n } from "@/i18n";
import type { LuaSkillSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";

// LuaSkillStatusFilter identifies the enabled-state subset shown in the LuaSkills list.
// LuaSkillStatusFilter 标识 LuaSkills 列表中显示的启停状态子集。
type LuaSkillStatusFilter = "all" | "enabled" | "disabled";

// LuaSkillSourceFilter identifies the host ownership subset shown in the LuaSkills list.
// LuaSkillSourceFilter 标识 LuaSkills 列表中显示的宿主归属子集。
type LuaSkillSourceFilter = "all" | "system" | "user";

// luaSkillMatchesSearch checks one Lua Skill against the exact user-visible searchable fields.
// luaSkillMatchesSearch 使用明确的用户可见字段检查单个 Lua Skill 是否匹配搜索词。
//
// Parameters: skill is the projected Lua Skill row and query is the current free-text filter.
// 参数：skill 是投影后的 Lua Skill 行，query 是当前自由文本筛选词。
// Returns: true when the normalized query is empty or appears in a visible identifier, name, source, root, or directory.
// 返回值：规范化查询为空，或出现在可见标识、名称、来源、层级或目录中时返回 true。
function luaSkillMatchesSearch(skill: LuaSkillSummary, query: string): boolean {
  // normalizedQuery provides case-insensitive matching without altering source values.
  // normalizedQuery 在不改动来源值的情况下提供不区分大小写的匹配。
  const normalizedQuery = query.trim().toLocaleLowerCase();
  if (!normalizedQuery) {
    return true;
  }
  return [
    skill.skillId,
    skill.displayName,
    skill.sourceType ?? "",
    skill.sourceLocator ?? "",
    skill.rootName,
    skill.skillDir,
  ].some((value) => value.toLocaleLowerCase().includes(normalizedQuery));
}

// LuaSkillsPanel renders package lifecycle controls and runtime tool registration controls.
// LuaSkillsPanel 渲染包生命周期控件与运行时工具注册控件。
export function LuaSkillsPanel(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps;
  // Lua Skill rows.
  // Lua Skill 行。
  skills: LuaSkillSummary[];
}) {
  const { t } = useI18n();
  // searchQuery stores the free-text LuaSkills filter.
  // searchQuery 保存 LuaSkills 自由文本筛选词。
  const [searchQuery, setSearchQuery] = React.useState("");
  // statusFilter stores the selected LuaSkills enabled-state filter.
  // statusFilter 保存选中的 LuaSkills 启停状态筛选。
  const [statusFilter, setStatusFilter] = React.useState<LuaSkillStatusFilter>("all");
  // sourceFilter stores the selected system or user ownership filter.
  // sourceFilter 保存选中的系统或用户归属筛选。
  const [sourceFilter, setSourceFilter] = React.useState<LuaSkillSourceFilter>("all");
  // busyAction identifies the page-exclusive LuaSkills mutation currently in flight.
  // busyAction 标识当前正在执行的页面互斥 LuaSkills 变更。
  const [busyAction, setBusyAction] = React.useState<string | null>(null);
  // actionRunningRef closes the synchronous repeat-click window before React commits busy state.
  // actionRunningRef 在 React 提交忙碌状态前关闭同步重复点击窗口。
  const actionRunningRef = React.useRef(false);
  // detailSkillId stores the Lua Skill selected for the details dialog.
  // detailSkillId 保存当前打开详情弹窗的 Lua Skill 标识。
  const [detailSkillId, setDetailSkillId] = React.useState<string | null>(null);
  // toolsSkillId stores the Lua Skill selected for the tools dialog.
  // toolsSkillId 保存当前打开工具弹窗的 Lua Skill 标识。
  const [toolsSkillId, setToolsSkillId] = React.useState<string | null>(null);
  // configSkillId stores the Lua Skill selected for package configuration.
  // configSkillId 保存当前打开技能包配置的 Lua Skill 标识。
  const [configSkillId, setConfigSkillId] = React.useState<string | null>(null);
  // detailSkill derives the selected details skill from the latest snapshot.
  // detailSkill 从最新快照派生当前详情技能，避免弹窗持有旧对象。
  const detailSkill = React.useMemo(
    () => props.skills.find((skill) => skill.skillId === detailSkillId) ?? null,
    [detailSkillId, props.skills],
  );
  // toolsSkill derives the selected tools skill from the latest snapshot.
  // toolsSkill 从最新快照派生当前工具技能，避免工具状态同步后仍显示旧对象。
  const toolsSkill = React.useMemo(
    () => props.skills.find((skill) => skill.skillId === toolsSkillId) ?? null,
    [props.skills, toolsSkillId],
  );
  // configSkill derives the selected configuration skill from the latest snapshot.
  // configSkill 从最新快照派生当前配置技能。
  const configSkill = React.useMemo(
    () => props.skills.find((skill) => skill.skillId === configSkillId) ?? null,
    [configSkillId, props.skills],
  );
  // visibleSkills applies text, enabled-state, and ownership filters to the current snapshot.
  // visibleSkills 对当前快照应用文本、启停状态与归属筛选。
  const visibleSkills = React.useMemo(() => props.skills.filter((skill) => {
    if (statusFilter === "enabled" && !skill.enabled) {
      return false;
    }
    if (statusFilter === "disabled" && skill.enabled) {
      return false;
    }
    if (sourceFilter === "system" && !skill.systemOwned) {
      return false;
    }
    if (sourceFilter === "user" && skill.systemOwned) {
      return false;
    }
    return luaSkillMatchesSearch(skill, searchQuery);
  }), [props.skills, searchQuery, sourceFilter, statusFilter]);

  // runAction executes one page-exclusive LuaSkills mutation.
  // runAction 执行单个页面互斥的 LuaSkills 变更。
  //
  // Parameters: label identifies the busy indicator and action performs the service mutation.
  // 参数：label 标识忙碌提示，action 执行服务变更。
  // Returns: a promise that settles after the accepted action releases the synchronous guard.
  // 返回值：在已接受动作释放同步守卫后结束的 Promise。
  const runAction = React.useCallback(async (label: string, action: () => Promise<void>) => {
    if (actionRunningRef.current) {
      return;
    }
    actionRunningRef.current = true;
    setBusyAction(label);
    try {
      await action();
    } finally {
      actionRunningRef.current = false;
      setBusyAction(null);
    }
  }, []);

  return (
    <div className="skill-page-body lua-skill-page-body">
      <div className="agent-catalog-filter-row skill-list-filter-row">
        <label className="agent-catalog-search-input skill-list-search-input">
          <Search size={15} />
          <input
            aria-label={t("skills.filter.searchLabel")}
            onChange={(event) => setSearchQuery(event.target.value)}
            placeholder={t("skills.filter.searchPlaceholder")}
            value={searchQuery}
          />
        </label>
        <select
          aria-label={t("skills.filter.statusLabel")}
          className="agent-catalog-status-select"
          onChange={(event) => setStatusFilter(event.target.value as LuaSkillStatusFilter)}
          value={statusFilter}
        >
          <option value="all">{t("skills.filter.allStatuses")}</option>
          <option value="enabled">{t("skills.status.enabled")}</option>
          <option value="disabled">{t("skills.status.stopped")}</option>
        </select>
        <select
          aria-label={t("skills.filter.sourceLabel")}
          className="agent-catalog-status-select"
          onChange={(event) => setSourceFilter(event.target.value as LuaSkillSourceFilter)}
          value={sourceFilter}
        >
          <option value="all">{t("skills.filter.allSources")}</option>
          <option value="system">{t("skills.label.system")}</option>
          <option value="user">{t("skills.label.user")}</option>
        </select>
      </div>
      <div className="skill-card-list agent-skill-card-list">
        {visibleSkills.length === 0 ? (
          <div className="skill-empty-state">
            {props.skills.length === 0 ? t("skills.lua.emptyDetail") : t("skills.filter.empty")}
          </div>
        ) : visibleSkills.map((skill) => (
          <LuaSkillCard
            actions={props.actions}
            busy={busyAction !== null}
            key={skill.skillId}
            onOpenDetail={(selectedSkill) => setDetailSkillId(selectedSkill.skillId)}
            onOpenConfig={(selectedSkill) => setConfigSkillId(selectedSkill.skillId)}
            onOpenTools={(selectedSkill) => setToolsSkillId(selectedSkill.skillId)}
            onRunAction={runAction}
            skill={skill}
          />
        ))}
      </div>
      <LuaSkillDetailDialog
        open={detailSkillId !== null && detailSkill !== null}
        skill={detailSkill}
        onOpenChange={(open) => {
          if (!open) {
            setDetailSkillId(null);
          }
        }}
      />
      <LuaSkillToolsDialog
        actions={props.actions}
        busy={busyAction !== null}
        open={toolsSkillId !== null && toolsSkill !== null}
        skill={toolsSkill}
        onOpenChange={(open) => {
          if (!open) {
            setToolsSkillId(null);
          }
        }}
      />
      <LuaSkillConfigDialog
        actions={props.actions}
        open={configSkillId !== null && configSkill !== null}
        skill={configSkill}
        onOpenChange={(open) => {
          if (!open) {
            setConfigSkillId(null);
          }
        }}
      />
    </div>
  );
}

// LuaSkillInstallDialog renders the standalone LuaSkills install form.
// LuaSkillInstallDialog 渲染独立的 LuaSkills 安装表单。
export function LuaSkillInstallDialog(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps;
  // Whether the install dialog is open.
  // 安装弹窗当前是否打开。
  open: boolean;
  // Updates the install dialog open state.
  // 更新安装弹窗打开状态。
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useI18n();
  // sourceType stores the selected install source kind.
  // sourceType 保存当前选择的安装来源类型。
  const [sourceType, setSourceType] = React.useState<"github" | "url">("github");
  // source stores the install source locator.
  // source 保存安装来源定位值。
  const [source, setSource] = React.useState("");
  // installing tracks the active install request.
  // installing 跟踪当前安装请求。
  const [installing, setInstalling] = React.useState(false);
  // installingRef rejects a second form submit before the installing render is committed.
  // installingRef 在 installing 渲染提交前拒绝第二次表单提交。
  const installingRef = React.useRef(false);

  React.useEffect(() => {
    if (props.open) {
      setSourceType("github");
      setSource("");
      installingRef.current = false;
      setInstalling(false);
    }
  }, [props.open]);

  // submitInstall validates and runs one LuaSkills install request.
  // submitInstall 校验并执行单次 LuaSkills 安装请求。
  const submitInstall = React.useCallback(async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (installingRef.current || source.trim().length === 0) {
      return;
    }
    installingRef.current = true;
    setInstalling(true);
    try {
      await props.actions.onInstallLuaSkill("", source, sourceType);
      props.onOpenChange(false);
    } finally {
      installingRef.current = false;
      setInstalling(false);
    }
  }, [props.actions, props.onOpenChange, source, sourceType]);

  return (
    <Dialog open={props.open} onOpenChange={(open) => !installingRef.current && props.onOpenChange(open)}>
      <DialogContent className="lua-skill-install-dialog">
        <DialogHeader>
          <DialogTitle>{t("skills.lua.installTitle")}</DialogTitle>
          <DialogDescription>{t("skills.lua.installDescription")}</DialogDescription>
        </DialogHeader>
        <form className="lua-skill-install-form" onSubmit={submitInstall}>
          <label className="lua-skill-install-field">
            <span>{t("skills.lua.sourceType")}</span>
            <select disabled={installing} value={sourceType} onChange={(event) => setSourceType(event.target.value as "github" | "url")}>
              <option value="github">GitHub</option>
              <option value="url">URL</option>
            </select>
          </label>
          <label className="lua-skill-install-field">
            <span>{t("skills.lua.installSource")}</span>
            <input
              autoFocus
              disabled={installing}
              placeholder={sourceType === "github" ? t("skills.lua.placeholderGithub") : t("skills.lua.placeholderUrl")}
              value={source}
              onChange={(event) => setSource(event.target.value)}
            />
          </label>
          <DialogFooter>
            <Button disabled={installing} onClick={() => props.onOpenChange(false)} type="button" variant="secondary">{t("skills.action.cancel")}</Button>
            <Button disabled={installing || source.trim().length === 0} type="submit">
              <PackagePlus size={15} />
              <span>{installing ? t("skills.action.installing") : t("skills.action.install")}</span>
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
