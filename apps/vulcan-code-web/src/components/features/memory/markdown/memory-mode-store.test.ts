import { describe, expect, it, vi } from "vitest";
import { loadMemorySettings, saveMemorySettings } from "@/service/markdown-memory-service";
import type { MemorySettingsView } from "@/types";
import { applyMemoryMode, MEMORY_MODE_SECTION, memorySectionForMode } from "./memory-mode-store";

vi.mock("@/service/markdown-memory-service", () => ({
  loadMemorySettings: vi.fn(),
  saveMemorySettings: vi.fn(),
}));

describe("memorySectionForMode", () => {
  it("keeps the requested section while the mode is loading", () => {
    expect(memorySectionForMode("content", null)).toBe("content");
    expect(memorySectionForMode("md_space", null)).toBe("md_space");
  });

  it("opens model and mode settings from content navigation when memory is off", () => {
    expect(memorySectionForMode("content", "off")).toBe("inference");
    expect(memorySectionForMode("md_user", "off")).toBe("inference");
  });

  it("maps VMM content onto a Markdown scope in Markdown mode", () => {
    expect(memorySectionForMode("content", "markdown")).toBe("md_user");
    expect(memorySectionForMode("md_project", "markdown")).toBe("md_project");
  });

  it("maps Markdown scopes back to the VMM overview in VMM mode", () => {
    expect(memorySectionForMode("md_project", "vmm")).toBe("overview");
    expect(memorySectionForMode("rules", "vmm")).toBe("rules");
  });

  it.each(["vmm", "markdown", "off"] as const)("keeps model and mode settings reachable in %s", (mode) => {
    expect(memorySectionForMode("inference", mode)).toBe("inference");
    expect(memorySectionForMode(MEMORY_MODE_SECTION, mode)).toBe("inference");
  });
});

// Verify the one-click switch uses the persisted budget and leaves other settings alone.
// 验证一键切换使用已持久化预算，并保留其他设置。
it("uses the saved Markdown budget when switching modes", async () => {
  // Give each budget part a distinct value so any accidental reset is visible.
  // 为每个预算部分设置不同值，以便发现意外重置。
  const budget = { user: 150, spaceBase: 250, projectBase: 350, spaceCatalog: 450, projectCatalog: 550 };
  // Simulate a fresh read from the same service used by the full settings page.
  // 模拟从完整设置页共用的服务重新读取设置。
  const settings: MemorySettingsView = { mode: "vmm", budget, defaultBudget: budget, maxPartTokens: 2000, warningPercent: 80 };
  vi.mocked(loadMemorySettings).mockResolvedValueOnce(settings);
  vi.mocked(saveMemorySettings).mockResolvedValueOnce({ ...settings, mode: "markdown" });

  await applyMemoryMode("markdown");

  expect(saveMemorySettings).toHaveBeenCalledExactlyOnceWith({ mode: "markdown", budget });
});
