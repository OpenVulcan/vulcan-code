import * as React from "react";
import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import { loadPublicAiSettings, savePublicAiModelOverrides } from "@/service/settings-actions-service";
import type {
  PublicAiModelOverrides,
  PublicAiPromptCacheCapability,
  PublicAiSettingsSnapshot,
} from "@/types";

// ANTHROPIC_MESSAGES_PROTOCOL_IDS mirrors the backend routes that cache by default when no choice is stored.
// ANTHROPIC_MESSAGES_PROTOCOL_IDS 对应后端在未保存选择时默认启用缓存的路由。
const ANTHROPIC_MESSAGES_PROTOCOL_IDS = new Set(["anthropic-messages.v1", "anthropic-messages-video.v1"]);

// ComposerPromptCacheRoute is the cache-lifetime state of the active Anthropic Messages route.
// ComposerPromptCacheRoute 是当前 Anthropic Messages 路由的缓存时长状态。
export interface ComposerPromptCacheRoute {
  // Provider, model and protocol keying the stored model overrides.
  // 作为已保存模型覆盖键的供应商、模型与协议。
  providerId: string;
  modelId: string;
  protocolId: string;
  // Lifetime options declared by the route capability, in backend order.
  // 路由能力声明的时长选项，按后端顺序排列。
  options: PublicAiPromptCacheCapability["supported_ttl_options"];
  // Lifetime applied now; null when caching was explicitly switched off in settings.
  // 当前生效的时长；在设置中显式关闭缓存时为 null。
  selectedOptionId: string | null;
  // Stored overrides of this route, kept intact apart from the cache choice.
  // 该路由已保存的覆盖，除缓存选择外保持不变。
  overrides: PublicAiModelOverrides | undefined;
}

// StoredPromptCachePolicy is the persisted prompt-cache policy shape.
// StoredPromptCachePolicy 是持久化提示词缓存策略的结构。
interface StoredPromptCachePolicy {
  mode?: string;
  ttl?: { type?: string; option_id?: string };
}

// resolveComposerPromptCacheRoute finds the cache-lifetime state of the active model account.
// resolveComposerPromptCacheRoute 解析当前模型账号的缓存时长状态。
// Parameters: snapshot is the latest settings snapshot; choice the active provider/account/model row.
// 参数：snapshot 为最新设置快照；choice 为当前供应商/账号/模型行。
// Returns: the route state, or null when the route is not Anthropic Messages or declares no lifetimes.
// 返回值：路由状态；路由不是 Anthropic Messages 或未声明时长时返回 null。
export function resolveComposerPromptCacheRoute(
  snapshot: Pick<PublicAiSettingsSnapshot, "catalogs" | "modelRouteRecords" | "modelOverrideRecords"> | null,
  choice: Pick<ModelPickerChoice, "providerId" | "connectionId" | "modelId"> | null,
): ComposerPromptCacheRoute | null {
  if (!snapshot || !choice) {
    return null;
  }
  const protocolId = snapshot.modelRouteRecords?.find((record) => (
    record.providerId === choice.providerId
    && record.connectionId === choice.connectionId
    && record.modelId === choice.modelId
    && record.executable
  ))?.protocolId;
  if (!protocolId || !ANTHROPIC_MESSAGES_PROTOCOL_IDS.has(protocolId)) {
    return null;
  }
  const capability = snapshot.catalogs[choice.connectionId]?.models
    .find((model) => model.id === choice.modelId)?.capabilities.prompt_cache;
  if (!capability?.supports_managed_explicit || capability.supported_ttl_options.length === 0) {
    return null;
  }
  const overrides = snapshot.modelOverrideRecords.find((record) => (
    record.providerId === choice.providerId
    && record.modelId === choice.modelId
    && record.protocolId === protocolId
  ))?.overrides;
  return {
    providerId: choice.providerId,
    modelId: choice.modelId,
    protocolId,
    options: capability.supported_ttl_options,
    selectedOptionId: storedPromptCacheOptionId(overrides?.prompt_cache as StoredPromptCachePolicy | undefined, capability),
    overrides,
  };
}

// storedPromptCacheOptionId maps a stored policy to the lifetime it applies.
// storedPromptCacheOptionId 将已保存策略映射为其生效时长。
function storedPromptCacheOptionId(
  policy: StoredPromptCachePolicy | undefined,
  capability: PublicAiPromptCacheCapability,
): string | null {
  // Without a stored policy the backend applies the managed profile default.
  // 未保存策略时，后端采用托管配置默认时长。
  if (policy && policy.mode !== "managed_explicit") {
    return null;
  }
  const optionId = policy?.ttl?.type === "capability_option" ? policy.ttl.option_id : undefined;
  return optionId
    ?? capability.profile_default_ttl_option_id
    ?? capability.supported_ttl_options[0]?.id
    ?? null;
}

// promptCacheOverridesWithOption returns the route overrides with one explicit cache lifetime.
// promptCacheOverridesWithOption 返回带有一个显式缓存时长的路由覆盖。
// Parameters: overrides are the stored route overrides; optionId the chosen lifetime.
// 参数：overrides 为已保存的路由覆盖；optionId 为所选时长。
// Returns: a new overrides object whose model parameters are preserved.
// 返回值：保留模型参数的新覆盖对象。
export function promptCacheOverridesWithOption(
  overrides: PublicAiModelOverrides | undefined,
  optionId: string,
): PublicAiModelOverrides {
  return {
    ...overrides,
    prompt_cache: { mode: "managed_explicit", ttl: { type: "capability_option", option_id: optionId } },
  };
}

