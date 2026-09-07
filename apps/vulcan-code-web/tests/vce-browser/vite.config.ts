import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

// Serve production components with isolated transport aliases, never the user-service discovery plugin.
// 使用隔离传输别名提供生产组件，绝不启用用户服务发现插件。
export default defineConfig({
  root: __dirname,
  plugins: [react(), tailwindcss()],
  resolve: { alias: [
    { find: "@/service/settings-actions-service", replacement: path.resolve(__dirname, "transport.ts") },
    { find: "@/service/vce-service", replacement: path.resolve(__dirname, "transport.ts") },
    { find: "@", replacement: path.resolve(__dirname, "../../src") },
  ] },
  server: { host: "127.0.0.1", port: 17617, strictPort: true },
});
