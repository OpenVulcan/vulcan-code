import { describe, expect, it } from "vitest";

import {
  filterModelPickerChoices,
  groupModelPickerChoices,
  modelAccountEffectiveProtocolId,
  modelAccountProtocolDisplayName,
  modelPickerChoiceKey,
  modelPickerChoicesFromProviders,
  resolveModelPickerDisplayedChoice,
  resolveModelPickerGroupChoice,
} from "./model-picker-control";
import type { ProviderSummary } from "@/types";

// miniMaxProviders builds two credential-scoped rows exposing one shared model.
// miniMaxProviders 构造两个暴露同一模型的凭据作用域行。
function miniMaxProviders(): ProviderSummary[] {
  return [
    {
      id: "connection-cn",
      sourceProviderId: "minimax",
      connectionId: "connection-cn",
      accountName: "MiniMaxCN",
      name: "MiniMax · MiniMaxCN",
      providerDisplayName: "MiniMax",
      status: "目录可用",
      models: [{
        id: "MiniMax-M2.1",
        name: "MiniMax-M2.1",
        active: false,
        preferredConnection: true,
      }],
    },
    {
      id: "connection-api",
      sourceProviderId: "minimax",
      connectionId: "connection-api",
      accountName: "MiniMaxCN-API",
      name: "MiniMax · MiniMaxCN-API",
      providerDisplayName: "MiniMax",
      status: "目录可用",
      models: [{
        id: "MiniMax-M2.1",
        name: "MiniMax-M2.1",
        active: false,
        preferredConnection: false,
      }],
    },
  ];
}

describe("model picker account grouping", () => {
  it("collapses account-specific rows into one provider model", () => {
    // choices preserves the exact credential choices before visual grouping.
    // choices 在视觉聚合前保留精确凭据选项。
    const choices = modelPickerChoicesFromProviders(miniMaxProviders());
    // groups represents the single provider/model row rendered by the composer.
    // groups 表示输入区渲染的单个 Provider/模型行。
    const groups = groupModelPickerChoices(choices);

    expect(choices).toHaveLength(2);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.accounts.map((choice) => choice.accountName)).toEqual([
      "MiniMaxCN",
      "MiniMaxCN-API",
    ]);
    expect(groups[0]?.accounts.find((choice) => choice.preferredConnection)?.connectionId)
      .toBe("connection-cn");
    expect(resolveModelPickerGroupChoice(groups[0]!, undefined).connectionId).toBe("connection-cn");
  });

  // Verifies the picker receives exact route-native media and search declarations from the backend summary.
  // 验证选择器从后端摘要接收精确路由原生的媒体与搜索声明。
  it("projects built-in capabilities from the exact credential model route", () => {
    const providers = miniMaxProviders();
    providers[0]!.models![0]!.inputModalities = ["image", "video"];
    providers[0]!.models![0]!.providerNativeTools = ["web_search"];

    const choice = modelPickerChoicesFromProviders(providers)[0]!;
    expect(choice.inputModalities).toEqual(["image", "video"]);
    expect(choice.providerNativeTools).toEqual(["web_search"]);
  });

  // Verifies grouped rows display the account that selection will actually activate instead of a capability union.
  // 验证聚合行展示选择操作实际激活的账号，而不是能力并集。
  it("keeps built-in capabilities scoped to the resolved account", () => {
    const providers = miniMaxProviders();
    providers[1]!.models![0]!.providerNativeTools = ["web_search"];
    const choices = modelPickerChoicesFromProviders(providers);
    const group = groupModelPickerChoices(choices)[0]!;

    expect(resolveModelPickerGroupChoice(group, undefined).providerNativeTools).toEqual([]);
    expect(resolveModelPickerGroupChoice(group, choices[1]).providerNativeTools).toEqual([
      "web_search",
    ]);
  });

  // Verifies a new-session model click follows the backend model-account preference instead of a stale global route.
  // 验证新会话点击模型时遵循后端模型账号偏好，而不是沿用陈旧的全局路由。
  it("prefers the model account binding for a new session", () => {
    const choices = modelPickerChoicesFromProviders(miniMaxProviders());
    const group = groupModelPickerChoices(choices)[0]!;

    expect(resolveModelPickerGroupChoice(group, choices[1], true).connectionId)
      .toBe("connection-cn");
    expect(resolveModelPickerGroupChoice(group, choices[1], false).connectionId)
      .toBe("connection-api");
  });

  it("filters by model, provider and exact account labels", () => {
    // choices carries provider, account, and model labels used to prove the filter boundary.
    // choices 携带供应商、账号与模型标签，用于证明过滤边界。
    const choices = modelPickerChoicesFromProviders(miniMaxProviders());

    expect(filterModelPickerChoices(choices, "m2.1")).toHaveLength(2);
    expect(filterModelPickerChoices(choices, "minimaxcn")).toHaveLength(2);
    expect(filterModelPickerChoices(choices, "connection-api")).toHaveLength(1);
    expect(filterModelPickerChoices(choices, "provider")).toHaveLength(0);
  });

  it("degrades a removed account to the model preferred account", () => {
    // choices contains only accounts that remain available after credential removal.
    // choices 仅包含凭据移除后仍然可用的账号。
    const choices = modelPickerChoicesFromProviders(miniMaxProviders());
    // group is the authoritative current account set for the model.
    // group 是该模型当前权威的账号集合。
    const group = groupModelPickerChoices(choices)[0]!;
    // removedChoice simulates a session value that referenced a deleted credential.
    // removedChoice 模拟引用已删除凭据的会话值。
    const removedChoice = {
      ...choices[0]!,
      connectionId: "connection-removed",
      accountName: "Removed",
    };

    expect(resolveModelPickerGroupChoice(group, removedChoice).connectionId).toBe("connection-cn");
  });

  it("keeps same-model conversation accounts as distinct selection keys", () => {
    // choices exposes two credentials for the same provider model.
    // choices 暴露同一 Provider 模型的两个凭据。
    const choices = modelPickerChoicesFromProviders(miniMaxProviders());
    // firstKey and secondKey prove that conversation restoration includes the connection identity.
    // firstKey 与 secondKey 证明会话恢复包含连接身份。
    const firstKey = modelPickerChoiceKey(choices[0]);
    const secondKey = modelPickerChoiceKey(choices[1]);

    expect(choices[0]?.providerId).toBe(choices[1]?.providerId);
    expect(choices[0]?.modelId).toBe(choices[1]?.modelId);
    expect(firstKey).not.toBe(secondKey);
    expect(JSON.parse(firstKey)[2]).toBe(choices[0]?.connectionId);
    expect(JSON.parse(secondKey)[2]).toBe(choices[1]?.connectionId);
  });

  // Verifies an unavailable exact session route never renders another Provider or account as selected.
  // 验证不可用的精确会话路由绝不会把其他 Provider 或账号渲染为已选。
  it("requires reselection instead of displaying the first available route", () => {
    // choices contains an active row that would previously impersonate an unavailable exact target.
    // choices 包含一条激活行，旧逻辑会让它冒充不可用的精确目标。
    const choices = modelPickerChoicesFromProviders(miniMaxProviders());
    choices[0]!.active = true;
    // unavailableValue names a different exact Provider, model, and credential tuple.
    // unavailableValue 命名另一个精确 Provider、模型与凭据三元组。
    const unavailableValue = JSON.stringify([
      "command-code",
      "deepseek-v4-flash",
      "command-code-account",
    ]);

    expect(resolveModelPickerDisplayedChoice(choices, unavailableValue)).toBeUndefined();
    expect(resolveModelPickerDisplayedChoice(choices, "")?.connectionId).toBe("connection-cn");
  });
});

