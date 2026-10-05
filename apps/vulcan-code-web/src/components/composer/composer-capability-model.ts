import type { ComposerProviderToolSettingsSnapshot } from "@/components/composer/composer-model";
import type { useI18n } from "@/i18n";
import {
  isDirectProviderToolCapability,
  isModelExtensionCapability,
  modelCapabilitiesSupportMediaUnderstanding,
} from "@/lib/provider-tool-capabilities";
import { mediaReplacementReasoningEfforts } from "@/lib/reasoning-effort-labels";
import type {
  PublicAiMediaUnderstandingBackendSelection,
  PublicAiMediaUnderstandingModelCandidate,
  PublicAiProviderToolCapabilityKind,
  PublicAiProviderToolTargetSelection,
  PublicAiProviderToolTargetSnapshot,
  PublicAiReasoningEffort,
  PublicAiSessionProviderToolConflict,
  PublicAiSettingsSnapshot,
} from "@/types";

// defaultComposerProviderToolSettings is the single source of the global defaults a session starts from or restores.
// defaultComposerProviderToolSettings 是会话初始值与还原默认所用全局默认值的唯一来源。
// Parameters: snapshot is the authoritative public AI settings; returns a complete session snapshot copied from it.
// 参数：snapshot 是权威公开 AI 设置；返回从中复制的完整会话快照。
export function defaultComposerProviderToolSettings(
  snapshot: PublicAiSettingsSnapshot,
): ComposerProviderToolSettingsSnapshot {
  return {
    enabledCapabilities: snapshot.providerToolEnabledCapabilities,
    selections: snapshot.providerToolSelections,
    extensionTargetSets: snapshot.providerExtensionTargetSets,
    mediaUnderstandingSelections: snapshot.mediaUnderstandingSelections ?? {},
  };
}

// providerToolSettingsSignature serializes one complete snapshot deterministically for dirty checks.
// providerToolSettingsSignature 以确定性方式序列化完整快照用于脏检查。
export function providerToolSettingsSignature(
  settings: ComposerProviderToolSettingsSnapshot,
): string {
  return JSON.stringify(
    [
      [...settings.enabledCapabilities].sort(),
      Object.entries(settings.selections)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([capability, selection]) => [
        capability,
        selection?.connection_id ?? "",
        selection?.target_id ?? "",
      ]),
      Object.entries(settings.extensionTargetSets)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([capability, selections]) => [
          capability,
          ...(selections ?? [])
            .map((selection) => `${selection.connection_id.length}:${selection.connection_id}:${selection.target_id.length}:${selection.target_id}`)
            .sort(),
        ]),
      Object.entries(settings.mediaUnderstandingSelections)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([capability, selection]) => selection?.mode === "provider_tool"
          ? [
              capability,
              selection.mode,
              selection.selection.connection_id,
              selection.selection.target_id,
            ]
          : [
              capability,
              selection?.mode ?? "",
              selection?.selection.provider_id ?? "",
              selection?.selection.model_id ?? "",
              selection?.selection.connection_id ?? "",
              selection?.reasoning_effort ?? "",
            ]),
    ],
  );
}

// normalizedComposerProviderToolSettings upgrades an older in-memory draft with the exact route map.
// normalizedComposerProviderToolSettings 使用精确路由映射升级旧版内存草稿。
export function normalizedComposerProviderToolSettings(
  settings: ComposerProviderToolSettingsSnapshot,
): ComposerProviderToolSettingsSnapshot {
  return {
    ...settings,
    mediaUnderstandingSelections: settings.mediaUnderstandingSelections ?? {},
  };
}

// orderProviderToolCapabilities returns unique capabilities with explicit web-search priority.
// orderProviderToolCapabilities 返回去重后且明确将网页搜索置顶的能力列表。
export function orderProviderToolCapabilities(
  capabilities: PublicAiProviderToolCapabilityKind[],
): PublicAiProviderToolCapabilityKind[] {
  const priority = (capability: PublicAiProviderToolCapabilityKind): number => {
    if (capability === "web_search") {
      return 0;
    }
    if (capability === "web_extract") {
      return 1;
    }
    return 2;
  };
  return Array.from(new Set(capabilities)).sort((left, right) => {
    const priorityDifference = priority(left) - priority(right);
    return priorityDifference === 0 ? left.localeCompare(right) : priorityDifference;
  });
}

