import type { TranslationKey } from "@/i18n";
import type { PluginCapacityPurpose, PluginCapacityReceipt } from "@/types/plugin-runtime";
import type { PluginPrewarmPolicyPhase } from "@/types/plugin-prewarm-policy";

// capacityPurposeLabels names each native capacity purpose by what the user does with the plugin.
// capacityPurposeLabels 以用户使用插件的方式命名各原生容量用途。
export const capacityPurposeLabels: Record<PluginCapacityPurpose, TranslationKey> = {
  tool_call: "pluginRuntime.purpose.toolCall", tool_session: "pluginRuntime.purpose.toolSession",
  hook: "pluginRuntime.purpose.hook", ui_action: "pluginRuntime.purpose.uiAction",
  lifecycle: "pluginRuntime.purpose.lifecycle", activation_probe: "pluginRuntime.purpose.activationProbe",
};

// capacityReceiptLabels explains the last capacity change in plain words.
// capacityReceiptLabels 以平实语言说明上一次容量调整。
export const capacityReceiptLabels: Record<PluginCapacityReceipt["phase"], TranslationKey> = {
  prepared: "pluginRuntime.receipt.prepared", applied: "pluginRuntime.receipt.applied", committed: "pluginRuntime.receipt.committed",
  rejected: "pluginRuntime.receipt.rejected", interrupted: "pluginRuntime.receipt.interrupted",
};

// prewarmReceiptLabels explains the last automatic prewarm rule publication in plain words.
// prewarmReceiptLabels 以平实语言说明上一次自动预热规则发布。
export const prewarmReceiptLabels: Record<PluginPrewarmPolicyPhase, TranslationKey> = {
  prepared: "pluginPrewarmPolicy.receipt.prepared", publishing: "pluginPrewarmPolicy.receipt.publishing",
  committed: "pluginPrewarmPolicy.receipt.committed", cancelled: "pluginPrewarmPolicy.receipt.cancelled",
  interrupted: "pluginPrewarmPolicy.receipt.interrupted",
};

// capacityActionLabels and prewarmActionLabels name each service-offered receipt continuation with its button variant.
// capacityActionLabels 与 prewarmActionLabels 命名服务提供的各回执继续方式及其按钮样式。
export const capacityActionLabels = {
  cancel: ["pluginRuntime.cancelUpdate", "ghost"], resume: ["pluginRuntime.resume", "default"], forget: ["pluginRuntime.ack", "outline"],
} as const satisfies Record<"cancel" | "resume" | "forget", readonly [TranslationKey, "ghost" | "default" | "outline"]>;
export const prewarmActionLabels = {
  cancel: ["pluginPrewarmPolicy.cancel", "ghost"], resume: ["pluginPrewarmPolicy.resume", "default"], forget: ["pluginPrewarmPolicy.forget", "outline"],
} as const satisfies Record<"cancel" | "resume" | "forget", readonly [TranslationKey, "ghost" | "default" | "outline"]>;
