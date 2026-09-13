import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import type { Plugin } from "vite";
import type { PluginSummary, PluginUiModuleSource } from "../../src/types";

// parserRequire resolves the YAML parser already owned by the installed Vite toolchain.
// parserRequire 解析已安装 Vite 工具链持有的 YAML 解析器。
const parserRequire = createRequire(createRequire(import.meta.url).resolve("vite"));
// parse rejects duplicate YAML keys instead of silently replacing locale messages.
// parse 拒绝重复 YAML 键，不静默覆盖语言消息。
const { parse } = parserRequire("yaml") as { parse: (source: string, options: { uniqueKeys: boolean }) => any };
// pluginRoot is the authoritative source package directory, never output or installed runtime state.
// pluginRoot 是权威源码包目录，绝非 output 或已安装运行时状态。
const pluginRoot = path.resolve(import.meta.dirname, "../../../../plugins");

// readAuditPluginAssets reads two exact source allowlist entries and returns detached host projections.
// readAuditPluginAssets 读取两个准确源码白名单条目，返回独立宿主投影。
// Returns: real manifest metadata, ESM text, locale messages and output schemas; no executable runtime is loaded.
// 返回值：真实清单元数据、ESM 文本、语言消息与输出 Schema；不加载可执行运行时。
export function readAuditPluginAssets() {
  return Object.fromEntries(["context7", "vulcan-usage-statistics"].map((id) => {
    // root and manifest stay inside the two fixed source package paths.
    // root 与 manifest 保持在两个固定源码包路径内。
    const root = path.join(pluginRoot, id);
    const manifest = parse(readFileSync(path.join(root, "plugin.yaml"), "utf8"), { uniqueKeys: true });
    // locales preserves only declared messages and public display metadata.
    // locales 仅保留已声明消息与公开展示元数据。
    const locales = Object.fromEntries(Object.entries(manifest.locales as Record<string, string>).map(([locale, file]) =>
      [locale, parse(readFileSync(path.join(root, file), "utf8"), { uniqueKeys: true })]));
    const mount = manifest.ui.mounts[0];
    const summary: PluginSummary = {
      id, version: manifest.version, name: manifest.display.name, kind: "Vulcan Plugin", status: "enabled", runtimeState: "enabled",
      capability: manifest.display.capability, enabled: true, desiredEnabled: true, available: true,
      nameByLocale: Object.fromEntries(Object.entries(locales).map(([locale, value]) => [locale, value.name])),
      capabilityByLocale: Object.fromEntries(Object.entries(locales).map(([locale, value]) => [locale, value.capability])),
      toolCount: manifest.lua_runtime.tools?.length ?? 0,
      tools: (manifest.lua_runtime.tools ?? []).map((tool: { id: string; group: string; description: string }) => ({ id: tool.id, group: tool.group, description: tool.description })),
      hookCount: 0, skillCount: 0, agentCount: 0, activeSessionCount: 0, diagnostics: ["隔离验收：业务数据为合成记录，写入和联网动作被拒绝。"],
      managementKind: "vulcanPlugin", extension: { origin: "bundled", permissions: manifest.permissions, hooks: [],
        actions: manifest.lua_runtime.actions.map((action: { id: string }) => action.id),
        uiMounts: [{ id: mount.id, point: "skillsPluginDetail", label: mount.label, icon: mount.icon, exportName: mount.export,
          labelByLocale: Object.fromEntries(Object.entries(locales).map(([locale, value]) => [locale, value.messages[mount.label_message_key]])) }],
        skills: [], agents: [], hasLuaRuntime: true, mcpServers: [] },
    };
    const module: PluginUiModuleSource = { pluginId: id, mountId: mount.id, exportName: mount.export,
      source: readFileSync(path.join(root, mount.entry), "utf8"),
      messagesByLocale: Object.fromEntries(Object.entries(locales).map(([locale, value]) => [locale, value.messages])) };
    // schemas admits only the output contracts for fixture-supported read actions.
    // schemas 仅准入夹具支持的读取动作输出契约。
    const schemas = Object.fromEntries(manifest.lua_runtime.actions
      .filter((action: { id: string }) => ["settings-get", "dashboard-get", "pricing-get", "bootstrap-get", "usage-get"].includes(action.id))
      .map((action: { id: string; output_schema_file: string }) => [action.id, JSON.parse(readFileSync(path.join(root, action.output_schema_file), "utf8"))]));
    return [id, { summary, module, schemas }];
  }));
}

// auditPluginAssets injects allowlisted source text into the test build without widening Vite filesystem access.
// auditPluginAssets 将白名单源码文本注入测试构建，不扩大 Vite 文件系统访问范围。
// Returns: a test-only virtual module plugin, shared by browser and unit-test entry points.
// 返回值：浏览器与单元测试入口共用的测试专用虚拟模块插件。
export function auditPluginAssets(): Plugin {
  return { name: "ui-audit-plugin-source-assets", resolveId(id) { return id === "virtual:ui-audit-plugin-assets" ? "\0ui-audit-plugin-assets" : null; },
    load(id) { return id === "\0ui-audit-plugin-assets" ? `export const pluginAssets = ${JSON.stringify(readAuditPluginAssets())};` : null; } };
}
