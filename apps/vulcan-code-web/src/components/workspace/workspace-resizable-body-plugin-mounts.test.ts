import { describe, expect, it } from "vitest";
import type { WorkbenchSnapshot } from "@/types";
import { selectConversationRightPluginMounts } from "./workspace-resizable-body";

// plugin builds the smallest service projection needed to exercise mount visibility.
// plugin 构造验证挂载可见性所需的最小服务投影。
function plugin(overrides: Partial<WorkbenchSnapshot["plugins"][number]> = {}): WorkbenchSnapshot["plugins"][number] {
  return {
    id: "vulcan-browser",
    version: "0.1.0",
    name: "Vulcan Browser",
    kind: "Vulcan Plugin",
    status: "enabled",
    runtimeState: "enabled",
    capability: "Core browser projection",
    enabled: true,
    desiredEnabled: true,
    available: true,
    nameByLocale: {},
    capabilityByLocale: {},
    toolCount: 0,
    tools: [],
    hookCount: 0,
    skillCount: 0,
    agentCount: 0,
    activeSessionCount: 0,
    diagnostics: [],
    managementKind: "vulcanPlugin",
    extension: {
      runtimeIdentity: "fixture:1",
      origin: "bundled",
      permissions: ["ui_host_context"],
      hooks: [],
      actions: [],
      uiMounts: [{
        id: "core-browser",
        point: "conversationRightPanel",
        label: "AI Browser",
        labelByLocale: {},
        icon: "plugin",
        kind: "module",
        exportName: "createPluginPanel",
      }],
      skills: [],
      agents: [],
      hasLuaRuntime: false,
      mcpServers: [],
    },
    ...overrides,
  };
}

describe("selectConversationRightPluginMounts", () => {
  it("exposes only enabled runnable conversation mounts", () => {
    expect(selectConversationRightPluginMounts([
      plugin(),
      plugin({ id: "disabled", enabled: false }),
      plugin({ id: "preparing", runtimeState: "preparing" }),
    ])).toEqual([{ plugin: plugin(), mount: plugin().extension!.uiMounts[0] }]);
  });

  it("keeps non-conversation mounts out of the right panel", () => {
    expect(selectConversationRightPluginMounts([
      plugin({ extension: { ...plugin().extension!, uiMounts: [{ ...plugin().extension!.uiMounts[0], point: "workspaceMain" }] } }),
    ])).toEqual([]);
  });
});