// orderedProviderToolCapabilities merges provider-native targets with model-replacement-only media backends.
// orderedProviderToolCapabilities 合并供应商原生目标与仅由模型代替提供的媒体后端。
export function orderedProviderToolCapabilities(
  snapshot: PublicAiSettingsSnapshot,
): PublicAiProviderToolCapabilityKind[] {
  // candidateCapabilities exposes every fixed facade backed by any currently executable model route.
  // candidateCapabilities 公开由任一当前可执行模型路由支持的全部固定外壳。
  const candidateCapabilities = ([
    "image_understand",
    "video_understand",
    "audio_understand",
  ] as const).filter((capability) => (snapshot.mediaUnderstandingModelCandidates ?? [])
    .some((candidate) => (
    modelCapabilitiesSupportMediaUnderstanding(candidate.capabilities, capability)
  )));
  // mediaCapabilities includes available routes without treating global defaults as an allowlist.
  // mediaCapabilities 包含可用路由，不把全局默认值当作允许列表。
  const mediaCapabilities = Array.from(new Set([
    ...candidateCapabilities,
  ]))
    .filter(isMediaUnderstandingCapability);
  return orderProviderToolCapabilities(
    [
      ...snapshot.providerToolTargets.map((target) => target.target.contract.capability),
      ...mediaCapabilities,
    ],
  );
}

// composerProviderToolCapabilities retains configurable empty media modes and saved extension categories for recovery.
// composerProviderToolCapabilities 保留可配置的空媒体模式与已保存的扩展分类，供用户恢复配置。
// Parameters: snapshot supplies current targets and settings supplies exact saved choices. Returns ordered unique categories.
// 参数：snapshot 提供当前目标，settings 提供准确已存选择。返回已排序且去重的分类。
export function composerProviderToolCapabilities(
  snapshot: PublicAiSettingsSnapshot,
  settings: ComposerProviderToolSettingsSnapshot,
): PublicAiProviderToolCapabilityKind[] {
  return orderProviderToolCapabilities([
    ...orderedProviderToolCapabilities(snapshot),
    "image_understand", "video_understand", "audio_understand",
    ...settings.enabledCapabilities,
    ...Object.keys(settings.extensionTargetSets).filter((capability) =>
      isModelExtensionCapability(capability as PublicAiProviderToolCapabilityKind)),
    ...Object.keys(settings.mediaUnderstandingSelections).filter(isMediaUnderstandingCapability),
  ] as PublicAiProviderToolCapabilityKind[]);
}

// PublicAiMediaUnderstandingCapabilityKind is the closed fixed input-only media facade set.
// PublicAiMediaUnderstandingCapabilityKind 是封闭的固定仅输入型媒体外壳集合。
export type PublicAiMediaUnderstandingCapabilityKind =
  | "image_understand"
  | "video_understand"
  | "audio_understand";

// isMediaUnderstandingCapability identifies the three fixed input-only media facades.
// isMediaUnderstandingCapability 标识三个固定的仅输入型媒体外壳。
export function isMediaUnderstandingCapability(
  capability: string,
): capability is PublicAiMediaUnderstandingCapabilityKind {
  return capability === "image_understand"
    || capability === "video_understand"
    || capability === "audio_understand";
}


// composerCapabilityIsEnabled resolves direct switches and extension selections without mixing stores.
// composerCapabilityIsEnabled 在不混用存储的前提下解析直连开关与扩展选择状态。
export function composerCapabilityIsEnabled(
  settings: ComposerProviderToolSettingsSnapshot,
  capability: PublicAiProviderToolCapabilityKind,
): boolean {
  return isDirectProviderToolCapability(capability)
    ? settings.enabledCapabilities.includes(capability)
    : (settings.extensionTargetSets[capability]?.length ?? 0) > 0;
}

// providerToolCapabilitySwitchDisabled blocks saving and unavailable new activation while preserving stale-state recovery.
// providerToolCapabilitySwitchDisabled 在保存期间以及无可用后端的新开启场景禁用开关，同时保留过期状态恢复能力。
export function providerToolCapabilitySwitchDisabled(
  saving: boolean,
  targetCount: number,
  enabled: boolean,
): boolean {
  return saving || (targetCount === 0 && !enabled);
}