// promptCacheOptionShortLabel renders a lifetime id such as "5m" as the compact "5M".
// promptCacheOptionShortLabel 将 "5m" 等时长标识渲染为紧凑的 "5M"。
export function promptCacheOptionShortLabel(optionId: string): string {
  return optionId.toUpperCase();
}

// ComposerPromptCache is the active route's cache lifetime with its read and save actions.
// ComposerPromptCache 是当前路由的缓存时长及其读取与保存操作。
export interface ComposerPromptCache {
  // Anthropic Messages route state; null hides the lifetime selector.
  // Anthropic Messages 路由状态；为 null 时隐藏时长选择器。
  route: ComposerPromptCacheRoute | null;
  // Lifetime shown now: the pending choice while saving, otherwise the stored one.
  // 当前显示的时长：保存期间为待定选择，否则为已保存选择。
  optionId: string | null;
  // Whether a lifetime choice is being saved.
  // 是否正在保存时长选择。
  saving: boolean;
  // Latest read or save failure for this route.
  // 该路由最近一次读取或保存失败。
  error: string | null;
  // Rereads the settings snapshot so choices made in settings show up.
  // 重新读取设置快照，使设置页中的选择得以显示。
  load: () => Promise<void>;
  // Saves one lifetime as this route's explicit cache choice.
  // 将一个时长保存为该路由的显式缓存选择。
  select: (optionId: string) => Promise<void>;
}

// useComposerPromptCache owns the settings snapshot behind the composer cache-lifetime selector.
// useComposerPromptCache 持有输入区缓存时长选择器背后的设置快照。
// Parameters: choice is the active provider/account/model row, null when no model is selected;
// sharedSnapshot is a fresher snapshot read by a sibling control, adopted instead of reading again.
// 参数：choice 为当前供应商/账号/模型行，未选择模型时为 null；sharedSnapshot 为同级控件读取的较新快照，
// 直接采用而不再重复读取。
// Returns: the route state and actions; responses are scoped to the exact row and component lifetime.
// 返回值：路由状态与操作；响应限定到精确行及组件生命周期。
export function useComposerPromptCache(
  choice: Pick<ModelPickerChoice, "providerId" | "connectionId" | "modelId"> | null,
  sharedSnapshot: PublicAiSettingsSnapshot | null,
): ComposerPromptCache {
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [pendingOptionId, setPendingOptionId] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const choiceKey = choice?.connectionId ? `${choice.providerId}\u0000${choice.connectionId}\u0000${choice.modelId}` : "";
  // owner scopes responses to the exact row and component lifetime.
  // owner 将响应限定到精确行及组件生命周期。
  const owner = React.useRef({ choiceKey, active: true, readVersion: 0 });
  if (owner.current.choiceKey !== choiceKey) {
    owner.current.active = false;
    owner.current = { choiceKey, active: true, readVersion: 0 };
  }

  const load = React.useCallback(async () => {
    const current = owner.current;
    if (!current.choiceKey) {
      return;
    }
    const version = ++current.readVersion;
    try {
      const next = await loadPublicAiSettings();
      if (current.active && current.readVersion === version) {
        setSnapshot(next);
        setError(null);
      }
    } catch (reason) {
      if (current.active && current.readVersion === version) setError(reason instanceof Error ? reason.message : String(reason));
    }
  }, []);

  React.useEffect(() => {
    const current = owner.current;
    current.active = true;
    setPendingOptionId(null);
    setError(null);
    void load();
    return () => { current.active = false; };
  }, [choiceKey, load]);

  React.useEffect(() => {
    // A pending save owns the next snapshot; adopting an older shared one would revert it.
    // 待定保存持有下一份快照；采用较旧的共享快照会回退该保存。
    if (sharedSnapshot && pendingOptionId === null) {
      owner.current.readVersion += 1;
      setSnapshot(sharedSnapshot);
    }
    // pendingOptionId is read only as a guard; a settled save must not re-adopt the shared snapshot.
    // pendingOptionId 仅作为守卫读取；保存结束后不得重新采用共享快照。
  }, [sharedSnapshot]);

  const providerId = choice?.providerId ?? "";
  const connectionId = choice?.connectionId ?? "";
  const modelId = choice?.modelId ?? "";
  const route = React.useMemo(
    () => (connectionId ? resolveComposerPromptCacheRoute(snapshot, { providerId, connectionId, modelId }) : null),
    [connectionId, modelId, providerId, snapshot],
  );

  const select = React.useCallback(async (optionId: string) => {
    const current = owner.current;
    if (!route || pendingOptionId !== null || optionId === route.selectedOptionId) {
      return;
    }
    // A save invalidates any read still in flight, so an older snapshot cannot replace the saved one.
    // 保存使仍在进行的读取失效，避免较旧快照覆盖已保存结果。
    current.readVersion += 1;
    setPendingOptionId(optionId);
    setError(null);
    try {
      const next = await savePublicAiModelOverrides({
        providerId: route.providerId,
        modelId: route.modelId,
        protocolId: route.protocolId,
        overrides: promptCacheOverridesWithOption(route.overrides, optionId),
      });
      if (current.active) setSnapshot(next);
    } catch (reason) {
      if (current.active) setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (current.active) setPendingOptionId(null);
    }
  }, [pendingOptionId, route]);

  return {
    route,
    optionId: pendingOptionId ?? route?.selectedOptionId ?? null,
    saving: pendingOptionId !== null,
    error,
    load,
    select,
  };
}
