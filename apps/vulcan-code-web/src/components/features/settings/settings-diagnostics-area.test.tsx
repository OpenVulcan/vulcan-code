// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it } from "vitest";

import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import { I18nProvider } from "@/i18n";
import { LogsArea } from "./settings-diagnostics-area";

// SETTINGS_ACTIONS supplies stable no-op service callbacks for diagnostic-page rendering.
// SETTINGS_ACTIONS 为诊断页面渲染提供稳定的空操作服务回调。
const SETTINGS_ACTIONS = {
  onSavePromptOutputStyle: async () => undefined,
  onSavePromptAnswerMode: async () => undefined,
  onSaveGitCommitSettings: async () => undefined,
  onSaveDefaultCommandShell: async () => undefined,
  onSaveProviderRetrySettings: async () => undefined,
  onSaveProviderRequestTraceSettings: async () => undefined,
  onOpenSelectedProviderApiTraceDirectory: async () => undefined,
  onClearSelectedProviderApiTraceDirectory: async () => false,
  onRestoreArchivedConversations: async () => undefined,
  onDeleteArchivedConversations: async () => undefined,
} satisfies SettingsActionProps;

describe("runtime diagnostics provider request trace setting", () => {
  // Verifies the trace controls sit beside the logs and read as off by default.
  // 验证追踪控制与日志同页，且默认明确显示为关闭。
  it("shows the trace controls beside the logs, off by default", async () => {
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    await act(async () => root.render(
      <I18nProvider initialLocale="zh-CN">
        <LogsArea
          actions={SETTINGS_ACTIONS}
          logs={[]}
          providerRequestTraceEnabled={false}
          providerErrorTraceEnabled={false}
        />
      </I18nProvider>,
    ));
    // Logs and the trace controls share one page, so the trace panel renders without navigation.
    // 日志与追踪控制同处一页，因此追踪面板无需导航即可渲染。
    expect(host.querySelector(".provider-trace-settings")).not.toBeNull();
    const markup = host.innerHTML;

    expect(markup).toContain("模型请求记录");
    expect(markup).toContain("已关闭");
    expect(markup).toContain("打开记录文件夹");
    expect(markup).toContain("清空当前会话的记录");
    expect(markup).toContain('data-state="unchecked"');
    await act(async () => root.unmount());
    host.remove();
  });
});
