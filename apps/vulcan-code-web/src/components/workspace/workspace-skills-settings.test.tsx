import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { I18nProvider } from "@/i18n";
import type { WorkbenchSnapshot } from "@/types";

import { PluginWorkspaceMenu } from "./workspace-skills-settings";

// noop accepts menu callbacks without introducing test behavior.
// noop 接收菜单回调且不引入测试行为。
function noop(): void {}

describe("PluginWorkspaceMenu", () => {
  // Plugin management and the installed plugin list retain their navigation order.
  // 插件管理与已安装插件列表保持既有导航顺序。
  it("renders plugin management before the installed plugin list", () => {
    // markup captures the real expanded plugin-area menu in Chinese.
    // markup 捕获真实的中文展开插件功能区菜单。
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <PluginWorkspaceMenu
          activeSubArea="plugins"
          onSelectSubArea={noop}
          workbench={{ plugins: [] } as unknown as WorkbenchSnapshot}
        />
      </I18nProvider>,
    );

    expect(markup).toContain('aria-label="内置插件与管理"');
    expect(markup).toContain('aria-label="插件列表"');
    expect(markup.indexOf("插件管理")).toBeLessThan(markup.indexOf('aria-label="插件列表"'));
  });
});
