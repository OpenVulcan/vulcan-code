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
  VceJobSummary,
  VceProjectSettingsInput,
  VceProjectSettingsSummary,
  VmmConfigDraftSummary,
  WorkbenchSnapshot,
} from "@/types";
import { CodeContextArea } from "@/components/features/code-context/code-context-area";
import { useVceProjectActivity } from "@/components/features/code-context/use-vce-project-activity";
import { useVceActionScope } from "@/components/features/code-context/use-vce-action-scope";
import { useI18n } from "@/i18n";
import {
  ProjectAgentSkillsSection,
  ProjectAttachmentCleanupSection,
  ProjectOverviewSection,
  ProjectVmmSection,
} from "./project-settings-sections";
import { ProjectKnowledgeBasesSection } from "./project-knowledge-bases-section";
import { ProjectCollaborationGrantsSection } from "./project-collaboration-grants-section";
import type {
  ProjectAgentSkillState,
  ProjectAttachmentCleanupKinds,
  ProjectAttachmentCleanupPreviewSummary,
  ProjectAttachmentCleanupReportSummary,
  ProjectAttachmentCleanupRequest,
  ProjectSettingsDialogTarget,
  ProjectSettingsRevision,
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
  ProjectSettingsRevision,
} from "./project-settings-types";
export {
  projectAgentSkillSaveMessage,
  projectAttachmentPreviewFallback,
  projectConversationCount,
} from "./project-settings-utils";
import {
  submitVceJob,
  updateVceProjectSettings,
} from "@/service/vce-service";

// PROJECT_SETTINGS_RETRY_BASE_DELAY_MS is the first reconnect recovery delay.
// PROJECT_SETTINGS_RETRY_BASE_DELAY_MS 是首次连接恢复重试的等待时间。
const PROJECT_SETTINGS_RETRY_BASE_DELAY_MS = 1_000;

// PROJECT_SETTINGS_RETRY_MAX_DELAY_MS caps background recovery traffic during a prolonged outage.
// PROJECT_SETTINGS_RETRY_MAX_DELAY_MS 限制长时间故障期间的后台恢复流量。
const PROJECT_SETTINGS_RETRY_MAX_DELAY_MS = 30_000;

// projectSettingsRetryDelayMs calculates the capped exponential delay after a failed domain read.
// projectSettingsRetryDelayMs 计算领域读取失败后的有上限指数退避时间。
//
// Parameters:
// 参数：
// failureCount is the consecutive failure count for the exact project and settings domain.
// failureCount 是准确项目和设置领域的连续失败次数。
//
// Returns:
// 返回值：
// The retry delay in milliseconds, capped at the prolonged-outage interval.
// 以毫秒表示的重试延迟，最大不超过长时间故障间隔。
function projectSettingsRetryDelayMs(failureCount: number): number {
  // exponent starts at zero and remains bounded before exponentiation.
  // exponent 从零开始，并在指数运算前保持有界。
  const exponent = Math.min(Math.max(failureCount - 1, 0), 5);
  return Math.min(
    PROJECT_SETTINGS_RETRY_BASE_DELAY_MS * (2 ** exponent),
    PROJECT_SETTINGS_RETRY_MAX_DELAY_MS,
  );
}

