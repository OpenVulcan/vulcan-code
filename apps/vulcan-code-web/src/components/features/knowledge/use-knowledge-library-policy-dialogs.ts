import * as React from "react";

import type { useI18n } from "@/i18n";
import { createKnowledgeLibrary, updateKnowledgeLibrarySettings } from "@/service/knowledge-base-service";
import { loadPublicAiSettings } from "@/service/settings-actions-service";
import type { KnowledgeLibrarySummary, KnowledgeRetrievalMode } from "@/types";
import type { PublicAiInternalInferenceCandidate } from "@/types/public-ai";
import type { KnowledgeBusyOperation } from "./knowledge-area-types";
import type { KnowledgeLibraryCreateDialogModel, KnowledgeLibraryCreateDialogProps } from "./knowledge-library-create-dialog";

// knowledgeCandidateRouteKey derives the same immutable route key as the creation dialog.
// knowledgeCandidateRouteKey 根据不可变路由生成与创建对话框一致的键。
function knowledgeCandidateRouteKey(candidate: PublicAiInternalInferenceCandidate): string {
  const route = candidate.route;
  return [route.provider_id, route.service_entry_id, route.connection_id, route.model_id, route.operation, route.binding.kind, route.binding.kind === "protocol" ? route.binding.protocol_profile_id : route.binding.action_binding_id].join("/");
}

// knowledgeRouteFromCandidate converts one official candidate into the persisted background model route.
// knowledgeRouteFromCandidate 将一个官方候选转换为持久化的后台模型路由。
// Parameters: candidate is the selected route or null. Returns: the persisted route or null when unset.
// 参数：candidate 是所选路由或空。返回值：持久化路由；未选择时为空。
function knowledgeRouteFromCandidate(candidate: PublicAiInternalInferenceCandidate | null) {
  return candidate ? ({
    consumerProfileId: "knowledge_base",
    providerId: candidate.route.provider_id,
    protocol: candidate.route.binding.kind === "protocol" ? candidate.route.binding.protocol_profile_id : candidate.route.binding.action_binding_id,
    bindingKind: candidate.route.binding.kind,
    serviceEntryId: candidate.route.service_entry_id,
    modelId: candidate.route.model_id,
    usageScope: "internal_background" as const,
    connectionId: candidate.route.connection_id,
  }) : null;
}

// KnowledgeLibraryPolicyDialogsOptions supplies the page-owned selection, mutation slot and error surface.
// KnowledgeLibraryPolicyDialogsOptions 提供页面持有的选择、变更槽位与错误展示。
export interface KnowledgeLibraryPolicyDialogsOptions {
  // Library whose persisted policy the settings dialog edits.
  // 设置对话框编辑其持久化策略的知识库。
  selectedLibrary: KnowledgeLibrarySummary | null;
  // Claims the single page mutation slot; returns false when another operation owns it.
  // 占用唯一页面变更槽位；被其他操作占用时返回 false。
  acquireBusy: (operation: KnowledgeBusyOperation) => boolean;
  // Releases the mutation slot only for its exact owner.
  // 仅为精确所有者释放变更槽位。
  releaseBusy: (operation: KnowledgeBusyOperation) => void;
  // Serializes one registry mutation and refreshes the registry afterwards.
  // 串行执行一次注册表变更并在之后刷新注册表。
  runMutation: (operation: KnowledgeBusyOperation, label: string, action: () => Promise<string | void>) => Promise<void>;
  // Reloads the authoritative library registry.
  // 重新加载权威知识库注册表。
  refreshLibraries: () => Promise<void>;
  // Shows a non-job failure in the page's in-window message dialog.
  // 在页面窗口内消息弹窗中显示非任务失败。
  setActionError: (message: string | null) => void;
  // Selects the newly created library.
  // 选中新创建的知识库。
  setSelectedLibraryId: (libraryId: string | null) => void;
  // Localized text resolver owned by the page.
  // 页面持有的本地化文本解析器。
  t: ReturnType<typeof useI18n>["t"];
}

