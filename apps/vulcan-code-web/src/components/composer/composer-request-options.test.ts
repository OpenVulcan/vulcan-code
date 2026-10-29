import { describe, expect, it } from "vitest";

import { requestOptionShortLabel, resolveComposerRequestOptionRoutes } from "./composer-request-options";
import type { PublicAiModelParameterDefinition, PublicAiSettingsSnapshot } from "@/types";

type RouteSnapshot = Pick<PublicAiSettingsSnapshot, "catalogs" | "modelRouteRecords">;

const choice = { providerId: "provider-a", connectionId: "connection-a", modelId: "model-a" };

// tierParameter builds the Codex service-tier parameter registered for one protocol.
// tierParameter 构造为一个协议注册的 Codex 服务档位参数。
function tierParameter(protocolId: string, values: string[]): PublicAiModelParameterDefinition {
  return {
    id: "codex.service_tier",
    display_name: "Service tier",
    description: "",
    schema: { type: "enum", values },
    default: { type: "none" },
    protocol_ids: [protocolId],
    model_ids: ["model-a"],
    dependent_required: [],
    conflicts: [],
    evidence_revision: "test",
  };
}

// routeSnapshot builds one executable route whose model declares the given capabilities.
// routeSnapshot 构造一条可执行路由，其模型声明给定能力。
function routeSnapshot(
  protocolId: string,
  options: { cache?: boolean; parameters?: PublicAiModelParameterDefinition[] } = {},
): RouteSnapshot {
  return {
    catalogs: {
      "connection-a": {
        connection_id: "connection-a",
        generation: 1,
        page_count: 1,
        discovery_schema_revision: "test",
        models: [{
          id: "model-a",
          capabilities: {
            evidence_revision: "test",
            reasoning: "fixed_on",
            model_parameters: options.parameters ?? [],
            prompt_cache: {
              provider_default_behavior: "",
              supports_managed_explicit: options.cache ?? false,
              supported_ttl_options: options.cache
                ? [{ id: "5m", display_name: "5 minutes" }, { id: "1h", display_name: "1 hour" }]
                : [],
              profile_default_ttl_option_id: options.cache ? "5m" : undefined,
            },
          },
        }],
      },
    } as unknown as RouteSnapshot["catalogs"],
    modelRouteRecords: [{
      providerId: "provider-a",
      connectionId: "connection-a",
      serviceEntryId: "api",
      modelId: "model-a",
      protocolId,
      availableProtocols: [protocolId],
      manifestRevision: 1,
      executable: true,
    }],
  };
}

describe("resolveComposerRequestOptionRoutes", () => {
  it("offers cache lifetimes with the profile default on Anthropic Messages routes", () => {
    const routes = resolveComposerRequestOptionRoutes(routeSnapshot("anthropic-messages.v1", { cache: true }), choice);
    expect(routes.promptCache?.options.map((option) => option.id)).toEqual(["5m", "1h"]);
    expect(routes.promptCache?.defaultOptionId).toBe("5m");
    expect(routes.serviceTier).toBeNull();
  });

  it("hides cache lifetimes on other protocols even when the model declares them", () => {
    expect(resolveComposerRequestOptionRoutes(routeSnapshot("openai-chat-completions.v1", { cache: true }), choice).promptCache)
      .toBeNull();
  });

  it("offers service tiers only where the parameter is registered for the route protocol", () => {
    const parameters = [tierParameter("openai-codex-responses.v1", ["priority", "ultrafast"])];
    expect(resolveComposerRequestOptionRoutes(routeSnapshot("openai-codex-responses.v1", { parameters }), choice).serviceTier)
      .toEqual({ values: ["priority", "ultrafast"] });
    expect(resolveComposerRequestOptionRoutes(routeSnapshot("openai-responses.v1", { parameters }), choice).serviceTier)
      .toBeNull();
  });

  it("offers nothing without a selected executable route", () => {
    expect(resolveComposerRequestOptionRoutes(null, choice)).toEqual({ promptCache: null, serviceTier: null });
    expect(resolveComposerRequestOptionRoutes(routeSnapshot("anthropic-messages.v1", { cache: true }), { ...choice, modelId: "other" }))
      .toEqual({ promptCache: null, serviceTier: null });
  });
});

describe("requestOptionShortLabel", () => {
  it("upper-cases lifetimes and capitalizes tier names", () => {
    expect(requestOptionShortLabel("5m")).toBe("5M");
    expect(requestOptionShortLabel("priority")).toBe("Priority");
  });
});
