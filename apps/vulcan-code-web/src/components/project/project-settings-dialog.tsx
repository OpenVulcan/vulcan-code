import React from "react";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import type {
  VmmConfigDraftSummary,
  WorkbenchSnapshot,
} from "@/types";
import {
  ProjectAgentSkillsSection,
  ProjectAttachmentCleanupSection,
  ProjectOverviewSection,
  ProjectVmmSection,
} from "./project-settings-sections";
import type {
  ProjectAgentSkillState,
  ProjectAttachmentCleanupKinds,
  ProjectAttachmentCleanupPreviewSummary,
  ProjectAttachmentCleanupReportSummary,
  ProjectAttachmentCleanupRequest,
  ProjectSettingsDialogTarget,
  ProjectSettingsSectionId,
} from "./project-settings-types";
import {
  projectAgentSkillSaveMessage,
  projectAttachmentPreviewFallback,
  projectAttachmentRoot,
  projectConversationCount,
  projectSettingsSections,
} from "./project-settings-utils";

export type {
  ProjectAgentSkillState,
  ProjectAttachmentCleanupKinds,
  ProjectAttachmentCleanupPreviewSummary,
  ProjectAttachmentCleanupReportSummary,
  ProjectAttachmentCleanupRequest,
  ProjectSettingsDialogTarget,
} from "./project-settings-types";
export {
  projectAgentSkillSaveMessage,
  projectAttachmentPreviewFallback,
  projectConversationCount,
} from "./project-settings-utils";

