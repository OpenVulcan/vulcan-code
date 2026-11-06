// Parts of the project home page: the identity header with inline rename, and the sessions card.
// 项目首页的组成部分：带行内重命名的身份页头，以及会话卡片。
import React from "react";
import { Bot, Check, Copy, FolderOpen, MessagesSquare, Pencil, SquarePen, TerminalSquare, X } from "lucide-react";
import { Button, IconButton } from "@/ui-kit/primitives/button";
import { Input } from "@/ui-kit/primitives/input";
import { FilterChip, FilterChipGroup } from "@/ui-kit/primitives/choice-controls";
import { formatMessage, useI18n } from "@/i18n";
import type { ConversationSummary, ProjectSummary, WorkbenchSnapshot } from "@/types";
import { conversationIsRunning } from "@/components/conversation/conversation-runtime";
import { ProjectIcon } from "./project-icons";
import { formatConversationRelativeTime, formatConversationUpdatedAt } from "./project-sidebar-time";
import type { ProjectMenuAction } from "./project-sidebar-types";

// PROJECT_HOME_SESSION_LIMIT bounds the rendered session rows; search reaches the rest.
// PROJECT_HOME_SESSION_LIMIT 限制渲染的会话行数；其余会话通过搜索找到。
const PROJECT_HOME_SESSION_LIMIT = 60;

// ProjectHomeSessionFilter names the session list filters on the project home.
// ProjectHomeSessionFilter 命名项目首页会话列表的筛选项。
type ProjectHomeSessionFilter = "all" | "running" | "unread";

// ProjectHomeConversations groups one project's sessions, newest activity first.
// ProjectHomeConversations 汇总单个项目的会话，最新活动在前。
export interface ProjectHomeConversations {
  // Every session of the project.
  // 项目的全部会话。
  all: ConversationSummary[];
  // Sessions with a turn in progress.
  // 有回合进行中的会话。
  running: ConversationSummary[];
  // Sessions with unread output.
  // 有未读输出的会话。
  unread: ConversationSummary[];
}

// conversationActivityMs returns the newest known activity instant of one conversation, or zero without a timestamp.
// conversationActivityMs 返回单个会话最新的已知活动时刻；没有时间戳时返回零。
function conversationActivityMs(conversation: ConversationSummary): number {
  const updated = Number.parseInt(conversation.lastUpdatedAt ?? "", 10);
  const requested = Number.parseInt(conversation.lastRequestAt ?? "", 10);
  return Math.max(Number.isFinite(updated) ? updated : 0, Number.isFinite(requested) ? requested : 0);
}

// projectHomeConversations lists one project's sessions for the home page.
// projectHomeConversations 为首页列出单个项目的会话。
export function projectHomeConversations(workbench: WorkbenchSnapshot, projectId: string): ProjectHomeConversations {
  const all = workbench.conversations
    .filter((conversation) => conversation.projectId === projectId)
    .sort((left, right) => conversationActivityMs(right) - conversationActivityMs(left));
  return {
    all,
    running: all.filter((conversation) => conversationIsRunning(conversation)),
    unread: all.filter((conversation) => conversation.unread),
  };
}

