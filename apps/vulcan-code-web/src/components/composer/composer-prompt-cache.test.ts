import { describe, expect, it } from "vitest";

import {
  promptCacheOptionShortLabel,
  promptCacheOverridesWithOption,
  resolveComposerPromptCacheRoute,
} from "./composer-prompt-cache";
import type { PublicAiModelOverrideRecord, PublicAiSettingsSnapshot } from "@/types";

type CacheSnapshot = Pick<PublicAiSettingsSnapshot, "catalogs" | "modelRouteRecords" | "modelOverrideRecords">;

const choice = { providerId: "anthropic", connectionId: "connection-a", modelId: "claude-opus-5-5" };

// cacheSnapshot builds one executable route with the Anthropic lifetime capability.
// cacheSnapshot 构造一条带有 Anthropic 时长能力的可执行路由。
function cacheSnapshot(protocolId: string, overrideRecords: PublicAiModelOverrideRecord[] = []): CacheSnapshot {
  return {
    catalogs: {
      "connection-a": {
        connection_id: "connection-a",
        generation: 1,
        page_count: 1,
        discovery_schema_revision: "test",
        models: [{
          id: "claude-opus-5-5",
          capabilities: {
            evidence_revision: "test",
            reasoning: "fixed_on",
            model_parameters: [],
            prompt_cache: {
              provider_default_behavior: "",
              supports_managed_explicit: true,
              supported_ttl_options: [
                { id: "5m", display_name: "5 minutes" },
                { id: "1h", display_name: "1 hour" },
              ],
              profile_default_ttl_option_id: "5m",
            },
          },
        }],
      },
    } as unknown as CacheSnapshot["catalogs"],
    modelRouteRecords: [{
      providerId: "anthropic",
      connectionId: "connection-a",
      serviceEntryId: "api",
      modelId: "claude-opus-5-5",
      protocolId,
      availableProtocols: [protocolId],
      manifestRevision: 1,
      executable: true,
    }],
    modelOverrideRecords: overrideRecords,
  };
}

describe("resolveComposerPromptCacheRoute", () => {
  it("defaults an Anthropic Messages route without a stored choice to the profile lifetime", () => {
    const route = resolveComposerPromptCacheRoute(cacheSnapshot("anthropic-messages.v1"), choice);
    expect(route?.protocolId).toBe("anthropic-messages.v1");
    expect(route?.selectedOptionId).toBe("5m");
    expect(route?.options.map((option) => option.id)).toEqual(["5m", "1h"]);
  });

  it("reads an explicit lifetime and an explicit provider-default choice", () => {
    const record = (promptCache: unknown): PublicAiModelOverrideRecord => ({
      providerId: "anthropic",
      modelId: "claude-opus-5-5",
      protocolId: "anthropic-messages.v1",
      overrides: { prompt_cache: promptCache },
    });
    const hour = { mode: "managed_explicit", ttl: { type: "capability_option", option_id: "1h" } };
    expect(resolveComposerPromptCacheRoute(cacheSnapshot("anthropic-messages.v1", [record(hour)]), choice)?.selectedOptionId)
      .toBe("1h");
    expect(resolveComposerPromptCacheRoute(cacheSnapshot("anthropic-messages.v1", [record({ mode: "provider_default" })]), choice)?.selectedOptionId)
      .toBeNull();
  });

  it("hides the selector for other protocols", () => {
    expect(resolveComposerPromptCacheRoute(cacheSnapshot("openai-chat-completions.v1"), choice)).toBeNull();
  });
});

describe("promptCacheOverridesWithOption", () => {
  it("keeps model parameters while replacing the cache choice", () => {
    const parameters = { temperature: { type: "number", value: 0.2 } };
    expect(promptCacheOverridesWithOption({ model_parameters: parameters, prompt_cache: { mode: "provider_default" } }, "1h"))
      .toEqual({
        model_parameters: parameters,
        prompt_cache: { mode: "managed_explicit", ttl: { type: "capability_option", option_id: "1h" } },
      });
    expect(promptCacheOptionShortLabel("5m")).toBe("5M");
  });
});
