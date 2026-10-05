import type { useI18n } from "@/i18n";
import type { PublicAiProviderToolCapabilityKind } from "@/types";

// ProviderToolCapabilityGroupId names one intent group shared by the settings tabs and the session capability dialog.
// ProviderToolCapabilityGroupId 命名设置页标签与会话能力弹窗共用的一个意图分组。
export type ProviderToolCapabilityGroupId = "search" | "understand" | "generate" | "audio";

// ProviderToolCapabilityGroup binds one intent group to its localized label and ordered member capabilities.
// ProviderToolCapabilityGroup 将一个意图分组绑定到其本地化名称与有序成员能力。
export interface ProviderToolCapabilityGroup {
  // Stable group identity.
  // 稳定的分组标识。
  id: ProviderToolCapabilityGroupId;
  // Translation key of the group name.
  // 分组名称的翻译键。
  labelKey: Parameters<ReturnType<typeof useI18n>["t"]>[0];
  // Member capabilities in display order.
  // 按展示顺序排列的成员能力。
  capabilities: PublicAiProviderToolCapabilityKind[];
}

// PROVIDER_TOOL_CAPABILITY_GROUPS is the single owner of capability grouping and in-group order.
// PROVIDER_TOOL_CAPABILITY_GROUPS 是能力分组与组内顺序的唯一来源。
export const PROVIDER_TOOL_CAPABILITY_GROUPS: ProviderToolCapabilityGroup[] = [
  { id: "search", labelKey: "settings.aiCapabilities.tab.search", capabilities: ["web_search", "web_extract"] },
  {
    id: "understand",
    labelKey: "settings.aiCapabilities.tab.understand",
    capabilities: ["image_understand", "video_understand", "audio_understand"],
  },
  {
    id: "generate",
    labelKey: "settings.aiCapabilities.tab.generate",
    capabilities: ["image_generate", "image_edit", "video_generate", "video_edit", "video_extend"],
  },
  {
    id: "audio",
    labelKey: "settings.aiCapabilities.tab.audio",
    capabilities: ["speech_synthesize", "speech_transcribe", "music_generate", "music_cover"],
  },
];

// groupProviderToolCapabilities splits an ordered capability list into the shared intent groups.
// groupProviderToolCapabilities 将有序能力列表拆分到共享意图分组中。
// Parameters: capabilities is the visible set; returns non-empty groups in shared order, followed by an unlabeled group
// holding any capability no group declares yet, so a newly published capability is never hidden.
// 参数：capabilities 是可见集合；按共享顺序返回非空分组，并在末尾附加一个无名称分组，容纳尚未归组的能力，
// 确保新发布的能力不会被隐藏。
export function groupProviderToolCapabilities(
  capabilities: PublicAiProviderToolCapabilityKind[],
): Array<{ group: ProviderToolCapabilityGroup | null; capabilities: PublicAiProviderToolCapabilityKind[] }> {
  const visible = new Set(capabilities);
  const grouped = PROVIDER_TOOL_CAPABILITY_GROUPS
    .map((group) => ({ group, capabilities: group.capabilities.filter((capability) => visible.has(capability)) }))
    .filter((entry) => entry.capabilities.length > 0);
  const declared = new Set(PROVIDER_TOOL_CAPABILITY_GROUPS.flatMap((group) => group.capabilities));
  const ungrouped = capabilities.filter((capability) => !declared.has(capability));
  return ungrouped.length > 0 ? [...grouped, { group: null, capabilities: ungrouped }] : grouped;
}
