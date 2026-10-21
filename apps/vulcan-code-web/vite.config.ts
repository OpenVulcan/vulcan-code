import { defineConfig, searchForWorkspaceRoot } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import os from "node:os";
import fs from "node:fs/promises";
import { configDefaults } from "vitest/config";
import { VULCAN_DEV_WEBUI_ORIGIN } from "./src/service/vite-service-discovery-origin";

// catalogsDir holds the language registry and catalogs shared with the TUI, the desktop shell and Core.
// catalogsDir 保存与 TUI、桌面壳及 Core 共用的语言注册表与目录。
const catalogsDir = path.resolve(__dirname, "../../crates/vulcan-i18n/catalogs");

// Configure the Vite app served by Core and loaded by the desktop shell.
// 配置由 Core 提供页面并供桌面壳加载的 Vite 应用。
export default defineConfig(({ mode }) => {
  // isRelease selects the packaged release behavior where no Vite server is needed.
  // isRelease 选择无需 Vite 服务端的打包 release 行为。
  const isRelease = mode === "production";
  // userVulcanRootName keeps development discovery isolated from release user data.
  // userVulcanRootName 让开发发现文件与 release 用户数据相互隔离。
  const userVulcanRootName = isRelease ? ".vulcan" : ".vulcan-debug";

  return {
    // The live-backend acceptance server uses this package's default cache; keep its optimizer writes out of the live workbench.
    // 真实后端验收服务使用本包默认缓存；隔离其依赖预构建写入，防止污染运行中的工作台。
    // Unit tests also own a separate cache so their module graph cannot replace the browser's React singleton.
    // 单元测试也拥有独立缓存，避免其模块图替换浏览器中的 React 单例。
    cacheDir: path.resolve(__dirname, "node_modules", mode === "test" ? ".vite-workbench-test" : ".vite-workbench"),
    plugins: [react(), vulcanServiceDiscoveryPlugin(userVulcanRootName)],
    resolve: {
      alias: {
        "@": path.resolve(__dirname, "./src"),
        "@catalogs": catalogsDir,
      },
    },
    clearScreen: false,
    worker: {
      format: "es",
    },
    build: {
      // manifest exposes the actual static import graph for the first-workbench budget.
      // manifest 暴露真实静态导入图，供首屏工作台资源预算核算。
      manifest: true,
      // Provider artwork stays as separate files: inlined, the small icons added about 45 KB of base64 to the
      // workbench chunk, while each settings or model-picker row needs only the few it shows.
      // 供应商图标保持为独立文件：若内联，这些小图标会向工作台分块加入约 45 KB 的 base64，而设置或模型选择行只需显示其中少数几个。
      assetsInlineLimit: (file) => (file.replaceAll("\\", "/").includes("/assets/provider-icons/") ? false : undefined),
      rollupOptions: {
        output: {
          // Each locale catalog gets its own chunk and cache identity. Only the English fallback is a
          // static import counted by the initial-workbench budget; every other language loads on demand.
          // 每种语言目录拥有独立分块与缓存身份。只有英文回退目录是静态导入并计入首次工作台预算；
          // 其他语言按需加载。
          manualChunks(id) {
            const localeCatalog = /\/catalogs\/webui\/([a-z]{2}-[A-Z]{2})\.json$/u.exec(id);
            return localeCatalog ? `i18n-messages.${localeCatalog[1]}` : undefined;
          },
        },
      },
    },
    test: {
      // Node-native script tests run through node:test, not Vitest.
      // Node 原生脚本测试通过 node:test 运行，不经 Vitest。
      exclude: [...configDefaults.exclude, "scripts/*.test.mjs"],
      // Common bilingual fixtures are preloaded; locale-specific suites await their own additional catalogs.
      // 预加载通用中英夹具；特定语言测试等待加载自身所需的其他目录。
      setupFiles: ["src/test-setup/preload-locale-catalogs.ts"],
    },
    server: isRelease
      ? undefined
      : {
          // The shared language catalogs live in the vulcan-i18n crate, outside this package.
          // 共享语言目录位于本包之外的 vulcan-i18n crate。
          fs: { allow: [searchForWorkspaceRoot(process.cwd()), catalogsDir] },
          host: new URL(VULCAN_DEV_WEBUI_ORIGIN).hostname,
          port: Number(new URL(VULCAN_DEV_WEBUI_ORIGIN).port),
          strictPort: true,
        },
  };
});

// vulcanServiceDiscoveryPlugin exposes the latest local service endpoint to browser-mode dev clients.
// vulcanServiceDiscoveryPlugin 向浏览器开发客户端暴露最新本地服务端点。
function vulcanServiceDiscoveryPlugin(userVulcanRootName: string) {
  return {
    name: "vulcan-service-discovery",
    configureServer(server: { middlewares: { use: (route: string, handler: (request: unknown, response: import("node:http").ServerResponse) => void) => void } }) {
      server.middlewares.use("/vulcan-service-discovery.json", (request: import("node:http").IncomingMessage, response) => {
        // A different localhost website must not receive the Core token through Vite's dev CORS.
        // 其他 localhost 网站不得借助 Vite 开发服务器的 CORS 取得 Core 令牌。
        const origin = request.headers.origin;
        if (origin !== undefined && origin !== VULCAN_DEV_WEBUI_ORIGIN) {
          response.statusCode = 403;
          response.setHeader("cache-control", "no-store");
          response.end();
          return;
        }
        void readVulcanServiceDiscoveryFile(userVulcanRootName)
          .then((body) => {
            response.setHeader("content-type", "application/json; charset=utf-8");
            response.setHeader("cache-control", "no-store");
            response.end(body);
          })
          .catch(() => {
            response.statusCode = 404;
            response.setHeader("cache-control", "no-store");
            response.end(JSON.stringify({ status: "missing" }));
          });
      });
    },
  };
}

// readVulcanServiceDiscoveryFile reads the service discovery snapshot written by the desktop service.
// readVulcanServiceDiscoveryFile 读取桌面服务写入的服务发现快照。
async function readVulcanServiceDiscoveryFile(userVulcanRootName: string): Promise<string> {
  const file = path.join(os.homedir(), userVulcanRootName, "code", "webui-service.json");
  return fs.readFile(file, "utf8");
}
