// State hooks behind the project home cards: the project memory (VMM) draft, Agent Skill overrides and attachment
// cleanup. Each hook binds asynchronous results to one page lifetime of one project, so a late response never writes
// into another project's page.
// 项目首页卡片背后的状态钩子：项目记忆（VMM）草稿、Agent Skill 覆盖与附件清理。每个钩子都把异步结果绑定到
// 单个项目的一次页面周期，迟到的响应绝不会写入其他项目的页面。
import React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import { useI18n } from "@/i18n";
import type { VmmConfigDraftSummary } from "@/types";
import type {
  ProjectAgentSkillState,
  ProjectAttachmentCleanupKinds,
  ProjectAttachmentCleanupPreviewSummary,
  ProjectAttachmentCleanupReportSummary,
  ProjectAttachmentCleanupRequest,
} from "./project-settings-types";

// PROJECT_SETTINGS_RETRY_BASE_DELAY_MS is the first reconnect recovery delay.
// PROJECT_SETTINGS_RETRY_BASE_DELAY_MS 是首次连接恢复重试的等待时间。
const PROJECT_SETTINGS_RETRY_BASE_DELAY_MS = 1_000;

// PROJECT_SETTINGS_RETRY_MAX_DELAY_MS caps background recovery traffic during a prolonged outage.
// PROJECT_SETTINGS_RETRY_MAX_DELAY_MS 限制长时间故障期间的后台恢复流量。
const PROJECT_SETTINGS_RETRY_MAX_DELAY_MS = 30_000;

// ALL_CLEANUP_KINDS is the cleanup selection a page starts with.
// ALL_CLEANUP_KINDS 是页面初始的清理类型选择。
const ALL_CLEANUP_KINDS: ProjectAttachmentCleanupKinds = { image: true, audio: true, video: true, pdf: true };

// unsavedVmmDrafts keeps an edited, unsaved project memory draft when the home page is left, so returning restores it.
// The map lives in memory only; nothing is written until the user saves.
// unsavedVmmDrafts 在离开项目首页时保留已编辑未保存的项目记忆草稿，回到首页时恢复。
// 该映射只存在于内存中；用户保存之前不写入任何内容。
const unsavedVmmDrafts = new Map<string, VmmConfigDraftSummary>();

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
  return Math.min(PROJECT_SETTINGS_RETRY_BASE_DELAY_MS * (2 ** exponent), PROJECT_SETTINGS_RETRY_MAX_DELAY_MS);
}

// errorText reads a rejection as display text.
// errorText 将拒绝原因读取为显示文本。
function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

// ProjectScopedError is a failure message tied to the project it was reported for.
// ProjectScopedError 是绑定到其报告项目的失败消息。
type ProjectScopedError = { projectId: string; message: string } | null;

// useProjectActionOwner returns a ref whose object is replaced for every page lifetime of a project; an async action
// captures the object and writes its result only while the same object is current.
// useProjectActionOwner 返回一个引用，其对象在项目的每次页面周期都会替换；异步动作捕获该对象，
// 只有该对象仍为当前对象时才写入结果。
function useProjectActionOwner(projectId: string): React.MutableRefObject<{ projectId: string }> {
  const ownerRef = React.useRef({ projectId });
  React.useLayoutEffect(() => {
    // A fresh object distinguishes leaving and reopening the same project from the previous page lifetime.
    // 新对象可区分同一项目离开后重开与之前的页面周期。
    ownerRef.current = { projectId };
  }, [projectId]);
  return ownerRef;
}

