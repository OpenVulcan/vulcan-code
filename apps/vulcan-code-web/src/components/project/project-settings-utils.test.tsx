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
  // The code-context section must not exist until backend configuration is fully ready.
  // 后端配置完全就绪前，代码上下文分区必须不存在。
  it("fails closed with the authoritative global readiness flag", () => {
    const project = { pinned: false, vceEnabled: true } as ProjectSummary;

    const closed = projectSettingsSections(
      project,
      workbenchFixture(),
      false,
      translationFixture,
    );
    const ready = projectSettingsSections(
      project,
      workbenchFixture(),
      true,
      translationFixture,
    );

    expect(closed.some((section) => section.id === "code-context")).toBe(false);
    expect(ready.some((section) => section.id === "code-context")).toBe(true);
  });
});
