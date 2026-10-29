import * as React from "react";
import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import { getSessionRequestOptions, setSessionRequestOptions } from "@/service/conversation-actions-service";
import { loadPublicAiSettings } from "@/service/settings-actions-service";
import type { PublicAiPromptCacheCapability, PublicAiSettingsSnapshot, SessionRequestOptions } from "@/types";

// ANTHROPIC_MESSAGES_PROTOCOL_IDS are the routes whose cache lifetime the session can choose.
// ANTHROPIC_MESSAGES_PROTOCOL_IDS 是会话可选择缓存时长的路由。
const ANTHROPIC_MESSAGES_PROTOCOL_IDS = new Set(["anthropic-messages.v1", "anthropic-messages-video.v1"]);

// CODEX_SERVICE_TIER_PARAMETER_ID mirrors the backend parameter carrying the Codex service tier.
// CODEX_SERVICE_TIER_PARAMETER_ID 对应承载 Codex 服务档位的后端参数。
const CODEX_SERVICE_TIER_PARAMETER_ID = "codex.service_tier";

// ComposerRequestOptionRoutes are the session request options the active route offers.
// ComposerRequestOptionRoutes 是当前路由提供的会话请求选项。
export interface ComposerRequestOptionRoutes {
  // Cache lifetimes of an Anthropic Messages route; null hides the lifetime selector.
  // Anthropic Messages 路由的缓存时长；为 null 时隐藏时长选择器。
  promptCache: {
    options: PublicAiPromptCacheCapability["supported_ttl_options"];
    // Lifetime the route applies when the session stores no choice.
    // 会话未保存选择时路由采用的时长。
    defaultOptionId: string | null;
  } | null;
  // Service tiers listed by the account catalog besides the standard tier; null hides the tier selector.
  // 账号目录列出的标准档位以外的服务档位；为 null 时隐藏档位选择器。
  serviceTier: { values: string[] } | null;
}

// NO_ROUTES is the shared empty result.
// NO_ROUTES 是共享的空结果。
const NO_ROUTES: ComposerRequestOptionRoutes = { promptCache: null, serviceTier: null };

// resolveComposerRequestOptionRoutes finds the session request options offered by the active model account.
// resolveComposerRequestOptionRoutes 解析当前模型账号提供的会话请求选项。
// Parameters: snapshot is the latest settings snapshot; choice the active provider/account/model row.
// 参数：snapshot 为最新设置快照；choice 为当前供应商/账号/模型行。
// Returns: the offered options; both are null when the route offers neither.
// 返回值：提供的选项；路由两者都不提供时均为 null。
export function resolveComposerRequestOptionRoutes(
  snapshot: Pick<PublicAiSettingsSnapshot, "catalogs" | "modelRouteRecords"> | null,
  choice: Pick<ModelPickerChoice, "providerId" | "connectionId" | "modelId"> | null,
): ComposerRequestOptionRoutes {
  if (!snapshot || !choice) {
    return NO_ROUTES;
  }
  const protocolId = snapshot.modelRouteRecords?.find((record) => (
    record.providerId === choice.providerId
    && record.connectionId === choice.connectionId
    && record.modelId === choice.modelId
    && record.executable
  ))?.protocolId;
  const capabilities = snapshot.catalogs[choice.connectionId]?.models
    .find((model) => model.id === choice.modelId)?.capabilities;
  if (!protocolId || !capabilities) {
    return NO_ROUTES;
  }
  const cache = capabilities.prompt_cache;
  const promptCache = ANTHROPIC_MESSAGES_PROTOCOL_IDS.has(protocolId)
    && cache?.supports_managed_explicit
    && cache.supported_ttl_options.length > 0
    ? {
      options: cache.supported_ttl_options,
      defaultOptionId: cache.profile_default_ttl_option_id ?? cache.supported_ttl_options[0]?.id ?? null,
    }
    : null;
  const tier = capabilities.model_parameters.find((parameter) => (
    parameter.id === CODEX_SERVICE_TIER_PARAMETER_ID
    && (parameter.protocol_ids.length === 0 || parameter.protocol_ids.includes(protocolId))
    && (parameter.model_ids.length === 0 || parameter.model_ids.includes(choice.modelId))
  ));
  const tierValues = tier?.schema.type === "enum" && Array.isArray(tier.schema.values)
    ? tier.schema.values.filter((value): value is string => typeof value === "string")
    : [];
  return {
    promptCache,
    serviceTier: tierValues.length > 0 ? { values: tierValues } : null,
  };
}

// requestOptionShortLabel renders an option id such as "5m" or "priority" as a compact label.
// requestOptionShortLabel 将 "5m" 或 "priority" 等选项标识渲染为紧凑标签。
export function requestOptionShortLabel(optionId: string): string {
  return /^\d/.test(optionId) ? optionId.toUpperCase() : optionId.charAt(0).toUpperCase() + optionId.slice(1);
}

// ComposerRequestOptionChoice is one partial change of the session request options.
// ComposerRequestOptionChoice 是会话请求选项的一次部分修改。
export type ComposerRequestOptionChoice = Partial<Pick<SessionRequestOptions, "promptCacheTtl" | "serviceTier">>;

