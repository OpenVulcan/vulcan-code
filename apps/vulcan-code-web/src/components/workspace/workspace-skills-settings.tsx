import * as React from "react";
import {
  Bot,
  Hammer,
  Maximize2,
  Minimize2,
} from "lucide-react";
import {
  LuaSkillInstallDialog,
  SkillsArea,
  type SkillActionProps,
  type SkillSubAreaId,
} from "@/components/features/skills/SkillsArea";
import type { WorkbenchSnapshot } from "@/types";

// WorkspaceSkillsSettings renders the embedded skills settings page.
// WorkspaceSkillsSettings 渲染内嵌技能设置页面。
export function WorkspaceSkillsSettings(props: {
  // Selected embedded skills sub page.
  // 当前选中的内嵌技能子页面。
  activeSubArea: SkillSubAreaId;
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  skillActions: SkillActionProps;
  // Whether the LuaSkills install dialog is open.
  // LuaSkills 安装弹窗当前是否打开。
  luaInstallDialogOpen: boolean;
  // Updates the LuaSkills install dialog open state.
  // 更新 LuaSkills 安装弹窗打开状态。
  onLuaInstallDialogOpenChange: (open: boolean) => void;
  // Workbench snapshot containing Lua Skills and Agent Skills catalogs.
  // 包含 Lua Skills 与 Agent Skills 目录的工作台快照。
  workbench: WorkbenchSnapshot;
}) {
  const {
    activeSubArea,
    luaInstallDialogOpen,
    onLuaInstallDialogOpenChange,
    skillActions,
    workbench,
  } = props;

  return (
    <>
      <SkillsArea
        activeTab={activeSubArea}
        skillActions={skillActions}
        workbench={workbench}
      />
      <LuaSkillInstallDialog
        actions={skillActions}
        open={luaInstallDialogOpen}
        onOpenChange={onLuaInstallDialogOpenChange}
      />
    </>
  );
}

// FloatingSkillMenu renders the floating two-row embedded skills selector.
// FloatingSkillMenu 渲染两行浮动内嵌技能选择器。
export function FloatingSkillMenu(props: {
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
        className="workspace-tool-panel-menu-collapsed"
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
    <aside className="workspace-tool-panel-menu" aria-label="技能设置菜单">
      <div className="workspace-tool-panel-menu-header">
        <span>菜单</span>
        <button
          aria-label="收起为胶囊"
          className="workspace-tool-panel-menu-icon-button"
          onClick={() => props.onExpandedChange(false)}
          type="button"
        >
          <Minimize2 size={13} />
        </button>
      </div>
      <div className="workspace-tool-panel-menu-list">
        {rows.map((row) => (
          <button
            className={`workspace-tool-panel-menu-row ${props.activeSubArea === row.id ? "active" : ""}`}
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
export function skillSubAreaLabel(subArea: SkillSubAreaId): string {
  return subArea === "agent" ? "AgentSkills" : "LuaSkills";
}
