import React from "react";
import {
  ChevronsUpDown,
  CodeXml,
  Folder,
  FolderPlus,
  MessagesSquare,
  Pin,
  Search,
  SquarePen,
  UserCog,
} from "lucide-react";
import { Button, IconButton } from "@/ui-kit/primitives/button";
import { Input } from "@/ui-kit/primitives/input";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui-kit/primitives/tooltip";
import {
  Sidebar,
  SidebarContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuItem,
  useSidebarNavigation,
} from "@/ui-kit/shell/sidebar";
import { SidebarBrandMenu } from "@/components/sidebar/sidebar-profile-menus";
import { useI18n, type TranslationKey } from "@/i18n";
import type { ConversationSummary, ProjectSummary, ProjectTreeSummary } from "@/types";
import { ConversationTreeRow, type ConversationRowVariant } from "./project-sidebar-conversation-row";
import { EMPTY_PROJECT_TREE } from "./project-group-layout";
import { ProjectTreeSection, type ProjectGroupAction, type ProjectRowPlacement } from "./project-sidebar-groups";
import { SortableProjectRow } from "./project-sidebar-project-row";
import type { SortableRowBinding } from "./project-sidebar-sortable";
import { ProjectIcon } from "./project-icons";
import type { ConversationMenuAction, ProjectMenuAction } from "./project-sidebar-types";
import { handleSessionTreeKey } from "./session-tree-keys";
import { setSidebarFlow, useSidebarFlow, type SidebarFlow } from "./sidebar-flow";

export type { ConversationMenuAction, ProjectMenuAction } from "./project-sidebar-types";
export type { ProjectGroupAction } from "./project-sidebar-groups";

// ProjectSessionTreeProps describes the session navigation shared by the session panel and the rail's session flyout.
// ProjectSessionTreeProps 描述会话面板与轨道会话浮层共用的会话导航。
export type ProjectSessionTreeProps = {
  projects: ProjectSummary[];
  conversations: ConversationSummary[];
  expandedProjectIds?: string[];
  // Project tree shown by the code flow: groups, the default group position and the pinned section.
  // 代码流显示的项目树：分组、默认分组位置与置顶分区。
  projectTree?: ProjectTreeSummary;
  pinnedConversationIds: Set<string>;
  selectedConversationId: string | null;
  // Explicit project that owns project-level floating tools.
  // 持有项目级浮动工具的明确项目。
  activeProjectId: string | null;
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  onCreateConversation: (project: ProjectSummary) => void;
  // Opens a new draft preset to a dedicated assistant; the entry is hidden when absent.
  // 打开预设为专职助手的新草稿；缺省时隐藏该入口。
  onCreateDedicatedConversation?: (project: ProjectSummary) => void;
  // Opens the add-project flow; a group id places the chosen folder in that group.
  // 打开添加项目流程；给出分组标识时，所选文件夹放入该分组。
  onCreateProject: (groupId?: string) => void;
  onExpandedProjectIdsChange: (projectIds: string[]) => void;
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  // Persists one project-tree change; rejects with the service error.
  // 持久化一次项目树变更；失败时以服务错误拒绝。
  onProjectGroupAction: (action: ProjectGroupAction) => Promise<void>;
  // Selects one project without selecting a conversation.
  // 选择一个项目且不选择会话。
  onSelectProject: (projectId: string) => void;
  onSelectConversation: (conversationId: string) => void;
};

// ProjectSidebar renders the session panel: the brand strip with the flow switch above the shared navigation.
// ProjectSidebar 渲染会话面板：带流切换的品牌条及其下方的共用导航。
export function ProjectSidebar(props: ProjectSessionTreeProps & {
  // Always-visible activity rail rendered beside this panel.
  // 渲染在此面板旁、始终可见的活动轨道。
  rail: React.ReactNode;
  // Whether the sidebar brand content should be a native window drag region.
  // 侧栏品牌内容是否作为原生窗口拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
}) {
  const { rail, titlebarDragRegion, onTitlebarDoubleClick, ...navigationProps } = props;
  return (
    <Sidebar collapsible="offcanvas" rail={rail} variant="inset">
      <SidebarHeader>
        <SidebarBrandMenu
          actions={<SidebarFlowSwitch />}
          titlebarDragRegion={titlebarDragRegion}
          onTitlebarDoubleClick={onTitlebarDoubleClick}
        />
      </SidebarHeader>
      <SidebarContent>
        <NavigationPanelBody {...navigationProps} />
      </SidebarContent>
    </Sidebar>
  );
}

