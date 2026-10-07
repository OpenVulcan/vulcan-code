import { expect, it } from "vitest";
import { parsePluginUninstallStatus, type PluginUninstallStatus } from "./plugin-uninstall-contract";

// original is a retained request independent from any currently published plugin row.
// original 是保留请求，独立于任何当前已发布插件行。
const original: PluginUninstallStatus = { requestId: "12345678-1234-4234-8234-123456789abc", pluginId: "hidden-plugin",
  phase: "collection", requiresExternalEffectsAcknowledgement: false };

// The declared fields pass through unchanged while undeclared response content is dropped.
// 声明字段原样通过，未声明响应内容被丢弃。
it("keeps the declared original fields and drops undeclared content", () => {
  expect(parsePluginUninstallStatus({ ...original, callback: "private-result" }, original.pluginId)).toEqual(original);
});

// Every required field is strict; aliases, unknown phases and a foreign owner cannot grant continuation.
// 每个必需字段均严格校验；别名、未知阶段及其他所有者不能授予继续。
it.each([null, [], {}, { ...original, requestId: "unknown" }, { ...original, phase: "committed" },
  { ...original, pluginId: "other" }, { ...original, requiresExternalEffectsAcknowledgement: undefined, acknowledge: true }])(
  "rejects malformed original observations %#", (value) => {
    expect(() => parsePluginUninstallStatus(value, original.pluginId)).toThrow("Invalid plugin uninstallation");
  });
