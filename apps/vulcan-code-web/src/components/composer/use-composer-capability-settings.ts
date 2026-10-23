import React from "react";
import type { ComposerCapabilitySummaryItem } from "@/components/composer/composer-capability-summary";
import type { ComposerProviderToolSettingsSnapshot } from "@/components/composer/composer-model";
import { useI18n } from "@/i18n";
import {
  isDirectProviderToolCapability,
  isModelExtensionCapability,
  modelCapabilitiesSupportMediaUnderstanding,
} from "@/lib/provider-tool-capabilities";
import { mediaReplacementReasoningOverrides } from "@/lib/reasoning-effort-labels";
import {
  loadPublicAiSessionProviderToolSelections,
  loadPublicAiSettings,
  rebuildPublicAiSessionProviderToolSelections,
  savePublicAiSessionProviderToolSelections,
} from "@/service/settings-actions-service";
import type {
  PublicAiMediaUnderstandingBackendMode,
  PublicAiMediaUnderstandingBackendSelection,
  PublicAiProviderToolCapabilityKind,
  PublicAiSettingsSnapshot,
} from "@/types";
import { errorMessage } from "@/lib/display-format";
import {
  type PublicAiMediaUnderstandingCapabilityKind,
  composerCapabilityIsEnabled,
  composerCapabilitySummaryDetail,
  composerExtensionSelections,
  composerProviderToolCapabilities,
  defaultComposerProviderToolSettings,
  isMediaUnderstandingCapability,
  mediaUnderstandingModelKey,
  normalizedComposerProviderToolSettings,
  orderedProviderToolCapabilities,
  providerToolCapabilityColor,
  providerToolCapabilityLabel,
  providerToolModelKey,
  providerToolSettingsConflicts,
  providerToolSettingsSignature,
  uniqueMediaUnderstandingModels,
  uniqueProviderToolModels,
} from "@/components/composer/composer-capability-model";

// ComposerCapabilitySettingsOptions identifies the session settings owner and the dialog visibility it follows.
// ComposerCapabilitySettingsOptions 标识会话设置归属及其跟随的对话框可见性。
export interface ComposerCapabilitySettingsOptions {
  // Current persisted conversation, or null for a new conversation draft.
  // 当前持久化会话；新会话草稿时为空。
  conversationId: string | null;
  // Optional complete draft snapshot captured before the conversation exists.
  // 会话创建前捕获的可选完整草稿快照。
  draftSettings?: ComposerProviderToolSettingsSnapshot;
  // Saves one complete draft snapshot without calling a session RPC.
  // 在不调用会话 RPC 的情况下保存完整草稿快照。
  onDraftSettingsChange?: (settings: ComposerProviderToolSettingsSnapshot) => void;
  // Reports whether unresolved session choices must block sending.
  // 报告未解决的会话选择是否必须阻止发送。
  onConflictChange?: (blocked: boolean) => void;
  // Whether the editor dialog is open; each open re-reads the authoritative settings.
  // 编辑对话框是否打开；每次打开都会重新读取权威设置。
  open: boolean;
  // Closes the editor dialog after a successful save.
  // 保存成功后关闭编辑对话框。
  setOpen: (open: boolean) => void;
}