// useRetryingProjectRead loads one project settings domain whenever its revision advances, retrying failures with a
// capped backoff, and reports the first failure of a streak as a toast.
// useRetryingProjectRead 在项目设置领域版本推进时加载该领域，失败时以有上限的退避重试，并对连续失败的首次失败弹出提示。
function useRetryingProjectRead<T>(options: {
  // Project whose domain is read.
  // 读取其领域的项目。
  projectId: string;
  // Latest accepted invalidation revision; zero still loads once on first open.
  // 最新已接收的失效版本；零版本在首次打开时同样加载一次。
  revision: number;
  // Pauses reads, for example while a local draft has unsaved edits.
  // 暂停读取，例如本地草稿存在未保存编辑时。
  paused: boolean;
  // Reads the domain.
  // 读取该领域。
  load: (projectId: string) => Promise<T>;
  // Applies one successful read.
  // 应用一次成功读取。
  onLoaded: (value: T) => void;
  // Records one failed read.
  // 记录一次失败读取。
  onFailed: (message: string) => void;
}) {
  // loadedRevisionByProjectRef advances only after one read succeeds.
  // loadedRevisionByProjectRef 仅在一次读取成功后推进。
  const loadedRevisionByProjectRef = React.useRef<Map<string, number>>(new Map());
  // failureCountByProjectRef tracks consecutive failures for reconnect backoff.
  // failureCountByProjectRef 跟踪连续失败次数以执行重连退避。
  const failureCountByProjectRef = React.useRef<Map<string, number>>(new Map());
  // loadSequenceRef prevents an older read from overwriting a newer one.
  // loadSequenceRef 防止较旧读取覆盖较新的读取。
  const loadSequenceRef = React.useRef(0);
  // retryNonce triggers a fresh authoritative read after a backoff delay or an explicit reload.
  // retryNonce 在退避结束或显式重新加载后触发新的权威读取。
  const [retryNonce, setRetryNonce] = React.useState(0);
  const [loading, setLoading] = React.useState(false);
  // callbacksRef lets the read effect use the newest callbacks without re-running for each render.
  // callbacksRef 让读取副作用使用最新回调，而无需每次渲染都重新执行。
  const callbacksRef = React.useRef({ onLoaded: options.onLoaded, onFailed: options.onFailed });
  React.useLayoutEffect(() => {
    callbacksRef.current = { onLoaded: options.onLoaded, onFailed: options.onFailed };
  });
  const { projectId, revision, paused, load } = options;

  React.useLayoutEffect(() => {
    loadedRevisionByProjectRef.current.delete(projectId);
    failureCountByProjectRef.current.delete(projectId);
    loadSequenceRef.current += 1;
    setRetryNonce(0);
    setLoading(false);
  }, [projectId]);

  React.useEffect(() => {
    if (paused) return;
    const loadedRevision = loadedRevisionByProjectRef.current.get(projectId);
    if (loadedRevision !== undefined && revision <= loadedRevision) return;
    let cancelled = false;
    // loadSequence immediately supersedes every earlier read.
    // loadSequence 立即取代所有更早的读取。
    const loadSequence = loadSequenceRef.current + 1;
    loadSequenceRef.current = loadSequence;
    const current = () => !cancelled && loadSequenceRef.current === loadSequence;
    // retryTimer schedules recovery without requiring a new settings revision.
    // retryTimer 在无需新设置版本的情况下安排恢复读取。
    let retryTimer: number | null = null;
    setLoading(true);
    void load(projectId)
      .then((value) => {
        if (!current()) return;
        callbacksRef.current.onLoaded(value);
        loadedRevisionByProjectRef.current.set(projectId, revision);
        failureCountByProjectRef.current.delete(projectId);
      })
      .catch((error) => {
        if (!current()) return;
        // failureCount is scoped to this exact domain and resets only after a successful read.
        // failureCount 仅属于当前准确领域，并且只在成功读取后重置。
        const failureCount = (failureCountByProjectRef.current.get(projectId) ?? 0) + 1;
        failureCountByProjectRef.current.set(projectId, failureCount);
        callbacksRef.current.onFailed(errorText(error));
        if (failureCount === 1) toast.error(errorText(error));
        retryTimer = window.setTimeout(() => {
          if (current()) setRetryNonce((nonce) => nonce + 1);
        }, projectSettingsRetryDelayMs(failureCount));
      })
      .finally(() => {
        if (current()) setLoading(false);
      });
    return () => {
      cancelled = true;
      if (retryTimer !== null) window.clearTimeout(retryTimer);
    };
  }, [projectId, load, revision, paused, retryNonce]);

  return {
    loading,
    // loadedRevision reports whether the latest accepted revision has been read for this project.
    // loadedRevision 报告当前项目是否已读取最新已接收版本。
    loadedRevision: loadedRevisionByProjectRef.current.get(projectId) === revision,
    // reload forgets the loaded revision and reads again.
    // reload 遗忘已加载版本并重新读取。
    reload: () => {
      loadedRevisionByProjectRef.current.delete(projectId);
      setRetryNonce((nonce) => nonce + 1);
    },
  };
}

