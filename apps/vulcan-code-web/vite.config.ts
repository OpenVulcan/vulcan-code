import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";
import os from "node:os";
import fs from "node:fs/promises";
import { configDefaults } from "vitest/config";

// VULCAN_DEV_WEBUI_ORIGIN is the exact debug workbench allowed to read its service discovery token.
// VULCAN_DEV_WEBUI_ORIGIN 是获准读取服务发现令牌的精确调试工作台来源。
const VULCAN_DEV_WEBUI_ORIGIN = "http://127.0.0.1:17601";

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
    // Parallel audit servers share this package's default cache; keep their optimizer writes out of the live workbench.
    // 并行验收服务共用本包默认缓存；隔离其依赖预构建写入，防止污染运行中的工作台。
    // Unit tests also own a separate cache so their module graph cannot replace the browser's React singleton.
    // 单元测试也拥有独立缓存，避免其模块图替换浏览器中的 React 单例。
    cacheDir: path.resolve(__dirname, "node_modules", mode === "test" ? ".vite-workbench-test" : ".vite-workbench"),
    plugins: [react(), tailwindcss(), vulcanServiceDiscoveryPlugin(userVulcanRootName)],
    resolve: {
      alias: {
        "@": path.resolve(__dirname, "./src"),
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
      rollupOptions: {
        output: {
          // Each locale catalog gets its own chunk and cache identity. Only the English fallback is a
          // static import counted by the initial-workbench budget; every other language loads on demand.
          // 每种语言目录拥有独立分块与缓存身份。只有英文回退目录是静态导入并计入首次工作台预算；
          // 其他语言按需加载。
          manualChunks(id) {
            const localeCatalog = /i18n-messages\.([a-z]{2}-[A-Z]{2})\.ts$/u.exec(id);
            return localeCatalog ? `i18n-messages.${localeCatalog[1]}` : undefined;
          },
        },
      },
    },
    test: {
      // The full-workbench audit requires its isolated transport aliases and dedicated Vitest configuration.
      // 完整工作台验收依赖隔离传输别名及其专属 Vitest 配置。
      exclude: [...configDefaults.exclude, "scripts/*.test.mjs", "tests/ui-audit/**"],
      // Every locale catalog is preloaded so tests can read any language synchronously.
      // 预加载全部语言目录，使测试可以同步读取任意语言。
      setupFiles: ["src/test-setup/preload-locale-catalogs.ts"],
    },
    server: isRelease
      ? undefined
      : {
          host: "127.0.0.1",
          port: 17601,
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