// useComposerCapabilitySettings owns the session capability draft, its authoritative loads and guarded saves.
// useComposerCapabilitySettings 持有会话能力草稿及其权威读取与受保护保存。
// Parameters: props identifies the settings owner and dialog visibility; returns the draft state, derived routes and actions.
// 参数：props 标识设置归属与对话框可见性；返回草稿状态、派生路由与操作。
export function useComposerCapabilitySettings(props: ComposerCapabilitySettingsOptions) {
  const { t } = useI18n();
  const { open, setOpen } = props;
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [selectedCapability, setSelectedCapability] =
    React.useState<PublicAiProviderToolCapabilityKind>("web_search");
  const [draft, setDraft] = React.useState<ComposerProviderToolSettingsSnapshot>({
    enabledCapabilities: [],
    selections: {},
    extensionTargetSets: {},
    extensionModes: {},
    mediaUnderstandingSelections: {},
  });
  const [baselineSignature, setBaselineSignature] = React.useState("{}");
  // requestsStarted means the session already sent a request, so a saved tool change breaks context.
  // requestsStarted 表示会话已发送过请求，保存的工具修改会打破上下文。
  const [requestsStarted, setRequestsStarted] = React.useState(false);
  // contextBreakPending holds a save until the user confirms breaking the conversation context.
  // contextBreakPending 在用户确认打破会话上下文之前暂缓保存。
  const [contextBreakPending, setContextBreakPending] = React.useState(false);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  // loadState binds availability to its owning conversation; stale settings must never be saved to another session.
  // loadState 将可用性绑定到所属会话；旧设置绝不能保存到另一个会话。
  const [loadState, setLoadState] = React.useState<{
    conversationId: string | null;
    status: "loading" | "ready" | "failed";
  }>({ conversationId: props.conversationId, status: "loading" });
  // reloadVersion retries the same authoritative reads without replacing session settings with empty defaults.
  // reloadVersion 重试相同的权威读取，不用空默认值替换会话设置。
  const [reloadVersion, setReloadVersion] = React.useState(0);
  // saveGeneration rejects late save responses after the settings owner or load generation changes.
  // saveGeneration 拒绝设置归属或加载代次变化后迟到的保存响应。
  const saveGeneration = React.useRef(0);
  // mutationPending closes the same-frame duplicate window before React renders saving state.
  // mutationPending 在 React 渲染保存状态前关闭同帧重复操作窗口。
  const mutationPending = React.useRef(false);
  // mediaEditModes owns empty UI modes without fabricating a complete persisted route.
  // mediaEditModes 持有空的界面模式，不伪造完整持久化路由。
  const [mediaEditModes, setMediaEditModes] = React.useState<Partial<Record<PublicAiMediaUnderstandingCapabilityKind, PublicAiMediaUnderstandingBackendMode>>>({});
  // loadStatus invalidates the previous owner synchronously, before the next effect can start.
  // loadStatus 在下一次 effect 启动前同步使上一个会话的状态失效。
  const loadStatus = loadState.conversationId === props.conversationId ? loadState.status : "loading";
  // settingsReady is required both by the UI and the save callback to prevent destructive empty writes after read errors.
  // settingsReady 同时约束界面与保存回调，防止读取失败后用空配置覆盖原设置。
  const settingsReady = loadStatus === "ready";
  React.useEffect(() => {
    let active = true;
    saveGeneration.current += 1;
    mutationPending.current = false;
    setMediaEditModes({});
    setSaving(false);
    setContextBreakPending(false);
    setLoadState({ conversationId: props.conversationId, status: "loading" });
    setError(null);
    void Promise.all([
      loadPublicAiSettings(),
      props.conversationId
        ? loadPublicAiSessionProviderToolSelections(props.conversationId)
        : Promise.resolve(null),
    ])
      .then(([nextSnapshot, sessionSettings]) => {
        if (!active) {
          return;
        }
        const nextDraft = sessionSettings
          ? {
              enabledCapabilities: sessionSettings.enabledCapabilities,
              selections: sessionSettings.selections,
              extensionTargetSets: sessionSettings.extensionTargetSets,
              extensionModes: sessionSettings.extensionModes,
              mediaUnderstandingSelections: sessionSettings.mediaUnderstandingSelections,
            }
          : props.draftSettings
            ? normalizedComposerProviderToolSettings(props.draftSettings)
            : defaultComposerProviderToolSettings(nextSnapshot);
        const capabilities = composerProviderToolCapabilities(nextSnapshot, nextDraft);
        // Prefer a configured family initially; fixed empty media categories remain selectable when none is configured.
        // 初次优先展示已有配置的能力族；无配置时仍可选中固定空媒体分类。
        const initialCapability = orderedProviderToolCapabilities(nextSnapshot)[0] ?? capabilities[0];
        setSnapshot(nextSnapshot);
        setDraft(nextDraft);
        setRequestsStarted(sessionSettings?.requestsStarted ?? false);
        setBaselineSignature(providerToolSettingsSignature(nextDraft));
        if (!props.conversationId && !props.draftSettings) {
          props.onDraftSettingsChange?.(nextDraft);
        }
        setSelectedCapability((current) => (
          capabilities.includes(current) ? current : initialCapability
        ));
        setError(null);
        setLoadState({ conversationId: props.conversationId, status: "ready" });
      })
      .catch((reason) => {
        if (active) {
          setError(errorMessage(reason));
          setLoadState({ conversationId: props.conversationId, status: "failed" });
        }
      });
    return () => {
      active = false;
      saveGeneration.current += 1;
    };
  }, [open, props.conversationId, props.draftSettings, props.onDraftSettingsChange, reloadVersion]);

  const capabilities = settingsReady && snapshot
    ? composerProviderToolCapabilities(snapshot, draft)
    : [];
  const capabilityTargets = snapshot?.providerToolTargets.filter(
    (target) => target.target.contract.capability === selectedCapability
      && (!isModelExtensionCapability(selectedCapability)
        || target.manifestSourceLayer !== "custom"),
  ) ?? [];
  const selectedModels = uniqueProviderToolModels(capabilityTargets);
  // selectedExtensionTargets is the fixed model or the multi-model set of the selected capability.
  // selectedExtensionTargets 是当前选中能力的固定模型或多模型集合。
  const selectedExtensionTargets = composerExtensionSelections(draft, selectedCapability);
  // selectedExtensionSingle reports whether the selected capability uses one fixed model.
  // selectedExtensionSingle 报告当前选中能力是否使用一个固定模型。
  const selectedExtensionSingle = draft.extensionModes[selectedCapability]?.mode !== "multi";
  // unavailableExtensionTargets preserves exact saved identities that cannot appear in a current model card.
  // unavailableExtensionTargets 保留无法出现在当前模型卡片中的准确已存身份。
  const unavailableExtensionTargets = selectedExtensionTargets.filter((selection) => !capabilityTargets.some((target) => (
    target.connectionId === selection.connection_id && target.target.target_id === selection.target_id
  )));
  // updateMediaSelection changes one explicit route while requiring manual activation after an unavailable route.
  // updateMediaSelection 修改一条显式路由；原路由不可用时，要求用户重新手动开启。
  // Parameters: capability identifies the media facade; selection is the complete chosen route. Returns nothing.
  // 参数：capability 标识媒体能力；selection 是完整选定路由。无返回值。
  const updateMediaSelection = (
    capability: PublicAiProviderToolCapabilityKind,
    selection: PublicAiMediaUnderstandingBackendSelection,
  ): void => {
    if (!snapshot || !isMediaUnderstandingCapability(capability) || mutationPending.current) return;
    setDraft((current) => {
      // wasAvailable uses the same route and mode checks as the displayed switch, never its stale stored flag alone.
      // wasAvailable 使用与显示开关一致的路由和模式校验，绝不单独采用过期的已存标记。
      const wasAvailable = (mediaEditModes[capability] ?? current.mediaUnderstandingSelections[capability]?.mode)
        === current.mediaUnderstandingSelections[capability]?.mode
        && !providerToolSettingsConflicts(snapshot, { ...current, enabledCapabilities: [capability] })
          .some((conflict) => conflict.capability === capability);
      return {
        ...current,
        enabledCapabilities: wasAvailable ? current.enabledCapabilities
          : current.enabledCapabilities.filter((enabled) => enabled !== capability),
        mediaUnderstandingSelections: { ...current.mediaUnderstandingSelections, [capability]: selection },
      };
    });
  };
  const selectedSelection = draft.selections[selectedCapability];
  const selectedModel = selectedModels.find(
    (target) => target.target.target_id === selectedSelection?.target_id,
  ) ?? null;
  const selectedAccounts = selectedModel
    ? capabilityTargets.filter((target) => target.target.target_id === selectedModel.target.target_id)
    : [];
  const selectedAccount = selectedAccounts.find(
    (target) => target.connectionId === selectedSelection?.connection_id,
  ) ?? null;
  // selectedMediaTargets contains every exact provider-tool route for the selected media kind.
  // selectedMediaTargets 包含当前媒体类型的全部精确供应商工具路由。
  const selectedMediaTargets = isMediaUnderstandingCapability(selectedCapability)
    ? capabilityTargets
    : [];
  const selectedMediaProviderModels = uniqueProviderToolModels(selectedMediaTargets);
  // selectedMediaCandidates is independent from system defaults and global enable switches.
  // selectedMediaCandidates 与系统默认值及全局启用开关完全独立。
  const selectedMediaCandidates = snapshot && isMediaUnderstandingCapability(selectedCapability)
    ? (snapshot.mediaUnderstandingModelCandidates ?? []).filter((candidate) => (
      modelCapabilitiesSupportMediaUnderstanding(candidate.capabilities, selectedCapability)
    ))
    : [];
  const selectedMediaModelChoices = uniqueMediaUnderstandingModels(selectedMediaCandidates);
  // selectedMediaSelection is the complete route owned by this session.
  // selectedMediaSelection 是当前会话持有的完整路由。
  const selectedMediaSelection = isMediaUnderstandingCapability(selectedCapability)
    ? draft.mediaUnderstandingSelections[selectedCapability]
    : undefined;
  // selectedMediaMode remains readable even when the authoritative selection is absent.
  // selectedMediaMode 在权威选择缺失时仍保留可读的编辑模式。
  const selectedMediaMode = isMediaUnderstandingCapability(selectedCapability)
    ? mediaEditModes[selectedCapability] ?? selectedMediaSelection?.mode ?? "provider_tool"
    : "provider_tool";
  const selectedMediaProviderTarget = selectedMediaSelection?.mode === "provider_tool"
    ? selectedMediaTargets.find((target) => (
      target.connectionId === selectedMediaSelection.selection.connection_id
      && target.target.target_id === selectedMediaSelection.selection.target_id
    ))
    : undefined;
  const selectedMediaProviderModelKey = selectedMediaProviderTarget
    ? providerToolModelKey(selectedMediaProviderTarget)
    : "";
  const selectedMediaProviderAccounts = selectedMediaTargets.filter(
    (target) => providerToolModelKey(target) === selectedMediaProviderModelKey,
  );
  const selectedMediaModelCandidate = selectedMediaSelection?.mode === "model_replacement"
    ? selectedMediaCandidates.find((candidate) => (
      candidate.selection.provider_id === selectedMediaSelection.selection.provider_id
      && candidate.selection.model_id === selectedMediaSelection.selection.model_id
      && candidate.selection.connection_id === selectedMediaSelection.selection.connection_id
    ))
    : undefined;
  const selectedMediaModelKey = selectedMediaModelCandidate
    ? mediaUnderstandingModelKey(selectedMediaModelCandidate)
    : "";
  const selectedMediaModelAccounts = selectedMediaCandidates.filter(
    (candidate) => mediaUnderstandingModelKey(candidate) === selectedMediaModelKey,
  );
  const selectedMediaReasoningEfforts = selectedMediaModelCandidate
    ? mediaReplacementReasoningOverrides(selectedMediaModelCandidate.capabilities)
    : [];
  const localConflicts = snapshot ? providerToolSettingsConflicts(snapshot, draft) : [];
  const conflicts = localConflicts;
  // enabledCount combines direct enabled capabilities and non-empty extension sets.
  // enabledCount 合并直连已启用能力与非空扩展集合。
  const enabledCount = draft.enabledCapabilities.length + capabilities
    .filter((capability) => !isDirectProviderToolCapability(capability)
      && composerExtensionSelections(draft, capability).length > 0).length;
  const hasUnsavedChanges = settingsReady
    && providerToolSettingsSignature(draft) !== baselineSignature;
  // defaultSettings is what "restore defaults" writes into the draft; the action is idle when the draft already matches.
  // defaultSettings 是“还原默认”写入草稿的内容；草稿已与之一致时该操作不可用。
  const defaultSettings = settingsReady && snapshot ? defaultComposerProviderToolSettings(snapshot) : null;
  const draftMatchesDefaults = defaultSettings !== null
    && providerToolSettingsSignature(defaultSettings) === providerToolSettingsSignature(draft);
  // restoreDefaults replaces only the unsaved draft; the session changes when the user saves.
  // restoreDefaults 只替换未保存草稿；用户保存后会话才会改变。
  const restoreDefaults = (): void => {
    if (!defaultSettings || mutationPending.current) return;
    setDraft(defaultSettings);
    setMediaEditModes({});
    setError(null);
  };
  React.useEffect(() => {
    props.onConflictChange?.(!settingsReady || conflicts.length > 0);
  }, [settingsReady, conflicts.length, props.onConflictChange]);
  // save persists the draft; once requests were sent, the first call asks for confirmation and
  // `contextBreakConfirmed` carries it out.
  // save 持久化草稿；已发送过请求时，首次调用先请求确认，由 `contextBreakConfirmed` 执行。
  const save = React.useCallback(async (contextBreakConfirmed = false) => {
    if (!settingsReady || mutationPending.current || conflicts.length > 0) {
      return;
    }
    if (props.conversationId && requestsStarted && hasUnsavedChanges && !contextBreakConfirmed) {
      setContextBreakPending(true);
      return;
    }
    setContextBreakPending(false);
    if (!props.conversationId) {
      props.onDraftSettingsChange?.(draft);
      setBaselineSignature(providerToolSettingsSignature(draft));
      setOpen(false);
      return;
    }
    mutationPending.current = true;
    setSaving(true);
    // generation pins this response to the exact settings view that initiated the save.
    // generation 将响应固定到发起保存的精确设置视图。
    const generation = ++saveGeneration.current;
    try {
      const next = await savePublicAiSessionProviderToolSelections({
        conversationId: props.conversationId,
        enabledCapabilities: draft.enabledCapabilities,
        selections: draft.selections,
        extensionTargetSets: draft.extensionTargetSets,
        extensionModes: draft.extensionModes,
        mediaUnderstandingSelections: draft.mediaUnderstandingSelections,
      });
      if (generation !== saveGeneration.current) {
        return;
      }
      const nextDraft = {
        enabledCapabilities: next.enabledCapabilities,
        selections: next.selections,
        extensionTargetSets: next.extensionTargetSets,
        extensionModes: next.extensionModes,
        mediaUnderstandingSelections: next.mediaUnderstandingSelections,
      };
      setRequestsStarted(next.requestsStarted);
      setDraft(nextDraft);
      setBaselineSignature(providerToolSettingsSignature(nextDraft));
      setError(null);
      setOpen(false);
    } catch (reason) {
      if (generation === saveGeneration.current) {
        setError(errorMessage(reason));
      }
    } finally {
      if (generation === saveGeneration.current) {
        mutationPending.current = false;
        setSaving(false);
      }
    }
  }, [
    draft,
    settingsReady,
    conflicts.length,
    saving,
    requestsStarted,
    hasUnsavedChanges,
    props.conversationId,
    props.onDraftSettingsChange,
  ]);

  // Rebuild only the selected conversation's addon after an explicit click; ignore stale responses.
  // 仅在显式点击后重建当前会话附加项；忽略过期响应。
  // Returns after the write, then reloads authoritative settings without closing the recovery view.
  // 写入完成后返回，并重新加载权威设置，不关闭恢复视图。
  const rebuild = React.useCallback(async () => {
    if (!props.conversationId || mutationPending.current) {
      return;
    }
    const generation = ++saveGeneration.current;
    mutationPending.current = true;
    setSaving(true);
    setError(null);
    try {
      await rebuildPublicAiSessionProviderToolSelections(props.conversationId);
      if (generation === saveGeneration.current) {
        setReloadVersion((current) => current + 1);
      }
    } catch (reason) {
      if (generation === saveGeneration.current) {
        setError(errorMessage(reason));
      }
    } finally {
      if (generation === saveGeneration.current) {
        mutationPending.current = false;
        setSaving(false);
      }
    }
  }, [props.conversationId, saving]);

  // summaryItems resolves the hover card rows from the same draft and conflicts the dialog edits.
  // summaryItems 基于对话框编辑的同一份草稿与冲突解析悬浮卡片行。
  const summaryItems: ComposerCapabilitySummaryItem[] = snapshot ? capabilities.map((capability) => ({
    capability,
    label: providerToolCapabilityLabel(capability, t),
    color: providerToolCapabilityColor(capability),
    enabled: composerCapabilityIsEnabled(draft, capability),
    conflict: conflicts.some((conflict) => conflict.capability === capability),
    detail: composerCapabilitySummaryDetail(snapshot, draft, capability, t),
  })) : [];

  // describeCapability resolves one capability's switch state, catalogs and the one-line status shared by list and header.
  // describeCapability 解析单项能力的开关状态、目录与列表和标题共用的一行状态。
  // Parameters: capability is the row being described; returns the values the dialog renders for it.
  // 参数：capability 是被描述的行；返回弹窗为其渲染的值。
  const describeCapability = (capability: PublicAiProviderToolCapabilityKind) => {
    // directMode controls whether the capability uses one selected route and a switch.
    // directMode 控制当前能力是否使用单路由与开关。
    const directMode = isDirectProviderToolCapability(capability);
    // extensionSelectionCount reports the native models AI can reach in the capability's mode.
    // extensionSelectionCount 报告该能力当前模式下 AI 可使用的原生模型数量。
    const extensionSelectionCount = directMode ? 0 : composerExtensionSelections(draft, capability).length;
    // enabled represents direct activation or a non-empty native extension set.
    // enabled 表示直连启用或非空原生扩展集合。
    const storedEnabled = directMode
      ? draft.enabledCapabilities.includes(capability)
      : extensionSelectionCount > 0;
    // mediaAvailable reuses exact route conflicts, including reasoning support, independently from the stored switch.
    // mediaAvailable 独立于已存开关复用精确路由冲突校验，包括思考档位支持。
    const mediaAvailable = isMediaUnderstandingCapability(capability) && snapshot
      ? (mediaEditModes[capability] ?? draft.mediaUnderstandingSelections[capability]?.mode ?? "provider_tool") === draft.mediaUnderstandingSelections[capability]?.mode
        && !providerToolSettingsConflicts(snapshot, { ...draft, enabledCapabilities: [capability] })
          .some((item) => item.capability === capability)
      : true;
    const enabled = storedEnabled && mediaAvailable;
    const targets = snapshot?.providerToolTargets.filter(
      (target) => target.target.contract.capability === capability
        && (!isModelExtensionCapability(capability)
          || target.manifestSourceLayer !== "custom"),
    ) ?? [];
    const mediaCandidates = snapshot && isMediaUnderstandingCapability(capability)
      ? (snapshot.mediaUnderstandingModelCandidates ?? []).filter((candidate) => (
        modelCapabilitiesSupportMediaUnderstanding(candidate.capabilities, capability)
      ))
      : [];
    // targetCount reports every independently selectable session model across both backend families.
    // targetCount 报告两种后端类型中全部可由会话独立选择的模型。
    const targetCount = isMediaUnderstandingCapability(capability)
      ? uniqueProviderToolModels(targets).length
        + uniqueMediaUnderstandingModels(mediaCandidates).length
      : uniqueProviderToolModels(targets).length;
    const conflict = conflicts.find((item) => item.capability === capability);
    // subtitle names the chosen model when enabled, the chosen share for extensions, or the available count otherwise.
    // subtitle 在启用时给出所选模型，扩展能力给出已选占比，否则给出可用数量。
    const subtitle = enabled
      ? directMode || draft.extensionModes[capability]?.mode !== "multi"
        ? (snapshot ? composerCapabilitySummaryDetail(snapshot, draft, capability, t) : null)
          ?? t("composer.capability.source.targets").replace("{count}", String(targetCount))
        : t("composer.capability.source.targets").replace("{count}", `${extensionSelectionCount}/${targetCount}`)
      : targetCount > 0
        ? t("composer.capability.source.targets").replace("{count}", String(targetCount))
        : t("composer.capability.source.noTargets");
    return { directMode, extensionSelectionCount, mediaAvailable, enabled, targets, targetCount, conflict, subtitle };
  };
  const selectedDescription = settingsReady && capabilities.includes(selectedCapability)
    ? describeCapability(selectedCapability)
    : null;
  return {
    snapshot,
    selectedCapability,
    setSelectedCapability,
    draft,
    setDraft,
    saving,
    error,
    loadState,
    setReloadVersion,
    mutationPending,
    mediaEditModes,
    setMediaEditModes,
    loadStatus,
    settingsReady,
    capabilities,
    capabilityTargets,
    selectedModels,
    selectedExtensionTargets,
    selectedExtensionSingle,
    unavailableExtensionTargets,
    updateMediaSelection,
    selectedModel,
    selectedAccounts,
    selectedAccount,
    selectedMediaTargets,
    selectedMediaProviderModels,
    selectedMediaCandidates,
    selectedMediaModelChoices,
    selectedMediaSelection,
    selectedMediaMode,
    selectedMediaProviderTarget,
    selectedMediaProviderModelKey,
    selectedMediaProviderAccounts,
    selectedMediaModelCandidate,
    selectedMediaModelKey,
    selectedMediaModelAccounts,
    selectedMediaReasoningEfforts,
    conflicts,
    enabledCount,
    hasUnsavedChanges,
    requestsStarted,
    contextBreakPending,
    setContextBreakPending,
    defaultSettings,
    draftMatchesDefaults,
    restoreDefaults,
    save,
    rebuild,
    summaryItems,
    describeCapability,
    selectedDescription,
  };
}

// ComposerCapabilitySettings is the complete state and action surface the capability dialog renders.
// ComposerCapabilitySettings 是能力对话框渲染所用的完整状态与操作集合。
export type ComposerCapabilitySettings = ReturnType<typeof useComposerCapabilitySettings>;