// SIDEBAR_FLOWS lists the flow switch buttons with their icon, label and hint keys.
// SIDEBAR_FLOWS 列出流切换按钮及其图标、标签与提示键。
const SIDEBAR_FLOWS: ReadonlyArray<{ flow: SidebarFlow; icon: React.ReactNode; label: TranslationKey; hint: TranslationKey }> = [
  { flow: "session", icon: <MessagesSquare aria-hidden="true" />, label: "sidebar.flow.session", hint: "sidebar.flow.sessionHint" },
  { flow: "code", icon: <CodeXml aria-hidden="true" />, label: "sidebar.flow.code", hint: "sidebar.flow.codeHint" },
];

// SidebarFlowSwitch toggles between the session flow and the code flow beside the wordmark.
// SidebarFlowSwitch 在字标旁切换会话流与代码流。
export function SidebarFlowSwitch() {
  const { t } = useI18n();
  const flow = useSidebarFlow();
  return (
    <div aria-label={t("sidebar.flow.label")} className="vk-flow-switch" role="group">
      {SIDEBAR_FLOWS.map((entry) => (
        <Tooltip key={entry.flow}>
          <TooltipTrigger asChild>
            <button
              aria-label={t(entry.label)}
              aria-pressed={flow === entry.flow}
              onClick={() => setSidebarFlow(entry.flow)}
              type="button"
            >
              {entry.icon}
            </button>
          </TooltipTrigger>
          <TooltipContent side="bottom">{t(entry.hint)}</TooltipContent>
        </Tooltip>
      ))}
    </div>
  );
}

// NavigationPanelBody renders the panel tools above the list of the chosen flow. The panel and the rail flyout
// both mount it, so every surface that lists sessions looks and behaves the same.
// NavigationPanelBody 在所选流的列表上方渲染面板工具。面板与轨道浮层都挂载它，使所有列出会话的界面外观与行为一致。
export function NavigationPanelBody(props: ProjectSessionTreeProps) {
  const { t } = useI18n();
  const flow = useSidebarFlow();
  const navigate = useSidebarNavigation();
  // query filters the list by session title, and by project name in the code flow.
  // query 按会话标题筛选列表；代码流中同时按项目名称筛选。
  const [query, setQuery] = React.useState("");
  const normalizedQuery = query.trim().toLocaleLowerCase();
  // currentProject is the project the session flow lists and the new-session actions target.
  // currentProject 是会话流所列出的项目，也是新建会话操作的目标。
  const currentProject = resolveCurrentProject(props);
  return (
    <div className="vk-nav-flow" data-flow={flow}>
      <div className="vk-nav-tools">
        {flow === "session" && currentProject ? (
          <ProjectSwitch
            currentProject={currentProject}
            onCreateProject={() => props.onCreateProject()}
            onSelectProject={props.onSelectProject}
            projects={props.projects}
          />
        ) : null}
        {currentProject ? (
          <div className="vk-sidebar-actions">
            <Button onClick={() => navigate(props.onCreateConversation)(currentProject)} size="sm" type="button">
              <SquarePen aria-hidden="true" />
              {t("sidebar.newSession")}
            </Button>
            {props.onCreateDedicatedConversation ? (
              <Tooltip>
                <TooltipTrigger asChild>
                  <IconButton
                    aria-label={t("sidebar.newDedicated")}
                  data-action="new-dedicated"
                    onClick={() => navigate(props.onCreateDedicatedConversation!)(currentProject)}
                    variant="outline"
                  >
                    <UserCog aria-hidden="true" />
                  </IconButton>
                </TooltipTrigger>
                <TooltipContent side="bottom">{t("sidebar.newDedicated")}</TooltipContent>
              </Tooltip>
            ) : null}
            <Tooltip>
              <TooltipTrigger asChild>
                <IconButton
                  aria-label={t("sidebar.newProject")}
                  data-action="new-project"
                  onClick={() => navigate(props.onCreateProject)()}
                  variant="outline"
                >
                  <FolderPlus aria-hidden="true" />
                </IconButton>
              </TooltipTrigger>
              <TooltipContent side="bottom">{t("sidebar.newProject")}</TooltipContent>
            </Tooltip>
          </div>
        ) : null}
        {props.projects.length > 0 ? (
          <div className="vk-input-wrap">
            <Search aria-hidden="true" />
            <Input
              aria-label={t(flow === "session" ? "sidebar.searchSessions" : "sidebar.searchTree")}
              density="compact"
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Escape" && query) {
                  event.stopPropagation();
                  setQuery("");
                }
              }}
              placeholder={t(flow === "session" ? "sidebar.searchSessions" : "sidebar.searchTree")}
              type="search"
              value={query}
            />
          </div>
        ) : null}
      </div>
      <div className="vk-nav-list" onKeyDown={(event) => { handleSessionTreeKey(event); }}>
        {props.projects.length === 0 ? (
          <NoProjectsEntry onCreateProject={() => navigate(props.onCreateProject)()} />
        ) : flow === "session" && currentProject ? (
          <SessionFlowList {...props} currentProject={currentProject} query={normalizedQuery} />
        ) : (
          <ProjectSessionTree {...props} query={normalizedQuery} />
        )}
      </div>
    </div>
  );
}