// ProjectHomeHeader renders the project icon, the name with inline rename, the path and the quick actions.
// Mount it keyed by project so the editor never carries over to another project.
// ProjectHomeHeader 渲染项目图标、可行内重命名的名称、路径与快捷操作。按项目设置 key 挂载，使编辑器不会延续到其他项目。
export function ProjectHomeHeader(props: {
  // Project shown in the header.
  // 页头展示的项目。
  project: ProjectSummary;
  // Renames the project; an empty name restores the folder name. Rejects with the service error.
  // 重命名项目；空名称恢复文件夹名称。失败时以服务错误拒绝。
  onRename: (name: string) => Promise<void>;
  // Runs one project menu action (icon, open directory, open terminal).
  // 执行单个项目菜单动作（图标、打开目录、打开终端）。
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  // Starts a new conversation draft in this project.
  // 在此项目中开始新会话草稿。
  onCreateConversation: (project: ProjectSummary) => void;
  // Starts a new dedicated-assistant draft in this project.
  // 在此项目中开始新的专职助手草稿。
  onCreateDedicatedConversation: (project: ProjectSummary) => void;
}) {
  const { t } = useI18n();
  const project = props.project;
  // renameDraft holds the inline name editor value; null means the name is shown, not edited.
  // renameDraft 保存行内名称编辑值；null 表示显示名称而非编辑。
  const [renameDraft, setRenameDraft] = React.useState<string | null>(null);
  const [renameBusy, setRenameBusy] = React.useState(false);
  const [renameError, setRenameError] = React.useState<string | null>(null);
  // pathCopy reports the last copy-path outcome.
  // pathCopy 报告最近一次复制路径的结果。
  const [pathCopy, setPathCopy] = React.useState<"copied" | "copyFailed" | null>(null);
  // mountedRef drops a late rename result after the header unmounted.
  // mountedRef 在页头卸载后丢弃迟到的重命名结果。
  const mountedRef = React.useRef(true);
  React.useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);

  const closeRename = () => { setRenameDraft(null); setRenameError(null); };

  // copyPath writes the project path and reports failure so the visible path can still be selected by hand.
  // copyPath 写入项目路径并报告失败，使可见路径仍可手动选择复制。
  const copyPath = async () => {
    try { await navigator.clipboard.writeText(project.path); setPathCopy("copied"); }
    catch { setPathCopy("copyFailed"); }
  };

  // submitRename commits the inline name; the editor stays open with the error when the service rejects it.
  // submitRename 提交行内名称；服务拒绝时编辑器保持打开并显示错误。
  const submitRename = async () => {
    if (renameDraft === null || renameBusy) return;
    if (renameDraft.trim() === project.name) { closeRename(); return; }
    setRenameBusy(true);
    try {
      await props.onRename(renameDraft);
      if (mountedRef.current) closeRename();
    } catch (error) {
      if (mountedRef.current) setRenameError(error instanceof Error ? error.message : String(error));
    } finally {
      if (mountedRef.current) setRenameBusy(false);
    }
  };

  const errorId = `project-home-rename-error-${project.id}`;
  return (
    <header className="project-home-head">
      <IconButton
        aria-label={t("projectMenu.setIcon")}
        className="project-home-icon"
        size="lg"
        title={t("projectMenu.setIcon")}
        variant="outline"
        onClick={() => props.onProjectMenuAction("set-icon", project)}
      >
        <ProjectIcon isOpen project={project} size={26} />
      </IconButton>
      <div className="project-home-identity">
        {renameDraft !== null ? (
          <form className="project-home-rename" onSubmit={(event) => { event.preventDefault(); void submitRename(); }}>
            <Input
              aria-describedby={renameError ? errorId : undefined}
              aria-invalid={renameError ? true : undefined}
              aria-label={t("projectMenu.renameProject")}
              autoFocus
              disabled={renameBusy}
              placeholder={t("projectHome.renamePlaceholder")}
              value={renameDraft}
              onChange={(event) => setRenameDraft(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Escape") { event.preventDefault(); closeRename(); }
              }}
            />
            <IconButton aria-label={t("projectHome.renameSave")} disabled={renameBusy} title={t("projectHome.renameSave")} type="submit"><Check /></IconButton>
            <IconButton aria-label={t("projectHome.renameCancel")} disabled={renameBusy} title={t("projectHome.renameCancel")} onClick={closeRename}><X /></IconButton>
          </form>
        ) : (
          <div className="project-home-name">
            <h2 title={project.name}>{project.name}</h2>
            <IconButton aria-label={t("projectMenu.renameProject")} size="xs" title={t("projectMenu.renameProject")} onClick={() => setRenameDraft(project.name)}><Pencil /></IconButton>
          </div>
        )}
        {renameError ? <p className="vk-field-error" id={errorId} role="alert">{renameError}</p> : null}
        <div className="project-home-path">
          <span title={project.path}>{project.path}</span>
          <IconButton aria-label={t("projectOverview.copyPath")} size="xs" title={t("projectOverview.copyPath")} onClick={() => void copyPath()}><Copy /></IconButton>
          <span className="project-home-copy-status" role="status">{pathCopy ? t(`projectOverview.${pathCopy}`) : null}</span>
        </div>
      </div>
      <div className="project-home-actions">
        <Button size="sm" variant="primary" onClick={() => props.onCreateConversation(project)}><SquarePen />{t("projectMenu.newConversation")}</Button>
        <Button size="sm" variant="outline" onClick={() => props.onCreateDedicatedConversation(project)}><Bot />{t("sidebar.newDedicated")}</Button>
        <IconButton aria-label={t("projectMenu.openDirectory")} title={t("projectMenu.openDirectory")} variant="outline" onClick={() => props.onProjectMenuAction("open-directory", project)}><FolderOpen /></IconButton>
        <IconButton aria-label={t("projectMenu.openTerminal")} title={t("projectMenu.openTerminal")} variant="outline" onClick={() => props.onProjectMenuAction("open-terminal", project)}><TerminalSquare /></IconButton>
      </div>
    </header>
  );
}