// useProjectVmmDraft owns the editable project memory draft: authoritative reads, saves, discards, and keeping
// unsaved edits in memory when the page is left.
// useProjectVmmDraft 管理可编辑的项目记忆草稿：权威读取、保存、放弃，以及离开页面时在内存中保留未保存编辑。
export function useProjectVmmDraft(options: {
  // Project whose memory settings are edited.
  // 编辑其记忆设置的项目。
  projectId: string;
  // Latest accepted VMM settings revision.
  // 最新已接收的 VMM 设置版本。
  revision: number;
  // Loads the project VMM draft.
  // 加载项目 VMM 草稿。
  onLoad: (projectId: string) => Promise<VmmConfigDraftSummary>;
  // Saves the project VMM draft.
  // 保存项目 VMM 草稿。
  onSave: (projectId: string, draft: VmmConfigDraftSummary) => Promise<void>;
}) {
  const { t } = useI18n();
  const { projectId } = options;
  // draft stores the editable configuration; draftProjectId names the project it was read or restored for.
  // draft 保存可编辑配置；draftProjectId 指明其读取或恢复所属的项目。
  const [draft, setDraft] = React.useState<VmmConfigDraftSummary | null>(null);
  const [draftProjectId, setDraftProjectId] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  // dirty records unsaved local edits; restored marks a draft carried over from an earlier visit.
  // dirty 记录未保存的本地编辑；restored 标记从上次访问带回的草稿。
  const [dirty, setDirty] = React.useState(false);
  const [restored, setRestored] = React.useState(false);
  const [error, setError] = React.useState<ProjectScopedError>(null);
  const ownerRef = useProjectActionOwner(projectId);
  // saveSequenceRef lets only the latest save finalize state; editSequenceRef keeps edits made while a save runs.
  // saveSequenceRef 只允许最新一次保存收尾状态；editSequenceRef 保留保存进行中产生的新编辑。
  const saveSequenceRef = React.useRef(0);
  const editSequenceRef = React.useRef(0);
  // saveInFlightRef synchronously rejects duplicate save clicks before React commits busy state.
  // saveInFlightRef 在 React 提交忙碌状态前同步拒绝重复保存点击。
  const saveInFlightRef = React.useRef(false);
  // latestRef lets the leave handler see the newest draft without re-running per edit.
  // latestRef 让离开处理读取最新草稿，而无需每次编辑都重新执行。
  const latestRef = React.useRef({ projectId, draft, dirty });
  React.useLayoutEffect(() => {
    latestRef.current = { projectId, draft, dirty };
  });

  const read = useRetryingProjectRead({
    projectId,
    revision: options.revision,
    paused: dirty || saving,
    load: options.onLoad,
    onLoaded: (value: VmmConfigDraftSummary) => {
      setDraft(value);
      setDraftProjectId(projectId);
      setError(null);
      setDirty(false);
      setRestored(false);
      unsavedVmmDrafts.delete(projectId);
    },
    onFailed: (message) => setError({ projectId, message }),
  });

  React.useLayoutEffect(() => {
    saveSequenceRef.current += 1;
    editSequenceRef.current = 0;
    saveInFlightRef.current = false;
    // A draft left unsaved on an earlier visit comes back as the visible, still-unsaved draft. It stays in the map
    // until a save, discard or reload settles it, so a remount before the next render cannot lose it.
    // 上次访问未保存的草稿会作为仍未保存的可见草稿恢复。它在保存、放弃或重新加载前一直留在映射中，
    // 因此在下次渲染前的重新挂载也不会丢失。
    const restoredDraft = unsavedVmmDrafts.get(projectId) ?? null;
    setDraft(restoredDraft);
    setDraftProjectId(restoredDraft ? projectId : null);
    setDirty(restoredDraft !== null);
    setRestored(restoredDraft !== null);
    setSaving(false);
    setError(null);
    return () => {
      // Leaving with unsaved memory edits keeps them in memory and says where to find them.
      // 带着未保存的记忆编辑离开时，在内存中保留并提示回到哪里保存。
      const latest = latestRef.current;
      if (latest.projectId === projectId && latest.dirty && latest.draft) {
        unsavedVmmDrafts.set(projectId, latest.draft);
        toast.info(t("projectHome.vmmKept"), { id: `project-home-vmm-kept-${projectId}` });
      }
    };
    // t is read only when leaving; the active language at that moment is the right one.
    // t 只在离开时读取；该时刻的当前语言即为正确语言。
  }, [projectId]);

  // update applies a partial edit to the current draft.
  // update 将局部编辑应用到当前草稿。
  const update = (patch: Partial<VmmConfigDraftSummary>) => {
    if (!draft) return;
    editSequenceRef.current += 1;
    setDirty(true);
    setDraft((current) => current ? { ...current, ...patch } : current);
  };

  // discard drops unsaved edits and reloads the saved settings.
  // discard 丢弃未保存的编辑并重新加载已保存设置。
  const discard = () => {
    if (saveInFlightRef.current) return;
    unsavedVmmDrafts.delete(projectId);
    editSequenceRef.current += 1;
    setDirty(false);
    setRestored(false);
    read.reload();
  };

  // save persists the current draft, or the exact draft given, through the host.
  // save 通过宿主持久化当前草稿或指定的准确草稿。
  const save = async (exactDraft?: VmmConfigDraftSummary): Promise<boolean> => {
    const draftToSave = exactDraft ?? draft;
    if (!draftToSave || saveInFlightRef.current) return false;
    saveInFlightRef.current = true;
    // owner and saveSequence bind every completion path to this exact visible draft.
    // owner 与 saveSequence 将所有完成路径绑定到当前准确的可见草稿。
    const owner = ownerRef.current;
    const saveSequence = saveSequenceRef.current + 1;
    const savedEditSequence = editSequenceRef.current;
    saveSequenceRef.current = saveSequence;
    const current = () => ownerRef.current === owner && saveSequenceRef.current === saveSequence;
    try {
      setSaving(true);
      await options.onSave(projectId, draftToSave);
      if (!current()) return false;
      if (editSequenceRef.current === savedEditSequence) {
        setDraft(draftToSave);
        setDirty(false);
        setRestored(false);
        unsavedVmmDrafts.delete(projectId);
      }
      setError(null);
      return true;
    } catch (failure) {
      if (!current()) return false;
      setError({ projectId, message: errorText(failure) });
      toast.error(errorText(failure));
      return false;
    } finally {
      if (current()) {
        saveInFlightRef.current = false;
        setSaving(false);
      }
    }
  };

  // authoritative is true once the visible draft belongs to this project.
  // authoritative 在可见草稿归属当前项目后为 true。
  const authoritative = draftProjectId === projectId && draft !== null;
  return {
    busy: saving ? "saving" as const : read.loading ? "loading" as const : "idle" as const,
    dirty,
    discard,
    draft,
    // overviewDraft is the draft only when it is authoritative for this project.
    // overviewDraft 仅在草稿对当前项目具有权威性时提供。
    overviewDraft: authoritative ? draft : null,
    error: error?.projectId === projectId ? error.message : null,
    hasOverride: authoritative ? draft.hasOverride : null,
    restored,
    save,
    update,
  };
}