// providerToolSettingsConflicts reports stale choices without silently replacing them.
// providerToolSettingsConflicts 报告过期选择且不静默替换。
export function providerToolSettingsConflicts(
  snapshot: PublicAiSettingsSnapshot,
  settings: ComposerProviderToolSettingsSnapshot,
): PublicAiSessionProviderToolConflict[] {
  const current = new Map(snapshot.providerToolTargets
    .map((target) => [
      `${target.connectionId.length}:${target.connectionId}:${target.target.target_id.length}:${target.target.target_id}`,
      target.target.contract.capability,
    ]));
  const capabilities = new Set<PublicAiProviderToolCapabilityKind>(settings.enabledCapabilities);
  const conflicts: PublicAiSessionProviderToolConflict[] = [];
  for (const capability of capabilities) {
    if (isMediaUnderstandingCapability(capability)) {
      // mediaSelection contains the complete session-owned route and never resolves through system defaults.
      // mediaSelection 包含完整会话自有路由，绝不通过系统默认值解析。
      const mediaSelection = settings.mediaUnderstandingSelections[capability];
      if (!mediaSelection) {
        conflicts.push({
          capability,
          kind: "missing_selection",
          message: `Capability ${capability} has no exact media-understanding route.`,
        });
        continue;
      }
      const available = mediaSelection.mode === "provider_tool"
        ? snapshot.providerToolTargets.some((target) => (
            target.connectionId === mediaSelection.selection.connection_id
            && target.target.target_id === mediaSelection.selection.target_id
            && target.target.contract.capability === capability
          ))
        : (snapshot.mediaUnderstandingModelCandidates ?? []).some((candidate) => (
            candidate.selection.provider_id === mediaSelection.selection.provider_id
            && candidate.selection.model_id === mediaSelection.selection.model_id
            && candidate.selection.connection_id === mediaSelection.selection.connection_id
            && modelCapabilitiesSupportMediaUnderstanding(candidate.capabilities, capability)
            && (
              mediaSelection.reasoning_effort === undefined
              || mediaReplacementReasoningEfforts(candidate.capabilities)
                .includes(mediaSelection.reasoning_effort)
            )
          ));
      if (!available) {
        conflicts.push({
          capability,
          kind: "target_unavailable",
          message: `The exact media-understanding route for ${capability} is no longer available.`,
        });
      }
      continue;
    }
    const selection = settings.selections[capability];
    if (!selection) {
      conflicts.push({
        capability,
        kind: "missing_selection",
        message: `Capability ${capability} has no selected model and account.`,
      });
      continue;
    }
    const key = `${selection.connection_id.length}:${selection.connection_id}:${selection.target_id.length}:${selection.target_id}`;
    if (current.get(key) !== capability) {
      conflicts.push({
        capability,
        kind: "target_unavailable",
        message: `The saved model or account for ${capability} is no longer available.`,
      });
    }
  }
  for (const [capability, selections] of Object.entries(settings.extensionTargetSets)) {
    if (!isModelExtensionCapability(capability as PublicAiProviderToolCapabilityKind)) {
      conflicts.push({
        capability: capability as PublicAiProviderToolCapabilityKind,
        kind: "invalid_extension_target_set",
        message: `Capability ${capability} cannot use extension targets.`,
      });
      continue;
    }
    for (const selection of selections ?? []) {
      const key = `${selection.connection_id.length}:${selection.connection_id}:${selection.target_id.length}:${selection.target_id}`;
      const target = snapshot.providerToolTargets.find((candidate) => (
        candidate.connectionId === selection.connection_id
        && candidate.target.target_id === selection.target_id
      ));
      if (current.get(key) !== capability || target?.manifestSourceLayer === "custom") {
        conflicts.push({
          capability: capability as PublicAiProviderToolCapabilityKind,
          kind: "invalid_extension_target_set",
          message: `The saved native extension model or account for ${capability} is unavailable.`,
        });
        break;
      }
    }
  }
  return conflicts;
}

// uniqueProviderToolModels collapses account-expanded targets into one row per model.
// uniqueProviderToolModels 将按账号展开的目标折叠为每个模型一行。
export function providerToolModelKey(target: PublicAiProviderToolTargetSnapshot): string {
  return JSON.stringify([
    target.target.provider_id,
    target.target.upstream_target_id,
  ]);
}

// mediaUnderstandingModelKey returns one browser-local provider-model identity.
// mediaUnderstandingModelKey 返回一个浏览器本地供应商模型身份。
export function mediaUnderstandingModelKey(candidate: PublicAiMediaUnderstandingModelCandidate): string {
  return JSON.stringify([
    candidate.selection.provider_id,
    candidate.selection.model_id,
  ]);
}

