import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";
// Read the authoritative registry without depending on the WebUI bundler's aliases.
// 直接读取权威注册表，不依赖 WebUI 打包器的别名。
const SUPPORTED_LOCALES = JSON.parse(await readFile(new URL("../../../crates/vulcan-i18n/catalogs/locales.json", import.meta.url), "utf8")).locales;

// Dependencies come from the existing host; no runtime plugin bundle dependency is added.
// 依赖来自现有宿主，不添加运行时插件打包依赖。
const require = createRequire(new URL("../../../apps/vulcan-code-web/package.json", import.meta.url));
// loadYaml parses the shipped locale and manifest files with the existing host dependency.
// loadYaml 使用宿主现有依赖解析实际发布的语言与清单文件。
const { load: loadYaml } = require("js-yaml");
// manifest is the exact plugin declaration whose locale assets must cover the host registry.
// manifest 是语言资源必须覆盖宿主语言注册表的实际插件清单。
const manifest = loadYaml(await readFile(new URL("../plugin.yaml", import.meta.url), "utf8"));
// catalogs contains every host language's actual package translations, indexed by locale code.
// catalogs 按语言代码保存每种宿主语言的实际包内翻译。
const catalogs = new Map(await Promise.all(SUPPORTED_LOCALES.map(async ({ code }) => [code,
  loadYaml(await readFile(new URL(`../${manifest.locales[code]}`, import.meta.url), "utf8"))])));
// defaults and layout are the shipped configuration defaults and declarative settings layout.
// defaults 与 layout 是随包发布的配置默认值与声明式设置版面。
const defaults = JSON.parse(await readFile(new URL("../config/defaults.json", import.meta.url), "utf8"));
const layout = JSON.parse(await readFile(new URL("./settings.json", import.meta.url), "utf8"));

// layoutMessageKeys collects every `*_message_key` value of the layout, however deeply it is nested.
// layoutMessageKeys 收集版面中任意层级的全部 `*_message_key` 值。
function layoutMessageKeys(value, keys = new Set()) {
  if (Array.isArray(value)) value.forEach((item) => layoutMessageKeys(item, keys));
  else if (value && typeof value === "object") {
    for (const [name, child] of Object.entries(value)) {
      if (name.endsWith("_message_key") && typeof child === "string") keys.add(child);
      else layoutMessageKeys(child, keys);
    }
  }
  return keys;
}

// The declarative layout offers every route once, with the shipped defaults as the curated group.
// 声明式版面恰好提供每条路由一次，并以随包默认值作为精选分组。
test("settings layout route groups are disjoint and the curated group equals the default routes", () => {
  const fields = layout.sections.flatMap((section) => section.items ?? []).filter((item) => item.control === "checklist");
  assert.deepEqual(fields.map((field) => [field.key, field.selection, field.visible_when.values]),
    [["enabledRoutes", "include", ["selected"]], ["disabledRoutes", "exclude", ["all"]]]);
  for (const field of fields) {
    const values = field.option_groups.flatMap((group) => group.values);
    assert.equal(new Set(values).size, values.length, `${field.key} offers a route twice`);
    assert.deepEqual(field.option_groups.find((group) => group.id === "curated").values, defaults.enabledRoutes);
  }
  assert.deepEqual(fields[0].option_groups, fields[1].option_groups);
});

// The page keeps the original three tabs: switches, routes and statistics read from one cached action.
// 页面保留原有三个标签：开关、路由，以及读取单个缓存 Action 的统计。
test("settings layout keeps the settings, routes and statistics tabs", () => {
  assert.deepEqual(layout.tabs.map((tab) => [tab.id, tab.sections]), [["settings", ["general"]], ["routes", ["routes"]], ["statistics", ["gain"]]]);
  const gain = layout.sections.find((section) => section.id === "gain");
  assert.equal(gain.data.action, "statistics-get");
  assert.deepEqual(gain.blocks.map((block) => block.type), ["metrics", "chart", "table"]);
  assert.deepEqual(gain.blocks[1].periods.map((period) => period.id), ["daily", "weekly", "monthly"]);
  assert.ok(manifest.required_files.includes(layout.header.documents[0].asset), "the license document must be packaged");
});

// Shipped translations cover host languages, every layout key, and retain exact interpolation contracts.
// 发布翻译覆盖宿主语言与全部版面键，并保留精确的插值契约。
test("all host locales have complete translated catalogs and required assets", () => {
  // baseline is the English source catalog, not a second manually maintained key registry.
  // baseline 为英文源翻译，不另行维护第二份键注册表。
  const baseline = catalogs.get("en-US");
  assert.equal(manifest.locales.en, manifest.locales["en-US"]);
  for (const key of layoutMessageKeys(layout)) assert.ok(baseline.messages[key], `layout key ${key} must exist in English`);
  for (const { code } of SUPPORTED_LOCALES) {
    const catalog = catalogs.get(code);
    assert.ok(manifest.required_files.includes(manifest.locales[code]), `${code} locale must be packaged`);
    assert.equal(catalog.name, "Vulcan RTK");
    assert.equal(typeof catalog.capability, "string");
    assert.deepEqual(Object.keys(catalog.messages).sort(), Object.keys(baseline.messages).sort(), code);
    // key and value identify one real translation whose placeholders must match its source contract.
    // key 与 value 标识一条实际翻译，其占位符必须符合源契约。
    for (const [key, value] of Object.entries(catalog.messages)) {
      assert.equal(typeof value, "string", `${code}:${key}`);
      assert.ok(value.trim(), `${code}:${key} must be translated`);
      assert.deepEqual(value.match(/\{\{[^}]+\}\}/g) ?? [], baseline.messages[key].match(/\{\{[^}]+\}\}/g) ?? [], `${code}:${key} placeholders`);
    }
    if (code !== "en-US") {
      assert.notEqual(catalog.capability, baseline.capability, `${code} capability must be translated`);
      assert.notEqual(catalog.messages["stats.title"], baseline.messages["stats.title"], `${code} statistics must be translated`);
    }
  }
});
