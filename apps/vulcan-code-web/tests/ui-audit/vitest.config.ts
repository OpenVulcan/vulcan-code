import { defineConfig } from "vitest/config";
import { auditAliases } from "./vite.config";
import { auditPluginAssets } from "./plugin-assets.server";

// Smoke tests use the exact browser aliases while never starting a browser or a service listener.
// 冒烟测试使用准确浏览器别名，不启动浏览器或服务监听。
export default defineConfig({ plugins: [auditPluginAssets()], resolve: { alias: auditAliases }, test: { environment: "jsdom", include: ["tests/ui-audit/*.test.tsx", "tests/ui-audit/*.test.ts"] } });
