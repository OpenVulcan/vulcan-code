import * as React from "react";
import {
  Bot,
  Hammer,
  Maximize2,
  Minimize2,
  PackagePlus,
  RefreshCw,
} from "lucide-react";
import {
  LuaSkillInstallDialog,
  SkillsArea,
  activeSkillHeaderSummary,
  type SkillActionProps,
  type SkillHeaderBusyActionId,
  type SkillSubAreaId,
} from "@/components/features/skills/SkillsArea";
import { AdaptiveToolPanel } from "@/components/workspace/adaptive-tool-panel";
import { useI18n } from "@/i18n";
import type { WorkbenchSnapshot } from "@/types";

// WorkspaceSkillsHeaderState describes the embedded skills title bar projection.
// WorkspaceSkillsHeaderState 描述内嵌技能设置标题栏投影。
export interface WorkspaceSkillsHeaderState {
  // Header title rendered in the workspace title bar.
  // 渲染在工作台标题栏中的标题。
  title: string;
  // Header detail rendered below the title with trailing fade.
  // 渲染在标题下方并带尾部渐隐的详情。
  detail: string;
  // Header actions rendered on the right side.
  // 渲染在标题栏右侧的操作区。
  actions: React.ReactNode;
}

// WorkspaceSkillsSettings renders the embedded skills settings page.
// WorkspaceSkillsSettings 渲染内嵌技能设置页面。
export function WorkspaceSkillsSettings(props: {
  // Selected embedded skills sub page.
  // 当前选中的内嵌技能子页面。
  activeSubArea: SkillSubAreaId;
  // Updates the selected embedded skills sub page.
  // 更新当前选中的内嵌技能子页面。
  onActiveSubAreaChange: (subArea: SkillSubAreaId) => void;
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  skillActions: SkillActionProps;
  // Publishes the computed title bar state to the workspace header.
  // 将计算后的标题栏状态发布到工作台标题栏。
  onHeaderStateChange: (state: WorkspaceSkillsHeaderState) => void;
  // Workbench snapshot containing Lua Skills and Agent Skills catalogs.
  // 包含 Lua Skills 与 Agent Skills 目录的工作台快照。
  workbench: WorkbenchSnapshot;
}) {
  const {
    activeSubArea,
    onActiveSubAreaChange,
    onHeaderStateChange,
    skillActions,
    workbench,
  } = props;
  // t translates shared skill action labels.
  // t 翻译共享技能操作标签。
  const { t } = useI18n();
  // menuExpanded stores whether the floating skill menu shows every row.
  // menuExpanded 保存浮动技能菜单是否显示完整行。
  const [menuExpanded, setMenuExpanded] = React.useState(true);
  // luaInstallDialogOpen controls the embedded LuaSkills install dialog.
  // luaInstallDialogOpen 控制内嵌 LuaSkills 安装弹窗。
  const [luaInstallDialogOpen, setLuaInstallDialogOpen] = React.useState(false);
  // busyAction stores the title-bar action currently showing busy feedback.
  // busyAction 保存当前正在标题栏展示忙碌反馈的动作。
  const [busyAction, setBusyAction] = React.useState<SkillHeaderBusyActionId | null>(null);
  // activeLabel stores the compact display name for the selected skill sub page.
  // activeLabel 保存当前技能子页面的紧凑显示名称。
  const activeLabel = skillSubAreaLabel(activeSubArea);
  // headerSummary stores the active skill page title and metric text.
  // headerSummary 保存当前技能页标题与统计文本。
  const headerSummary = activeSkillHeaderSummary(activeSubArea, workbench, t);

  // runHeaderAction wraps title-bar actions with disabled and spinner feedback.
  // runHeaderAction 为标题栏动作包裹禁用与旋转反馈。
  const runHeaderAction = React.useCallback(async (actionId: SkillHeaderBusyActionId, action: () => Promise<void>) => {
    setBusyAction(actionId);
    const startedAt = window.performance.now();
    try {
      await action();
    } finally {
      const remainingMs = 450 - (window.performance.now() - startedAt);
      if (remainingMs > 0) {
        await new Promise((resolve) => {
          window.setTimeout(resolve, remainingMs);
        });
      }
      setBusyAction(null);
    }
  }, []);

  // headerActions stores the current sub-page action buttons for the workspace title bar.
  // headerActions 保存当前子页面在工作台标题栏中的操作按钮。
  const headerActions = React.useMemo(() => {
    if (activeSubArea === "lua") {
      return (
        <div className="feature-header-actions workspace-skills-header-actions">
          <button
            className="skill-action-button secondary"
            disabled={busyAction !== null}
            onClick={() => void runHeaderAction("lua-refresh", skillActions.onRefreshLuaSkills)}
            type="button"
          >
            <RefreshCw className={busyAction === "lua-refresh" ? "spin" : undefined} size={15} />
            <span>{busyAction === "lua-refresh" ? t("skills.action.refreshing") : t("skills.action.refresh")}</span>
          </button>
          <button className="skill-action-button" onClick={() => setLuaInstallDialogOpen(true)} type="button">
            <PackagePlus size={15} />
            <span>{t("skills.action.installLua")}</span>
          </button>
        </div>
      );
    }
    return (
      <div className="feature-header-actions workspace-skills-header-actions">
        <button
          className="skill-action-button secondary"
          disabled={busyAction !== null}
          onClick={() => void runHeaderAction("agent-refresh", skillActions.onRefreshAgentSkills)}
          type="button"
        >
          <RefreshCw className={busyAction === "agent-refresh" ? "spin" : undefined} size={15} />
          <span>{busyAction === "agent-refresh" ? t("skills.action.refreshing") : t("skills.action.refresh")}</span>
        </button>
        <button
          className="skill-action-button secondary"
          disabled={busyAction !== null}
          onClick={() => void runHeaderAction("agent-sync", skillActions.onInstallSystemAgentSkills)}
          type="button"
        >
          <PackagePlus className={busyAction === "agent-sync" ? "spin" : undefined} size={15} />
          <span>{busyAction === "agent-sync" ? t("skills.action.syncing") : t("skills.action.syncSystem")}</span>
        </button>
      </div>
    );
  }, [activeSubArea, busyAction, runHeaderAction, skillActions, t]);

  React.useEffect(() => {
    onHeaderStateChange({
      title: "技能设置",
      detail: `${activeLabel} · ${headerSummary.detail}`,
      actions: headerActions,
    });
  }, [activeLabel, headerActions, headerSummary.detail, onHeaderStateChange]);

  return (
    <AdaptiveToolPanel
      className="workspace-skills-settings"
      contentClassName="workspace-skills-content"
      contentShellClassName="workspace-skills-content-shell"
      expanded={menuExpanded}
      panel={(
        <FloatingSkillMenu
          activeSubArea={activeSubArea}
          expanded={menuExpanded}
          onExpandedChange={setMenuExpanded}
          onSelectSubArea={onActiveSubAreaChange}
        />
      )}
      side="left"
    >
      <SkillsArea
        activeTab={activeSubArea}
        skillActions={skillActions}
        workbench={workbench}
      />
      <LuaSkillInstallDialog
        actions={skillActions}
        open={luaInstallDialogOpen}
        onOpenChange={setLuaInstallDialogOpen}
      />
    </AdaptiveToolPanel>
  );
}