describe("modelAccountProtocolDisplayName", () => {
  // Verifies the account card presents protocol semantics instead of provider implementation metadata.
  // 验证账号卡片展示协议语义，而不是供应商实现元数据。
  it("formats known protocols and preserves future identifiers", () => {
    expect(modelAccountProtocolDisplayName("anthropic-messages.v1")).toBe("Anthropic Messages");
    expect(modelAccountProtocolDisplayName("openai-responses.v1")).toBe("OpenAI Responses");
    expect(modelAccountProtocolDisplayName("future-protocol.v1")).toBe("future-protocol.v1");
  });
});

describe("modelAccountEffectiveProtocolId", () => {
  // Verifies one shared credential displays the exact selected model route from backend authority.
  // 验证共享凭据展示后端权威提供的精确选中模型路由。
  it("uses the backend provider-account-model route as its sole authority", () => {
    // choice identifies the Responses-bound model on one multi-protocol account.
    // choice 标识一个多协议账号上绑定 Responses 的模型。
    const choice = modelPickerChoicesFromProviders(miniMaxProviders())[0]!;
    expect(modelAccountEffectiveProtocolId({
      modelRouteRecords: [
        {
          providerId: choice.providerId,
          connectionId: choice.connectionId,
          serviceEntryId: "official-api",
          modelId: choice.modelId,
          protocolId: "openai-responses.v1",
          availableProtocols: ["openai-responses.v1", "anthropic-messages.v1"],
          bindingSource: "system_manifest",
          bindingRuleId: "responses-model",
          manifestRevision: 1,
          executable: true,
        },
        {
          providerId: choice.providerId,
          connectionId: choice.connectionId,
          serviceEntryId: "official-api",
          modelId: "another-model",
          protocolId: "anthropic-messages.v1",
          availableProtocols: ["anthropic-messages.v1"],
          bindingSource: "system_manifest",
          bindingRuleId: "anthropic-model",
          manifestRevision: 1,
          executable: true,
        },
      ],
    }, choice)).toBe("openai-responses.v1");
  });

  // Verifies missing or explicitly blocked backend evidence never falls back to a connection protocol.
  // 验证缺失或被后端显式阻止的证据绝不回退到连接协议。
  it("fails closed when the exact model route is absent or blocked", () => {
    const choice = modelPickerChoicesFromProviders(miniMaxProviders())[0]!;
    expect(modelAccountEffectiveProtocolId({ modelRouteRecords: [] }, choice)).toBeNull();
    expect(modelAccountEffectiveProtocolId({
      modelRouteRecords: [{
        providerId: choice.providerId,
        connectionId: choice.connectionId,
        serviceEntryId: "official-api",
        modelId: choice.modelId,
        availableProtocols: ["anthropic-messages.v1"],
        manifestRevision: 2,
        executable: false,
        unavailableReason: "No executable model-level binding.",
      }],
    }, choice)).toBeNull();
  });
});
