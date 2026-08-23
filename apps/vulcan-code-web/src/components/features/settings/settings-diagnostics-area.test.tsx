import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import { I18nProvider } from "@/i18n";
import type { StartupSnapshot } from "@/types";
import { LogsArea } from "./settings-diagnostics-area";

// SETTINGS_ACTIONS supplies stable no-op service callbacks for diagnostic-page rendering.
// SETTINGS_ACTIONS 为诊断页面渲染提供稳定的空操作服务回调。
const SETTINGS_ACTIONS = {
  onSavePromptOutputStyle: async () => undefined,
  onSaveGitCommitSettings: async () => undefined,
  onSaveProviderRetrySettings: async () => undefined,
  onSaveProviderRequestTraceSettings: async () => undefined,
  onSaveMcpServer: async () => undefined,
  onRemoveMcpServer: async () => undefined,
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
  it("renders the bounded wire-request trace switch as disabled by default", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <LogsArea
          actions={SETTINGS_ACTIONS}
          logs={[]}
          providerRequestTraceEnabled={false}
          startup={STARTUP_SNAPSHOT}
        />
      </I18nProvider>,
    );

    expect(markup).toContain("Provider 请求追踪");
    expect(markup).toContain("已关闭");
    expect(markup).toContain("temp/provider-request-trace");
    expect(markup).toContain('data-state="unchecked"');
  });
});
