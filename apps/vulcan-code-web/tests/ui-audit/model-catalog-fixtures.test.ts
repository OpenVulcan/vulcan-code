import { describe, expect, it } from "vitest";
import { createAuditPublicAi } from "./fixtures";
import { buildProviderModelGroups } from "../../src/components/features/settings/provider-model-routes";

describe("synthetic model-management route projection", () => {
  // The production grouping function proves these records reach the actual model list rather than only a chat picker.
  // 生产分组函数证明这些记录确实进入模型列表，而不只是聊天选择器。
  it("projects long-list providers and preserves protocol/account access distinctions", () => {
    const groups = buildProviderModelGroups(createAuditPublicAi("long-list"));
    expect(groups).toHaveLength(24);
    expect(groups[0].models.size).toBe(35);
    const routes = groups[0].models.get("audit-chat-1")!;
    expect(routes).toHaveLength(2);
    expect(routes[0]).toMatchObject({ protocolId: "openai-chat-completions.v1", accountCount: 2 });
    expect(routes[0].accounts).toHaveLength(3);
    expect(routes[0].accounts.find((account) => account.connectionId === "audit-account-1-restricted")?.enabled).toBe(false);
    expect(routes[1]).toMatchObject({ protocolId: "openai-responses.v1", accountCount: 1 });
  });
  // Empty and invalid-default retain their original acceptance purpose after route projection is added.
  // 增加路由投影后，空场景与失效默认场景保留原验收目的。
  it("keeps empty empty and invalid-default unavailable while exposing manageable models", () => {
    expect(buildProviderModelGroups(createAuditPublicAi("empty"))).toEqual([]);
    const snapshot = createAuditPublicAi("invalid-default");
    expect(buildProviderModelGroups(snapshot)).toHaveLength(2);
    expect(snapshot.active?.model_id).toBe("deleted-default-model");
    expect(snapshot.mediaUnderstandingSelections?.image_understand?.mode).toBe("model_replacement");
  });
});