// useProjectAgentSkills owns the project Agent Skill override map: authoritative reads and optimistic saves.
// useProjectAgentSkills 管理项目 Agent Skill 覆盖映射：权威读取与乐观保存。
export function useProjectAgentSkills(options: {
  // Project whose overrides are shown.
  // 展示其覆盖的项目。
  projectId: string;
  // Latest accepted Agent Skill settings revision.
  // 最新已接收的 Agent Skill 设置版本。
  revision: number;
  // Loads project Agent Skill overrides.
  // 加载项目 Agent Skill 覆盖。
  onLoad: (projectId: string) => Promise<Record<string, string>>;
  // Saves one project Agent Skill tri-state override.
  // 保存单个项目 Agent Skill 三态覆盖。
  onSet: (projectId: string, name: string, stateKey: ProjectAgentSkillState) => Promise<void>;
}) {
  const { projectId } = options;
  const [overrides, setOverrides] = React.useState<Record<string, string>>({});
  // overridesProjectId names the project the visible overrides were read for.
  // overridesProjectId 指明可见覆盖读取所属的项目。
  const [overridesProjectId, setOverridesProjectId] = React.useState<string | null>(null);
  const [error, setError] = React.useState<ProjectScopedError>(null);
  // busySkill names the skill being saved.
  // busySkill 指明正在保存的技能。
  const [busySkill, setBusySkill] = React.useState<string | null>(null);
  const ownerRef = useProjectActionOwner(projectId);
  // saveInFlightRef serializes writes and prevents cross-skill rollback races.
  // saveInFlightRef 串行化写入并防止跨技能回滚竞态。
  const saveInFlightRef = React.useRef(false);

  const read = useRetryingProjectRead({
    projectId,
    revision: options.revision,
    paused: false,
    load: options.onLoad,
    onLoaded: (value: Record<string, string>) => {
      setOverrides(value);
      setOverridesProjectId(projectId);
      setError(null);
    },
    onFailed: (message) => setError({ projectId, message }),
  });

  React.useLayoutEffect(() => {
    saveInFlightRef.current = false;
    setOverrides({});
    setOverridesProjectId(null);
    setError(null);
    setBusySkill(null);
  }, [projectId]);

  const loaded = overridesProjectId === projectId;
  const visibleError = error?.projectId === projectId ? error.message : null;

  // set persists one tri-state override optimistically and rolls back on failure.
  // set 乐观持久化单个三态覆盖，失败时回滚。
  const set = async (name: string, stateKey: ProjectAgentSkillState) => {
    if (saveInFlightRef.current || read.loading || !loaded || visibleError !== null || !read.loadedRevision) return;
    saveInFlightRef.current = true;
    // owner prevents a request from one page lifetime from mutating another.
    // owner 防止一个页面周期的请求修改另一个周期。
    const owner = ownerRef.current;
    const previous = overrides;
    setOverrides((current) => {
      const next = { ...current };
      if (stateKey === "inherit") delete next[name];
      else next[name] = stateKey;
      return next;
    });
    setBusySkill(name);
    try {
      await options.onSet(projectId, name, stateKey);
      if (ownerRef.current === owner) setError(null);
    } catch (failure) {
      if (ownerRef.current !== owner) return;
      setOverrides(previous);
      setError({ projectId, message: errorText(failure) });
      toast.error(errorText(failure));
    } finally {
      if (ownerRef.current === owner) {
        saveInFlightRef.current = false;
        setBusySkill(null);
      }
    }
  };

  return {
    busySkill,
    error: visibleError,
    loaded,
    // loadedCurrent is true once the latest accepted revision is the one shown.
    // loadedCurrent 在显示的即为最新已接收版本后为 true。
    loadedCurrent: loaded && read.loadedRevision,
    loading: read.loading,
    overrides,
    retry: () => {
      if (read.loading || saveInFlightRef.current) return;
      read.reload();
    },
    set,
  };
}

