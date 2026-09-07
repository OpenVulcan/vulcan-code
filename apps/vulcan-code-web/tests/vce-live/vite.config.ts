import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// root is an explicitly supplied private backend profile, never the normal discovery directory.
// root 是明确指定的私有后端配置，绝不是正常发现目录。
const root = process.env.VULCAN_VCE_BROWSER_TEST_ROOT;
if (!root || !path.isAbsolute(root)) {
  throw new Error("VULCAN_VCE_BROWSER_TEST_ROOT must be an absolute private test directory.");
}
// discoveryPath is the single receipt produced by the isolated production-protocol test host.
// discoveryPath 是隔离生产协议测试宿主生成的唯一回执。
const discoveryPath = path.join(fs.realpathSync(root), "browser-discovery.json");
// discovery uses only the exact persisted host contract; missing or malformed receipts fail startup.
// discovery 只使用精确持久宿主契约；缺失或畸形回执使启动失败。
const discovery: { baseUrl: string; serviceToken: string; status: string } = JSON.parse(
  fs.readFileSync(discoveryPath, "utf8"),
);
// endpoint is restricted to the private loopback host before it is admitted into browser policy.
// endpoint 在加入浏览器策略前被限制为私有回环宿主。
const endpoint = new URL(discovery.baseUrl);
if (endpoint.protocol !== "http:" || endpoint.hostname !== "127.0.0.1"
    || !endpoint.port || endpoint.pathname !== "/" || endpoint.search || endpoint.hash
    || endpoint.username || endpoint.password || !discovery.serviceToken || discovery.status !== "ready") {
  throw new Error("The private browser host receipt is not ready or has an invalid endpoint.");
}
// appRoot renders the unmodified production application, not a component harness.
// appRoot 渲染未修改的生产应用，而不是组件夹具。
const appRoot = fileURLToPath(new URL("../../", import.meta.url));

/**
 * Publish only the private discovery receipt through Vite; never read a user-profile discovery file.
 * 仅通过 Vite 发布私有发现回执；绝不读取用户配置的发现文件。
 * No arguments; returns middleware pinned to the identity admitted when this Vite process started.
 * 无参数；返回固定到此 Vite 进程启动时准入身份的中间件。
 */
function privateDiscoveryPlugin(): Plugin {
  return {
    name: "vce-private-live-discovery",
    // Register the exact discovery route on this server; no other request is handled here.
    // 在此服务器上登记精确发现路由；这里不处理其他请求。
    configureServer(server) {
      // Return the frozen private receipt to this request and do not consult user configuration.
      // 向此请求返回固定私有回执，不查询用户配置。
      server.middlewares.use("/vulcan-service-discovery.json", (_request, response) => {
        response.setHeader("content-type", "application/json; charset=utf-8");
        response.setHeader("cache-control", "no-store");
        response.end(JSON.stringify(discovery));
      });
    },
  };
}

// CSP blocks every other HTTP/WebSocket service even if production discovery tries its default range.
// 内容安全策略阻断所有其他 HTTP／WebSocket 服务，即使生产发现尝试默认端口范围。
export default defineConfig({
  root: appRoot,
  plugins: [react(), tailwindcss(), privateDiscoveryPlugin()],
  resolve: { alias: { "@": path.join(appRoot, "src") } },
  worker: { format: "es" },
  server: {
    host: "127.0.0.1",
    port: 17619,
    strictPort: true,
    headers: {
      "Content-Security-Policy": `connect-src 'self' ${endpoint.origin} ${endpoint.origin.replace("http:", "ws:")} ws://127.0.0.1:17619;`,
    },
  },
});
