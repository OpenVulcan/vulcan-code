// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { VmmConfigDraftSummary } from "@/types";
import { ProjectVmmSection } from "./project-settings-sections";

// Only host IO and the independently tested business-policy editor are isolated here.
// 此处仅隔离宿主输入输出和已经独立测试的业务策略编辑器。
const { run } = vi.hoisted(() => ({ run: vi.fn() }));
vi.mock("@/service/memory-actions-service", () => ({ runVmmMemoryAction: run }));
vi.mock("@/components/features/memory/vmm-native-host-project-policy", () => ({ VmmNativeHostProjectPolicy: () => null }));

// A complete native draft carries inherited transport fields that must never appear as native configuration.
// 完整原生草稿携带继承的传输字段，这些字段绝不能作为原生配置显示。
const draft: VmmConfigDraftSummary = {
  hasOverride: true, backendMode: "native", nativeProjectBindingMode: "fixed", externalProjectBindingMode: "fixed",
  nativeFixedProjectId: "1", externalProjectPrefix: "vulcan/space", enabled: true, grpcTarget: "http://localhost:50051",
  grpcApiKey: "do-not-render", managementBaseUrl: "http://localhost:8080", managementBearerToken: "do-not-render",
  managementTimeoutMs: "30000", userId: "2", projectId: "1", receiveTimeoutMs: "30000", handshakeTimeoutMs: "10000",
  maxReceiveMessageBytes: "16777216", grpcKeepaliveTimeMs: "30000", grpcKeepaliveTimeoutMs: "10000", grpcKeepalivePermitWithoutCalls: false,
  aiToolsEnabled: true, precheckEnabled: true, memoryWritebackEnabled: true, precheckMode: "inherit", memoryWritebackMode: "inherit",
  implicitMemoryTurns: "8", profileRefreshTurns: "24", profileNodeLimit: "64", injectionTag: "memory", injectionDescription: "description",
};

describe("project native memory controls", () => {
  it("keeps transport details out of native UI and serializes binding mutations with retry after failure", async () => {
    // The catalog declares one exact owner and a second recall-only candidate.
    // 目录声明一个精确归属项目及第二个仅召回候选。
    const catalog = { summary: "已保存", data: { kind: "native_project_catalog", value: {
      projects: [{ projectId: 1, name: "Primary" }, { projectId: 2, name: "Related" }],
      bindings: [{ vulcanProjectId: "project-a", ownerProjectId: 1, linkedProjectIds: [] }],
    } } };
    // A pending rejected mutation exposes same-batch duplicates and verifies the guard is released after failure.
    // 待拒绝的修改暴露同批次重复提交，并验证失败后释放同步锁。
    let rejectSave!: (error: Error) => void;
    const save = new Promise((_, reject) => { rejectSave = reject; });
    run.mockReset().mockRejectedValueOnce(new Error("catalog unavailable")).mockResolvedValueOnce(catalog).mockReturnValueOnce(save).mockResolvedValue(catalog);
    const host = document.createElement("div");
    document.body.appendChild(host);
    const root = createRoot(host);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    try {
      await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><ProjectVmmSection
        projectId="project-a" vmmDraft={draft} vmmBusy="idle" vmmDirty={false} onUpdateDraft={vi.fn()} onSaveDraft={vi.fn(async () => true)}
      /></I18nProvider>));
      expect(host.textContent).not.toContain("外置连接");
      expect(host.innerHTML).not.toContain("do-not-render");
      expect(host.querySelector(".project-vmm-advanced")?.hasAttribute("open")).toBe(false);
      expect(host.querySelectorAll('[role="switch"]').length).toBe(3);
      const saveButton = Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.includes("保存原生绑定"))!;
      expect(saveButton.disabled).toBe(true);
      expect(host.querySelector('[role="alert"]')?.textContent).toContain("catalog unavailable");
      const reload = Array.from(host.querySelectorAll("button")).find((button) => button.textContent === "重新加载")!;
      await act(async () => reload.click());
      expect(saveButton.disabled).toBe(false);
      await act(async () => { saveButton.click(); saveButton.click(); });
      expect(run.mock.calls.filter(([request]) => request.action === "native_project_set_access")).toHaveLength(1);
      expect(run.mock.calls[2][0].payload).toEqual({ vulcanProjectId: "project-a", ownerProjectId: 1, linkedProjectIds: [] });
      expect(saveButton.disabled).toBe(true);
      await act(async () => rejectSave(new Error("binding unavailable")));
      expect(host.querySelector('[role="status"]')?.textContent).toContain("binding unavailable");
      expect(saveButton.disabled).toBe(false);
      await act(async () => saveButton.click());
      expect(run.mock.calls.filter(([request]) => request.action === "native_project_set_access")).toHaveLength(2);
    } finally {
      await act(async () => root.unmount());
      host.remove();
      vi.unstubAllGlobals();
    }
  });
});
