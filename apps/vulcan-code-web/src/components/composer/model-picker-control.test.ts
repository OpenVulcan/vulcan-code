import { describe, expect, it } from "vitest";

import {
  filterModelPickerChoices,
  groupModelPickerChoices,
  modelAccountProtocolDisplayName,
  modelPickerChoiceKey,
  modelPickerChoicesFromProviders,
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

  it("filters only by model name or model id", () => {
    // choices carries provider, account, and model labels used to prove the filter boundary.
    // choices 携带供应商、账号与模型标签，用于证明过滤边界。
    const choices = modelPickerChoicesFromProviders(miniMaxProviders());

    expect(filterModelPickerChoices(choices, "m2.1")).toHaveLength(2);
    expect(filterModelPickerChoices(choices, "minimaxcn")).toHaveLength(0);
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