// useKnowledgeLibraryPolicyDialogs owns the policy draft shared by the create and settings dialogs.
// useKnowledgeLibraryPolicyDialogs 持有创建与设置对话框共用的策略草稿。
// Parameters: options supplies page-owned state and mutations. Returns: dialog visibility, shared fields and actions.
// 参数：options 提供页面持有的状态与变更。返回值：对话框可见性、共享字段与动作。
export function useKnowledgeLibraryPolicyDialogs({
  selectedLibrary,
  acquireBusy,
  releaseBusy,
  runMutation,
  refreshLibraries,
  setActionError,
  setSelectedLibraryId,
  t,
}: KnowledgeLibraryPolicyDialogsOptions) {
  // createName stores the new-library name field.
  // createName 保存新知识库名称字段。
  const [createName, setCreateName] = React.useState("");
  // createDialogOpen controls the creation-time policy wizard.
  // createDialogOpen 控制创建阶段的策略向导。
  const [createDialogOpen, setCreateDialogOpen] = React.useState(false);
  // createRetrievalMode stores the selected corpus policy before submission.
  // createRetrievalMode 保存提交前选择的内容层策略。
  const [createRetrievalMode, setCreateRetrievalMode] = React.useState<KnowledgeRetrievalMode>("raw_bm25");
  // createInferenceCandidates mirrors the official capability snapshot used by the wizard.
  // createInferenceCandidates 镜像向导使用的官方能力快照候选。
  const [createInferenceCandidates, setCreateInferenceCandidates] = React.useState<PublicAiInternalInferenceCandidate[]>([]);
  // createCurationCandidateKey identifies the selected LLM route.
  // createCurationCandidateKey 标识所选整理 LLM 路由。
  const [createCurationCandidateKey, setCreateCurationCandidateKey] = React.useState("");
  // createCurationReasoningEffort stores the selected official reasoning level.
  // createCurationReasoningEffort 保存官方声明的整理推理级别。
  const [createCurationReasoningEffort, setCreateCurationReasoningEffort] = React.useState("");
  // createEmbeddingCandidateKey identifies the selected embedding route.
  // createEmbeddingCandidateKey 标识所选向量路由。
  const [createEmbeddingCandidateKey, setCreateEmbeddingCandidateKey] = React.useState("");
  // createConfiguredDimension stores the selected capability dimension.
  // createConfiguredDimension 保存所选能力维度。
  const [createConfiguredDimension, setCreateConfiguredDimension] = React.useState("");
  // createRequestedBatchSize stores an optional user override.
  // createRequestedBatchSize 保存可选的用户批量覆盖值。
  const [createRequestedBatchSize, setCreateRequestedBatchSize] = React.useState("");
  // createCompatibilityOverride stores a deliberate same-dimension index reuse note.
  // createCompatibilityOverride 保存明确复用同维度索引的风险说明。
  const [createCompatibilityOverride, setCreateCompatibilityOverride] = React.useState("");
  // settingsDialogOpen controls editing of the selected library policy.
  // settingsDialogOpen 控制当前资料库策略编辑对话框。
  const [settingsDialogOpen, setSettingsDialogOpen] = React.useState(false);
  // editingLibraryId freezes the exact library whose policy the user opened, across background registry updates.
  // editingLibraryId 在后台注册表更新期间冻结用户打开策略的精确知识库。
  const [editingLibraryId, setEditingLibraryId] = React.useState<string | null>(null);

  // openCreateDialog loads the current official model capability snapshot before showing choices.
  // openCreateDialog 在显示选项前加载当前官方模型能力快照。
  const openCreateDialog = React.useCallback(async () => {
    if (!acquireBusy("create")) return;
    try {
      const snapshot = await loadPublicAiSettings();
      setCreateInferenceCandidates(snapshot.internalInferenceCandidates);
      setCreateRetrievalMode("raw_bm25");
      setCreateCurationCandidateKey("");
      setCreateCurationReasoningEffort("");
      setCreateEmbeddingCandidateKey("");
      setCreateConfiguredDimension("");
      setCreateRequestedBatchSize("");
      setCreateCompatibilityOverride("");
      setCreateDialogOpen(true);
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      releaseBusy("create");
    }
  }, [acquireBusy, releaseBusy]);

  // openSettingsDialog loads fresh official candidates and mirrors the persisted library choices.
  // openSettingsDialog 加载最新官方候选并镜像持久化的资料库选择。
  const openSettingsDialog = React.useCallback(async () => {
    if (!selectedLibrary || !acquireBusy("settings")) return;
    try {
      const snapshot = await loadPublicAiSettings();
      const settings = selectedLibrary.settings;
      setCreateInferenceCandidates(snapshot.internalInferenceCandidates);
      setCreateName(selectedLibrary.name);
      setCreateRetrievalMode(settings.retrievalMode);
      const routeKeyForSettings = (route: { providerId: string; serviceEntryId: string; connectionId?: string | null; modelId: string; protocol: string; bindingKind?: "protocol" | "action" } | null | undefined, operation: string) => route ? snapshot.internalInferenceCandidates.find((candidate) => candidate.route.operation === operation && candidate.route.provider_id === route.providerId && candidate.route.service_entry_id === route.serviceEntryId && candidate.route.connection_id === route.connectionId && candidate.route.model_id === route.modelId && candidate.route.binding.kind === (route.bindingKind ?? "protocol") && (candidate.route.binding.kind === "protocol" ? candidate.route.binding.protocol_profile_id : candidate.route.binding.action_binding_id) === route.protocol) : null;
      const curationCandidate = routeKeyForSettings(settings.curation.route, "conversation_respond");
      const embeddingCandidate = routeKeyForSettings(settings.embedding.route, "embedding_create");
      setCreateCurationCandidateKey(curationCandidate ? knowledgeCandidateRouteKey(curationCandidate) : "");
      setCreateCurationReasoningEffort(settings.curation.reasoningEffort ?? "");
      setCreateEmbeddingCandidateKey(embeddingCandidate ? knowledgeCandidateRouteKey(embeddingCandidate) : "");
      setCreateConfiguredDimension(settings.embedding.configuredDimension ? String(settings.embedding.configuredDimension) : "");
      setCreateRequestedBatchSize(settings.embedding.embeddingBatchSize ? String(settings.embedding.embeddingBatchSize) : "");
      setCreateCompatibilityOverride(settings.vectorIndex.compatibilityOverride ?? "");
      setEditingLibraryId(selectedLibrary.id);
      setSettingsDialogOpen(true);
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      releaseBusy("settings");
    }
  }, [acquireBusy, releaseBusy, selectedLibrary]);

  // createLibrary validates and submits one named library with the selected model policy.
  // createLibrary 校验并提交一个带模型策略的命名知识库。
  const createLibrary = React.useCallback(async (model: KnowledgeLibraryCreateDialogModel) => {
    const name = createName.trim();
    if (!name) {
      setActionError(t("knowledge.nameRequired"));
      return;
    }
    const embeddingCapability = model.embeddingCandidate?.capabilities.embedding;
    await runMutation("create", t("knowledge.create"), async () => {
      const created = await createKnowledgeLibrary(name, "", [], {
        retrievalMode: model.retrievalMode,
        fallbackPolicy: "explicit_only",
        curation: {
          enabled: model.retrievalMode !== "raw_bm25",
          route: knowledgeRouteFromCandidate(model.curationCandidate),
          promptVersion: "knowledge-display-v1",
          reasoningEffort: model.reasoningEffort || null,
        },
        embedding: {
          enabled: model.retrievalMode === "display_hybrid",
          route: knowledgeRouteFromCandidate(model.embeddingCandidate),
          configuredDimension: model.configuredDimension,
          modelMaxBatchSize: embeddingCapability?.maximum_inputs ?? null,
          requestedBatchSize: model.requestedBatchSize,
          compatibilityOverride: model.compatibilityOverride.trim() || null,
        },
      });
      setCreateName("");
      setSelectedLibraryId(created.id);
      setCreateDialogOpen(false);
    });
  }, [createName, runMutation, t]);

  // updateLibrarySettings persists an edited policy without re-importing source files.
  // updateLibrarySettings 持久化编辑后的策略且不重新导入原始文件。
  const updateLibrarySettings = React.useCallback(async (model: KnowledgeLibraryCreateDialogModel) => {
    if (!editingLibraryId) return;
    const embeddingCapability = model.embeddingCandidate?.capabilities.embedding;
    await runMutation("settings", t("knowledge.experience.saveSettings"), async () => {
      await updateKnowledgeLibrarySettings(editingLibraryId, {
        retrievalMode: model.retrievalMode,
        fallbackPolicy: "explicit_only",
        curation: { enabled: model.retrievalMode !== "raw_bm25", route: knowledgeRouteFromCandidate(model.curationCandidate), promptVersion: "knowledge-display-v1", reasoningEffort: model.reasoningEffort || null },
        embedding: { enabled: model.retrievalMode === "display_hybrid", route: knowledgeRouteFromCandidate(model.embeddingCandidate), configuredDimension: model.configuredDimension, modelMaxBatchSize: embeddingCapability?.maximum_inputs ?? null, requestedBatchSize: model.requestedBatchSize, compatibilityOverride: model.compatibilityOverride.trim() || null },
      });
      setSettingsDialogOpen(false);
      await refreshLibraries();
    });
  }, [editingLibraryId, refreshLibraries, runMutation]);

  // policyDialogFields carries the draft fields both policy dialogs render identically.
  // policyDialogFields 承载两个策略对话框以相同方式渲染的草稿字段。
  const policyDialogFields = {
    candidates: createInferenceCandidates,
    configuredDimension: createConfiguredDimension,
    compatibilityOverride: createCompatibilityOverride,
    curationCandidateKey: createCurationCandidateKey,
    embeddingCandidateKey: createEmbeddingCandidateKey,
    reasoningEffort: createCurationReasoningEffort,
    name: createName,
    onConfiguredDimensionChange: setCreateConfiguredDimension,
    onCompatibilityOverrideChange: setCreateCompatibilityOverride,
    onCurationCandidateChange: setCreateCurationCandidateKey,
    onReasoningEffortChange: setCreateCurationReasoningEffort,
    onEmbeddingCandidateChange: setCreateEmbeddingCandidateKey,
    onNameChange: setCreateName,
    onRequestedBatchSizeChange: setCreateRequestedBatchSize,
    onRetrievalModeChange: setCreateRetrievalMode,
    requestedBatchSize: createRequestedBatchSize,
    retrievalMode: createRetrievalMode,
  } satisfies Partial<KnowledgeLibraryCreateDialogProps>;
  return {
    createName,
    setCreateName,
    createDialogOpen,
    setCreateDialogOpen,
    settingsDialogOpen,
    setSettingsDialogOpen,
    policyDialogFields,
    openCreateDialog,
    openSettingsDialog,
    createLibrary,
    updateLibrarySettings,
  };
}
