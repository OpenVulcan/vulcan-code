// Service revision gates and the snapshot/delta synchronization that advances them for the workbench layout.
// 工作台布局的服务版本闸门，以及推进这些闸门的快照/增量同步。
import React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import type { ProjectSettingsRevision } from "@/components/project/project-settings-types";
import {
  isConversationUploadFailure,
  mediaUploadProgressKey,
  reduceMediaUploadProgress,
} from "@/components/workspace/media-upload-progress-state";
import type { WorkbenchDeltaSignal } from "@/components/workspace/workbench-layout/types";
import type { TranslationKey } from "@/i18n";
import {
  applyWorkbenchDelta,
  mergeWorkbenchSnapshot,
  shouldAcceptWorkbenchRevision,
  type WorkbenchMergeOptions,
} from "@/state/workbench-state";
import type {
  AgentSkillsCatalogUpdate,
  ConversationArchivedDelta,
  LuaSkillsCatalogUpdate,
  MediaUploadProgressChangedDelta,
  SubagentCatalogUpdate,
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";

// WorkbenchRevisionGatesInput is the workbench state the revision gates read and write.
// WorkbenchRevisionGatesInput 是版本闸门读取与写入的工作台状态。
export interface WorkbenchRevisionGatesInput {
  // Local service generation that owns every accepted revision.
  // 持有全部已接受版本的本地服务代次。
  serviceInstanceEpoch: number;
  // Rendered workbench state used to seed the gates and track accepted revisions.
  // 用于播种闸门并跟踪已接受版本的已渲染工作台状态。
  workbenchState: WorkbenchSnapshot;
  // Setter for the rendered workbench state.
  // 已渲染工作台状态的设置器。
  setWorkbenchState: React.Dispatch<React.SetStateAction<WorkbenchSnapshot>>;
  // Latest workbench state for convergence checks outside render.
  // 供渲染外收敛检查使用的最新工作台状态。
  workbenchStateRef: React.MutableRefObject<WorkbenchSnapshot>;
}

// WorkbenchRevisionGates is the gate state shared by merges, lifecycle responses and service sync.
// WorkbenchRevisionGates 是合并、生命周期响应与服务同步共享的闸门状态。
export type WorkbenchRevisionGates = ReturnType<typeof useWorkbenchRevisionGates>;

/** Owns every service revision gate so no entrypoint can apply state older than one already accepted.
 * 持有全部服务版本闸门，使任何入口都无法应用比已接受状态更旧的状态。
 * Parameter input carries the workbench state; returns the gates, merge helpers and settings revisions.
 * 参数 input 携带工作台状态；返回闸门、合并辅助函数与设置版本。 */
export function useWorkbenchRevisionGates(input: WorkbenchRevisionGatesInput) {
  const { workbenchState, setWorkbenchState, workbenchStateRef } = input;
  // acceptedWorkbenchRevisionRef tracks the highest service revision accepted by any state entrypoint.
  // acceptedWorkbenchRevisionRef 跟踪任意状态入口已接受的最高服务版本。
  const acceptedWorkbenchRevisionRef = React.useRef(workbenchState.revision ?? 0);
  // acceptedLuaSkillsRevisionRef tracks the newest full LuaSkills domain projection applied locally.
  // acceptedLuaSkillsRevisionRef 跟踪本地已应用的最新完整 LuaSkills 领域投影版本。
  const acceptedLuaSkillsRevisionRef = React.useRef(workbenchState.revision ?? 0);
  // acceptedAgentSkillsRevisionRef tracks the newest full Agent Skills domain projection applied locally.
  // acceptedAgentSkillsRevisionRef 跟踪本地已应用的最新完整 Agent Skills 领域投影版本。
  const acceptedAgentSkillsRevisionRef = React.useRef(workbenchState.revision ?? 0);
  // acceptedSubagentCatalogRevisionRef tracks the newest full agent-catalog domain projection applied locally.
  // acceptedSubagentCatalogRevisionRef 跟踪本地已应用的最新完整智能体目录领域投影版本。
  const acceptedSubagentCatalogRevisionRef = React.useRef(workbenchState.revision ?? 0);
  // acceptedConversationLifecycleRevisionRef tracks the newest complete conversation lifecycle transition independently.
  // acceptedConversationLifecycleRevisionRef 独立跟踪最新的完整会话生命周期转换。
  const acceptedConversationLifecycleRevisionRef = React.useRef(workbenchState.revision ?? 0);
  // acceptedConversationLifecycleRevisionByIdRef orders target-owned archive responses independently.
  // acceptedConversationLifecycleRevisionByIdRef 按目标独立排序归档响应。
  const acceptedConversationLifecycleRevisionByIdRef = React.useRef<Map<string, number>>(new Map());
  // acceptedProjectLifecycleRevisionRef tracks the newest authoritative full-project lifecycle snapshot.
  // acceptedProjectLifecycleRevisionRef 跟踪最新权威完整项目生命周期快照。
  const acceptedProjectLifecycleRevisionRef = React.useRef(workbenchState.revision ?? 0);
  // acceptedProjectLifecycleRevisionByIdRef orders target-owned project removal responses independently.
  // acceptedProjectLifecycleRevisionByIdRef 按目标独立排序项目移除响应。
  const acceptedProjectLifecycleRevisionByIdRef = React.useRef<Map<string, number>>(new Map());
  // acceptedProjectSettingsRevisionByKeyRef orders settings invalidations independently by project and domain.
  // acceptedProjectSettingsRevisionByKeyRef 按项目与领域独立排序设置失效通知。
  const acceptedProjectSettingsRevisionByKeyRef = React.useRef<Map<string, number>>(new Map());
  // projectSettingsRevisionInitializedRef limits initial snapshot seeding to the first render.
  // projectSettingsRevisionInitializedRef 将初始快照播种限制在首次渲染。
  const projectSettingsRevisionInitializedRef = React.useRef(false);
  if (!projectSettingsRevisionInitializedRef.current) {
    for (const [projectId, revisions] of Object.entries(workbenchState.projectSettingsRevisions.byProject)) {
      acceptedProjectSettingsRevisionByKeyRef.current.set(`${projectId}:vmm`, revisions.vmm);
      acceptedProjectSettingsRevisionByKeyRef.current.set(`${projectId}:agentSkills`, revisions.agentSkills);
    }
    projectSettingsRevisionInitializedRef.current = true;
  }
  // acceptedGlobalProjectSettingsRevisionByDomainRef orders global invalidations independently by domain.
  // acceptedGlobalProjectSettingsRevisionByDomainRef 按领域独立排序全局设置失效通知。
  const acceptedGlobalProjectSettingsRevisionByDomainRef = React.useRef<ProjectSettingsRevision>(
    workbenchState.projectSettingsRevisions.global,
  );
  // acceptedServiceInstanceEpochRef identifies which local service owns all accepted revisions.
  // acceptedServiceInstanceEpochRef 标识所有已接受版本归属的本地服务实例。
  const acceptedServiceInstanceEpochRef = React.useRef(input.serviceInstanceEpoch);
  acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, workbenchState.revision ?? 0);
  // mergeWorkbenchWithLocalCaches centralizes service snapshot merges while preserving UI-local state.
  // mergeWorkbenchWithLocalCaches 集中处理服务快照合并，同时保留界面本地状态。
  const mergeWorkbenchWithLocalCaches = React.useCallback((
    current: WorkbenchSnapshot,
    incoming: WorkbenchSnapshot,
    options?: WorkbenchMergeOptions,
  ) => (
    (() => {
      const merged = mergeWorkbenchSnapshot(
        current,
        incoming,
        options,
      );
      if (merged !== current) {
        // mergedRevision is the authoritative full-snapshot version accepted by this merge.
        // mergedRevision 是本次合并接受的权威全量快照版本。
        const mergedRevision = merged.revision ?? 0;
        acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, mergedRevision);
        acceptedLuaSkillsRevisionRef.current = Math.max(acceptedLuaSkillsRevisionRef.current, mergedRevision);
        acceptedAgentSkillsRevisionRef.current = Math.max(acceptedAgentSkillsRevisionRef.current, mergedRevision);
        acceptedSubagentCatalogRevisionRef.current = Math.max(acceptedSubagentCatalogRevisionRef.current, mergedRevision);
        acceptedConversationLifecycleRevisionRef.current = Math.max(
          acceptedConversationLifecycleRevisionRef.current,
          mergedRevision,
        );
        acceptedProjectLifecycleRevisionRef.current = Math.max(
          acceptedProjectLifecycleRevisionRef.current,
          mergedRevision,
        );
      }
      return merged;
    })()
  ), []);
  // resetAcceptedWorkbenchOrderingForServiceInstance clears ordering owned by a replaced service.
  // resetAcceptedWorkbenchOrderingForServiceInstance 清理由已替换服务持有的版本排序。
  const resetAcceptedWorkbenchOrderingForServiceInstance = React.useCallback((
    serviceInstanceEpoch: number,
  ): boolean => {
    // serviceInstanceChanged is the only condition that permits a lower revision to become authoritative.
    // serviceInstanceChanged 是唯一允许较低版本重新成为权威状态的条件。
    const serviceInstanceChanged =
      acceptedServiceInstanceEpochRef.current !== serviceInstanceEpoch;
    if (!serviceInstanceChanged) {
      return false;
    }
    acceptedServiceInstanceEpochRef.current = serviceInstanceEpoch;
    acceptedWorkbenchRevisionRef.current = 0;
    acceptedLuaSkillsRevisionRef.current = 0;
    acceptedAgentSkillsRevisionRef.current = 0;
    acceptedSubagentCatalogRevisionRef.current = 0;
    acceptedConversationLifecycleRevisionRef.current = 0;
    acceptedConversationLifecycleRevisionByIdRef.current.clear();
    acceptedProjectLifecycleRevisionRef.current = 0;
    acceptedProjectLifecycleRevisionByIdRef.current.clear();
    acceptedProjectSettingsRevisionByKeyRef.current.clear();
    acceptedGlobalProjectSettingsRevisionByDomainRef.current = {
      vmm: 0,
      agentSkills: 0,
    };
    return true;
  }, []);
  // acceptServiceWorkbenchSnapshot reserves one service snapshot revision before applying related local state.
  // acceptServiceWorkbenchSnapshot 在应用关联本地状态前预占一个服务快照版本。
  const acceptServiceWorkbenchSnapshot = React.useCallback((
    snapshot: WorkbenchSnapshot,
    options?: Pick<WorkbenchMergeOptions, "allowSameServiceRevision">,
  ): boolean => {
    const incomingRevision = snapshot.revision ?? 0;
    if (
      !(options?.allowSameServiceRevision && incomingRevision === acceptedWorkbenchRevisionRef.current)
      && !shouldAcceptWorkbenchRevision(acceptedWorkbenchRevisionRef.current, incomingRevision)
    ) {
      return false;
    }
    acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, incomingRevision);
    acceptedLuaSkillsRevisionRef.current = Math.max(acceptedLuaSkillsRevisionRef.current, incomingRevision);
    acceptedAgentSkillsRevisionRef.current = Math.max(acceptedAgentSkillsRevisionRef.current, incomingRevision);
    acceptedSubagentCatalogRevisionRef.current = Math.max(acceptedSubagentCatalogRevisionRef.current, incomingRevision);
    acceptedConversationLifecycleRevisionRef.current = Math.max(
      acceptedConversationLifecycleRevisionRef.current,
      incomingRevision,
    );
    acceptedProjectLifecycleRevisionRef.current = Math.max(
      acceptedProjectLifecycleRevisionRef.current,
      incomingRevision,
    );
    return true;
  }, []);
  // acceptConversationArchiveTransitions filters archive payloads through full-snapshot and per-target revisions.
  // acceptConversationArchiveTransitions 通过完整快照与按目标版本过滤归档载荷。
  //
  // Parameters: transitions are exact archive payloads and generation is their committed backend revision.
  // 参数：transitions 是精确归档载荷，generation 是其已提交后端版本。
  // Returns: only transitions that are newer for their own conversation lifecycle target.
  // 返回值：仅返回对各自会话生命周期目标而言较新的转换。
  const acceptConversationArchiveTransitions = React.useCallback((
    transitions: ConversationArchivedDelta[],
    generation: number,
  ): ConversationArchivedDelta[] => transitions.filter((transition) => {
    // targetRevision combines the latest full snapshot and target-owned archive response.
    // targetRevision 合并最新完整快照与目标自有归档响应。
    const targetRevision = Math.max(
      acceptedConversationLifecycleRevisionRef.current,
      acceptedConversationLifecycleRevisionByIdRef.current.get(transition.conversationId) ?? 0,
    );
    if (!shouldAcceptWorkbenchRevision(targetRevision, generation)) {
      return false;
    }
    acceptedConversationLifecycleRevisionByIdRef.current.set(transition.conversationId, generation);
    return true;
  }), []);
  // acceptProjectRemovalTransition reserves one target-owned project removal revision.
  // acceptProjectRemovalTransition 预占一条目标自有项目移除版本。
  //
  // Parameters: projectId is the removed project target and generation is the committed backend revision.
  // 参数：projectId 是移除项目目标，generation 是已提交后端版本。
  // Returns: true only when this removal is newer for the exact project target.
  // 返回值：仅当此移除对精确项目目标而言较新时返回 true。
  const acceptProjectRemovalTransition = React.useCallback((projectId: string, generation: number): boolean => {
    // targetRevision combines the latest full snapshot and target-owned project removal response.
    // targetRevision 合并最新完整快照与目标自有项目移除响应。
    const targetRevision = Math.max(
      acceptedProjectLifecycleRevisionRef.current,
      acceptedProjectLifecycleRevisionByIdRef.current.get(projectId) ?? 0,
    );
    if (!shouldAcceptWorkbenchRevision(targetRevision, generation)) {
      return false;
    }
    acceptedProjectLifecycleRevisionByIdRef.current.set(projectId, generation);
    return true;
  }, []);
  // applyLifecycleResponseSnapshot applies only archive and project-removal fields from a rejected full RPC snapshot.
  // applyLifecycleResponseSnapshot 仅从被拒绝的完整 RPC 快照应用归档与项目移除字段。
  //
  // Parameters: snapshot is the committed RPC result, conversationIds are archived targets, and removedProjectId is the optional removed project.
  // 参数：snapshot 是已提交 RPC 结果，conversationIds 是归档目标，removedProjectId 是可选的已移除项目。
  // Returns: true when at least one target transition was applied or the targets already converged.
  // 返回值：至少应用一条目标转换或目标已经收敛时返回 true。
  const applyLifecycleResponseSnapshot = React.useCallback((
    snapshot: WorkbenchSnapshot,
    conversationIds: string[],
    removedProjectId?: string,
  ): boolean => {
    // generation is the exact backend revision committed by the lifecycle RPC.
    // generation 是生命周期 RPC 提交的精确后端版本。
    const generation = snapshot.revision ?? 0;
    // archiveTransitions project only target-owned fields from the full RPC snapshot.
    // archiveTransitions 仅从完整 RPC 快照投影目标自有字段。
    const archiveTransitions = conversationIds.map((conversationId): ConversationArchivedDelta => ({
      conversationId,
      archivedConversation: (snapshot.archivedConversations ?? [])
        .find((conversation) => conversation.id === conversationId) ?? null,
      selectedConversationId: snapshot.selectedConversationId ?? null,
    }));
    // acceptedArchives contains only target transitions newer than their own lifecycle state.
    // acceptedArchives 仅包含比各自生命周期状态更新的目标转换。
    const acceptedArchives = acceptConversationArchiveTransitions(archiveTransitions, generation);
    // projectAccepted records whether the optional project removal owns a newer target revision.
    // projectAccepted 记录可选项目移除是否拥有较新的目标版本。
    const projectAccepted = removedProjectId
      ? acceptProjectRemovalTransition(removedProjectId, generation)
      : false;
    if (acceptedArchives.length === 0 && !projectAccepted) {
      // currentWorkbench is used only to verify that a previously accepted transition already converged.
      // currentWorkbench 仅用于验证此前接受的转换已经收敛。
      const currentWorkbench = workbenchStateRef.current;
      // conversationsScheduled recognizes target transitions already accepted but not yet committed by React rendering.
      // conversationsScheduled 识别已经接受但尚未由 React 渲染提交的目标转换。
      const conversationsScheduled = conversationIds.every((conversationId) => Math.max(
        acceptedConversationLifecycleRevisionRef.current,
        acceptedConversationLifecycleRevisionByIdRef.current.get(conversationId) ?? 0,
      ) >= generation);
      const conversationsConverged = conversationIds.every((conversationId) => (
        !currentWorkbench.conversations.some((conversation) => conversation.id === conversationId)
      ));
      // projectScheduled recognizes a project removal already accepted but not yet rendered.
      // projectScheduled 识别已经接受但尚未渲染的项目移除。
      const projectScheduled = !removedProjectId || Math.max(
        acceptedProjectLifecycleRevisionRef.current,
        acceptedProjectLifecycleRevisionByIdRef.current.get(removedProjectId) ?? 0,
      ) >= generation;
      const projectConverged = !removedProjectId
        || !currentWorkbench.projects.some((project) => project.id === removedProjectId);
      return (conversationsScheduled || conversationsConverged)
        && (projectScheduled || projectConverged);
    }
    // acceptedEnvelopes separates conversation and project domains when only one target class is newer.
    // acceptedEnvelopes 在仅一类目标较新时拆分会话与项目领域。
    const acceptedEnvelopes: WorkbenchDeltaEnvelope[] = [];
    if (projectAccepted && removedProjectId) {
      acceptedEnvelopes.push({
        generation,
        delta: {
          kind: "projectRemoved",
          payload: {
            projectId: removedProjectId,
            projects: snapshot.projects,
            expandedProjectIds: snapshot.expandedProjectIds ?? [],
            archivedConversations: acceptedArchives,
          },
        },
      });
    } else if (acceptedArchives.length > 0) {
      acceptedEnvelopes.push({
        generation,
        delta: {
          kind: "conversationsArchived",
          payload: { archivedConversations: acceptedArchives },
        },
      });
    }
    acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, generation);
    setWorkbenchState((current) => acceptedEnvelopes.reduce(
      (nextWorkbench, envelope) => applyWorkbenchDelta(nextWorkbench, envelope),
      current,
    ));
    return true;
  }, [acceptConversationArchiveTransitions, acceptProjectRemovalTransition]);
  // applyLocalPreviewWithLocalCaches applies optimistic snapshots without service revision gating.
  // applyLocalPreviewWithLocalCaches 应用乐观快照，不经过服务版本门禁。
  const applyLocalPreviewWithLocalCaches = React.useCallback((current: WorkbenchSnapshot, incoming: WorkbenchSnapshot) => (
    incoming
  ), []);
  // applyLuaSkillsCatalogUpdate applies the newest complete LuaSkills domain response independently of unrelated deltas.
  // applyLuaSkillsCatalogUpdate 独立于无关增量应用最新的完整 LuaSkills 领域响应。
  //
  // Parameters: update contains the committed revision, LuaSkills catalog, and derived plugin rows.
  // 参数：update 包含已提交版本、LuaSkills 目录与派生插件行。
  // Returns: nothing; accepted data is scheduled into workbench state without lowering its global revision.
  // 返回值：无；已接受数据会安排到工作台状态且不会降低全局版本。
  const applyLuaSkillsCatalogUpdate = React.useCallback((update: LuaSkillsCatalogUpdate) => {
    if (!shouldAcceptWorkbenchRevision(acceptedLuaSkillsRevisionRef.current, update.revision)) {
      return;
    }
    acceptedLuaSkillsRevisionRef.current = update.revision;
    acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, update.revision);
    setWorkbenchState((current) => {
      if (update.revision < acceptedLuaSkillsRevisionRef.current) {
        return current;
      }
      return {
        ...current,
        revision: Math.max(current.revision ?? 0, update.revision),
        luaSkills: update.luaSkills,
        plugins: update.plugins,
      };
    });
  }, []);
  // applyAgentSkillsCatalogUpdate applies the newest complete Agent Skills domain response independently of unrelated deltas.
  // applyAgentSkillsCatalogUpdate 独立于无关增量应用最新的完整 Agent Skills 领域响应。
  //
  // Parameters: update contains the committed revision and complete Agent Skills catalog.
  // 参数：update 包含已提交版本与完整 Agent Skills 目录。
  // Returns: nothing; accepted data is scheduled into workbench state without lowering its global revision.
  // 返回值：无；已接受数据会安排到工作台状态且不会降低全局版本。
  const applyAgentSkillsCatalogUpdate = React.useCallback((update: AgentSkillsCatalogUpdate) => {
    if (!shouldAcceptWorkbenchRevision(acceptedAgentSkillsRevisionRef.current, update.revision)) {
      return;
    }
    acceptedAgentSkillsRevisionRef.current = update.revision;
    acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, update.revision);
    setWorkbenchState((current) => {
      if (update.revision < acceptedAgentSkillsRevisionRef.current) {
        return current;
      }
      return {
        ...current,
        revision: Math.max(current.revision ?? 0, update.revision),
        agentSkills: update.agentSkills,
      };
    });
  }, []);
  // applySubagentCatalogUpdate applies the newest complete agent-catalog domain response independently of unrelated deltas.
  // applySubagentCatalogUpdate 独立于无关增量应用最新的完整智能体目录领域响应。
  //
  // Parameters: update contains the committed revision and complete agent catalog.
  // 参数：update 包含已提交版本与完整智能体目录。
  // Returns: nothing; accepted data is scheduled into workbench state without lowering its global revision.
  // 返回值：无；已接受数据会安排到工作台状态且不会降低全局版本。
  const applySubagentCatalogUpdate = React.useCallback((update: SubagentCatalogUpdate) => {
    if (!shouldAcceptWorkbenchRevision(acceptedSubagentCatalogRevisionRef.current, update.revision)) {
      return;
    }
    acceptedSubagentCatalogRevisionRef.current = update.revision;
    acceptedWorkbenchRevisionRef.current = Math.max(acceptedWorkbenchRevisionRef.current, update.revision);
    setWorkbenchState((current) => {
      if (update.revision < acceptedSubagentCatalogRevisionRef.current) {
        return current;
      }
      return {
        ...current,
        revision: Math.max(current.revision ?? 0, update.revision),
        subagentCatalog: update.subagentCatalog,
      };
    });
  }, []);
  // projectSettingsRevisionById exposes accepted domain revisions without coupling projects or drafts.
  // projectSettingsRevisionById 暴露已接收的领域版本，同时避免项目或草稿相互耦合。
  const [projectSettingsRevisionById, setProjectSettingsRevisionById] = React.useState<Record<string, ProjectSettingsRevision>>(
    workbenchState.projectSettingsRevisions.byProject,
  );
  // globalProjectSettingsRevision exposes independently ordered global invalidations for every project dialog.
  // globalProjectSettingsRevision 向所有项目弹窗暴露独立排序的全局失效版本。
  const [globalProjectSettingsRevision, setGlobalProjectSettingsRevision] = React.useState<ProjectSettingsRevision>(
    workbenchState.projectSettingsRevisions.global,
  );
  return {
    acceptedWorkbenchRevisionRef,
    acceptedLuaSkillsRevisionRef,
    acceptedAgentSkillsRevisionRef,
    acceptedSubagentCatalogRevisionRef,
    acceptedConversationLifecycleRevisionRef,
    acceptedConversationLifecycleRevisionByIdRef,
    acceptedProjectLifecycleRevisionRef,
    acceptedProjectLifecycleRevisionByIdRef,
    acceptedProjectSettingsRevisionByKeyRef,
    acceptedGlobalProjectSettingsRevisionByDomainRef,
    acceptedServiceInstanceEpochRef,
    resetAcceptedWorkbenchOrderingForServiceInstance,
    acceptServiceWorkbenchSnapshot,
    acceptConversationArchiveTransitions,
    acceptProjectRemovalTransition,
    setProjectSettingsRevisionById,
    setGlobalProjectSettingsRevision,
    mergeWorkbenchWithLocalCaches,
    applyLifecycleResponseSnapshot,
    applyLocalPreviewWithLocalCaches,
    applyLuaSkillsCatalogUpdate,
    applyAgentSkillsCatalogUpdate,
    applySubagentCatalogUpdate,
    projectSettingsRevisionById,
    globalProjectSettingsRevision,
  };
}

