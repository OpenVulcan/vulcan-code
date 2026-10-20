import { DirectoryRow, WorkspaceDirectoryMenu } from "@/components/workspace/workspace-directory-layout";
import * as React from "react";
import {
  BookMarked,
  ChartNoAxesCombined,
  Hammer,
  Plug,
  Puzzle,
  Route,
  ScanSearch,
  Settings,
  Wrench,
} from "lucide-react";
import type { SkillActionProps, SkillSubAreaId } from "@/components/features/skills/skill-area-types";
import {
  enabledPluginDetailMounts,
  findEnabledPluginDetailMount,
  localizedPluginMountLabel,
} from "@/components/features/skills/skill-area-types";
import { useI18n } from "@/i18n";
import type { WorkbenchSnapshot } from "@/types";

// LazySkillsArea keeps the full LuaSkills catalog outside the console-first workbench chunk.
// LazySkillsArea 将完整 LuaSkills 目录保留在控制台优先的工作台分块之外。
const LazySkillsArea = React.lazy(() => import("@/components/features/skills/skills-area").then((module) => ({ default: module.SkillsArea })));
// LazyLuaSkillInstallDialog keeps installation UI outside the initial workbench chunk.
// LazyLuaSkillInstallDialog 将安装界面保留在初始工作台分块之外。
const LazyLuaSkillInstallDialog = React.lazy(() => import("@/components/features/skills/lua-skills-panel").then((module) => ({ default: module.LuaSkillInstallDialog })));

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
  // Opens the application-wide runtime diagnostics page from the MCP server page.
  // 从 MCP 服务器页面打开应用全局运行诊断页。
  onOpenDiagnostics: () => void;
  // onSelectPluginResourceOwner preserves the real workspace-to-service navigation chain.
  // onSelectPluginResourceOwner 保留真实工作台至服务导航链。
  onSelectPluginResourceOwner?: (conversationId: string, observation: AbortSignal) => Promise<boolean>;
}) {
  const {
    activeSubArea,
    luaInstallDialogOpen,
    onLuaInstallDialogOpenChange,
    onOpenDiagnostics,
    skillActions,
    workbench,
  } = props;

  return (
    <>
      <React.Suspense fallback={null}>
        <LazySkillsArea activeTab={activeSubArea} onOpenDiagnostics={onOpenDiagnostics} onSelectPluginResourceOwner={props.onSelectPluginResourceOwner} skillActions={skillActions} workbench={workbench} />
      </React.Suspense>
      {luaInstallDialogOpen ? (
        <React.Suspense fallback={null}>
          <LazyLuaSkillInstallDialog actions={skillActions} open={luaInstallDialogOpen} onOpenChange={onLuaInstallDialogOpenChange} />
        </React.Suspense>
      ) : null}
    </>
  );
}

// PluginWorkspaceMenu renders the fixed plugin workspace navigation.
// PluginWorkspaceMenu 渲染固定插件工作区导航。
export function PluginWorkspaceMenu(props: {
  // Selected embedded skills sub page.
  // 当前选中的内嵌技能子页面。
  activeSubArea: SkillSubAreaId;
  // Selects one embedded skills sub page.
  // 选择一个内嵌技能子页面。
  onSelectSubArea: (subArea: SkillSubAreaId) => void;
  // Workbench snapshot used to expose settings entries only for enabled plugins.
  // 用于仅向已启用插件暴露设置入口的工作台快照。
  workbench: WorkbenchSnapshot;
}) {
  const { locale, t } = useI18n();
  // pluginRows projects every enabled plugin detail mount into the embedded menu.
  // pluginRows 将每个已启用插件详情挂载点投影到内嵌菜单。
  const pluginRows: Array<{ icon: React.ReactNode; id: SkillSubAreaId; label: string }> = enabledPluginDetailMounts(props.workbench.plugins)
    .map((selection) => ({
      icon: embeddedPluginMountIcon(selection.mount.icon),
      id: selection.subAreaId,
      label: localizedPluginMountLabel(selection, locale),
    }));
  // rows stores the static embedded skills menu rows.
  // rows 保存静态内嵌技能菜单行。
  const rows: Array<{ icon: React.ReactNode; id: SkillSubAreaId; label: string }> = [
    { icon: <Hammer size={15} />, id: "catalog", label: t("skills.navigation.list") },
    { icon: <Settings size={15} />, id: "settings", label: t("skills.navigation.luaSettings") },
    { icon: <Plug size={15} />, id: "mcp-servers", label: t("settings.mcpServers") },
    { icon: <BookMarked size={15} />, id: "knowledge", label: t("area.knowledge") },
    { icon: <Puzzle size={15} />, id: "plugins", label: t("skills.navigation.plugins") },
    { icon: <Wrench size={15} />, id: "plugin-maintenance", label: t("skills.navigation.pluginMaintenance") },
  ];

  return (
    <div className="vk-dir-groups">
    {[{ label: t("skills.navigation.builtins"), items: rows }, { label: t("skills.navigation.externalPlugins"), items: pluginRows }].map((group) => (
    <WorkspaceDirectoryMenu key={group.label} ariaLabel={group.label} heading={group.label}>
      {group.items.map((row) => (
        <DirectoryRow active={props.activeSubArea === row.id} icon={row.icon} key={row.id} label={row.label} onSelect={() => props.onSelectSubArea(row.id)} />
      ))}
    </WorkspaceDirectoryMenu>
    ))}
    </div>
  );
}

// skillSubAreaLabel returns the compact label for one embedded skills sub page.
// skillSubAreaLabel 返回单个内嵌技能子页面的紧凑标签。
export function skillSubAreaLabel(
  subArea: SkillSubAreaId,
  workbench: WorkbenchSnapshot,
  locale: string,
  t: ReturnType<typeof useI18n>["t"],
): string {
  if (subArea === "settings") {
    return t("skills.navigation.luaSettings");
  }
  if (subArea === "plugins") {
    return t("skills.navigation.plugins");
  }
  if (subArea === "plugin-maintenance") {
    return t("skills.navigation.pluginMaintenance");
  }
  if (subArea === "mcp-servers") {
    return t("settings.mcpServers");
  }
  if (subArea === "knowledge") {
    return t("area.knowledge");
  }
  // pluginSelection resolves the selected manifest-owned navigation label.
  // pluginSelection 解析选中的清单自有导航名称。
  const pluginSelection = findEnabledPluginDetailMount(workbench.plugins, subArea);
  if (pluginSelection) {
    return localizedPluginMountLabel(pluginSelection, locale);
  }
  return t("skills.navigation.list");
}

// embeddedPluginMountIcon returns the compact host icon declared by one plugin mount.
// embeddedPluginMountIcon 返回单个插件挂载点声明的紧凑宿主图标。
//
// Parameters: icon is the validated service projection identifier.
// 参数：icon 是已经校验的服务投影标识。
// Returns: the corresponding fifteen-pixel Lucide icon.
// 返回值：返回对应的十五像素 Lucide 图标。
function embeddedPluginMountIcon(icon: "plugin" | "route" | "chart"): React.ReactNode {
  if (icon === "route") {
    return <Route size={15} />;
  }
  if (icon === "chart") {
    return <ChartNoAxesCombined size={15} />;
  }
  return <Puzzle size={15} />;
}
