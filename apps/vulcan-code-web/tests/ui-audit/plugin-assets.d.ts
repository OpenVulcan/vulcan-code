declare module "virtual:ui-audit-plugin-assets" {
  // pluginAssets contains source-only plugin projections produced by the isolated Vite build.
  // pluginAssets 包含由隔离 Vite 构建生成的纯源码插件投影。
  export const pluginAssets: Record<string, {
    summary: import("../../src/types").PluginSummary;
    module: import("../../src/types").PluginUiModuleSource;
    schemas: Record<string, object>;
  }>;
}
