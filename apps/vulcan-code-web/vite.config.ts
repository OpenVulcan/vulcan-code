import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";
import os from "node:os";
import fs from "node:fs/promises";

// Configure the Vite app used by the Tauri desktop shell.
// 配置 Tauri 桌面壳使用的 Vite 应用。
export default defineConfig({
  plugins: [react(), tailwindcss(), vulcanServiceDiscoveryPlugin()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
  },
});

// vulcanServiceDiscoveryPlugin exposes the latest local service endpoint to browser-mode dev clients.
// vulcanServiceDiscoveryPlugin 向浏览器开发客户端暴露最新本地服务端点。
function vulcanServiceDiscoveryPlugin() {
  return {
    name: "vulcan-service-discovery",
    configureServer(server: { middlewares: { use: (route: string, handler: (request: unknown, response: import("node:http").ServerResponse) => void) => void } }) {
      server.middlewares.use("/vulcan-service-discovery.json", (_request, response) => {
        void readVulcanServiceDiscoveryFile()
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
async function readVulcanServiceDiscoveryFile(): Promise<string> {
  const file = path.join(os.homedir(), ".vulcan", "code", "webui-service.json");
  return fs.readFile(file, "utf8");
}
