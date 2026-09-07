import * as React from "react";
import { submitVceJob, updateVceProjectSettings } from "@/service/vce-service";
import type { VceIndexUpdateMode, VceProjectActivityResponse, VceProjectSettingsInput } from "@/types";
import { useVceProjectJob } from "./use-vce-project-job";
import { useVceProjectActivity } from "./use-vce-project-activity";

// Display prerequisite priority separately from job execution; ordinary updates do not revoke queries.
// 将展示前提优先级与作业执行分离；普通更新不撤销查询。
export type VceSurfaceGate = "no_project" | "loading" | "unavailable" | "engine_off" | "project_off" | "uninitialized" | "rebuild_required" | "ready";

// Resolve the explicit prerequisite state from host activity without deriving it from progress.
// 从宿主活动解析明确前提状态，不从进度推断。
// projectId selects ownership, activity supplies metadata, error marks a failed read, and projectEnabled admits current workbench intent.
// projectId 选择归属，activity 提供元数据，error 标记读取失败，projectEnabled 准入当前工作台意图。
// Returns the highest-priority actionable gate.
// 返回优先级最高的可操作门禁。
export function vceSurfaceGate(projectId: string | null, activity: VceProjectActivityResponse | null, error: string | null, projectEnabled: boolean): VceSurfaceGate {
  if (projectId === null) return "no_project";
  if (error !== null) return "unavailable";
  if (activity === null) return "loading";
  if (!activity.configuration.ready) return "engine_off";
  // A cached enabled snapshot cannot overrule the workbench's newer explicit project revocation.
  // 缓存中的已启用快照不能覆盖工作台较新的明确项目撤销。
  if (!projectEnabled || !activity.settings.enabled) return "project_off";
  if (activity.status.lifecycle === "rebuild_required") return "rebuild_required";
  if (activity.status.checkpointVersion == null) return "uninitialized";
  return activity.queryAvailable ? "ready" : "unavailable";
}

// Shared current-project UI contract; job progress and policy are never conversation-owned.
// 共享当前项目界面契约；作业进度与策略绝不归会话持有。
interface VceProjectSurface {
  // Exact selected conversation/draft project, never a different sidebar active project.
  // 精确所选会话或草稿项目，绝非侧栏另一个活动项目。
  projectId: string | null;
  // Current prerequisite projection.
  // 当前前提投影。
  gate: VceSurfaceGate;
  // Authoritative metadata or absence while awaiting this project's response.
  // 权威元数据，等待本项目响应时为空。
  activity: VceProjectActivityResponse | null;
  // Show the floating node only when the authoritative project switch is enabled.
  // 仅在权威项目开关启用时展示浮动节点。
  visible: boolean;
  // Shared durable receipt observation and exact cancellation.
  // 共享持久回执观察与精确取消。
  jobState: ReturnType<typeof useVceProjectJob>;
  // Pending project policy or manual admission, independent from conversation busy state.
  // 等待中的项目策略或手动准入，独立于会话忙碌状态。
  pending: boolean;
  // Explicit status/action diagnostic.
  // 明确状态或操作诊断。
  error: string | null;
  // Read metadata again without starting indexing.
  // 再次读取元数据，不启动索引。
  reload: () => void;
  // Save the selected project mode through its optimistic settings service.
  // 通过项目乐观设置服务保存选定模式。
  saveMode: (mode: VceIndexUpdateMode) => Promise<void>;
  // Submit an explicit manual synchronization and expose its durable receipt.
  // 提交明确手动同步并展示持久回执。
  synchronize: () => Promise<void>;
  // Explicitly retry the latest failed/cancelled/interrupted receipt with a new durable identity.
  // 以新持久身份明确重试最近失败、取消或中断回执。
  retryLatest: () => Promise<void>;
  // Open the fixed ContextEngine model settings node.
  // 打开固定语义引擎模型设置节点。
  openEngineSettings: () => void;
  // Open the selected project's semantic-index settings section.
  // 打开所选项目的语义索引设置分类。
  openProjectSettings: () => void;
}

