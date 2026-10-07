// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { readLocaleMessage, SUPPORTED_LOCALES } from "@/i18n";
import { ensureLocaleCatalog, loadedLocaleCatalog } from "@/i18n-catalogs";
import type { PluginSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";
import { VulcanPluginsPanel } from "./vulcan-plugins-panel";

// Render only the management status boundary in English; no native browser or plugin process is started.
// 仅以英语渲染管理状态边界；不启动原生浏览器或插件进程。
vi.mock("@/i18n", async (importOriginal) => {
  // Preserve real catalogs so language coverage checks cannot pass through mocked translations.
  // 保留真实语言表，避免语言覆盖检查通过模拟翻译蒙混过关。
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ locale: "en-US", t: (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("en-US", key) }) };
});
vi.mock("@/service/core-browser-service", () => ({ getCoreBrowserRuntimeStatus: vi.fn().mockResolvedValue({ ready: true }) }));

// The descriptor keeps plugin enablement independent from the supplied MCP connection status.
// 描述符让插件启用与传入的 MCP 连接状态相互独立。
// Input is a service status key; returns an enabled browser plugin with one projected MCP row.
// 输入为服务状态键；返回包含一行 MCP 投影的已启用浏览器插件。
function browserPlugin(status: string): PluginSummary {
  return {
    id: "vulcan-browser", name: "Browser", kind: "plugin", runtimeState: "enabled",
    capability: "Browser tools", nameByLocale: {}, capabilityByLocale: {}, enabled: true, desiredEnabled: true,
    available: true, toolCount: 0, tools: [], hookCount: 0, skillCount: 0, agentCount: 0, activeSessionCount: 0,
    diagnostics: [], managementKind: "vulcanPlugin", extension: {
      runtimeIdentity: "vulcan-browser:5", origin: "bundled", permissions: [], hooks: [],
      actions: [], uiMounts: [], skills: [], agents: [], hasLuaRuntime: false, hasConfiguration: false,
      mcpServers: [{ id: "playwright", name: "Playwright Browser", transport: "stdio", endpoint: "playwright-mcp",
        status, generation: 5, retryCount: 0, toolCount: status === "ready" ? 2 : 0, promptCount: 0, resourceCount: 0,
        secretKeys: [], recentLogs: [], ...(status === "failed" ? { lastError: "fixture connection failed" } : {}) }],
    },
  };
}

// Only read operations needed by this panel are supplied, preventing accidental environment installation.
// 仅提供此面板所需的读取操作，防止意外安装环境。
const actions = {
  onPluginEnvironmentReady: async () => true,
  onPluginEnvironmentInstallStatusForPlugin: async () => null,
  onReadPluginPendingOperations: async () => [],
} as unknown as SkillActionProps;

it.each(["onDemand", "connecting", "ready", "failed", "draining", "stopped"] as const)(
  "renders MCP %s separately from enabled plugin and installed components", async (status) => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    // A detached test root isolates this regression from other plugin-management tests.
    // 独立测试根将此回归与其他插件管理测试隔离。
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    try {
      await act(async () => root.render(<VulcanPluginsPanel plugins={[browserPlugin(status)]} actions={actions} />));
      // Query the named MCP region, not badges or counts from unrelated component-readiness controls.
      // 查询命名 MCP 区域，不混入无关组件就绪控件的标签或数量。
      const region = host.querySelector(`section[aria-label="${readLocaleMessage("en-US", "skills.plugins.mcpServersAndCredentials")}"]`);
      expect(region?.textContent).toContain(readLocaleMessage("en-US", `skills.plugins.mcpStatus.${status}`));
      if (status === "onDemand") {
        expect(region?.textContent).not.toContain(readLocaleMessage("en-US", "skills.plugins.mcpStatus.stopped"));
      }
      if (status === "failed") expect(region?.querySelector('[role="alert"]')?.textContent).toContain("fixture connection failed");
    } finally {
      await act(async () => root.unmount());
      host.remove();
      vi.unstubAllGlobals();
    }
  },
);

it("defines the on-demand status in every supported language without English fallback", async () => {
  for (const { code } of SUPPORTED_LOCALES) {
    await ensureLocaleCatalog(code);
    expect(loadedLocaleCatalog(code)?.["skills.plugins.mcpStatus.onDemand"], code).toBeTruthy();
  }
});
