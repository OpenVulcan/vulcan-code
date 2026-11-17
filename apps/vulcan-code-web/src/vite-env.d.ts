/// <reference types="vite/client" />

// Describes the packaged browser skill and original tool names projected by the build.
// 描述由构建阶段投影的随包浏览器技能及原始工具名称。
declare const __VULCAN_BROWSER_ACTION_DISPLAY__: {
  // Namespaced prompt skill that owns the aliases.
  // 拥有该别名的带命名空间提示技能。
  skill: string;
  // Exact alias-to-card name lookup generated from the plugin manifest.
  // 从插件清单生成的精确别名到工具卡名称映射。
  actions: Readonly<Record<string, string>>;
};

// Declares imported SVG files as build-resolved asset URLs.
// 将导入的 SVG 文件声明为构建阶段解析的资源 URL。
declare module "*.svg" {
  const assetUrl: string
  export default assetUrl
}