// useProjectAttachmentCleanup owns the attachment cleanup form: selection, preview and the destructive execute step,
// which only a fresh matching preview unlocks.
// useProjectAttachmentCleanup 管理附件清理表单：选择、预览以及只有最新匹配预览才能解锁的破坏性执行步骤。
export function useProjectAttachmentCleanup(options: {
  // Project whose attachments are cleaned.
  // 清理其附件的项目。
  projectId: string;
  // Loads a managed attachment cleanup preview.
  // 加载托管附件清理预览。
  onPreview: (request: ProjectAttachmentCleanupRequest) => Promise<ProjectAttachmentCleanupPreviewSummary>;
  // Executes one managed attachment cleanup.
  // 执行一次托管附件清理。
  onExecute: (request: ProjectAttachmentCleanupRequest) => Promise<ProjectAttachmentCleanupReportSummary>;
}) {
  const { t } = useI18n();
  const { projectId } = options;
  const [kinds, setKinds] = React.useState<ProjectAttachmentCleanupKinds>(ALL_CLEANUP_KINDS);
  const [cutoffDate, setCutoffDate] = React.useState("");
  // notice stores an explicit cleanup result; null delegates the initial copy to the active locale.
  // notice 保存明确的清理结果；null 表示初始文案跟随当前语言。
  const [notice, setNotice] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState<"idle" | "preview" | "execute">("idle");
  const [preview, setPreview] = React.useState<ProjectAttachmentCleanupPreviewSummary | null>(null);
  const [report, setReport] = React.useState<ProjectAttachmentCleanupReportSummary | null>(null);
  const ownerRef = useProjectActionOwner(projectId);
  // sequenceRef invalidates results when inputs change; inFlightRef rejects duplicate requests synchronously.
  // sequenceRef 在输入变化时使结果失效；inFlightRef 同步拒绝重复请求。
  const sequenceRef = React.useRef(0);
  const inFlightRef = React.useRef(false);

  React.useLayoutEffect(() => {
    sequenceRef.current += 1;
    inFlightRef.current = false;
    setKinds(ALL_CLEANUP_KINDS);
    setCutoffDate(new Date().toISOString().slice(0, 10));
    setNotice(null);
    setBusy("idle");
    setPreview(null);
    setReport(null);
  }, [projectId]);

  // run sends one preview or execute request bound to the exact project, date and kind selection.
  // run 发送一次绑定到准确项目、日期与类型选择的预览或执行请求。
  const run = async (phase: "preview" | "execute") => {
    if (inFlightRef.current) return;
    inFlightRef.current = true;
    const request: ProjectAttachmentCleanupRequest = { projectId, cutoffDate, selection: kinds };
    const owner = ownerRef.current;
    const sequence = sequenceRef.current + 1;
    sequenceRef.current = sequence;
    const current = () => ownerRef.current === owner && sequenceRef.current === sequence;
    try {
      setBusy(phase);
      if (phase === "preview") {
        const result = await options.onPreview(request);
        if (!current()) return;
        setPreview(result);
        setReport(null);
        setNotice(result.summary);
      } else {
        const result = await options.onExecute(request);
        if (!current()) return;
        setReport(result);
        setPreview(null);
        setNotice(result.summary);
        toast.success(t("projectSettings.cleanupCompleted"));
      }
    } catch (failure) {
      if (!current()) return;
      if (phase === "preview") setPreview(null);
      setNotice(errorText(failure));
      toast.error(errorText(failure));
    } finally {
      if (current()) {
        inFlightRef.current = false;
        setBusy("idle");
      }
    }
  };

  // changeInput applies one selection change and drops the preview it no longer matches.
  // changeInput 应用一次选择变化，并丢弃不再匹配的预览。
  const changeInput = (apply: () => void) => {
    if (inFlightRef.current) return;
    sequenceRef.current += 1;
    apply();
    setPreview(null);
  };

  return {
    cleanupBusy: busy,
    cleanupCutoffDate: cutoffDate,
    cleanupKinds: kinds,
    cleanupNotice: notice ?? t("projectSettings.cleanup.initialNotice"),
    cleanupPreview: preview,
    // A destructive execute needs a fresh positive preview for the same cutoff date.
    // 破坏性执行需要同一截止日期的最新有效正向预览。
    cleanupPreviewMatchesCurrent: Boolean(preview && preview.cutoffDate === cutoffDate && preview.matchedAssetCount > 0),
    cleanupReport: report,
    onChangeCleanupCutoffDate: (value: string) => changeInput(() => setCutoffDate(value)),
    onChangeCleanupKinds: (value: ProjectAttachmentCleanupKinds) => changeInput(() => setKinds(value)),
    onExecuteCleanup: () => void run("execute"),
    onPreviewCleanup: () => void run("preview"),
  };
}
