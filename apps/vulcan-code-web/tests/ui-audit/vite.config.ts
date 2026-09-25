import { defineConfig, normalizePath, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";
import { auditPluginAssets } from "./plugin-assets.server";

// Audit aliases replace every known business/discovery singleton before production module evaluation.
// 验收别名在生产模块求值前替换所有已知业务与发现单例。
export const auditAliases = [
  { find: "@/service/webui-service-runtime", replacement: path.resolve(__dirname, "runtime.ts") },
  { find: "@/service/browser-service-discovery", replacement: path.resolve(__dirname, "discovery.ts") },
  { find: "@/service/browserTransport", replacement: path.resolve(__dirname, "discovery.ts") },
  { find: "@/host-shell/client", replacement: path.resolve(__dirname, "shell.ts") },
  { find: "@", replacement: path.resolve(__dirname, "../../src") },
];
// auditStyleSources explicitly scans every production source before any lazy page is opened.
// auditStyleSources 在打开任何懒加载页面前明确扫描全部生产源码。
// Returns: a test-only transform of the existing stylesheet; no production file is modified.
// 返回值：对既有样式表的测试专用转换；不修改生产文件。
function auditStyleSources(): Plugin {
  // owner is the exact lazy workbench importer of the production Tailwind entry.
  // owner 是生产 Tailwind 入口的准确懒加载工作台导入者。
  const owner = normalizePath(path.resolve(__dirname, "../../src/app/workbench-app.tsx"));
  return { name: "ui-audit-explicit-style-sources", enforce: "pre", resolveId(source, importer) {
    if (source !== "../styles.css" || !importer || normalizePath(importer.split("?")[0]) !== owner) return null;
    return path.resolve(__dirname, "app.css");
  } };
}
// This server has no service-discovery plugin, RPC proxy or production runtime alias fallback.
// 此服务器没有服务发现插件、RPC 代理或生产运行时别名回退。
export default defineConfig({
  root: __dirname, plugins: [auditStyleSources(), auditPluginAssets(), react(), tailwindcss()], resolve: { alias: auditAliases },
  // Serve the same brand assets as the production Vite root.
  // 提供与生产 Vite 根目录相同的品牌资源。
  publicDir: path.resolve(__dirname, "../../public"),
  worker: { format: "es" },
  server: { host: "127.0.0.1", port: 17618, strictPort: true, hmr: false },
  build: { outDir: "dist", emptyOutDir: true, rollupOptions: { input: { index: path.resolve(__dirname, "index.html"), app: path.resolve(__dirname, "app.html") } } },
});