// ProjectHomeSessions renders the searchable, filterable session list of one project.
// Mount it keyed by project so the search and filter start fresh for each project.
// ProjectHomeSessions 渲染单个项目可搜索、可筛选的会话列表。按项目设置 key 挂载，使搜索与筛选对每个项目重新开始。
export function ProjectHomeSessions(props: {
  // The project's sessions.
  // 项目的会话。
  conversations: ProjectHomeConversations;
  // Instant that anchors relative times for this render.
  // 本次渲染锚定相对时间的时刻。
  nowMs: number;
  // Opens one existing conversation.
  // 打开一个已有会话。
  onSelectConversation: (conversationId: string) => void;
}) {
  const { t, locale } = useI18n();
  const [query, setQuery] = React.useState("");
  const [filter, setFilter] = React.useState<ProjectHomeSessionFilter>("all");
  const { all, running, unread } = props.conversations;
  // normalizedQuery matches titles case-insensitively.
  // normalizedQuery 以不区分大小写的方式匹配标题。
  const normalizedQuery = query.trim().toLowerCase();
  const filtered = (filter === "running" ? running : filter === "unread" ? unread : all)
    .filter((conversation) => !normalizedQuery || conversation.title.toLowerCase().includes(normalizedQuery));
  const visible = filtered.slice(0, PROJECT_HOME_SESSION_LIMIT);

  // chip renders one session filter with its count.
  // chip 渲染带数量的单个会话筛选项。
  const chip = (value: ProjectHomeSessionFilter, key: "projectHome.filterAll" | "projectHome.filterRunning" | "projectHome.filterUnread", count: number) => (
    <FilterChip selected={filter === value} onClick={() => setFilter(value)}>{formatMessage(t(key), { count })}</FilterChip>
  );

  return (
    <div className="project-settings-section">
      <header className="project-settings-heading">
        <MessagesSquare size={18} />
        <div>
          <strong>{t("projectHome.sessionsTitle")}</strong>
          <span>{t("projectHome.sessionsDescription")}</span>
        </div>
      </header>
      <div className="project-home-session-tools">
        <Input
          aria-label={t("projectHome.sessionsSearch")}
          density="compact"
          placeholder={t("projectHome.sessionsSearch")}
          type="search"
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
        />
        <FilterChipGroup aria-label={t("projectHome.filterLabel")}>
          {chip("all", "projectHome.filterAll", all.length)}
          {chip("running", "projectHome.filterRunning", running.length)}
          {chip("unread", "projectHome.filterUnread", unread.length)}
        </FilterChipGroup>
      </div>
      {all.length === 0 ? (
        <p className="project-settings-muted">{t("projectHome.sessionsEmpty")}</p>
      ) : visible.length === 0 ? (
        <p className="project-settings-muted">{t("projectHome.sessionsNoMatch")}</p>
      ) : (
        <ul className="project-home-session-list">
          {visible.map((conversation) => (
            <li key={conversation.id}>
              <button
                className="project-home-session"
                data-running={conversationIsRunning(conversation) ? "" : undefined}
                data-unread={conversation.unread ? "" : undefined}
                title={formatConversationUpdatedAt(conversation, locale) ?? undefined}
                type="button"
                onClick={() => props.onSelectConversation(conversation.id)}
              >
                <span className="project-home-session-mark" aria-hidden="true" />
                <span className="project-home-session-title">{conversation.title}</span>
                {conversation.dedicated ? <Bot aria-label={conversation.assistantName ?? t("sidebar.newDedicated")} size={13} /> : null}
                <small>{formatConversationRelativeTime(conversation, props.nowMs, locale)}</small>
              </button>
            </li>
          ))}
        </ul>
      )}
      {filtered.length > visible.length ? (
        <p className="project-settings-muted">{formatMessage(t("projectHome.sessionsMore"), { count: filtered.length - visible.length })}</p>
      ) : null}
    </div>
  );
}
