import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { I18nProvider } from "@/i18n";
import type { WorkbenchSnapshot } from "@/types";

import { FloatingSkillMenu } from "./workspace-skills-settings";

// noop accepts menu callbacks without introducing test behavior.
// noop 接收菜单回调且不引入测试行为。
function noop(): void {}

describe("FloatingSkillMenu", () => {
  // The ContextEngine entry is a fixed peer of LuaSkills settings, not plugin-management content.
  // ContextEngine 入口是 LuaSkills 设置的固定同级节点，而非插件管理内容。
  it("renders the fixed ContextEngine node before plugin management", () => {
    // markup captures the real expanded plugin-area menu in Chinese.
    // markup 捕获真实的中文展开插件功能区菜单。
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <FloatingSkillMenu
          activeSubArea="context-engine"
          expanded
          onExpandedChange={noop}
          onSelectSubArea={noop}
          workbench={{ plugins: [] } as unknown as WorkbenchSnapshot}
        />
      </I18nProvider>,
    );

    expect(markup).toContain("语义引擎");
    expect(markup.indexOf("语义引擎")).toBeLessThan(markup.indexOf("插件管理"));
  });
});
