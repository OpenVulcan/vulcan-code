import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

// The desktop startup page is the WebUI startup screen built on its own, so the desktop window can show it from local
// files before Core serves the WebUI. It runs after the main build, which empties `dist/`, and writes into
// `dist/desktop-startup/`; `scripts/desktop-runtime.mjs` copies that folder into the desktop app.
// 桌面启动页是单独构建的 WebUI 启动界面，使桌面窗口在 Core 提供 WebUI 之前即可从本地文件显示它。它在会清空 `dist/`
// 的主构建之后运行，写入 `dist/desktop-startup/`；`scripts/desktop-runtime.mjs` 将该目录复制进桌面应用。

// catalogsDir holds the language registry and the boot catalogs shared with the WebUI startup screen.
// catalogsDir 保存与 WebUI 启动界面共用的语言注册表与启动目录。
const catalogsDir = path.resolve(__dirname, "../../crates/vulcan-i18n/catalogs");

export default defineConfig({
  cacheDir: path.resolve(__dirname, "node_modules", ".vite-desktop-startup"),
  plugins: [react()],
  // Relative asset URLs keep the page loadable from a file URL.
  // 相对资源地址使页面可以从文件地址加载。
  base: "./",
  // The page uses none of the WebUI's public files.
  // 本页不使用 WebUI 的任何公共文件。
  publicDir: false,
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "@catalogs": catalogsDir,
    },
  },
  clearScreen: false,
  build: {
    outDir: "dist/desktop-startup",
    emptyOutDir: true,
    rollupOptions: { input: path.resolve(__dirname, "desktop-startup.html") },
  },
});
