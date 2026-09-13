import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it, vi } from "vitest";
import { pluginAssets } from "virtual:ui-audit-plugin-assets";
import { auditPluginCatalog, auditPluginModule, auditPluginRead, type AuditPluginScenario } from "./plugin-fixtures";
import { serviceClient } from "@/service/webui-service-runtime";

// validationRequire reuses the existing toolchain validator without adding a production dependency.
// validationRequire 复用既有工具链校验器，不增加生产依赖。
const validationRequire = createRequire(createRequire(import.meta.url).resolve("vite"));
const Ajv2020 = validationRequire("ajv/dist/2020").default;
const addFormats = validationRequire("ajv-formats").default;
// validator enforces all declared output fields, formats and additional-property boundaries.
// validator 强制校验全部声明输出字段、格式与额外属性边界。
const validator = new Ajv2020({ strict: true, allErrors: true });
addFormats(validator);

describe("real plugin source in isolated visual fixtures", () => {
  it("permits the production Blob ESM loader while keeping network destinations restricted to the isolated origin", () => {
    // The browser enforces this entry policy before dynamic plugin imports, unlike jsdom transport tests.
    // 浏览器会在动态插件导入前执行此入口策略，而 jsdom 传输测试不会。
    const html = readFileSync(path.resolve(import.meta.dirname, "app.html"), "utf8");
    const policy = html.match(/http-equiv="Content-Security-Policy" content="([^"]+)"/)![1];
    const directives = new Map(policy.split(";").map((entry) => {
      const [name, ...values] = entry.trim().split(/\s+/); return [name, values];
    }));
    expect(directives.get("script-src")).toContain("blob:");
    expect(directives.get("connect-src")).toEqual(["'self'"]);
    expect(directives.get("default-src")).toEqual(["'self'"]);
  });
  it("loads the current authoritative ESM and strict duplicate-free locale maps", () => {
    expect(auditPluginCatalog()).toHaveLength(2);
    for (const asset of Object.values(pluginAssets)) {
      const module = auditPluginModule(asset.summary.id, asset.module.mountId);
      expect(module.source).toBe(readFileSync(path.resolve(import.meta.dirname, "../../../../plugins", asset.summary.id, "ui/index.mjs"), "utf8"));
      expect(module.messagesByLocale["zh-CN"].title).toBeTruthy();
      expect(module.messagesByLocale.en.title).toBeTruthy();
      expect(asset.summary.extension?.uiMounts[0].point).toBe("skillsPluginDetail");
      expect(asset.summary.runtimeState).toBe("enabled");
    }
    expect(pluginAssets.context7.module.messagesByLocale.en.loading).toBe("Loading Context7 settings…");
    expect(() => auditPluginModule("context7", "wrong-mount")).toThrow("UI_AUDIT_UNIMPLEMENTED_PLUGIN_MOUNT");
  });

  it.each<AuditPluginScenario>(["empty", "long-list", "invalid-default"])("validates every admitted %s output against its original JSON Schema", (scenario) => {
    for (const [id, asset] of Object.entries(pluginAssets)) {
      for (const [action, schema] of Object.entries(asset.schemas)) {
        // Context7 settings-get is declared but intentionally not admitted: the real panel uses its one bootstrap read.
        // Context7 声明了 settings-get 但此处有意不准入：真实面板使用单次 bootstrap 读取。
        if (id === "context7" && action === "settings-get") continue;
        if (id === "context7" && action === "usage-get" && scenario === "invalid-default") {
          expect(() => auditPluginRead(id, action, {}, scenario)).toThrow("UI_AUDIT_SYNTHETIC_USAGE_FAILURE"); continue;
        }
        const validate = validator.compile(schema);
        const output = auditPluginRead(id, action, {}, scenario);
        expect(validate(output), `${id}/${action}: ${JSON.stringify(validate.errors)}`).toBe(true);
      }
    }
  });

  it("filters complete pricing data before pagination and keeps unknown prices distinct from real zeros", () => {
    const first = auditPluginRead("vulcan-usage-statistics", "pricing-get", { filter: "all", offset: 0 }, "long-list") as any;
    const next = auditPluginRead("vulcan-usage-statistics", "pricing-get", { filter: "all", offset: 25 }, "long-list") as any;
    expect(first.totalCount).toBe(42); expect(first.models).toHaveLength(25); expect(next.models).toHaveLength(17);
    expect(new Set([...first.models, ...next.models].map((row) => row.modelId)).size).toBe(42);
    const used = auditPluginRead("vulcan-usage-statistics", "pricing-get", { filter: "used", offset: 25 }, "long-list") as any;
    expect(used.totalCount).toBe(36); expect(used.models).toHaveLength(11);
    const found = auditPluginRead("vulcan-usage-statistics", "pricing-get", { search: "model-42" }, "long-list") as any;
    expect(found.totalCount).toBe(1); expect(found.models[0].attempts).toBe(0);
    expect(first.models.some((row: any) => row.prices === null)).toBe(true);
    expect(first.models.some((row: any) => row.prices?.inputPerMillion === "0")).toBe(true);
  });

  it("keeps overview, rows, tokens and exact costs in the same filtered range", () => {
    const result = auditPluginRead("vulcan-usage-statistics", "dashboard-get", {
      view: "details", rangeDays: 7, consumerId: "conversation", modelQuery: "model-0", sortOrder: "oldest", dailyOffset: 0, recentOffset: 0,
    }, "long-list") as any;
    expect(result.daily.length).toBeGreaterThan(0);
    expect(result.dailyCount).toBe(result.daily.length);
    expect(result.summary.attempts).toBe(result.daily.length);
    expect(result.summary.totalTokens).toBe(result.daily.length * 1500);
    expect(result.daily.every((row: any) => row.consumerId === "conversation" && row.modelId.includes("model-0"))).toBe(true);
    expect(result.summary.costs[0].amount).toBe("0.007696875");
    expect(validator.compile(pluginAssets["vulcan-usage-statistics"].schemas["dashboard-get"])(result)).toBe(true);
  });

  it("serves module/read RPCs while rejecting every manifest write and network action without fetch", async () => {
    const fetchSpy = vi.fn(() => { throw new Error("network forbidden"); });
    vi.stubGlobal("fetch", fetchSpy);
    try {
      for (const asset of Object.values(pluginAssets)) {
        const id = asset.summary.id;
        await expect(serviceClient.request("service_load_plugin_ui_module", { pluginId: id, mountId: asset.module.mountId })).resolves.toMatchObject({ pluginId: id });
        for (const action of asset.summary.extension!.actions) {
          const admitted = id === "context7" ? ["bootstrap-get", "usage-get"] : ["settings-get", "dashboard-get", "pricing-get"];
          const request = serviceClient.request("service_invoke_plugin_action", { pluginId: id, actionId: action, arguments: {}, workspacePath: null });
          if (admitted.includes(action)) await expect(request).resolves.toBeDefined();
          else await expect(request).rejects.toThrow("UI_AUDIT_UNIMPLEMENTED_PLUGIN_ACTION");
        }
      }
      expect(fetchSpy).not.toHaveBeenCalled();
    } finally { vi.unstubAllGlobals(); }
  });
});