// ProjectSettingsDialog renders project-level settings owned by the WebUI workbench.
// ProjectSettingsDialog 渲染由 WebUI 工作台持有的项目级设置。
export function ProjectSettingsDialog(props: {
  // Project currently edited by this dialog.
  // 当前弹窗正在编辑的项目。
  target: ProjectSettingsDialogTarget | null;
  // Workbench snapshot supplying catalogs and counters.
  // 提供目录与计数的工作台快照。
  workbench: WorkbenchSnapshot;
  // Closes the dialog without forcing a save.
  // 关闭弹窗且不强制保存。
  onCancel: () => void;
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
}) {
  const project = props.target?.project ?? null;
  // activeSection stores the selected settings category.
  // activeSection 保存当前选中的设置分类。
  const [activeSection, setActiveSection] = React.useState<ProjectSettingsSectionId>("overview");
  // vmmDraft stores the editable project-level VMM configuration.
  // vmmDraft 保存可编辑的项目级 VMM 配置。
  const [vmmDraft, setVmmDraft] = React.useState<VmmConfigDraftSummary | null>(null);
  // vmmBusy stores the current VMM load or save phase.
  // vmmBusy 保存当前 VMM 加载或保存阶段。
  const [vmmBusy, setVmmBusy] = React.useState<"idle" | "loading" | "saving">("idle");
  // agentOverrides stores project-local Agent Skill override states.
  // agentOverrides 保存项目本地 Agent Skill 覆盖状态。
  const [agentOverrides, setAgentOverrides] = React.useState<Record<string, string>>({});
  // agentLoading records whether the Agent Skill override map is loading.
  // agentLoading 记录 Agent Skill 覆盖映射是否正在加载。
  const [agentLoading, setAgentLoading] = React.useState(false);
  // agentBusySkill stores the Agent Skill currently being saved.
  // agentBusySkill 保存当前正在保存的 Agent Skill。
  const [agentBusySkill, setAgentBusySkill] = React.useState<string | null>(null);
  // cleanupKinds stores selected attachment families for cleanup preview.
  // cleanupKinds 保存附件清理预览选择的附件类型。
  const [cleanupKinds, setCleanupKinds] = React.useState<ProjectAttachmentCleanupKinds>({
    image: true,
    audio: true,
    video: true,
    pdf: true,
  });
  // cleanupCutoffDate stores the project cleanup cutoff date text.
  // cleanupCutoffDate 保存项目清理截止日期文本。
  const [cleanupCutoffDate, setCleanupCutoffDate] = React.useState("");
  // cleanupNotice stores the modal-local attachment cleanup status.
  // cleanupNotice 保存弹窗内的附件清理状态。
  const [cleanupNotice, setCleanupNotice] = React.useState("选择截止日期和附件类型后可预览当前项目的托管附件清理范围。");
  // cleanupBusy stores the current attachment cleanup phase.
  // cleanupBusy 保存当前附件清理阶段。
  const [cleanupBusy, setCleanupBusy] = React.useState<"idle" | "preview" | "execute">("idle");
  // cleanupPreview stores the latest successful cleanup preview.
  // cleanupPreview 保存最近一次成功的清理预览。
  const [cleanupPreview, setCleanupPreview] = React.useState<ProjectAttachmentCleanupPreviewSummary | null>(null);
  // cleanupReport stores the latest successful cleanup execution report.
  // cleanupReport 保存最近一次成功的清理执行报告。
  const [cleanupReport, setCleanupReport] = React.useState<ProjectAttachmentCleanupReportSummary | null>(null);

  React.useEffect(() => {
    if (!project) {
      return;
    }
    let cancelled = false;
    setActiveSection("overview");
    setVmmDraft(null);
    setVmmBusy("loading");
    setAgentOverrides({});
    setAgentLoading(true);
    setAgentBusySkill(null);
    setCleanupKinds({ image: true, audio: true, video: true, pdf: true });
    setCleanupCutoffDate(new Date().toISOString().slice(0, 10));
    setCleanupNotice("选择截止日期和附件类型后可预览当前项目的托管附件清理范围。");
    setCleanupBusy("idle");
    setCleanupPreview(null);
    setCleanupReport(null);

    void props.onLoadVmmDraft(project.id)
      .then((draft) => {
        if (!cancelled) {
          setVmmDraft(draft);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          toast.error(error instanceof Error ? error.message : String(error));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setVmmBusy("idle");
        }
      });

    void props.onLoadAgentSkillOverrides(project.id)
      .then((overrides) => {
        if (!cancelled) {
          setAgentOverrides(overrides);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          toast.error(error instanceof Error ? error.message : String(error));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setAgentLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [project, props.onLoadAgentSkillOverrides, props.onLoadVmmDraft]);

  if (!project) {
    return null;
  }

  const sections = projectSettingsSections(project, props.workbench);
  const attachmentRoot = projectAttachmentRoot(project);
  const conversationCount = projectConversationCount(props.workbench, project.id);

  // updateVmmDraft applies a partial edit to the current VMM draft.
  // updateVmmDraft 将局部编辑应用到当前 VMM 草稿。
  const updateVmmDraft = (patch: Partial<VmmConfigDraftSummary>) => {
    setVmmDraft((current) => current ? { ...current, ...patch } : current);
  };

  // saveVmmDraft persists the current VMM draft through the host.
  // saveVmmDraft 通过宿主持久化当前 VMM 草稿。
  const saveVmmDraft = async () => {
    if (!vmmDraft) {
      return;
    }
    try {
      setVmmBusy("saving");
      await props.onSaveVmmDraft(project.id, vmmDraft);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setVmmBusy("idle");
    }
  };

  // setProjectAgentSkill persists one Agent Skill project tri-state override optimistically.
  // setProjectAgentSkill 乐观持久化单个 Agent Skill 项目三态覆盖。
  const setProjectAgentSkill = async (name: string, stateKey: ProjectAgentSkillState) => {
    const previousOverrides = agentOverrides;
    setAgentOverrides((current) => {
      const next = { ...current };
      if (stateKey === "inherit") {
        delete next[name];
      } else {
        next[name] = stateKey;
      }
      return next;
    });
    setAgentBusySkill(name);
    try {
      await props.onSetAgentSkillState(project.id, name, stateKey);
    } catch (error) {
      setAgentOverrides(previousOverrides);
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setAgentBusySkill(null);
    }
  };

  // cleanupRequest builds the current attachment cleanup command payload.
  // cleanupRequest 构建当前附件清理命令载荷。
  const cleanupRequest = (): ProjectAttachmentCleanupRequest => ({
    projectId: project.id,
    cutoffDate: cleanupCutoffDate,
    selection: cleanupKinds,
  });

  // previewCleanup loads the current project attachment cleanup preview.
  // previewCleanup 加载当前项目附件清理预览。
  const previewCleanup = async () => {
    try {
      setCleanupBusy("preview");
      const preview = await props.onPreviewAttachmentCleanup(cleanupRequest());
      setCleanupPreview(preview);
      setCleanupReport(null);
      setCleanupNotice(preview.summary);
    } catch (error) {
      setCleanupPreview(null);
      setCleanupNotice(error instanceof Error ? error.message : String(error));
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setCleanupBusy("idle");
    }
  };

  // executeCleanup executes the cleanup represented by the latest matching preview.
  // executeCleanup 执行最近一次匹配预览代表的清理。
  const executeCleanup = async () => {
    try {
      setCleanupBusy("execute");
      const report = await props.onExecuteAttachmentCleanup(cleanupRequest());
      setCleanupReport(report);
      setCleanupPreview(null);
      setCleanupNotice(report.summary);
      toast.success("项目附件缓存清理已完成。");
    } catch (error) {
      setCleanupNotice(error instanceof Error ? error.message : String(error));
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setCleanupBusy("idle");
    }
  };

  // cleanupPreviewMatchesCurrent gates destructive execution behind a fresh positive preview.
  // cleanupPreviewMatchesCurrent 用最近一次有效正向预览控制破坏性执行入口。
  const cleanupPreviewMatchesCurrent = cleanupPreview
    && cleanupPreview.cutoffDate === cleanupCutoffDate
    && cleanupPreview.matchedAssetCount > 0;

  return (
    <Dialog open={Boolean(project)} onOpenChange={(open) => !open && props.onCancel()}>
      <DialogContent className="project-settings-dialog">
        <DialogHeader>
          <DialogTitle>项目设置：{project.name}</DialogTitle>
          <DialogDescription>项目级配置与缓存清理只作用于当前项目空间。</DialogDescription>
        </DialogHeader>
        <div className="project-settings-layout">
          <nav className="project-settings-nav" aria-label="项目设置分类">
            {sections.map((section) => (
              <button
                className={cn("project-settings-nav-button", activeSection === section.id && "active")}
                key={section.id}
                onClick={() => setActiveSection(section.id)}
                type="button"
              >
                {section.icon}
                <span>
                  <strong>{section.label}</strong>
                  <small>{section.detail}</small>
                </span>
              </button>
            ))}
          </nav>
          <section className="project-settings-content">
            {activeSection === "overview" ? (
              <ProjectOverviewSection
                agentOverrides={agentOverrides}
                attachmentRoot={attachmentRoot}
                conversationCount={conversationCount}
                project={project}
                vmmDraft={vmmDraft}
              />
            ) : null}

            {activeSection === "vmm" ? (
              <ProjectVmmSection
                onSaveDraft={() => void saveVmmDraft()}
                onUpdateDraft={updateVmmDraft}
                vmmBusy={vmmBusy}
                vmmDraft={vmmDraft}
              />
            ) : null}

            {activeSection === "agent-skills" ? (
              <ProjectAgentSkillsSection
                agentBusySkill={agentBusySkill}
                agentLoading={agentLoading}
                agentOverrides={agentOverrides}
                onSetProjectAgentSkill={(name, stateKey) => void setProjectAgentSkill(name, stateKey)}
                workbench={props.workbench}
              />
            ) : null}

            {activeSection === "attachments" ? (
              <ProjectAttachmentCleanupSection
                attachmentRoot={attachmentRoot}
                cleanupBusy={cleanupBusy}
                cleanupCutoffDate={cleanupCutoffDate}
                cleanupKinds={cleanupKinds}
                cleanupNotice={cleanupNotice}
                cleanupPreview={cleanupPreview}
                cleanupPreviewMatchesCurrent={Boolean(cleanupPreviewMatchesCurrent)}
                cleanupReport={cleanupReport}
                onChangeCleanupCutoffDate={(cutoffDate) => {
                  setCleanupCutoffDate(cutoffDate);
                  setCleanupPreview(null);
                }}
                onChangeCleanupKinds={(kinds) => {
                  setCleanupKinds(kinds);
                  setCleanupPreview(null);
                }}
                onExecuteCleanup={() => void executeCleanup()}
                onPreviewCleanup={() => void previewCleanup()}
              />
            ) : null}
          </section>
        </div>
      </DialogContent>
    </Dialog>
  );
}
