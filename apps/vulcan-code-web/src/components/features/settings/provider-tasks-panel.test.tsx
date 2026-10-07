// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ProviderTaskSummary } from "@/types";
import { ProviderTasksPanel } from "./provider-tasks-panel";

// service mocks the task RPCs so the panel never reaches a real provider.
// service 模拟任务 RPC，使面板永不触达真实供应商。
const { service } = vi.hoisted(() => ({ service: {
  listProviderTasks: vi.fn(),
  cancelProviderTask: vi.fn(),
} }));
vi.mock("@/service/settings-actions-service", () => service);
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("en-US", key);
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});

// task builds one summary whose lifecycle fields each scenario overrides.
// task 构建一份摘要，各场景覆盖其生命周期字段。
function task(overrides: Partial<ProviderTaskSummary>): ProviderTaskSummary {
  return {
    executionId: "exec-1",
    ownerSessionId: "session-1",
    capability: "video_generate",
    targetDisplayName: "Video model",
    providerId: "provider-1",
    connectionId: "connection-1",
    targetId: "video-target",
    phase: "running",
    terminal: false,
    cancellationSupported: true,
    createdAtUnixMs: 1_700_000_000_000,
    updatedAtUnixMs: 1_700_000_000_000,
    deadlineUnixMs: 1_700_000_600_000,
    consecutiveObservationFailures: 0,
    error: null,
    resultAvailable: false,
    ...overrides,
  };
}

// container and root own a fresh isolated client mount for every scenario.
// container 与 root 为每个场景持有新的隔离客户端挂载。
let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.clearAllMocks();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
});

// renderPanel mounts the panel and settles its initial list read.
// renderPanel 挂载面板并等待首次列表读取完成。
async function renderPanel(): Promise<void> {
  await act(async () => {
    root.render(<ProviderTasksPanel capabilityLabel={(capability) => capability} />);
  });
}

// cancelButtons returns every rendered cancellation control.
// cancelButtons 返回全部已渲染的取消控件。
function cancelButtons(): HTMLButtonElement[] {
  return Array.from(container.querySelectorAll<HTMLButtonElement>("button[aria-label^='Cancel task']"));
}

describe("ProviderTasksPanel", () => {
  it("offers cancellation only for live tasks whose provider supports it", async () => {
    service.listProviderTasks.mockResolvedValue([
      task({ executionId: "live" }),
      task({ executionId: "unsupported", cancellationSupported: false }),
      task({ executionId: "cancelling", phase: "cancellation_requested" }),
      task({ executionId: "done", phase: "completed", terminal: true }),
    ]);

    await renderPanel();

    expect(container.querySelectorAll("li")).toHaveLength(4);
    expect(cancelButtons()).toHaveLength(1);
  });

  it("cancels with the listed owner and re-reads the list", async () => {
    service.listProviderTasks.mockResolvedValue([task({})]);
    service.cancelProviderTask.mockResolvedValue(task({ phase: "cancelled", terminal: true }));

    await renderPanel();
    await act(async () => {
      cancelButtons()[0].click();
    });

    expect(service.cancelProviderTask).toHaveBeenCalledWith(
      expect.objectContaining({ executionId: "exec-1", ownerSessionId: "session-1" }),
    );
    expect(service.listProviderTasks).toHaveBeenCalledTimes(2);
  });

  it("shows the empty state when no task is retained", async () => {
    service.listProviderTasks.mockResolvedValue([]);

    await renderPanel();

    expect(container.querySelector("li")).toBeNull();
    expect(container.querySelector(".ai-capabilities-message")).not.toBeNull();
  });
});