// uniqueMediaUnderstandingModels collapses exact account routes into stable model choices.
// uniqueMediaUnderstandingModels 将精确账号路由折叠为稳定模型选择。
export function uniqueMediaUnderstandingModels(
  candidates: PublicAiMediaUnderstandingModelCandidate[],
): PublicAiMediaUnderstandingModelCandidate[] {
  const models = new Map<string, PublicAiMediaUnderstandingModelCandidate>();
  for (const candidate of candidates) {
    const modelKey = mediaUnderstandingModelKey(candidate);
    if (!models.has(modelKey)) models.set(modelKey, candidate);
  }
  return Array.from(models.values());
}

// providerMediaUnderstandingSelection freezes one provider target into the closed backend union.
// providerMediaUnderstandingSelection 将一个供应商目标冻结为封闭后端联合类型。
export function providerMediaUnderstandingSelection(
  target: PublicAiProviderToolTargetSnapshot,
): PublicAiMediaUnderstandingBackendSelection {
  return {
    mode: "provider_tool",
    selection: selectionForTarget(target),
  };
}

// modelReplacementSelection freezes one model route and optional reasoning level.
// modelReplacementSelection 冻结一条模型路由及其可选推理档位。
export function modelReplacementSelection(
  candidate: PublicAiMediaUnderstandingModelCandidate,
  reasoningEffort?: PublicAiReasoningEffort,
): PublicAiMediaUnderstandingBackendSelection {
  return {
    mode: "model_replacement",
    selection: candidate.selection,
    ...(reasoningEffort ? { reasoning_effort: reasoningEffort } : {}),
  };
}

// supportedMediaReplacementReasoningEffort retains an effort only when the exact route supports it.
// supportedMediaReplacementReasoningEffort 仅在精确路由支持时保留推理档位。
export function supportedMediaReplacementReasoningEffort(
  candidate: PublicAiMediaUnderstandingModelCandidate,
  reasoningEffort?: PublicAiReasoningEffort,
): PublicAiReasoningEffort | undefined {
  if (!reasoningEffort) return undefined;
  return mediaReplacementReasoningEfforts(candidate.capabilities).includes(reasoningEffort)
    ? reasoningEffort
    : undefined;
}

// firstAvailableMediaUnderstandingSelection resolves the system default only as a preference.
// firstAvailableMediaUnderstandingSelection 仅把系统默认值作为优先项解析。
function firstAvailableMediaUnderstandingSelection(
  snapshot: PublicAiSettingsSnapshot,
  capability: PublicAiMediaUnderstandingCapabilityKind,
  targets: PublicAiProviderToolTargetSnapshot[],
  candidates: PublicAiMediaUnderstandingModelCandidate[],
): PublicAiMediaUnderstandingBackendSelection | undefined {
  const systemDefault = snapshot.mediaUnderstandingSelections?.[capability];
  if (systemDefault?.mode === "provider_tool") {
    const target = targets.find((candidate) => (
      candidate.connectionId === systemDefault.selection.connection_id
      && candidate.target.target_id === systemDefault.selection.target_id
    ));
    if (target) return providerMediaUnderstandingSelection(target);
  }
  if (systemDefault?.mode === "model_replacement") {
    const candidate = candidates.find((item) => (
      item.selection.provider_id === systemDefault.selection.provider_id
      && item.selection.model_id === systemDefault.selection.model_id
      && item.selection.connection_id === systemDefault.selection.connection_id
    ));
    if (candidate) {
      return modelReplacementSelection(
        candidate,
        supportedMediaReplacementReasoningEffort(candidate, systemDefault.reasoning_effort),
      );
    }
  }
  if (targets[0]) return providerMediaUnderstandingSelection(targets[0]);
  return candidates[0] ? modelReplacementSelection(candidates[0]) : undefined;
}

export function uniqueProviderToolModels(
  targets: PublicAiProviderToolTargetSnapshot[],
): PublicAiProviderToolTargetSnapshot[] {
  const models = new Map<string, PublicAiProviderToolTargetSnapshot>();
  for (const target of targets) {
    // modelKey prevents collisions across providers while grouping account-expanded targets.
    // modelKey 在折叠账号展开目标时防止不同供应商之间发生冲突。
    const modelKey = providerToolModelKey(target);
    if (!models.has(modelKey)) models.set(modelKey, target);
  }
  return Array.from(models.values()).sort((left, right) => (
    left.target.display_name.localeCompare(right.target.display_name)
    || left.target.target_id.localeCompare(right.target.target_id)
  ));
}