// ProjectSettingsDialog renders project-level settings owned by the WebUI workbench.
// ProjectSettingsDialog 渲染由 WebUI 工作台持有的项目级设置。
export function ProjectSettingsDialog(props: {
  // Project currently edited by this dialog.
  // 当前弹窗正在编辑的项目。
  target: ProjectSettingsDialogTarget | null;
  // Workbench snapshot supplying catalogs and counters.
  // 提供目录与计数的工作台快照。
  workbench: WorkbenchSnapshot;
  // Latest accepted invalidation revisions for this project's independently loaded settings.
  // 当前项目独立加载设置的最新已接收失效版本。
  settingsRevision: ProjectSettingsRevision;
  // Closes the dialog without forcing a save.
  // 关闭弹窗且不强制保存。
  onCancel: () => void;
  // Close this dialog and navigate to the fixed ContextEngine model settings page; no inputs or return value.
  // 关闭本弹窗并导航到固定语义引擎模型设置页；无输入及返回值。
  onOpenEngineSettings: () => void;
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
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const project = props.target?.project ?? null;
  // projectId is the stable dialog identity and does not change with locale or snapshot object identity.
  // projectId 是稳定的弹窗标识，不随语言或快照对象身份变化。
  const projectId = project?.id ?? null;
  // activeSection stores the selected settings category.
  // activeSection 保存当前选中的设置分类。
  const [activeSection, setActiveSection] = React.useState<ProjectSettingsSectionId>("overview");
  // Each explicit dialog target applies its requested section once after prerequisite metadata is ready.
  // 每个明确弹窗目标在前提元数据就绪后仅应用一次请求分类。
  const appliedSectionTarget = React.useRef<ProjectSettingsDialogTarget | null>(null);
  // vmmDraft stores the editable project-level VMM configuration.
  // vmmDraft 保存可编辑的项目级 VMM 配置。
  const [vmmDraft, setVmmDraft] = React.useState<VmmConfigDraftSummary | null>(null);
  // vmmBusy stores the current VMM load or save phase.
  // vmmBusy 保存当前 VMM 加载或保存阶段。
  const [vmmBusy, setVmmBusy] = React.useState<"idle" | "loading" | "saving">("idle");
  // vmmDirty records whether the visible draft contains unsaved local edits.
  // vmmDirty 记录可见草稿是否包含未保存的本地编辑。
  const [vmmDirty, setVmmDirty] = React.useState(false);
  // vmmSaveInFlight prevents a save-triggered invalidation from racing the active save response.
  // vmmSaveInFlight 防止保存触发的失效通知与当前保存响应竞争。
  const [vmmSaveInFlight, setVmmSaveInFlight] = React.useState(false);
  // agentOverrides stores project-local Agent Skill override states.
  // agentOverrides 保存项目本地 Agent Skill 覆盖状态。
  const [agentOverrides, setAgentOverrides] = React.useState<Record<string, string>>({});
  // agentLoading records whether the Agent Skill override map is loading.
  // agentLoading 记录 Agent Skill 覆盖映射是否正在加载。
  const [agentLoading, setAgentLoading] = React.useState(false);
  // Overview authority binds accepted settings to a project and preserves read failures independently of drafts.
  // 概览权威将已接受设置绑定项目，并独立于草稿保留读取失败。
  const [overviewAuthority, setOverviewAuthority] = React.useState<{
    projectId: string | null; vmmProjectId: string | null; agentProjectId: string | null; vmmError: string | null; agentError: string | null;
  }>({ projectId: null, vmmProjectId: null, agentProjectId: null, vmmError: null, agentError: null });
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
  // cleanupNotice stores an explicit cleanup result; null delegates the initial copy to the active locale.
  // cleanupNotice 保存明确的清理结果；null 表示初始文案跟随当前语言。
  const [cleanupNotice, setCleanupNotice] = React.useState<string | null>(null);
  // cleanupBusy stores the current attachment cleanup phase.
  // cleanupBusy 保存当前附件清理阶段。
  const [cleanupBusy, setCleanupBusy] = React.useState<"idle" | "preview" | "execute">("idle");
  // cleanupPreview stores the latest successful cleanup preview.
  // cleanupPreview 保存最近一次成功的清理预览。
  const [cleanupPreview, setCleanupPreview] = React.useState<ProjectAttachmentCleanupPreviewSummary | null>(null);
  // cleanupReport stores the latest successful cleanup execution report.
  // cleanupReport 保存最近一次成功的清理执行报告。
  const [cleanupReport, setCleanupReport] = React.useState<ProjectAttachmentCleanupReportSummary | null>(null);
  // vceActivity shares lightweight project authority with the composer and floating status node.
  // vceActivity 与输入区及浮动状态节点共享轻量项目权威。
  const vceActivity = useVceProjectActivity(projectId, String(props.settingsRevision.vce));
  // vceOverview intentionally excludes management scans; the detail panel loads those on demand.
  // vceOverview 明确不包含管理扫描；详情面板按需加载这些数据。
  const vceOverview = vceActivity.activity;
  // vceMutationBusy tracks one user-initiated settings save or refresh.
  // vceMutationBusy 跟踪一次用户发起的设置保存或刷新。
  const [vceMutationBusy, setVceMutationBusy] = React.useState(false);
  // loadedVmmRevisionByProjectRef advances only after one VMM read succeeds.
  // loadedVmmRevisionByProjectRef 仅在一次 VMM 读取成功后推进。
  const loadedVmmRevisionByProjectRef = React.useRef<Map<string, number>>(new Map());
  // loadedAgentRevisionByProjectRef advances only after one Agent Skill read succeeds.
  // loadedAgentRevisionByProjectRef 仅在一次 Agent Skill 读取成功后推进。
  const loadedAgentRevisionByProjectRef = React.useRef<Map<string, number>>(new Map());
  // vmmLoadFailureCountByProjectRef tracks consecutive VMM failures for reconnect backoff.
  // vmmLoadFailureCountByProjectRef 跟踪 VMM 连续失败次数以执行重连退避。
  const vmmLoadFailureCountByProjectRef = React.useRef<Map<string, number>>(new Map());
  // agentLoadFailureCountByProjectRef tracks consecutive Agent Skill failures for reconnect backoff.
  // agentLoadFailureCountByProjectRef 跟踪 Agent Skill 连续失败次数以执行重连退避。
  const agentLoadFailureCountByProjectRef = React.useRef<Map<string, number>>(new Map());
  // vmmLoadSequenceRef prevents an older VMM read from overwriting a newer domain read.
  // vmmLoadSequenceRef 防止较旧 VMM 读取覆盖较新的领域读取。
  const vmmLoadSequenceRef = React.useRef(0);
  // agentLoadSequenceRef prevents an older Agent Skill read from overwriting a newer domain read.
  // agentLoadSequenceRef 防止较旧 Agent Skill 读取覆盖较新的领域读取。
  const agentLoadSequenceRef = React.useRef(0);
  // vmmLoadRetryNonce triggers a fresh authoritative VMM read after a backoff delay.
  // vmmLoadRetryNonce 在退避结束后触发新的权威 VMM 读取。
  const [vmmLoadRetryNonce, setVmmLoadRetryNonce] = React.useState(0);
  // agentLoadRetryNonce triggers a fresh authoritative Agent Skill read after a backoff delay.
  // agentLoadRetryNonce 在退避结束后触发新的权威 Agent Skill 读取。
  const [agentLoadRetryNonce, setAgentLoadRetryNonce] = React.useState(0);
  // projectActionOwnerRef identifies the exact dialog-open lifetime that owns asynchronous writes.
  // projectActionOwnerRef 标识拥有异步状态写入权的准确弹窗打开周期。
  const projectActionOwnerRef = React.useRef<{ projectId: string | null }>({ projectId });
  // vmmSaveSequenceRef ensures only the latest VMM save in one dialog lifetime can finalize state.
  // vmmSaveSequenceRef 确保同一弹窗周期内仅最新 VMM 保存可以收尾状态。
  const vmmSaveSequenceRef = React.useRef(0);
  // vmmEditSequenceRef preserves edits made while an earlier VMM save is still running.
  // vmmEditSequenceRef 保留较早 VMM 保存仍在运行时产生的新编辑。
  const vmmEditSequenceRef = React.useRef(0);
  // vmmSaveInFlightRef synchronously rejects duplicate save clicks before React commits busy state.
  // vmmSaveInFlightRef 在 React 提交忙碌状态前同步拒绝重复保存点击。
  const vmmSaveInFlightRef = React.useRef(false);
  // agentSaveInFlightRef serializes project Agent Skill writes and prevents cross-skill rollback races.
  // agentSaveInFlightRef 串行化项目 Agent Skill 写入并防止跨技能回滚竞态。
  const agentSaveInFlightRef = React.useRef(false);
  // cleanupOperationSequenceRef invalidates cleanup results when project identity or inputs change.
  // cleanupOperationSequenceRef 在项目身份或输入变化时使清理结果失效。
  const cleanupOperationSequenceRef = React.useRef(0);
  // cleanupOperationInFlightRef synchronously rejects duplicate preview or execute requests.
  // cleanupOperationInFlightRef 同步拒绝重复的预览或执行请求。
  const cleanupOperationInFlightRef = React.useRef(false);
  // vceMutationInFlightRef synchronously rejects duplicate settings or refresh actions.
  // vceMutationInFlightRef 同步拒绝重复的设置或刷新操作。
  const vceMutationInFlightRef = React.useRef(false);
  // VCE actions also expire on service reconnection, independently of VMM's dialog ownership.
  // VCE 操作也在服务重连时失效，与 VMM 的弹窗归属独立。
  const isCurrentVceAction = useVceActionScope(projectId);
  React.useLayoutEffect(() => {
    vceMutationInFlightRef.current = false;
    setVceMutationBusy(false);
  }, [isCurrentVceAction]);

  React.useLayoutEffect(() => {
    // A fresh object distinguishes closing and reopening the same project from the previous dialog lifetime.
    // 新对象可区分同一项目关闭后重开与之前的弹窗周期。
    projectActionOwnerRef.current = { projectId };
    vmmSaveSequenceRef.current += 1;
    vmmEditSequenceRef.current = 0;
    vmmSaveInFlightRef.current = false;
    agentSaveInFlightRef.current = false;
    cleanupOperationSequenceRef.current += 1;
    cleanupOperationInFlightRef.current = false;
    vceMutationInFlightRef.current = false;
  }, [projectId]);

  React.useLayoutEffect(() => {
    if (projectId === null) {
      loadedVmmRevisionByProjectRef.current.clear();
      loadedAgentRevisionByProjectRef.current.clear();
      vmmLoadFailureCountByProjectRef.current.clear();
      agentLoadFailureCountByProjectRef.current.clear();
      return;
    }
    loadedVmmRevisionByProjectRef.current.delete(projectId);
    loadedAgentRevisionByProjectRef.current.delete(projectId);
    vmmLoadFailureCountByProjectRef.current.delete(projectId);
    agentLoadFailureCountByProjectRef.current.delete(projectId);
    vmmLoadSequenceRef.current += 1;
    agentLoadSequenceRef.current += 1;
    setVmmLoadRetryNonce(0);
    setAgentLoadRetryNonce(0);
    setActiveSection("overview");
    setVmmDraft(null);
    setVmmBusy("idle");
    setVmmDirty(false);
    setVmmSaveInFlight(false);
    setAgentOverrides({});
    setAgentLoading(false);
    setOverviewAuthority({ projectId, vmmProjectId: null, agentProjectId: null, vmmError: null, agentError: null });
    setAgentBusySkill(null);
    setCleanupKinds({ image: true, audio: true, video: true, pdf: true });
    setCleanupCutoffDate(new Date().toISOString().slice(0, 10));
    setCleanupNotice(null);
    setCleanupBusy("idle");
    setCleanupPreview(null);
    setCleanupReport(null);
    setVceMutationBusy(false);
  }, [projectId]);

  // Configuration remains reachable even when models are unavailable; runtime actions keep separate gates.
  // 模型不可用时配置仍可访问；运行时操作保留独立闸门。
  React.useEffect(() => {
    const target = props.target;
    if (target === null) { appliedSectionTarget.current = null; return; }
    if (target === appliedSectionTarget.current || target.initialSection === undefined) return;
    appliedSectionTarget.current = target;
    setActiveSection(target.initialSection);
  }, [props.target]);

  React.useEffect(() => {
    if (projectId === null || vmmDirty || vmmSaveInFlight) {
      return;
    }
    // targetRevision includes zero so the first dialog open always loads authoritative state.
    // targetRevision 包含零版本，使弹窗首次打开时始终加载权威状态。
    const targetRevision = props.settingsRevision.vmm;
    const loadedRevision = loadedVmmRevisionByProjectRef.current.get(projectId);
    if (loadedRevision !== undefined && targetRevision <= loadedRevision) {
      return;
    }
    let cancelled = false;
    // loadSequence immediately supersedes every earlier VMM read.
    // loadSequence 立即取代所有更早的 VMM 读取。
    const loadSequence = vmmLoadSequenceRef.current + 1;
    vmmLoadSequenceRef.current = loadSequence;
    // retryTimer schedules recovery without requiring a new settings revision.
    // retryTimer 在无需新设置版本的情况下安排恢复读取。
    let retryTimer: number | null = null;
    setVmmBusy("loading");
    void props.onLoadVmmDraft(projectId)
      .then((draft) => {
        if (!cancelled && vmmLoadSequenceRef.current === loadSequence) {
          setVmmDraft(draft);
          setOverviewAuthority((current) => ({ ...current, vmmProjectId: projectId, vmmError: null }));
          setVmmDirty(false);
          loadedVmmRevisionByProjectRef.current.set(projectId, targetRevision);
          vmmLoadFailureCountByProjectRef.current.delete(projectId);
        }
      })
      .catch((error) => {
        if (!cancelled && vmmLoadSequenceRef.current === loadSequence) {
          // failureCount is scoped to this exact domain and resets only after a successful read.
          // failureCount 仅属于当前准确领域，并且只在成功读取后重置。
          const failureCount = (vmmLoadFailureCountByProjectRef.current.get(projectId) ?? 0) + 1;
          vmmLoadFailureCountByProjectRef.current.set(projectId, failureCount);
          setOverviewAuthority((current) => ({ ...current, vmmError: error instanceof Error ? error.message : String(error) }));
          if (failureCount === 1) {
            toast.error(error instanceof Error ? error.message : String(error));
          }
          retryTimer = window.setTimeout(() => {
            if (!cancelled && vmmLoadSequenceRef.current === loadSequence) {
              setVmmLoadRetryNonce((current) => current + 1);
            }
          }, projectSettingsRetryDelayMs(failureCount));
        }
      })
      .finally(() => {
        if (!cancelled && vmmLoadSequenceRef.current === loadSequence) {
          setVmmBusy("idle");
        }
      });
    return () => {
      cancelled = true;
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
      }
    };
  }, [projectId, props.onLoadVmmDraft, props.settingsRevision.vmm, vmmDirty, vmmSaveInFlight, vmmLoadRetryNonce]);

  React.useEffect(() => {
    if (projectId === null) {
      return;
    }
    // targetRevision includes zero so the first dialog open always loads authoritative state.
    // targetRevision 包含零版本，使弹窗首次打开时始终加载权威状态。
    const targetRevision = props.settingsRevision.agentSkills;
    const loadedRevision = loadedAgentRevisionByProjectRef.current.get(projectId);
    if (loadedRevision !== undefined && targetRevision <= loadedRevision) {
      return;
    }
    let cancelled = false;
    // loadSequence immediately supersedes every earlier Agent Skill read.
    // loadSequence 立即取代所有更早的 Agent Skill 读取。
    const loadSequence = agentLoadSequenceRef.current + 1;
    agentLoadSequenceRef.current = loadSequence;
    // retryTimer schedules recovery without requiring a new settings revision.
    // retryTimer 在无需新设置版本的情况下安排恢复读取。
    let retryTimer: number | null = null;
    setAgentLoading(true);
    void props.onLoadAgentSkillOverrides(projectId)
      .then((overrides) => {
        if (!cancelled && agentLoadSequenceRef.current === loadSequence) {
          setAgentOverrides(overrides);
          setOverviewAuthority((current) => ({ ...current, agentProjectId: projectId, agentError: null }));
          loadedAgentRevisionByProjectRef.current.set(projectId, targetRevision);
          agentLoadFailureCountByProjectRef.current.delete(projectId);
        }
      })
      .catch((error) => {
        if (!cancelled && agentLoadSequenceRef.current === loadSequence) {
          // failureCount is scoped to this exact domain and resets only after a successful read.
          // failureCount 仅属于当前准确领域，并且只在成功读取后重置。
          const failureCount = (agentLoadFailureCountByProjectRef.current.get(projectId) ?? 0) + 1;
          agentLoadFailureCountByProjectRef.current.set(projectId, failureCount);
          setOverviewAuthority((current) => ({ ...current, agentError: error instanceof Error ? error.message : String(error) }));
          if (failureCount === 1) {
            toast.error(error instanceof Error ? error.message : String(error));
          }
          retryTimer = window.setTimeout(() => {
            if (!cancelled && agentLoadSequenceRef.current === loadSequence) {
              setAgentLoadRetryNonce((current) => current + 1);
            }
          }, projectSettingsRetryDelayMs(failureCount));
        }
      })
      .finally(() => {
        if (!cancelled && agentLoadSequenceRef.current === loadSequence) {
          setAgentLoading(false);
        }
      });
    return () => {
      cancelled = true;
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
      }
    };
  }, [projectId, props.onLoadAgentSkillOverrides, props.settingsRevision.agentSkills, agentLoadRetryNonce]);

  if (!project) {
    return null;
  }

  // effectiveProject mirrors the just-saved setting before the next workbench projection arrives.
  // effectiveProject 在下一份工作台投影到达前镜像刚保存的设置。
  const effectiveProject = {
    ...project,
    vceEnabled: vceOverview?.settings.enabled ?? project.vceEnabled,
  };
  const sections = projectSettingsSections(
    effectiveProject,
    props.workbench,
    t,
    overviewAuthority.vmmProjectId === project.id && vmmDraft !== null ? vmmDraft.hasOverride : null,
  );
  const attachmentRoot = projectAttachmentRoot(project);
  const conversationCount = projectConversationCount(props.workbench, project.id);

  // updateVmmDraft applies a partial edit to the current VMM draft.
  // updateVmmDraft 将局部编辑应用到当前 VMM 草稿。
  const updateVmmDraft = (patch: Partial<VmmConfigDraftSummary>) => {
    if (!vmmDraft) {
      return;
    }
    vmmEditSequenceRef.current += 1;
    setVmmDirty(true);
    setVmmDraft((current) => current ? { ...current, ...patch } : current);
  };

  // saveVmmDraft persists the current VMM draft through the host.
  // saveVmmDraft 通过宿主持久化当前 VMM 草稿。
  const saveVmmDraft = async (exactDraft?: VmmConfigDraftSummary): Promise<boolean> => {
    const draftToSave = exactDraft ?? vmmDraft;
    if (!draftToSave || vmmSaveInFlightRef.current) {
      return false;
    }
    vmmSaveInFlightRef.current = true;
    // actionOwner and saveSequence bind every completion path to this exact visible draft.
    // actionOwner 与 saveSequence 将所有完成路径绑定到当前准确的可见草稿。
    const actionOwner = projectActionOwnerRef.current;
    const saveSequence = vmmSaveSequenceRef.current + 1;
    const savedEditSequence = vmmEditSequenceRef.current;
    vmmSaveSequenceRef.current = saveSequence;
    try {
      setVmmSaveInFlight(true);
      setVmmBusy("saving");
      await props.onSaveVmmDraft(project.id, draftToSave);
      if (
        projectActionOwnerRef.current !== actionOwner
        || vmmSaveSequenceRef.current !== saveSequence
      ) {
        return false;
      }
      if (vmmEditSequenceRef.current === savedEditSequence) {
        setVmmDraft(draftToSave);
        setVmmDirty(false);
      }
      setOverviewAuthority((current) => ({ ...current, vmmError: null }));
      return true;
    } catch (error) {
      if (
        projectActionOwnerRef.current !== actionOwner
        || vmmSaveSequenceRef.current !== saveSequence
      ) {
        return false;
      }
      setOverviewAuthority((current) => ({ ...current, vmmError: error instanceof Error ? error.message : String(error) }));
      toast.error(error instanceof Error ? error.message : String(error));
      return false;
    } finally {
      if (
        projectActionOwnerRef.current === actionOwner
        && vmmSaveSequenceRef.current === saveSequence
      ) {
        vmmSaveInFlightRef.current = false;
        setVmmSaveInFlight(false);
        setVmmBusy("idle");
      }
    }
  };

  // setProjectAgentSkill persists one Agent Skill project tri-state override optimistically.
  // setProjectAgentSkill 乐观持久化单个 Agent Skill 项目三态覆盖。
  const setProjectAgentSkill = async (name: string, stateKey: ProjectAgentSkillState) => {
    if (agentSaveInFlightRef.current || agentLoading
      || overviewAuthority.agentProjectId !== project.id
      || overviewAuthority.agentError !== null
      || loadedAgentRevisionByProjectRef.current.get(project.id) !== props.settingsRevision.agentSkills) {
      return;
    }
    agentSaveInFlightRef.current = true;
    // actionOwner prevents a request from one project dialog lifetime from mutating another.
    // actionOwner 防止一个项目弹窗周期的请求修改另一个周期。
    const actionOwner = projectActionOwnerRef.current;
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
      if (projectActionOwnerRef.current === actionOwner) {
        setOverviewAuthority((current) => ({ ...current, agentError: null }));
      }
    } catch (error) {
      if (projectActionOwnerRef.current !== actionOwner) {
        return;
      }
      setAgentOverrides(previousOverrides);
      setOverviewAuthority((current) => ({ ...current, agentError: error instanceof Error ? error.message : String(error) }));
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      if (projectActionOwnerRef.current === actionOwner) {
        agentSaveInFlightRef.current = false;
        setAgentBusySkill(null);
      }
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
    if (cleanupOperationInFlightRef.current) {
      return;
    }
    cleanupOperationInFlightRef.current = true;
    // request and operationSequence bind the response to the exact project, date, and kind selection.
    // request 与 operationSequence 将响应绑定到准确的项目、日期及类型选择。
    const request = cleanupRequest();
    const actionOwner = projectActionOwnerRef.current;
    const operationSequence = cleanupOperationSequenceRef.current + 1;
    cleanupOperationSequenceRef.current = operationSequence;
    try {
      setCleanupBusy("preview");
      const preview = await props.onPreviewAttachmentCleanup(request);
      if (
        projectActionOwnerRef.current !== actionOwner
        || cleanupOperationSequenceRef.current !== operationSequence
      ) {
        return;
      }
      setCleanupPreview(preview);
      setCleanupReport(null);
      setCleanupNotice(preview.summary);
    } catch (error) {
      if (
        projectActionOwnerRef.current !== actionOwner
        || cleanupOperationSequenceRef.current !== operationSequence
      ) {
        return;
      }
      setCleanupPreview(null);
      setCleanupNotice(error instanceof Error ? error.message : String(error));
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      if (
        projectActionOwnerRef.current === actionOwner
        && cleanupOperationSequenceRef.current === operationSequence
      ) {
        cleanupOperationInFlightRef.current = false;
        setCleanupBusy("idle");
      }
    }
  };

  // executeCleanup executes the cleanup represented by the latest matching preview.
  // executeCleanup 执行最近一次匹配预览代表的清理。
  const executeCleanup = async () => {
    if (cleanupOperationInFlightRef.current) {
      return;
    }
    cleanupOperationInFlightRef.current = true;
    // request and operationSequence prevent a destructive result from crossing dialog ownership.
    // request 与 operationSequence 防止破坏性操作结果跨越弹窗归属边界。
    const request = cleanupRequest();
    const actionOwner = projectActionOwnerRef.current;
    const operationSequence = cleanupOperationSequenceRef.current + 1;
    cleanupOperationSequenceRef.current = operationSequence;
    try {
      setCleanupBusy("execute");
      const report = await props.onExecuteAttachmentCleanup(request);
      if (
        projectActionOwnerRef.current !== actionOwner
        || cleanupOperationSequenceRef.current !== operationSequence
      ) {
        return;
      }
      setCleanupReport(report);
      setCleanupPreview(null);
      setCleanupNotice(report.summary);
      toast.success(t("projectSettings.cleanupCompleted"));
    } catch (error) {
      if (
        projectActionOwnerRef.current !== actionOwner
        || cleanupOperationSequenceRef.current !== operationSequence
      ) {
        return;
      }
      setCleanupNotice(error instanceof Error ? error.message : String(error));
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      if (
        projectActionOwnerRef.current === actionOwner
        && cleanupOperationSequenceRef.current === operationSequence
      ) {
        cleanupOperationInFlightRef.current = false;
        setCleanupBusy("idle");
      }
    }
  };

  // cleanupPreviewMatchesCurrent gates destructive execution behind a fresh positive preview.
  // cleanupPreviewMatchesCurrent 用最近一次有效正向预览控制破坏性执行入口。
  const cleanupPreviewMatchesCurrent = cleanupPreview
    && cleanupPreview.cutoffDate === cleanupCutoffDate
    && cleanupPreview.matchedAssetCount > 0;
  // vceBusy disables actions while either the authoritative read or one mutation is active.
  // vceBusy 在权威读取或一次变更进行期间禁用操作。
  const vceBusy = vceOverview === null || vceMutationBusy;

  // saveVceSettings persists one complete settings document and reloads authoritative diagnostics.
  // saveVceSettings 持久化一份完整设置文档，并重新加载权威诊断。
  const saveVceSettings = async (settings: VceProjectSettingsInput, expectedRevision: number): Promise<VceProjectSettingsSummary> => {
    if (!projectId || !vceOverview || vceBusy || vceMutationInFlightRef.current) {
      throw new Error("请等待当前项目状态读取或设置保存完成。");
    }
    const targetProjectId = projectId;
    const requestOwner = projectActionOwnerRef.current;
    vceMutationInFlightRef.current = true;
    setVceMutationBusy(true);
    try {
      const saved = await updateVceProjectSettings(
        targetProjectId,
        expectedRevision,
        settings,
      );
      if (projectActionOwnerRef.current === requestOwner && isCurrentVceAction()) {
        await vceActivity.reload();
      }
      if (projectActionOwnerRef.current === requestOwner && isCurrentVceAction()) {
        toast.success("代码语义索引设置已保存。");
      }
      return saved;
    } catch (error) {
      if (projectActionOwnerRef.current === requestOwner && isCurrentVceAction()) {
        toast.error(error instanceof Error ? error.message : String(error));
      }
      throw error;
    } finally {
      if (projectActionOwnerRef.current === requestOwner && isCurrentVceAction()) {
        vceMutationInFlightRef.current = false;
        setVceMutationBusy(false);
      }
    }
  };

  // Admit one explicit initialization/synchronization and return its durable receipt immediately.
  // 准入一次明确初始化或同步，并立即返回其持久回执。
  // Takes no arguments; the visible authoritative revision and checkpoint guard this button action.
  // 无参数；可见权威修订与检查点保护此按钮操作。
  const refreshVceIndex = async (): Promise<VceJobSummary> => {
    if (!projectId || !vceOverview || vceBusy || vceMutationInFlightRef.current) {
      throw new Error("请等待当前项目状态读取或设置保存完成。");
    }
    const targetProjectId = projectId;
    const requestOwner = projectActionOwnerRef.current;
    vceMutationInFlightRef.current = true;
    setVceMutationBusy(true);
    try {
      const checkpoint = vceOverview.status.checkpointVersion ?? null;
      const receipt = await submitVceJob({
        projectId: targetProjectId,
        action: checkpoint === null ? "initialize" : "synchronize",
        idempotencyKey: crypto.randomUUID(),
        expectedSettingsRevision: vceOverview.settings.revision,
        expectedCheckpointVersion: checkpoint,
      });
      if (receipt.projectId !== targetProjectId) {
        throw new Error("VCE job admission returned a mismatched project identity.");
      }
      if (projectActionOwnerRef.current === requestOwner && isCurrentVceAction()) {
        toast.success("索引作业已提交，可关闭设置并继续会话。");
      }
      return receipt;
    } catch (error) {
      if (projectActionOwnerRef.current === requestOwner && isCurrentVceAction()) {
        toast.error(error instanceof Error ? error.message : String(error));
      }
      throw error;
    } finally {
      if (projectActionOwnerRef.current === requestOwner && isCurrentVceAction()) {
        vceMutationInFlightRef.current = false;
        setVceMutationBusy(false);
      }
    }
  };

  return (
    <Dialog open={Boolean(project)} onOpenChange={(open) => !open && props.onCancel()}>
      <DialogContent size="viewer" layout="document" scrollMode="content-scroll" className="project-settings-dialog">
        <DialogHeader className="project-settings-dialog-header">
          <DialogTitle>{t("projectSettings.dialogTitle").replace("{project}", project.name)}</DialogTitle>
          <DialogDescription>{t("projectSettings.dialogDescription")}</DialogDescription>
        </DialogHeader>
        <div className="project-settings-layout">
          <nav className="project-settings-nav" aria-label={t("projectSettings.categoryAria")}>
            {sections.map((section) => (
              <button
                className={cn("project-settings-nav-button", activeSection === section.id && "active")}
                aria-current={activeSection === section.id ? "page" : undefined}
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
                key={project.id}
                agentOverrides={agentOverrides}
                agentLoaded={overviewAuthority.agentProjectId === project.id}
                agentLoading={agentLoading}
                agentError={overviewAuthority.projectId === project.id ? overviewAuthority.agentError : null}
                attachmentRoot={attachmentRoot}
                conversationCount={conversationCount}
                memory={props.workbench.vmmMemory}
                onOpenSection={setActiveSection}
                project={project}
                vceActivity={vceOverview}
                vceError={vceActivity.error}
                vmmDraft={overviewAuthority.vmmProjectId === project.id ? vmmDraft : null}
                vmmLoading={vmmBusy === "loading"}
                vmmError={overviewAuthority.projectId === project.id ? overviewAuthority.vmmError : null}
                vmmDirty={vmmDirty}
                vmmRevision={props.settingsRevision.vmm}
              />
            ) : null}

            {activeSection === "vmm" ? (
              <ProjectVmmSection
                onSaveDraft={saveVmmDraft}
                onUpdateDraft={updateVmmDraft}
                projectId={project.id}
                vmmBusy={vmmBusy}
                vmmDraft={vmmDraft}
                vmmDirty={vmmDirty}
              />
            ) : null}

            {activeSection === "agent-skills" ? (
              <ProjectAgentSkillsSection
                agentBusySkill={agentBusySkill}
                agentLoading={agentLoading}
                agentLoaded={overviewAuthority.agentProjectId === project.id
                  && loadedAgentRevisionByProjectRef.current.get(project.id) === props.settingsRevision.agentSkills}
                agentError={overviewAuthority.projectId === project.id ? overviewAuthority.agentError : null}
                onRetry={() => {
                  if (agentLoading || agentSaveInFlightRef.current) return;
                  loadedAgentRevisionByProjectRef.current.delete(project.id);
                  setAgentLoadRetryNonce((current) => current + 1);
                }}
                agentOverrides={agentOverrides}
                onSetProjectAgentSkill={(name, stateKey) => void setProjectAgentSkill(name, stateKey)}
                workbench={props.workbench}
              />
            ) : null}

            {activeSection === "code-context" ? (
              <CodeContextArea
                busy={vceBusy}
                key={project.id}
                onOverviewChanged={() => { void vceActivity.reload(); }}
                activityError={vceActivity.error}
                onReloadActivity={vceActivity.reload}
                onRefresh={refreshVceIndex}
                onSave={saveVceSettings}
                onOpenEngineSettings={props.onOpenEngineSettings}
                overview={vceOverview}
                projectId={project.id}
              />
            ) : null}

            {activeSection === "knowledge-bases" ? (
              <ProjectKnowledgeBasesSection projectId={project.id} />
            ) : null}

            {activeSection === "collaboration-grants" ? (
              <ProjectCollaborationGrantsSection projectId={project.id} workbench={props.workbench} />
            ) : null}

            {activeSection === "attachments" ? (
              <ProjectAttachmentCleanupSection
                attachmentRoot={attachmentRoot}
                cleanupBusy={cleanupBusy}
                cleanupCutoffDate={cleanupCutoffDate}
                cleanupKinds={cleanupKinds}
                cleanupNotice={cleanupNotice ?? t("projectSettings.cleanup.initialNotice")}
                cleanupPreview={cleanupPreview}
                cleanupPreviewMatchesCurrent={Boolean(cleanupPreviewMatchesCurrent)}
                cleanupReport={cleanupReport}
                onChangeCleanupCutoffDate={(cutoffDate) => {
                  if (cleanupOperationInFlightRef.current) {
                    return;
                  }
                  cleanupOperationSequenceRef.current += 1;
                  setCleanupCutoffDate(cutoffDate);
                  setCleanupPreview(null);
                }}
                onChangeCleanupKinds={(kinds) => {
                  if (cleanupOperationInFlightRef.current) {
                    return;
                  }
                  cleanupOperationSequenceRef.current += 1;
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