// No provider means this surface is outside the selected workspace, not an enabled engine.
// 没有提供者表示界面不在所选工作区内，而非引擎已启用。
const VceProjectSurfaceContext = React.createContext<VceProjectSurface | null>(null);
// Geometry consumers observe only this Boolean, so progress updates do not rerender the chat shell.
// 布局消费者仅观察此布尔值，使进度更新不会重新渲染聊天外壳。
const VceProjectVisibilityContext = React.createContext(false);

// Read project-node visibility without subscribing the surrounding conversation layout to progress.
// 读取项目节点可见性，不使外围会话布局订阅进度。
// Takes no parameters. Returns false outside the workspace owner.
// 无参数。工作区归属之外返回假。
export function useVceProjectVisibility(): boolean {
  return React.useContext(VceProjectVisibilityContext);
}

// Read the selected workspace's shared semantic state; returns null outside its provider.
// 读取所选工作区共享语义状态；提供者之外返回空值。
// Takes no parameters and performs no service I/O.
// 无参数，不执行服务输入输出。
export function useVceProjectSurface(): VceProjectSurface | null {
  return React.useContext(VceProjectSurfaceContext);
}

// Own one metadata subscription for the selected project across composer and floating status UI.
// 为输入区与浮动状态界面持有所选项目的一份元数据订阅。
// Props supply exact project authority, invalidation revision, navigation, and child UI.
// 参数提供精确项目权威、失效修订、导航与子界面。
// Returns children under the shared project state, without changing their mount identity.
// 在共享项目状态下返回子界面，不改变子界面的挂载身份。
export function VceProjectSurfaceProvider(props: {
  // Current persisted or draft conversation's project identity.
  // 当前持久或草稿会话的项目身份。
  projectId: string | null;
  // Project switch from the authoritative workbench projection, used for immediate hiding.
  // 权威工作台投影中的项目开关，用于立即隐藏。
  projectEnabled: boolean;
  // Existing host settings revisions and service epoch invalidate stale metadata.
  // 既有宿主设置修订与服务代次使旧元数据失效。
  revisionKey: string;
  // Navigation to the fixed global model settings node.
  // 导航到固定全局模型设置节点。
  onOpenEngineSettings: () => void;
  // Navigation to the exact project settings category.
  // 导航到精确项目设置分类。
  onOpenProjectSettings: () => void;
  // Workspace UI that retains its existing lifecycle during project changes.
  // 项目变化期间保留既有生命周期的工作区界面。
  children: React.ReactNode;
}) {
  // scope includes existing authority revisions, not a fabricated persistent revision source.
  // scope 包含既有权威修订，不虚构持久修订来源。
  const scope = JSON.stringify([props.projectId, props.revisionKey, props.projectEnabled]);
  // observed shares activity with project settings rather than owning another subscription or poll.
  // observed 与项目设置共享活动，不再持有另一份订阅或轮询。
  const observed = useVceProjectActivity(props.projectId, props.revisionKey);
  // failure belongs only to a user action; read/stream failures belong to the shared observer.
  // failure 仅属于用户操作；读取及事件流失败属于共享观察者。
  const [failure, setFailure] = React.useState<{ scope: string; message: string } | null>(null);
  // owner distinguishes effect lifetimes including rapid navigation and settings changes.
  // owner 区分效果生命周期，包括快速导航与设置变化。
  const owner = React.useRef<object | null>(null);
  // Freeze the render's action authority; retired closures cannot borrow a later ref value.
  // 冻结本次渲染的操作权威；退役闭包不能借用之后的引用值。
  const actionOwner = React.useMemo(() => ({}), [scope, observed.reload]);
  // mutation excludes duplicate human actions only; chat is never disabled.
  // mutation 仅排除重复人为操作；绝不禁用聊天。
  const mutation = React.useRef<object | null>(null);
  const [pending, setPending] = React.useState(false);
  // jobState supplies the same durable receipt to every consumer below this provider.
  // jobState 为此提供者下的每个消费者提供同一持久回执。
  const jobState = useVceProjectJob(props.projectId);

  React.useLayoutEffect(() => {
    owner.current = actionOwner;
    mutation.current = null;
    setPending(false);
    setFailure(null);
    return () => { if (owner.current === actionOwner) owner.current = null; };
  }, [actionOwner]);

  React.useEffect(() => { if (jobState.job !== null) void observed.reload(); }, [jobState.job?.jobId, jobState.job?.state, observed.reload]);
  // Only matching responses may control project actions or appear in the floating panel.
  // 仅匹配响应可以控制项目操作或出现在浮动面板。
  const activity = observed.activity;
  const error = failure?.scope === scope ? failure.message : observed.error;
  const gate = vceSurfaceGate(props.projectId, activity, observed.error, props.projectEnabled);
  // mutate captures exact authority and ignores completion after its owning UI lifetime has ended.
  // mutate 捕获精确权威，并忽略所属界面生命周期结束后的完成。
  const mutate = async (action: "synchronize" | VceIndexUpdateMode, retryOfJobId: string | null = null): Promise<void> => {
    if (owner.current !== actionOwner) return;
    if (activity === null || props.projectId === null || mutation.current !== null) return;
    if (gate !== "ready" && !(gate === "uninitialized" && retryOfJobId !== null)) return;
    const identity = actionOwner;
    const requestOwner = {};
    mutation.current = requestOwner;
    setPending(true);
    try {
      if (action === "synchronize") {
        const receipt = await submitVceJob({ projectId: props.projectId,
          action: activity.status.checkpointVersion == null ? "initialize" : "synchronize", retryOfJobId,
          expectedSettingsRevision: activity.settings.revision,
          expectedCheckpointVersion: activity.status.checkpointVersion ?? null,
          idempotencyKey: crypto.randomUUID() });
        if (owner.current === identity) jobState.receive(receipt);
      } else {
        // Explicitly project editable fields so host metadata never enters a settings mutation.
        // 明确投影可编辑字段，使宿主元数据绝不进入设置修改。
        const saved = activity.settings;
        const settings: VceProjectSettingsInput = { enabled: saved.enabled, updateMode: action,
          autoUpdateMinIntervalSeconds: saved.autoUpdateMinIntervalSeconds, maxFileBytes: saved.maxFileBytes,
          extraIgnorePatterns: saved.extraIgnorePatterns, allowQueryRewrite: saved.allowQueryRewrite,
          allowRerank: saved.allowRerank, embeddingMaxConcurrency: saved.embeddingMaxConcurrency,
          queryMaxConcurrency: saved.queryMaxConcurrency, maxResults: saved.maxResults, maxContextChars: saved.maxContextChars };
        await updateVceProjectSettings(props.projectId, saved.revision, settings);
      }
      if (owner.current === identity) { setFailure(null); void observed.reload(); }
    } catch (caught) {
      if (owner.current === identity) setFailure({ scope, message: caught instanceof Error ? caught.message : String(caught) });
    } finally {
      if (mutation.current === requestOwner) { mutation.current = null; setPending(false); }
    }
  };
  // Visibility changes only with project authority, never from progress counters or job phases.
  // 可见性仅随项目权威改变，绝不来自进度计数或作业阶段。
  const visible = props.projectId !== null && props.projectEnabled && (activity === null || activity.settings.enabled);
  return <VceProjectVisibilityContext.Provider value={visible}><VceProjectSurfaceContext.Provider value={{ projectId: props.projectId, gate, activity,
    visible, jobState, pending, error,
    reload: () => { if (owner.current === actionOwner) { setFailure(null); void observed.reload(); } }, saveMode: (mode) => mutate(mode), synchronize: () => mutate("synchronize"),
    retryLatest: async () => {
      const job = jobState.job;
      if (job !== null && (job.state === "failed" || job.state === "cancelled" || job.state === "interrupted")) {
        await mutate("synchronize", job.jobId);
      }
    },
    openEngineSettings: () => { if (owner.current === actionOwner) props.onOpenEngineSettings(); },
    openProjectSettings: () => { if (owner.current === actionOwner) props.onOpenProjectSettings(); } }}>
    {props.children}
  </VceProjectSurfaceContext.Provider></VceProjectVisibilityContext.Provider>;
}
