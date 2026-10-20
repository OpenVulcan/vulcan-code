import { describe, expect, it } from "vitest";
import { MEMORY_MODE_SECTION, memorySectionForMode } from "./memory-mode-store";

describe("memorySectionForMode", () => {
  it("keeps the requested section while the mode is loading", () => {
    expect(memorySectionForMode("content", null)).toBe("content");
    expect(memorySectionForMode("md_space", null)).toBe("md_space");
  });

  it("shows only the mode settings when memory is off", () => {
    expect(memorySectionForMode("content", "off")).toBe(MEMORY_MODE_SECTION);
    expect(memorySectionForMode("md_user", "off")).toBe(MEMORY_MODE_SECTION);
  });

  it("maps VMM sections onto a Markdown scope in Markdown mode", () => {
    expect(memorySectionForMode("content", "markdown")).toBe("md_user");
    expect(memorySectionForMode("md_project", "markdown")).toBe("md_project");
  });

  it("maps Markdown scopes back to the VMM overview in VMM mode", () => {
    expect(memorySectionForMode("md_project", "vmm")).toBe("overview");
    expect(memorySectionForMode("rules", "vmm")).toBe("rules");
  });

  it("keeps the mode settings reachable in every mode", () => {
    for (const mode of ["off", "vmm", "markdown"] as const) {
      expect(memorySectionForMode(MEMORY_MODE_SECTION, mode)).toBe(MEMORY_MODE_SECTION);
    }
  });
});
