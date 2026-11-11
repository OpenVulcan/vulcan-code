import React from "react";
import { FolderCog, FolderTree, MessagesSquare } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { useI18n } from "@/i18n";
import type { ProjectSummary, VmmConfigDraftSummary, WorkbenchSnapshot } from "@/types";
import {
  ProjectAgentSkillsSection,
  ProjectAttachmentCleanupSection,
  ProjectVmmSection,
} from "./project-settings-sections";
import { ProjectOverviewSection } from "./project-overview-section";
import { ProjectAttachedFoldersSection } from "./project-attached-folders-section";
import { ProjectKnowledgeBasesSection } from "./project-knowledge-bases-section";
import { ProjectCollaborationGrantsSection } from "./project-collaboration-grants-section";
import { ProjectPermissionJudgeSection } from "./project-permission-judge-section";
import { formatConversationRelativeTime, formatConversationUpdatedAt } from "./project-sidebar-time";
import type { ProjectMenuAction } from "./project-sidebar-types";
import type {
  ProjectAgentSkillState,
  ProjectAttachmentCleanupPreviewSummary,
  ProjectAttachmentCleanupReportSummary,
  ProjectAttachmentCleanupRequest,
  ProjectSettingsRevision,
  ProjectSettingsSectionId,
} from "./project-settings-types";
import {
  projectAttachmentRoot,
  projectConversationCount,
  projectSettingsSections,
} from "./project-settings-utils";
import { ProjectHomeHeader, ProjectHomeSessions, projectHomeConversations } from "./project-home-parts";
import { useProjectAgentSkills, useProjectAttachmentCleanup, useProjectVmmDraft } from "./project-home-state";
import "@/ui-kit/styles/features/project-settings.css";

// projectHomeCardId names the anchor of one home card so overview links and the jump bar can scroll to it.
// projectHomeCardId 命名单张首页卡片的锚点，供概览链接与跳转条滚动定位。
export function projectHomeCardId(projectId: string, section: ProjectSettingsSectionId | "sessions" | "attached-folders"): string {
  return `project-home-${projectId}-${section}`;
}

