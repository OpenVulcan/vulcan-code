import * as React from "react";
import { PackagePlus, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { FilterChip, FilterChipGroup } from "@/components/ui/choice-controls";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
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
  const { t, locale } = useI18n();
  // actionError retains the latest lifecycle failure instead of leaving an unhandled rejection.
  // actionError 保留最近的生命周期失败，避免未处理的拒绝。
  const [actionError, setActionError] = React.useState("");
  // pendingUninstall retains the exact user package awaiting explicit removal confirmation.
  // pendingUninstall 保留等待明确删除确认的精确用户包。
  const [pendingUninstall, setPendingUninstall] = React.useState<LuaSkillSummary | null>(null);
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
    setActionError("");
    try {
      await action();
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      actionRunningRef.current = false;
      setBusyAction(null);
    }
  }, []);

  return (
    <div className="skill-page-body lua-skill-page-body">
      {actionError ? <div role="alert" className="skill-empty-state warning">
        <p>{locale.startsWith("zh") ? "技能操作失败，请重试。" : "Skill action failed. Try again."}</p>
        <details><summary>{locale.startsWith("zh") ? "错误详情" : "Error details"}</summary><pre className="plugin-control-error-details">{actionError}</pre></details>
      </div> : null}
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
        <FilterChipGroup aria-label={t("skills.filter.statusLabel")}>
          <FilterChip onClick={() => setStatusFilter("all")} selected={statusFilter === "all"}>{t("skills.filter.allStatuses")}</FilterChip>
          <FilterChip onClick={() => setStatusFilter("enabled")} selected={statusFilter === "enabled"}>{t("skills.status.enabled")}</FilterChip>
          <FilterChip onClick={() => setStatusFilter("disabled")} selected={statusFilter === "disabled"}>{t("skills.status.stopped")}</FilterChip>
        </FilterChipGroup>
        <Select onValueChange={(value) => setSourceFilter(value as LuaSkillSourceFilter)} value={sourceFilter}>
          <SelectTrigger aria-label={t("skills.filter.sourceLabel")} className="agent-catalog-status-select">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t("skills.filter.allSources")}</SelectItem>
            <SelectItem value="system">{t("skills.label.system")}</SelectItem>
            <SelectItem value="user">{t("skills.label.user")}</SelectItem>
          </SelectContent>
        </Select>
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
            onRequestUninstall={(selectedSkill) => { setActionError(""); setPendingUninstall(selectedSkill); }}
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
      <AlertDialog open={pendingUninstall !== null} onOpenChange={(open) => { if (!open && !actionRunningRef.current) setPendingUninstall(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{locale.startsWith("zh") ? "卸载技能" : "Uninstall skill"}</AlertDialogTitle>
            <AlertDialogDescription>{pendingUninstall ? (locale.startsWith("zh") ? `确认移除技能包“${pendingUninstall.displayName}”？` : `Remove skill package “${pendingUninstall.displayName}”?`) : ""}</AlertDialogDescription>
          </AlertDialogHeader>
          {actionError ? <details role="alert" className="text-sm text-destructive"><summary>{locale.startsWith("zh") ? "卸载失败，请重试" : "Uninstall failed. Retry."}</summary><pre className="plugin-control-error-details">{actionError}</pre></details> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busyAction !== null}>{t("skills.action.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={busyAction !== null} onClick={(event) => {
              event.preventDefault();
              if (pendingUninstall) void runAction("uninstall", async () => {
                await props.actions.onUninstallLuaSkill(pendingUninstall.skillId);
                setPendingUninstall(null);
              });
            }}>{busyAction !== null ? (locale.startsWith("zh") ? "处理中…" : "Working…") : t("skills.action.delete")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
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
  const { t, locale } = useI18n();
  // installError keeps failed installation feedback in the still-open form.
  // installError 将安装失败反馈保留在仍打开的表单内。
  const [installError, setInstallError] = React.useState("");
  // installFormId links the fixed footer to the independently scrolling form.
  // installFormId 将固定页脚关联到独立滚动的表单。
  const installFormId = React.useId();
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
      setInstallError("");
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
    setInstallError("");
    try {
      await props.actions.onInstallLuaSkill("", source, sourceType);
      props.onOpenChange(false);
    } catch (error) {
      setInstallError(error instanceof Error ? error.message : String(error));
    } finally {
      installingRef.current = false;
      setInstalling(false);
    }
  }, [props.actions, props.onOpenChange, source, sourceType]);

  return (
    <Dialog open={props.open} onOpenChange={(open) => !installingRef.current && props.onOpenChange(open)}>
      <DialogContent size="md" scrollMode="fixed-footer" className="grid-rows-[auto_minmax(0,1fr)_auto]">
        <DialogHeader>
          <DialogTitle>{t("skills.lua.installTitle")}</DialogTitle>
          <DialogDescription>{t("skills.lua.installDescription")}</DialogDescription>
        </DialogHeader>
        <form id={installFormId} className="lua-skill-install-form min-h-0 overflow-y-auto" onSubmit={submitInstall}>
          <label className="lua-skill-install-field">
            <span>{t("skills.lua.sourceType")}</span>
            <Select disabled={installing} value={sourceType} onValueChange={(value) => setSourceType(value as "github" | "url")}>
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="github">GitHub</SelectItem>
                <SelectItem value="url">URL</SelectItem>
              </SelectContent>
            </Select>
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
          {installError ? <div role="alert">
            <p>{locale.startsWith("zh") ? "技能安装失败，可修改来源后重试。" : "Skill installation failed. Check the source and retry."}</p>
            <details><summary>{locale.startsWith("zh") ? "错误详情" : "Error details"}</summary><pre className="plugin-control-error-details">{installError}</pre></details>
          </div> : null}
        </form>
          <DialogFooter>
            <Button disabled={installing} onClick={() => props.onOpenChange(false)} type="button" variant="secondary">{t("skills.action.cancel")}</Button>
            <Button disabled={installing || source.trim().length === 0} form={installFormId} type="submit">
              <PackagePlus size={15} />
              <span>{installing ? t("skills.action.installing") : t("skills.action.install")}</span>
            </Button>
          </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
// The standalone install dialog can load before SkillsArea and owns the same directory rules.
// 独立安装弹窗可能早于 SkillsArea 载入，并拥有同一组目录规则。
import "@/styles/skill-directory.css";