// resolveCurrentProject picks the explicit active project, then the selected session's project, then the first project.
// resolveCurrentProject 依次选取明确的活动项目、所选会话的项目、第一个项目。
function resolveCurrentProject(props: ProjectSessionTreeProps): ProjectSummary | null {
  const byId = (id: string | null | undefined) => (id ? props.projects.find((project) => project.id === id) : undefined);
  const selected = props.conversations.find((conversation) => conversation.id === props.selectedConversationId);
  return byId(props.activeProjectId) ?? byId(selected?.projectId) ?? props.projects[0] ?? null;
}

// ProjectSwitch shows the session flow's project and lists every project to switch to.
// ProjectSwitch 显示会话流当前的项目，并列出可切换的全部项目。
function ProjectSwitch(props: {
  // Project the session flow currently lists.
  // 会话流当前列出的项目。
  currentProject: ProjectSummary;
  // Every project available to switch to.
  // 可切换到的全部项目。
  projects: ProjectSummary[];
  // Makes one project current.
  // 将一个项目设为当前项目。
  onSelectProject: (projectId: string) => void;
  // Opens the add-project flow.
  // 打开添加项目流程。
  onCreateProject: () => void;
}) {
  const { t } = useI18n();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button aria-label={t("sidebar.switchProject")} className="vk-project-switch" title={props.currentProject.path} type="button">
          <span className="vk-project-mark"><ProjectIcon project={props.currentProject} size={15} /></span>
          <span className="vk-project-switch-text">
            <strong>{props.currentProject.name}</strong>
            <small>{props.currentProject.path}</small>
          </span>
          <ChevronsUpDown aria-hidden="true" className="vk-faint" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="vk-project-switch-menu">
        {props.projects.map((project) => (
          <DropdownMenuItem
            data-current={project.id === props.currentProject.id ? "" : undefined}
            key={project.id}
            onSelect={() => props.onSelectProject(project.id)}
          >
            <ProjectIcon project={project} size={15} />
            <span>{project.name}</span>
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={props.onCreateProject}>
          <FolderPlus aria-hidden="true" />
          <span>{t("sidebar.addProject")}</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// NoProjectsEntry is the actionable empty state shown before any project exists.
// NoProjectsEntry 是尚无项目时显示的可操作空状态。
function NoProjectsEntry(props: { onCreateProject: () => void }) {
  const { t } = useI18n();
  // WorkbenchApp mounts this panel only after the authoritative snapshot exists.
  // WorkbenchApp 仅在权威快照存在后挂载此面板。
  return (
    <button className="vk-nav-start" onClick={props.onCreateProject} type="button">
      <FolderPlus aria-hidden="true" />
      <span>
        <strong>{t("sidebar.noProjects")}</strong>
        <small>{t("project.sidebar.addAFolderTo")}</small>
      </span>
    </button>
  );
}

// useRelativeNow refreshes relative time labels once a minute without touching hover or confirmation state.
// useRelativeNow 每分钟刷新一次相对时间标签，不触碰悬停或确认状态。
function useRelativeNow(): number {
  const [nowMs, setNowMs] = React.useState(() => Date.now());
  React.useEffect(() => {
    const intervalId = window.setInterval(() => setNowMs(Date.now()), 60_000);
    return () => window.clearInterval(intervalId);
  }, []);
  return nowMs;
}

// listedConversationsOf excludes sessions floating as assistants; they live in the top-bar assistant dock.
// listedConversationsOf 排除作为助手浮动的会话；它们位于顶部助手停靠中。
function listedConversationsOf(conversations: ConversationSummary[]): ConversationSummary[] {
  return conversations.filter((conversation) => !conversation.assistantName);
}

// matchesQuery reports whether a lower-cased query occurs in one text; an empty query matches everything.
// matchesQuery 判断小写查询是否出现在文本中；空查询匹配所有内容。
function matchesQuery(text: string, query: string): boolean {
  return query === "" || text.toLocaleLowerCase().includes(query);
}

// TreeHead renders a quiet section heading with optional trailing actions.
// TreeHead 渲染安静的分区标题及可选的尾部操作。
function TreeHead(props: { icon: React.ReactNode; label: string; children?: React.ReactNode }) {
  return (
    <div className="vk-tree-head">
      <span className="vk-label">{props.icon}{props.label}</span>
      {props.children}
    </div>
  );
}

// SessionWhen names the recency buckets of the session flow, newest first.
// SessionWhen 命名会话流的最近程度分组，按从新到旧排列。
type SessionWhen = "today" | "yesterday" | "week" | "month" | "older";

// SESSION_WHEN_ORDER fixes the bucket order and the catalog key of each heading.
// SESSION_WHEN_ORDER 固定分组顺序及各标题的目录键。
const SESSION_WHEN_ORDER: ReadonlyArray<[SessionWhen, TranslationKey]> = [
  ["today", "sidebar.when.today"],
  ["yesterday", "sidebar.when.yesterday"],
  ["week", "sidebar.when.week"],
  ["month", "sidebar.when.month"],
  ["older", "sidebar.when.older"],
];

// sessionWhen places one conversation in a calendar bucket relative to local midnight; unknown times are older.
// sessionWhen 以本地零点为基准将会话放入日历分组；时间未知的归入更早。
export function sessionWhen(conversation: ConversationSummary, nowMs: number): SessionWhen {
  const updatedAtMs = Number.parseInt(conversation.lastUpdatedAt ?? "", 10);
  if (!Number.isFinite(updatedAtMs) || updatedAtMs <= 0) return "older";
  const midnight = new Date(nowMs);
  midnight.setHours(0, 0, 0, 0);
  const dayMs = 24 * 60 * 60 * 1000;
  const startOfToday = midnight.getTime();
  if (updatedAtMs >= startOfToday) return "today";
  if (updatedAtMs >= startOfToday - dayMs) return "yesterday";
  if (updatedAtMs >= startOfToday - 6 * dayMs) return "week";
  if (updatedAtMs >= startOfToday - 29 * dayMs) return "month";
  return "older";
}

// SessionFlowList lists the current project's sessions by recency, pinned ones first.
// SessionFlowList 按最近程度列出当前项目的会话，置顶会话在前。
function SessionFlowList(props: ProjectSessionTreeProps & {
  // Project whose sessions are listed.
  // 列出其会话的项目。
  currentProject: ProjectSummary;
  // Lower-cased title filter; empty lists everything.
  // 小写标题筛选；为空时列出全部。
  query: string;
}) {
  const { t, locale } = useI18n();
  const navigate = useSidebarNavigation();
  const nowMs = useRelativeNow();
  const projectSessions = React.useMemo(
    () => listedConversationsOf(props.conversations)
      .filter((conversation) => conversation.projectId === props.currentProject.id && matchesQuery(conversation.title, props.query)),
    [props.conversations, props.currentProject.id, props.query],
  );
  // pinOrder follows the service's pinned order, which the pin set preserves.
  // pinOrder 遵循服务的置顶顺序，置顶集合保留了该顺序。
  const pinOrder = [...props.pinnedConversationIds];
  const pinned = projectSessions
    .filter((conversation) => props.pinnedConversationIds.has(conversation.id))
    .sort((left, right) => pinOrder.indexOf(left.id) - pinOrder.indexOf(right.id));
  const buckets = new Map<SessionWhen, ConversationSummary[]>();
  for (const conversation of projectSessions) {
    if (props.pinnedConversationIds.has(conversation.id)) continue;
    const when = sessionWhen(conversation, nowMs);
    buckets.set(when, [...(buckets.get(when) ?? []), conversation]);
  }
  const renderRow = (conversation: ConversationSummary, variant: ConversationRowVariant) => (
    <ConversationTreeRow
      compactPinned={false}
      conversation={conversation}
      isPinned={props.pinnedConversationIds.has(conversation.id)}
      isSelected={props.selectedConversationId === conversation.id}
      key={conversation.id}
      locale={locale}
      nowMs={nowMs}
      onAction={props.onConversationMenuAction}
      onSelectConversation={navigate(props.onSelectConversation)}
      variant={variant}
    />
  );
  if (projectSessions.length === 0) {
    return <div className="vk-tree-blank">{t(props.query ? "sidebar.noMatches" : "projectRow.noConversations")}</div>;
  }
  return (
    <div className="vk-session-flow">
      {pinned.length > 0 ? (
        <section>
          <TreeHead icon={<Pin aria-hidden="true" />} label={t("sidebar.pinned")} />
          <SidebarMenu>{pinned.map((conversation) => renderRow(conversation, "flow"))}</SidebarMenu>
        </section>
      ) : null}
      {SESSION_WHEN_ORDER.map(([when, label]) => {
        const sessions = buckets.get(when);
        if (!sessions?.length) return null;
        return (
          <section key={when}>
            <div className="vk-session-group vk-label">{t(label)}</div>
            <SidebarMenu>{sessions.map((conversation) => renderRow(conversation, "flow"))}</SidebarMenu>
          </section>
        );
      })}
    </div>
  );
}

// ProjectSessionTree renders pinned conversations, projects and their conversations, and unlinked conversations.
// ProjectSessionTree 渲染置顶会话、项目及其会话，以及未关联项目的会话。
function ProjectSessionTree(props: ProjectSessionTreeProps & {
  // Lower-cased filter over project names and session titles; empty lists everything.
  // 对项目名称与会话标题的小写筛选；为空时列出全部。
  query: string;
}) {
  // t translates stable sidebar labels while project names remain user data.
  // t 翻译稳定侧栏标签，项目名称仍保留为用户数据。
  const { t, locale } = useI18n();
  // navigate preserves each destination action and closes phone navigation.
  // navigate 保留各导航动作并关闭手机导航。
  const navigate = useSidebarNavigation();
  const nowMs = useRelativeNow();
  // openProjectIds stores the project folders currently expanded in the tree.
  // openProjectIds 保存当前在树中展开的项目文件夹。
  const [openProjectIds, setOpenProjectIds] = React.useState<Set<string>>(
    () => initialExpandedProjectIdSet(props.projects, props.expandedProjectIds),
  );
  // openProjectIdsRef lets event handlers serialize rapid toggles without reading state during an updater.
  // openProjectIdsRef 让事件处理器在不于 updater 中读取状态的情况下串行化快速切换。
  const openProjectIdsRef = React.useRef(openProjectIds);
  // sorting records when a project or group drag is active so hover-only UI can pause.
  // sorting 记录项目或分组拖动是否进行中，以便暂停仅悬停显示的界面。
  const [sorting, setSorting] = React.useState(false);
  // expandedConversationProjectIds stores project ids whose conversation list is fully expanded.
  // expandedConversationProjectIds 保存会话列表已完整展开的项目标识。
  const [expandedConversationProjectIds, setExpandedConversationProjectIds] = React.useState<Set<string>>(() => new Set());
  // projectIds stores the explicit project identifiers available in this snapshot.
  // projectIds 保存当前快照中明确存在的项目标识。
  const projectIds = React.useMemo(() => new Set(props.projects.map((project) => project.id)), [props.projects]);
  // projectsById resolves conversation project icons without changing conversation data.
  // projectsById 在不改变会话数据的情况下解析会话所属项目图标。
  const projectsById = React.useMemo(() => new Map(props.projects.map((project) => [project.id, project])), [props.projects]);
  const listedConversations = React.useMemo(() => listedConversationsOf(props.conversations), [props.conversations]);
  // conversationsByProject groups conversations beneath their owning project folder, in service recency order.
  // conversationsByProject 按服务端最近活动顺序将会话归组到所属项目文件夹下。
  const conversationsByProject = React.useMemo(() => {
    return listedConversations.reduce<Record<string, ConversationSummary[]>>((groups, conversation) => {
      const projectId = conversation.projectId;
      if (!projectId || !projectIds.has(projectId)) {
        return groups;
      }
      groups[projectId] = groups[projectId] ?? [];
      groups[projectId].push(conversation);
      return groups;
    }, {});
  }, [projectIds, listedConversations]);
  // unlinkedConversations keeps conversations without a valid project owner in a diagnostic group.
  // unlinkedConversations 将没有有效项目归属的会话保留在诊断分组中。
  const unlinkedConversations = listedConversations.filter((conversation) => (
    !props.pinnedConversationIds.has(conversation.id)
    && (!conversation.projectId || !projectIds.has(conversation.projectId))
    && matchesQuery(conversation.title, props.query)
  ));
  React.useEffect(() => {
    // next is the externally authoritative expansion projection for this render.
    // next 是本次渲染对应的外部权威展开投影。
    const next = initialExpandedProjectIdSet(props.projects, props.expandedProjectIds);
    openProjectIdsRef.current = next;
    setOpenProjectIds(next);
  }, [props.expandedProjectIds, props.projects]);
  React.useEffect(() => {
    const visibleProjectIds = new Set(props.projects.map((project) => project.id));
    setExpandedConversationProjectIds((current) => {
      const next = new Set([...current].filter((projectId) => visibleProjectIds.has(projectId)));
      return next.size === current.size ? current : next;
    });
  }, [props.projects]);
  // toggleProjectOpen toggles one project folder without changing the selected conversation.
  // toggleProjectOpen 切换单个项目文件夹展开状态且不改变当前会话选择。
  const toggleProjectOpen = React.useCallback((projectId: string) => {
    // next is the event-ordered expansion result used for both local state and persistence.
    // next 是按事件顺序得到的展开结果，同时用于本地状态和持久化。
    const next = new Set(openProjectIdsRef.current);
    if (next.has(projectId)) {
      next.delete(projectId);
    } else {
      next.add(projectId);
    }
    // Keep the state updater pure; parent notification belongs to this user event boundary.
    // 保持状态 updater 纯净；父组件通知属于当前用户事件边界。
    openProjectIdsRef.current = next;
    setOpenProjectIds(next);
    props.onExpandedProjectIdsChange(expandedProjectIdsInProjectOrder(props.projects, next));
  }, [props.onExpandedProjectIdsChange, props.projects]);
  // toggleProjectConversations toggles whether one project shows every conversation row.
  // toggleProjectConversations 切换单个项目是否显示全部会话行。
  const toggleProjectConversations = React.useCallback((projectId: string) => {
    setExpandedConversationProjectIds((current) => {
      const next = new Set(current);
      if (next.has(projectId)) {
        next.delete(projectId);
      } else {
        next.add(projectId);
      }
      return next;
    });
  }, []);
  // renderConversationRow renders a conversation row outside or inside one project folder.
  // renderConversationRow 渲染项目内外复用的会话行。
  const renderConversationRow = (conversation: ConversationSummary, compactPinned = false, sortable?: SortableRowBinding) => (
    <ConversationTreeRow
      compactPinned={compactPinned}
      sortable={sortable}
      conversation={conversation}
      isPinned={props.pinnedConversationIds.has(conversation.id)}
      isSelected={props.selectedConversationId === conversation.id}
      key={conversation.id}
      locale={locale}
      nowMs={nowMs}
      onAction={props.onConversationMenuAction}
      onSelectConversation={navigate(props.onSelectConversation)}
      project={conversation.projectId ? projectsById.get(conversation.projectId) : undefined}
    />
  );
  // filtering keeps matching projects open and drops projects with neither a name nor a session match.
  // 筛选时保持匹配项目展开，并去掉名称与会话均不匹配的项目。
  const filtering = props.query !== "";
  const isProjectVisible = (project: ProjectSummary) => !filtering
    || matchesQuery(project.name, props.query)
    || (conversationsByProject[project.id] ?? []).some((conversation) => matchesQuery(conversation.title, props.query));
  // renderProjectRow renders one project row; placement offers its move-to-group entries.
  // renderProjectRow 渲染一个项目行；placement 提供其“移动到分组”入口。
  const renderProjectRow = (project: ProjectSummary, placement: ProjectRowPlacement | null, sortableId?: string) => {
    const allConversations = conversationsByProject[project.id] ?? [];
    const nameMatches = matchesQuery(project.name, props.query);
    const projectConversations = allConversations
      .filter((conversation) => !props.pinnedConversationIds.has(conversation.id))
      .filter((conversation) => nameMatches || matchesQuery(conversation.title, props.query));
    return (
      <SortableProjectRow
        conversations={projectConversations}
        hasSelectedConversation={props.selectedConversationId ? allConversations.some((conversation) => conversation.id === props.selectedConversationId) : false}
        isActiveProject={props.activeProjectId === project.id}
        isOpen={filtering || openProjectIds.has(project.id)}
        isConversationListExpanded={filtering || expandedConversationProjectIds.has(project.id)}
        isPinnedConversation={(conversationId) => props.pinnedConversationIds.has(conversationId)}
        key={project.id}
        onConversationMenuAction={props.onConversationMenuAction}
        onCreateConversation={navigate(props.onCreateConversation)}
        onProjectMenuAction={props.onProjectMenuAction}
        renderConversationRow={renderConversationRow}
        onSelectConversation={navigate(props.onSelectConversation)}
        onSelectProject={props.onSelectProject}
        onToggleConversationList={toggleProjectConversations}
        onToggleOpen={toggleProjectOpen}
        placement={placement}
        project={project}
        selectedConversationId={props.selectedConversationId}
        sortableId={sortableId}
      />
    );
  };

  return (
    <div className="vk-project-tree" data-sorting={sorting ? "" : undefined}>
      <ProjectTreeSection
        conversations={listedConversations}
        emptyNotice={unlinkedConversations.length === 0
          ? <SidebarMenuItem><div className="vk-tree-blank">{t("sidebar.noMatches")}</div></SidebarMenuItem>
          : null}
        filtering={filtering}
        isConversationVisible={(conversation) => matchesQuery(conversation.title, props.query)}
        isProjectVisible={isProjectVisible}
        onCreateProject={(groupId) => navigate(props.onCreateProject)(groupId)}
        onGroupAction={props.onProjectGroupAction}
        onSortingChange={setSorting}
        projectTree={props.projectTree ?? EMPTY_PROJECT_TREE}
        projects={props.projects}
        renderPinnedConversation={(conversation, sortable) => renderConversationRow(conversation, true, sortable)}
        renderProjectRow={renderProjectRow}
        sessionCount={(projectId) => conversationsByProject[projectId]?.length ?? 0}
      />
      {unlinkedConversations.length > 0 ? (
        <section>
          <TreeHead icon={<Folder aria-hidden="true" />} label={t("sidebar.unlinkedConversations")} />
          <SidebarMenu>
            {unlinkedConversations.map((conversation) => renderConversationRow(conversation))}
          </SidebarMenu>
        </section>
      ) : null}
    </div>
  );
}

// initialExpandedProjectIdSet restores project expansion, defaulting missing state to fully expanded.
// initialExpandedProjectIdSet 恢复项目展开状态，并在缺少状态时默认全部展开。
function initialExpandedProjectIdSet(
  projects: ProjectSummary[],
  expandedProjectIds: string[] | undefined,
): Set<string> {
  if (!expandedProjectIds) {
    return new Set(projects.map((project) => project.id));
  }
  const projectIds = new Set(projects.map((project) => project.id));
  return new Set(expandedProjectIds.filter((projectId) => projectIds.has(projectId)));
}

// expandedProjectIdsInProjectOrder serializes expanded project ids in the visible project order.
// expandedProjectIdsInProjectOrder 按可见项目顺序序列化已展开项目标识。
function expandedProjectIdsInProjectOrder(
  projects: ProjectSummary[],
  openProjectIds: Set<string>,
): string[] {
  return projects
    .filter((project) => openProjectIds.has(project.id))
    .map((project) => project.id);
}