// selectionForTarget projects one browser target into the exact persisted selection.
// selectionForTarget 将一个浏览器目标投影为精确持久化选择。
export function selectionForTarget(
  target: PublicAiProviderToolTargetSnapshot,
): PublicAiProviderToolTargetSelection {
  return {
    connection_id: target.connectionId,
    target_id: target.target.target_id,
  };
}

// providerToolCapabilityColor restores the established capability color tokens.
// providerToolCapabilityColor 恢复既有能力颜色令牌。
// Parameters: capability is the closed tool capability; returns a complete existing CSS color-token expression.
// 参数：capability 是封闭工具能力；返回现有 CSS 完整颜色令牌表达式。
export function providerToolCapabilityColor(capability: PublicAiProviderToolCapabilityKind): string {
  if (capability === "web_search" || capability === "web_extract") {
    return "var(--chart-1)";
  }
  if (
    capability === "image_understand"
    || capability === "video_understand"
    || capability === "audio_understand"
  ) {
    return "var(--chart-3)";
  }
  if (capability === "image_generate" || capability === "image_edit") {
    return "var(--chart-5)";
  }
  if (capability === "video_generate" || capability === "video_edit" || capability === "video_extend") {
    return "var(--chart-4)";
  }
  if (capability === "speech_synthesize" || capability === "speech_transcribe") {
    return "var(--chart-2)";
  }
  if (capability === "music_cover") {
    return "var(--chart-2)";
  }
  return "var(--chart-3)";
}

// composerCapabilitySummaryDetail names the session's selected target for one capability, or its selected-model count.
// composerCapabilitySummaryDetail 给出会话为某项能力所选目标的名称，或已选模型数量。
// Parameters: snapshot owns the target catalogs, settings is the session draft; returns null when nothing is selected or resolvable.
// 参数：snapshot 持有目标目录，settings 是会话草稿；无选择或无法解析时返回 null。
export function composerCapabilitySummaryDetail(
  snapshot: PublicAiSettingsSnapshot,
  settings: ComposerProviderToolSettingsSnapshot,
  capability: PublicAiProviderToolCapabilityKind,
  t: ReturnType<typeof useI18n>["t"],
): string | null {
  const targetName = (selection: PublicAiProviderToolTargetSelection | undefined) => selection
    ? snapshot.providerToolTargets.find((target) => (
      target.connectionId === selection.connection_id && target.target.target_id === selection.target_id
    ))?.target.display_name ?? null
    : null;
  if (!isDirectProviderToolCapability(capability)) {
    const count = settings.extensionTargetSets[capability]?.length ?? 0;
    return count > 0 ? t("composer.capability.source.targets").replace("{count}", String(count)) : null;
  }
  if (isMediaUnderstandingCapability(capability)) {
    const media = settings.mediaUnderstandingSelections[capability];
    if (media?.mode === "provider_tool") return targetName(media.selection);
    if (media?.mode === "model_replacement") {
      return (snapshot.mediaUnderstandingModelCandidates ?? []).find((candidate) => (
        candidate.selection.provider_id === media.selection.provider_id
        && candidate.selection.model_id === media.selection.model_id
        && candidate.selection.connection_id === media.selection.connection_id
      ))?.modelDisplayName ?? null;
    }
    return null;
  }
  return targetName(settings.selections[capability]);
}

// providerToolCapabilityLabel localizes one provider tool capability.
// providerToolCapabilityLabel 本地化一项供应商工具能力。
export function providerToolCapabilityLabel(
  capability: PublicAiProviderToolCapabilityKind,
  t: ReturnType<typeof useI18n>["t"],
): string {
  const keys: Partial<Record<
    PublicAiProviderToolCapabilityKind,
    Parameters<typeof t>[0]
  >> = {
    web_search: "settings.providerTools.webSearch",
    web_extract: "settings.providerTools.webExtract",
    image_understand: "settings.aiUnderstandImage",
    video_understand: "settings.aiUnderstandVideo",
    audio_understand: "settings.aiUnderstandAudio",
    image_generate: "settings.providerTools.imageGenerate",
    image_edit: "settings.providerTools.imageEdit",
    video_generate: "settings.providerTools.videoGenerate",
    video_edit: "settings.providerTools.videoEdit",
    video_extend: "settings.providerTools.videoExtend",
    speech_synthesize: "settings.providerTools.speechSynthesize",
    speech_transcribe: "settings.providerTools.speechTranscribe",
    music_generate: "settings.providerTools.musicGenerate",
    music_cover: "settings.providerTools.musicCover",
  };
  const key = keys[capability];
  return key ? t(key) : capability;
}