// FloatingSkillMenu renders the floating two-row embedded skills selector.
// FloatingSkillMenu 渲染两行浮动内嵌技能选择器。
function FloatingSkillMenu(props: {
  // Selected embedded skills sub page.
  // 当前选中的内嵌技能子页面。
  activeSubArea: SkillSubAreaId;
  // Whether the menu is expanded.
  // 菜单是否展开。
  expanded: boolean;
  // Updates the expanded menu state.
  // 更新菜单展开状态。
  onExpandedChange: (expanded: boolean) => void;
  // Selects one embedded skills sub page.
  // 选择一个内嵌技能子页面。
  onSelectSubArea: (subArea: SkillSubAreaId) => void;
}) {
  // rows stores the static embedded skills menu rows.
  // rows 保存静态内嵌技能菜单行。
  const rows: Array<{ icon: React.ReactNode; id: SkillSubAreaId; label: string }> = [
    { icon: <Hammer size={15} />, id: "lua", label: "LuaSkills" },
    { icon: <Bot size={15} />, id: "agent", label: "AgentSkills" },
  ];
  // activeLabel stores the selected menu label for the collapsed pill.
  // activeLabel 保存收起胶囊展示的当前菜单标签。
  const activeLabel = skillSubAreaLabel(props.activeSubArea);

  if (!props.expanded) {
    return (
      <button
        className="workspace-skills-menu-collapsed"
        onClick={() => props.onExpandedChange(true)}
        title="展开状态"
        type="button"
      >
        <span>{activeLabel}</span>
        <Maximize2 size={13} />
      </button>
    );
  }

  return (
    <aside className="workspace-skills-menu" aria-label="技能设置菜单">
      <div className="workspace-skills-menu-header">
        <span>菜单</span>
        <button
          aria-label="收起为胶囊"
          className="workspace-skills-menu-icon-button"
          onClick={() => props.onExpandedChange(false)}
          type="button"
        >
          <Minimize2 size={13} />
        </button>
      </div>
      <div className="workspace-skills-menu-list">
        {rows.map((row) => (
          <button
            className={`workspace-skills-menu-row ${props.activeSubArea === row.id ? "active" : ""}`}
            key={row.id}
            onClick={() => props.onSelectSubArea(row.id)}
            type="button"
          >
            {row.icon}
            <span>{row.label}</span>
          </button>
        ))}
      </div>
    </aside>
  );
}

// skillSubAreaLabel returns the compact label for one embedded skills sub page.
// skillSubAreaLabel 返回单个内嵌技能子页面的紧凑标签。
function skillSubAreaLabel(subArea: SkillSubAreaId): string {
  return subArea === "agent" ? "AgentSkills" : "LuaSkills";
}