// WorkbenchServiceSyncInput carries the service signals and the local state they update.
// WorkbenchServiceSyncInput 携带服务信号及其更新的本地状态。
export interface WorkbenchServiceSyncInput {
  // Local service generation used to reset revision ordering after a reconnect.
  // 用于在重连后重置版本排序的本地服务代次。
  serviceInstanceEpoch: number;
  // Latest full workbench snapshot from the backend.
  // 后端返回的最近完整工作台快照。
  workbench: WorkbenchSnapshot;
  // Latest partial workbench update signal from the backend.
  // 后端返回的最近局部工作台更新信号。
  workbenchDelta: WorkbenchDeltaSignal | null;
  // Acknowledges one exact partial signal after every envelope has been considered.
  // 在每条增量都完成处理后确认一个精确的局部信号。
  onWorkbenchDeltaConsumed: (sequence: number) => void;
  // Setter for the rendered workbench state.
  // 已渲染工作台状态的设置器。
  setWorkbenchState: React.Dispatch<React.SetStateAction<WorkbenchSnapshot>>;
  // Setter for the selected durable or draft conversation.
  // 选中的持久化或草稿会话的设置器。
  setSelectedConversationId: React.Dispatch<React.SetStateAction<string | null>>;
  // Setter for active media upload progress rows.
  // 进行中媒体上传进度行的设置器。
  setMediaUploadProgresses: React.Dispatch<React.SetStateAction<Record<string, MediaUploadProgressChangedDelta>>>;
  // Local draft that a service snapshot must not replace as the selection.
  // 服务快照不得替换为选中项的本地草稿。
  draftConversationIdRef: React.MutableRefObject<string | null>;
  // Conversation materialized from a draft whose sidebar row has not arrived yet.
  // 已由草稿物化、但侧栏行尚未到达的会话。
  pendingMaterializedConversationIdRef: React.MutableRefObject<string | null>;
  // Whether the user has edited project expansion locally in this session.
  // 本轮会话中用户是否已在本地修改项目展开状态。
  expandedProjectIdsLocallyEditedRef: React.MutableRefObject<boolean>;
  // Translator for upload failure notices.
  // 上传失败通知的翻译函数。
  t: (key: TranslationKey) => string;
}

