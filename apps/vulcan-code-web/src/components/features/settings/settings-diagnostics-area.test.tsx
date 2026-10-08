// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it } from "vitest";

import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import { I18nProvider } from "@/i18n";
import type { StartupSnapshot } from "@/types";
import { LogsArea } from "./settings-diagnostics-area";

// SETTINGS_ACTIONS supplies stable no-op service callbacks for diagnostic-page rendering.
// SETTINGS_ACTIONS 为诊断页面渲染提供稳定的空操作服务回调。
const SETTINGS_ACTIONS = {
  onSavePromptOutputStyle: async () => undefined,
  onSavePromptAnswerMode: async () => undefined,
  onSaveGitCommitSettings: async () => undefined,
  onSaveProviderRetrySettings: async () => undefined,
  onSaveProviderRequestTraceSettings: async () => undefined,
  onOpenSelectedProviderApiTraceDirectory: async () => undefined,
  onClearSelectedProviderApiTraceDirectory: async () => false,
  onRestoreArchivedConversations: async () => undefined,
  onDeleteArchivedConversations: async () => undefined,
} satisfies SettingsActionProps;

// STARTUP_SNAPSHOT contains the exact diagnostics fields rendered by LogsArea.
// STARTUP_SNAPSHOT 包含 LogsArea 渲染的精确诊断字段。
const STARTUP_SNAPSHOT = {
  steps: [],
  recentLogs: [],
} as unknown as StartupSnapshot;

describe("runtime diagnostics provider request trace setting", () => {
  // Verifies the system setting is visibly off by default and explains its sensitive session-local output.
  // 验证系统设置默认明确关闭，并说明其包含敏感内容的会话本地输出位置。
  it("opens tracing on demand with its disabled state and privacy explanation", async () => {
    // Use the real directory action so unopened categories are not mistaken for missing settings.
    // 使用真实目录操作，避免将未打开分类误判为设置缺失。
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
          startup={STARTUP_SNAPSHOT}
        />
      </I18nProvider>,
    ));
    // The first view contains startup diagnostics without the unrelated trace form.
    // 初始视图仅展示启动诊断，不混入无关追踪表单。
    expect(host.textContent).not.toContain("打开记录文件夹");
    const tab = Array.from(host.querySelectorAll<HTMLButtonElement>('.workspace-directory-navigation button')).find((entry) => entry.textContent === "模型请求记录");
    expect(tab).toBeDefined();
    await act(async () => tab!.click());
    expect(tab!.getAttribute("aria-current")).toBe("page");
    expect(host.querySelector('.workspace-directory-content .provider-trace-settings')).not.toBeNull();
    const markup = host.innerHTML;

    expect(markup).toContain("模型请求记录");
    expect(markup).toContain("已关闭");
    expect(markup).toContain("temp/provider-api-trace");
    expect(markup).toContain("原始内容");
    expect(markup).toContain("打开记录文件夹");
    expect(markup).toContain("清空当前会话的记录");
    expect(markup).toContain('data-state="unchecked"');
    await act(async () => root.unmount());
    host.remove();
  });
});