// ComposerRequestOptions is the active route's session request options with their read and save actions.
// ComposerRequestOptions 是当前路由的会话请求选项及其读取与保存操作。
export interface ComposerRequestOptions {
  // Options the active route offers.
  // 当前路由提供的选项。
  routes: ComposerRequestOptionRoutes;
  // Lifetime shown now: the pending or stored session choice, otherwise the route default.
  // 当前显示的时长：待定或已保存的会话选择，否则为路由默认值。
  promptCacheTtl: string | null;
  // Tier shown now; null is the standard tier.
  // 当前显示的档位；null 为标准档位。
  serviceTier: string | null;
  // Whether choices can be saved: only a created session stores them.
  // 是否可保存选择：只有已创建的会话会保存。
  editable: boolean;
  // Whether a choice is being saved.
  // 是否正在保存选择。
  saving: boolean;
  // Latest read or save failure.
  // 最近一次读取或保存失败。
  error: string | null;
  // Rereads the session options, and the settings snapshot unless a sibling control supplies it.
  // 重新读取会话选项；除非同级控件提供设置快照，否则一并重读。
  load: (includeSettings?: boolean) => Promise<void>;
  // Saves one partial change as the session's request options.
  // 将一次部分修改保存为会话请求选项。
  select: (choice: ComposerRequestOptionChoice) => Promise<void>;
}

// useComposerRequestOptions owns the reads behind the composer cache-lifetime and service-tier selectors.
// useComposerRequestOptions 持有输入区缓存时长与服务档位选择器背后的读取。
// Parameters: choice is the active provider/account/model row, null when no model is selected;
// sharedSnapshot is a fresher settings snapshot read by a sibling control, adopted instead of reading again;
// conversationId is the created session, null for a draft.
// 参数：choice 为当前供应商/账号/模型行，未选择模型时为 null；sharedSnapshot 为同级控件读取的较新设置快照，
// 直接采用而不再重复读取；conversationId 为已创建会话，草稿时为 null。
// Returns: the options and actions; responses are scoped to the exact row, session and component lifetime.
// 返回值：选项与操作；响应限定到精确行、会话及组件生命周期。
export function useComposerRequestOptions(
  choice: Pick<ModelPickerChoice, "providerId" | "connectionId" | "modelId"> | null,
  sharedSnapshot: PublicAiSettingsSnapshot | null,
  conversationId: string | null,
): ComposerRequestOptions {
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [stored, setStored] = React.useState<SessionRequestOptions | null>(null);
  const [pending, setPending] = React.useState<SessionRequestOptions | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const choiceKey = choice?.connectionId ? `${choice.providerId}\u0000${choice.connectionId}\u0000${choice.modelId}` : "";
  const scopeKey = `${choiceKey}\u0000${conversationId ?? ""}`;
  // owner scopes responses to the exact row, session and component lifetime.
  // owner 将响应限定到精确行、会话及组件生命周期。
  const owner = React.useRef({ scopeKey, active: true, readVersion: 0 });
  if (owner.current.scopeKey !== scopeKey) {
    owner.current.active = false;
    owner.current = { scopeKey, active: true, readVersion: 0 };
  }

  const load = React.useCallback(async (includeSettings = true) => {
    const current = owner.current;
    if (!choiceKey) {
      return;
    }
    const version = ++current.readVersion;
    try {
      const [nextSnapshot, nextStored] = await Promise.all([
        includeSettings ? loadPublicAiSettings() : Promise.resolve(null),
        conversationId ? getSessionRequestOptions(conversationId) : Promise.resolve(null),
      ]);
      if (current.active && current.readVersion === version) {
        if (nextSnapshot) setSnapshot(nextSnapshot);
        setStored(nextStored);
        setError(null);
      }
    } catch (reason) {
      if (current.active && current.readVersion === version) setError(reason instanceof Error ? reason.message : String(reason));
    }
  }, [choiceKey, conversationId]);

  React.useEffect(() => {
    const current = owner.current;
    current.active = true;
    setStored(null);
    setPending(null);
    setError(null);
    void load();
    return () => { current.active = false; };
  }, [scopeKey, load]);

  React.useEffect(() => {
    if (sharedSnapshot) {
      setSnapshot(sharedSnapshot);
    }
  }, [sharedSnapshot]);

  const providerId = choice?.providerId ?? "";
  const connectionId = choice?.connectionId ?? "";
  const modelId = choice?.modelId ?? "";
  const routes = React.useMemo(
    () => (connectionId ? resolveComposerRequestOptionRoutes(snapshot, { providerId, connectionId, modelId }) : NO_ROUTES),
    [connectionId, modelId, providerId, snapshot],
  );
  const shown = pending ?? stored;

  const select = React.useCallback(async (change: ComposerRequestOptionChoice) => {
    const current = owner.current;
    if (!conversationId || pending !== null) {
      return;
    }
    const next: SessionRequestOptions = {
      conversationId,
      promptCacheTtl: stored?.promptCacheTtl ?? null,
      serviceTier: stored?.serviceTier ?? null,
      ...change,
    };
    if (next.promptCacheTtl === (stored?.promptCacheTtl ?? null) && next.serviceTier === (stored?.serviceTier ?? null)) {
      return;
    }
    // A save invalidates any read still in flight, so older options cannot replace the saved ones.
    // 保存使仍在进行的读取失效，避免较旧选项覆盖已保存结果。
    current.readVersion += 1;
    setPending(next);
    setError(null);
    try {
      const saved = await setSessionRequestOptions(next);
      if (current.active) setStored(saved);
    } catch (reason) {
      if (current.active) setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (current.active) setPending(null);
    }
  }, [conversationId, pending, stored]);

  // A stored choice the current route does not offer is ignored by the backend, so the default shows.
  // 当前路由未提供的已保存选择会被后端忽略，因此显示默认值。
  const storedTtl = shown?.promptCacheTtl;
  const storedTier = shown?.serviceTier;
  return {
    routes,
    promptCacheTtl: storedTtl && routes.promptCache?.options.some((option) => option.id === storedTtl)
      ? storedTtl
      : routes.promptCache?.defaultOptionId ?? null,
    serviceTier: storedTier && routes.serviceTier?.values.includes(storedTier) ? storedTier : null,
    editable: Boolean(conversationId),
    saving: pending !== null,
    error,
    load,
    select,
  };
}