/** Applies full service snapshots and delta batches through the revision gates.
 * 通过版本闸门应用完整服务快照与增量批次。
 * Parameters are the gates from useWorkbenchRevisionGates and the service signals; returns nothing.
 * 参数为 useWorkbenchRevisionGates 返回的闸门与服务信号；无返回值。 */
export function useWorkbenchServiceSync(gates: WorkbenchRevisionGates, props: WorkbenchServiceSyncInput) {
  const {
    acceptedWorkbenchRevisionRef,
    acceptedLuaSkillsRevisionRef,
    acceptedAgentSkillsRevisionRef,
    acceptedSubagentCatalogRevisionRef,
    acceptedProjectSettingsRevisionByKeyRef,
    acceptedGlobalProjectSettingsRevisionByDomainRef,
    resetAcceptedWorkbenchOrderingForServiceInstance,
    acceptServiceWorkbenchSnapshot,
    acceptConversationArchiveTransitions,
    acceptProjectRemovalTransition,
    setProjectSettingsRevisionById,
    setGlobalProjectSettingsRevision,
  } = gates;
  const {
    setWorkbenchState,
    setSelectedConversationId,
    setMediaUploadProgresses,
    draftConversationIdRef,
    pendingMaterializedConversationIdRef,
    expandedProjectIdsLocallyEditedRef,
    t,
  } = props;
  React.useEffect(() => {
    const serviceInstanceChanged = resetAcceptedWorkbenchOrderingForServiceInstance(
      props.serviceInstanceEpoch,
    );
    if (!acceptServiceWorkbenchSnapshot(props.workbench)) {
      return;
    }
    // acceptedGlobalSettings keeps a later delta authoritative if it arrived before this snapshot effect ran.
    // acceptedGlobalSettings 在较新增量先于当前快照副作用到达时继续保持其权威性。
    const acceptedGlobalSettings = {
      vmm: Math.max(
        acceptedGlobalProjectSettingsRevisionByDomainRef.current.vmm,
        props.workbench.projectSettingsRevisions.global.vmm,
      ),
      agentSkills: Math.max(
        acceptedGlobalProjectSettingsRevisionByDomainRef.current.agentSkills,
        props.workbench.projectSettingsRevisions.global.agentSkills,
      ),
    };
    acceptedGlobalProjectSettingsRevisionByDomainRef.current = acceptedGlobalSettings;
    setGlobalProjectSettingsRevision(acceptedGlobalSettings);
    // acceptedProjectSettings applies every full-snapshot domain revision without lowering a newer focused delta.
    // acceptedProjectSettings 应用全量快照中的每个领域版本，同时不降低较新的聚焦增量。
    const acceptedProjectSettings: Record<string, ProjectSettingsRevision> = {};
    for (const [projectId, revisions] of Object.entries(props.workbench.projectSettingsRevisions.byProject)) {
      // acceptedRevisions merges two independently ordered domains for this exact project.
      // acceptedRevisions 合并当前准确项目的两个独立排序领域。
      const acceptedRevisions = {
        vmm: Math.max(
          acceptedProjectSettingsRevisionByKeyRef.current.get(`${projectId}:vmm`) ?? 0,
          revisions.vmm,
        ),
        agentSkills: Math.max(
          acceptedProjectSettingsRevisionByKeyRef.current.get(`${projectId}:agentSkills`) ?? 0,
          revisions.agentSkills,
        ),
      };
      acceptedProjectSettingsRevisionByKeyRef.current.set(`${projectId}:vmm`, acceptedRevisions.vmm);
      acceptedProjectSettingsRevisionByKeyRef.current.set(
        `${projectId}:agentSkills`,
        acceptedRevisions.agentSkills,
      );
      acceptedProjectSettings[projectId] = acceptedRevisions;
    }
    setProjectSettingsRevisionById((current) => (
      serviceInstanceChanged
        ? acceptedProjectSettings
        : { ...current, ...acceptedProjectSettings }
    ));
    setWorkbenchState((current) => (
      mergeWorkbenchSnapshot(
        current,
        props.workbench,
        {
          allowServiceRevisionReset: serviceInstanceChanged,
          selectedConversationSource: serviceInstanceChanged ? "incoming" : undefined,
          preserveExpandedProjectIds: expandedProjectIdsLocallyEditedRef.current,
        },
      )
    ));
    const nextSelected = props.workbench.selectedConversationId ?? null;
    setSelectedConversationId((current) => {
      if (current && current === draftConversationIdRef.current) {
        return current;
      }
      if (current && current === pendingMaterializedConversationIdRef.current) {
        return current;
      }
      if (!serviceInstanceChanged && current && props.workbench.conversations.some((conversation) => conversation.id === current)) {
        return current;
      }
      return nextSelected;
    });
  }, [
    acceptServiceWorkbenchSnapshot,
    props.serviceInstanceEpoch,
    props.workbench,
    resetAcceptedWorkbenchOrderingForServiceInstance,
  ]);

  React.useEffect(() => {
    if (!props.workbenchDelta) {
      return;
    }
    // signalSequence binds acknowledgement to the exact batch observed by this effect.
    // signalSequence 将确认操作绑定到当前 effect 观察到的精确批次。
    const signalSequence = props.workbenchDelta.seq;
    const acceptedEnvelopes: WorkbenchDeltaEnvelope[] = [];
    // acceptedProjectSettingsRevisions collects target-and-domain invalidations committed by this event batch.
    // acceptedProjectSettingsRevisions 收集当前事件批次提交的目标与领域失效版本。
    const acceptedProjectSettingsRevisions = new Map<string, ProjectSettingsRevision>();
    let nextAcceptedRevision = acceptedWorkbenchRevisionRef.current;
    for (const envelope of props.workbenchDelta.envelopes) {
      if (envelope.delta.kind === "mediaUploadProgressChanged") {
        // Upload progress still participates in global ordering so a terminal event cannot be overtaken by an older snapshot.
        // 上传进度仍参与全局排序，避免旧快照覆盖上传终态。
        if (!shouldAcceptWorkbenchRevision(nextAcceptedRevision, envelope.generation)) {
          continue;
        }
        const progress = envelope.delta.payload;
        setMediaUploadProgresses((current) => reduceMediaUploadProgress(current, progress));
        // Accepted failed conversation uploads close the progress row but retain a visible attachment notice.
        // 已接纳的会话上传失败会关闭进度行，同时保留可见附件通知。
        if (isConversationUploadFailure(progress)) {
          toast.error(t("conversation.submission.uploadFailed"), {
            id: `media-upload-failure:${mediaUploadProgressKey(progress)}`,
            description: `${progress.fileName}\n${t("conversation.submission.uploadFailedAttachmentKept")}${progress.error ? `\n${progress.error}` : ""}`,
          });
        }
        acceptedEnvelopes.push(envelope);
        nextAcceptedRevision = envelope.generation;
        continue;
      }
      if (envelope.delta.kind === "conversationArchived") {
        // acceptedArchives filters this single target through its own lifecycle revision.
        // acceptedArchives 通过目标自身生命周期版本过滤此单个目标。
        const acceptedArchives = acceptConversationArchiveTransitions(
          [envelope.delta.payload],
          envelope.generation,
        );
        if (acceptedArchives.length > 0) {
          acceptedEnvelopes.push(envelope);
          nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        }
        continue;
      }
      if (envelope.delta.kind === "conversationsArchived") {
        // acceptedArchives independently filters every child target in the shared batch generation.
        // acceptedArchives 在共享批次 generation 中独立过滤每个子目标。
        const acceptedArchives = acceptConversationArchiveTransitions(
          envelope.delta.payload.archivedConversations,
          envelope.generation,
        );
        if (acceptedArchives.length > 0) {
          acceptedEnvelopes.push({
            ...envelope,
            delta: {
              kind: "conversationsArchived",
              payload: { archivedConversations: acceptedArchives },
            },
          });
          nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        }
        continue;
      }
      if (envelope.delta.kind === "projectRemoved") {
        // acceptedArchives filters child lifecycle targets independently from the project target.
        // acceptedArchives 独立于项目目标过滤子生命周期目标。
        const acceptedArchives = acceptConversationArchiveTransitions(
          envelope.delta.payload.archivedConversations,
          envelope.generation,
        );
        // projectAccepted orders removal only against the exact project lifecycle target.
        // projectAccepted 仅针对精确项目生命周期目标排序移除操作。
        const projectAccepted = acceptProjectRemovalTransition(
          envelope.delta.payload.projectId,
          envelope.generation,
        );
        if (projectAccepted) {
          acceptedEnvelopes.push({
            ...envelope,
            delta: {
              kind: "projectRemoved",
              payload: {
                ...envelope.delta.payload,
                archivedConversations: acceptedArchives,
              },
            },
          });
        } else if (acceptedArchives.length > 0) {
          acceptedEnvelopes.push({
            generation: envelope.generation,
            delta: {
              kind: "conversationsArchived",
              payload: { archivedConversations: acceptedArchives },
            },
          });
        }
        if (projectAccepted || acceptedArchives.length > 0) {
          nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        }
        continue;
      }
      if (envelope.delta.kind === "pluginCatalogChanged") {
        // One plugin catalog event owns all three revision gates so its domains are never applied partially.
        // 单个插件目录事件同时拥有三个版本闸门，确保其领域绝不被部分应用。
        const acceptedPluginCatalogRevision = Math.max(
          acceptedLuaSkillsRevisionRef.current,
          acceptedAgentSkillsRevisionRef.current,
          acceptedSubagentCatalogRevisionRef.current,
        );
        if (!shouldAcceptWorkbenchRevision(acceptedPluginCatalogRevision, envelope.generation)) {
          continue;
        }
        acceptedLuaSkillsRevisionRef.current = envelope.generation;
        acceptedAgentSkillsRevisionRef.current = envelope.generation;
        acceptedSubagentCatalogRevisionRef.current = envelope.generation;
        acceptedEnvelopes.push(envelope);
        nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        continue;
      }
      if (envelope.delta.kind === "agentSkillsCatalogChanged") {
        // Agent Skills uses its own revision so an unrelated newer domain cannot suppress this catalog.
        // Agent Skills 使用独立版本，避免无关领域的较新版本压掉当前目录。
        if (!shouldAcceptWorkbenchRevision(acceptedAgentSkillsRevisionRef.current, envelope.generation)) {
          continue;
        }
        acceptedAgentSkillsRevisionRef.current = envelope.generation;
        acceptedEnvelopes.push(envelope);
        nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        continue;
      }
      if (
        envelope.delta.kind === "luaSkillsCatalogChanged"
        || envelope.delta.kind === "luaSkillsManagedRuntimeProgressChanged"
        || envelope.delta.kind === "luaSkillsOperationProgressChanged"
      ) {
        // Every LuaSkills projection shares one domain gate so unrelated newer deltas cannot hide progress.
        // 所有 LuaSkills 投影共享同一领域闸门，避免无关的较新增量隐藏进度。
        if (!shouldAcceptWorkbenchRevision(acceptedLuaSkillsRevisionRef.current, envelope.generation)) {
          continue;
        }
        acceptedLuaSkillsRevisionRef.current = envelope.generation;
        acceptedEnvelopes.push(envelope);
        nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        continue;
      }
      if (envelope.delta.kind === "subagentCatalogChanged") {
        // Subagent catalog uses one revision gate for broadcast events and initiating RPC responses.
        // 智能体目录让广播事件与发起方 RPC 响应共享同一版本闸门。
        if (!shouldAcceptWorkbenchRevision(acceptedSubagentCatalogRevisionRef.current, envelope.generation)) {
          continue;
        }
        acceptedSubagentCatalogRevisionRef.current = envelope.generation;
        acceptedEnvelopes.push(envelope);
        nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        continue;
      }
      if (envelope.delta.kind === "projectSettingsChanged") {
        // projectId and domain own independent gates so unrelated settings never suppress each other.
        // projectId 与 domain 持有独立闸门，避免无关设置相互压制。
        const projectId = envelope.delta.payload.projectId;
        const domain = envelope.delta.payload.domain;
        if (projectId === null) {
          const acceptedRevision = acceptedGlobalProjectSettingsRevisionByDomainRef.current[domain];
          if (!shouldAcceptWorkbenchRevision(acceptedRevision, envelope.generation)) {
            continue;
          }
          acceptedGlobalProjectSettingsRevisionByDomainRef.current = {
            ...acceptedGlobalProjectSettingsRevisionByDomainRef.current,
            [domain]: envelope.generation,
          };
          setGlobalProjectSettingsRevision((current) => ({
            ...current,
            [domain]: envelope.generation,
          }));
          acceptedEnvelopes.push(envelope);
          nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
          continue;
        }
        const settingsKey = `${projectId}:${domain}`;
        const acceptedRevision = acceptedProjectSettingsRevisionByKeyRef.current.get(settingsKey) ?? 0;
        if (!shouldAcceptWorkbenchRevision(acceptedRevision, envelope.generation)) {
          continue;
        }
        acceptedProjectSettingsRevisionByKeyRef.current.set(settingsKey, envelope.generation);
        const acceptedDomains = acceptedProjectSettingsRevisions.get(projectId)
          ?? {
            vmm: acceptedProjectSettingsRevisionByKeyRef.current.get(`${projectId}:vmm`) ?? 0,
            agentSkills: acceptedProjectSettingsRevisionByKeyRef.current.get(`${projectId}:agentSkills`) ?? 0,
          };
        acceptedProjectSettingsRevisions.set(projectId, {
          ...acceptedDomains,
          [domain]: envelope.generation,
        });
        acceptedEnvelopes.push(envelope);
        nextAcceptedRevision = Math.max(nextAcceptedRevision, envelope.generation);
        continue;
      }
      if (!shouldAcceptWorkbenchRevision(nextAcceptedRevision, envelope.generation)) {
        continue;
      }
      acceptedEnvelopes.push(envelope);
      nextAcceptedRevision = envelope.generation;
    }
    if (acceptedProjectSettingsRevisions.size > 0) {
      setProjectSettingsRevisionById((current) => {
        // next exposes every independently accepted project revision in one React commit.
        // next 在一次 React 提交中暴露所有独立接收的项目版本。
        const next = { ...current };
        for (const [projectId, revisions] of acceptedProjectSettingsRevisions) {
          next[projectId] = revisions;
        }
        return next;
      });
    }
    if (acceptedEnvelopes.length === 0) {
      props.onWorkbenchDeltaConsumed(signalSequence);
      return;
    }
    acceptedWorkbenchRevisionRef.current = nextAcceptedRevision;
    setWorkbenchState((current) => (
      acceptedEnvelopes.reduce(
        (nextWorkbench, envelope) => applyWorkbenchDelta(nextWorkbench, envelope),
        current,
      )
    ));
    props.onWorkbenchDeltaConsumed(signalSequence);
  }, [
    acceptConversationArchiveTransitions,
    acceptProjectRemovalTransition,
    props.onWorkbenchDeltaConsumed,
    props.workbenchDelta,
    t,
  ]);
}