// ProjectHome renders one project's home page in the main stage: identity, sessions and every project setting as cards.
// ProjectHome 在主舞台渲染单个项目的首页：项目身份、会话以及以卡片呈现的全部项目设置。
export function ProjectHome(props: {
  // Project shown on this home page; the latest workbench projection, so renames and icons appear at once.
  // 此首页展示的项目；取最新工作台投影，使重命名与图标立即生效。
  project: ProjectSummary;
  // Workbench snapshot supplying catalogs, conversations and counters.
  // 提供目录、会话与计数的工作台快照。
  workbench: WorkbenchSnapshot;
  // Latest accepted invalidation revisions for this project's independently loaded settings.
  // 当前项目独立加载设置的最新已接收失效版本。
  settingsRevision: ProjectSettingsRevision;
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
  // Opens one existing conversation.
  // 打开一个已有会话。
  onSelectConversation: (conversationId: string) => void;
  // Loads project Agent Skill overrides.
  // 加载项目 Agent Skill 覆盖。
  onLoadAgentSkillOverrides: (projectId: string) => Promise<Record<string, string>>;
  // Loads the project VMM draft.
  // 加载项目 VMM 草稿。
  onLoadVmmDraft: (projectId: string) => Promise<VmmConfigDraftSummary>;
  // Saves the project VMM draft.
  // 保存项目 VMM 草稿。
  onSaveVmmDraft: (projectId: string, draft: VmmConfigDraftSummary) => Promise<void>;
  // Saves one project Agent Skill tri-state override.
  // 保存单个项目 Agent Skill 三态覆盖。
  onSetAgentSkillState: (projectId: string, name: string, stateKey: ProjectAgentSkillState) => Promise<void>;
  // Loads a managed attachment cleanup preview for the current project.
  // 加载当前项目的托管附件清理预览。
  onPreviewAttachmentCleanup: (request: ProjectAttachmentCleanupRequest) => Promise<ProjectAttachmentCleanupPreviewSummary>;
  // Executes one managed attachment cleanup for the current project.
  // 执行当前项目的一次托管附件清理。
  onExecuteAttachmentCleanup: (request: ProjectAttachmentCleanupRequest) => Promise<ProjectAttachmentCleanupReportSummary>;
  // Opens knowledge library management.
  // 打开知识库管理。
  onOpenKnowledge?: () => void;
}) {
  // t resolves stable WebUI copy keys for the active language; locale formats session times.
  // t 根据当前语言解析稳定的 WebUI 文案键；locale 用于格式化会话时间。
  const { t, locale } = useI18n();
  const project = props.project;
  const vmm = useProjectVmmDraft({
    projectId: project.id,
    revision: props.settingsRevision.vmm,
    onLoad: props.onLoadVmmDraft,
    onSave: props.onSaveVmmDraft,
  });
  const agent = useProjectAgentSkills({
    projectId: project.id,
    revision: props.settingsRevision.agentSkills,
    onLoad: props.onLoadAgentSkillOverrides,
    onSet: props.onSetAgentSkillState,
  });
  const cleanup = useProjectAttachmentCleanup({
    projectId: project.id,
    onPreview: props.onPreviewAttachmentCleanup,
    onExecute: props.onExecuteAttachmentCleanup,
  });
  // nowMs anchors relative session times for one render.
  // nowMs 为一次渲染锚定会话的相对时间。
  const nowMs = Date.now();
  const sections = projectSettingsSections(project, props.workbench, t, vmm.hasOverride);
  const attachmentRoot = projectAttachmentRoot(project);
  const conversationCount = projectConversationCount(props.workbench, project.id);
  const conversations = React.useMemo(
    () => projectHomeConversations(props.workbench, project.id),
    [props.workbench, project.id],
  );
  const latestConversation = conversations.all[0] ?? null;

  // scrollToCard brings one card into view; overview links and the jump bar share it.
  // scrollToCard 将单张卡片滚动到可见区域；概览链接与跳转条共用。
  const scrollToCard = (section: ProjectSettingsSectionId | "sessions" | "attached-folders") => {
    document.getElementById(projectHomeCardId(project.id, section))?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  // card renders one anchored home card around an existing settings section.
  // card 将既有设置分区包裹为一张带锚点的首页卡片。
  const card = (section: ProjectSettingsSectionId | "sessions" | "attached-folders", content: React.ReactNode) => (
    <section className="project-home-card" id={projectHomeCardId(project.id, section)} key={section}>{content}</section>
  );

  return (
    <div className="project-home" data-project-id={project.id}>
      <ProjectHomeHeader
        key={project.id}
        project={project}
        onCreateConversation={props.onCreateConversation}
        onCreateDedicatedConversation={props.onCreateDedicatedConversation}
        onProjectMenuAction={props.onProjectMenuAction}
        onRename={props.onRename}
      />

      <dl className="project-home-stats">
        <div><dt>{t("projectHome.statSessions")}</dt><dd>{conversationCount}</dd></div>
        <div data-tone={conversations.running.length > 0 ? "live" : undefined}><dt>{t("projectHome.statRunning")}</dt><dd>{conversations.running.length}</dd></div>
        <div><dt>{t("projectHome.statUnread")}</dt><dd>{conversations.unread.length}</dd></div>
        <div>
          <dt>{t("projectHome.statLastActivity")}</dt>
          <dd title={latestConversation ? formatConversationUpdatedAt(latestConversation, locale) ?? undefined : undefined}>
            {latestConversation ? formatConversationRelativeTime(latestConversation, nowMs, locale) : "-"}
          </dd>
        </div>
      </dl>

      <nav aria-label={t("projectSettings.categoryAria")} className="project-home-jump">
        <button type="button" onClick={() => scrollToCard("sessions")}><MessagesSquare size={14} />{t("projectHome.sessionsTitle")}</button>
        <button type="button" onClick={() => scrollToCard("attached-folders")}><FolderTree size={14} />{t("projectAttachedFolders.title")}</button>
        {sections.map((section) => (
          <button key={section.id} type="button" onClick={() => scrollToCard(section.id)}>{section.icon}{section.label}</button>
        ))}
      </nav>

      <div className="project-home-columns">
        <div className="project-home-column">
          {card("sessions", (
            <ProjectHomeSessions
              key={project.id}
              conversations={conversations}
              nowMs={nowMs}
              onSelectConversation={props.onSelectConversation}
            />
          ))}

          {card("attached-folders", <ProjectAttachedFoldersSection key={project.id} project={project} />)}

          {card("agent-skills", (
            <ProjectAgentSkillsSection
              agentBusySkill={agent.busySkill}
              agentLoading={agent.loading}
              agentLoaded={agent.loadedCurrent}
              agentError={agent.error}
              onRetry={agent.retry}
              agentOverrides={agent.overrides}
              onSetProjectAgentSkill={(name, stateKey) => void agent.set(name, stateKey)}
              workbench={props.workbench}
            />
          ))}

          {card("attachments", <ProjectAttachmentCleanupSection attachmentRoot={attachmentRoot} {...cleanup} />)}
        </div>

        <div className="project-home-column">
          {card("overview", (
            <div className="project-settings-section">
              <header className="project-settings-heading">
                <FolderCog size={18} />
                <div>
                  <strong>{t("projectSettings.overview")}</strong>
                  <span>{t("projectSettings.projectSpaceSummary")}</span>
                </div>
              </header>
              <ProjectOverviewSection
                key={project.id}
                agentOverrides={agent.overrides}
                agentLoaded={agent.loaded}
                agentLoading={agent.loading}
                agentError={agent.error}
                attachmentRoot={attachmentRoot}
                conversationCount={conversationCount}
                hideIdentity
                memory={props.workbench.vmmMemory}
                onOpenSection={scrollToCard}
                project={project}
                vmmDraft={vmm.overviewDraft}
                vmmLoading={vmm.busy === "loading"}
                vmmError={vmm.error}
                vmmDirty={vmm.dirty}
                vmmRevision={props.settingsRevision.vmm}
              />
            </div>
          ))}

          {card("vmm", (
            <>
              {vmm.restored && vmm.dirty ? (
                <div className="project-home-restored" role="status">
                  <span>{t("projectHome.vmmRestored")}</span>
                  <Button disabled={vmm.busy === "saving"} size="xs" variant="ghost" onClick={vmm.discard}>{t("projectHome.vmmDiscard")}</Button>
                </div>
              ) : null}
              <ProjectVmmSection
                onSaveDraft={vmm.save}
                onUpdateDraft={vmm.update}
                projectId={project.id}
                vmmBusy={vmm.busy}
                vmmDraft={vmm.draft}
                vmmDirty={vmm.dirty}
              />
            </>
          ))}

          {card("knowledge-bases", <ProjectKnowledgeBasesSection projectId={project.id} onOpenKnowledge={props.onOpenKnowledge} />)}

          {props.workbench.permissionJudgeBackend ? card("permission-judge", <ProjectPermissionJudgeSection projectId={project.id} />) : null}

          {card("collaboration-grants", <ProjectCollaborationGrantsSection projectId={project.id} workbench={props.workbench} />)}
        </div>
      </div>
    </div>
  );
}
