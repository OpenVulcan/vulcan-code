import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";
import { SUPPORTED_LOCALES } from "../../../apps/vulcan-code-web/src/i18n-locales.ts";

// require uses existing host dependencies without adding a plugin runtime library.
// require 使用既有宿主依赖，不向插件运行时增加库。
const require = createRequire(new URL("../../../apps/vulcan-code-web/package.json", import.meta.url));
const { load: loadYaml } = require("js-yaml");
// manifest, catalogs, layout and usageInput are the exact shipped package files.
// manifest、catalogs、layout 与 usageInput 是实际随包发布的文件。
const manifest = loadYaml(await readFile(new URL("../plugin.yaml", import.meta.url), "utf8"));
const catalogs = new Map(await Promise.all(SUPPORTED_LOCALES.map(async ({ code }) => [code,
  loadYaml(await readFile(new URL(`../${manifest.locales[code]}`, import.meta.url), "utf8"))])));
const layout = JSON.parse(await readFile(new URL("./settings.json", import.meta.url), "utf8"));
const usageInput = JSON.parse(await readFile(new URL("../schemas/usage-input.json", import.meta.url), "utf8"));

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

// The usage section offers exactly the ranges the action accepts and re-reads on the plugin's data topic.
// 用量区块恰好提供 Action 接受的范围，并在插件数据主题上重新读取。
test("usage section offers every accepted range and follows data invalidation", () => {
  const usage = layout.sections.find((section) => section.id === "usage");
  assert.deepEqual(usage.data.arguments[0].options.map((option) => option.value), usageInput.properties.rangeDays.enum);
  assert.deepEqual(usage.data.refresh_topics, ["context7.data"]);
  assert.deepEqual(manifest.ui.mounts.map((mount) => mount.kind), ["settings"]);
});

// Shipped translations cover host languages, every layout key, and retain exact interpolation contracts.
// 发布翻译覆盖宿主语言与全部版面键，并保留精确的插值契约。
test("all host locales have complete translated catalogs", () => {
  const baseline = catalogs.get("en-US");
  for (const key of layoutMessageKeys(layout)) assert.ok(baseline.messages[key], `layout key ${key} must exist in English`);
  for (const { code } of SUPPORTED_LOCALES) {
    const catalog = catalogs.get(code);
    assert.deepEqual(Object.keys(catalog.messages).sort(), Object.keys(baseline.messages).sort(), code);
    for (const [key, value] of Object.entries(catalog.messages)) {
      assert.ok(typeof value === "string" && value.trim(), `${code}:${key} must be translated`);
      assert.deepEqual(value.match(/\{\{[^}]+\}\}/g) ?? [], baseline.messages[key].match(/\{\{[^}]+\}\}/g) ?? [], `${code}:${key} placeholders`);
    }
    if (code !== "en-US") assert.notEqual(catalog.messages.usageTitle, baseline.messages.usageTitle, `${code} usage must be translated`);
  }
});
