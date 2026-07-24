import { describe, expect, it } from "vitest";
import type { LuaSkillConfigSummary } from "@/types";
import { hasLuaSkillConfigOptions } from "./lua-skill-card";

// configSummary builds one complete configuration summary fixture with a caller-selected item count.
// configSummary 构建一份由调用方指定配置项数量的完整配置摘要夹具。
//
// The itemCount parameter represents the number of declarations projected by the service.
// itemCount 参数表示服务端投影出的声明数量。
//
// Returns a valid LuaSkillConfigSummary used by visibility tests.
// 返回供可见性测试使用的合法 LuaSkillConfigSummary。
function configSummary(itemCount: number): LuaSkillConfigSummary {
  return {
    complete: true,
    revision: "1",
    storeScope: "skills",
    itemCount,
    missingCount: 0,
    invalidCount: 0,
    businessIssueCount: 0,
    orphanedCount: 0,
  };
}

describe("Lua Skill configuration action visibility", () => {
  it("hides configuration actions when the catalog has no configuration summary", () => {
    expect(hasLuaSkillConfigOptions({ config: null })).toBe(false);
  });

  it("hides configuration actions when the package declares zero items", () => {
    expect(hasLuaSkillConfigOptions({ config: configSummary(0) })).toBe(false);
  });

  it("shows configuration actions when the package declares at least one item", () => {
    expect(hasLuaSkillConfigOptions({ config: configSummary(1) })).toBe(true);
  });
});
