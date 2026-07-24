import { describe, expect, it } from "vitest";

import type { LuaSkillConfigDescriptor, LuaSkillConfigItem } from "@/types";
import {
  draftFromDescriptor,
  groupConfigItems,
  orderedConfigItems,
  parseDraftValue,
} from "./lua-skill-config-dialog";

// configItemFixture builds one complete declaration with explicit test overrides.
// configItemFixture 使用显式测试覆盖构建一份完整配置声明。
//
// Parameters: overrides contains only fields relevant to the current test.
// 参数：overrides 仅包含当前测试关注的字段。
// Returns: one structurally complete host-safe configuration item.
// 返回值：一个结构完整的宿主安全配置项。
function configItemFixture(overrides: Partial<LuaSkillConfigItem>): LuaSkillConfigItem {
  return {
    key: "endpoint",
    type: "string",
    required: false,
    sensitive: false,
    description: "",
    constraints: {},
    options: [],
    advanced: false,
    restartRequired: false,
    deprecated: false,
    state: "unset",
    satisfied: true,
    value: null,
    ...overrides,
  };
}

// configDescriptorFixture builds one host-safe descriptor around exact declaration rows.
// configDescriptorFixture 围绕精确声明行构建一份宿主安全描述。
//
// Parameters: items is the declaration list under test.
// 参数：items 是待测试的声明列表。
// Returns: one complete descriptor with stable revision metadata.
// 返回值：一份带稳定修订元数据的完整描述。
function configDescriptorFixture(items: LuaSkillConfigItem[]): LuaSkillConfigDescriptor {
  return {
    skillId: "fixture",
    skillVersion: "1.0.0",
    complete: true,
    revision: "3",
    storeScope: "skills",
    missingCount: 0,
    invalidCount: 0,
    businessIssueCount: 0,
    orphanedCount: 0,
    orphaned: [],
    items,
  };
}

describe("Lua Skill configuration editor model", () => {
  it("never fills sensitive drafts and preserves non-sensitive canonical values", () => {
    const descriptor = configDescriptorFixture([
      configItemFixture({
        key: "api_token",
        sensitive: true,
        state: "configured",
        value: null,
      }),
      configItemFixture({
        key: "enabled",
        type: "boolean",
        state: "configured",
        value: "true",
      }),
      configItemFixture({
        key: "retries",
        type: "integer",
        state: "configured",
        value: "3",
      }),
    ]);

    expect(draftFromDescriptor(descriptor)).toEqual({
      api_token: "",
      enabled: true,
      retries: "3",
    });
  });

  it("rejects blank or unsafe numeric drafts and decodes the sensitive boolean tri-state", () => {
    const integerItem = configItemFixture({ key: "retries", type: "integer" });
    const booleanItem = configItemFixture({ key: "enabled", type: "boolean", sensitive: true });

    expect(() => parseDraftValue(integerItem, "")).toThrow("integer value required");
    expect(() => parseDraftValue(integerItem, "9007199254740992")).toThrow("safe integer required");
    expect(parseDraftValue(integerItem, "003")).toBe(3);
    expect(parseDraftValue(booleanItem, "true")).toBe(true);
    expect(parseDraftValue(booleanItem, "false")).toBe(false);
    expect(() => parseDraftValue(booleanItem, "")).toThrow("boolean value required");
  });

  it("keeps declarations stable by group, author order, and key", () => {
    const ordered = orderedConfigItems([
      configItemFixture({ key: "beta", group: "B", order: 1 }),
      configItemFixture({ key: "alpha-two", group: "A", order: 2 }),
      configItemFixture({ key: "alpha-one", group: "A", order: 1 }),
    ]);
    const groups = groupConfigItems(ordered);

    expect(groups.map((group) => group.label)).toEqual(["A", "B"]);
    expect(groups[0]?.items.map((item) => item.key)).toEqual(["alpha-one", "alpha-two"]);
    expect(groups[1]?.items.map((item) => item.key)).toEqual(["beta"]);
  });
});
