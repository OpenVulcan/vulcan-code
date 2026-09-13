import { describe, expect, it } from "vitest";

import type { TranslationKey } from "@/i18n";
import type { ProjectSummary, WorkbenchSnapshot } from "@/types";

import { projectSettingsSections } from "./project-settings-utils";

// translationFixture preserves stable keys without requiring a rendered locale provider.
// translationFixture 保留稳定键名且无需渲染语言环境 Provider。
function translationFixture(key: TranslationKey): string {
  return key;
}

// workbenchFixture supplies only the authoritative fields consumed by project navigation.
// workbenchFixture 只提供项目导航实际消费的权威字段。
function workbenchFixture(): WorkbenchSnapshot {
  return {
    conversations: [],
    agentSkills: { records: [] },
    vmmMemory: { projectDraft: { hasOverride: false } },
  } as unknown as WorkbenchSnapshot;
}

describe("project VCE settings visibility", () => {
  // Project intent remains configurable without giving settings navigation runtime admission authority.
  // 项目意图保持可配置，设置导航不拥有运行时准入权威。
  it("keeps the semantic settings node for both disabled and enabled project intent", () => {
    const project = { pinned: false, vceEnabled: true } as ProjectSummary;

    const closed = projectSettingsSections(
      { ...project, vceEnabled: false },
      workbenchFixture(),
      translationFixture,
      null,
    );
    const ready = projectSettingsSections(
      project,
      workbenchFixture(),
      translationFixture,
      null,
    );

    expect(closed.find((section) => section.id === "code-context")?.detail).toBe("projectSettings.codeContext.disabled");
    expect(ready.some((section) => section.id === "code-context")).toBe(true);
  });

  // A settings dialog may target a different project from the selected conversation's memory snapshot.
  // 设置弹窗可能属于另一个项目，不能读取当前会话的记忆快照来决定其覆盖状态。
  it("uses only the dialog project's loaded memory authority", () => {
    const project = { id: "other-project", pinned: false, vceEnabled: false } as ProjectSummary;
    const workbench = workbenchFixture();
    expect(projectSettingsSections(project, workbench, translationFixture, true).find((section) => section.id === "vmm")?.detail).toBe("projectSettings.projectOverride");
    expect(projectSettingsSections(project, workbench, translationFixture, false).find((section) => section.id === "vmm")?.detail).toBe("projectSettings.inheritGlobal");
    expect(projectSettingsSections(project, workbench, translationFixture, null).find((section) => section.id === "vmm")?.detail).toBe("-");
  });
});
