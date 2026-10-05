import * as React from "react";
import { PackagePlus, Search } from "lucide-react";
import { PageEmptyState, PageNotice } from "@/components/page";
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
import { LuaSkillUpdateProgressDialog, type LuaSkillUpdateRequest } from "./lua-skill-update-progress-dialog";
import { formatMessage, useI18n } from "@/i18n";
import type { LuaSkillSummary, LuaSkillsOperationProgressSummary } from "@/types";
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
  // Latest install or update operation progress, or null when none is tracked.
  // 最近的安装或更新操作进度；未跟踪任何操作时为 null。
  operationProgress?: LuaSkillsOperationProgressSummary | null;
  // Lua Skill rows.
  // Lua Skill 行。
  skills: LuaSkillSummary[];
}) {
  const { t } = useI18n();
  // operationProgress is the latest tracked install or update, or null when none is.
  // operationProgress 是最近跟踪的安装或更新；没有时为 null。
  const operationProgress = props.operationProgress ?? null;
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
  // updateRequest opens progress immediately, before the first service event arrives.
  // updateRequest 在首个服务事件到达前立即打开进度弹窗。
  const [updateRequest, setUpdateRequest] = React.useState<LuaSkillUpdateRequest | null>(null);
  // dismissedUpdateId prevents a retained terminal record from reopening after dismissal or page entry.
  // dismissedUpdateId 防止保留的终态记录在关闭后或进入页面时再次弹出。
  const [dismissedUpdateId, setDismissedUpdateId] = React.useState<string | null>(
    operationProgress !== null && !operationProgress.inFlight ? operationProgress.correlationId : null,
  );
  // updateProgress excludes the previous operation and events that explicitly identify another package.
  // updateProgress 排除上一次操作以及明确标识其他技能包的事件。
  const updateProgress = operationProgress?.action === "update"
    && operationProgress.correlationId !== updateRequest?.previousCorrelationId
    && (updateRequest === null || operationProgress.skillId == null || operationProgress.skillId === updateRequest.skillId)
    ? operationProgress : null;
  // updateDialogOpen follows the local request or a newly observed service update.
  // updateDialogOpen 跟随本地请求或新观察到的服务更新。
  const updateDialogOpen = updateRequest !== null
    || (updateProgress !== null && updateProgress.correlationId !== dismissedUpdateId);
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
  // Parameters: label identifies the action, action performs it, and skillId identifies the clicked package.
  // 参数：label 标识动作，action 执行动作，skillId 标识已点击的技能包。
  // Returns: a promise that settles after the accepted action releases the synchronous guard.
  // 返回值：在已接受动作释放同步守卫后结束的 Promise。
  const runAction = React.useCallback(async (label: string, action: () => Promise<void>, skillId: string) => {
    if (actionRunningRef.current) {
      return;
    }
    actionRunningRef.current = true;
    setBusyAction(label);
    setActionError("");
    if (label === "update") {
      setUpdateRequest({ skillId, previousCorrelationId: operationProgress?.correlationId ?? null, outcome: null, error: null });
    }
    try {
      await action();
      if (label === "update") {
        setUpdateRequest((current) => current === null ? null : { ...current, outcome: "succeeded" });
      }
    } catch (error) {
      // message belongs to the request error, not an inferred progress event.
      // message 属于请求错误，不是推测出来的进度事件。
      const message = error instanceof Error ? error.message : String(error);
      if (label === "update") {
        setUpdateRequest((current) => current === null ? null : { ...current, outcome: "failed", error: message });
      } else {
        setActionError(message);
      }
    } finally {
      actionRunningRef.current = false;
      setBusyAction(null);
    }
  }, [operationProgress?.correlationId]);
  // configWatchIssues retains unresolved configuration-watch or reload failures by
  // their exact store and optional package identity.
  // configWatchIssues 按精确存储域与可选技能包身份保留尚未恢复的配置监听或重载失败。
  //
  // The list owns these instead of the configuration dialog because a broken watcher
  // affects every package in its store even when no dialog is open. Multiple stores
  // remain independent, so recovery in one cannot hide an unresolved failure in another.
  // 它们由列表而非配置弹窗持有，因为监听损坏会影响对应存储域中的全部技能包——包括没有
  // 弹窗打开时。多个存储域彼此独立，因此一个域恢复时不能隐藏另一个域尚未恢复的故障。
  const [configWatchIssues, setConfigWatchIssues] = React.useState<Array<{
    code: string;
    message: string;
    skillId: string | null;
    storeScope: string;
  }>>([]);
  // configEventCursor is this list's own poll position. The retained engine queue is
  // shared, so the dialog keeps an independent cursor over the same events.
  // configEventCursor 是本列表自己的轮询位置。保留引擎队列是共享的，因此弹窗在同一批
  // 事件上保有独立游标。
  const configEventCursorRef = React.useRef<string | null>(null);
  React.useEffect(() => {
    let cancelled = false;
    let timeoutId: number | null = null;
    const poll = async (): Promise<void> => {
      try {
        const batch = await props.actions.onPollLuaSkillConfigEvents(
          configEventCursorRef.current,
        );
        if (cancelled) {
          return;
        }
        configEventCursorRef.current = batch.nextSequence;
        if (batch.events.length > 0) {
          setConfigWatchIssues((current) => {
            // unresolved retains independent failures instead of letting the newest
            // store event erase a still-broken sibling store.
            // unresolved 保留彼此独立的故障，避免最新存储域事件抹掉仍然损坏的同级存储域。
            const unresolved = new Map(current.map((issue) => [
              `${issue.storeScope}\0${issue.skillId ?? ""}`,
              issue,
            ]));
            for (const event of batch.events) {
              const skillId = event.skillId ?? null;
              const exactKey = `${event.storeScope}\0${skillId ?? ""}`;
              const storeKey = `${event.storeScope}\0`;
              if (event.error) {
                unresolved.set(exactKey, {
                  code: event.error.code,
                  message: event.error.message,
                  skillId,
                  storeScope: event.storeScope,
                });
                continue;
              }
              // Only an event produced by the external reload path proves the watcher
              // for this store is alive. A successful local write proves storage only.
              // 只有外部重载路径产生的事件才能证明当前存储域的监听器仍然存活；成功的本地
              // 写入只能证明存储可用。
              if (event.source === "external_reload") {
                unresolved.delete(storeKey);
                unresolved.delete(exactKey);
              }
            }
            return [...unresolved.values()];
          });
        }
      } catch {
        // A failed poll keeps the last known issue: clearing it here would hide the
        // very failure this banner exists to report.
        // 轮询失败时保留上一次已知问题：在此清除它会隐藏横幅本应报告的失败。
      } finally {
        if (!cancelled) {
          timeoutId = window.setTimeout(() => void poll(), 3000);
        }
      }
    };
    timeoutId = window.setTimeout(() => void poll(), 1500);
    return () => {
      cancelled = true;
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [props.actions]);


  return (
    <div className="skill-page-body lua-skill-page-body">
      {actionError ? <PageNotice details={actionError} detailsLabel={t("skills.lua.errorDetails")} tone="error">
        {t("skills.lua.skillActionFailedTry")}
      </PageNotice> : null}
      {operationProgress && operationProgress.action !== "update" ? <PageNotice
        tone={operationProgress.outcome === "failed" ? "error" : operationProgress.outcome === "succeeded" ? "success" : "info"}
      >
        <p>{t(operationProgress.inFlight
          ? (operationProgress.action === "update" ? "skills.lua.operationUpdating" : "skills.lua.operationInstalling")
          : operationProgress.outcome === "succeeded"
            ? "skills.lua.operationCompleted"
            : "skills.lua.operationFailed")}</p>
        <p>{[
          operationProgress.skillId ?? null,
          operationProgress.latestPhase ?? null,
          operationProgress.percent != null ? `${operationProgress.percent}%` : null,
          operationProgress.bytesDone != null && operationProgress.bytesTotal != null
            ? `${operationProgress.bytesDone}/${operationProgress.bytesTotal}`
            : null,
        ].filter(Boolean).join(" · ")}</p>
        {/* A failure carries only a fixed host-owned message; arbitrary upstream text
            never crosses the service boundary. Overflowed says phases were dropped so
            a client does not read a gap as a stall. */}
        {/* 失败只携带宿主自有的固定文案；任意上游文本绝不跨越服务边界。overflowed 说明阶段
            曾被丢弃，使客户端不会把空档读成卡死。 */}
        {operationProgress.message || operationProgress.overflowed ? <p>
          {operationProgress.message ?? ""}
          {operationProgress.overflowed
            ? `${operationProgress.message ? " " : ""}${t("skills.lua.somePhasesWereDropped")}`
            : ""}
        </p> : null}
        {/* The engine write lock is held for the entire install or update, including
            download and verification, so unrelated skill operations queue behind it.
            The concurrency plan requires that queueing to be shown as an explicit
            limitation instead of being hidden. */}
        {/* 引擎写锁在整次安装或更新期间持有，含下载与校验，因此无关的技能操作会在其后排队。
            并发方案要求把该排队作为明确限制显示，而不是隐藏。 */}
        {operationProgress.inFlight ? <p>{t("skills.lua.theSkillEngineIs")}</p> : null}
      </PageNotice> : null}
      {configWatchIssues.map((configWatchIssue) => <PageNotice
        key={`${configWatchIssue.storeScope}\0${configWatchIssue.skillId ?? ""}`}
        role="alert"
        tone="warning"
      >
        <p>{configWatchIssue.skillId
          ? formatMessage(t("skills.lua.configWatchSkill"), { skillId: configWatchIssue.skillId, storeScope: configWatchIssue.storeScope })
          : formatMessage(t("skills.lua.configWatchScope"), { storeScope: configWatchIssue.storeScope })}</p>
        <pre>{configWatchIssue.code}: {configWatchIssue.message}</pre>
      </PageNotice>)}
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
          <PageEmptyState size="panel" title={props.skills.length === 0 ? t("skills.lua.emptyDetail") : t("skills.filter.empty")} />
        ) : visibleSkills.map((skill) => (
          <LuaSkillCard
            actions={props.actions}
            busy={busyAction !== null || operationProgress?.inFlight === true}
            key={skill.skillId}
            onOpenDetail={(selectedSkill) => setDetailSkillId(selectedSkill.skillId)}
            onOpenConfig={(selectedSkill) => setConfigSkillId(selectedSkill.skillId)}
            onOpenTools={(selectedSkill) => setToolsSkillId(selectedSkill.skillId)}
            onRequestUninstall={(selectedSkill) => { setActionError(""); setPendingUninstall(selectedSkill); }}
            onRunAction={(label, action) => runAction(label, action, skill.skillId)}
            skill={skill}
          />
        ))}
      </div>
      <LuaSkillUpdateProgressDialog
        open={updateDialogOpen}
        progress={updateProgress}
        request={updateRequest}
        onClose={() => {
          setDismissedUpdateId(operationProgress?.correlationId ?? null);
          setUpdateRequest(null);
        }}
      />
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
            <AlertDialogTitle>{t("skills.lua.uninstallSkill")}</AlertDialogTitle>
            <AlertDialogDescription>{pendingUninstall ? (formatMessage(t("skills.lua.removeSkillPackage"), { displayName: pendingUninstall.displayName })) : ""}</AlertDialogDescription>
          </AlertDialogHeader>
          {actionError ? <PageNotice details={actionError} detailsLabel={t("skills.lua.errorDetails")} tone="error">{t("skills.lua.uninstallFailedRetry")}</PageNotice> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busyAction !== null}>{t("skills.action.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={busyAction !== null} onClick={(event) => {
              event.preventDefault();
              if (pendingUninstall) void runAction("uninstall", async () => {
                await props.actions.onUninstallLuaSkill(pendingUninstall.skillId);
                setPendingUninstall(null);
              }, pendingUninstall.skillId);
            }}>{busyAction !== null ? t("skills.lua.working") : t("skills.action.delete")}</AlertDialogAction>
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
  const { t } = useI18n();
  // installError keeps failed installation feedback in the still-open form.
  // installError 将安装失败反馈保留在仍打开的表单内。
  const [installError, setInstallError] = React.useState("");
  // installFormId links the fixed footer to the independently scrolling form.
  // installFormId 将固定页脚关联到独立滚动的表单。
  const installFormId = React.useId();
  // sourceType stores the selected install source kind.
  // sourceType 保存当前选择的安装来源类型。
  //
  // GitHub is the only selectable kind: upstream source policy rejects
  // `SkillInstallSourceType::Url` unconditionally for install and update alike, so the
  // host boundary rejects it too and offering it here would present a choice that can
  // never succeed.
  // 只有 GitHub 可选：上游来源策略对安装与更新都无条件拒绝
  // `SkillInstallSourceType::Url`，因此宿主边界同样拒绝它，在此提供该选项只会呈现一个永远
  // 不可能成功的选择。
  const [sourceType, setSourceType] = React.useState<"github">("github");
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
            <Select disabled={installing} value={sourceType} onValueChange={(value) => setSourceType(value as "github")}>
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="github">GitHub</SelectItem>
              </SelectContent>
            </Select>
          </label>
          <label className="lua-skill-install-field">
            <span>{t("skills.lua.installSource")}</span>
            <input
              autoFocus
              disabled={installing}
              placeholder={t("skills.lua.placeholderGithub")}
              value={source}
              onChange={(event) => setSource(event.target.value)}
            />
          </label>
          {installError ? <PageNotice details={installError} detailsLabel={t("skills.lua.errorDetails")} tone="error">
            {t("skills.lua.skillInstallationFailedCheck")}
          </PageNotice> : null}
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
