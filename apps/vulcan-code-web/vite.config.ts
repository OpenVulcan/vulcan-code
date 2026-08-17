import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";
import os from "node:os";
import fs from "node:fs/promises";
import { configDefaults } from "vitest/config";

// Configure the Vite app used by the Tauri desktop shell.
// 配置 Tauri 桌面壳使用的 Vite 应用。
export default defineConfig(({ mode }) => {
  // isRelease selects the packaged release behavior where no Vite server is needed.
  // isRelease 选择无需 Vite 服务端的打包 release 行为。
  const isRelease = mode === "production";
  // userVulcanRootName keeps development discovery isolated from release user data.
  // userVulcanRootName 让开发发现文件与 release 用户数据相互隔离。
  const userVulcanRootName = isRelease ? ".vulcan" : ".vulcan-debug";

  return {
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
    test: {
      exclude: [...configDefaults.exclude, "scripts/*.test.mjs"],
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
      server.middlewares.use("/vulcan-service-discovery.json", (_request, response) => {
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
