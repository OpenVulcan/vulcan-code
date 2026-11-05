import type { TranslationKey } from "@/i18n-messages";
import {
  contextWindowFitsModel,
  contextWindowSelectionTokens,
  formatContextWindowSelectionLabel,
  sameContextWindowSelection,
} from "@/lib/context-window-selection";
import { toast } from "@/ui-kit/primitives/toast";
import type {
  ContextWindowSelection,
  PublicAiContextCompactionWindowNode,
  PublicAiSettingsSnapshot,
  WorkbenchSnapshot,
} from "@/types";

// ModelContextWindowOffer is the model default a session can switch to after a model change.
// ModelContextWindowOffer 是会话换模型后可切换到的模型默认值。
export interface ModelContextWindowOffer {
  // Window the model default selects; null follows the model.
  // 模型默认值选择的窗口；为空表示跟随模型。
  selection: ContextWindowSelection | null;
  // Window the session uses now.
  // 会话当前使用的窗口。
  current: ContextWindowSelection | null;
}

// modelContextWindowOffer returns the target model's explicit default when it differs from the session and fits the model.
// modelContextWindowOffer 在目标模型的显式默认值与会话不同且适配模型时返回它。
// Parameters: settings and workbench are the snapshots read after the switch; providerId/modelId name the new model.
// 参数：settings 与 workbench 是切换后读取的快照；providerId/modelId 指定新模型。
// Returns null when the model has no explicit default, it already applies, or it no longer fits.
// 模型没有显式默认值、已生效或不再适配时返回 null。
export function modelContextWindowOffer(
  settings: PublicAiSettingsSnapshot,
  workbench: WorkbenchSnapshot,
  providerId: string,
  modelId: string,
): ModelContextWindowOffer | null {
  const entry = settings.modelContextWindowDefaults
    .find((candidate) => candidate.providerId === providerId && candidate.modelId === modelId);
  if (!entry) return null;
  const current = workbench.sessionContextWindowSelection ?? null;
  if (sameContextWindowSelection(entry.selection, current)) return null;
  if (entry.selection) {
    const limit = settings.modelContextWindowCandidates
      .find((candidate) => candidate.providerId === providerId && candidate.modelId === modelId)?.contextLimit ?? null;
    const tokens = contextWindowSelectionTokens(entry.selection, settings.contextCompactionWindowNodes.nodes);
    if (limit === null || tokens === null || !contextWindowFitsModel(tokens, limit)) return null;
  }
  return { selection: entry.selection, current };
}

// showModelContextWindowOffer asks, in a toast that stays until closed, whether to switch to the model default.
// showModelContextWindowOffer 用一个关闭前常驻的通知询问是否切换到模型默认值。
// Closing the toast keeps the session unchanged; only the Switch action calls onSwitch.
// 关闭通知会保持会话不变；只有“切换”操作会调用 onSwitch。
export function showModelContextWindowOffer(options: {
  offer: ModelContextWindowOffer;
  modelName: string;
  nodes: PublicAiContextCompactionWindowNode[];
  translate: (key: TranslationKey) => string;
  onSwitch: (selection: ContextWindowSelection | null) => void;
}): void {
  const { offer, nodes, translate } = options;
  const label = (selection: ContextWindowSelection | null): string => {
    const tokens = contextWindowSelectionTokens(selection, nodes);
    return selection && tokens !== null
      ? formatContextWindowSelectionLabel(selection, tokens)
      : translate("composer.contextWindow.default");
  };
  const description = translate("composer.contextWindow.modelDefault.description")
    .replaceAll("{model}", options.modelName)
    .replaceAll("{window}", label(offer.selection))
    .replaceAll("{current}", label(offer.current));
  toast(translate("composer.contextWindow.modelDefault.title"), {
    description,
    duration: Infinity,
    action: {
      label: translate("composer.contextWindow.modelDefault.switch"),
      onClick: () => options.onSwitch(offer.selection),
    },
  });
}
